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
