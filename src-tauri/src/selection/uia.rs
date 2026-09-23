//! Windows 选区通道（需求文档 §5.3）：
//! 1) SetWinEventHook 订阅 EVENT_OBJECT_TEXTSELECTIONCHANGED（事件驱动，非轮询）；
//! 2) GetGUIThreadInfo 取 caret/选区矩形定位；
//! 3) 文本经 WM_COPY 拷贝到剪贴板读取并还原（标准控件 / 浏览器 / 编辑器通用）；
//! 4) 增强路径：UIA TextPattern.GetSelection() 精确定位（见文件尾注）。
use std::sync::mpsc::Sender;
use std::sync::OnceLock;

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::Accessibility::{
    SetWinEventHook, UnhookWinEvent, EVENT_OBJECT_TEXTSELECTIONCHANGED, HWINEVENTHOOK,
    WINEVENT_OUTOFCONTEXT,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetGUIThreadInfo, GetWindowThreadProcessId, GUITHREADINFO, WM_COPY,
};

use super::super::clipboard;
use super::{Selection, SelectionEvent, SelectionSource};

static TX: OnceLock<Sender<SelectionEvent>> = OnceLock::new();
static HOOK: OnceLock<HWINEVENTHOOK> = OnceLock::new();

/// 启动 WinEventHook 监听线程（事件经 channel 发给统一分发器）
pub fn start(tx: Sender<SelectionEvent>) {
    if TX.set(tx).is_err() {
        return;
    }
    std::thread::spawn(|| unsafe {
        let hook = SetWinEventHook(
            EVENT_OBJECT_TEXTSELECTIONCHANGED,
            EVENT_OBJECT_TEXTSELECTIONCHANGED,
            None,
            Some(on_selection_changed),
            0,
            0,
            WINEVENT_OUTOFCONTEXT,
        );
        if let Ok(h) = hook {
            let _ = HOOK.set(h);
            log::info!("UIA/WinEvent selection watcher started");
        } else {
            log::error!("SetWinEventHook failed");
            return;
        }
        // 消息循环：GetGUIThreadInfo 等本线程调用需要消息队列
        let mut msg = windows::Win32::UI::WindowsAndMessaging::MSG::default();
        while windows::Win32::UI::WindowsAndMessaging::GetMessageW(
            &mut msg,
            HWND(std::ptr::null_mut()),
            0,
            0,
        )
        .as_bool()
        {
            let _ = windows::Win32::UI::WindowsAndMessaging::TranslateMessage(&msg);
            let _ = windows::Win32::UI::WindowsAndMessaging::DispatchMessageW(&msg);
        }
    });
}

unsafe extern "system" fn on_selection_changed(
    _hwineventhook: HWINEVENTHOOK,
    _event: u32,
    hwnd: HWND,
    _idobject: i32,
    _idchild: i32,
    _ideventthread: u32,
    _dwmseventtime: u32,
) {
    let Some(tx) = TX.get() else { return };
    let Some(sel) = read_selection(hwnd) else { return };
    let _ = tx.send(SelectionEvent::Selected(sel));
}

fn read_selection(hwnd: HWND) -> Option<Selection> {
    // 1) 定位：caret/选区矩形（拿不到则以光标位置兜底，§5.3「尽力而为」）
    let anchor = caret_anchor(hwnd).or_else(cursor_anchor);
    // 2) 文本：WM_COPY → 剪贴板 → 还原
    let text = clipboard::copy_selection_via_wm_copy(hwnd)?;
    if text.trim().is_empty() {
        return None;
    }
    Some(Selection {
        text,
        anchor,
        source: SelectionSource::Uia,
    })
}

/// GetGUIThreadInfo 取选区/caret 矩形中心（逻辑像素）
unsafe fn caret_anchor(hwnd: HWND) -> Option<(i32, i32)> {
    let mut info = GUITHREADINFO::default();
    info.cbSize = std::mem::size_of::<GUITHREADINFO>() as u32;
    let thread_id = unsafe { GetWindowThreadProcessId(hwnd, None) };
    if thread_id == 0 {
        return None;
    }
    unsafe {
        if !GetGUIThreadInfo(thread_id, &mut info).as_bool() {
            return None;
        }
    }
    let rc = info.rcCaret;
    if rc.right <= rc.left || rc.bottom <= rc.top {
        return None;
    }
    Some(((rc.left + rc.right) / 2, (rc.top + rc.bottom) / 2))
}

/// 兜底：光标位置
fn cursor_anchor() -> Option<(i32, i32)> {
    use windows::Win32::UI::Input::KeyboardAndMouse::GetCursorPos;
    use windows::Win32::UI::Input::KeyboardAndMouse::POINT;
    let mut pt = POINT { x: 0, y: 0 };
    unsafe {
        if GetCursorPos(&mut pt).as_bool() {
            Some((pt.x, pt.y))
        } else {
            None
        }
    }
}

/// 前台窗口标题（应用黑名单用，FR-1.5）
pub fn frontmost_app_name() -> Option<String> {
    use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowTextW};
    let mut buf = [0u16; 256];
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.0.is_null() {
            return None;
        }
        let len = GetWindowTextW(hwnd, &mut buf);
        if len == 0 {
            return None;
        }
        Some(String::from_utf16_lossy(&buf[..len as usize]))
    }
}

/// 本模块仅做选区监听；文本能力委托 clipboard::copy_selection_via_wm_copy。
/// 增强路径（UIA TextPattern）：对支持 TextPattern 的应用（Edge/Chrome/Office/记事本）
/// 可用 IUIAutomationTextPattern::GetSelection + GetBoundingRectangles 获得精确选区
/// 坐标并避免触碰剪贴板，后续按需在该模块内叠加。
