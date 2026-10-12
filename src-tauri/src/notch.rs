use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

use serde::Deserialize;
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, WebviewWindow};

use crate::placement::{self, Area, Edge, Placement, Screen, Style};
use crate::resistance::Rect;
use crate::{AppState, NOTCH_LABEL, platform};

/// A rectangle in logical pixels, relative to the window.
#[derive(Debug, Clone, Copy, Deserialize)]
pub struct Bounds {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// One visible shape, placed by the front (it draws them): the notch, the
/// other cards and the gaps between them (DF-0011), the pins (DF-0012).
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Shape {
    #[serde(flatten)]
    pub bounds: Bounds,
    pub radius: f64,
    /// Glued to the screen edge: square corners on that side.
    pub attached: bool,
    /// Outward curves where it meets the edge (DF-0028); 0 for none.
    #[serde(default)]
    pub flare: f64,
}

/// Radius of the outward curves of the attached notch (DF-0028): `FLARE` in
/// src/App.svelte, used here until the front reports its shapes.
const FLARE: f64 = 6.0;

/// Visible shapes, the notch first. `None` until the front reports them:
/// the compact notch of the current edge is used meanwhile.
static HIT_AREA: Mutex<Option<Vec<Shape>>> = Mutex::new(None);

/// Depth of the notch window towards the inside of the screen, in logical
/// pixels: the tallest open notch on the top / bottom edges (`MAX_HEIGHT` in
/// src/App.svelte), the widest open shape on the left / right ones. Along the
/// edge the window spans it all (`placement::window_size`). Room for every
/// module in its own card (DF-0011), the media player's included.
const WINDOW_DEPTH: f64 = 600.0;
const WINDOW_DEPTH_SIDE: f64 = 400.0;

/// Around an open shape, in logical pixels: the cursor may stray this far
/// out without closing it, e.g. aiming at a button near its edge. Not a part
/// of the window (clicks there reach the app below): only the closing waits.
const SAFETY_MARGIN: f64 = 16.0;

/// Bumped to stop the running `watch_leave`, if any.
static LEAVE_WATCH: AtomicU64 = AtomicU64::new(0);

/// How often `watch_leave` reads the cursor position.
const LEAVE_POLL: Duration = Duration::from_millis(30);

/// Drag in progress (DF-0006, step 3): placement and cursor position (along
/// the edge, physical pixels) when the button was pressed.
static DRAG: Mutex<Option<(Placement, i32)>> = Mutex::new(None);

/// Distance, in logical pixels, within which a dragged notch snaps to a
/// quarter, the middle or three quarters of its edge.
const SNAP: f64 = 24.0;

/// Center of the compact notch along the edge, from the start of the window,
/// in logical pixels (`placement::Layout::anchor`). `None` before the first
/// placement: the middle of the window is used meanwhile.
static ANCHOR: Mutex<Option<f64>> = Mutex::new(None);

/// Anchor sent to the front, which centers the drawn shape on it.
pub fn anchor() -> Option<f64> {
    *ANCHOR.lock().unwrap()
}

/// Compact shape per edge and style; must match `compactShape` in
/// src/App.svelte and `--notch-radius` in src/app.css. A pill is fully
/// rounded: its radius is half its thickness.
fn compact_shape(edge: Edge, style: Style) -> (f64, f64, f64) {
    let radius = match style {
        Style::Notch => 14.0,
        Style::Pill => 18.0,
    };
    if edge.is_vertical() {
        (36.0, 120.0, radius)
    } else {
        (300.0, 36.0, radius)
    }
}

/// Length of the compact notch along `edge`, in physical pixels.
fn compact_length(window: &WebviewWindow, edge: Edge) -> tauri::Result<i32> {
    let (width, height, _) = compact_shape(edge, Style::Notch);
    let length = if edge.is_vertical() { height } else { width };
    Ok((length * window.scale_factor()?).round() as i32)
}

fn current_placement(window: &WebviewWindow) -> Placement {
    window
        .app_handle()
        .state::<AppState>()
        .config
        .lock()
        .unwrap()
        .placement
        .clone()
}

/// Records the visible shapes and restricts the window to them.
pub fn set_hit_area(window: &WebviewWindow, shapes: Vec<Shape>) -> tauri::Result<()> {
    if shapes.is_empty() {
        return Ok(());
    }
    *HIT_AREA.lock().unwrap() = Some(shapes);
    apply_hit_area(window)
}

/// Restricts the window to the visible shapes and gives the notch (the first
/// one) in screen coordinates to the cursor resistance (DF-0004). Re-applied
/// after moves, DPI or placement changes.
pub fn apply_hit_area(window: &WebviewWindow) -> tauri::Result<()> {
    let placement = current_placement(window);
    let edge = placement.edge;
    let scale = window.scale_factor()?;
    let px = |v: f64| (v * scale).round() as i32;
    let shapes: Vec<platform::Shape> = match HIT_AREA.lock().unwrap().clone() {
        Some(shapes) => shapes
            .iter()
            .map(|s| platform::Shape {
                x: px(s.bounds.x),
                y: px(s.bounds.y),
                width: px(s.bounds.width),
                height: px(s.bounds.height),
                radius: px(s.radius),
                detached: !s.attached,
                flare: px(s.flare),
            })
            .collect(),
        // Before the front reports: the compact notch, centered on the anchor.
        None => {
            let (width, height, radius) = compact_shape(edge, placement.style);
            // Window regions are relative to the outer window rectangle. The
            // inner size is also wrong before the first show.
            let size = window.outer_size()?;
            let size = (size.width as i32, size.height as i32);
            let anchor = match anchor() {
                Some(anchor) => px(anchor),
                None if edge.is_vertical() => size.1 / 2,
                None => size.0 / 2,
            };
            let shape = (px(width), px(height));
            let (x, y) = placement::shape_in_window(edge, size, shape, anchor);
            vec![platform::Shape {
                x,
                y,
                width: shape.0,
                height: shape.1,
                radius: px(radius),
                detached: placement.style == Style::Pill,
                flare: if placement.style == Style::Pill {
                    0
                } else {
                    px(FLARE)
                },
            }]
        }
    };
    platform::restrict_to_shapes(window, edge, &shapes)?;

    // The resistance guards the notch, the way in; the other shapes only open
    // while the cursor is already there.
    let notch = shapes[0];
    let position = window.outer_position()?;
    platform::set_resistance_rect(Rect {
        left: position.x + notch.x,
        top: position.y + notch.y,
        right: position.x + notch.x + notch.width,
        bottom: position.y + notch.y + notch.height,
        radius: notch.radius,
        attached: edge,
        detached: notch.detached,
    });
    Ok(())
}

/// The cursor left an open shape (DF-0011, DF-0012): tells the front once
/// it is past the safety margin around `area` too. Stopped by `cancel_leave`
/// or the next call.
pub fn watch_leave(window: &WebviewWindow, area: Bounds) -> tauri::Result<()> {
    let scale = window.scale_factor()?;
    let position = window.outer_position()?;
    // In physical screen pixels, margin included.
    let left = f64::from(position.x) + (area.x - SAFETY_MARGIN) * scale;
    let top = f64::from(position.y) + (area.y - SAFETY_MARGIN) * scale;
    let right = f64::from(position.x) + (area.x + area.width + SAFETY_MARGIN) * scale;
    let bottom = f64::from(position.y) + (area.y + area.height + SAFETY_MARGIN) * scale;
    let app = window.app_handle().clone();
    let watch = LEAVE_WATCH.fetch_add(1, Ordering::Relaxed) + 1;
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(LEAVE_POLL);
            if LEAVE_WATCH.load(Ordering::Relaxed) != watch {
                return;
            }
            let Ok(cursor) = app.cursor_position() else {
                return;
            };
            let near = cursor.x >= left && cursor.x < right && cursor.y >= top && cursor.y < bottom;
            if !near {
                let _ = app.emit_to(NOTCH_LABEL, "pointer-left", ());
                return;
            }
        }
    });
    Ok(())
}

pub fn cancel_leave() {
    LEAVE_WATCH.fetch_add(1, Ordering::Relaxed);
}

/// Forgets the reported shape: after an edge change the front reports the
/// new compact shape, meanwhile the default one of the new edge is used.
pub fn reset_hit_area() {
    *HIT_AREA.lock().unwrap() = None;
}

/// Tucked into the edge by the auto-hide (DF-0018): the pill then sits
/// against the edge, and the cursor passes freely.
static TUCKED: AtomicBool = AtomicBool::new(false);

/// The front tucked the notch and its pins into the edge, or brought them
/// out: the pill window moves to the edge (or back to its gap), and the
/// cursor resistance follows.
pub fn set_tucked(app: &AppHandle, tucked: bool) {
    if TUCKED.swap(tucked, Ordering::Relaxed) == tucked {
        return;
    }
    update_cursor_resistance(app);
    if let Some(window) = app.get_webview_window(NOTCH_LABEL)
        && let Err(e) = place(&window).and_then(|_| apply_hit_area(&window))
    {
        log::warn!("cannot move the tucked notch: {e}");
    }
}

/// Cursor resistance runs only while the notch is shown and the option is on,
/// never in move mode (the cursor must reach the notch freely to grab it)
/// nor tucked (the cursor would stop at a notch it cannot see).
pub fn update_cursor_resistance(app: &AppHandle) {
    let state = app.state::<AppState>();
    let enabled = state.config.lock().unwrap().cursor_resistance
        && !state.movable.load(Ordering::Relaxed)
        && !TUCKED.load(Ordering::Relaxed);
    let visible = app
        .get_webview_window(NOTCH_LABEL)
        .and_then(|w| w.is_visible().ok())
        .unwrap_or(false);
    platform::set_cursor_resistance(enabled && visible);
}

/// Turns the move mode on or off (DF-0006, step 3): only then can the notch be
/// dragged, so it is never moved by accident.
pub fn set_movable(app: &AppHandle, movable: bool) {
    let state = app.state::<AppState>();
    if state.movable.swap(movable, Ordering::Relaxed) == movable {
        return;
    }
    if !movable {
        end_drag(app);
    }
    update_cursor_resistance(app);
    crate::emit_status(app);
}

/// Cursor position along the notch edge, in physical pixels.
fn cursor_along(window: &WebviewWindow, placement: &Placement) -> tauri::Result<i32> {
    let cursor = window.cursor_position()?;
    let along = if placement.edge.is_vertical() {
        cursor.y
    } else {
        cursor.x
    };
    Ok(along.round() as i32)
}

/// Mouse button pressed on the notch in move mode.
pub fn start_drag(window: &WebviewWindow) -> tauri::Result<()> {
    if !window.state::<AppState>().movable.load(Ordering::Relaxed) {
        return Ok(());
    }
    let placement = current_placement(window);
    let cursor = cursor_along(window, &placement)?;
    *DRAG.lock().unwrap() = Some((placement, cursor));
    Ok(())
}

/// Cursor moved while dragging: the notch follows it along its edge. The
/// offset is kept in memory and saved to disk once the button is released.
pub fn drag(window: &WebviewWindow) -> tauri::Result<()> {
    let Some((start, cursor_start)) = DRAG.lock().unwrap().clone() else {
        return Ok(());
    };
    let Some(area) = work_area(window)? else {
        return Ok(());
    };
    let delta = cursor_along(window, &start)? - cursor_start;
    let compact = compact_length(window, start.edge)?;
    let snap = (SNAP * window.scale_factor()?).round() as i32;
    let offset = placement::dragged_offset(&start, area, compact, delta, snap);
    window
        .state::<AppState>()
        .config
        .lock()
        .unwrap()
        .placement
        .offset = offset;
    // Near the ends the notch also moves inside the window: follow it.
    place(window)?;
    apply_hit_area(window)
}

/// Mouse button released: saves where the notch was dropped.
pub fn end_drag(app: &AppHandle) {
    if DRAG.lock().unwrap().take().is_none() {
        return;
    }
    let state = app.state::<AppState>();
    let config = state.config.lock().unwrap().clone();
    if let Err(e) = crate::config::save(&state.config_dir, &config) {
        log::error!("cannot save config: {e}");
    }
    if let Some(window) = app.get_webview_window(NOTCH_LABEL)
        && let Err(e) = apply_hit_area(&window)
    {
        log::warn!("cannot update the notch area: {e}");
    }
    crate::settings::changed(app);
}

/// All connected monitors, in physical pixels.
pub fn screens(app: &AppHandle) -> tauri::Result<Vec<Screen>> {
    let primary = app.primary_monitor()?.and_then(|m| m.name().cloned());
    Ok(app
        .available_monitors()?
        .into_iter()
        .map(|m| {
            let (position, size, wa) = (m.position(), m.size(), m.work_area());
            let id = m.name().cloned().unwrap_or_default();
            Screen {
                primary: primary.as_ref() == Some(&id),
                id,
                bounds: Area {
                    x: position.x,
                    y: position.y,
                    width: size.width as i32,
                    height: size.height as i32,
                },
                work_area: Area {
                    x: wa.position.x,
                    y: wa.position.y,
                    width: wa.size.width as i32,
                    height: wa.size.height as i32,
                },
            }
        })
        .collect())
}

/// Work area (screen minus taskbar) of the screen chosen for the notch, or
/// of the primary one if it is not connected (DF-0006, step 2).
fn work_area(window: &WebviewWindow) -> tauri::Result<Option<Area>> {
    let screens = screens(window.app_handle())?;
    let wanted = current_placement(window).screen;
    Ok(placement::choose_screen(&screens, wanted.as_deref()).map(|s| s.work_area))
}

/// Glues the notch window to the chosen edge of the work area (DF-0006).
/// Physical pixels, so it stays right at any DPI scaling. The front is told
/// where the notch sits in the window when that changes (ends of the edge).
pub fn place(window: &WebviewWindow) -> tauri::Result<()> {
    let Some(area) = work_area(window)? else {
        log::warn!("no monitor found, notch left where it is");
        return Ok(());
    };
    let placement = current_placement(window);
    // Resized only with the edge, the screen or the scale, never to animate:
    // resizing a WebView2 flashes.
    let depth = if placement.edge.is_vertical() {
        WINDOW_DEPTH_SIDE
    } else {
        WINDOW_DEPTH
    };
    let depth = (depth * window.scale_factor()?).round() as i32;
    let size = placement::window_size(placement.edge, area, depth);
    let current = window.outer_size()?;
    if (current.width as i32, current.height as i32) != size {
        window.set_size(PhysicalSize::new(size.0 as u32, size.1 as u32))?;
    }
    let layout = placement::layout(
        &placement,
        area,
        size,
        compact_length(window, placement.edge)?,
        // Tucked, the pill sits against the edge like the notch (DF-0018).
        if TUCKED.load(Ordering::Relaxed) {
            0
        } else {
            placement.gap_px(window.scale_factor()?)
        },
    );
    window.set_position(PhysicalPosition::new(layout.window.0, layout.window.1))?;

    let anchor = Some(f64::from(layout.anchor) / window.scale_factor()?);
    let previous = std::mem::replace(&mut *ANCHOR.lock().unwrap(), anchor);
    if previous != anchor {
        crate::emit_status(window.app_handle());
    }
    Ok(())
}

/// Shows the notch and restores the overlay styles Tauri resets on show.
pub fn show(window: &WebviewWindow) -> tauri::Result<()> {
    window.show()?;
    platform::prepare_overlay(window)
}

/// Shows or hides the notch on the user's request (tray menu).
pub fn set_user_visible(app: &AppHandle, visible: bool) {
    app.state::<AppState>()
        .user_hidden
        .store(!visible, Ordering::Relaxed);
    if let Some(w) = app.get_webview_window(NOTCH_LABEL) {
        let result = if visible { show(&w) } else { w.hide() };
        if let Err(e) = result {
            log::warn!("cannot change notch visibility: {e}");
        }
    }
}

/// Once per second: re-centers the notch when the primary monitor changes
/// (resolution, other screen made primary) and hides it while an app is
/// fullscreen on that monitor, see docs/fonctionnel/DF-0002-comportement-fenetre.md.
pub fn spawn_watcher(app: AppHandle) {
    std::thread::spawn(move || {
        let mut last_monitor = None;
        let mut fullscreen = false;
        loop {
            std::thread::sleep(Duration::from_secs(1));
            let Some(window) = app.get_webview_window(NOTCH_LABEL) else {
                continue;
            };
            // Self-heal: any show/hide/re-layering by Tauri drops the overlay styles.
            if let Err(e) = platform::prepare_overlay(&window) {
                log::warn!("cannot restore overlay styles: {e}");
            }

            // Resolution, primary monitor or taskbar changed: re-place.
            if let Ok(Some(current)) = work_area(&window) {
                if last_monitor.is_some_and(|last| last != current)
                    && let Err(e) = place(&window).and_then(|_| apply_hit_area(&window))
                {
                    log::warn!("failed to reposition notch: {e}");
                }
                last_monitor = Some(current);
            }

            // Idempotent: follows visibility changes (fullscreen, tray, setting).
            update_cursor_resistance(&app);
            if let Some(b) = platform::take_breakthrough() {
                log::info!(
                    "cursor entered the notch through {}: {} ms, {} mouse events",
                    if b.inner {
                        "the inner edge"
                    } else {
                        "a lateral edge"
                    },
                    b.duration_ms,
                    b.events
                );
            }

            // With the option off, a fullscreen app is never detected: the notch
            // stays (or comes back) on screen.
            let hide_in_fullscreen = app
                .state::<AppState>()
                .config
                .lock()
                .unwrap()
                .hide_in_fullscreen;
            let now_fullscreen =
                hide_in_fullscreen && platform::fullscreen_on_notch_screen(&window);
            if now_fullscreen == fullscreen {
                continue;
            }
            fullscreen = now_fullscreen;
            if fullscreen {
                log::info!(
                    "fullscreen app on the notch screen ({}): notch hidden",
                    platform::foreground_class()
                );
            } else {
                log::info!("fullscreen app gone: notch shown");
            }
            let user_hidden = app.state::<AppState>().user_hidden.load(Ordering::Relaxed);
            let result = if fullscreen {
                window.hide()
            } else if !user_hidden {
                show(&window)
            } else {
                Ok(())
            };
            if let Err(e) = result {
                log::warn!("cannot change notch visibility: {e}");
            }
        }
    });
}
