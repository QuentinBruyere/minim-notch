use tauri::{
    AppHandle, Manager,
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
};

use crate::{NOTCH_LABEL, t};

const TRAY_ID: &str = "main";

/// Tray menu: quick actions only, everything else lives in the settings window.
fn menu(app: &AppHandle) -> tauri::Result<Menu<tauri::Wry>> {
    let item = |id: &str, text: String| MenuItem::with_id(app, id, text, true, None::<&str>);
    let menu = Menu::with_items(
        app,
        &[
            &item("settings", t!("tray.settings"))?,
            &item("toggle", t!("tray.toggle"))?,
            &PredefinedMenuItem::separator(app)?,
            &item("quit", t!("tray.quit"))?,
        ],
    )?;
    // An update found (ADR-0013): shown in the settings.
    if let Some(version) = crate::updates::available_version(app) {
        menu.prepend(&item("update", t!("tray.update", version = version))?)?;
    }
    Ok(menu)
}

/// Rebuilds the menu in the current language (ADR-0012).
pub fn retranslate(app: &AppHandle) -> tauri::Result<()> {
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        tray.set_menu(Some(menu(app)?))?;
    }
    Ok(())
}

pub fn create(app: &AppHandle) -> tauri::Result<()> {
    TrayIconBuilder::with_id(TRAY_ID)
        .icon(
            app.default_window_icon()
                .cloned()
                .expect("bundle icon is configured"),
        )
        .tooltip("Minim Notch")
        .menu(&menu(app)?)
        .show_menu_on_left_click(false)
        // Left click opens the settings, right click shows the menu.
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
                && let Err(e) = crate::settings::open(tray.app_handle())
            {
                log::error!("cannot open settings: {e}");
            }
        })
        .on_menu_event(|app, event| match event.id.as_ref() {
            "settings" | "update" => {
                if let Err(e) = crate::settings::open(app) {
                    log::error!("cannot open settings: {e}");
                }
            }
            "toggle" => {
                let visible = app
                    .get_webview_window(NOTCH_LABEL)
                    .and_then(|w| w.is_visible().ok())
                    .unwrap_or(false);
                crate::notch::set_user_visible(app, !visible);
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .build(app)?;
    Ok(())
}
