//! Windows 选区通道（需求文档 §5.3）：
//! 1) SetWinEventHook 订阅 EVENT_OBJECT_TEXTSELECTIONCHANGED（事件驱动，非轮询）；
//! 2) **轮询兜底**：Chrome/Edge 等浏览器拖选不触发上述 WinEvent，改为每 120ms 对前台
//!    窗口焦点控件做哨兵式 WM_COPY 探测，文本变化才上报（§5.3「尽力而为」）；
//! 3) GetGUIThreadInfo 取 caret/选区矩形定位；
//! 4) 文本经 WM_COPY 拷贝到剪贴板读取并还原（标准控件 / 浏览器 / 编辑器通用）。
use std::sync::mpsc::Sender;
use std::sync::OnceLock;

use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Accessibility::{SetWinEventHook, HWINEVENTHOOK};
use windows::Win32::UI::WindowsAndMessaging::{
    GetForegroundWindow, GetGUIThreadInfo, GetWindowThreadProcessId,
    EVENT_OBJECT_TEXTSELECTIONCHANGED, GUITHREADINFO, WINEVENT_OUTOFCONTEXT,
};

use super::clipboard;
use super::{Selection, SelectionEvent, SelectionSource};

static CTX: OnceLock<(Sender<SelectionEvent>, tauri::AppHandle)> = OnceLock::new();
static HOOK: OnceLock<isize> = OnceLock::new();

/// 轮询间隔：120ms 探测 + 分发器 180ms 防抖 ≈ 300ms 内弹出（FR-1.1）
const POLL_INTERVAL: std::time::Duration = std::time::Duration::from_millis(120);

/// 启动 WinEventHook 监听线程 + 浏览器轮询线程（事件经 channel 发给统一分发器）
pub fn start(tx: Sender<SelectionEvent>, app: tauri::AppHandle) {
    if CTX.set((tx, app)).is_err() {
        return;
    }
    std::thread::spawn(|| unsafe {
        let hook: HWINEVENTHOOK = SetWinEventHook(
            EVENT_OBJECT_TEXTSELECTIONCHANGED,
            EVENT_OBJECT_TEXTSELECTIONCHANGED,
            None,
            Some(on_selection_changed),
            0,
            0,
            WINEVENT_OUTOFCONTEXT,
        );
        let _ = HOOK.set(hook.0 as isize);
        log::info!("UIA/WinEvent selection watcher started");

        // 消息循环：本线程的 WinEvent 回调需要消息队列
        let mut msg = windows::Win32::UI::WindowsAndMessaging::MSG::default();
        while windows::Win32::UI::WindowsAndMessaging::GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = windows::Win32::UI::WindowsAndMessaging::TranslateMessage(&msg);
            let _ = windows::Win32::UI::WindowsAndMessaging::DispatchMessageW(&msg);
        }
    });
    std::thread::spawn(poll_loop);
    log::info!("browser selection poller started");
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
    let Some((tx, app)) = CTX.get() else { return };
    let Some(sel) = read_selection(app, hwnd) else {
        return;
    };
    let _ = tx.send(SelectionEvent::Selected(sel));
}

/// 轮询兜底：对前台窗口焦点控件做选区探测（Chrome/Edge 拖选不触发 WinEvent）。
/// 文本与上次相同不重复上报；连续探测不到或焦点移走则上报 Cleared。
fn poll_loop() {
    let mut last_text: Option<String> = None;
    let mut last_hwnd: Option<isize> = None;
    let mut miss_streak = 0u32;
    loop {
        std::thread::sleep(POLL_INTERVAL);
        let Some((tx, app)) = CTX.get() else { continue };
        let Some(target) = foreground_focus_hwnd() else {
            continue;
        };

        match read_selection(app, target) {
            Some(sel) => {
                miss_streak = 0;
                if last_text.as_deref() != Some(sel.text.as_str()) {
                    last_text = Some(sel.text.clone());
                    last_hwnd = Some(target.0 as isize);
                    let _ = tx.send(SelectionEvent::Selected(sel));
                }
            }
            None => {
                if last_text.is_none() {
                    continue;
                }
                miss_streak += 1;
                let focus_moved = last_hwnd.is_some_and(|h| h != target.0 as isize);
                if focus_moved || miss_streak >= 2 {
                    last_text = None;
                    last_hwnd = None;
                    let _ = tx.send(SelectionEvent::Cleared);
                }
            }
        }
    }
}

/// 前台窗口的焦点控件（跳过 WordPick 自身窗口，避免自我探测）
fn foreground_focus_hwnd() -> Option<HWND> {
    unsafe {
        let fg = GetForegroundWindow();
        if fg.0.is_null() {
            return None;
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(fg, Some(&mut pid));
        if pid == std::process::id() {
            return None;
        }
        let thread_id = GetWindowThreadProcessId(fg, None);
        let mut info = GUITHREADINFO {
            cbSize: std::mem::size_of::<GUITHREADINFO>() as u32,
            ..Default::default()
        };
        if GetGUIThreadInfo(thread_id, &mut info).is_err() {
            return None;
        }
        if info.hwndFocus.0.is_null() {
            None
        } else {
            Some(info.hwndFocus)
        }
    }
}

fn read_selection(app: &tauri::AppHandle, hwnd: HWND) -> Option<Selection> {
    // 1) 定位：caret/选区矩形（拿不到则以光标位置兜底，§5.3「尽力而为」）
    let anchor = unsafe { caret_anchor(hwnd) }.or_else(clipboard::cursor_pos);
    // 2) 文本：哨兵式 WM_COPY → 剪贴板 → 还原
    let text = clipboard::copy_selection_via_wm_copy(app, hwnd)?;
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
    let mut info = GUITHREADINFO {
        cbSize: std::mem::size_of::<GUITHREADINFO>() as u32,
        ..Default::default()
    };
    let thread_id = unsafe { GetWindowThreadProcessId(hwnd, None) };
    if thread_id == 0 {
        return None;
    }
    unsafe {
        if GetGUIThreadInfo(thread_id, &mut info).is_err() {
            return None;
        }
    }
    let rc = info.rcCaret;
    if rc.right <= rc.left || rc.bottom <= rc.top {
        return None;
    }
    Some(((rc.left + rc.right) / 2, (rc.top + rc.bottom) / 2))
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

// 增强路径（UIA TextPattern）：对支持 TextPattern 的应用（Edge/Chrome/Office/记事本）
// 可用 IUIAutomationTextPattern::GetSelection + GetBoundingRectangles 获得精确选区
// 坐标并避免触碰剪贴板，后续按需在该模块内叠加（当前轮询路径已覆盖浏览器取词）。
