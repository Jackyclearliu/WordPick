//! 剪贴板通道：快捷键唤起时读取选区文本（FR-1.3 兜底通道），
//! Windows 自动通道经 WM_COPY 取文本并「读后还原」（不污染用户剪贴板）。
use super::{Selection, SelectionSource};
use tauri_plugin_clipboard_manager::ClipboardExt;

/// 快捷键通道：读取当前剪贴板文本作为选区（无坐标，按光标位置定位）
pub fn read_selection_from_clipboard(app: &tauri::AppHandle) -> Option<Selection> {
    let text: String = app.clipboard().read_text().ok()?;
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(Selection {
        text: trimmed.to_string(),
        anchor: cursor_pos(),
        source: SelectionSource::Shortcut,
    })
}

/// 当前鼠标光标屏幕坐标（各平台，逻辑像素）
pub fn cursor_pos() -> Option<(i32, i32)> {
    platform_cursor_pos()
}

#[cfg(windows)]
fn platform_cursor_pos() -> Option<(i32, i32)> {
    use windows::Win32::Foundation::POINT;
    use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;
    let mut pt = POINT { x: 0, y: 0 };
    unsafe {
        if GetCursorPos(&mut pt).is_ok() {
            Some((pt.x, pt.y))
        } else {
            None
        }
    }
}

#[cfg(target_os = "macos")]
fn platform_cursor_pos() -> Option<(i32, i32)> {
    use core_graphics::event::CGEvent;
    use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
    let src = CGEventSource::new(CGEventSourceStateID::CombinedSessionState)?;
    let event = CGEvent::new(src)?;
    let pt = event.location();
    Some((pt.x as i32, pt.y as i32))
}

#[cfg(not(any(windows, target_os = "macos")))]
fn platform_cursor_pos() -> Option<(i32, i32)> {
    None
}

// ---------------------------------------------------------------------------
// Windows：模拟 Ctrl+C 取词（Chromium 系应用唯一可靠的通道，§5.3）
// Chrome/Edge/VS Code 等自绘应用：不响应 WM_COPY、不触发选区 WinEvent、UIA 树惰性
// 不可见——划词工具通行做法是模拟 Ctrl+C（SendInput 送往前台窗口，用户刚在其中完成
// 拖选松开，前台即目标应用），剪贴板内容变化即选区文本，读后还原。
// ---------------------------------------------------------------------------
/// 两路探测共享的剪贴板串行锁（见 simulate_ctrl_c_probe）
#[cfg(windows)]
fn probe_lock() -> &'static std::sync::Mutex<()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    &LOCK
}

#[cfg(windows)]
pub fn simulate_ctrl_c_probe(app: &tauri::AppHandle) -> Option<String> {
    use std::sync::atomic::{AtomicU64, Ordering};
    // 两路探测（WinEvent / 拖选轮询）可能并发进入：剪贴板「存哨兵→触发复制→读→还原」
    // 必须串行，否则交叉写入会把哨兵或脏数据当选区文本返回
    let _probe_guard = probe_lock().lock().ok()?;
    use std::time::Duration;
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP,
        VK_C, VK_CONTROL,
    };

    let previous: Option<String> = app.clipboard().read_text().ok();
    if previous.is_none() && app.clipboard().read_image().is_ok() {
        return None; // 非文本剪贴板（图片等）：不探测，避免破坏
    }

    static SEQ: AtomicU64 = AtomicU64::new(0);
    let sentinel = format!(
        " wp-ctrlc-{}-{}",
        std::process::id(),
        SEQ.fetch_add(1, Ordering::Relaxed)
    );
    if app.clipboard().write_text(&sentinel).is_err() {
        return None;
    }

    // SendInput 合成 Ctrl↓ C↓ C↑ Ctrl↑（输入会送往前台焦点窗口）
    let key = |vk: u16, up: bool| INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: windows::Win32::UI::Input::KeyboardAndMouse::VIRTUAL_KEY(vk),
                wScan: 0,
                dwFlags: if up {
                    KEYEVENTF_KEYUP
                } else {
                    KEYBD_EVENT_FLAGS(0)
                },
                time: 0,
                dwExtraInfo: 0,
            },
        },
    };
    let inputs = [
        key(VK_CONTROL.0, false),
        key(VK_C.0, false),
        key(VK_C.0, true),
        key(VK_CONTROL.0, true),
    ];
    unsafe {
        SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
    }

    // 部分应用复制是异步的：最多等 300ms，剪贴板离开哨兵即拿到结果
    let mut after: Option<String> = None;
    for _ in 0..15 {
        std::thread::sleep(Duration::from_millis(20));
        let cur = app.clipboard().read_text().ok();
        if cur.as_deref() != Some(sentinel.as_str()) {
            after = cur;
            break;
        }
    }

    // 判定：拿到真实复制结果则保留选中文本在剪贴板（与用户自己 Ctrl+C 的语义一致），
    // **不还原**——无条件还原会覆盖用户在探测窗口内的并发复制（系统 Ctrl+C「失效」的根因）。
    // 仅当应用未复制（剪贴板仍是哨兵/空）时清除哨兵、还原旧内容。
    match &after {
        Some(text) if text != &sentinel && !text.trim().is_empty() => Some(text.clone()),
        _ => {
            match &previous {
                Some(prev) => {
                    let _ = app.clipboard().write_text(prev);
                }
                None => {
                    let _ = app.clipboard().clear();
                }
            }
            None
        }
    }
}

// ---------------------------------------------------------------------------
// Windows：哨兵判定式 WM_COPY 取选中文本（读后还原，不污染用户剪贴板）
// ---------------------------------------------------------------------------
#[cfg(windows)]
pub fn copy_selection_via_wm_copy(
    app: &tauri::AppHandle,
    hwnd: windows::Win32::Foundation::HWND,
) -> Option<String> {
    use std::sync::atomic::{AtomicU64, Ordering};
    let _probe_guard = probe_lock().lock().ok()?;
    use windows::Win32::Foundation::{LPARAM, WPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{
        SendMessageTimeoutW, SMTO_ABORTIFHUNG, SMTO_NORMAL, WM_COPY,
    };

    // 先存原文；剪贴板是非文本内容（图片/文件等）时本轮不探测——哨兵写入会破坏其内容。
    // 空剪贴板 read_text 亦返回 Err，视为可探测（探测后 clear 还原为空）。
    let previous: Option<String> = app.clipboard().read_text().ok();
    if previous.is_none() && app.clipboard().read_image().is_ok() {
        return None;
    }

    // 哨兵：探测前后内容相同 ⇒ 应用未处理 WM_COPY（不支持或当前无选区），判为无选区
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let sentinel = format!(
        "wp-sentinel-{}-{}",
        std::process::id(),
        SEQ.fetch_add(1, Ordering::Relaxed)
    );
    if app.clipboard().write_text(&sentinel).is_err() {
        return None;
    }

    // 超时 150ms 且挂起即放弃：轮询路径不能因目标窗口卡死而阻塞
    unsafe {
        let _ = SendMessageTimeoutW(
            hwnd,
            WM_COPY,
            WPARAM(0),
            LPARAM(0),
            SMTO_ABORTIFHUNG | SMTO_NORMAL,
            150,
            None,
        );
    }
    let after: Option<String> = app.clipboard().read_text().ok();

    let text = after?;
    if text == sentinel || text.trim().is_empty() {
        // 应用未处理 WM_COPY：剪贴板仍是我们的哨兵，清除并还原旧内容
        match &previous {
            Some(prev) => {
                let _ = app.clipboard().write_text(prev);
            }
            None => {
                let _ = app.clipboard().clear();
            }
        }
        return None;
    }
    // 复制成功：保留选中文本在剪贴板，不还原（与 simulate_ctrl_c_probe 同一策略，
    // 还原会覆盖用户在探测窗口内的并发复制）
    Some(text)
}
