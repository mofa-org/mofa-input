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
