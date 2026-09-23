// WordPick Rust 核心层：系统能力（选区探测 / 窗口管理 / 模型适配 / 配置与安全存储）
pub mod commands;
pub mod config;
pub mod llm;
pub mod secure;
pub mod selection;
pub mod state;
pub mod tray;
pub mod windows;

use std::sync::mpsc;
use std::sync::Arc;
use std::time::Duration;

use state::AppState;
use tauri::Manager;
use windows::position::PopupPosition;

/// 选区事件防抖窗口：拖选期间事件高频触发，安静 220ms 后才弹出工具条（FR-1.1 ≤300ms）
const DEBOUNCE: Duration = Duration::from_millis(220);

/// 统一事件分发器：过滤（长度/黑名单）→ 防抖 → 工具条显示/隐藏
fn spawn_dispatcher(
    rx: mpsc::Receiver<selection::SelectionEvent>,
    app: tauri::AppHandle,
    config: Arc<config::AppConfig>,
    state: Arc<AppState>,
) {
    std::thread::spawn(move || {
        let mut pending: Option<selection::Selection> = None;
        let mut last_change: Option<std::time::Instant> = None;
        loop {
            match rx.recv_timeout(Duration::from_millis(60)) {
                Ok(selection::SelectionEvent::Selected(sel)) => {
                    // 过滤：空白 / 超长 / 黑名单（FR-1.4 / FR-1.5）
                    let trimmed = sel.text.trim();
                    if trimmed.is_empty()
                        || trimmed.chars().count() > config.general.max_selection_chars
                    {
                        continue;
                    }
                    if let Some(app_name) = frontmost_app_name() {
                        if config
                            .general
                            .blacklist_apps
                            .iter()
                            .any(|b| app_name.to_lowercase().contains(&b.to_lowercase()))
                        {
                            continue;
                        }
                    }
                    pending = Some(sel);
                    last_change = Some(std::time::Instant::now());
                }
                Ok(selection::SelectionEvent::Cleared) => {
                    pending = None;
                    last_change = None;
                    *state.last_selection.lock().unwrap() = None;
                    windows::manager::hide_toolbar(&app);
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    // 防抖到期（安静满 DEBOUNCE 才弹出，FR-1.1 ≤300ms）
                    let quiet = last_change.is_some_and(|t| t.elapsed() >= DEBOUNCE);
                    if quiet {
                        last_change = None;
                        if let Some(sel) = pending.take() {
                            let anchor = sel.anchor;
                            *state.last_selection.lock().unwrap() = Some(sel);
                            let pref = match config.general.popup_position.as_str() {
                                "top-right" => PopupPosition::TopRight,
                                _ => PopupPosition::Below,
                            };
                            windows::manager::show_toolbar_at(&app, anchor, pref);
                        }
                    }
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
        }
    });
}

#[cfg(target_os = "macos")]
fn frontmost_app_name() -> Option<String> {
    selection::ax::frontmost_app_name()
}

#[cfg(windows)]
fn frontmost_app_name() -> Option<String> {
    selection::uia::frontmost_app_name()
}

#[cfg(not(any(target_os = "macos", windows)))]
fn frontmost_app_name() -> Option<String> {
    None
}

/// 按触发模式启动选区监听（FR-1.3）
fn start_selection_watchers(
    app: &tauri::AppHandle,
    tx: mpsc::Sender<selection::SelectionEvent>,
    auto: bool,
) {
    if !auto {
        log::info!("trigger_mode=shortcut：仅快捷键通道");
        return;
    }
    #[cfg(target_os = "macos")]
    {
        if !selection::ax::accessibility_granted() {
            // 未授权：系统已弹引导；降级为快捷键模式（§4.4）
            log::warn!("辅助功能未授权，降级为快捷键唤起模式");
            return;
        }
        selection::ax::start(tx);
    }
    #[cfg(windows)]
    {
        selection::uia::start(tx, app.clone());
    }
    #[cfg(not(any(target_os = "macos", windows)))]
    {
        let _ = tx;
        log::warn!("当前平台仅支持快捷键通道");
    }
}

/// 注册全局快捷键（FR-1.3 / FR-6.1）
fn register_shortcuts(
    app: &tauri::AppHandle,
    config: Arc<config::AppConfig>,
) -> Result<(), Box<dyn std::error::Error>> {
    use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

    let toggle = config.general.shortcut_toggle.clone();
    let settings = config.general.shortcut_settings.clone();

    app.global_shortcut()
        .on_shortcut(toggle.as_str(), move |app, _shortcut, event| {
            if event.state != ShortcutState::Pressed {
                return;
            }
            // 快捷键通道：读取剪贴板 → 弹出工具条（FR-1.3 shortcut 模式）
            if let Some(sel) = selection::clipboard::read_selection_from_clipboard(app) {
                let app2 = app.clone();
                let st = app2.state::<AppState>();
                let pref = match st.config.read().general.popup_position.as_str() {
                    "top-right" => PopupPosition::TopRight,
                    _ => PopupPosition::Below,
                };
                *st.last_selection.lock().unwrap() = Some(sel.clone());
                windows::manager::show_toolbar_at(app, sel.anchor, pref);
            }
        })?;

    app.global_shortcut()
        .on_shortcut(settings.as_str(), move |app, _shortcut, event| {
            if event.state != ShortcutState::Pressed {
                return;
            }
            windows::manager::show_window(
                app,
                "settings",
                windows::WindowKind::Settings,
                "settings.html",
            );
        })?;
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    env_logger::init();

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            // 单实例：重复启动时唤起已有实例（FR-7.3）
            if let Some(win) = app.get_webview_window("settings") {
                let _ = win.set_focus();
            }
        }))
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .setup(|app| {
            let config = Arc::new(config::load_or_create());
            let state = AppState::new((*config).clone());
            app.manage(state.clone());
            let state = Arc::new(state);

            // 写 pid 文件（CLI status / stop 用，FR-7.2）
            if let Some(pid_path) = dirs::runtime_dir()
                .or_else(|| Some(std::env::temp_dir()))
                .map(|d| d.join("wordpick.pid"))
            {
                let _ = std::fs::write(&pid_path, std::process::id().to_string());
            }

            // 系统托盘（FR-7.5）
            if let Err(e) = tray::setup_tray(app.handle()) {
                log::warn!("托盘初始化失败（无图标环境可忽略）: {e}");
            }

            // 事件通道 + 分发器
            let (tx, rx) = mpsc::channel();
            spawn_dispatcher(rx, app.handle().clone(), config.clone(), state);

            // 选区监听
            let auto = config.general.trigger_mode == "auto";
            start_selection_watchers(app.handle(), tx, auto);

            // 全局快捷键
            if let Err(e) = register_shortcuts(app.handle(), config) {
                log::error!("快捷键注册失败: {e}");
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::toolbar_action,
            commands::take_panel_context,
            commands::hide_toolbar,
            commands::open_settings,
            commands::get_config,
            commands::selection_cleared,
            commands::start_chat,
            commands::stop_chat,
            commands::test_connection,
            commands::build_first_messages,
            commands::set_panel_pinned,
            commands::error_kinds,
            commands::save_config,
            commands::save_api_key,
            commands::get_api_key_mask,
            commands::has_any_api_key,
            commands::fit_toolbar_window,
            commands::get_autostart,
            commands::set_autostart,
        ])
        .run(tauri::generate_context!())
        .expect("error while running WordPick");
}
