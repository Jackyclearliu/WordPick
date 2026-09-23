//! 窗口管理器：窗口工厂与复用池（窗口创建开销 < 100ms，NFR 4.1）。
//! 工具条跟随选区显示/隐藏；对话框按功能会话制复用；设置单实例聚焦。
//!
//! ⚠️ Windows 已知问题（WebviewWindowBuilder 文档 Known issues）：
//! 在**同步 command / 事件回调线程**上创建窗口会与 WebView2 死锁（进程挂起后以
//! 0xCFFFFFFF / STATUS_APPLICATION_HANG 退出）。因此本模块所有创建路径都投递到
//! Tauri 异步运行时线程执行；调用方（托盘菜单、快捷键回调、分发线程）一律不阻塞。
use tauri::{Manager, PhysicalPosition, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

use super::position::{locate, PopupPosition, Rect};
use super::WindowKind;

/// 纯窗口创建（调用方必须保证不在同步 command / 事件回调线程上）
fn build_window(
    app: &tauri::AppHandle,
    label: &str,
    kind: WindowKind,
    url: &str,
) -> tauri::Result<WebviewWindow> {
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

/// 取已存在的窗口；不存在则调度创建（异步运行时线程），创建完成后回调 on_created。
///
/// - 已存在：立即返回 `Some(win)`，调用方可继续同步操作该窗口（show/position 线程安全）。
/// - 不存在：返回 `None`，创建成功后由 on_created 继续（定位 / 显示 / 聚焦）。
pub fn with_window<F>(
    app: &tauri::AppHandle,
    label: &str,
    kind: WindowKind,
    url: &str,
    on_created: F,
) -> Option<WebviewWindow>
where
    F: FnOnce(&WebviewWindow) + Send + 'static,
{
    if let Some(win) = app.get_webview_window(label) {
        return Some(win);
    }
    let app = app.clone();
    let label = label.to_string();
    let url = url.to_string();
    tauri::async_runtime::spawn(async move {
        match build_window(&app, &label, kind, &url) {
            Ok(win) => on_created(&win),
            Err(e) => log::error!("create window '{label}' failed: {e}"),
        }
    });
    None
}

/// 显示窗口（已存在则显示并聚焦：设置窗口单实例聚焦，FR-7.3 同类语义）
pub fn show_window(app: &tauri::AppHandle, label: &str, kind: WindowKind, url: &str) {
    fn show(win: &WebviewWindow, kind: WindowKind) {
        let _ = win.show();
        if kind == WindowKind::Settings {
            let _ = win.set_focus();
        }
    }
    let created = with_window(app, label, kind, url, move |w| show(w, kind));
    if let Some(win) = created {
        show(&win, kind);
    }
}

/// 把窗口定位到锚点旁（FR-3.1：选区下方/右侧可配置，空间不足自动翻转）
pub fn position_at(
    app: &tauri::AppHandle,
    win: &WebviewWindow,
    anchor: (i32, i32),
    pref: PopupPosition,
) {
    // 工作区：优先取包含锚点的显示器，退回主显示器（多显示器由 Tauri 统一处理）
    let workarea = app
        .monitor_from_point(anchor.0 as f64, anchor.1 as f64)
        .ok()
        .flatten()
        .or_else(|| app.primary_monitor().ok().flatten())
        .map(|m| Rect {
            x: m.position().x,
            y: m.position().y,
            w: m.size().width as i32,
            h: m.size().height as i32,
        })
        .unwrap_or(Rect {
            x: 0,
            y: 0,
            w: 1920,
            h: 1080,
        });

    let size = win
        .inner_size()
        .unwrap_or(tauri::PhysicalSize::new(420, 540));
    let (x, y) = locate(
        anchor,
        (size.width as i32, size.height as i32),
        workarea,
        pref,
    );
    // 锚点（GetCursorPos/GetGUIThreadInfo）与工作区均为物理像素，直接置位；
    // 再除 scale 会把高 DPI 屏上的窗口向原点拉（「跑到左上角」 bug）
    let _ = win.set_position(PhysicalPosition::new(x, y));
}

fn place_toolbar(
    app: &tauri::AppHandle,
    win: &WebviewWindow,
    anchor: (i32, i32),
    pref: PopupPosition,
) {
    position_at(app, win, anchor, pref);
    let _ = win.show();
    let _ = win.set_focus(); // 无焦点工具条需要一次焦点以接收键盘操作（FR-2.4），随后不抢占输入
}

/// 在选区锚点处显示工具条（含定位算法与工作区约束，FR-1.1）
pub fn show_toolbar_at(app: &tauri::AppHandle, anchor: Option<(i32, i32)>, pref: PopupPosition) {
    let Some(anchor) = anchor.or_else(crate::selection::clipboard::cursor_pos) else {
        return;
    };
    let app2 = app.clone();
    let created = with_window(
        app,
        "toolbar",
        WindowKind::Toolbar,
        "toolbar.html",
        move |w| {
            place_toolbar(&app2, w, anchor, pref);
        },
    );
    if let Some(win) = created {
        place_toolbar(app, &win, anchor, pref);
    }
}

fn place_panel(
    app: &tauri::AppHandle,
    win: &WebviewWindow,
    anchor: (i32, i32),
    pref: PopupPosition,
) {
    position_at(app, win, anchor, pref);
    let _ = win.show();
}

/// 在选区旁显示对话框（翻译/解释入口）
pub fn show_panel(app: &tauri::AppHandle, anchor: Option<(i32, i32)>, pref: PopupPosition) {
    let Some(anchor) = anchor.or_else(crate::selection::clipboard::cursor_pos) else {
        return;
    };
    let app2 = app.clone();
    let created = with_window(app, "panel", WindowKind::Panel, "panel.html", move |w| {
        place_panel(&app2, w, anchor, pref);
    });
    if let Some(win) = created {
        place_panel(app, &win, anchor, pref);
    }
}

/// 隐藏工具条（选区取消，FR-1.2）
pub fn hide_toolbar<M, R>(app: &M)
where
    M: Manager<R>,
    R: tauri::Runtime,
{
    if let Some(win) = app.get_webview_window("toolbar") {
        let _ = win.hide();
    }
}
