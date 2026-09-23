//! macOS 辅助功能（Accessibility）通道（需求文档 §5.3）：
//! 监听 AXSelectedTextChanged / AXFocusedUIElementChanged，
//! 读取 AXSelectedText / AXSelectedTextRange，经 AXBoundsForRange 取选区屏幕坐标。
//! 未授权时由上层降级为快捷键模式（§4.4）。
use std::sync::mpsc::Sender;

use accessibility_sys::*;
use core_foundation::base::TCFType;
use core_foundation::string::CFString;

use super::{Selection, SelectionEvent, SelectionSource};

/// 检查（并可引导）辅助功能权限
pub fn accessibility_granted() -> bool {
    unsafe {
        // 以 prompt=true 查询：未授权时系统弹出授权引导
        let key = CFString::new("AXTrustedCheckOptionPrompt");
        let value = core_foundation::boolean::CFBoolean::true_value();
        let dict = core_foundation::dictionary::CFDictionary::from_CFType_pairs(&[(
            key.as_CFType(),
            value.as_CFType(),
        )]);
        AXIsProcessTrustedWithOptions(dict.as_concrete_TypeRef())
    }
}

/// 当前前台应用名（应用黑名单用，FR-1.5）
pub fn frontmost_app_name() -> Option<String> {
    use cocoa::appkit::NSWorkspace;
    use cocoa::base::nil;
    use objc::msg_send;
    unsafe {
        let app = NSWorkspace::sharedWorkspace(nil).frontmostApplication();
        if app == nil {
            return None;
        }
        let name: cocoa::base::id = msg_send![app, localizedName];
        if name == nil {
            return None;
        }
        Some(cfstring_to_string(
            name as core_foundation::string::CFStringRef,
        ))
    }
}

unsafe fn cfstring_to_string(s: core_foundation::string::CFStringRef) -> String {
    CFString::wrap_under_get_rule(s).to_string()
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct CGPoint {
    x: f64,
    y: f64,
}
#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct CGSize {
    w: f64,
    h: f64,
}
#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct CGRect {
    origin: CGPoint,
    size: CGSize,
}

/// 启动 AX 监听线程（在该线程的 CFRunLoop 上执行观察者回调）。
/// 事件经 channel 发给统一分发器（含防抖）。
pub fn start(tx: Sender<SelectionEvent>) {
    std::thread::spawn(move || unsafe {
        let mut observer: AXObserverRef = std::ptr::null_mut();
        let pid = std::process::id() as pid_t;
        let cb: AXObserverCallback = on_ax_event;
        let err = AXObserverCreate(pid, cb, &mut observer);
        if err != kAXErrorSuccess || observer.is_null() {
            log::error!("AXObserverCreate failed: {err}");
            return;
        }
        // refcon 携带 Sender
        let tx_ptr = Box::into_raw(Box::new(tx)) as *mut std::ffi::c_void;
        let system_wide = AXUIElementCreateSystemWide();
        for notification in [
            kAXSelectedTextChangedNotification,
            kAXFocusedUIElementChangedNotification,
        ] {
            let name = CFString::new(notification);
            let err = AXObserverAddNotification(
                observer,
                system_wide,
                name.as_concrete_TypeRef(),
                tx_ptr,
            );
            if err != kAXErrorSuccess {
                log::warn!("AddNotification {notification} failed: {err}");
            }
        }
        let source = AXObserverGetRunLoopSource(observer);
        let runloop = core_foundation::runloop::CFRunLoop::current();
        core_foundation::runloop::CFRunLoopAddSource(
            runloop.as_concrete_TypeRef(),
            source,
            core_foundation::runloop::kCFRunLoopDefaultMode,
        );
        log::info!("AX selection watcher started");
        core_foundation::runloop::CFRunLoopRun();
    });
}

unsafe extern "C" fn on_ax_event(
    _observer: AXObserverRef,
    _element: AXUIElementRef,
    _notification: core_foundation::string::CFStringRef,
    refcon: *mut std::ffi::c_void,
) {
    let tx = &*(refcon as *const Sender<SelectionEvent>);
    if let Some(sel) = read_selection() {
        let _ = tx.send(SelectionEvent::Selected(sel));
    }
}

/// 从系统焦点元素读取选中文本与选区末端屏幕坐标
unsafe fn read_selection() -> Option<Selection> {
    let system_wide = AXUIElementCreateSystemWide();

    // 1) 焦点元素
    let mut focused: core_foundation::base::CFTypeRef = std::ptr::null();
    let attr = CFString::new(kAXFocusedUIElementAttribute);
    let err = AXUIElementCopyAttributeValue(system_wide, attr.as_concrete_TypeRef(), &mut focused);
    if err != kAXErrorSuccess || focused.is_null() {
        return None;
    }

    // 2) AXSelectedText
    let attr = CFString::new(kAXSelectedTextAttribute);
    let mut text_ref: core_foundation::base::CFTypeRef = std::ptr::null();
    if AXUIElementCopyAttributeValue(
        focused as AXUIElementRef,
        attr.as_concrete_TypeRef(),
        &mut text_ref,
    ) != kAXErrorSuccess
        || text_ref.is_null()
    {
        return None;
    }
    let text =
        CFString::wrap_under_get_rule(text_ref as core_foundation::string::CFStringRef).to_string();
    if text.trim().is_empty() {
        return None;
    }

    // 3) AXSelectedTextRange → AXBoundsForRange 取选区矩形（可能缺失，尽力而为）
    let anchor = selection_anchor(focused as AXUIElementRef).map(|r| {
        (
            (r.origin.x + r.size.w) as i32,
            (r.origin.y + r.size.h) as i32,
        )
    });

    Some(Selection {
        text,
        anchor,
        source: SelectionSource::Ax,
    })
}

/// 读取选区末端坐标：AXSelectedTextRange → AXBoundsForRange
unsafe fn selection_anchor(element: AXUIElementRef) -> Option<CGRect> {
    use core_foundation_sys::base::CFRange;

    let attr = CFString::new(kAXSelectedTextRangeAttribute);
    let mut range_ref: core_foundation::base::CFTypeRef = std::ptr::null();
    if AXUIElementCopyAttributeValue(element, attr.as_concrete_TypeRef(), &mut range_ref)
        != kAXErrorSuccess
        || range_ref.is_null()
    {
        return None;
    }
    let mut range = CFRange {
        location: 0,
        length: 0,
    };
    if !AXValueGetValue(
        range_ref as AXValueRef,
        kAXValueTypeCFRange,
        &mut range as *mut CFRange as *mut std::ffi::c_void,
    ) {
        return None;
    }
    let range_value = AXValueCreate(
        kAXValueTypeCFRange,
        &range as *const CFRange as *const std::ffi::c_void,
    );
    if range_value.is_null() {
        return None;
    }
    let param = CFString::new(kAXBoundsForRangeParameterizedAttribute);
    let mut bounds_ref: core_foundation::base::CFTypeRef = std::ptr::null();
    if AXUIElementCopyParameterizedAttributeValue(
        element,
        param.as_concrete_TypeRef(),
        range_value as core_foundation::base::CFTypeRef,
        &mut bounds_ref,
    ) != kAXErrorSuccess
        || bounds_ref.is_null()
    {
        return None;
    }
    let mut rect = CGRect {
        origin: CGPoint { x: 0.0, y: 0.0 },
        size: CGSize { w: 0.0, h: 0.0 },
    };
    if AXValueGetValue(
        bounds_ref as AXValueRef,
        kAXValueTypeCGRect,
        &mut rect as *mut CGRect as *mut std::ffi::c_void,
    ) {
        Some(rect)
    } else {
        None
    }
}
