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

/// 当前鼠标光标屏幕坐标（各平台）
pub fn cursor_pos() -> Option<(i32, i32)> {
    platform_cursor_pos()
}

#[cfg(windows)]
fn platform_cursor_pos() -> Option<(i32, i32)> {
    use windows::Win32::UI::Input::KeyboardAndMouse::{GetCursorPos, POINT};
    let mut pt = POINT { x: 0, y: 0 };
    unsafe {
        if GetCursorPos(&mut pt).as_bool() {
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
// Windows：WM_COPY 取选中文本 + 剪贴板还原
// ---------------------------------------------------------------------------
#[cfg(windows)]
pub fn copy_selection_via_wm_copy(hwnd: windows::Win32::Foundation::HWND) -> Option<String> {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{SendMessageW, WM_COPY};

    let previous = read_clipboard_text();
    unsafe {
        let _ = SendMessageW(hwnd, WM_COPY, WPARAM_UNUSED, LPARAM_UNUSED);
    }
    let selected = read_clipboard_text();
    // 读后还原（尽力而为：仅当原剪贴板为文本时无损还原）
    if let Some(prev) = previous {
        write_clipboard_text(&prev);
    }
    selected.filter(|s| !s.trim().is_empty())
}

#[cfg(windows)]
const WPARAM_UNUSED: windows::Win32::Foundation::WPARAM =
    windows::Win32::Foundation::WPARAM(0);
#[cfg(windows)]
const LPARAM_UNUSED: windows::Win32::Foundation::LPARAM =
    windows::Win32::Foundation::LPARAM(0);

/// 读取剪贴板纯文本（非文本内容返回 None）
#[cfg(windows)]
fn read_clipboard_text() -> Option<String> {
    use windows::Win32::System::DataExchange::{CloseClipboard, GetClipboardData, OpenClipboard};
    use windows::Win32::System::Memory::{GlobalLock, GlobalSize, GlobalUnlock, GMEM_MOVEABLE};
    use windows::Win32::System::SystemServices::CF_UNICODETEXT;

    unsafe {
        if !OpenClipboard(None).as_bool() {
            return None;
        }
        let result = (|| {
            let handle = GetClipboardData(CF_UNICODETEXT.0 as u32)?;
            let size = GlobalSize(handle);
            if size == 0 {
                return None;
            }
            let ptr = GlobalLock(handle);
            if ptr.is_null() {
                return None;
            }
            let wide_len = (size as usize) / 2;
            let slice = std::slice::from_raw_parts(ptr as *const u16, wide_len);
            let s = String::from_utf16_lossy(slice);
            let _ = GlobalUnlock(handle);
            // 去掉结尾 NUL
            Some(s.trim_end_matches('\0').to_string())
        })();
        let _ = CloseClipboard();
        result
    }
}

/// 写入剪贴板纯文本
#[cfg(windows)]
fn write_clipboard_text(text: &str) {
    use windows::Win32::System::DataExchange::{
        CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData,
    };
    use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
    use windows::Win32::System::SystemServices::CF_UNICODETEXT;

    unsafe {
        if !OpenClipboard(None).as_bool() {
            return;
        }
        let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
        let bytes = wide.len() * 2;
        let handle = GlobalAlloc(GMEM_MOVEABLE, bytes);
        if !handle.is_null() {
            let ptr = GlobalLock(handle);
            if !ptr.is_null() {
                std::ptr::copy_nonoverlapping(wide.as_ptr(), ptr as *mut u16, wide.len());
                let _ = GlobalUnlock(handle);
            }
            let _ = EmptyClipboard();
            let _ = SetClipboardData(CF_UNICODETEXT.0 as u32, handle);
        }
        let _ = CloseClipboard();
    }
}
