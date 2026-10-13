pub mod companion;
mod config;
mod context_menu;
mod diagnostics;
pub mod i18n;
pub mod module;
pub mod modules;
mod notch;
mod placement;
mod platform;
mod resistance;
mod settings;
mod tray;
mod updates;

use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, WindowEvent};

use config::Config;
use module::{Content, Host, Module};

pub const NOTCH_LABEL: &str = "notch";

pub struct AppState {
    pub modules: Vec<Box<dyn Module>>,
    pub config: Mutex<Config>,
    pub config_dir: PathBuf,
    /// Hidden from the tray menu: fullscreen detection must not show it back.
    pub user_hidden: AtomicBool,
    /// Move mode, on while the settings ask for it (DF-0006, step 3).
    pub movable: AtomicBool,
    /// Where the log file and the crash marker live (ADR-0014).
    pub log_dir: PathBuf,
    /// The previous run's crash, shown in the settings until dismissed.
    pub crash: Mutex<Option<diagnostics::Crash>>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    sound_enabled: bool,
    /// Screen edge the notch is attached to: drives its orientation.
    edge: placement::Edge,
    /// Notch glued to the edge, or pill detached from it: drives its corners.
    style: placement::Style,
    /// Move mode: the notch can be dragged along its edge.
    movable: bool,
    /// Modules in the notch itself, or one card each (DF-0011).
    layout: config::ModuleLayout,
    speed: config::NotchSpeed,
    /// Light or dark (DF-0014), applied by the front.
    appearance: config::Appearance,
    /// The language shown (ADR-0012), the system's already resolved.
    language: &'static str,
    /// Tucks into the edge when unused, and after how long (DF-0018).
    auto_hide: bool,
    auto_hide_delay_ms: u32,
    /// Pins curve outwards where they meet the edge (DF-0028).
    pin_flares: bool,
    /// The notch's own color (DF-0019); `None` = the theme's.
    notch_color: Option<String>,
    notch_opacity: u8,
    notch_grain: u8,
    /// Center of the compact notch along the edge, from the start of the
    /// window, in logical pixels: every shape is centered on it, then kept
    /// inside the window. `None` = the middle of the window.
    anchor: Option<f64>,
}

#[tauri::command]
fn get_content(state: tauri::State<AppState>) -> Content {
    content(&state)
}

#[tauri::command]
fn get_status(state: tauri::State<AppState>) -> Status {
    status(&state)
}

/// Clicking the notch means the user has seen what it shows: one of a
/// module's items (`<module id>:<item id>`) if the click was on it, else the
/// whole module.
#[tauri::command]
fn acknowledge(state: tauri::State<AppState>, module: String, item: Option<String>) {
    let config = state.config.lock().unwrap().clone();
    let item = item
        .as_deref()
        .and_then(|item| item.split_once(':'))
        .filter(|(owner, _)| *owner == module)
        .map(|(_, id)| id);
    for m in state
        .modules
        .iter()
        .filter(|m| m.id() == module && module::is_enabled(m.as_ref(), &config))
    {
        match item {
            Some(item) => m.acknowledge_item(item),
            None => m.acknowledge(),
        }
    }
}

/// A button of an item in the open notch; `item` is `<module id>:<item id>`.
#[tauri::command]
fn item_action(state: tauri::State<AppState>, item: String, action: String) {
    let Some((module_id, item_id)) = item.split_once(':') else {
        return;
    };
    let config = state.config.lock().unwrap().clone();
    if let Some(module) = state
        .modules
        .iter()
        .find(|m| m.id() == module_id && module::is_enabled(m.as_ref(), &config))
    {
        module.item_action(item_id, &action);
    }
}

/// The front decides the visible shapes: the notch first, compact or
/// expanded (DF-0003), then the other cards (DF-0011) and pins (DF-0012).
#[tauri::command]
fn set_hit_area(window: tauri::WebviewWindow, shapes: Vec<notch::Shape>) -> Result<(), String> {
    notch::set_hit_area(&window, shapes).map_err(|e| e.to_string())
}

/// The cursor left an open shape (`area`, logical pixels in the window): it
/// closes once the cursor is also past the safety margin around it, or not
/// if the cursor comes back first.
#[tauri::command]
fn watch_leave(window: tauri::WebviewWindow, area: notch::Bounds) -> Result<(), String> {
    notch::watch_leave(&window, area).map_err(|e| e.to_string())
}

/// The cursor is back on the notch: stops `watch_leave`.
#[tauri::command]
fn cancel_leave() {
    notch::cancel_leave();
}

/// Dragging the notch in move mode (DF-0006, step 3).
#[tauri::command]
fn start_drag(window: tauri::WebviewWindow) -> Result<(), String> {
    notch::start_drag(&window).map_err(|e| e.to_string())
}

#[tauri::command]
fn drag(window: tauri::WebviewWindow) -> Result<(), String> {
    notch::drag(&window).map_err(|e| e.to_string())
}

#[tauri::command]
fn end_drag(app: AppHandle) {
    notch::end_drag(&app);
}

fn status(state: &AppState) -> Status {
    // Lock once: temporaries live until the end of the statement, so two
    // `config.lock()` in this struct literal would deadlock the main thread.
    let config = state.config.lock().unwrap().clone();
    Status {
        sound_enabled: config.sound_enabled,
        edge: config.placement.edge,
        style: config.placement.style,
        movable: state.movable.load(Ordering::Relaxed),
        layout: config.module_layout,
        speed: config.notch_speed,
        appearance: config.appearance,
        language: i18n::language(),
        auto_hide: config.auto_hide,
        auto_hide_delay_ms: config.auto_hide_delay_ms,
        pin_flares: config.pin_flares,
        notch_color: config.notch_color.clone(),
        notch_opacity: config.notch_opacity,
        notch_grain: config.notch_grain,
        anchor: notch::anchor(),
    }
}

/// The front tucked the notch into the edge, or brought it out (DF-0018).
#[tauri::command]
fn set_tucked(app: AppHandle, tucked: bool) {
    notch::set_tucked(&app, tucked);
}

/// The front starts or ends typing in the notch (a timer's duration).
#[tauri::command]
fn set_typing(app: AppHandle, typing: bool) {
    notch::set_typing(&app, typing);
}

fn content(state: &AppState) -> Content {
    // Clone and release: modules may lock the config while listing items.
    let config = state.config.lock().unwrap().clone();
    module::content(&state.modules, &config)
}

pub fn emit_content(app: &AppHandle) {
    let _ = app.emit("content-changed", content(&app.state::<AppState>()));
}

pub fn emit_status(app: &AppHandle) {
    let _ = app.emit("status-changed", status(&app.state::<AppState>()));
}

/// Starts Minim Notch with the given modules (ADR-0009). `context` comes from
/// `tauri::generate_context!()` in the calling binary: its tauri.conf.json
/// decides the product, the front-end and the bundle.
pub fn run(context: tauri::Context, mut modules: Vec<Box<dyn Module>>) {
    tauri::Builder::default()
        // A second launch would fail to bind the hook port: bring the running
        // one forward instead, notch and settings (e.g. from the Start menu).
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            notch::set_user_visible(app, true);
            if let Err(e) = settings::open(app) {
                log::error!("cannot open settings: {e}");
            }
        }))
        .plugin(tauri_plugin_autostart::Builder::new().build())
        // Rust-side file pickers (custom companions, DF-0025).
        .plugin(tauri_plugin_dialog::init())
        // In installed builds too (ADR-0014).
        .plugin(diagnostics::log_plugin())
        // Updates, installed on the user's click only (ADR-0013).
        .plugin(tauri_plugin_updater::Builder::new().build())
        .invoke_handler(tauri::generate_handler![
            get_content,
            get_status,
            acknowledge,
            item_action,
            set_hit_area,
            watch_leave,
            cancel_leave,
            start_drag,
            drag,
            end_drag,
            settings::get_settings,
            settings::set_sound,
            settings::set_hide_in_fullscreen,
            settings::set_edge,
            settings::set_style,
            settings::set_gap,
            settings::set_screen,
            settings::set_movable,
            settings::recenter,
            settings::set_cursor_resistance,
            settings::set_autostart,
            settings::set_module_enabled,
            settings::set_module_order,
            settings::set_module_layout,
            settings::set_notch_speed,
            settings::set_appearance,
            settings::set_language,
            settings::set_auto_hide,
            settings::set_pin_flares,
            settings::set_auto_hide_delay,
            settings::set_notch_color,
            settings::set_notch_opacity,
            settings::set_notch_grain,
            set_tucked,
            set_typing,
            settings::set_module_pin,
            settings::module_call,
            settings::open_link,
            settings::set_check_updates,
            updates::check_updates,
            updates::install_update,
            context_menu::module_menu,
            diagnostics::open_log,
            diagnostics::reveal_log,
            diagnostics::crash_report,
            diagnostics::dismiss_crash,
            companion::library::get_companions,
            companion::library::reload_companions,
            companion::library::import_companion,
            companion::library::delete_companion,
            companion::library::reveal_companions,
            companion::library::export_companion_template
        ])
        .setup(move |app| {
            let log_dir = app.path().app_log_dir()?;
            diagnostics::install_panic_hook(
                log_dir.clone(),
                app.package_info().version.to_string(),
            );
            let crash = diagnostics::take_crash(&log_dir);
            diagnostics::test_panic();
            if let Some(crash) = &crash {
                log::warn!(
                    "the previous run crashed at {}: {}",
                    crash.location,
                    crash.message
                );
            }

            let config_dir = app.path().app_config_dir()?;
            let config = config::load_or_create(&config_dir);
            // Before the modules: they offer the imported companions.
            companion::load_custom(&config_dir);
            for module in &modules {
                for (language, json) in module.locales() {
                    i18n::register(language, json);
                }
            }
            i18n::set_language(i18n::resolve(config.language.as_deref()));
            platform::set_resistance_strength(config.cursor_resistance_strength);
            let enabled: Vec<bool> = modules
                .iter()
                .map(|m| module::is_enabled(m.as_ref(), &config))
                .collect();
            app.manage(AppState {
                modules: std::mem::take(&mut modules),
                config: Mutex::new(config),
                config_dir,
                user_hidden: AtomicBool::new(false),
                movable: AtomicBool::new(false),
                log_dir,
                crash: Mutex::new(crash.clone()),
            });
            app.manage(updates::Updates::default());
            let state = app.state::<AppState>();
            for (module, enabled) in state.modules.iter().zip(enabled) {
                if enabled {
                    module.start(&Host::new(app.handle().clone(), module.id()));
                }
            }

            let window = app
                .get_webview_window(NOTCH_LABEL)
                .expect("notch window is declared in tauri.conf.json");
            platform::hide_frame(&window)?;
            platform::track_foreground(&window)?;
            notch::place(&window)?;
            notch::apply_hit_area(&window)?;
            notch::show(&window)?;
            notch::spawn_watcher(app.handle().clone());
            updates::spawn_watcher(app.handle().clone());

            tray::create(app.handle())?;
            // The right-click menu of the modules (the tray has its own).
            app.on_menu_event(|app, event| context_menu::handle(app, &event));
            // The crash is reported in the settings, opened without taking
            // the focus from the user's app.
            if crash.is_some() {
                settings::open_in_background(app.handle())?;
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            if window.label() != NOTCH_LABEL {
                return;
            }
            // The notch never keeps the focus: see `platform::track_foreground`.
            // But while the user types in it (`notch::set_typing`).
            // Typing, a loss of the keyboard (a click elsewhere) ends it.
            if let WindowEvent::Focused(false) = event {
                notch::focus_lost(window.app_handle());
            }
            if let WindowEvent::Focused(true) = event
                && !notch::typing()
            {
                log::info!("notch took the focus, given back");
                platform::give_back_foreground();
            }
            // On a DPI change the window still has its old size when this event fires,
            // and Windows then applies its own suggested rect. Re-center afterwards,
            // once the event loop is done with the resize: window calls made from
            // another thread are queued on the event loop instead of running inline.
            if matches!(
                event,
                WindowEvent::ScaleFactorChanged { .. } | WindowEvent::Resized(_)
            ) && let Some(w) = window.app_handle().get_webview_window(NOTCH_LABEL)
            {
                std::thread::spawn(move || {
                    if let Err(e) = notch::place(&w).and_then(|_| notch::apply_hit_area(&w)) {
                        log::warn!("failed to reposition notch: {e}");
                    }
                });
            }
        })
        .run(context)
        .expect("error while running Minim Notch");
}
