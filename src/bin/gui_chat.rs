use eframe::egui;
use std::path::PathBuf;
use std::sync::mpsc::{channel, Sender, Receiver};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum ModelSize {
    Small,    // 0.5B
    Medium,   // 1.5B
    Large,    // 7B
    XLarge,   // 14B
}

impl ModelSize {
    fn path(&self) -> PathBuf {
        let base = dirs::home_dir()
            .map(|h| h.join(".mofa/models"))
            .unwrap_or_else(|| PathBuf::from("./models"));

        std::fs::create_dir_all(&base).ok();

        match self {
            ModelSize::Small => base.join("qwen2.5-0.5b-q4_k_m.gguf"),
            ModelSize::Medium => base.join("qwen2.5-1.5b-q4_k_m.gguf"),
            ModelSize::Large => base.join("qwen2.5-7b-q4_k_m.gguf"),
            ModelSize::XLarge => base.join("qwen2.5-14b-q4_k_m.gguf"),
        }
    }

    fn name(&self) -> &'static str {
        match self {
            ModelSize::Small => "0.5B",
            ModelSize::Medium => "1.5B",
            ModelSize::Large => "7B",
            ModelSize::XLarge => "14B",
        }
    }

    fn description(&self) -> &'static str {
        match self {
            ModelSize::Small => "超快，适合简单任务 (~400MB)",
            ModelSize::Medium => "推荐，速度与质量均衡 (~1GB)",
            ModelSize::Large => "更智能，需更多内存 (~4.5GB)",
            ModelSize::XLarge => "最聪明，推理能力强 (~9GB)",
        }
    }

    fn size_mb(&self) -> u64 {
        match self {
            ModelSize::Small => 400,
            ModelSize::Medium => 1000,
            ModelSize::Large => 4500,
            ModelSize::XLarge => 9000,
        }
    }

    fn download_url(&self) -> &'static str {
        match self {
            ModelSize::Small => "https://huggingface.co/lmstudio-community/Qwen2.5-0.5B-Instruct-GGUF/resolve/main/Qwen2.5-0.5B-Instruct-Q4_K_M.gguf",
            ModelSize::Medium => "https://huggingface.co/lmstudio-community/Qwen2.5-1.5B-Instruct-GGUF/resolve/main/Qwen2.5-1.5B-Instruct-Q4_K_M.gguf",
            ModelSize::Large => "https://huggingface.co/lmstudio-community/Qwen2.5-7B-Instruct-GGUF/resolve/main/Qwen2.5-7B-Instruct-Q4_K_M.gguf",
            ModelSize::XLarge => "https://huggingface.co/lmstudio-community/Qwen2.5-14B-Instruct-GGUF/resolve/main/Qwen2.5-14B-Instruct-Q4_K_M.gguf",
        }
    }

    fn all() -> [ModelSize; 4] {
        [ModelSize::Small, ModelSize::Medium, ModelSize::Large, ModelSize::XLarge]
    }
}

struct ChatMessage {
    role: String,
    content: String,
}

enum AppEvent {
    Token(String),
    GenerationComplete,
    ModelLoaded,
    Error(String),
    DownloadProgress(ModelSize, f32), // model, percent
    DownloadComplete(ModelSize),
    DownloadError(ModelSize, String),
}

struct ChatApp {
    chat: Option<mofa_input::llm::ChatSession>,
    messages: Vec<ChatMessage>,
    input: String,
    selected_model: ModelSize,
    loaded_model: Option<ModelSize>,
    is_loading: bool,
    is_generating: bool,
    status: String,
    token_count: i32,
    event_receiver: Receiver<AppEvent>,
    event_sender: Sender<AppEvent>,
    current_response: String,
    show_switch_confirm: bool,
    pending_model: Option<ModelSize>,
    download_progress: HashMap<ModelSize, f32>,
    downloading_models: HashSet<ModelSize>,
    show_download_manager: bool,
    show_delete_confirm: bool,
    pending_delete: Option<ModelSize>,
}

impl ChatApp {
    fn new() -> Self {
        let (tx, rx) = channel();
        Self {
            chat: None,
            messages: Vec::new(),
            input: String::new(),
            selected_model: ModelSize::Medium,
            loaded_model: None,
            is_loading: false,
            is_generating: false,
            status: "请选择模型".to_string(),
            token_count: 0,
            event_receiver: rx,
            event_sender: tx,
            current_response: String::new(),
            show_switch_confirm: false,
            pending_model: None,
            download_progress: HashMap::new(),
            downloading_models: HashSet::new(),
            show_download_manager: false,
            show_delete_confirm: false,
            pending_delete: None,
        }
    }

    fn is_model_available(&self, model: ModelSize) -> bool {
        model.path().exists() && !self.downloading_models.contains(&model)
    }

    fn cancel_download(&mut self, model: ModelSize) {
        self.downloading_models.remove(&model);
        self.download_progress.remove(&model);
        let path = model.path();
        if path.exists() {
            let _ = std::fs::remove_file(&path);
        }
        self.status = format!("{} 下载已取消", model.name());
    }

    fn delete_model(&mut self, model: ModelSize) {
        if self.loaded_model == Some(model) {
            self.chat = None;
            self.loaded_model = None;
            self.token_count = 0;
        }
        let path = model.path();
        if path.exists() {
            let _ = std::fs::remove_file(&path);
        }
        self.status = format!("{} 已删除", model.name());
    }

    fn has_download_tool() -> bool {
        use std::process::{Command, Stdio};
        Command::new("wget").arg("--version").stdout(Stdio::null()).stderr(Stdio::null()).status().is_ok()
            || Command::new("curl").arg("--version").stdout(Stdio::null()).stderr(Stdio::null()).status().is_ok()
    }

    fn download_model(&mut self, model: ModelSize) {
        if self.downloading_models.contains(&model) {
            return;
        }

        if !Self::has_download_tool() {
            self.status = "错误: 未找到wget或curl，请手动安装".to_string();
            return;
        }

        self.downloading_models.insert(model);
        let sender = self.event_sender.clone();
        let url = model.download_url().to_string();
        let path = model.path();

        std::thread::spawn(move || {
            // Create parent directory
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }

            // Download with progress
            match Self::download_with_progress(&url, &path, model, sender.clone()) {
                Ok(_) => {
                    let _ = sender.send(AppEvent::DownloadComplete(model));
                }
                Err(e) => {
                    let _ = sender.send(AppEvent::DownloadError(model, e));
                }
            }
        });
    }

    fn download_with_progress(
        url: &str,
        path: &PathBuf,
        model: ModelSize,
        sender: Sender<AppEvent>,
    ) -> Result<(), String> {
        use std::process::{Command, Stdio};
        use std::thread;
        use std::time::Duration;

        let path_str = path.to_string_lossy().to_string();
        let url = url.to_string();
        let expected_size = model.size_mb() * 1024 * 1024;

        let _ = sender.send(AppEvent::DownloadProgress(model, 0.0));

        // Try wget first, then curl
        let has_wget = Command::new("wget").arg("--version").stdout(Stdio::null()).stderr(Stdio::null()).status().is_ok();
        let mut child = if has_wget {
            let mut c = Command::new("wget");
            c.args([&url, "-O", &path_str, "--timeout=60", "--tries=3", "-q"])
             .stdout(Stdio::null())
             .stderr(Stdio::null())
             .spawn()
             .map_err(|e| format!("启动wget失败: {}", e))?
        } else if Command::new("curl").arg("--version").stdout(Stdio::null()).stderr(Stdio::null()).status().is_ok() {
            let mut c = Command::new("curl");
            c.args(["-L", "-o", &path_str, &url, "--connect-timeout", "60", "--max-time", "600", "-s"])
             .stdout(Stdio::null())
             .stderr(Stdio::null())
             .spawn()
             .map_err(|e| format!("启动curl失败: {}", e))?
        } else {
            return Err("未找到wget或curl，请手动安装".to_string());
        };

        let path_clone = path.clone();
        let sender_clone = sender.clone();
        let progress_handle = thread::spawn(move || {
            loop {
                thread::sleep(Duration::from_millis(500));
                if let Ok(metadata) = std::fs::metadata(&path_clone) {
                    let downloaded = metadata.len();
                    let percent = (downloaded as f64 / expected_size as f64 * 100.0).min(99.0);
                    let _ = sender_clone.send(AppEvent::DownloadProgress(model, percent as f32));
                }
            }
        });

        let result = child.wait()
            .map_err(|e| format!("等待下载失败: {}", e))?;

        // Stop progress monitoring
        drop(progress_handle);

        if result.success() {
            let _ = sender.send(AppEvent::DownloadProgress(model, 100.0));
            Ok(())
        } else {
            Err("下载失败".to_string())
        }
    }

    fn load_model(&mut self) {
        let model_path = self.selected_model.path();
        if !model_path.exists() {
            self.status = format!("模型未下载");
            return;
        }

        self.is_loading = true;
        self.status = format!("正在加载 {} 模型...", self.selected_model.name());

        let sender = self.event_sender.clone();
        std::thread::spawn(move || {
            match mofa_input::llm::ChatSession::new(&model_path) {
                Ok(_) => {
                    let _ = sender.send(AppEvent::ModelLoaded);
                }
                Err(e) => {
                    let _ = sender.send(AppEvent::Error(e.to_string()));
                }
            }
        });
    }

    fn switch_model(&mut self, new_model: ModelSize) {
        if !new_model.path().exists() {
            self.download_model(new_model);
            return;
        }

        if self.chat.is_none() {
            self.selected_model = new_model;
            self.load_model();
            return;
        }

        if self.loaded_model == Some(new_model) {
            self.status = format!("{} 已在运行", new_model.name());
            return;
        }

        if !self.messages.is_empty() {
            self.pending_model = Some(new_model);
            self.show_switch_confirm = true;
        } else {
            self.selected_model = new_model;
            self.chat = None;
            self.loaded_model = None;
            self.token_count = 0;
            self.load_model();
        }
    }

    fn confirm_switch(&mut self) {
        if let Some(new_model) = self.pending_model {
            self.selected_model = new_model;
            self.chat = None;
            self.loaded_model = None;
            self.messages.clear();
            self.token_count = 0;
            self.show_switch_confirm = false;
            self.pending_model = None;
            self.load_model();
        }
    }

    fn cancel_switch(&mut self) {
        self.show_switch_confirm = false;
        self.pending_model = None;
        if let Some(loaded) = self.loaded_model {
            self.selected_model = loaded;
        }
    }

    fn send_message(&mut self) {
        if self.input.trim().is_empty() || self.chat.is_none() || self.is_generating {
            return;
        }

        let message = self.input.trim().to_string();
        self.input.clear();

        self.messages.push(ChatMessage {
            role: "user".to_string(),
            content: message.clone(),
        });

        self.current_response = String::new();
        self.messages.push(ChatMessage {
            role: "assistant".to_string(),
            content: String::new(),
        });

        self.is_generating = true;
        self.status = "生成中...".to_string();

        let chat = self.chat.clone().unwrap();
        let sender = self.event_sender.clone();

        std::thread::spawn(move || {
            let sender2 = sender.clone();
            chat.send_stream(&message, 512, 0.7, move |token| {
                let _ = sender2.send(AppEvent::Token(token.to_string()));
            });
            let _ = sender.send(AppEvent::GenerationComplete);
        });
    }

    fn clear_chat(&mut self) {
        if let Some(chat) = &self.chat {
            chat.clear();
        }
        self.messages.clear();
        self.token_count = 0;
        self.current_response.clear();
        self.status = "对话已清空".to_string();
    }

    fn handle_events(&mut self) {
        while let Ok(event) = self.event_receiver.try_recv() {
            match event {
                AppEvent::Token(token) => {
                    self.current_response.push_str(&token);
                    if let Some(last) = self.messages.last_mut() {
                        last.content = self.current_response.clone();
                    }
                }
                AppEvent::GenerationComplete => {
                    self.is_generating = false;
                    if let Some(chat) = &self.chat {
                        self.token_count = chat.token_count();
                    }
                    self.status = format!("就绪 ({} tokens)", self.token_count);
                }
                AppEvent::ModelLoaded => {
                    let model_path = self.selected_model.path();
                    self.chat = mofa_input::llm::ChatSession::new(&model_path).ok();
                    self.loaded_model = Some(self.selected_model);
                    self.is_loading = false;
                    self.status = format!("{} 已就绪", self.selected_model.name());
                }
                AppEvent::Error(e) => {
                    self.is_loading = false;
                    self.status = format!("错误: {}", e);
                }
                AppEvent::DownloadProgress(model, percent) => {
                    self.download_progress.insert(model, percent);
                    self.status = format!("{} 下载中... {:.1}%", model.name(), percent);
                }
                AppEvent::DownloadComplete(model) => {
                    self.downloading_models.remove(&model);
                    self.download_progress.remove(&model);
                    self.status = format!("{} 下载完成，点击加载", model.name());
                }
                AppEvent::DownloadError(model, e) => {
                    self.downloading_models.remove(&model);
                    self.status = format!("{} 下载失败: {}", model.name(), e);
                }
            }
        }
    }
}

impl eframe::App for ChatApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.handle_events();

        if self.is_generating {
            ctx.request_repaint();
        }

        // Model switch confirmation
        if self.show_switch_confirm {
            egui::Window::new("切换模型")
                .collapsible(false)
                .resizable(false)
                .show(ctx, |ui| {
                    ui.label(format!(
                        "切换到 {} 将清空当前对话。\n是否继续？",
                        self.pending_model.map(|m| m.name()).unwrap_or("")
                    ));
                    ui.horizontal(|ui| {
                        if ui.button("确认").clicked() {
                            self.confirm_switch();
                        }
                        if ui.button("取消").clicked() {
                            self.cancel_switch();
                        }
                    });
                });
        }

        // Delete model confirmation
        if self.show_delete_confirm {
            egui::Window::new("删除模型")
                .collapsible(false)
                .resizable(false)
                .show(ctx, |ui| {
                    ui.label(format!(
                        "确认删除 {} 模型？\n此操作不可恢复。",
                        self.pending_delete.map(|m| m.name()).unwrap_or("")
                    ));
                    ui.horizontal(|ui| {
                        if ui.button("确认删除").clicked() {
                            if let Some(model) = self.pending_delete {
                                self.delete_model(model);
                            }
                            self.show_delete_confirm = false;
                            self.pending_delete = None;
                        }
                        if ui.button("取消").clicked() {
                            self.show_delete_confirm = false;
                            self.pending_delete = None;
                        }
                    });
                });
        }

        // Download manager window
        if self.show_download_manager {
            egui::Window::new("模型管理")
                .collapsible(false)
                .resizable(true)
                .default_size([400.0, 300.0])
                .show(ctx, |ui| {
                    ui.label("模型存储位置: ~/.mofa/models/");
                    ui.separator();

                    egui::ScrollArea::vertical().show(ui, |ui| {
                        for model in ModelSize::all() {
                            let available = self.is_model_available(model);
                            let downloading = self.downloading_models.contains(&model);

                            ui.horizontal(|ui| {
                                ui.strong(model.name());
                                ui.label(model.description());
                            });

                            ui.horizontal(|ui| {
                                if downloading {
                                    // Downloading - show progress and cancel button
                                    if let Some(&progress) = self.download_progress.get(&model) {
                                        let progress_bar = egui::ProgressBar::new(progress / 100.0)
                                            .text(format!("{:.1}%", progress))
                                            .desired_height(20.0)
                                            .desired_width(150.0);
                                        ui.add(progress_bar);
                                    } else {
                                        ui.spinner();
                                        ui.label("准备下载...");
                                    }
                                    let cancel_btn = egui::Button::new("取消")
                                        .fill(egui::Color32::from_rgb(239, 68, 68));
                                    if ui.add(cancel_btn).clicked() {
                                        self.cancel_download(model);
                                    }
                                } else if available {
                                    // Downloaded - show load/delete buttons
                                    ui.colored_label(egui::Color32::GREEN, "✓ 已下载");
                                    if self.loaded_model == Some(model) {
                                        ui.colored_label(egui::Color32::GREEN, "● 运行中");
                                        if ui.button("🗑 删除").clicked() {
                                            self.pending_delete = Some(model);
                                            self.show_delete_confirm = true;
                                        }
                                    } else {
                                        if ui.button("加载").clicked() {
                                            self.switch_model(model);
                                            self.show_download_manager = false;
                                        }
                                        let delete_btn = egui::Button::new("🗑 删除")
                                            .fill(egui::Color32::from_rgb(239, 68, 68));
                                        if ui.add(delete_btn).clicked() {
                                            self.pending_delete = Some(model);
                                            self.show_delete_confirm = true;
                                        }
                                    }
                                } else {
                                    ui.colored_label(egui::Color32::RED, "✗ 未下载");
                                    if ui.button("下载").clicked() {
                                        self.download_model(model);
                                    }
                                }
                            });

                            ui.separator();
                        }
                    });

                    if ui.button("关闭").clicked() {
                        self.show_download_manager = false;
                    }
                });
        }

        // Top panel
        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            ui.horizontal(|ui| {
                // Quick model buttons
                for model in ModelSize::all() {
                    let available = self.is_model_available(model);
                    let is_loaded = self.loaded_model == Some(model);
                    let downloading = self.downloading_models.contains(&model);

                    let btn_text = if downloading {
                        format!("{} ⏳", model.name())
                    } else if is_loaded {
                        format!("{} ●", model.name())
                    } else if available {
                        model.name().to_string()
                    } else {
                        format!("{} ✗", model.name())
                    };

                    let btn = if is_loaded {
                        egui::Button::new(&btn_text)
                            .fill(egui::Color32::from_rgb(34, 197, 94))
                    } else if !available {
                        egui::Button::new(&btn_text)
                            .fill(egui::Color32::from_rgb(239, 68, 68))
                    } else {
                        egui::Button::new(&btn_text)
                    };

                    if ui.add(btn).clicked() && !self.is_loading && !self.is_generating && !downloading {
                        if !available {
                            self.download_model(model);
                        } else {
                            self.switch_model(model);
                        }
                    }
                }

                ui.separator();

                if ui.button("模型管理").clicked() {
                    self.show_download_manager = true;
                }

                // Show download progress for active downloads
                if !self.downloading_models.is_empty() {
                    ui.separator();
                    for model in ModelSize::all() {
                        if self.downloading_models.contains(&model) {
                            ui.vertical(|ui| {
                                ui.set_width(120.0);
                                let progress = self.download_progress.get(&model).copied().unwrap_or(0.0);
                                ui.add(
                                    egui::ProgressBar::new(progress / 100.0)
                                        .text(format!("{} {:.0}%", model.name(), progress))
                                        .desired_height(16.0)
                                );
                            });
                        }
                    }
                }

                if self.is_loading {
                    ui.spinner();
                }

                ui.separator();

                if ui.button("清空").clicked() {
                    self.clear_chat();
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(&self.status);
                });
            });
            ui.separator();
        });

        // Main chat area
        egui::CentralPanel::default().show(ctx, |ui| {
            if self.chat.is_none() {
                ui.vertical_centered(|ui| {
                    ui.add_space(100.0);
                    ui.heading("👋 欢迎使用本地 LLM 聊天");
                    ui.add_space(20.0);
                    ui.label("点击上方模型按钮开始");
                    ui.add_space(10.0);
                    ui.label("绿色● = 运行中 | 红色✗ = 需下载 | ⏳ = 下载中");
                    ui.add_space(20.0);
                    ui.label("模型自动下载到: ~/.mofa/models/");
                });
            } else {
                egui::ScrollArea::vertical()
                    .stick_to_bottom(true)
                    .show(ui, |ui| {
                        for msg in &self.messages {
                            let (bg_color, name, text_color) = if msg.role == "user" {
                                (egui::Color32::from_rgb(59, 130, 246), "你", egui::Color32::WHITE)
                            } else {
                                (egui::Color32::from_rgb(31, 41, 55), "AI", egui::Color32::WHITE)
                            };

                            ui.label(egui::RichText::new(name).color(text_color).strong());

                            egui::Frame::group(ui.style())
                                .fill(bg_color)
                                .show(ui, |ui| {
                                    ui.set_width(ui.available_width());
                                    ui.label(egui::RichText::new(&msg.content).color(text_color).size(14.0));
                                });

                            ui.add_space(10.0);
                        }

                        if self.is_generating {
                            ui.horizontal(|ui| {
                                ui.spinner();
                                ui.label("生成中...");
                            });
                        }
                    });
            }
        });

        // Bottom input panel
        egui::TopBottomPanel::bottom("input_panel").show(ctx, |ui| {
            ui.horizontal(|ui| {
                let available_width = ui.available_width();
                let text_edit = egui::TextEdit::multiline(&mut self.input)
                    .hint_text("输入消息... (Enter发送, Shift+Enter换行)")
                    .desired_rows(2)
                    .lock_focus(true);

                let response = ui.add_sized(
                    egui::vec2(available_width - 80.0, 60.0),
                    text_edit
                );

                if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter) && !i.modifiers.shift) {
                    self.send_message();
                    response.request_focus();
                }

                ui.vertical(|ui| {
                    let send_btn = egui::Button::new("发送")
                        .fill(egui::Color32::from_rgb(59, 130, 246));
                    if ui.add_sized(egui::vec2(70.0, 28.0), send_btn).clicked() && !self.is_generating {
                        self.send_message();
                    }

                    if ui.add_sized(egui::vec2(70.0, 28.0), egui::Button::new("退出")).clicked() {
                        std::process::exit(0);
                    }
                });
            });
        });
    }
}

fn main() {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([900.0, 700.0])
            .with_min_inner_size([600.0, 400.0]),
        ..Default::default()
    };

    eframe::run_native(
        "本地 LLM 聊天",
        options,
        Box::new(|cc| {
            // Configure Chinese font support
            let mut fonts = egui::FontDefinitions::default();

            let font_paths = [
                "/System/Library/Fonts/Hiragino Sans GB.ttc",
                "/System/Library/Fonts/STHeiti Light.ttc",
                "/usr/share/fonts/truetype/wqy/wqy-zenhei.ttc",
                "C:\\Windows\\Fonts\\msyh.ttc",
            ];

            for path in &font_paths {
                if let Ok(font_data) = std::fs::read(path) {
                    fonts.font_data.insert(
                        "chinese".to_owned(),
                        egui::FontData::from_owned(font_data),
                    );

                    if let Some(proportional) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
                        proportional.push("chinese".to_owned());
                    }
                    if let Some(monospace) = fonts.families.get_mut(&egui::FontFamily::Monospace) {
                        monospace.push("chinese".to_owned());
                    }

                    cc.egui_ctx.set_fonts(fonts);
                    break;
                }
            }

            Box::new(ChatApp::new())
        }),
    ).unwrap();
}
