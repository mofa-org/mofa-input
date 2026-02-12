use eframe::egui;
use std::path::PathBuf;
use std::sync::mpsc::{channel, Sender, Receiver};

#[derive(Clone, Copy, PartialEq)]
enum ModelSize {
    Small,    // 0.5B
    Medium,   // 1.5B
    Large,    // 7B
}

impl ModelSize {
    fn path(&self) -> PathBuf {
        let base = "/Users/yao/Desktop/code/work/mofa-org/mofa-input/models";
        match self {
            ModelSize::Small => PathBuf::from(base).join("qwen2.5-0.5b-q4_k_m.gguf"),
            ModelSize::Medium => PathBuf::from(base).join("qwen2.5-1.5b-q4_k_m.gguf"),
            ModelSize::Large => PathBuf::from(base).join("qwen2.5-7b-q4_k_m.gguf"),
        }
    }

    fn name(&self) -> &'static str {
        match self {
            ModelSize::Small => "0.5B (快)",
            ModelSize::Medium => "1.5B (均衡)",
            ModelSize::Large => "7B (智能)",
        }
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
}

struct ChatApp {
    chat: Option<mofa_input::llm::ChatSession>,
    messages: Vec<ChatMessage>,
    input: String,
    selected_model: ModelSize,      // 当前选择的模型（UI状态）
    loaded_model: Option<ModelSize>, // 实际已加载的模型
    is_loading: bool,
    is_generating: bool,
    status: String,
    token_count: i32,
    event_receiver: Receiver<AppEvent>,
    event_sender: Sender<AppEvent>,
    current_response: String,
    show_switch_confirm: bool,      // 是否显示切换确认对话框
    pending_model: Option<ModelSize>, // 待切换的模型
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
            status: "请选择模型并点击加载".to_string(),
            token_count: 0,
            event_receiver: rx,
            event_sender: tx,
            current_response: String::new(),
            show_switch_confirm: false,
            pending_model: None,
        }
    }

    fn load_model(&mut self) {
        let model_path = self.selected_model.path();
        if !model_path.exists() {
            self.status = format!("模型文件不存在");
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
        // 如果当前没有模型，直接加载
        if self.chat.is_none() {
            self.selected_model = new_model;
            self.load_model();
            return;
        }

        // 如果选择的是同一个模型，无需切换
        if self.loaded_model == Some(new_model) {
            self.status = format!("{} 模型已在运行", new_model.name());
            return;
        }

        // 如果有对话历史，显示确认对话框
        if !self.messages.is_empty() {
            self.pending_model = Some(new_model);
            self.show_switch_confirm = true;
        } else {
            // 没有对话历史，直接切换
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
        // 恢复选择为当前加载的模型
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

        // Add user message
        self.messages.push(ChatMessage {
            role: "user".to_string(),
            content: message.clone(),
        });

        // Add placeholder for assistant
        self.current_response = String::new();
        self.messages.push(ChatMessage {
            role: "assistant".to_string(),
            content: String::new(),
        });

        self.is_generating = true;
        self.status = "生成中...".to_string();

        // Clone chat session for the background thread
        let chat = self.chat.clone().unwrap();
        let sender = self.event_sender.clone();

        // Run generation in background thread so UI can receive tokens in real-time
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
                    self.status = format!("就绪 (KV缓存: {} tokens)", self.token_count);
                }
                AppEvent::ModelLoaded => {
                    let model_path = self.selected_model.path();
                    self.chat = mofa_input::llm::ChatSession::new(&model_path).ok();
                    self.loaded_model = Some(self.selected_model);
                    self.is_loading = false;
                    self.status = format!("{} 模型已加载！", self.selected_model.name());
                }
                AppEvent::Error(e) => {
                    self.is_loading = false;
                    self.status = format!("错误: {}", e);
                }
            }
        }
    }
}

impl eframe::App for ChatApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Handle background events
        self.handle_events();

        // Request continuous updates while generating
        if self.is_generating {
            ctx.request_repaint();
        }

        // Model switch confirmation dialog
        if self.show_switch_confirm {
            egui::Window::new("切换模型")
                .collapsible(false)
                .resizable(false)
                .show(ctx, |ui| {
                    ui.label(format!(
                        "切换到 {} 将清空当前对话历史。\n是否继续？",
                        self.pending_model.map(|m| m.name()).unwrap_or("")
                    ));
                    ui.horizontal(|ui| {
                        if ui.button("确认切换").clicked() {
                            self.confirm_switch();
                        }
                        if ui.button("取消").clicked() {
                            self.cancel_switch();
                        }
                    });
                });
        }

        // Top panel
        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            ui.horizontal(|ui| {
                // Model selection buttons - clicking switches model
                ui.label("模型:");

                for (model, label) in [
                    (ModelSize::Small, ModelSize::Small.name()),
                    (ModelSize::Medium, ModelSize::Medium.name()),
                    (ModelSize::Large, ModelSize::Large.name()),
                ] {
                    let is_loaded = self.loaded_model == Some(model);
                    let btn = if is_loaded {
                        egui::Button::new(format!("✓ {}", label))
                            .fill(egui::Color32::from_rgb(34, 197, 94)) // Green for active
                    } else {
                        egui::Button::new(label)
                    };

                    if ui.add(btn).clicked() && !self.is_loading && !self.is_generating {
                        self.switch_model(model);
                    }
                }

                ui.separator();

                if self.is_loading {
                    ui.spinner();
                }

                ui.separator();

                if ui.button("清空对话").clicked() {
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
                    ui.label("请从上方点击模型按钮开始（绿色✓表示当前已加载）");
                    ui.add_space(10.0);
                    ui.label("• 0.5B (快) - 速度最快，适合简单任务");
                    ui.label("• 1.5B (均衡) - 速度与质量均衡 (推荐)");
                    ui.label("• 7B (智能) - 最聪明，但需要更多内存");
                    ui.add_space(20.0);
                    ui.label("点击不同模型可随时切换（将清空对话历史）");
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
                    .hint_text("输入消息... (Shift+Enter换行, Enter发送)")
                    .desired_rows(2)
                    .lock_focus(true);

                let response = ui.add_sized(
                    egui::vec2(available_width - 80.0, 60.0),
                    text_edit
                );

                // Handle Enter key
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

            // Try to load system Chinese fonts
            let font_paths = [
                "/System/Library/Fonts/Hiragino Sans GB.ttc",  // macOS Hiragino (GB has Chinese)
                "/System/Library/Fonts/STHeiti Light.ttc",  // macOS Heiti
                "/usr/share/fonts/truetype/wqy/wqy-zenhei.ttc",  // Linux WenQuanYi
                "C:\\Windows\\Fonts\\msyh.ttc",  // Windows Microsoft YaHei
            ];

            for path in &font_paths {
                if let Ok(font_data) = std::fs::read(path) {
                    fonts.font_data.insert(
                        "chinese".to_owned(),
                        egui::FontData::from_owned(font_data),
                    );

                    // Add to proportional and monospace families
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
