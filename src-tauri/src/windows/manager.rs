//! 窗口管理器：窗口工厂与复用池（窗口创建开销 < 100ms，NFR 4.1）。
//! 工具条跟随选区显示/隐藏；对话框按功能会话制复用；设置单实例聚焦。
use tauri::{Manager, PhysicalPosition, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

use super::position::{locate, PopupPosition, Rect};
use super::WindowKind;

/// 获取或创建指定窗口（复用池：预创建、隐藏/显示切换）
pub fn get_or_create<M: Manager>(
    app: &M,
    label: &str,
    kind: WindowKind,
    url: &str,
) -> tauri::Result<WebviewWindow> {
    if let Some(win) = app.get_webview_window(label) {
        return Ok(win);
    }
    let builder = match kind {
        WindowKind::Toolbar => WebviewWindowBuilder::new(app, label, WebviewUrl::App(url.into()))
            .decorations(false)
            .transparent(true)
            .skip_taskbar(true)
            .focused(false) // 不抢占焦点（FR-2.2）
            .resizable(false)
            .shadow(false)
            .inner_size(280.0, 44.0),
        WindowKind::Panel => WebviewWindowBuilder::new(app, label, WebviewUrl::App(url.into()))
            .decorations(false)
            .transparent(true)
            .skip_taskbar(true)
            .focused(true)
            .resizable(true)
            .inner_size(420.0, 540.0),
        WindowKind::Settings => WebviewWindowBuilder::new(app, label, WebviewUrl::App(url.into()))
            .decorations(true)
            .inner_size(760.0, 600.0),
    };
    builder.build()
}

/// 显示窗口（已存在则显示并聚焦：设置窗口单实例聚焦，FR-7.3 同类语义）
pub fn show_window<M: Manager>(app: &M, label: &str, kind: WindowKind, url: &str) {
    match get_or_create(app, label, kind, url) {
        Ok(win) => {
            let _ = win.show();
            if kind == WindowKind::Settings {
                let _ = win.set_focus();
            }
        }
        Err(e) => log::error!("show_window {label} failed: {e}"),
    }
}

/// 在选区锚点处显示工具条（含定位算法与工作区约束，FR-1.1）
pub fn show_toolbar_at<M: Manager>(
    app: &M,
    anchor: Option<(i32, i32)>,
    pref: PopupPosition,
) {
    let Some(anchor) = anchor.or_else(crate::selection::clipboard::cursor_pos) else {
        return;
    };
    let Ok(win) = get_or_create(app, "toolbar", WindowKind::Toolbar, "toolbar.html") else {
        return;
    };

    // 工作区：优先取包含锚点的显示器，退回主显示器（多显示器 + DPI 由 Tauri 统一处理）
    let (workarea, scale) = app
        .monitor_from_point(anchor.0 as f64, anchor.1 as f64)
        .ok()
        .flatten()
        .or_else(|| app.primary_monitor().ok().flatten())
        .map(|m| {
            (
                Rect {
                    x: m.position().x,
                    y: m.position().y,
                    w: m.size().width,
                    h: m.size().height,
                },
                m.scale_factor(),
            )
        })
        .unwrap_or((
            Rect { x: 0, y: 0, w: 1920, h: 1080 },
            1.0,
        ));

    let size = win.inner_size().unwrap_or(tauri::PhysicalSize::new(280, 44));
    let (x, y) = locate(
        anchor,
        (size.width as i32, size.height as i32),
        workarea,
        pref,
    );
    let _ = win.set_position(PhysicalPosition::new(
        (x as f64 / scale) as i32,
        (y as f64 / scale) as i32,
    ));
    let _ = win.show();
    let _ = win.set_focus(); // 无焦点工具条需要一次焦点以接收键盘操作（FR-2.4），随后不抢占输入
}

/// 隐藏工具条（选区取消，FR-1.2）
pub fn hide_toolbar<M: Manager>(app: &M) {
    if let Some(win) = app.get_webview_window("toolbar") {
        let _ = win.hide();
    }
}
