fn refresh_models(
    model_base: &Path,
    cfg: AppConfig,
    asr: &mut Option<mofa_input::asr::AsrSession>,
    asr_loaded_path: &mut Option<PathBuf>,
    llm: &mut Option<mofa_input::llm::ChatSession>,
    llm_loaded_path: &mut Option<PathBuf>,
    monitor: MonitorHandle,
) {
    let desired_asr = choose_asr_model(model_base, cfg.asr_model);
    if desired_asr != *asr_loaded_path {
        *asr = None;
        *asr_loaded_path = desired_asr.clone();

        if let Some(path) = desired_asr {
            match mofa_input::asr::AsrSession::new(&path) {
                Ok(s) => {
                    *asr = Some(s);
                    if cfg.asr_model != AsrModelChoice::Auto {
                        monitor.set_hint(&format!("ASR 已切换: {}", cfg.asr_model.label()));
                    }
                }
                Err(e) => {
                    eprintln!("[mofa-ime] ASR 加载失败 {:?}: {e}", path);
                    monitor.set_hint("ASR 加载失败");
                }
            }
        } else {
            monitor.set_hint("未发现可用 ASR 模型");
        }
    }

    let desired_llm = choose_llm_model(model_base, cfg.llm_model);
    if desired_llm != *llm_loaded_path {
        *llm = None;
        *llm_loaded_path = desired_llm.clone();

        if let Some(path) = desired_llm {
            match mofa_input::llm::ChatSession::new(&path) {
                Ok(s) => {
                    *llm = Some(s);
                    if cfg.llm_model != LlmModelChoice::Auto {
                        monitor.set_hint(&format!("LLM 已切换: {}", cfg.llm_model.label()));
                    }
                }
                Err(e) => {
                    eprintln!("[mofa-ime] LLM 加载失败 {:?}: {e}", path);
                    monitor.set_hint("LLM 加载失败");
                }
            }
        } else {
            monitor.set_hint("未发现 LLM，默认直发识别文本");
        }
    }
}

fn spawn_pipeline_worker(
    rx: Receiver<HotkeySignal>,
    status: StatusHandle,
    monitor: MonitorHandle,
    overlay: OverlayHandle,
) {
    std::thread::spawn(move || {
        let model_base = model_base_dir();

        let mut asr: Option<mofa_input::asr::AsrSession> = None;
        let mut asr_loaded_path: Option<PathBuf> = None;
        let mut llm: Option<mofa_input::llm::ChatSession> = None;
        let mut llm_loaded_path: Option<PathBuf> = None;

        monitor.set_state("就绪");
        monitor.set_asr("-");
        monitor.set_output("-");
        monitor.set_hint("-");
        overlay.hide();
        let startup_cfg = load_app_config();
        refresh_models(
            &model_base,
            startup_cfg,
            &mut asr,
            &mut asr_loaded_path,
            &mut llm,
            &mut llm_loaded_path,
            monitor,
        );

        let mut recorder: Option<ActiveRecorder> = None;
        let mut recording_ticker: Option<RecordingTicker> = None;

        while let Ok(sig) = rx.recv() {
            match sig {
                HotkeySignal::Down => {
                    if recorder.is_none() {
                        match ActiveRecorder::start() {
                            Ok(r) => {
                                let ticker = RecordingTicker::start(
                                    r.sample_buffer(),
                                    r.sample_rate(),
                                    overlay,
                                );
                                recording_ticker = Some(ticker);
                                recorder = Some(r);
                                status.set(TrayState::Recording);
                                monitor.set_state("录音中");
                                monitor.set_hint("-");
                                overlay.show_recording();
                            }
                            Err(e) => {
                                eprintln!("[mofa-ime] 录音启动失败: {e}");
                                status.set(TrayState::Error);
                                monitor.set_state("录音启动失败");
                                monitor.set_hint("录音启动失败");
                                overlay.show_error("录音启动失败");
                                std::thread::sleep(Duration::from_millis(900));
                                overlay.hide();
                            }
                        }
                    }
                }
                HotkeySignal::Up => {
                    if let Some(ticker) = recording_ticker.take() {
                        ticker.stop();
                    }

                    let app_cfg = load_app_config();
                    refresh_models(
                        &model_base,
                        app_cfg,
                        &mut asr,
                        &mut asr_loaded_path,
                        &mut llm,
                        &mut llm_loaded_path,
                        monitor,
                    );

                    let Some(r) = recorder.take() else {
                        overlay.hide();
                        continue;
                    };

                    status.set(TrayState::Processing);
                    monitor.set_state("识别中");
                    overlay.show_transcribing();

                    let samples = match r.stop() {
                        Ok(s) => s,
                        Err(e) => {
                            eprintln!("[mofa-ime] 录音结束失败: {e}");
                            status.set(TrayState::Error);
                            monitor.set_state("录音结束失败");
                            monitor.set_hint("录音结束失败");
                            overlay.show_error("录音结束失败");
                            std::thread::sleep(Duration::from_millis(900));
                            overlay.fade_out_quick();
                            continue;
                        }
                    };

                    if samples.len() < 3200 {
                        // < 0.2s @16k
                        status.set(TrayState::Idle);
                        monitor.set_state("录音过短");
                        monitor.set_hint("录音过短");
                        overlay.show_error("录音过短，请重试");
                        std::thread::sleep(Duration::from_millis(700));
                        overlay.fade_out_quick();
                        continue;
                    }

                    if audio_rms(&samples) < SILENCE_RMS_THRESHOLD {
                        status.set(TrayState::Idle);
                        monitor.set_state("无语音");
                        monitor.set_hint("检测到静音");
                        overlay.show_error("未检测到有效语音");
                        std::thread::sleep(Duration::from_millis(760));
                        overlay.fade_out_quick();
                        continue;
                    }

                    let Some(asr_session) = asr.as_ref() else {
                        eprintln!("[mofa-ime] ASR 未加载，跳过");
                        status.set(TrayState::Error);
                        monitor.set_state("ASR 未加载");
                        monitor.set_hint("ASR 模型缺失");
                        overlay.show_error("Whisper 未就绪");
                        std::thread::sleep(Duration::from_millis(900));
                        overlay.fade_out_quick();
                        continue;
                    };

                    let asr_preview = Arc::new(Mutex::new(String::new()));
                    let asr_preview_cb = Arc::clone(&asr_preview);
                    let overlay_cb = overlay;
                    let raw_text =
                        match asr_session.transcribe_with_progress(&samples, move |seg| {
                            let seg = seg.trim();
                            if seg.is_empty() {
                                return;
                            }

                            if let Ok(mut acc) = asr_preview_cb.lock() {
                                if !acc.is_empty() {
                                    acc.push(' ');
                                }
                                acc.push_str(seg);
                                overlay_cb.set_preview(acc.as_str());
                            }
                        }) {
                            Ok(t) => t.trim().to_string(),
                            Err(e) => {
                                eprintln!("[mofa-ime] ASR 失败: {e}");
                                status.set(TrayState::Error);
                                monitor.set_state("ASR 失败");
                                monitor.set_hint("语音识别失败");
                                overlay.show_error("语音识别失败");
                                std::thread::sleep(Duration::from_millis(900));
                                overlay.fade_out_quick();
                                continue;
                            }
                        };
                    let raw_text = normalize_transcript(&raw_text);
                    monitor.set_asr(&raw_text);
                    if !raw_text.is_empty() {
                        overlay.set_preview(&raw_text);
                    }

                    if should_drop_transcript(&raw_text) {
                        status.set(TrayState::Idle);
                        monitor.set_state("空识别结果");
                        monitor.set_hint("未识别到有效语音");
                        overlay.show_error("未识别到有效语音");
                        std::thread::sleep(Duration::from_millis(900));
                        overlay.fade_out_quick();
                        continue;
                    }

                    std::thread::sleep(Duration::from_millis(ASR_PREVIEW_HOLD_MS));

                    let mut final_text = raw_text.clone();
                    let mut mode_text = app_cfg.output_mode.label();
                    if app_cfg.output_mode == OutputMode::Llm {
                        overlay.show_refining();
                        if let Some(chat) = llm.as_ref() {
                            let prompt = build_refine_prompt(&raw_text);
                            chat.clear();
                            let llm_out = chat.send(&prompt, 256, 0.2).unwrap_or(raw_text.clone());
                            let llm_out = normalize_transcript(&llm_out);
                            if !llm_out.is_empty() && !is_template_noise_text(&llm_out) {
                                final_text = llm_out;
                            } else {
                                mode_text = "ASR 原文";
                                monitor.set_hint("LLM 输出无效，回退 ASR");
                            }
                        } else {
                            mode_text = "ASR 原文";
                            monitor.set_hint("LLM 未就绪，回退 ASR");
                        }
                    }

                    if should_drop_transcript(&final_text) {
                        status.set(TrayState::Idle);
                        monitor.set_state("空结果");
                        monitor.set_hint("结果被过滤");
                        overlay.show_error("未识别到有效语音");
                        std::thread::sleep(Duration::from_millis(760));
                        overlay.fade_out_quick();
                        continue;
                    }

                    monitor.set_output(&final_text);

                    if let Err(e) = inject_text(&final_text) {
                        eprintln!("[mofa-ime] 注入失败: {e}");
                        status.set(TrayState::Error);
                        monitor.set_state("发送失败");
                        monitor.set_hint("文本发送失败");
                        overlay.show_error("文本注入失败");
                        std::thread::sleep(Duration::from_millis(900));
                        overlay.fade_out_quick();
                        continue;
                    }
                    monitor.set_hint(&format!("发送模式: {mode_text}"));

                    status.set(TrayState::Injected);
                    monitor.set_state("已发送");
                    overlay.show_injected();
                    std::thread::sleep(Duration::from_millis(RESULT_OVERLAY_HOLD_MS));
                    overlay.fade_out_quick();
                }
            }
        }
    });
}
