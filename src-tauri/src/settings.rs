//! Settings window and the commands it calls, see
//! docs/fonctionnel/DF-0005-fenetre-de-parametres.md. Every change is applied
//! immediately and saved to config.json.

use std::sync::atomic::Ordering;

use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager, Theme, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_autostart::ManagerExt;

use crate::module::Host;
use crate::placement::{self, Edge, GAP_RANGE, Style};
use crate::resistance::Strength;
use crate::{AppState, config, notch, platform};

pub const SETTINGS_LABEL: &str = "settings";

/// A monitor the notch can go on.
#[derive(Clone, Serialize)]
pub struct ScreenChoice {
    id: String,
    label: String,
    primary: bool,
}

/// A module and its section of the settings window (ADR-0009).
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModuleInfo {
    id: String,
    name: String,
    description: String,
    enabled: bool,
    /// Pinned next to the notch, and on which side (DF-0012).
    pin: Option<crate::config::PinSide>,
    /// Can be pinned: enabled and not the first module (the notch itself).
    pinnable: bool,
    /// The module's own data, read by its settings component.
    settings: Value,
}

/// Everything the settings window displays.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    sound_enabled: bool,
    hide_in_fullscreen: bool,
    edge: Edge,
    /// Notch or pill, and the pill's distance to the edge (ADR-0003).
    style: Style,
    gap: u32,
    /// Chosen screen, `None` = the primary one.
    screen: Option<String>,
    screens: Vec<ScreenChoice>,
    /// Move mode, see `notch::set_movable`.
    movable: bool,
    /// The notch is not centered on its edge.
    off_center: bool,
    cursor_resistance: bool,
    resistance_strength: Strength,
    /// Cursor resistance only exists on Windows for now.
    resistance_available: bool,
    autostart: bool,
    modules: Vec<ModuleInfo>,
    module_layout: crate::config::ModuleLayout,
    notch_speed: crate::config::NotchSpeed,
    appearance: crate::config::Appearance,
    auto_hide: bool,
    auto_hide_delay_ms: u32,
    pin_flares: bool,
    notch_color: Option<String>,
    notch_opacity: u8,
    notch_grain: u8,
    /// Chosen language code, `None` = the system's (ADR-0012).
    language: Option<String>,
    /// Translated languages: code and own name.
    languages: Vec<(&'static str, &'static str)>,
    /// The language shown, the system's resolved.
    shown_language: &'static str,
    config_dir: String,
    /// The log file (ADR-0014).
    log_file: String,
    /// The previous run's crash, until the user dismisses it.
    crash: Option<crate::diagnostics::Crash>,
    /// Automatic update checks (ADR-0013).
    check_updates: bool,
    /// False in the development version, which never checks.
    updates_enabled: bool,
    update: crate::updates::Status,
}

pub fn current(app: &AppHandle) -> Settings {
    let state = app.state::<AppState>();
    let config = state.config.lock().unwrap().clone();
    // The first enabled module is the notch itself: it cannot be pinned.
    let first_enabled =
        crate::module::first_enabled(&state.modules, &config).map(|m| m.id().to_string());
    Settings {
        sound_enabled: config.sound_enabled,
        hide_in_fullscreen: config.hide_in_fullscreen,
        edge: config.placement.edge,
        style: config.placement.style,
        gap: config.placement.gap,
        screen: config.placement.screen.clone(),
        screens: screen_choices(app),
        movable: state.movable.load(Ordering::Relaxed),
        off_center: config.placement.offset != 0.5,
        cursor_resistance: config.cursor_resistance,
        resistance_strength: config.cursor_resistance_strength,
        resistance_available: cfg!(windows),
        autostart: app.autolaunch().is_enabled().unwrap_or(false),
        modules: crate::module::ordered(&state.modules, &config.module_order)
            .into_iter()
            .map(|m| {
                let enabled = crate::module::is_enabled(m, &config);
                ModuleInfo {
                    id: m.id().into(),
                    name: m.name(),
                    description: m.description(),
                    enabled,
                    pin: config.pins.get(m.id()).copied(),
                    pinnable: enabled && !m.blank() && first_enabled.as_deref() != Some(m.id()),
                    settings: if enabled { m.settings() } else { Value::Null },
                }
            })
            .collect(),
        module_layout: config.module_layout,
        notch_speed: config.notch_speed,
        appearance: config.appearance,
        auto_hide: config.auto_hide,
        auto_hide_delay_ms: config.auto_hide_delay_ms,
        pin_flares: config.pin_flares,
        notch_color: config.notch_color.clone(),
        notch_opacity: config.notch_opacity,
        notch_grain: config.notch_grain,
        language: config.language.clone(),
        languages: crate::i18n::LANGUAGES.to_vec(),
        shown_language: crate::i18n::language(),
        config_dir: state.config_dir.display().to_string(),
        log_file: crate::diagnostics::log_file(&state.log_dir)
            .display()
            .to_string(),
        crash: state.crash.lock().unwrap().clone(),
        check_updates: config.check_updates,
        updates_enabled: crate::updates::enabled(),
        update: crate::updates::status(app),
    }
}

fn screen_choices(app: &AppHandle) -> Vec<ScreenChoice> {
    let screens = crate::notch::screens(app).unwrap_or_default();
    placement::screen_labels(&screens)
        .into_iter()
        .map(|(id, label)| ScreenChoice {
            primary: screens.iter().any(|s| s.id == id && s.primary),
            id,
            label,
        })
        .collect()
}

/// Opens the settings window, or brings it back to the front.
pub fn open(app: &AppHandle) -> tauri::Result<()> {
    open_window(app, true)
}

/// Opens the settings window without taking the focus: Minim Notch opens it by
/// itself (a crash to report, ADR-0014), the user is busy elsewhere.
pub fn open_in_background(app: &AppHandle) -> tauri::Result<()> {
    open_window(app, false)
}

fn open_window(app: &AppHandle, focus: bool) -> tauri::Result<()> {
    if let Some(window) = app.get_webview_window(SETTINGS_LABEL) {
        window.unminimize()?;
        window.show()?;
        return if focus {
            platform::bring_to_front(&window)
        } else {
            Ok(())
        };
    }
    let mut builder =
        WebviewWindowBuilder::new(app, SETTINGS_LABEL, WebviewUrl::App("index.html".into()))
            .title(crate::t!("settings.window_title"))
            .inner_size(760.0, 560.0)
            .min_inner_size(600.0, 420.0)
            .center()
            .theme(window_theme(app))
            .focused(focus);
    // All windows share one WebView2, which refuses a window with other
    // browser arguments: the notch's, from tauri.conf.json.
    if let Some(args) = app
        .config()
        .app
        .windows
        .iter()
        .find_map(|w| w.additional_browser_args.clone())
    {
        builder = builder.additional_browser_args(&args);
    }
    let window = builder.build()?;
    // Move mode only lasts while the settings are open.
    let handle = app.clone();
    window.on_window_event(move |event| {
        if matches!(event, tauri::WindowEvent::Destroyed) {
            notch::set_movable(&handle, false);
        }
    });
    if focus {
        platform::bring_to_front(&window)
    } else {
        Ok(())
    }
}

/// Tells every window (notch included) that settings changed.
pub fn changed(app: &AppHandle) {
    let _ = app.emit("settings-changed", current(app));
    crate::emit_status(app);
}

pub(crate) fn update_config(app: &AppHandle, edit: impl FnOnce(&mut config::Config)) {
    let state = app.state::<AppState>();
    let mut config = state.config.lock().unwrap();
    edit(&mut config);
    if let Err(e) = config::save(&state.config_dir, &config) {
        log::error!("cannot save config: {e}");
    }
}

#[tauri::command]
pub fn get_settings(app: AppHandle) -> Settings {
    current(&app)
}

#[tauri::command]
pub fn set_sound(app: AppHandle, enabled: bool) {
    update_config(&app, |c| c.sound_enabled = enabled);
    changed(&app);
}

/// Moves the notch to another screen edge, centered on it (DF-0006).
#[tauri::command]
pub fn set_edge(app: AppHandle, edge: Edge) -> Result<(), String> {
    update_config(&app, |c| {
        c.placement.edge = edge;
        c.placement.offset = 0.5;
    });
    notch::reset_hit_area();
    if let Some(window) = app.get_webview_window(crate::NOTCH_LABEL) {
        notch::place(&window)
            .and_then(|_| notch::apply_hit_area(&window))
            .map_err(|e| e.to_string())?;
    }
    changed(&app);
    Ok(())
}

/// Notch glued to the edge, or pill detached from it (ADR-0003).
#[tauri::command]
pub fn set_style(app: AppHandle, style: Style) -> Result<(), String> {
    update_config(&app, |c| c.placement.style = style);
    notch::reset_hit_area();
    replace(&app)
}

/// Distance between the pill and its edge, in logical pixels.
#[tauri::command]
pub fn set_gap(app: AppHandle, gap: u32) -> Result<(), String> {
    if !GAP_RANGE.contains(&gap) {
        return Err(crate::t!(
            "settings.gap_range",
            min = GAP_RANGE.start(),
            max = GAP_RANGE.end()
        ));
    }
    update_config(&app, |c| c.placement.gap = gap);
    replace(&app)
}

/// Puts the notch window back where the placement says, with its shape.
fn replace(app: &AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window(crate::NOTCH_LABEL) {
        notch::place(&window)
            .and_then(|_| notch::apply_hit_area(&window))
            .map_err(|e| e.to_string())?;
    }
    changed(app);
    Ok(())
}

/// Moves the notch to another monitor, `None` = the primary one (DF-0006).
/// An unplugged monitor falls back to the primary one until it comes back.
#[tauri::command]
pub fn set_screen(app: AppHandle, screen: Option<String>) -> Result<(), String> {
    update_config(&app, |c| c.placement.screen = screen);
    if let Some(window) = app.get_webview_window(crate::NOTCH_LABEL) {
        notch::place(&window)
            .and_then(|_| notch::apply_hit_area(&window))
            .map_err(|e| e.to_string())?;
    }
    changed(&app);
    Ok(())
}

/// Move mode: the notch can be dragged along its edge while the settings
/// are open (DF-0006, step 3).
#[tauri::command]
pub fn set_movable(app: AppHandle, movable: bool) {
    notch::set_movable(&app, movable);
    changed(&app);
}

/// Puts the notch back in the middle of its edge.
#[tauri::command]
pub fn recenter(app: AppHandle) -> Result<(), String> {
    update_config(&app, |c| c.placement.offset = 0.5);
    if let Some(window) = app.get_webview_window(crate::NOTCH_LABEL) {
        notch::place(&window)
            .and_then(|_| notch::apply_hit_area(&window))
            .map_err(|e| e.to_string())?;
    }
    changed(&app);
    Ok(())
}

/// Automatic update checks; "Check now" works either way (ADR-0013).
#[tauri::command]
pub fn set_check_updates(app: AppHandle, enabled: bool) {
    update_config(&app, |c| c.check_updates = enabled);
    changed(&app);
}

/// Applied by the watcher within a second (DF-0002).
#[tauri::command]
pub fn set_hide_in_fullscreen(app: AppHandle, enabled: bool) {
    update_config(&app, |c| c.hide_in_fullscreen = enabled);
    changed(&app);
}

#[tauri::command]
pub fn set_cursor_resistance(app: AppHandle, enabled: bool, strength: Strength) {
    update_config(&app, |c| {
        c.cursor_resistance = enabled;
        c.cursor_resistance_strength = strength;
    });
    platform::set_resistance_strength(strength);
    notch::update_cursor_resistance(&app);
    changed(&app);
}

#[tauri::command]
pub fn set_autostart(app: AppHandle, enabled: bool) -> Result<(), String> {
    let launcher = app.autolaunch();
    let result = if enabled {
        launcher.enable()
    } else {
        launcher.disable()
    };
    changed(&app);
    result.map_err(|e| e.to_string())
}

/// Turns a module on or off; a disabled module does not run at all.
#[tauri::command]
pub fn set_module_enabled(app: AppHandle, id: String, enabled: bool) -> Result<(), String> {
    let state = app.state::<AppState>();
    let module = state
        .modules
        .iter()
        .find(|m| m.id() == id)
        .ok_or_else(|| crate::t!("settings.unknown_module", id = id))?;
    let was_enabled = crate::module::is_enabled(module.as_ref(), &state.config.lock().unwrap());
    if was_enabled != enabled {
        update_config(&app, |c| {
            c.modules.entry(id.clone()).or_default().enabled = Some(enabled)
        });
        if enabled {
            module.start(&Host::new(app.clone(), module.id()));
        } else {
            module.stop();
        }
    }
    changed(&app);
    crate::emit_content(&app);
    Ok(())
}

/// The user reordered the modules (DF-0011): `order` lists module ids, the
/// first one closest to the edge. Unknown ids are dropped.
#[tauri::command]
pub fn set_module_order(app: AppHandle, order: Vec<String>) {
    let known: Vec<String> = {
        let state = app.state::<AppState>();
        order
            .into_iter()
            .filter(|id| state.modules.iter().any(|m| m.id() == id))
            .collect()
    };
    update_config(&app, |c| c.module_order = known);
    changed(&app);
    crate::emit_content(&app);
}

/// Modules joined in the notch or one card each (DF-0011).
#[tauri::command]
pub fn set_module_layout(app: AppHandle, layout: crate::config::ModuleLayout) {
    update_config(&app, |c| c.module_layout = layout);
    changed(&app);
    crate::emit_status(&app);
}

/// Pins a module next to the notch on a side, or unpins it with `None`
/// (DF-0012).
#[tauri::command]
pub fn set_module_pin(app: AppHandle, id: String, pin: Option<crate::config::PinSide>) {
    update_config(&app, |c| match pin {
        Some(side) => {
            c.pins.insert(id, side);
        }
        None => {
            c.pins.remove(&id);
        }
    });
    changed(&app);
    crate::emit_content(&app);
}

/// How fast the notch opens and closes.
#[tauri::command]
pub fn set_notch_speed(app: AppHandle, speed: crate::config::NotchSpeed) {
    update_config(&app, |c| c.notch_speed = speed);
    changed(&app);
    crate::emit_status(&app);
}

/// Light, dark, or the Windows app mode (DF-0014).
#[tauri::command]
pub fn set_appearance(app: AppHandle, appearance: crate::config::Appearance) {
    update_config(&app, |c| c.appearance = appearance);
    // The pages restyle themselves from the setting; the settings window's
    // title bar follows the window theme.
    if let Some(window) = app.get_webview_window(SETTINGS_LABEL)
        && let Err(e) = window.set_theme(window_theme(&app))
    {
        log::warn!("cannot set the settings window theme: {e}");
    }
    changed(&app);
}

/// Tucks the notch into the edge when unused (DF-0018).
#[tauri::command]
pub fn set_auto_hide(app: AppHandle, enabled: bool) {
    update_config(&app, |c| c.auto_hide = enabled);
    changed(&app);
}

/// Pins curve outwards where they meet the edge, like the notch (DF-0028).
#[tauri::command]
pub fn set_pin_flares(app: AppHandle, enabled: bool) {
    update_config(&app, |c| c.pin_flares = enabled);
    changed(&app);
}

/// How long after the cursor left before the notch tucks, in ms.
#[tauri::command]
pub fn set_auto_hide_delay(app: AppHandle, ms: u32) {
    update_config(&app, |c| c.auto_hide_delay_ms = ms.min(5000));
    changed(&app);
}

/// The notch's own color, `#rrggbb`, or `None` for the theme's (DF-0019).
#[tauri::command]
pub fn set_notch_color(app: AppHandle, color: Option<String>) -> Result<(), String> {
    let color = match color {
        Some(value) => {
            Some(config::color(&value).ok_or_else(|| crate::t!("settings.color.invalid"))?)
        }
        None => None,
    };
    update_config(&app, |c| c.notch_color = color);
    changed(&app);
    Ok(())
}

/// How opaque the notch's background is, 0 to 100 (DF-0019).
#[tauri::command]
pub fn set_notch_opacity(app: AppHandle, opacity: u8) {
    update_config(&app, |c| c.notch_opacity = opacity.min(100));
    changed(&app);
}

/// Grain over the notch's background, 0 (none) to `config::MAX_GRAIN` (DF-0019).
#[tauri::command]
pub fn set_notch_grain(app: AppHandle, grain: u8) {
    update_config(&app, |c| c.notch_grain = grain.min(config::MAX_GRAIN));
    changed(&app);
}

/// A language code, or `None` for the system's (DF-0015). Applies at once:
/// the pages, the notch's items, the tray menu and this window's title.
#[tauri::command]
pub fn set_language(app: AppHandle, language: Option<String>) {
    update_config(&app, |c| c.language = language.clone());
    crate::i18n::set_language(crate::i18n::resolve(language.as_deref()));
    if let Err(e) = crate::tray::retranslate(&app) {
        log::warn!("cannot translate the tray menu: {e}");
    }
    if let Some(window) = app.get_webview_window(SETTINGS_LABEL)
        && let Err(e) = window.set_title(&crate::t!("settings.window_title"))
    {
        log::warn!("cannot translate the settings window title: {e}");
    }
    changed(&app);
    crate::emit_content(&app);
}

/// The settings window's theme; `None` follows Windows.
fn window_theme(app: &AppHandle) -> Option<Theme> {
    match app.state::<AppState>().config.lock().unwrap().appearance {
        crate::config::Appearance::System => None,
        crate::config::Appearance::Light => Some(Theme::Light),
        crate::config::Appearance::Dark => Some(Theme::Dark),
    }
}

/// Action of a module's settings section, handled by the module itself.
#[tauri::command]
pub fn module_call(
    app: AppHandle,
    id: String,
    action: String,
    args: Option<Value>,
) -> Result<Value, String> {
    let state = app.state::<AppState>();
    let module = state
        .modules
        .iter()
        .find(|m| m.id() == id)
        .ok_or_else(|| crate::t!("settings.unknown_module", id = id))?;
    if !crate::module::is_enabled(module.as_ref(), &state.config.lock().unwrap()) {
        return Err(crate::t!("settings.module_disabled", name = module.name()));
    }
    module.call(&action, args.unwrap_or(Value::Null))
}

/// Web pages the settings may open, by name: the front never passes a URL.
const LINKS: [(&str, &str); 2] = [
    ("website", "https://minim-notch.com"),
    (
        "issues",
        "https://github.com/QuentinBruyere/minim-notch/issues",
    ),
];

/// Opens one of `LINKS` in the default browser.
#[tauri::command]
pub fn open_link(link: String) -> Result<(), String> {
    let (_, url) = LINKS
        .iter()
        .find(|(name, _)| *name == link)
        .ok_or_else(|| crate::t!("settings.help.open_failed"))?;
    tauri_plugin_opener::open_url(*url, None::<&str>).map_err(|e| {
        log::warn!("cannot open {url}: {e}");
        crate::t!("settings.help.open_failed")
    })
}
