#![allow(unexpected_cfgs)]

use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::Duration;

use anyhow::{Context, Result};
use eframe::egui;

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("model-manager 仅支持 macOS");
}

#[cfg(target_os = "macos")]
fn main() -> Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([860.0, 620.0]),
        ..Default::default()
    };

    eframe::run_native(
        "设置",
        options,
        Box::new(|cc| {
            setup_cjk_font(&cc.egui_ctx);
            setup_ui_style(&cc.egui_ctx);
            Box::new(ModelManagerApp::new())
        }),
    )
    .map_err(|e| anyhow::anyhow!("启动模型管理器失败: {e}"))?;

    Ok(())
}

fn setup_cjk_font(ctx: &egui::Context) {
    let candidates = [
        "/System/Library/Fonts/PingFang.ttc",
        "/System/Library/Fonts/Hiragino Sans GB.ttc",
        "/System/Library/Fonts/STHeiti Medium.ttc",
        "/System/Library/Fonts/Supplemental/Songti.ttc",
        "/System/Library/Fonts/Supplemental/Arial Unicode.ttf",
        "/Library/Fonts/Arial Unicode.ttf",
    ];

    for path in candidates {
        let Ok(bytes) = fs::read(path) else {
            continue;
        };

        let mut fonts = egui::FontDefinitions::default();
        fonts
            .font_data
            .insert("system-cjk".to_owned(), egui::FontData::from_owned(bytes));
        if let Some(family) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
            family.insert(0, "system-cjk".to_owned());
        }
        if let Some(family) = fonts.families.get_mut(&egui::FontFamily::Monospace) {
            family.push("system-cjk".to_owned());
        }
        ctx.set_fonts(fonts);
        return;
    }
}

fn setup_ui_style(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();
    style.spacing.interact_size.y = 30.0;
    style.spacing.button_padding = egui::vec2(10.0, 6.0);
    ctx.set_style(style);
}

fn centered_button(ui: &mut egui::Ui, label: impl Into<egui::WidgetText>) -> egui::Response {
    ui.add(egui::Button::new(label).min_size(egui::vec2(0.0, 30.0)))
}

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum LlmModel {
    Qwen05,
    Qwen15,
    Qwen3,
    Qwen7,
}

impl LlmModel {
    fn all() -> [Self; 4] {
        [Self::Qwen05, Self::Qwen15, Self::Qwen3, Self::Qwen7]
    }

    fn id(self) -> &'static str {
        match self {
            Self::Qwen05 => "llm:qwen2.5-0.5b-q4_k_m.gguf",
            Self::Qwen15 => "llm:qwen2.5-1.5b-q4_k_m.gguf",
            Self::Qwen3 => "llm:qwen2.5-3b-q4_k_m.gguf",
            Self::Qwen7 => "llm:qwen2.5-7b-q4_k_m.gguf",
        }
    }

    fn file_name(self) -> &'static str {
        match self {
            Self::Qwen05 => "qwen2.5-0.5b-q4_k_m.gguf",
            Self::Qwen15 => "qwen2.5-1.5b-q4_k_m.gguf",
            Self::Qwen3 => "qwen2.5-3b-q4_k_m.gguf",
            Self::Qwen7 => "qwen2.5-7b-q4_k_m.gguf",
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Qwen05 => "Qwen2.5 0.5B",
            Self::Qwen15 => "Qwen2.5 1.5B",
            Self::Qwen3 => "Qwen2.5 3B",
            Self::Qwen7 => "Qwen2.5 7B",
        }
    }

    fn desc(self) -> &'static str {
        match self {
            Self::Qwen05 => "极省内存，低负载设备",
            Self::Qwen15 => "16GB 设备推荐档",
            Self::Qwen3 => "默认档，质量与速度平衡",
            Self::Qwen7 => "质量更高，需更大内存",
        }
    }

    fn size_mb(self) -> u64 {
        match self {
            Self::Qwen05 => 400,
            Self::Qwen15 => 900,
            Self::Qwen3 => 1900,
            Self::Qwen7 => 4400,
        }
    }

    fn url(self) -> &'static str {
        match self {
            Self::Qwen05 => "https://huggingface.co/lmstudio-community/Qwen2.5-0.5B-Instruct-GGUF/resolve/main/Qwen2.5-0.5B-Instruct-Q4_K_M.gguf",
            Self::Qwen15 => "https://huggingface.co/lmstudio-community/Qwen2.5-1.5B-Instruct-GGUF/resolve/main/Qwen2.5-1.5B-Instruct-Q4_K_M.gguf",
            Self::Qwen3 => "https://huggingface.co/lmstudio-community/Qwen2.5-3B-Instruct-GGUF/resolve/main/Qwen2.5-3B-Instruct-Q4_K_M.gguf",
            Self::Qwen7 => "https://huggingface.co/lmstudio-community/Qwen2.5-7B-Instruct-GGUF/resolve/main/Qwen2.5-7B-Instruct-Q4_K_M.gguf",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum AsrModel {
    WhisperSmall,
    WhisperBase,
    WhisperTiny,
    WhisperMedium,
}

impl AsrModel {
    fn all() -> [Self; 4] {
        [
            Self::WhisperSmall,
            Self::WhisperBase,
            Self::WhisperTiny,
            Self::WhisperMedium,
        ]
    }

    fn id(self) -> &'static str {
        match self {
            Self::WhisperTiny => "asr:ggml-tiny.bin",
            Self::WhisperBase => "asr:ggml-base.bin",
            Self::WhisperSmall => "asr:ggml-small.bin",
            Self::WhisperMedium => "asr:ggml-medium.bin",
        }
    }

    fn file_name(self) -> &'static str {
        match self {
            Self::WhisperTiny => "ggml-tiny.bin",
            Self::WhisperBase => "ggml-base.bin",
            Self::WhisperSmall => "ggml-small.bin",
            Self::WhisperMedium => "ggml-medium.bin",
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::WhisperTiny => "Whisper Tiny",
            Self::WhisperBase => "Whisper Base",
            Self::WhisperSmall => "Whisper Small",
            Self::WhisperMedium => "Whisper Medium",
        }
    }

    fn desc(self) -> &'static str {
        match self {
            Self::WhisperTiny => "最快，精度较低",
            Self::WhisperBase => "速度与精度平衡",
            Self::WhisperSmall => "当前主流程默认",
            Self::WhisperMedium => "精度更高，体积大",
        }
    }

    fn size_mb(self) -> u64 {
        match self {
            Self::WhisperTiny => 72,
            Self::WhisperBase => 142,
            Self::WhisperSmall => 466,
            Self::WhisperMedium => 1500,
        }
    }

    fn url(self) -> &'static str {
        match self {
            Self::WhisperTiny => "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-tiny.bin",
            Self::WhisperBase => "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.bin",
            Self::WhisperSmall => "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small.bin",
            Self::WhisperMedium => "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-medium.bin",
        }
    }
}

#[derive(Debug, Clone)]
struct ModelEntry {
    id: &'static str,
    name: &'static str,
    desc: &'static str,
    file_name: &'static str,
    url: &'static str,
    size_mb: u64,
}

impl ModelEntry {
    fn path(&self, base: &Path) -> PathBuf {
        base.join(self.file_name)
    }
}

fn llm_entries() -> Vec<ModelEntry> {
    LlmModel::all()
        .into_iter()
        .map(|m| ModelEntry {
            id: m.id(),
            name: m.name(),
            desc: m.desc(),
            file_name: m.file_name(),
            url: m.url(),
            size_mb: m.size_mb(),
        })
        .collect()
}

fn asr_entries() -> Vec<ModelEntry> {
    AsrModel::all()
        .into_iter()
        .map(|m| ModelEntry {
            id: m.id(),
            name: m.name(),
            desc: m.desc(),
            file_name: m.file_name(),
            url: m.url(),
            size_mb: m.size_mb(),
        })
        .collect()
}

enum DownloadEvent {
    Progress {
        id: String,
        progress: f32,
        downloaded_mb: f64,
    },
    Done {
        id: String,
    },
    Error {
        id: String,
        message: String,
    },
}

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

    fn reload_hotkey_setting(&mut self) {
        self.config = load_app_config();
        self.hotkey_status = format!("当前: {}", self.config.hotkey.label());
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

impl eframe::App for ModelManagerApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.handle_events();
        self.capture_hotkey_from_events(ctx);
        ctx.request_repaint_after(Duration::from_millis(120));

        let llm = llm_entries();
        let asr = asr_entries();

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("设置");
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
                if centered_button(ui, "重新读取").clicked() {
                    self.hotkey_recording = false;
                    self.reload_hotkey_setting();
                }
            });

            ui.small("点“开始录制”后，直接按组合键，如 Cmd+K。");
            ui.small("支持: Cmd/Ctrl/Alt/Shift + 主键；也可用“设为 Fn”。");
            ui.small(format!("热键状态: {}", self.hotkey_status));
            ui.add_space(8.0);

            let old_output = self.config.output_mode;
            let old_llm = self.config.llm_model;
            let old_asr = self.config.asr_model;
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
                        ui.selectable_value(&mut self.config.llm_model, LlmChoice::Auto, "自动");
                        ui.selectable_value(
                            &mut self.config.llm_model,
                            LlmChoice::Qwen05,
                            LlmChoice::Qwen05.label(),
                        );
                        ui.selectable_value(
                            &mut self.config.llm_model,
                            LlmChoice::Qwen15,
                            LlmChoice::Qwen15.label(),
                        );
                        ui.selectable_value(
                            &mut self.config.llm_model,
                            LlmChoice::Qwen3,
                            LlmChoice::Qwen3.label(),
                        );
                        ui.selectable_value(
                            &mut self.config.llm_model,
                            LlmChoice::Qwen7,
                            LlmChoice::Qwen7.label(),
                        );
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

            if centered_button(ui, "保存运行设置").clicked() {
                setting_changed = true;
            }
            if old_output != self.config.output_mode
                || old_llm != self.config.llm_model
                || old_asr != self.config.asr_model
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

fn do_download(entry: &ModelEntry, model_dir: &Path, tx: &Sender<DownloadEvent>) -> Result<()> {
    fs::create_dir_all(model_dir).context("创建模型目录失败")?;

    let path = entry.path(model_dir);
    let tmp_path = path.with_extension(format!("{}.part", entry.file_name));

    if tmp_path.exists() {
        let _ = fs::remove_file(&tmp_path);
    }

    let client = reqwest::blocking::Client::builder()
        .user_agent("mofa-macos-ime/0.1")
        .build()
        .context("初始化下载客户端失败")?;

    let mut resp = client
        .get(entry.url)
        .send()
        .with_context(|| format!("请求失败: {}", entry.url))?;

    if !resp.status().is_success() {
        anyhow::bail!("HTTP {}", resp.status());
    }

    let total = resp
        .content_length()
        .unwrap_or(entry.size_mb * 1024 * 1024)
        .max(1);

    let mut out = File::create(&tmp_path)
        .with_context(|| format!("创建文件失败: {}", tmp_path.display()))?;

    let mut downloaded: u64 = 0;
    let mut buf = [0u8; 64 * 1024];

    loop {
        let n = resp.read(&mut buf).context("下载流读取失败")?;
        if n == 0 {
            break;
        }

        out.write_all(&buf[..n]).context("写入模型文件失败")?;
        downloaded += n as u64;

        let percent = ((downloaded as f64 / total as f64) * 100.0).min(100.0) as f32;
        let downloaded_mb = downloaded as f64 / 1024.0 / 1024.0;

        let _ = tx.send(DownloadEvent::Progress {
            id: entry.id.to_string(),
            progress: percent,
            downloaded_mb,
        });
    }

    out.flush().context("刷新模型文件失败")?;
    fs::rename(&tmp_path, &path).with_context(|| {
        format!(
            "重命名临时文件失败: {} -> {}",
            tmp_path.display(),
            path.display()
        )
    })?;

    let _ = tx.send(DownloadEvent::Done {
        id: entry.id.to_string(),
    });

    Ok(())
}
