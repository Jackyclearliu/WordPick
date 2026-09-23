//! Tauri commands：UI 层与 Rust 核心层之间的唯一通道（NFR 4.5 解耦）
use tauri::{Manager, State};
use tauri_plugin_autostart::ManagerExt;

use crate::llm::adapter::Scenario;
use crate::state::{AppState, PanelContext};
use crate::windows::{manager, WindowKind};

/// 工具条功能按钮点击（FR-2.1）
#[tauri::command]
pub fn toolbar_action(action: String, app: tauri::AppHandle, state: State<'_, AppState>) {
    match action.as_str() {
        "translate" | "explain" => {
            let scenario = if action == "translate" {
                Scenario::Translate
            } else {
                Scenario::Explain
            };
            let selection = state.last_selection.lock().unwrap().clone();
            if let Some(selection) = selection {
                *state.pending_panel.lock().unwrap() = Some(PanelContext {
                    selection: selection.clone(),
                    scenario,
                });
                let pref = match state.config.read().general.popup_position.as_str() {
                    "top-right" => crate::windows::position::PopupPosition::TopRight,
                    _ => crate::windows::position::PopupPosition::Below,
                };
                manager::show_window(&app, "panel", WindowKind::Panel, "panel.html");
                place_panel_near_selection(&app, selection.anchor, pref);
            }
        }
        "settings" => open_settings(app, state),
        _ => {}
    }
}

/// 将对话框定位到选区旁（FR-3.1：选区下方/右侧可配置，空间不足自动翻转）
pub fn place_panel_near_selection(
    app: &tauri::AppHandle,
    anchor: Option<(i32, i32)>,
    pref: crate::windows::position::PopupPosition,
) {
    use crate::windows::position::locate;
    use tauri::PhysicalPosition;
    let Some(anchor) = anchor else { return };
    let Some(win) = app.get_webview_window("panel") else {
        return;
    };
    let (workarea, scale) = app
        .monitor_from_point(anchor.0 as f64, anchor.1 as f64)
        .ok()
        .flatten()
        .map(|m| {
            (
                crate::windows::position::Rect {
                    x: m.position().x,
                    y: m.position().y,
                    w: m.size().width as i32,
                    h: m.size().height as i32,
                },
                m.scale_factor(),
            )
        })
        .unwrap_or((
            crate::windows::position::Rect {
                x: 0,
                y: 0,
                w: 1920,
                h: 1080,
            },
            1.0,
        ));
    let size = win
        .inner_size()
        .unwrap_or(tauri::PhysicalSize::new(420, 540));
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
}

/// 面板 on mount 时取走上下文
#[tauri::command]
pub fn take_panel_context(state: State<'_, AppState>) -> Option<PanelContext> {
    state.pending_panel.lock().unwrap().take()
}

/// 隐藏工具条（Esc / 点击外部）
#[tauri::command]
pub fn hide_toolbar(app: tauri::AppHandle) {
    if let Some(win) = app.get_webview_window("toolbar") {
        let _ = win.hide();
    }
}

#[tauri::command]
pub fn open_settings(app: tauri::AppHandle, _state: State<'_, AppState>) {
    manager::show_window(&app, "settings", WindowKind::Settings, "settings.html");
}

/// UI 读取当前配置（设置页初始化用，FR-6）
#[tauri::command]
pub fn get_config(state: State<'_, AppState>) -> crate::config::AppConfig {
    state.config.read().clone()
}

// ---------------------------------------------------------------------------
// Phase 2：模型适配层命令（§5.5）
// ---------------------------------------------------------------------------

use crate::llm::adapter::{self, ChatMessage, ChatRequest, ErrorKind};

/// 发起流式对话（panel 调用；SSE chunk 经 wp://chat-chunk 事件回推）
#[tauri::command]
pub async fn start_chat(app: tauri::AppHandle, req: ChatRequest) -> Result<(), String> {
    tauri::async_runtime::spawn(adapter::chat_stream(app, req));
    Ok(())
}

/// 停止指定请求的流式输出（FR-5.4 停止按钮）
#[tauri::command]
pub fn stop_chat(request_id: String) {
    adapter::stop(&request_id);
}

/// 连通性测试（FR-6.5）
#[tauri::command]
pub async fn test_connection(
    provider: crate::config::ProviderConfig,
    model: String,
) -> Result<u128, String> {
    adapter::test_connection(&provider, &model)
        .await
        .map_err(|(kind, msg)| format!("{:?}: {msg}", kind))
}

/// 组装首轮消息：system prompt + 原文（UI 语言检测后调用）
#[tauri::command]
pub fn build_first_messages(
    scenario: adapter::Scenario,
    source_lang: String,
    target_lang: String,
    text: String,
) -> Vec<ChatMessage> {
    vec![
        ChatMessage {
            role: "system".into(),
            content: adapter::system_prompt(scenario, &source_lang, &target_lang),
        },
        ChatMessage {
            role: "user".into(),
            content: text,
        },
    ]
}

/// 面板钉住/取消钉住（FR-5.1）
#[tauri::command]
pub fn set_panel_pinned(app: tauri::AppHandle, pinned: bool) -> Result<(), String> {
    let win = app
        .get_webview_window("panel")
        .ok_or_else(|| "panel window not found".to_string())?;
    win.set_always_on_top(pinned).map_err(|e| e.to_string())?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Phase 4：设置与配置系统命令（FR-6）
// ---------------------------------------------------------------------------

use crate::config::AppConfig;

/// 保存配置：写入用户 config.toml 并热更新内存态（设置 ⇄ 文件双向同步，FR-6.2）
#[tauri::command]
pub fn save_config(config: AppConfig, state: State<'_, AppState>) -> Result<(), String> {
    let path = crate::config::user_config_path().ok_or("无法确定用户配置目录")?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let text = toml::to_string_pretty(&config).map_err(|e| e.to_string())?;
    std::fs::write(&path, text).map_err(|e| e.to_string())?;
    *state.config.write() = config;
    Ok(())
}

/// 保存 API Key 到系统安全存储（keychain，FR-6.3）
#[tauri::command]
pub fn save_api_key(provider: String, key: String) -> Result<(), String> {
    crate::secure::store_key(&provider, &key).map_err(|e| e.to_string())
}

/// 读取 Key 掩码（仅后 4 位；任何界面不回显完整 Key，FR-6.3）
/// 优先级：keychain > 配置明文 / ${ENV} 解析
#[tauri::command]
pub fn get_api_key_mask(provider: String, state: State<'_, AppState>) -> Option<String> {
    if let Ok(key) = crate::secure::read_key(&provider) {
        if !key.is_empty() {
            return Some(crate::secure::mask_key(&key));
        }
    }
    let cfg = state.config.read();
    let p = cfg.providers.iter().find(|p| p.name == provider)?;
    let key = crate::config::resolve_env_placeholder(&p.api_key);
    if key.is_empty() {
        None
    } else {
        Some(crate::secure::mask_key(&key))
    }
}

/// 是否已有任一可用 Key（Onboarding 判断，FR-6.4）
#[tauri::command]
pub fn has_any_api_key(state: State<'_, AppState>) -> bool {
    let cfg = state.config.read();
    cfg.providers.iter().any(|p| {
        crate::secure::read_key(&p.name)
            .map(|k| !k.is_empty())
            .unwrap_or(false)
            || !crate::config::resolve_env_placeholder(&p.api_key).is_empty()
    })
}

/// 工具条内容自适应尺寸（窗口复用池 + 内容尺寸上报）
#[tauri::command]
pub fn fit_toolbar_window(app: tauri::AppHandle, width: f64, height: f64) -> Result<(), String> {
    let win = app
        .get_webview_window("toolbar")
        .ok_or_else(|| "toolbar window not found".to_string())?;
    win.set_size(tauri::Size::Logical(tauri::LogicalSize::new(width, height)))
        .map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// Phase 5：开机自启（FR-7.4；macOS Login Items/LaunchAgent，Windows 注册表 Run 键）
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn get_autostart(app: tauri::AppHandle) -> bool {
    app.autolaunch().is_enabled().unwrap_or(false)
}

#[tauri::command]
pub fn set_autostart(app: tauri::AppHandle, enabled: bool) -> Result<(), String> {
    let auto = app.autolaunch();
    let result = if enabled {
        auto.enable()
    } else {
        auto.disable()
    };
    result.map_err(|e| e.to_string())
}

/// 错误类型导出（前端按类型渲染友好错误与设置入口，FR-5.4）
#[tauri::command]
pub fn error_kinds() -> Vec<ErrorKind> {
    vec![
        ErrorKind::Network,
        ErrorKind::RateLimited,
        ErrorKind::InvalidKey,
        ErrorKind::BadRequest,
        ErrorKind::Server,
        ErrorKind::Unknown,
    ]
}

/// 选区取消（前端检测到选区失效时主动通知，FR-1.2 补充通道）
#[tauri::command]
pub fn selection_cleared(app: tauri::AppHandle, state: State<'_, AppState>) {
    *state.last_selection.lock().unwrap() = None;
    hide_toolbar(app);
}
