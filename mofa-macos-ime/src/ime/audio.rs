struct RecordingTicker {
    stop: Arc<AtomicBool>,
    join: Option<std::thread::JoinHandle<()>>,
}

impl RecordingTicker {
    fn start(samples: Arc<Mutex<Vec<f32>>>, sample_rate: u32, overlay: OverlayHandle) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let stop_flag = Arc::clone(&stop);

        let join = std::thread::spawn(move || {
            while !stop_flag.load(Ordering::SeqCst) {
                let len = samples.lock().map(|buf| buf.len()).unwrap_or(0);
                let secs = len as f32 / sample_rate.max(1) as f32;
                overlay.set_status("录音中");
                overlay.set_preview(&format!("正在听写 {:.1}s", secs));
                std::thread::sleep(Duration::from_millis(180));
            }
        });

        Self {
            stop,
            join: Some(join),
        }
    }

    fn stop(mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

struct ActiveRecorder {
    stream: cpal::Stream,
    samples: Arc<Mutex<Vec<f32>>>,
    sample_rate: u32,
}

impl ActiveRecorder {
    fn start() -> Result<Self> {
        let host = cpal::default_host();
        let device = host
            .default_input_device()
            .ok_or_else(|| anyhow!("未找到麦克风设备"))?;

        let cfg = device.default_input_config()?;
        let sample_rate = cfg.sample_rate().0;
        let channels = cfg.channels() as usize;
        let samples = Arc::new(Mutex::new(Vec::<f32>::new()));

        let stream = match cfg.sample_format() {
            cpal::SampleFormat::F32 => {
                let samples_buf = Arc::clone(&samples);
                device.build_input_stream(
                    &cfg.clone().into(),
                    move |data: &[f32], _| append_mono_f32(&samples_buf, data, channels),
                    move |err| eprintln!("[mofa-ime] 音频流错误: {err}"),
                    None,
                )?
            }
            cpal::SampleFormat::I16 => {
                let samples_buf = Arc::clone(&samples);
                device.build_input_stream(
                    &cfg.clone().into(),
                    move |data: &[i16], _| append_mono_i16(&samples_buf, data, channels),
                    move |err| eprintln!("[mofa-ime] 音频流错误: {err}"),
                    None,
                )?
            }
            cpal::SampleFormat::U16 => {
                let samples_buf = Arc::clone(&samples);
                device.build_input_stream(
                    &cfg.clone().into(),
                    move |data: &[u16], _| append_mono_u16(&samples_buf, data, channels),
                    move |err| eprintln!("[mofa-ime] 音频流错误: {err}"),
                    None,
                )?
            }
            other => bail!("不支持的采样格式: {other:?}"),
        };

        stream.play()?;

        Ok(Self {
            stream,
            samples,
            sample_rate,
        })
    }

    fn sample_buffer(&self) -> Arc<Mutex<Vec<f32>>> {
        Arc::clone(&self.samples)
    }

    fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    fn stop(self) -> Result<Vec<f32>> {
        // drop stream first to stop capture
        drop(self.stream);

        // Give CoreAudio a short breath to flush callbacks.
        std::thread::sleep(Duration::from_millis(40));

        let raw = self
            .samples
            .lock()
            .map_err(|_| anyhow!("音频缓存锁失败"))?
            .clone();

        if raw.is_empty() {
            bail!("录音为空");
        }

        Ok(resample_to_16k(&raw, self.sample_rate))
    }
}

fn append_mono_f32(buf: &Arc<Mutex<Vec<f32>>>, data: &[f32], channels: usize) {
    if channels == 0 {
        return;
    }
    if let Ok(mut dst) = buf.lock() {
        if channels == 1 {
            dst.extend_from_slice(data);
            return;
        }
        for frame in data.chunks(channels) {
            let sum: f32 = frame.iter().copied().sum();
            dst.push(sum / channels as f32);
        }
    }
}

fn append_mono_i16(buf: &Arc<Mutex<Vec<f32>>>, data: &[i16], channels: usize) {
    if channels == 0 {
        return;
    }
    if let Ok(mut dst) = buf.lock() {
        for frame in data.chunks(channels) {
            let mut sum = 0.0f32;
            for s in frame {
                sum += *s as f32 / i16::MAX as f32;
            }
            dst.push(sum / frame.len() as f32);
        }
    }
}

fn append_mono_u16(buf: &Arc<Mutex<Vec<f32>>>, data: &[u16], channels: usize) {
    if channels == 0 {
        return;
    }
    if let Ok(mut dst) = buf.lock() {
        for frame in data.chunks(channels) {
            let mut sum = 0.0f32;
            for s in frame {
                sum += (*s as f32 / u16::MAX as f32) * 2.0 - 1.0;
            }
            dst.push(sum / frame.len() as f32);
        }
    }
}

fn resample_to_16k(samples: &[f32], from_rate: u32) -> Vec<f32> {
    const TARGET: u32 = 16_000;
    if from_rate == TARGET || samples.is_empty() {
        return samples.to_vec();
    }

    let ratio = TARGET as f64 / from_rate as f64;
    let new_len = (samples.len() as f64 * ratio) as usize;
    let mut out = Vec::with_capacity(new_len);

    for i in 0..new_len {
        let src_pos = i as f64 / ratio;
        let i0 = src_pos.floor() as usize;
        let i1 = (i0 + 1).min(samples.len() - 1);
        let frac = src_pos - i0 as f64;

        let y0 = samples[i0] as f64;
        let y1 = samples[i1] as f64;
        out.push((y0 + (y1 - y0) * frac) as f32);
    }

    out
}
