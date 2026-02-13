const OVERLAY_WIDTH: f64 = 560.0;
const OVERLAY_HEIGHT: f64 = 50.0;
const OVERLAY_BOTTOM_MARGIN: f64 = 24.0;
const OVERLAY_TOP_MARGIN: f64 = 24.0;
const OVERLAY_SWITCH_DISTANCE: f64 = 210.0;
const OVERLAY_STATUS_BADGE_X: f64 = 16.0;
const OVERLAY_STATUS_BADGE_WIDTH: f64 = 92.0;
const OVERLAY_STATUS_BADGE_HEIGHT: f64 = 33.0;
const OVERLAY_STATUS_TEXT_X_OFFSET: f64 = -OVERLAY_WIDTH + 85.0;
const OVERLAY_STATUS_TEXT_Y_OFFSET: f64 = -(OVERLAY_HEIGHT * 0.5) + 17.0;
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
    status_badge: id,
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
    let badge_x = OVERLAY_STATUS_BADGE_X;
    let preview_x = badge_x + status_w + 16.0;
    let preview_w = OVERLAY_WIDTH - preview_x - 10.0;
    let status_y = ((total_h - status_h) * 0.5).floor();
    let preview_y = ((total_h - preview_h) * 0.5).floor();
    let badge_frame = NSRect::new(
        NSPoint::new(badge_x, status_y),
        NSSize::new(status_w, status_h),
    );
    let status_text_frame = NSRect::new(
        NSPoint::new(
            OVERLAY_STATUS_TEXT_X_OFFSET,
            status_y + OVERLAY_STATUS_TEXT_Y_OFFSET,
        ),
        NSSize::new(OVERLAY_WIDTH, status_h),
    );
    let preview_frame = NSRect::new(
        NSPoint::new(preview_x, preview_y),
        NSSize::new(preview_w, preview_h),
    );
    let _: () = msg_send![status_badge, setFrame: badge_frame];
    let _: () = msg_send![status_label, setFrame: status_text_frame];
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

    let status_y = (OVERLAY_HEIGHT - OVERLAY_STATUS_BADGE_HEIGHT) * 0.5;
    let status_badge = NSView::initWithFrame_(
        NSView::alloc(nil),
        NSRect::new(
            NSPoint::new(OVERLAY_STATUS_BADGE_X, status_y),
            NSSize::new(OVERLAY_STATUS_BADGE_WIDTH, OVERLAY_STATUS_BADGE_HEIGHT),
        ),
    );
    let _: () = msg_send![status_badge, setWantsLayer: YES];
    let status_badge_layer: id = msg_send![status_badge, layer];
    if status_badge_layer != nil {
        let _: () = msg_send![
            status_badge_layer,
            setCornerRadius: (OVERLAY_STATUS_BADGE_HEIGHT * 0.5).floor()
        ];
        let _: () = msg_send![status_badge_layer, setMasksToBounds: YES];
    }
    content.addSubview_(status_badge);

    let status_label = NSTextField::initWithFrame_(
        NSTextField::alloc(nil),
        NSRect::new(
            NSPoint::new(
                OVERLAY_STATUS_TEXT_X_OFFSET,
                status_y + OVERLAY_STATUS_TEXT_Y_OFFSET,
            ),
            NSSize::new(OVERLAY_WIDTH, OVERLAY_STATUS_BADGE_HEIGHT),
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
    let _: () = msg_send![status_label, setStringValue: ns_string("就绪")];
    set_status_badge_appearance(status_badge, "就绪");
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
        status_badge_ptr: status_badge as usize,
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
