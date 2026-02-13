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


include!("model_manager/ui_bootstrap.rs");
include!("model_manager/config.rs");
include!("model_manager/catalog.rs");
include!("model_manager/download.rs");
include!("model_manager/app.rs");
