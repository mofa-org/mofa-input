#![allow(unexpected_cfgs)]

use anyhow::{anyhow, bail, Context, Result};
use cocoa::appkit::{
    NSApplication, NSApplicationActivationPolicyAccessory, NSBackingStoreBuffered, NSButton,
    NSMainMenuWindowLevel, NSMenu, NSMenuItem, NSPasteboard, NSPasteboardTypeString, NSStatusBar,
    NSStatusItem, NSTextField, NSVariableStatusItemLength, NSView, NSWindow,
    NSWindowCollectionBehavior, NSWindowStyleMask,
};
use cocoa::base::{id, nil, NO, YES};
use cocoa::foundation::{NSAutoreleasePool, NSPoint, NSRect, NSSize, NSString};
use core_foundation::base::{CFRelease, CFType, TCFType};
use core_foundation::runloop::{kCFRunLoopCommonModes, CFRunLoop, CFRunLoopSource};
use core_foundation::string::CFString;
use core_graphics::event::{
    CGEvent, CGEventFlags, CGEventTap, CGEventTapLocation, CGEventTapOptions, CGEventTapPlacement,
    CGEventType, CGKeyCode, EventField, KeyCode,
};
use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
use dispatch::Queue;
use objc::declare::ClassDecl;
use objc::runtime::{Class, Object, Sel};
use objc::{class, msg_send, sel, sel_impl};
use std::ffi::{c_void, CStr, CString};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::OnceLock;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("mofa-macos-ime 仅支持 macOS");
}

#[cfg(target_os = "macos")]
fn main() -> Result<()> {
    run_app()
}

#[cfg(target_os = "macos")]
fn run_app() -> Result<()> {
    let _pool = unsafe { NSAutoreleasePool::new(nil) };

    let app = unsafe { NSApplication::sharedApplication(nil) };
    unsafe {
        app.setActivationPolicy_(NSApplicationActivationPolicyAccessory);
    }

    let hotkey_spec = load_app_config().hotkey;
    let hotkey_store = Arc::new(std::sync::atomic::AtomicUsize::new(hotkey_spec.pack()));
    let _ = HOTKEY_STORE.set(Arc::clone(&hotkey_store));

    let (status_handle, monitor_handle, _status_item, _menu, _menu_handler) =
        unsafe { install_status_item(app)? };
    let overlay_handle = unsafe { install_overlay()? };

    let (hotkey_tx, hotkey_rx) = mpsc::channel::<HotkeySignal>();
    spawn_pipeline_worker(hotkey_rx, status_handle, monitor_handle, overlay_handle);
    spawn_hotkey_config_watcher(Arc::clone(&hotkey_store));

    let _hotkey_guard = install_hotkey_tap(hotkey_tx, hotkey_store)?;

    status_handle.set(TrayState::Idle);
    overlay_handle.hide();

    unsafe {
        app.run();
    }

    Ok(())
}

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
    Qwen7,
}

impl LlmModelChoice {
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

    fn file_name(self) -> Option<&'static str> {
        match self {
            Self::Auto => None,
            Self::Qwen05 => Some("qwen2.5-0.5b-q4_k_m.gguf"),
            Self::Qwen15 => Some("qwen2.5-1.5b-q4_k_m.gguf"),
            Self::Qwen3 => Some("qwen2.5-3b-q4_k_m.gguf"),
            Self::Qwen7 => Some("qwen2.5-7b-q4_k_m.gguf"),
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
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            hotkey: HotkeySpec::fn_key(),
            output_mode: OutputMode::Llm,
            llm_model: LlmModelChoice::Auto,
            asr_model: AsrModelChoice::Auto,
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
    let token = match code {
        24 => "=".to_string(),
        27 => "-".to_string(),
        36 => "return".to_string(),
        48 => "tab".to_string(),
        49 => "space".to_string(),
        51 => "delete".to_string(),
        53 => "esc".to_string(),
        _ => {
            let label = hotkey_code_to_label(code);
            if label.starts_with("Keycode ") {
                format!("keycode:{code}")
            } else {
                label.to_ascii_lowercase()
            }
        }
    };
    token
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

#[derive(Clone, Copy)]
enum TrayState {
    Idle,
    Recording,
    Processing,
    Injected,
    Error,
}

impl TrayState {
    fn title(self) -> &'static str {
        match self {
            TrayState::Idle => "就绪",
            TrayState::Recording => "录音中",
            TrayState::Processing => "识别中",
            TrayState::Injected => "已发送",
            TrayState::Error => "失败",
        }
    }

    fn symbol_name(self) -> &'static str {
        match self {
            TrayState::Idle => "circle",
            TrayState::Recording => "mic.fill",
            TrayState::Processing => "hourglass",
            TrayState::Injected => "checkmark.circle.fill",
            TrayState::Error => "exclamationmark.triangle.fill",
        }
    }
}

#[derive(Clone, Copy)]
struct StatusHandle {
    button_ptr: usize,
}

impl StatusHandle {
    fn set(self, state: TrayState) {
        let button_ptr = self.button_ptr;
        let title = state.title().to_string();
        let symbol = state.symbol_name().to_string();
        Queue::main().exec_async(move || unsafe {
            let button = button_ptr as id;
            if button != nil {
                set_status_button_symbol(button, &symbol);
                NSButton::setTitle_(button, ns_string(&title));
            }
        });
    }
}

#[derive(Clone, Copy)]
struct MonitorHandle {
    state_item_ptr: usize,
    asr_item_ptr: usize,
    output_item_ptr: usize,
    hint_item_ptr: usize,
}

impl MonitorHandle {
    fn set_state(self, text: &str) {
        self.set_item(self.state_item_ptr, "状态", text);
    }

    fn set_asr(self, text: &str) {
        self.set_item(self.asr_item_ptr, "识别", text);
    }

    fn set_output(self, text: &str) {
        self.set_item(self.output_item_ptr, "发送", text);
    }

    fn set_hint(self, text: &str) {
        self.set_item(self.hint_item_ptr, "提示", text);
    }

    fn set_item(self, item_ptr: usize, label: &str, value: &str) {
        let title = format!("{label}: {}", truncate_middle(value, 64));
        Queue::main().exec_async(move || unsafe {
            let item = item_ptr as id;
            if item != nil {
                let _: () = msg_send![item, setTitle: ns_string(&title)];
            }
        });
    }
}

#[derive(Clone, Copy)]
struct OverlayHandle {
    window_ptr: usize,
    status_label_ptr: usize,
    preview_label_ptr: usize,
}

impl OverlayHandle {
    fn show_recording(self) {
        self.show("录音中", "请说话，松开快捷键结束");
    }

    fn show_transcribing(self) {
        self.show("转录中", "语音识别进行中");
    }

    fn show_refining(self) {
        self.update(true, Some("润色中".to_string()), None);
    }

    fn show_injected(self) {
        self.show("已发送", "文本已写入目标输入框");
    }

    fn show_error(self, message: &str) {
        self.show("失败", message);
    }

    fn set_status(self, text: &str) {
        self.update(true, Some(text.to_string()), None);
    }

    fn set_preview(self, text: &str) {
        let line = wrap_preview_text(text);
        self.update(true, None, Some(line));
    }

    fn hide(self) {
        self.update(false, None, None);
    }

    fn fade_out_quick(self) {
        let window_ptr = self.window_ptr;
        let step_ms = (OVERLAY_FADE_TOTAL_MS / OVERLAY_FADE_STEPS.max(1)).max(1);
        for idx in (0..OVERLAY_FADE_STEPS).rev() {
            let alpha = idx as f64 / OVERLAY_FADE_STEPS as f64;
            Queue::main().exec_sync(move || unsafe {
                let window = window_ptr as id;
                if window != nil {
                    let _: () = msg_send![window, setAlphaValue: alpha];
                }
            });
            std::thread::sleep(Duration::from_millis(step_ms));
        }
        Queue::main().exec_sync(move || unsafe {
            let window = window_ptr as id;
            if window != nil {
                window.orderOut_(nil);
                let _: () = msg_send![window, setAlphaValue: 1.0f64];
            }
        });
    }

    fn show(self, status: &str, preview: &str) {
        self.update(
            true,
            Some(status.to_string()),
            Some(wrap_preview_text(preview)),
        );
    }

    fn update(self, visible: bool, status: Option<String>, preview: Option<String>) {
        let window_ptr = self.window_ptr;
        let status_ptr = self.status_label_ptr;
        let preview_ptr = self.preview_label_ptr;
        Queue::main().exec_async(move || unsafe {
            let window = window_ptr as id;
            if window == nil {
                return;
            }
            let preview_for_layout = preview.map(|p| wrap_preview_text(&p));

            if let Some(s) = status {
                let status_label = status_ptr as id;
                if status_label != nil {
                    let _: () = msg_send![status_label, setStringValue: ns_string(&s)];
                    set_status_badge_appearance(status_label, &s);
                }
            }

            if let Some(p) = preview_for_layout.as_ref() {
                let preview_label = preview_ptr as id;
                if preview_label != nil {
                    let _: () = msg_send![preview_label, setStringValue: ns_string(p)];
                }
            }

            let preview_label = preview_ptr as id;
            let status_label = status_ptr as id;
            if preview_label != nil && status_label != nil {
                let preview_text = if let Some(current) = preview_for_layout.as_ref() {
                    current.clone()
                } else {
                    let preview_ns: id = msg_send![preview_label, stringValue];
                    nsstring_to_rust(preview_ns).unwrap_or_default()
                };
                layout_overlay_window(window, status_label, preview_label, &preview_text);
            }

            if visible {
                position_overlay_window(window);
                let _: () = msg_send![window, setAlphaValue: 1.0f64];
                window.orderFrontRegardless();
            } else {
                window.orderOut_(nil);
                let _: () = msg_send![window, setAlphaValue: 1.0f64];
            }
        });
    }
}

fn truncate_middle(s: &str, max_chars: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= max_chars {
        return s.to_string();
    }
    if max_chars < 8 {
        return chars.into_iter().take(max_chars).collect();
    }
    let head = (max_chars - 1) / 2;
    let tail = max_chars - 1 - head;
    let mut out = String::new();
    out.extend(chars[..head].iter());
    out.push('…');
    out.extend(chars[chars.len() - tail..].iter());
    out
}

unsafe fn make_info_item(title: &str, target: id) -> id {
    let item = NSMenuItem::alloc(nil)
        .initWithTitle_action_keyEquivalent_(ns_string(title), sel!(noopInfo:), ns_string(""))
        .autorelease();
    NSMenuItem::setTarget_(item, target);
    let _: () = msg_send![item, setEnabled: NO];
    item
}

extern "C" fn open_model_manager_action(_this: &Object, _cmd: Sel, _sender: id) {
    if let Err(e) = spawn_model_manager() {
        eprintln!("[mofa-ime] 打开模型管理器失败: {e}");
    }
}

extern "C" fn noop_info_action(_this: &Object, _cmd: Sel, _sender: id) {}

fn menu_handler_class() -> *const Class {
    static CLS: OnceLock<usize> = OnceLock::new();
    *CLS.get_or_init(|| unsafe {
        let superclass = class!(NSObject);
        let mut decl =
            ClassDecl::new("MofaMenuHandler", superclass).expect("failed to declare class");
        decl.add_method(
            sel!(openModelManager:),
            open_model_manager_action as extern "C" fn(&Object, Sel, id),
        );
        decl.add_method(
            sel!(noopInfo:),
            noop_info_action as extern "C" fn(&Object, Sel, id),
        );
        (decl.register() as *const Class) as usize
    }) as *const Class
}

unsafe fn new_menu_handler() -> id {
    let cls = menu_handler_class();
    let obj: id = msg_send![cls, new];
    obj
}

fn spawn_model_manager() -> Result<()> {
    let exe = std::env::current_exe().context("无法获取当前可执行文件路径")?;
    let bin_dir = exe.parent().ok_or_else(|| anyhow!("无法获取可执行目录"))?;
    let project_dir = bin_dir
        .parent()
        .and_then(|p| p.parent())
        .ok_or_else(|| anyhow!("无法推断项目目录"))?;
    let cargo_toml = project_dir.join("Cargo.toml");
    if cfg!(debug_assertions) && cargo_toml.exists() {
        Command::new("cargo")
            .args(["run", "--offline", "--bin", "model-manager"])
            .current_dir(project_dir)
            .spawn()
            .context("以 cargo 启动 model-manager 失败")?;
        return Ok(());
    }

    let manager_bin = bin_dir.join("model-manager");
    if manager_bin.exists() {
        Command::new(manager_bin)
            .spawn()
            .context("启动 model-manager 失败")?;
        return Ok(());
    }

    if cargo_toml.exists() {
        Command::new("cargo")
            .args(["run", "--offline", "--bin", "model-manager"])
            .current_dir(project_dir)
            .spawn()
            .context("以 cargo 启动 model-manager 失败")?;
        return Ok(());
    }

    bail!("未找到 model-manager 可执行文件");
}

unsafe fn install_status_item(app: id) -> Result<(StatusHandle, MonitorHandle, id, id, id)> {
    let status_bar = NSStatusBar::systemStatusBar(nil);
    if status_bar == nil {
        bail!("无法创建 NSStatusBar");
    }

    let status_item = status_bar.statusItemWithLength_(NSVariableStatusItemLength);
    if status_item == nil {
        bail!("无法创建 status item");
    }

    let button = status_item.button();
    if button == nil {
        bail!("status item 无按钮");
    }
    NSButton::setTitle_(button, ns_string(TrayState::Idle.title()));
    set_status_button_symbol(button, TrayState::Idle.symbol_name());

    let menu = NSMenu::new(nil).autorelease();
    let menu_handler = new_menu_handler();
    let state_item = make_info_item("状态: 就绪", menu_handler);
    let asr_item = make_info_item("识别: -", menu_handler);
    let output_item = make_info_item("发送: -", menu_handler);
    let hint_item = make_info_item("提示: -", menu_handler);

    menu.addItem_(state_item);
    menu.addItem_(asr_item);
    menu.addItem_(output_item);
    menu.addItem_(hint_item);
    menu.addItem_(NSMenuItem::separatorItem(nil));

    let settings_item = NSMenuItem::alloc(nil)
        .initWithTitle_action_keyEquivalent_(
            ns_string("设置..."),
            sel!(openModelManager:),
            ns_string("s"),
        )
        .autorelease();
    NSMenuItem::setTarget_(settings_item, menu_handler);
    menu.addItem_(settings_item);

    menu.addItem_(NSMenuItem::separatorItem(nil));

    let quit_item = NSMenuItem::alloc(nil)
        .initWithTitle_action_keyEquivalent_(ns_string("退出"), sel!(terminate:), ns_string("q"))
        .autorelease();
    NSMenuItem::setTarget_(quit_item, app);
    menu.addItem_(quit_item);
    status_item.setMenu_(menu);

    Ok((
        StatusHandle {
            button_ptr: button as usize,
        },
        MonitorHandle {
            state_item_ptr: state_item as usize,
            asr_item_ptr: asr_item as usize,
            output_item_ptr: output_item as usize,
            hint_item_ptr: hint_item as usize,
        },
        status_item,
        menu,
        menu_handler,
    ))
}

const OVERLAY_WIDTH: f64 = 560.0;
const OVERLAY_HEIGHT: f64 = 50.0;
const OVERLAY_BOTTOM_MARGIN: f64 = 24.0;
const OVERLAY_TOP_MARGIN: f64 = 24.0;
const OVERLAY_SWITCH_DISTANCE: f64 = 210.0;
const OVERLAY_STATUS_BADGE_WIDTH: f64 = 92.0;
const OVERLAY_STATUS_BADGE_HEIGHT: f64 = 33.0;
const OVERLAY_PREVIEW_MAX_LINES: usize = 6;
const OVERLAY_PREVIEW_LINE_HEIGHT: f64 = 17.0;
const OVERLAY_PREVIEW_MIN_HEIGHT: f64 = 20.0;
const OVERLAY_PREVIEW_LINE_CAP: f32 = 24.0;
const OVERLAY_MAX_HEIGHT: f64 = 158.0;
const ASR_PREVIEW_HOLD_MS: u64 = 900;
const RESULT_OVERLAY_HOLD_MS: u64 = 950;
const OVERLAY_FADE_TOTAL_MS: u64 = 120;
const OVERLAY_FADE_STEPS: u64 = 4;
const SILENCE_RMS_THRESHOLD: f32 = 0.0035;

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct AxPoint {
    x: f64,
    y: f64,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct AxSize {
    width: f64,
    height: f64,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct AxRect {
    origin: AxPoint,
    size: AxSize,
}

type AXValueRef = *const c_void;
type AXValueType = u32;
const K_AX_VALUE_CGRECT_TYPE: AXValueType = 3;

unsafe fn visible_frame() -> NSRect {
    let screen: id = msg_send![class!(NSScreen), mainScreen];
    if screen != nil {
        let frame: NSRect = msg_send![screen, visibleFrame];
        frame
    } else {
        NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(1440.0, 900.0))
    }
}

fn clamp_overlay_origin(
    mut x: f64,
    mut y: f64,
    width: f64,
    height: f64,
    frame: NSRect,
) -> (f64, f64) {
    let min_x = frame.origin.x + 6.0;
    let max_x = frame.origin.x + frame.size.width - width - 6.0;
    let min_y = frame.origin.y + 6.0;
    let max_y = frame.origin.y + frame.size.height - height - 6.0;

    if x < min_x {
        x = min_x;
    } else if x > max_x {
        x = max_x;
    }

    if y < min_y {
        y = min_y;
    } else if y > max_y {
        y = max_y;
    }

    (x, y)
}

fn point_in_frame(p: NSPoint, frame: NSRect) -> bool {
    p.x >= frame.origin.x
        && p.x <= frame.origin.x + frame.size.width
        && p.y >= frame.origin.y
        && p.y <= frame.origin.y + frame.size.height
}

fn is_cjk_char(ch: char) -> bool {
    matches!(
        ch as u32,
        0x4E00..=0x9FFF
            | 0x3400..=0x4DBF
            | 0x3000..=0x303F
            | 0x3040..=0x309F
            | 0x30A0..=0x30FF
            | 0xAC00..=0xD7AF
    )
}

fn preview_char_unit(ch: char) -> f32 {
    if ch.is_ascii_alphabetic() || ch.is_ascii_digit() {
        0.58
    } else if ch.is_ascii_punctuation() {
        0.42
    } else if is_cjk_char(ch) {
        1.0
    } else {
        0.72
    }
}

fn wrap_preview_text(raw: &str) -> String {
    let text = raw.replace('\r', "");
    if text.trim().is_empty() {
        return String::new();
    }

    let mut lines: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut width_units = 0.0f32;
    let mut truncated = false;

    for ch in text.chars() {
        let ch = if ch == '\t' { ' ' } else { ch };
        if ch == '\n' {
            lines.push(current);
            current = String::new();
            width_units = 0.0;
            if lines.len() >= OVERLAY_PREVIEW_MAX_LINES {
                truncated = true;
                break;
            }
            continue;
        }

        let unit = preview_char_unit(ch);
        if width_units + unit > OVERLAY_PREVIEW_LINE_CAP {
            lines.push(current);
            current = String::new();
            width_units = 0.0;
            if lines.len() >= OVERLAY_PREVIEW_MAX_LINES {
                truncated = true;
                break;
            }
        }

        current.push(ch);
        width_units += unit;
    }

    if !current.is_empty() && lines.len() < OVERLAY_PREVIEW_MAX_LINES {
        lines.push(current);
    } else if !current.is_empty() {
        truncated = true;
    }

    if lines.is_empty() {
        lines.push(String::new());
    }

    if truncated {
        if let Some(last) = lines.last_mut() {
            if !last.ends_with('…') {
                last.push('…');
            }
        }
    }

    lines.join("\n")
}

fn estimate_preview_lines(text: &str) -> usize {
    let cnt = text.lines().count();
    cnt.max(1).min(OVERLAY_PREVIEW_MAX_LINES)
}

unsafe fn layout_overlay_window(
    window: id,
    status_label: id,
    preview_label: id,
    preview_text: &str,
) {
    let lines = estimate_preview_lines(preview_text);
    let preview_h = (OVERLAY_PREVIEW_LINE_HEIGHT * lines as f64).max(OVERLAY_PREVIEW_MIN_HEIGHT);
    let mut total_h = (preview_h + 18.0).max(OVERLAY_HEIGHT);
    if total_h > OVERLAY_MAX_HEIGHT {
        total_h = OVERLAY_MAX_HEIGHT;
    }

    let status_h = OVERLAY_STATUS_BADGE_HEIGHT;
    let status_w = OVERLAY_STATUS_BADGE_WIDTH;
    let preview_x = status_w + 16.0;
    let preview_w = OVERLAY_WIDTH - preview_x - 10.0;
    let status_y = ((total_h - status_h) * 0.5).floor();
    let preview_y = ((total_h - preview_h) * 0.5).floor();
    let status_frame = NSRect::new(
        NSPoint::new(10.0, status_y),
        NSSize::new(status_w, status_h),
    );
    let preview_frame = NSRect::new(
        NSPoint::new(preview_x, preview_y),
        NSSize::new(preview_w, preview_h),
    );
    let _: () = msg_send![status_label, setFrame: status_frame];
    let _: () = msg_send![preview_label, setFrame: preview_frame];

    let current_frame: NSRect = msg_send![window, frame];
    if (current_frame.size.height - total_h).abs() > 0.5 {
        let resized = NSRect::new(current_frame.origin, NSSize::new(OVERLAY_WIDTH, total_h));
        let _: () = msg_send![window, setFrame: resized display: NO];
    }
}

unsafe fn focused_caret_rect() -> Option<AxRect> {
    if AXIsProcessTrusted() == 0 {
        return None;
    }

    let system = AXUIElementCreateSystemWide();
    if system.is_null() {
        return None;
    }

    let focused_attr = CFString::new("AXFocusedUIElement");
    let mut focused_val: core_foundation_sys::base::CFTypeRef = std::ptr::null();
    let copy_err =
        AXUIElementCopyAttributeValue(system, focused_attr.as_concrete_TypeRef(), &mut focused_val);
    CFRelease(system as core_foundation_sys::base::CFTypeRef);

    if copy_err != 0 || focused_val.is_null() {
        return None;
    }

    let focused = focused_val as AXUIElementRef;
    let range_attr = CFString::new("AXSelectedTextRange");
    let mut range_val: core_foundation_sys::base::CFTypeRef = std::ptr::null();
    let range_err =
        AXUIElementCopyAttributeValue(focused, range_attr.as_concrete_TypeRef(), &mut range_val);
    if range_err != 0 || range_val.is_null() {
        CFRelease(focused as core_foundation_sys::base::CFTypeRef);
        return None;
    }

    let bounds_attr = CFString::new("AXBoundsForRange");
    let mut bounds_val: core_foundation_sys::base::CFTypeRef = std::ptr::null();
    let bounds_err = AXUIElementCopyParameterizedAttributeValue(
        focused,
        bounds_attr.as_concrete_TypeRef(),
        range_val,
        &mut bounds_val,
    );
    CFRelease(range_val);
    CFRelease(focused as core_foundation_sys::base::CFTypeRef);

    if bounds_err != 0 || bounds_val.is_null() {
        return None;
    }

    let ax_value = bounds_val as AXValueRef;
    if AXValueGetType(ax_value) != K_AX_VALUE_CGRECT_TYPE {
        CFRelease(bounds_val);
        return None;
    }

    let mut rect = AxRect::default();
    let ok = AXValueGetValue(
        ax_value,
        K_AX_VALUE_CGRECT_TYPE,
        &mut rect as *mut _ as *mut c_void,
    );
    CFRelease(bounds_val);

    if ok == 0 {
        None
    } else {
        Some(rect)
    }
}

fn pick_focus_point(frame: NSRect, mouse: NSPoint, caret: AxRect) -> Option<NSPoint> {
    let center_x = caret.origin.x + caret.size.width * 0.5;
    let y_bottom_origin = caret.origin.y + caret.size.height * 0.5;
    let y_top_origin =
        frame.origin.y + frame.size.height - caret.origin.y - caret.size.height * 0.5;

    let candidates = [
        NSPoint::new(center_x, y_bottom_origin),
        NSPoint::new(center_x, y_top_origin),
    ];

    let mut best: Option<(f64, NSPoint)> = None;
    for point in candidates {
        if !point_in_frame(point, frame) {
            continue;
        }
        let dist2 = (point.x - mouse.x).powi(2) + (point.y - mouse.y).powi(2);
        match best {
            None => best = Some((dist2, point)),
            Some((curr, _)) if dist2 < curr => best = Some((dist2, point)),
            _ => {}
        }
    }
    best.map(|(_, p)| p)
}

unsafe fn position_overlay_window(window: id) {
    let frame = visible_frame();
    let window_frame = NSWindow::frame(window);
    let width = window_frame.size.width;
    let height = window_frame.size.height;
    let x = frame.origin.x + (frame.size.width - width) * 0.5;
    let bottom_y = frame.origin.y + OVERLAY_BOTTOM_MARGIN;
    let top_y = frame.origin.y + frame.size.height - height - OVERLAY_TOP_MARGIN;
    let bottom_center = NSPoint::new(x + width * 0.5, bottom_y + height * 0.5);
    let mouse: NSPoint = msg_send![class!(NSEvent), mouseLocation];
    let focus = if let Some(caret) = focused_caret_rect() {
        pick_focus_point(frame, mouse, caret)
    } else if point_in_frame(mouse, frame) {
        Some(mouse)
    } else {
        None
    };
    let y = if let Some(p) = focus {
        let dx = p.x - bottom_center.x;
        let dy = p.y - bottom_center.y;
        if dx * dx + dy * dy <= OVERLAY_SWITCH_DISTANCE * OVERLAY_SWITCH_DISTANCE {
            top_y
        } else {
            bottom_y
        }
    } else {
        bottom_y
    };
    let (x, y) = clamp_overlay_origin(x, y, width, height, frame);
    window.setFrameOrigin_(NSPoint::new(x, y));
}

unsafe fn install_overlay() -> Result<OverlayHandle> {
    let frame = visible_frame();
    let width = OVERLAY_WIDTH;
    let height = OVERLAY_HEIGHT;
    let x = frame.origin.x + (frame.size.width - width) / 2.0;
    let y = frame.origin.y + OVERLAY_BOTTOM_MARGIN;
    let rect = NSRect::new(NSPoint::new(x, y), NSSize::new(width, height));

    let window = NSWindow::alloc(nil).initWithContentRect_styleMask_backing_defer_(
        rect,
        NSWindowStyleMask::NSBorderlessWindowMask,
        NSBackingStoreBuffered,
        NO,
    );
    if window == nil {
        bail!("无法创建浮层窗口");
    }

    let clear_color: id = msg_send![class!(NSColor), clearColor];
    window.setBackgroundColor_(clear_color);
    window.setOpaque_(NO);
    window.setHasShadow_(YES);
    window.setIgnoresMouseEvents_(YES);
    window.setHidesOnDeactivate_(NO);
    window.setLevel_((NSMainMenuWindowLevel + 1) as i64);
    window.setCollectionBehavior_(
        NSWindowCollectionBehavior::NSWindowCollectionBehaviorCanJoinAllSpaces
            | NSWindowCollectionBehavior::NSWindowCollectionBehaviorTransient,
    );
    let _: () = msg_send![window, setReleasedWhenClosed: NO];
    let _: () = msg_send![window, setMovableByWindowBackground: NO];

    let content = window.contentView();
    if content == nil {
        bail!("浮层 contentView 为空");
    }
    let _: () = msg_send![content, setWantsLayer: YES];
    let content_layer: id = msg_send![content, layer];
    if content_layer != nil {
        let content_bg: id = msg_send![
            class!(NSColor),
            colorWithCalibratedWhite: 0.16f64
            alpha: 0.93f64
        ];
        let content_border: id = msg_send![
            class!(NSColor),
            colorWithCalibratedWhite: 0.44f64
            alpha: 0.34f64
        ];
        let content_bg_cg: id = msg_send![content_bg, CGColor];
        let content_border_cg: id = msg_send![content_border, CGColor];
        let _: () = msg_send![content_layer, setCornerRadius: 15.0f64];
        let _: () = msg_send![content_layer, setMasksToBounds: YES];
        let _: () = msg_send![content_layer, setBackgroundColor: content_bg_cg];
        let _: () = msg_send![content_layer, setBorderWidth: 1.0f64];
        let _: () = msg_send![content_layer, setBorderColor: content_border_cg];
    }

    let status_label = NSTextField::initWithFrame_(
        NSTextField::alloc(nil),
        NSRect::new(
            NSPoint::new(8.0, (OVERLAY_HEIGHT - OVERLAY_STATUS_BADGE_HEIGHT) * 0.5),
            NSSize::new(OVERLAY_STATUS_BADGE_WIDTH, OVERLAY_STATUS_BADGE_HEIGHT),
        ),
    );
    let _: () = msg_send![status_label, setEditable: NO];
    let _: () = msg_send![status_label, setSelectable: NO];
    let _: () = msg_send![status_label, setBezeled: NO];
    let _: () = msg_send![status_label, setBordered: NO];
    let _: () = msg_send![status_label, setDrawsBackground: NO];
    let _: () = msg_send![status_label, setAlignment: 2usize];
    let status_font: id = msg_send![class!(NSFont), boldSystemFontOfSize: 13.0f64];
    let _: () = msg_send![status_label, setFont: status_font];
    let status_color: id = msg_send![class!(NSColor), whiteColor];
    let _: () = msg_send![status_label, setTextColor: status_color];
    let status_cell: id = msg_send![status_label, cell];
    if status_cell != nil {
        let _: () = msg_send![status_cell, setWraps: NO];
        let _: () = msg_send![status_cell, setScrollable: NO];
        let _: () = msg_send![status_cell, setUsesSingleLineMode: YES];
        let _: () = msg_send![status_cell, setLineBreakMode: 4usize];
        let _: () = msg_send![status_cell, setAlignment: 2usize];
    }
    let _: () = msg_send![status_label, setWantsLayer: YES];
    let status_layer: id = msg_send![status_label, layer];
    if status_layer != nil {
        let _: () = msg_send![
            status_layer,
            setCornerRadius: (OVERLAY_STATUS_BADGE_HEIGHT * 0.5).floor()
        ];
        let _: () = msg_send![status_layer, setMasksToBounds: YES];
    }
    let _: () = msg_send![status_label, setStringValue: ns_string("就绪")];
    set_status_badge_appearance(status_label, "就绪");
    content.addSubview_(status_label);

    let preview_label = NSTextField::initWithFrame_(
        NSTextField::alloc(nil),
        NSRect::new(NSPoint::new(108.0, 15.0), NSSize::new(442.0, 20.0)),
    );
    let _: () = msg_send![preview_label, setEditable: NO];
    let _: () = msg_send![preview_label, setSelectable: NO];
    let _: () = msg_send![preview_label, setBezeled: NO];
    let _: () = msg_send![preview_label, setBordered: NO];
    let _: () = msg_send![preview_label, setDrawsBackground: NO];
    let _: () = msg_send![preview_label, setAlignment: 0usize];
    let preview_font: id = msg_send![class!(NSFont), systemFontOfSize: 15.0f64];
    let _: () = msg_send![preview_label, setFont: preview_font];
    let preview_color: id = msg_send![
        class!(NSColor),
        colorWithCalibratedRed: 0.94f64
        green: 0.91f64
        blue: 0.78f64
        alpha: 1.0f64
    ];
    let _: () = msg_send![preview_label, setTextColor: preview_color];
    let cell: id = msg_send![preview_label, cell];
    if cell != nil {
        let _: () = msg_send![cell, setWraps: YES];
        let _: () = msg_send![cell, setScrollable: NO];
        let _: () = msg_send![cell, setUsesSingleLineMode: NO];
        let _: () = msg_send![cell, setLineBreakMode: 0usize];
    }
    let _: () = msg_send![preview_label, setStringValue: ns_string("按住快捷键说话")];
    content.addSubview_(preview_label);

    window.orderOut_(nil);

    Ok(OverlayHandle {
        window_ptr: window as usize,
        status_label_ptr: status_label as usize,
        preview_label_ptr: preview_label as usize,
    })
}

unsafe fn ns_string(s: &str) -> id {
    NSString::alloc(nil).init_str(s).autorelease()
}

unsafe fn set_status_badge_appearance(status_label: id, status: &str) {
    if status_label == nil {
        return;
    }
    let (r, g, b) = if status.contains("录音") {
        (0.20, 0.44, 0.95)
    } else if status.contains("转录") || status.contains("识别") {
        (0.35, 0.37, 0.44)
    } else if status.contains("润色") {
        (0.56, 0.43, 0.16)
    } else if status.contains("发送") || status.contains("注入") || status.contains("就绪") {
        (0.19, 0.42, 0.86)
    } else {
        (0.58, 0.24, 0.24)
    };
    let badge_bg: id = msg_send![
        class!(NSColor),
        colorWithCalibratedRed: r
        green: g
        blue: b
        alpha: 1.0f64
    ];
    let badge_bg_cg: id = msg_send![badge_bg, CGColor];
    let status_layer: id = msg_send![status_label, layer];
    if status_layer != nil {
        let bounds: NSRect = msg_send![status_label, bounds];
        let _: () = msg_send![
            status_layer,
            setCornerRadius: (bounds.size.height * 0.5).floor()
        ];
        let _: () = msg_send![status_layer, setMasksToBounds: YES];
        let _: () = msg_send![status_layer, setBackgroundColor: badge_bg_cg];
    }
}

unsafe fn set_status_button_symbol(button: id, symbol_name: &str) {
    let image: id = msg_send![
        class!(NSImage),
        imageWithSystemSymbolName: ns_string(symbol_name)
        accessibilityDescription: nil
    ];
    if image != nil {
        let _: () = msg_send![image, setTemplate: YES];
        NSButton::setImage_(button, image);
    }
}

#[derive(Debug, Clone, Copy)]
enum HotkeySignal {
    Down,
    Up,
}

struct HotkeyGuard {
    _tap: CGEventTap<'static>,
    _source: CFRunLoopSource,
}

fn event_flags_to_hotkey_modifiers(flags: CGEventFlags) -> u8 {
    let mut modifiers = 0u8;
    if flags.contains(CGEventFlags::CGEventFlagCommand) {
        modifiers |= HOTKEY_MOD_CMD;
    }
    if flags.contains(CGEventFlags::CGEventFlagControl) {
        modifiers |= HOTKEY_MOD_CTRL;
    }
    if flags.contains(CGEventFlags::CGEventFlagAlternate) {
        modifiers |= HOTKEY_MOD_ALT;
    }
    if flags.contains(CGEventFlags::CGEventFlagShift) {
        modifiers |= HOTKEY_MOD_SHIFT;
    }
    modifiers
}

fn install_hotkey_tap(
    tx: Sender<HotkeySignal>,
    hotkey_store: Arc<std::sync::atomic::AtomicUsize>,
) -> Result<HotkeyGuard> {
    let fn_pressed = Arc::new(AtomicBool::new(false));
    let fn_pressed_cb = Arc::clone(&fn_pressed);
    let combo_pressed = Arc::new(AtomicBool::new(false));
    let combo_pressed_cb = Arc::clone(&combo_pressed);

    let tap = CGEventTap::new(
        CGEventTapLocation::Session,
        CGEventTapPlacement::HeadInsertEventTap,
        CGEventTapOptions::ListenOnly,
        vec![
            CGEventType::FlagsChanged,
            CGEventType::KeyDown,
            CGEventType::KeyUp,
        ],
        move |_proxy, event_type, event| {
            let hotkey = HotkeySpec::unpack(hotkey_store.load(Ordering::SeqCst));
            match event_type {
                CGEventType::FlagsChanged => {
                    if hotkey.is_fn() {
                        combo_pressed_cb.store(false, Ordering::SeqCst);
                        // Fn / Globe key is exposed as SecondaryFn modifier flag on macOS.
                        let is_fn_now = event
                            .get_flags()
                            .contains(CGEventFlags::CGEventFlagSecondaryFn);
                        let was_fn = fn_pressed_cb.swap(is_fn_now, Ordering::SeqCst);
                        if is_fn_now && !was_fn {
                            let _ = tx.send(HotkeySignal::Down);
                        } else if !is_fn_now && was_fn {
                            let _ = tx.send(HotkeySignal::Up);
                        }
                        return None;
                    }

                    fn_pressed_cb.store(false, Ordering::SeqCst);
                    if combo_pressed_cb.load(Ordering::SeqCst) {
                        let modifiers = event_flags_to_hotkey_modifiers(event.get_flags());
                        if modifiers != hotkey.modifiers {
                            combo_pressed_cb.store(false, Ordering::SeqCst);
                            let _ = tx.send(HotkeySignal::Up);
                        }
                    }
                }
                CGEventType::KeyDown => {
                    if hotkey.is_fn() {
                        return None;
                    }
                    let keycode =
                        event.get_integer_value_field(EventField::KEYBOARD_EVENT_KEYCODE) as u16;
                    if keycode != hotkey.keycode {
                        return None;
                    }
                    let modifiers = event_flags_to_hotkey_modifiers(event.get_flags());
                    if modifiers != hotkey.modifiers {
                        return None;
                    }
                    let is_repeat =
                        event.get_integer_value_field(EventField::KEYBOARD_EVENT_AUTOREPEAT);
                    if is_repeat == 0 && !combo_pressed_cb.swap(true, Ordering::SeqCst) {
                        let _ = tx.send(HotkeySignal::Down);
                    }
                }
                CGEventType::KeyUp => {
                    if hotkey.is_fn() {
                        return None;
                    }
                    let keycode =
                        event.get_integer_value_field(EventField::KEYBOARD_EVENT_KEYCODE) as u16;
                    if keycode == hotkey.keycode && combo_pressed_cb.swap(false, Ordering::SeqCst) {
                        let _ = tx.send(HotkeySignal::Up);
                    }
                }
                _ => {}
            }
            None
        },
    )
    .map_err(|_| anyhow!("创建 CGEventTap 失败；请检查输入监控权限"))?;

    let source = tap
        .mach_port
        .create_runloop_source(0)
        .map_err(|_| anyhow!("创建 event tap runloop source 失败"))?;

    let runloop = CFRunLoop::get_current();
    unsafe {
        runloop.add_source(&source, kCFRunLoopCommonModes);
    }
    tap.enable();

    Ok(HotkeyGuard {
        _tap: tap,
        _source: source,
    })
}

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

fn model_base_dir() -> PathBuf {
    dirs::home_dir()
        .map(|h| h.join(".mofa/models"))
        .unwrap_or_else(|| PathBuf::from("./models"))
}

fn choose_llm_model(base: &Path, choice: LlmModelChoice) -> Option<PathBuf> {
    if let Some(file_name) = choice.file_name() {
        let selected = base.join(file_name);
        if selected.exists() {
            return Some(selected);
        }
    }
    choose_llm_model_auto(base)
}

fn choose_llm_model_auto(base: &Path) -> Option<PathBuf> {
    let mem_gb = total_memory_gb().unwrap_or(32);

    let preferred = if mem_gb <= 8 {
        "qwen2.5-0.5b-q4_k_m.gguf"
    } else if mem_gb <= 16 {
        "qwen2.5-1.5b-q4_k_m.gguf"
    } else {
        "qwen2.5-3b-q4_k_m.gguf"
    };

    let mut candidates = vec![
        preferred,
        "qwen2.5-1.5b-q4_k_m.gguf",
        "qwen2.5-0.5b-q4_k_m.gguf",
        "qwen2.5-3b-q4_k_m.gguf",
        "qwen2.5-7b-q4_k_m.gguf",
    ];
    candidates.dedup();

    candidates
        .into_iter()
        .map(|name| base.join(name))
        .find(|p| p.exists())
}

fn choose_asr_model(base: &Path, choice: AsrModelChoice) -> Option<PathBuf> {
    if let Some(file_name) = choice.file_name() {
        let selected = base.join(file_name);
        if selected.exists() {
            return Some(selected);
        }
    }
    choose_asr_model_auto(base)
}

fn choose_asr_model_auto(base: &Path) -> Option<PathBuf> {
    [
        "ggml-small.bin",
        "ggml-base.bin",
        "ggml-tiny.bin",
        "ggml-medium.bin",
    ]
    .into_iter()
    .map(|name| base.join(name))
    .find(|p| p.exists())
}

fn normalize_transcript(text: &str) -> String {
    let mut out = String::new();
    let mut prev_space = false;
    for ch in text.trim().chars() {
        if ch.is_whitespace() {
            if !prev_space {
                out.push(' ');
            }
            prev_space = true;
        } else {
            out.push(ch);
            prev_space = false;
        }
    }
    out.trim().to_string()
}

fn compact_for_filter(text: &str) -> String {
    text.chars()
        .filter(|c| {
            if c.is_whitespace() {
                return false;
            }
            !matches!(
                c,
                '，' | '。'
                    | '！'
                    | '？'
                    | '；'
                    | '：'
                    | '、'
                    | '（'
                    | '）'
                    | '【'
                    | '】'
                    | '.'
                    | ','
                    | '!'
                    | '?'
                    | ';'
                    | ':'
                    | '('
                    | ')'
                    | '['
                    | ']'
                    | '"'
                    | '\''
                    | '“'
                    | '”'
                    | '…'
            )
        })
        .collect::<String>()
        .to_ascii_lowercase()
}

fn is_template_noise_text(text: &str) -> bool {
    let compact = compact_for_filter(text);
    if compact.is_empty() {
        return true;
    }
    const PATTERNS: [&str; 11] = [
        "好的请提供需要转写和润色的语音内容",
        "请提供需要转写和润色的语音内容",
        "请提供需要转写的语音内容",
        "请提供语音内容",
        "未检测到有效语音",
        "未识别到有效语音",
        "未识别到语音",
        "pleaseprovidevoiceinput",
        "pleaseprovidetheaudiocontent",
        "pleaseprovidevoicetotranscribe",
        "novalidaudio",
    ];
    PATTERNS.iter().any(|p| compact.contains(p))
}

fn should_drop_transcript(text: &str) -> bool {
    let normalized = normalize_transcript(text);
    if normalized.is_empty() {
        return true;
    }
    if is_template_noise_text(&normalized) {
        return true;
    }
    let compact = compact_for_filter(&normalized);
    compact.chars().count() <= 1
}

fn audio_rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let mean_square = samples
        .iter()
        .map(|v| {
            let x = *v as f64;
            x * x
        })
        .sum::<f64>()
        / samples.len() as f64;
    mean_square.sqrt() as f32
}

fn english_char_ratio(text: &str) -> f32 {
    let mut latin = 0usize;
    let mut total = 0usize;
    for ch in text.chars() {
        if ch.is_ascii_alphabetic() {
            latin += 1;
            total += 1;
        } else if ('\u{4e00}'..='\u{9fff}').contains(&ch) {
            total += 1;
        }
    }
    if total == 0 {
        0.0
    } else {
        latin as f32 / total as f32
    }
}

fn build_refine_prompt(raw_text: &str) -> String {
    if english_char_ratio(raw_text) >= 0.7 {
        format!(
            "You are an input-method text polisher. Rewrite the ASR text into natural, concise English ready to send.\n\
Rules:\n\
1) Keep the original meaning and key facts.\n\
2) Remove fillers, stutters, and duplicate fragments.\n\
3) Keep names, numbers, code terms, and URLs unchanged.\n\
4) Do not translate into Chinese.\n\
5) Do not explain, do not add commentary, and do not ask follow-up questions.\n\
6) If content is empty/invalid, output an empty string.\n\
Output only the final text.\n\n{}",
            raw_text
        )
    } else {
        format!(
            "你是输入法润色器。将 ASR 文本整理为可直接发送的自然中文。\n\
规则：\n\
1) 保留原意与事实，不新增信息；\n\
2) 删除口癖、重复、卡顿；\n\
3) 专名、数字、代码、URL 原样保留；\n\
4) 不解释、不寒暄、不提问；\n\
5) 若内容为空或无效，仅输出空字符串；\n\
6) 只输出最终文本。\n\n{}",
            raw_text
        )
    }
}

fn total_memory_gb() -> Option<u64> {
    let name = CString::new("hw.memsize").ok()?;
    let mut value: u64 = 0;
    let mut size = std::mem::size_of::<u64>();
    let ret = unsafe {
        libc::sysctlbyname(
            name.as_ptr(),
            &mut value as *mut _ as *mut c_void,
            &mut size,
            std::ptr::null_mut(),
            0,
        )
    };
    if ret == 0 {
        Some(value / 1024 / 1024 / 1024)
    } else {
        None
    }
}

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

fn inject_text(text: &str) -> Result<()> {
    if text.trim().is_empty() {
        return Ok(());
    }

    let payload = text.to_string();
    Queue::main().exec_sync(move || unsafe {
        let _pool = NSAutoreleasePool::new(nil);

        // 优先 AX，尽量直接写入焦点控件。
        if try_insert_via_ax(&payload).is_ok() {
            return Ok(());
        }

        // 剪贴板粘贴重试两次，提升兼容性。
        for _ in 0..2 {
            if paste_via_clipboard(&payload).is_ok() {
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(90));
        }

        // 最后兜底：直接发送 Unicode 键盘事件。
        type_text_via_events(&payload)?;
        Ok(())
    })
}

type AXUIElementRef = *const c_void;
type AXError = i32;

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn AXIsProcessTrusted() -> core_foundation_sys::base::Boolean;
    fn AXUIElementCreateSystemWide() -> AXUIElementRef;
    fn AXUIElementCopyAttributeValue(
        element: AXUIElementRef,
        attribute: core_foundation_sys::string::CFStringRef,
        value: *mut core_foundation_sys::base::CFTypeRef,
    ) -> AXError;
    fn AXUIElementSetAttributeValue(
        element: AXUIElementRef,
        attribute: core_foundation_sys::string::CFStringRef,
        value: core_foundation_sys::base::CFTypeRef,
    ) -> AXError;
    fn AXUIElementCopyParameterizedAttributeValue(
        element: AXUIElementRef,
        parameterized_attribute: core_foundation_sys::string::CFStringRef,
        parameter: core_foundation_sys::base::CFTypeRef,
        value: *mut core_foundation_sys::base::CFTypeRef,
    ) -> AXError;
    fn AXValueGetType(value: AXValueRef) -> AXValueType;
    fn AXValueGetValue(
        value: AXValueRef,
        value_type: AXValueType,
        value_ptr: *mut c_void,
    ) -> core_foundation_sys::base::Boolean;
}

fn try_insert_via_ax(text: &str) -> Result<()> {
    unsafe {
        if AXIsProcessTrusted() == 0 {
            bail!("Accessibility 未授权");
        }

        let system = AXUIElementCreateSystemWide();
        if system.is_null() {
            bail!("无法创建 AX system element");
        }

        let focused_attr = CFString::new("AXFocusedUIElement");
        let mut focused_val: core_foundation_sys::base::CFTypeRef = std::ptr::null();
        let copy_err = AXUIElementCopyAttributeValue(
            system,
            focused_attr.as_concrete_TypeRef(),
            &mut focused_val,
        );
        CFRelease(system as core_foundation_sys::base::CFTypeRef);

        if copy_err != 0 || focused_val.is_null() {
            bail!("无法获取焦点元素: {copy_err}");
        }

        let focused = focused_val as AXUIElementRef;

        // Strategy A: replace selected text directly.
        let selected_attr = CFString::new("AXSelectedText");
        let text_cf = CFString::new(text);
        let set_selected_err = AXUIElementSetAttributeValue(
            focused,
            selected_attr.as_concrete_TypeRef(),
            text_cf.as_CFTypeRef(),
        );
        if set_selected_err == 0 {
            CFRelease(focused as core_foundation_sys::base::CFTypeRef);
            return Ok(());
        }

        // Strategy B: fallback to AXValue append.
        let value_attr = CFString::new("AXValue");
        let mut value_ref: core_foundation_sys::base::CFTypeRef = std::ptr::null();
        let get_val_err = AXUIElementCopyAttributeValue(
            focused,
            value_attr.as_concrete_TypeRef(),
            &mut value_ref,
        );

        if get_val_err == 0 && !value_ref.is_null() {
            let value_cf = CFType::wrap_under_create_rule(value_ref);
            if let Some(current) = value_cf.downcast::<CFString>() {
                let merged = format!("{}{}", current, text);
                let merged_cf = CFString::new(&merged);
                let set_val_err = AXUIElementSetAttributeValue(
                    focused,
                    value_attr.as_concrete_TypeRef(),
                    merged_cf.as_CFTypeRef(),
                );
                CFRelease(focused as core_foundation_sys::base::CFTypeRef);
                if set_val_err == 0 {
                    return Ok(());
                }
                bail!("AXValue 写入失败: {set_val_err}");
            }
        }

        CFRelease(focused as core_foundation_sys::base::CFTypeRef);
        bail!("AX 注入失败")
    }
}

fn paste_via_clipboard(text: &str) -> Result<()> {
    unsafe {
        let pboard: id = NSPasteboard::generalPasteboard(nil);
        if pboard == nil {
            bail!("无法获取 NSPasteboard");
        }

        let old_obj: id = pboard.stringForType(NSPasteboardTypeString);
        let old_text = nsstring_to_rust(old_obj);

        pboard.clearContents();
        let new_text = NSString::alloc(nil).init_str(text).autorelease();
        let ok = pboard.setString_forType(new_text, NSPasteboardTypeString);
        if !ok {
            bail!("写入剪贴板失败");
        }

        post_cmd_v()?;

        std::thread::sleep(Duration::from_millis(260));

        // Restore clipboard
        pboard.clearContents();
        if let Some(old) = old_text {
            let old_ns = NSString::alloc(nil).init_str(&old).autorelease();
            let _ = pboard.setString_forType(old_ns, NSPasteboardTypeString);
        }

        Ok(())
    }
}

fn type_text_via_events(text: &str) -> Result<()> {
    let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState)
        .map_err(|_| anyhow!("创建 CGEventSource 失败"))?;

    let key_down = CGEvent::new_keyboard_event(source.clone(), 0, true)
        .map_err(|_| anyhow!("创建文本事件失败"))?;
    key_down.set_string(text);
    key_down.post(CGEventTapLocation::HID);

    let key_up =
        CGEvent::new_keyboard_event(source, 0, false).map_err(|_| anyhow!("创建文本事件失败"))?;
    key_up.set_string(text);
    key_up.post(CGEventTapLocation::HID);

    Ok(())
}

unsafe fn nsstring_to_rust(s: id) -> Option<String> {
    if s == nil {
        return None;
    }
    let ptr = s.UTF8String();
    if ptr.is_null() {
        return None;
    }
    Some(CStr::from_ptr(ptr).to_string_lossy().into_owned())
}

fn post_cmd_v() -> Result<()> {
    const KEY_V: CGKeyCode = 0x09;

    let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState)
        .map_err(|_| anyhow!("创建 CGEventSource 失败"))?;

    let cmd_down = CGEvent::new_keyboard_event(source.clone(), KeyCode::COMMAND, true)
        .map_err(|_| anyhow!("创建 cmd down 失败"))?;
    cmd_down.post(CGEventTapLocation::HID);

    let v_down = CGEvent::new_keyboard_event(source.clone(), KEY_V, true)
        .map_err(|_| anyhow!("创建 v down 失败"))?;
    v_down.set_flags(CGEventFlags::CGEventFlagCommand);
    v_down.post(CGEventTapLocation::HID);

    let v_up = CGEvent::new_keyboard_event(source.clone(), KEY_V, false)
        .map_err(|_| anyhow!("创建 v up 失败"))?;
    v_up.set_flags(CGEventFlags::CGEventFlagCommand);
    v_up.post(CGEventTapLocation::HID);

    let cmd_up = CGEvent::new_keyboard_event(source, KeyCode::COMMAND, false)
        .map_err(|_| anyhow!("创建 cmd up 失败"))?;
    cmd_up.post(CGEventTapLocation::HID);

    Ok(())
}
