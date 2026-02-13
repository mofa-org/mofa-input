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
