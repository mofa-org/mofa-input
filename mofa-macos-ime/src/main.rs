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

include!("ime/config.rs");
include!("ime/tray.rs");
include!("ime/overlay.rs");
include!("ime/hotkey_tap.rs");
include!("ime/pipeline.rs");
include!("ime/text_model.rs");
include!("ime/audio.rs");
include!("ime/inject.rs");
