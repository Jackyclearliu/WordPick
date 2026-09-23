//! 窗口管理器：窗口工厂与复用池（窗口创建开销 < 100ms，NFR 4.1）。
//! 工具条跟随选区显示/隐藏；对话框按功能会话制复用；设置单实例聚焦。
//!
//! ⚠️ Windows 已知问题（WebviewWindowBuilder 文档 Known issues）：
//! 在**同步 command / 事件回调线程**上创建窗口会与 WebView2 死锁（进程挂起后以
//! 0xCFFFFFFF / STATUS_APPLICATION_HANG 退出）。因此本模块所有创建路径都投递到
//! Tauri 异步运行时线程执行；调用方（托盘菜单、快捷键回调、分发线程）一律不阻塞。
use once_cell::sync::Lazy;
use tauri::{Manager, PhysicalPosition, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

use super::position::{locate, PopupPosition, Rect};
use super::WindowKind;

/// 纯窗口创建（调用方必须保证不在同步 command / 事件回调线程上）
/// 创建即定位到屏幕外：Tauri v2 在 Windows 上「创建时 visible(false) 的窗口
/// 后续 show() 可能不渲染」（WebView2 合成器坑），屏幕外创建则既不可见又可靠。
fn build_window(
    app: &tauri::AppHandle,
    label: &str,
    kind: WindowKind,
    url: &str,
) -> tauri::Result<WebviewWindow> {
    const OFFSCREEN: f64 = -32000.0;
    let builder = match kind {
        WindowKind::Toolbar => WebviewWindowBuilder::new(app, label, WebviewUrl::App(url.into()))
            .decorations(false)
            .transparent(true)
            .skip_taskbar(true)
            .focused(false) // 不抢占焦点（FR-2.2）
            .position(OFFSCREEN, OFFSCREEN)
            .resizable(false)
            .shadow(false)
            .inner_size(280.0, 44.0),
        WindowKind::Panel => WebviewWindowBuilder::new(app, label, WebviewUrl::App(url.into()))
            .decorations(false)
            .transparent(true)
            .skip_taskbar(true)
            .focused(true)
            .position(OFFSCREEN, OFFSCREEN)
            .resizable(true)
            .inner_size(420.0, 540.0),
        WindowKind::Settings => WebviewWindowBuilder::new(app, label, WebviewUrl::App(url.into()))
            .decorations(true)
            .inner_size(760.0, 600.0),
    };
    let win = builder.build()?;
    // 工具条加 WS_EX_NOACTIVATE：「永不激活」顶层窗口。tauri 的 show() 走
    // SW_SHOW，正常会尝试激活并抢占前台（目标应用按键随即失联——系统级
    // 「Ctrl+C/Delete/打字全灭」的根因）；带此样式的窗口 SW_SHOW 只显示、
    // 永不夺焦点，渲染路径保持 tauri 原生（直接 SWP_SHOWWINDOW 曾致 WebView2
    // 空白不渲染）。鼠标点击照常工作；1/2/3/Esc 由低层键盘钩子在「光标位于
    // 工具条热区内」时接管，见 selection::uia::start（FR-2.4）。
    if kind == WindowKind::Toolbar {
        if let Ok(raw) = win.hwnd() {
            unsafe {
                use windows::Win32::Foundation::HWND;
                use windows::Win32::UI::WindowsAndMessaging::{
                    GetWindowLongPtrW, SetWindowLongPtrW, GWL_EXSTYLE, WS_EX_NOACTIVATE,
                };
                let hwnd = HWND(raw.0);
                let style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
                SetWindowLongPtrW(hwnd, GWL_EXSTYLE, style | WS_EX_NOACTIVATE.0 as isize);
            }
        }
    }
    Ok(win)
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
/// 返回最终窗口左上角（物理像素），调用方用于记录热区/后续逻辑
pub fn position_at(
    app: &tauri::AppHandle,
    win: &WebviewWindow,
    anchor: (i32, i32),
    pref: PopupPosition,
) -> (i32, i32) {
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
    (x, y)
}

/// 工具条热区（物理像素）：键盘钩子据此判断「光标在工具条内」才接管 1/2/3/Esc
/// （FR-2.4）；隐藏时置 None，钩子随之失效
pub type ToolbarRect = (i32, i32, i32, i32);
pub static TOOLBAR_RECT: Lazy<parking_lot::Mutex<Option<ToolbarRect>>> =
    Lazy::new(|| parking_lot::Mutex::new(None));

fn place_toolbar(
    app: &tauri::AppHandle,
    win: &WebviewWindow,
    anchor: (i32, i32),
    pref: PopupPosition,
) {
    // 整条「隐藏→定位→显示→提z序」必须在主线程同步按序执行：
    // 分散调用时 show()/set_position() 只是向主线程投递消息，与紧随其后的
    // raise（立即执行）乱序——raise 先跑、show 后到时会把 WS_EX_NOACTIVATE
    // 窗口按 SW_SHOW 语义压回活动窗口之下（被刚拖选激活的目标应用盖住）。
    let app2 = app.clone();
    let win = win.clone();
    let app_for_call = app.clone();
    let _ = app_for_call.run_on_main_thread(move || {
        // 先隐藏再移动：任何位置跳变都不暴露，只呈现最终位置
        let _ = win.hide();
        let (x, y) = position_at(&app2, &win, anchor, pref);
        let size = win
            .inner_size()
            .unwrap_or(tauri::PhysicalSize::new(280, 44));
        *TOOLBAR_RECT.lock() = Some((x, y, size.width as i32, size.height as i32));
        // show() 对 WS_EX_NOACTIVATE 窗口只显示不夺焦点（创建时已加样式，见
        // build_window）；但 SW_SHOW 不提升 z 序，拖选会把目标应用激活到顶层，
        // 必须随后 SetWindowPos(HWND_TOP|SWP_NOACTIVATE) 提到 z 序顶（仍不夺焦点）。
        // 1/2/3/Esc 由低层键盘钩子在「光标位于工具条热区内」时接管（FR-2.4）
        if let Err(e) = win.show() {
            log::warn!("toolbar show failed: {e}");
        }
        raise_no_activate(&win);
    });
}

/// 提升窗口到 z 序顶部但不夺前台焦点。工具条（WS_EX_NOACTIVATE）专用：
/// SW_SHOW 对不可激活窗口不提升 z 序；且 HWND_TOP 的提升受前台锁定限制——
/// 无前台权限的进程不能把窗口提到活动窗口之上（拖选→显示间隔约 300ms，
/// 探针 SendInput 挣得的前台权限早已过期），工具条会被目标应用盖住。
/// HWND_TOPMOST 不受此前台锁定限制（弹层类工具的通行做法），随显示置顶、
/// 随隐藏摘除（见 hide_toolbar），不会长期霸占最顶层。
fn raise_no_activate(win: &WebviewWindow) {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{
        SetWindowPos, HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
    };
    let Ok(raw) = win.hwnd() else { return };
    let hwnd = HWND(raw.0);
    unsafe {
        if let Err(e) = SetWindowPos(
            hwnd,
            HWND_TOPMOST,
            0,
            0,
            0,
            0,
            SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOSIZE,
        ) {
            log::warn!("raise toolbar failed: {e}");
        }
    }
}

/// 摘除工具条置顶（隐藏时调用；随显随置、随隐随摘）
fn unset_topmost<R: tauri::Runtime>(win: &WebviewWindow<R>) {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{
        SetWindowPos, HWND_NOTOPMOST, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
    };
    let Ok(raw) = win.hwnd() else { return };
    let hwnd = HWND(raw.0);
    unsafe {
        let _ = SetWindowPos(
            hwnd,
            HWND_NOTOPMOST,
            0,
            0,
            0,
            0,
            SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOSIZE,
        );
    }
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
    // 先隐藏再移动：任何位置跳变都不暴露，只呈现最终位置
    let _ = win.hide();
    position_at(app, win, anchor, pref);
    if let Err(e) = win.show() {
        log::warn!("panel show failed: {e}");
    }
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

/// 隐藏工具条（选区取消，FR-1.2）；同时清除热区，键盘钩子不再接管 1/2/3/Esc
pub fn hide_toolbar<M, R>(app: &M)
where
    M: Manager<R>,
    R: tauri::Runtime,
{
    *TOOLBAR_RECT.lock() = None;
    if let Some(win) = app.get_webview_window("toolbar") {
        unset_topmost(&win);
        let _ = win.hide();
    }
}
