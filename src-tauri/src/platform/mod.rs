//! OS-specific tweaks for the notch window. Each OS gets its own module.

/// A visible card of the notch, in physical pixels relative to the window.
/// The first card of a notch is attached to its edge (square corners on that
/// side); a pill and the other cards of the separate layout are `detached`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Shape {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub radius: i32,
    pub detached: bool,
}

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use windows::{
    bring_to_front, foreground_class, fullscreen_on_notch_screen, hide_frame, os_version,
    prepare_overlay, restrict_to_shapes, set_cursor_resistance, set_resistance_rect,
    set_resistance_strength, take_breakthrough,
};

#[cfg(not(windows))]
pub fn prepare_overlay(_window: &tauri::WebviewWindow) -> tauri::Result<()> {
    Ok(())
}

#[cfg(not(windows))]
pub fn hide_frame(_window: &tauri::WebviewWindow) -> tauri::Result<()> {
    Ok(())
}

#[cfg(not(windows))]
pub fn fullscreen_on_notch_screen(_notch: &tauri::WebviewWindow) -> bool {
    false
}

/// Outside Windows the window itself is resized around the cards for now.
#[cfg(not(windows))]
pub fn restrict_to_shapes(
    window: &tauri::WebviewWindow,
    _edge: crate::placement::Edge,
    shapes: &[Shape],
) -> tauri::Result<()> {
    let left = shapes.iter().map(|s| s.x).min().unwrap_or(0);
    let top = shapes.iter().map(|s| s.y).min().unwrap_or(0);
    let right = shapes.iter().map(|s| s.x + s.width).max().unwrap_or(0);
    let bottom = shapes.iter().map(|s| s.y + s.height).max().unwrap_or(0);
    window.set_size(tauri::PhysicalSize::new(
        (right - left) as u32,
        (bottom - top) as u32,
    ))?;
    crate::notch::place(window)
}

/// Cursor resistance needs a system-wide mouse hook: Windows only for now.
#[cfg(not(windows))]
pub fn set_cursor_resistance(_active: bool) {}

#[cfg(not(windows))]
pub fn set_resistance_rect(_rect: crate::resistance::Rect) {}

#[cfg(not(windows))]
pub fn set_resistance_strength(_strength: crate::resistance::Strength) {}

#[cfg(not(windows))]
pub fn bring_to_front(window: &tauri::WebviewWindow) -> tauri::Result<()> {
    window.set_focus()
}

#[cfg(not(windows))]
pub fn take_breakthrough() -> Option<crate::resistance::Breakthrough> {
    None
}

#[cfg(not(windows))]
pub fn foreground_class() -> String {
    String::new()
}

#[cfg(not(windows))]
pub fn os_version() -> String {
    std::env::consts::OS.to_owned()
}
