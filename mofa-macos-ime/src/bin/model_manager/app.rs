struct ModelManagerApp {
    model_dir: PathBuf,
    tx: Sender<DownloadEvent>,
    rx: Receiver<DownloadEvent>,
    downloading: HashSet<String>,
    progress: HashMap<String, f32>,
    status: String,
    config: AppConfig,
    hotkey_status: String,
    hotkey_recording: bool,
}

impl ModelManagerApp {
    fn new() -> Self {
        let model_dir = dirs::home_dir()
            .map(|h| h.join(".mofa/models"))
            .unwrap_or_else(|| PathBuf::from("./models"));
        let config = load_app_config();

        let (tx, rx) = mpsc::channel();

        Self {
            model_dir,
            tx,
            rx,
            downloading: HashSet::new(),
            progress: HashMap::new(),
            status: "就绪".to_string(),
            hotkey_status: format!("当前: {}", config.hotkey.label()),
            config,
            hotkey_recording: false,
        }
    }

    fn save_hotkey_setting(&mut self, spec: HotkeySpec) {
        self.config.hotkey = spec;
        match save_app_config(&self.config) {
            Ok(_) => {
                self.hotkey_status = format!("已保存: {}", spec.label());
            }
            Err(e) => {
                self.hotkey_status = format!("保存失败: {e}");
            }
        }
    }

    fn start_hotkey_recording(&mut self) {
        self.hotkey_recording = true;
        self.hotkey_status = "请按下快捷键".to_string();
    }

    fn cancel_hotkey_recording(&mut self) {
        self.hotkey_recording = false;
        self.hotkey_status = format!("当前: {}", self.config.hotkey.label());
    }

    fn capture_hotkey_from_events(&mut self, ctx: &egui::Context) {
        if !self.hotkey_recording {
            return;
        }

        let mut captured: Option<HotkeySpec> = None;
        ctx.input(|i| {
            for event in &i.events {
                let egui::Event::Key {
                    key,
                    pressed,
                    repeat,
                    modifiers,
                    ..
                } = event
                else {
                    continue;
                };
                if !*pressed || *repeat {
                    continue;
                }
                let Some(keycode) = hotkey_code_from_egui_key(*key) else {
                    continue;
                };
                let spec = HotkeySpec {
                    keycode,
                    modifiers: hotkey_modifiers_from_egui(*modifiers),
                };
                captured = Some(spec);
                break;
            }
        });

        if let Some(spec) = captured {
            self.hotkey_recording = false;
            self.save_hotkey_setting(spec);
        }
    }

    fn save_runtime_setting(&mut self) {
        match save_app_config(&self.config) {
            Ok(_) => {
                self.status = "设置已保存".to_string();
            }
            Err(e) => {
                self.status = format!("写入设置失败: {e}");
            }
        }
    }

    fn handle_events(&mut self) {
        while let Ok(evt) = self.rx.try_recv() {
            match evt {
                DownloadEvent::Progress {
                    id,
                    progress,
                    downloaded_mb,
                } => {
                    self.progress.insert(id.clone(), progress);
                    self.status = format!("下载中 {:.1}% ({downloaded_mb:.1}MB)", progress);
                }
                DownloadEvent::Done { id } => {
                    self.downloading.remove(&id);
                    self.progress.remove(&id);
                    self.status = format!("下载完成: {id}");
                }
                DownloadEvent::Error { id, message } => {
                    self.downloading.remove(&id);
                    self.progress.remove(&id);
                    self.status = format!("下载失败: {id} ({message})");
                }
            }
        }
    }

    fn open_model_dir(&mut self) {
        if let Err(e) = fs::create_dir_all(&self.model_dir) {
            self.status = format!("创建目录失败: {e}");
            return;
        }

        match std::process::Command::new("open").arg(&self.model_dir).spawn() {
            Ok(_) => {
                self.status = "已打开模型目录".to_string();
            }
            Err(e) => {
                self.status = format!("打开目录失败: {e}");
            }
        }
    }

    fn delete_model(&mut self, entry: &ModelEntry) {
        let path = entry.path(&self.model_dir);
        if !path.exists() {
            self.status = format!("{} 不存在", entry.name);
            return;
        }

        match fs::remove_file(&path) {
            Ok(_) => {
                self.status = format!("已删除 {}", entry.name);
            }
            Err(e) => {
                self.status = format!("删除失败 {}: {e}", entry.name);
            }
        }
    }

    fn download_model(&mut self, entry: ModelEntry) {
        if self.downloading.contains(entry.id) {
            return;
        }

        let model_dir = self.model_dir.clone();
        let tx = self.tx.clone();
        let id = entry.id.to_string();
        self.downloading.insert(id.clone());
        self.progress.insert(id.clone(), 0.0);
        self.status = format!("开始下载 {}", entry.name);

        thread::spawn(move || {
            if let Err(e) = do_download(&entry, &model_dir, &tx) {
                let _ = tx.send(DownloadEvent::Error {
                    id,
                    message: e.to_string(),
                });
            }
        });
    }

    fn section(&mut self, ui: &mut egui::Ui, title: &str, entries: &[ModelEntry]) {
        ui.heading(title);
        ui.add_space(6.0);

        for entry in entries {
            let path = entry.path(&self.model_dir);
            let available = path.exists();
            let id = entry.id.to_string();
            let downloading = self.downloading.contains(&id);
            let progress = self.progress.get(&id).copied().unwrap_or(0.0);

            egui::Frame::group(ui.style())
                .inner_margin(egui::Margin::same(10.0))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            ui.strong(entry.name);
                            ui.label(entry.desc);
                            ui.small(format!("文件: {}", entry.file_name));
                            ui.small(format!("预计大小: {}MB", entry.size_mb));
                            ui.hyperlink_to("手动下载", entry.url);
                            if available {
                                let actual_mb = path
                                    .metadata()
                                    .ok()
                                    .map(|m| m.len() as f64 / 1024.0 / 1024.0)
                                    .unwrap_or(0.0);
                                ui.colored_label(
                                    egui::Color32::from_rgb(70, 140, 80),
                                    format!("已安装 ({actual_mb:.1}MB)"),
                                );
                            } else if downloading {
                                ui.colored_label(
                                    egui::Color32::from_rgb(160, 120, 30),
                                    "下载中",
                                );
                            } else {
                                ui.colored_label(
                                    egui::Color32::from_rgb(150, 80, 80),
                                    "未安装",
                                );
                            }
                        });

                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if centered_button(ui, "复制链接").clicked() {
                                ui.output_mut(|o| {
                                    o.copied_text = entry.url.to_string();
                                });
                                self.status = format!("已复制链接: {}", entry.name);
                            }
                            if available {
                                if centered_button(ui, "删除").clicked() {
                                    self.delete_model(entry);
                                }
                            } else {
                                let button = egui::Button::new(if downloading {
                                    "下载中..."
                                } else {
                                    "下载"
                                })
                                .min_size(egui::vec2(0.0, 30.0));
                                if ui.add_enabled(!downloading, button).clicked() {
                                    self.download_model(entry.clone());
                                }
                            }
                        });
                    });

                    if downloading {
                        ui.add_space(6.0);
                        ui.add(
                            egui::ProgressBar::new((progress / 100.0).clamp(0.0, 1.0))
                                .show_percentage()
                                .text(format!("{progress:.1}%")),
                        );
                    }
                });

            ui.add_space(6.0);
        }
    }
}

fn common_hotkey_presets() -> &'static [(&'static str, HotkeySpec)] {
    const PRESETS: [(&str, HotkeySpec); 19] = [
        ("Alt+R", HotkeySpec { keycode: 15, modifiers: HOTKEY_MOD_ALT }),
        ("Alt+E", HotkeySpec { keycode: 14, modifiers: HOTKEY_MOD_ALT }),
        ("Alt+T", HotkeySpec { keycode: 17, modifiers: HOTKEY_MOD_ALT }),
        ("Alt+W", HotkeySpec { keycode: 13, modifiers: HOTKEY_MOD_ALT }),
        ("Alt+A", HotkeySpec { keycode: 0, modifiers: HOTKEY_MOD_ALT }),
        ("Alt+S", HotkeySpec { keycode: 1, modifiers: HOTKEY_MOD_ALT }),
        ("Alt+D", HotkeySpec { keycode: 2, modifiers: HOTKEY_MOD_ALT }),
        ("Alt+F", HotkeySpec { keycode: 3, modifiers: HOTKEY_MOD_ALT }),
        ("Alt+X", HotkeySpec { keycode: 7, modifiers: HOTKEY_MOD_ALT }),
        ("Alt+C", HotkeySpec { keycode: 8, modifiers: HOTKEY_MOD_ALT }),
        ("Alt+V", HotkeySpec { keycode: 9, modifiers: HOTKEY_MOD_ALT }),
        ("Alt+Space", HotkeySpec { keycode: 49, modifiers: HOTKEY_MOD_ALT }),
        ("Cmd+R", HotkeySpec { keycode: 15, modifiers: HOTKEY_MOD_CMD }),
        ("Cmd+E", HotkeySpec { keycode: 14, modifiers: HOTKEY_MOD_CMD }),
        ("Cmd+T", HotkeySpec { keycode: 17, modifiers: HOTKEY_MOD_CMD }),
        ("Cmd+W", HotkeySpec { keycode: 13, modifiers: HOTKEY_MOD_CMD }),
        ("Ctrl+Space", HotkeySpec { keycode: 49, modifiers: HOTKEY_MOD_CTRL }),
        ("Shift+Space", HotkeySpec { keycode: 49, modifiers: HOTKEY_MOD_SHIFT }),
        ("Fn", HotkeySpec { keycode: HOTKEY_FN_CODE, modifiers: 0 }),
    ];
    &PRESETS
}

impl eframe::App for ModelManagerApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.handle_events();
        self.capture_hotkey_from_events(ctx);
        ctx.request_repaint_after(Duration::from_millis(120));

        let llm = llm_entries();
        let asr = asr_entries();

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("MoFA IME 设置");
            ui.small("主程序模型目录: ~/.mofa/models");
            ui.add_space(8.0);

            ui.horizontal(|ui| {
                ui.label("快捷键:");
                ui.monospace(self.config.hotkey.label());
                if self.hotkey_recording {
                    if centered_button(ui, "取消录制").clicked() {
                        self.cancel_hotkey_recording();
                    }
                } else if centered_button(ui, "开始录制").clicked() {
                    self.start_hotkey_recording();
                }
                if centered_button(ui, "设为 Fn").clicked() {
                    self.hotkey_recording = false;
                    self.save_hotkey_setting(HotkeySpec::fn_key());
                }

                let mut preset_idx: i32 = -1;
                egui::ComboBox::from_id_source("hotkey_preset_combo")
                    .selected_text("常用快捷键")
                    .show_ui(ui, |ui| {
                        for (i, (label, _)) in common_hotkey_presets().iter().enumerate() {
                            ui.selectable_value(&mut preset_idx, i as i32, *label);
                        }
                    });
                if preset_idx >= 0 {
                    let spec = common_hotkey_presets()[preset_idx as usize].1;
                    self.hotkey_recording = false;
                    self.save_hotkey_setting(spec);
                }
            });

            ui.small("点“开始录制”后，直接按组合键，如 Cmd+K。");
            ui.small("支持: Cmd/Ctrl/Alt/Shift + 主键；也可用“设为 Fn”。");
            ui.small(format!("热键状态: {}", self.hotkey_status));
            ui.add_space(8.0);

            let old_output = self.config.output_mode;
            let old_llm = self.config.llm_model;
            let old_asr = self.config.asr_model;
            let old_show_orb = self.config.show_floating_orb;
            let mut setting_changed = false;
            ui.horizontal(|ui| {
                ui.label("发送内容:");
                egui::ComboBox::from_id_source("send_output_mode")
                    .selected_text(self.config.output_mode.label())
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut self.config.output_mode,
                            OutputModeCfg::Llm,
                            OutputModeCfg::Llm.label(),
                        );
                        ui.selectable_value(
                            &mut self.config.output_mode,
                            OutputModeCfg::Asr,
                            OutputModeCfg::Asr.label(),
                        );
                    });
            });
            ui.horizontal(|ui| {
                ui.label("LLM 模型:");
                egui::ComboBox::from_id_source("llm_model_choice")
                    .selected_text(self.config.llm_model.label())
                    .show_ui(ui, |ui| {
                        for choice in LlmChoice::all() {
                            ui.selectable_value(
                                &mut self.config.llm_model,
                                choice,
                                choice.label(),
                            );
                        }
                    });
            });
            ui.horizontal(|ui| {
                ui.label("ASR 模型:");
                egui::ComboBox::from_id_source("asr_model_choice")
                    .selected_text(self.config.asr_model.label())
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.config.asr_model, AsrChoice::Auto, "自动");
                        ui.selectable_value(
                            &mut self.config.asr_model,
                            AsrChoice::Tiny,
                            AsrChoice::Tiny.label(),
                        );
                        ui.selectable_value(
                            &mut self.config.asr_model,
                            AsrChoice::Base,
                            AsrChoice::Base.label(),
                        );
                        ui.selectable_value(
                            &mut self.config.asr_model,
                            AsrChoice::Small,
                            AsrChoice::Small.label(),
                        );
                        ui.selectable_value(
                            &mut self.config.asr_model,
                            AsrChoice::Medium,
                            AsrChoice::Medium.label(),
                        );
                    });
            });

            ui.horizontal(|ui| {
                let mut show_orb = self.config.show_floating_orb;
                if ui.checkbox(&mut show_orb, "显示悬浮球").changed() {
                    self.config.show_floating_orb = show_orb;
                    setting_changed = true;
                }
            });

            if old_output != self.config.output_mode
                || old_llm != self.config.llm_model
                || old_asr != self.config.asr_model
                || old_show_orb != self.config.show_floating_orb
            {
                setting_changed = true;
            }
            if setting_changed {
                self.save_runtime_setting();
            }
            ui.add_space(8.0);

            ui.horizontal(|ui| {
                if centered_button(ui, "打开模型目录").clicked() {
                    self.open_model_dir();
                }
                if centered_button(ui, "刷新").clicked() {
                    self.status = "已刷新".to_string();
                }
                ui.label(format!("状态: {}", self.status));
            });

            ui.add_space(10.0);
            ui.separator();
            ui.add_space(10.0);
            ui.heading("模型管理");
            ui.add_space(6.0);

            egui::ScrollArea::vertical().show(ui, |ui| {
                self.section(ui, "LLM 模型", &llm);
                ui.add_space(8.0);
                self.section(ui, "ASR 模型", &asr);
            });
        });
    }
}
