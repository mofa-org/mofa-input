//! Audio recording and processing utilities

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::{Arc, Mutex};

pub struct AudioRecorder {
    samples: Arc<Mutex<Vec<f32>>>,
    stream: Option<Box<dyn StreamTrait>>,
    is_recording: Arc<Mutex<bool>>,
    sample_rate: u32,
}

impl AudioRecorder {
    pub fn new() -> Self {
        Self {
            samples: Arc::new(Mutex::new(Vec::new())),
            stream: None,
            is_recording: Arc::new(Mutex::new(false)),
            sample_rate: 16000,
        }
    }

    /// Start recording audio
    pub fn start_recording(&mut self) -> anyhow::Result<()> {
        let host = cpal::default_host();
        let device = host
            .default_input_device()
            .ok_or_else(|| anyhow::anyhow!("No input device available"))?;

        let config = device.default_input_config()?;
        let sample_rate = config.sample_rate().0;
        let channels = config.channels();

        let samples = self.samples.clone();
        let is_recording = self.is_recording.clone();

        *is_recording.lock().unwrap() = true;
        samples.lock().unwrap().clear();

        let err_fn = |err| eprintln!("Audio stream error: {}", err);

        let stream = if config.sample_format() == cpal::SampleFormat::F32 {
            device.build_input_stream(
                &config.into(),
                move |data: &[f32], _| {
                    if *is_recording.lock().unwrap() {
                        let mut samples = samples.lock().unwrap();
                        // Convert to mono if stereo
                        if channels == 2 {
                            for chunk in data.chunks(2) {
                                let mono = (chunk[0] + chunk[1]) / 2.0;
                                samples.push(mono);
                            }
                        } else {
                            samples.extend_from_slice(data);
                        }
                    }
                },
                err_fn,
                None,
            )?
        } else {
            return Err(anyhow::anyhow!("Unsupported sample format"));
        };

        stream.play()?;
        self.stream = Some(Box::new(stream));
        self.sample_rate = sample_rate;

        Ok(())
    }

    /// Stop recording and return audio samples (16kHz, mono, f32)
    pub fn stop_recording(&mut self) -> anyhow::Result<Vec<f32>> {
        *self.is_recording.lock().unwrap() = false;
        self.stream = None;

        let samples = self.samples.lock().unwrap().clone();
        let samples = resample_to_16khz(&samples, self.sample_rate);
        Ok(samples)
    }

    /// Check if currently recording
    pub fn is_recording(&self) -> bool {
        *self.is_recording.lock().unwrap()
    }
}

/// Simple resampling from one sample rate to 16kHz
pub fn resample_to_16khz(samples: &[f32], from_rate: u32) -> Vec<f32> {
    if from_rate == 16000 {
        return samples.to_vec();
    }

    let ratio = 16000.0 / from_rate as f64;
    let new_len = (samples.len() as f64 * ratio) as usize;
    let mut result = Vec::with_capacity(new_len);

    for i in 0..new_len {
        let src_idx = i as f64 / ratio;
        let idx0 = src_idx.floor() as usize;
        let idx1 = (idx0 + 1).min(samples.len() - 1);
        let frac = src_idx - idx0 as f64;

        let val0 = samples[idx0] as f64;
        let val1 = samples[idx1] as f64;
        result.push((val0 + (val1 - val0) * frac) as f32);
    }

    result
}
