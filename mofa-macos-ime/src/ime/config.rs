static HOTKEY_STORE: OnceLock<Arc<std::sync::atomic::AtomicUsize>> = OnceLock::new();
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

    fn pack(self) -> usize {
        self.keycode as usize | ((self.modifiers as usize) << 16)
    }

    fn unpack(v: usize) -> Self {
        Self {
            keycode: (v & 0xFFFF) as u16,
            modifiers: ((v >> 16) & 0xFF) as u8,
        }
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

}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OutputMode {
    Llm,
    Asr,
}

impl OutputMode {
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
enum LlmModelChoice {
    Auto,
    Qwen05,
    Qwen15,
    Qwen3,
    Qwen4,
    Qwen7,
    Qwen8,
    Qwen14,
    Qwen14Q3,
    Qwen30A3B,
    Qwen32,
    Qwen32Q3,
    Qwen72,
    QwenCoder05,
    QwenCoder15,
    QwenCoder3,
    QwenCoder7,
    QwenCoder14,
    QwenCoder32,
}

impl LlmModelChoice {
    fn from_token(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "auto" => Some(Self::Auto),
            "qwen2.5-0.5b-q4_k_m.gguf" | "qwen0.5" => Some(Self::Qwen05),
            "qwen2.5-1.5b-q4_k_m.gguf" | "qwen1.5" => Some(Self::Qwen15),
            "qwen2.5-3b-q4_k_m.gguf" | "qwen3" => Some(Self::Qwen3),
            "qwen3-4b-q4_k_m.gguf" | "qwen4" => Some(Self::Qwen4),
            "qwen2.5-7b-q4_k_m.gguf" | "qwen7" => Some(Self::Qwen7),
            "qwen3-8b-q4_k_m.gguf" | "qwen8" => Some(Self::Qwen8),
            "qwen2.5-14b-q4_k_m.gguf" | "qwen14" => Some(Self::Qwen14),
            "qwen3-14b-q4_k_m.gguf" | "qwen3-14" => Some(Self::Qwen14Q3),
            "qwen3-30b-a3b-q4_k_m.gguf" | "qwen3-30a3b" => Some(Self::Qwen30A3B),
            "qwen2.5-32b-q4_k_m.gguf" | "qwen32" => Some(Self::Qwen32),
            "qwen3-32b-q4_k_m.gguf" | "qwen3-32" => Some(Self::Qwen32Q3),
            "qwen2.5-72b-q4_k_m.gguf" | "qwen72" => Some(Self::Qwen72),
            "qwen2.5-coder-0.5b-q4_k_m.gguf" | "qwen-coder0.5" => Some(Self::QwenCoder05),
            "qwen2.5-coder-1.5b-q4_k_m.gguf" | "qwen-coder1.5" => Some(Self::QwenCoder15),
            "qwen2.5-coder-3b-q4_k_m.gguf" | "qwen-coder3" => Some(Self::QwenCoder3),
            "qwen2.5-coder-7b-q4_k_m.gguf" | "qwen-coder7" => Some(Self::QwenCoder7),
            "qwen2.5-coder-14b-q4_k_m.gguf" | "qwen-coder14" => Some(Self::QwenCoder14),
            "qwen2.5-coder-32b-q4_k_m.gguf" | "qwen-coder32" => Some(Self::QwenCoder32),
            _ => None,
        }
    }

    fn token(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Qwen05 => "qwen2.5-0.5b-q4_k_m.gguf",
            Self::Qwen15 => "qwen2.5-1.5b-q4_k_m.gguf",
            Self::Qwen3 => "qwen2.5-3b-q4_k_m.gguf",
            Self::Qwen4 => "qwen3-4b-q4_k_m.gguf",
            Self::Qwen7 => "qwen2.5-7b-q4_k_m.gguf",
            Self::Qwen8 => "qwen3-8b-q4_k_m.gguf",
            Self::Qwen14 => "qwen2.5-14b-q4_k_m.gguf",
            Self::Qwen14Q3 => "qwen3-14b-q4_k_m.gguf",
            Self::Qwen30A3B => "qwen3-30b-a3b-q4_k_m.gguf",
            Self::Qwen32 => "qwen2.5-32b-q4_k_m.gguf",
            Self::Qwen32Q3 => "qwen3-32b-q4_k_m.gguf",
            Self::Qwen72 => "qwen2.5-72b-q4_k_m.gguf",
            Self::QwenCoder05 => "qwen2.5-coder-0.5b-q4_k_m.gguf",
            Self::QwenCoder15 => "qwen2.5-coder-1.5b-q4_k_m.gguf",
            Self::QwenCoder3 => "qwen2.5-coder-3b-q4_k_m.gguf",
            Self::QwenCoder7 => "qwen2.5-coder-7b-q4_k_m.gguf",
            Self::QwenCoder14 => "qwen2.5-coder-14b-q4_k_m.gguf",
            Self::QwenCoder32 => "qwen2.5-coder-32b-q4_k_m.gguf",
        }
    }

    fn file_name(self) -> Option<&'static str> {
        match self {
            Self::Auto => None,
            Self::Qwen05 => Some("qwen2.5-0.5b-q4_k_m.gguf"),
            Self::Qwen15 => Some("qwen2.5-1.5b-q4_k_m.gguf"),
            Self::Qwen3 => Some("qwen2.5-3b-q4_k_m.gguf"),
            Self::Qwen4 => Some("qwen3-4b-q4_k_m.gguf"),
            Self::Qwen7 => Some("qwen2.5-7b-q4_k_m.gguf"),
            Self::Qwen8 => Some("qwen3-8b-q4_k_m.gguf"),
            Self::Qwen14 => Some("qwen2.5-14b-q4_k_m.gguf"),
            Self::Qwen14Q3 => Some("qwen3-14b-q4_k_m.gguf"),
            Self::Qwen30A3B => Some("qwen3-30b-a3b-q4_k_m.gguf"),
            Self::Qwen32 => Some("qwen2.5-32b-q4_k_m.gguf"),
            Self::Qwen32Q3 => Some("qwen3-32b-q4_k_m.gguf"),
            Self::Qwen72 => Some("qwen2.5-72b-q4_k_m.gguf"),
            Self::QwenCoder05 => Some("qwen2.5-coder-0.5b-q4_k_m.gguf"),
            Self::QwenCoder15 => Some("qwen2.5-coder-1.5b-q4_k_m.gguf"),
            Self::QwenCoder3 => Some("qwen2.5-coder-3b-q4_k_m.gguf"),
            Self::QwenCoder7 => Some("qwen2.5-coder-7b-q4_k_m.gguf"),
            Self::QwenCoder14 => Some("qwen2.5-coder-14b-q4_k_m.gguf"),
            Self::QwenCoder32 => Some("qwen2.5-coder-32b-q4_k_m.gguf"),
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Auto => "自动",
            Self::Qwen05 => "Qwen2.5 0.5B",
            Self::Qwen15 => "Qwen2.5 1.5B",
            Self::Qwen3 => "Qwen2.5 3B",
            Self::Qwen7 => "Qwen2.5 7B",
            Self::Qwen4 => "Qwen3 4B",
            Self::Qwen8 => "Qwen3 8B",
            Self::Qwen14 => "Qwen2.5 14B",
            Self::Qwen14Q3 => "Qwen3 14B",
            Self::Qwen30A3B => "Qwen3 30B-A3B",
            Self::Qwen32 => "Qwen2.5 32B",
            Self::Qwen32Q3 => "Qwen3 32B",
            Self::Qwen72 => "Qwen2.5 72B",
            Self::QwenCoder05 => "Qwen2.5-Coder 0.5B",
            Self::QwenCoder15 => "Qwen2.5-Coder 1.5B",
            Self::QwenCoder3 => "Qwen2.5-Coder 3B",
            Self::QwenCoder7 => "Qwen2.5-Coder 7B",
            Self::QwenCoder14 => "Qwen2.5-Coder 14B",
            Self::QwenCoder32 => "Qwen2.5-Coder 32B",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AsrModelChoice {
    Auto,
    Tiny,
    Base,
    Small,
    Medium,
}

impl AsrModelChoice {
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

    fn file_name(self) -> Option<&'static str> {
        match self {
            Self::Auto => None,
            Self::Tiny => Some("ggml-tiny.bin"),
            Self::Base => Some("ggml-base.bin"),
            Self::Small => Some("ggml-small.bin"),
            Self::Medium => Some("ggml-medium.bin"),
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
    output_mode: OutputMode,
    llm_model: LlmModelChoice,
    asr_model: AsrModelChoice,
    show_floating_orb: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            hotkey: HotkeySpec::fn_key(),
            output_mode: OutputMode::Llm,
            llm_model: LlmModelChoice::Auto,
            asr_model: AsrModelChoice::Auto,
            show_floating_orb: true,
        }
    }
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

fn hotkey_config_path() -> PathBuf {
    dirs::home_dir()
        .map(|h| h.join(".mofa/macos-ime.conf"))
        .unwrap_or_else(|| PathBuf::from("./mofa-macos-ime.conf"))
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
            if let Some(mode) = OutputMode::from_token(v) {
                cfg.output_mode = mode;
            }
        } else if let Some(v) = line.strip_prefix("llm_model=") {
            if let Some(choice) = LlmModelChoice::from_token(v) {
                cfg.llm_model = choice;
            }
        } else if let Some(v) = line.strip_prefix("asr_model=") {
            if let Some(choice) = AsrModelChoice::from_token(v) {
                cfg.asr_model = choice;
            }
        } else if let Some(v) = line.strip_prefix("show_floating_orb=") {
            cfg.show_floating_orb = v.trim().to_ascii_lowercase() == "true";
        }
    }

    cfg
}

fn spawn_hotkey_config_watcher(store: Arc<std::sync::atomic::AtomicUsize>) {
    std::thread::spawn(move || loop {
        let loaded = load_app_config().hotkey;
        let current = HotkeySpec::unpack(store.load(Ordering::SeqCst));
        if loaded != current {
            store.store(loaded.pack(), Ordering::SeqCst);
        }
        std::thread::sleep(Duration::from_secs(1));
    });
}

// Global state for floating orb visibility
static ORB_VISIBLE: OnceLock<Arc<AtomicBool>> = OnceLock::new();

fn get_orb_visible() -> &'static Arc<AtomicBool> {
    ORB_VISIBLE.get_or_init(|| Arc::new(AtomicBool::new(true)))
}

pub fn set_orb_visible(visible: bool) {
    get_orb_visible().store(visible, Ordering::SeqCst);
}

pub fn is_orb_visible() -> bool {
    get_orb_visible().load(Ordering::SeqCst)
}

pub fn spawn_orb_config_watcher(overlay: OverlayHandle) {
    std::thread::spawn(move || {
        let orb_state = get_orb_visible();
        let mut last_visible = orb_state.load(Ordering::SeqCst);
        loop {
            let cfg = load_app_config();
            let current_visible = cfg.show_floating_orb;
            orb_state.store(current_visible, Ordering::SeqCst);

            // Handle visibility change
            if current_visible != last_visible {
                if current_visible {
                    overlay.show_orb();
                } else {
                    overlay.hide_orb();
                }
                last_visible = current_visible;
            }

            std::thread::sleep(Duration::from_secs(1));
        }
    });
}
