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
// Windows：WM_COPY 取选中文本 + 剪贴板还原（经 Tauri 剪贴板插件，跨 crate 版本安全）
// ---------------------------------------------------------------------------
#[cfg(windows)]
pub fn copy_selection_via_wm_copy(
    app: &tauri::AppHandle,
    hwnd: windows::Win32::Foundation::HWND,
) -> Option<String> {
    use windows::Win32::Foundation::{LPARAM, WPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{SendMessageW, WM_COPY};

    // 仅当原剪贴板为文本时无损还原（图片等内容不覆盖、不写回）
    let previous: Option<String> = app.clipboard().read_text().ok();
    unsafe {
        SendMessageW(hwnd, WM_COPY, WPARAM(0), LPARAM(0));
    }
    let selected: Option<String> = app.clipboard().read_text().ok();
    if let Some(prev) = previous {
        let _ = app.clipboard().write_text(prev);
    }
    selected.filter(|s| !s.trim().is_empty())
}
