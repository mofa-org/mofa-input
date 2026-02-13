const HOTKEY_FN_CODE: u16 = u16::MAX;
const HOTKEY_MOD_CMD: u8 = 1 << 0;
const HOTKEY_MOD_CTRL: u8 = 1 << 1;
const HOTKEY_MOD_ALT: u8 = 1 << 2;
const HOTKEY_MOD_SHIFT: u8 = 1 << 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct HotkeySpec {
    keycode: u16,
    modifiers: u8,
}

impl HotkeySpec {
    fn fn_key() -> Self {
        Self {
            keycode: HOTKEY_FN_CODE,
            modifiers: 0,
        }
    }

    fn is_fn(self) -> bool {
        self.keycode == HOTKEY_FN_CODE
    }

    fn parse(input: &str) -> Option<Self> {
        let text = input.trim().to_ascii_lowercase();
        if text.is_empty() {
            return None;
        }
        if text == "fn" {
            return Some(Self::fn_key());
        }

        let mut modifiers = 0u8;
        let mut keycode: Option<u16> = None;
        for part in text.split('+') {
            let p = part.trim();
            if p.is_empty() {
                continue;
            }
            let matched_modifier = match p {
                "cmd" | "command" => Some(HOTKEY_MOD_CMD),
                "ctrl" | "control" => Some(HOTKEY_MOD_CTRL),
                "alt" | "option" => Some(HOTKEY_MOD_ALT),
                "shift" => Some(HOTKEY_MOD_SHIFT),
                _ => None,
            };
            if let Some(m) = matched_modifier {
                modifiers |= m;
                continue;
            }

            let code = hotkey_code_from_token(p)?;
            if keycode.is_some() {
                return None;
            }
            keycode = Some(code);
        }

        let keycode = keycode?;
        if keycode == HOTKEY_FN_CODE && modifiers != 0 {
            return None;
        }
        Some(Self { keycode, modifiers })
    }

    fn token(self) -> String {
        if self.is_fn() {
            return "fn".to_string();
        }

        let mut parts: Vec<String> = Vec::new();
        if self.modifiers & HOTKEY_MOD_CMD != 0 {
            parts.push("cmd".to_string());
        }
        if self.modifiers & HOTKEY_MOD_CTRL != 0 {
            parts.push("ctrl".to_string());
        }
        if self.modifiers & HOTKEY_MOD_ALT != 0 {
            parts.push("alt".to_string());
        }
        if self.modifiers & HOTKEY_MOD_SHIFT != 0 {
            parts.push("shift".to_string());
        }
        parts.push(hotkey_code_to_token(self.keycode));
        parts.join("+")
    }

    fn label(self) -> String {
        if self.is_fn() {
            return "Fn".to_string();
        }
        let mut parts: Vec<&str> = Vec::new();
        if self.modifiers & HOTKEY_MOD_CMD != 0 {
            parts.push("Cmd");
        }
        if self.modifiers & HOTKEY_MOD_CTRL != 0 {
            parts.push("Ctrl");
        }
        if self.modifiers & HOTKEY_MOD_ALT != 0 {
            parts.push("Alt");
        }
        if self.modifiers & HOTKEY_MOD_SHIFT != 0 {
            parts.push("Shift");
        }
        let key = hotkey_code_to_label(self.keycode);
        if parts.is_empty() {
            key
        } else {
            format!("{}+{}", parts.join("+"), key)
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OutputModeCfg {
    Llm,
    Asr,
}

impl OutputModeCfg {
    fn from_token(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "llm" => Some(Self::Llm),
            "asr" => Some(Self::Asr),
            _ => None,
        }
    }

    fn token(self) -> &'static str {
        match self {
            Self::Llm => "llm",
            Self::Asr => "asr",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Llm => "LLM 润色",
            Self::Asr => "ASR 原文",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LlmChoice {
    Auto,
    Qwen05,
    Qwen15,
    Qwen3,
    Qwen7,
}

impl LlmChoice {
    fn from_token(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "auto" => Some(Self::Auto),
            "qwen2.5-0.5b-q4_k_m.gguf" | "qwen0.5" => Some(Self::Qwen05),
            "qwen2.5-1.5b-q4_k_m.gguf" | "qwen1.5" => Some(Self::Qwen15),
            "qwen2.5-3b-q4_k_m.gguf" | "qwen3" => Some(Self::Qwen3),
            "qwen2.5-7b-q4_k_m.gguf" | "qwen7" => Some(Self::Qwen7),
            _ => None,
        }
    }

    fn token(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Qwen05 => "qwen2.5-0.5b-q4_k_m.gguf",
            Self::Qwen15 => "qwen2.5-1.5b-q4_k_m.gguf",
            Self::Qwen3 => "qwen2.5-3b-q4_k_m.gguf",
            Self::Qwen7 => "qwen2.5-7b-q4_k_m.gguf",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Auto => "自动",
            Self::Qwen05 => "Qwen2.5 0.5B",
            Self::Qwen15 => "Qwen2.5 1.5B",
            Self::Qwen3 => "Qwen2.5 3B",
            Self::Qwen7 => "Qwen2.5 7B",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AsrChoice {
    Auto,
    Tiny,
    Base,
    Small,
    Medium,
}

impl AsrChoice {
    fn from_token(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "auto" => Some(Self::Auto),
            "ggml-tiny.bin" | "tiny" => Some(Self::Tiny),
            "ggml-base.bin" | "base" => Some(Self::Base),
            "ggml-small.bin" | "small" => Some(Self::Small),
            "ggml-medium.bin" | "medium" => Some(Self::Medium),
            _ => None,
        }
    }

    fn token(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Tiny => "ggml-tiny.bin",
            Self::Base => "ggml-base.bin",
            Self::Small => "ggml-small.bin",
            Self::Medium => "ggml-medium.bin",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Auto => "自动",
            Self::Tiny => "Whisper Tiny",
            Self::Base => "Whisper Base",
            Self::Small => "Whisper Small",
            Self::Medium => "Whisper Medium",
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct AppConfig {
    hotkey: HotkeySpec,
    output_mode: OutputModeCfg,
    llm_model: LlmChoice,
    asr_model: AsrChoice,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            hotkey: HotkeySpec::fn_key(),
            output_mode: OutputModeCfg::Llm,
            llm_model: LlmChoice::Auto,
            asr_model: AsrChoice::Auto,
        }
    }
}

fn hotkey_config_path() -> PathBuf {
    dirs::home_dir()
        .map(|h| h.join(".mofa/macos-ime.conf"))
        .unwrap_or_else(|| PathBuf::from("./mofa-macos-ime.conf"))
}

fn hotkey_code_from_token(token: &str) -> Option<u16> {
    let t = token.trim().to_ascii_lowercase();
    if t == "fn" {
        return Some(HOTKEY_FN_CODE);
    }

    if let Some(raw) = t.strip_prefix("keycode:") {
        if let Ok(v) = raw.trim().parse::<u16>() {
            return Some(v);
        }
    }
    if let Ok(v) = t.parse::<u16>() {
        return Some(v);
    }

    let code = match t.as_str() {
        "a" => 0,
        "s" => 1,
        "d" => 2,
        "f" => 3,
        "h" => 4,
        "g" => 5,
        "z" => 6,
        "x" => 7,
        "c" => 8,
        "v" => 9,
        "b" => 11,
        "q" => 12,
        "w" => 13,
        "e" => 14,
        "r" => 15,
        "y" => 16,
        "t" => 17,
        "1" => 18,
        "2" => 19,
        "3" => 20,
        "4" => 21,
        "6" => 22,
        "5" => 23,
        "equal" | "=" => 24,
        "9" => 25,
        "7" => 26,
        "minus" | "-" => 27,
        "8" => 28,
        "0" => 29,
        "return" | "enter" => 36,
        "tab" => 48,
        "space" => 49,
        "delete" | "backspace" => 51,
        "esc" | "escape" => 53,
        "f1" => 122,
        "f2" => 120,
        "f3" => 99,
        "f4" => 118,
        "f5" => 96,
        "f6" => 97,
        "f7" => 98,
        "f8" => 100,
        "f9" => 101,
        "f10" => 109,
        "f11" => 103,
        "f12" => 111,
        _ => return None,
    };
    Some(code)
}

fn hotkey_code_to_label(code: u16) -> String {
    if code == HOTKEY_FN_CODE {
        return "Fn".to_string();
    }
    let label = match code {
        0 => "A",
        1 => "S",
        2 => "D",
        3 => "F",
        4 => "H",
        5 => "G",
        6 => "Z",
        7 => "X",
        8 => "C",
        9 => "V",
        11 => "B",
        12 => "Q",
        13 => "W",
        14 => "E",
        15 => "R",
        16 => "Y",
        17 => "T",
        18 => "1",
        19 => "2",
        20 => "3",
        21 => "4",
        22 => "6",
        23 => "5",
        24 => "=",
        25 => "9",
        26 => "7",
        27 => "-",
        28 => "8",
        29 => "0",
        36 => "Return",
        48 => "Tab",
        49 => "Space",
        51 => "Delete",
        53 => "Esc",
        96 => "F5",
        97 => "F6",
        98 => "F7",
        99 => "F3",
        100 => "F8",
        101 => "F9",
        103 => "F11",
        109 => "F10",
        111 => "F12",
        118 => "F4",
        120 => "F2",
        122 => "F1",
        _ => return format!("Keycode {}", code),
    };
    label.to_string()
}

fn hotkey_code_to_token(code: u16) -> String {
    if code == HOTKEY_FN_CODE {
        return "fn".to_string();
    }
    let label = hotkey_code_to_label(code);
    if label.starts_with("Keycode ") {
        format!("keycode:{code}")
    } else {
        label.to_ascii_lowercase()
    }
}

fn load_app_config() -> AppConfig {
    let path = hotkey_config_path();
    let Ok(content) = fs::read_to_string(path) else {
        return AppConfig::default();
    };

    let mut cfg = AppConfig::default();
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(v) = line.strip_prefix("hotkey=") {
            if let Some(spec) = HotkeySpec::parse(v) {
                cfg.hotkey = spec;
            }
        } else if let Some(v) = line.strip_prefix("output_mode=") {
            if let Some(mode) = OutputModeCfg::from_token(v) {
                cfg.output_mode = mode;
            }
        } else if let Some(v) = line.strip_prefix("llm_model=") {
            if let Some(choice) = LlmChoice::from_token(v) {
                cfg.llm_model = choice;
            }
        } else if let Some(v) = line.strip_prefix("asr_model=") {
            if let Some(choice) = AsrChoice::from_token(v) {
                cfg.asr_model = choice;
            }
        }
    }

    cfg
}

fn save_app_config(cfg: &AppConfig) -> Result<()> {
    let path = hotkey_config_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("创建配置目录失败: {}", parent.display()))?;
    }
    let mut lines: Vec<String> = match fs::read_to_string(&path) {
        Ok(content) => content.lines().map(|line| line.to_string()).collect(),
        Err(_) => Vec::new(),
    };
    let pairs = [
        ("hotkey", cfg.hotkey.token()),
        ("output_mode", cfg.output_mode.token().to_string()),
        ("llm_model", cfg.llm_model.token().to_string()),
        ("asr_model", cfg.asr_model.token().to_string()),
    ];

    for (key, value) in pairs {
        let wanted = format!("{key}={value}");
        let mut replaced = false;
        for line in &mut lines {
            if line.trim_start().starts_with(&format!("{key}=")) {
                *line = wanted.clone();
                replaced = true;
                break;
            }
        }
        if !replaced {
            lines.push(wanted);
        }
    }
    let mut out = lines.join("\n");
    if !out.ends_with('\n') {
        out.push('\n');
    }
    fs::write(&path, out).with_context(|| format!("写入配置失败: {}", path.display()))?;
    Ok(())
}

fn hotkey_modifiers_from_egui(modifiers: egui::Modifiers) -> u8 {
    let mut out = 0u8;
    if modifiers.command {
        out |= HOTKEY_MOD_CMD;
    }
    if modifiers.ctrl {
        out |= HOTKEY_MOD_CTRL;
    }
    if modifiers.alt {
        out |= HOTKEY_MOD_ALT;
    }
    if modifiers.shift {
        out |= HOTKEY_MOD_SHIFT;
    }
    out
}

fn hotkey_code_from_egui_key(key: egui::Key) -> Option<u16> {
    use egui::Key;
    let code = match key {
        Key::A => 0,
        Key::S => 1,
        Key::D => 2,
        Key::F => 3,
        Key::H => 4,
        Key::G => 5,
        Key::Z => 6,
        Key::X => 7,
        Key::C => 8,
        Key::V => 9,
        Key::B => 11,
        Key::Q => 12,
        Key::W => 13,
        Key::E => 14,
        Key::R => 15,
        Key::Y => 16,
        Key::T => 17,
        Key::Num1 => 18,
        Key::Num2 => 19,
        Key::Num3 => 20,
        Key::Num4 => 21,
        Key::Num6 => 22,
        Key::Num5 => 23,
        Key::Num9 => 25,
        Key::Num7 => 26,
        Key::Num8 => 28,
        Key::Num0 => 29,
        Key::Enter => 36,
        Key::Tab => 48,
        Key::Space => 49,
        Key::Backspace => 51,
        Key::Escape => 53,
        Key::F1 => 122,
        Key::F2 => 120,
        Key::F3 => 99,
        Key::F4 => 118,
        Key::F5 => 96,
        Key::F6 => 97,
        Key::F7 => 98,
        Key::F8 => 100,
        Key::F9 => 101,
        Key::F10 => 109,
        Key::F11 => 103,
        Key::F12 => 111,
        _ => return None,
    };
    Some(code)
}
