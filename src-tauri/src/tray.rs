//! 系统托盘菜单（FR-7.5）与开机自启（FR-7.4）。
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{Manager, Runtime};

use crate::windows::{manager, WindowKind};

/// 生成 32×32 纯色圆角方块托盘图标（占位，打包时由真实图标替换）
fn tray_icon() -> tauri::image::Image<'static> {
    const N: usize = 32;
    let mut rgba = vec![0u8; N * N * 4];
    for y in 0..N {
        for x in 0..N {
            let corner = x < 6 && y < 6 || x >= N - 6 && y < 6 || x < 6 && y >= N - 6 || x >= N - 6 && y >= N - 6;
            if corner {
                continue; // 圆角透明
            }
            let i = (y * N + x) * 4;
            rgba[i] = 56; // WordPick 主题蓝
            rgba[i + 1] = 132;
            rgba[i + 2] = 255;
            rgba[i + 3] = 255;
        }
    }
    tauri::image::Image::from_rgba(&rgba, N as u32, N as u32).expect("tray icon")
}

pub fn setup_tray<R: Runtime>(app: &tauri::AppHandle<R>) -> tauri::Result<()> {
    let settings = MenuItem::with_id(app, "settings", "设置…", true, Some("CmdOrCtrl+,"))?;
    let autostart = CheckMenuItem::with_id(app, "autostart", "开机自启", true, true, None)?;
    let show = MenuItem::with_id(app, "show", "显示工具条", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&settings, &autostart, &show, &PredefinedMenuItem::separator(app)?, &quit])?;

    let autostart_enabled = app
        .autolaunch()
        .map(|a| a.is_enabled().unwrap_or(false))
        .unwrap_or(false);
    let _ = autostart.set_checked(autostart_enabled);

    TrayIconBuilder::new()
        .icon(tray_icon())
        .menu(&menu)
        .on_menu_event(move |app, event| match event.id().as_ref() {
            "settings" => manager::show_window(app, "settings", WindowKind::Settings, "settings.html"),
            "show" => {
                // 唤起工具条显示（配合快捷键读取剪贴板路径在 lib.rs 注册）
                if let Some(sel) = crate::selection::clipboard::read_selection_from_clipboard(app) {
                    let st = app.state::<crate::state::AppState>();
                    let pref = match st.config.read().general.popup_position.as_str() {
                        "top-right" => crate::windows::position::PopupPosition::TopRight,
                        _ => crate::windows::position::PopupPosition::Below,
                    };
                    *st.last_selection.lock().unwrap() = Some(sel.clone());
                    manager::show_toolbar_at(app, sel.anchor, pref);
                }
            }
            "autostart" => {
                if let Ok(auto) = app.autolaunch() {
                    let target = !auto.is_enabled().unwrap_or(false);
                    let _ = if target { auto.enable() } else { auto.disable() };
                }
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .build(app)?;
    Ok(())
}
