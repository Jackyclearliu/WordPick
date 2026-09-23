//! Windows 选区通道（需求文档 §5.3）：
//! 1) **鼠标拖选释放触发**：用户在目标应用拖选、松开左键 → 模拟 Ctrl+C 取词。
//!    这是 Chromium 系（Chrome/Edge/VS Code/Electron）唯一可靠通道：它们不响应
//!    WM_COPY、拖选不触发 EVENT_OBJECT_TEXTSELECTIONCHANGED、UIA 树惰性不可见；
//! 2) SetWinEventHook 订阅 EVENT_OBJECT_TEXTSELECTIONCHANGED（记事本/Office 等
//!    经典应用事件驱动），配合哨兵式 WM_COPY 取词；
//! 3) GetGUIThreadInfo 取 caret/选区矩形定位（取不到以光标位置兜底）。
//!
//! 设计要点：不做周期性剪贴板写入（剪贴板历史工具零干扰），只在「拖选松开」与
//! 「选区变更事件」两个时刻探测；剪贴板读后还原，非文本剪贴板（图片）跳过探测。
use std::sync::mpsc::Sender;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

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

/// 轮询周期：检测鼠标释放与前台切换（轻量，不触碰剪贴板）
const POLL_INTERVAL: Duration = Duration::from_millis(100);
/// 拖选判定：左键按住 ≥180ms 且位移 >6px（区分点按与拖选）
const DRAG_MIN_HOLD: Duration = Duration::from_millis(180);
const DRAG_MIN_MOVE: i32 = 6;

/// 启动 WinEventHook 监听线程 + 鼠标拖选轮询线程（事件经 channel 发给统一分发器）
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
    log::info!("drag-selection poller started");
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

/// 鼠标拖选轮询：左键「按住→位移→松开」即对前台应用做模拟 Ctrl+C 探测。
/// 前台切换离开选区窗口时上报 Cleared（工具条随选区消失而隐藏，FR-1.2）。
fn poll_loop() {
    let mut last_fg: Option<isize> = None;
    let mut down_since: Option<Instant> = None;
    let mut down_pos = (0i32, 0i32);
    loop {
        std::thread::sleep(POLL_INTERVAL);
        let Some((tx, app)) = CTX.get() else { continue };
        let fg = foreground_hwnd();

        // 前台变化：离开选区所在窗口（或切到 WordPick 自身）→ 清选区
        if fg.map(|h| h.0 as isize) != last_fg {
            if last_fg.is_some() {
                let _ = tx.send(SelectionEvent::Cleared);
            }
            last_fg = fg.map(|h| h.0 as isize);
        }

        // 鼠标左键状态跟踪
        let (down, pos) = mouse_state();
        if down {
            if down_since.is_none() {
                down_since = Some(Instant::now());
                down_pos = pos;
            }
            continue;
        }
        let Some(held) = down_since.take() else {
            continue;
        };
        let dragged = held.elapsed() >= DRAG_MIN_HOLD
            && (pos.0 - down_pos.0).abs() + (pos.1 - down_pos.1).abs() > DRAG_MIN_MOVE;
        if !dragged {
            continue;
        }
        let Some(target) = fg else { continue };
        if let Some(sel) = read_selection_via_ctrl_c(app, target) {
            let _ = tx.send(SelectionEvent::Selected(sel));
        } else {
            // 拖选后取不到文本（应用不支持复制/无实际选区）→ 视为选区已清
            let _ = tx.send(SelectionEvent::Cleared);
        }
    }
}

/// 前台窗口（跳过 WordPick 自身窗口，避免自我探测）
fn foreground_hwnd() -> Option<HWND> {
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
        Some(fg)
    }
}

/// 鼠标左键按下状态 + 当前光标位置
fn mouse_state() -> (bool, (i32, i32)) {
    use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_LBUTTON};
    let down = unsafe { GetAsyncKeyState(VK_LBUTTON.0 as i32) } as u16 & 0x8000 != 0;
    (down, clipboard::cursor_pos().unwrap_or((0, 0)))
}

/// 模拟 Ctrl+C 通道：Chromium 系应用的划词取词（用户在目标应用内拖选松开，前台即目标）
fn read_selection_via_ctrl_c(app: &tauri::AppHandle, hwnd: HWND) -> Option<Selection> {
    let anchor = unsafe { caret_anchor(hwnd) }.or_else(clipboard::cursor_pos);
    let text = clipboard::simulate_ctrl_c_probe(app)?;
    if text.trim().is_empty() {
        return None;
    }
    Some(Selection {
        text,
        anchor,
        source: SelectionSource::Uia,
    })
}

/// WinEvent 通道（经典应用）：WM_COPY 哨兵取词
fn read_selection(app: &tauri::AppHandle, hwnd: HWND) -> Option<Selection> {
    let anchor = unsafe { caret_anchor(hwnd) }.or_else(clipboard::cursor_pos);
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
