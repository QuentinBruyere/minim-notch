use std::mem::{size_of, zeroed};

use tauri::WebviewWindow;

use crate::placement::Edge;
use windows_sys::Win32::Foundation::{HWND, RECT};
use windows_sys::Win32::Graphics::Gdi::{
    CombineRgn, CreateRectRgn, CreateRoundRectRgn, DeleteObject, GetMonitorInfoW, HRGN,
    MONITOR_DEFAULTTONEAREST, MONITOR_DEFAULTTONULL, MONITORINFO, MonitorFromPoint,
    MonitorFromWindow, RGN_OR, SetWindowRgn,
};
use windows_sys::Win32::UI::Accessibility::{HWINEVENTHOOK, SetWinEventHook};
use windows_sys::Win32::UI::Shell::{DefSubclassProc, SetWindowSubclass};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    EVENT_SYSTEM_FOREGROUND, GWL_EXSTYLE, GWL_STYLE, GetClassNameW, GetForegroundWindow,
    GetWindowLongPtrW, GetWindowRect, IsWindow, SetForegroundWindow, SetWindowLongPtrW,
    WINEVENT_OUTOFCONTEXT, WM_NCACTIVATE, WM_NCPAINT, WS_CAPTION, WS_EX_APPWINDOW,
    WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
};

/// Turns the notch into a tool window that never takes focus:
/// hidden from Alt+Tab (WS_EX_TOOLWINDOW) and not activated on click (WS_EX_NOACTIVATE).
/// Tauri rewrites the extended style whenever it shows, hides or re-layers the
/// window, so this must be re-applied after such changes; it is a no-op when
/// the style is already right.
pub fn prepare_overlay(window: &WebviewWindow) -> tauri::Result<()> {
    let hwnd = window.hwnd()?.0;
    unsafe {
        let current = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        let wanted = (current & !(WS_EX_APPWINDOW as isize))
            | WS_EX_TOOLWINDOW as isize
            | WS_EX_NOACTIVATE as isize;
        if wanted != current {
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, wanted);
        }
    }
    Ok(())
}

/// Stops Windows from drawing a window frame around the notch. The window has
/// no visible frame, but Windows still paints a default (white) one on each
/// activation change (`WM_NCACTIVATE`, `WM_NCPAINT`): a click on the notch,
/// then elsewhere, or another of our windows opening. The webview is
/// transparent, so that white showed in the margin left around the shapes
/// (`AA_MARGIN`). Called once: the subclass lives as long as the window.
pub fn hide_frame(window: &WebviewWindow) -> tauri::Result<()> {
    let hwnd = window.hwnd()?.0;
    unsafe {
        SetWindowSubclass(hwnd, Some(frameless_proc), FRAMELESS_SUBCLASS, 0);
    }
    Ok(())
}

const FRAMELESS_SUBCLASS: usize = 0x6d6e;

unsafe extern "system" fn frameless_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _id: usize,
    _data: usize,
) -> LRESULT {
    unsafe {
        match msg {
            WM_NCPAINT => 0,
            // lParam -1: update the activation state without repainting the
            // frame (documented for WM_NCACTIVATE).
            WM_NCACTIVATE => DefSubclassProc(hwnd, msg, wparam, -1),
            _ => DefSubclassProc(hwnd, msg, wparam, lparam),
        }
    }
}

/// The notch must never keep the focus. `WS_EX_NOACTIVATE` refuses it to a
/// click, but the webview, run by another process that shares our input
/// queue, takes the focus itself when our app was active just before (the
/// settings, or a click on the notch right after them). So the window that
/// was in front is followed (`EVENT_SYSTEM_FOREGROUND`, only sent when it
/// changes), and `give_back_foreground` puts it back.
/// Called once, from the UI thread (the hook needs its message loop).
pub fn track_foreground(notch: &WebviewWindow) -> tauri::Result<()> {
    NOTCH_HWND.store(notch.hwnd()?.0 as isize, Ordering::Relaxed);
    PREVIOUS_FOREGROUND.store(unsafe { GetForegroundWindow() } as isize, Ordering::Relaxed);
    unsafe {
        SetWinEventHook(
            EVENT_SYSTEM_FOREGROUND,
            EVENT_SYSTEM_FOREGROUND,
            std::ptr::null_mut(),
            Some(on_foreground),
            0,
            0,
            WINEVENT_OUTOFCONTEXT,
        );
    }
    Ok(())
}

/// Gives the focus back to the window that had it before the notch.
pub fn give_back_foreground() {
    let previous = PREVIOUS_FOREGROUND.load(Ordering::Relaxed) as HWND;
    unsafe {
        if !previous.is_null() && IsWindow(previous) != 0 {
            SetForegroundWindow(previous);
        }
    }
}

static NOTCH_HWND: AtomicIsize = AtomicIsize::new(0);
/// Last window in front other than the notch.
static PREVIOUS_FOREGROUND: AtomicIsize = AtomicIsize::new(0);

unsafe extern "system" fn on_foreground(
    _hook: HWINEVENTHOOK,
    _event: u32,
    hwnd: HWND,
    _object: i32,
    _child: i32,
    _thread: u32,
    _time: u32,
) {
    let hwnd = hwnd as isize;
    if hwnd != 0 && hwnd != NOTCH_HWND.load(Ordering::Relaxed) {
        PREVIOUS_FOREGROUND.store(hwnd, Ordering::Relaxed);
    }
}

/// True when the foreground window covers the whole monitor the notch is on,
/// without a title bar: games, videos, presentations, browsers in F11 mode.
/// Maximized windows keep their caption, so they do not count.
pub fn fullscreen_on_notch_screen(notch: &WebviewWindow) -> bool {
    let Ok(notch) = notch.hwnd() else {
        return false;
    };
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.is_null() || is_shell_window(hwnd) {
            return false;
        }
        let monitor = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONULL);
        if monitor.is_null() || monitor != MonitorFromWindow(notch.0, MONITOR_DEFAULTTONEAREST) {
            return false;
        }
        let mut info: MONITORINFO = zeroed();
        info.cbSize = size_of::<MONITORINFO>() as u32;
        if GetMonitorInfoW(monitor, &mut info) == 0 {
            return false;
        }
        let mut rect: RECT = zeroed();
        if GetWindowRect(hwnd, &mut rect) == 0 {
            return false;
        }
        let screen = info.rcMonitor;
        let covers = rect.left <= screen.left
            && rect.top <= screen.top
            && rect.right >= screen.right
            && rect.bottom >= screen.bottom;
        let style = GetWindowLongPtrW(hwnd, GWL_STYLE) as u32;
        covers && style & WS_CAPTION != WS_CAPTION
    }
}

/// Shell windows that can be foreground and cover the whole screen without
/// being a fullscreen app: the notch must stay visible over them.
const SHELL_CLASSES: &[&str] = &[
    // Desktop and taskbar.
    "Progman",
    "WorkerW",
    "Shell_TrayWnd",
    // Alt+Tab and Win+Tab: Windows 10, then Windows 11.
    "MultitaskingViewFrame",
    "XamlExplorerHostIslandWindow",
];

unsafe fn is_shell_window(hwnd: windows_sys::Win32::Foundation::HWND) -> bool {
    SHELL_CLASSES.contains(&unsafe { class_name(hwnd) }.as_str())
}

unsafe fn class_name(hwnd: windows_sys::Win32::Foundation::HWND) -> String {
    let mut class = [0u16; 128];
    let len = unsafe { GetClassNameW(hwnd, class.as_mut_ptr(), class.len() as i32) };
    String::from_utf16_lossy(&class[..len.max(0) as usize])
}

/// Window class of the foreground window, logged when it hides the notch so
/// that other shell overlays wrongly taken for fullscreen apps can be spotted.
pub fn foreground_class() -> String {
    unsafe { class_name(GetForegroundWindow()) }
}

/// Windows' name, display version and build, for crash reports (ADR-0014).
/// The product name in the registry still says "Windows 10" on Windows 11:
/// the build tells them apart.
pub fn os_version() -> String {
    let build = current_version("CurrentBuild").unwrap_or_default();
    let name = if build.parse::<u32>().is_ok_and(|b| b >= 22000) {
        "Windows 11"
    } else {
        "Windows 10"
    };
    match current_version("DisplayVersion") {
        Some(display) => format!("{name} {display} (build {build})"),
        None => format!("{name} (build {build})"),
    }
}

/// A text value of `HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion`.
fn current_version(name: &str) -> Option<String> {
    use windows_sys::Win32::System::Registry::{HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ, RegGetValueW};
    let key: Vec<u16> = "SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion\0"
        .encode_utf16()
        .collect();
    let value: Vec<u16> = format!("{name}\0").encode_utf16().collect();
    let mut buffer = [0u16; 64];
    let mut bytes = size_of_val(&buffer) as u32;
    let status = unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            key.as_ptr(),
            value.as_ptr(),
            RRF_RT_REG_SZ,
            std::ptr::null_mut(),
            buffer.as_mut_ptr().cast(),
            &mut bytes,
        )
    };
    if status != 0 {
        return None;
    }
    // The size counts the final null.
    let len = (bytes as usize / 2).saturating_sub(1);
    Some(String::from_utf16_lossy(&buffer[..len]))
}

/// Physical pixels the window region extends past the drawn shape, so its
/// anti-aliased edge is not cut (see `card_region`).
const AA_MARGIN: i32 = 2;

/// Limits the window to the visible cards of the notch: outside them the
/// window neither paints nor receives clicks, which go to the app below.
/// Changing the region instead of resizing the window avoids the WebView2
/// repaint flash a resize causes.
pub fn restrict_to_shapes(
    window: &WebviewWindow,
    edge: Edge,
    shapes: &[super::Shape],
) -> tauri::Result<()> {
    let hwnd = window.hwnd()?.0;
    unsafe {
        let region = CreateRectRgn(0, 0, 0, 0);
        for shape in shapes {
            let card = card_region(edge, shape);
            CombineRgn(region, region, card, RGN_OR);
            DeleteObject(card);
        }
        // The system owns the region once it is set: no DeleteObject.
        SetWindowRgn(hwnd, region, 1);
    }
    Ok(())
}

/// Region of one card, owned by the caller. A `detached` card has its four
/// corners rounded, an attached one only those away from its edge.
/// Coordinates are physical, relative to the window.
fn card_region(edge: Edge, shape: &super::Shape) -> HRGN {
    // The region is all-or-nothing per pixel: cut on the drawn outline, it
    // would chop the anti-aliased edge of the shape into visible steps. A
    // small margin keeps the smooth edge whole; the region's own steps then
    // fall on transparent pixels.
    let (x, y, width, height) = (
        shape.x - AA_MARGIN,
        shape.y - AA_MARGIN,
        shape.width + 2 * AA_MARGIN,
        shape.height + 2 * AA_MARGIN,
    );
    let radius = shape.radius + AA_MARGIN;
    // Square half on the attached side: only the inner corners are rounded.
    let (sx, sy, sw, sh) = match edge {
        Edge::Top => (x, y, width, height / 2),
        Edge::Bottom => (x, y + height / 2, width, height - height / 2),
        Edge::Left => (x, y, width / 2, height),
        Edge::Right => (x + width / 2, y, width - width / 2, height),
    };
    unsafe {
        // Rounded like the drawn shape. The +1: round regions exclude their
        // right and bottom edges.
        let region =
            CreateRoundRectRgn(x, y, x + width + 1, y + height + 1, 2 * radius, 2 * radius);
        if !shape.detached {
            let square = CreateRectRgn(sx, sy, sx + sw, sy + sh);
            CombineRgn(region, region, square, RGN_OR);
            DeleteObject(square);
        }
        region
    }
}

// --- Cursor resistance (DF-0004): a low-level mouse hook on its own thread ---

use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicIsize, AtomicU32, AtomicU64, Ordering};

use windows_sys::Win32::Foundation::{LPARAM, LRESULT, POINT, WPARAM};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Threading::GetCurrentThreadId;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, GetCursorPos, GetMessageW, HC_ACTION, LLMHF_INJECTED, MSG, MSLLHOOKSTRUCT,
    PostThreadMessageW, SetCursorPos, SetWindowsHookExW, UnhookWindowsHookEx, WH_MOUSE_LL,
    WM_MOUSEMOVE, WM_QUIT,
};

use crate::resistance::{
    Breakthrough, Rect, Resistance, Strength, Verdict, beyond_attached_edge, clip_to_screen_edge,
};

/// Last breakthrough, packed for lock-free hand-over to the watcher thread
/// (which logs it): bit 63 = new, bit 62 = through the inner edge, bits 32..62 = events,
/// bits 0..32 = duration in ms.
static LAST_BREAKTHROUGH: AtomicU64 = AtomicU64::new(0);

/// Returns the breakthrough recorded since the last call, if any.
pub fn take_breakthrough() -> Option<Breakthrough> {
    let packed = LAST_BREAKTHROUGH.swap(0, Ordering::Relaxed);
    (packed >> 63 == 1).then_some(Breakthrough {
        inner: (packed >> 62) & 1 == 1,
        events: ((packed >> 32) & 0x3fff_ffff) as u32,
        duration_ms: packed as u32,
    })
}

/// Notch rectangle in physical screen pixels, read by the hook on every move.
static NOTCH_RECT: [AtomicI32; 7] = [const { AtomicI32::new(0) }; 7];
/// Push distance to get through, in px (from the chosen `Strength`).
static BREAKTHROUGH: AtomicU32 = AtomicU32::new(80);
/// Whether the hook should be installed.
static WANTED: AtomicBool = AtomicBool::new(false);
/// Thread id of the running hook thread, 0 when none.
static HOOK_THREAD: AtomicU32 = AtomicU32::new(0);
/// A hook thread has been spawned and has not exited yet.
static RUNNING: AtomicBool = AtomicBool::new(false);

thread_local! {
    static STATE: RefCell<Resistance> = RefCell::new(Resistance::default());
}

pub fn set_resistance_strength(strength: Strength) {
    // An infinite (impassable) threshold saturates to u32::MAX, read back as infinite.
    BREAKTHROUGH.store(strength.breakthrough() as u32, Ordering::Relaxed);
}

pub fn set_resistance_rect(rect: Rect) {
    for (slot, value) in NOTCH_RECT.iter().zip([
        rect.left,
        rect.top,
        rect.right,
        rect.bottom,
        rect.radius,
        rect.attached as i32,
        i32::from(rect.detached),
    ]) {
        slot.store(value, Ordering::Relaxed);
    }
}

fn load_rect() -> Rect {
    let [left, top, right, bottom, radius, attached, detached] =
        NOTCH_RECT.each_ref().map(|v| v.load(Ordering::Relaxed));
    let attached = match attached {
        1 => Edge::Bottom,
        2 => Edge::Left,
        3 => Edge::Right,
        _ => Edge::Top,
    };
    Rect {
        left,
        top,
        right,
        bottom,
        radius,
        attached,
        detached: detached != 0,
    }
}

/// Installs or removes the mouse hook. Idempotent; called every second by the
/// watcher. Removing it while the notch is hidden (fullscreen games…) means
/// zero cost on mouse input there.
pub fn set_cursor_resistance(active: bool) {
    WANTED.store(active, Ordering::SeqCst);
    if active {
        if !RUNNING.swap(true, Ordering::SeqCst) {
            std::thread::spawn(run_hook_thread);
        }
    } else {
        let thread = HOOK_THREAD.load(Ordering::SeqCst);
        if thread != 0 {
            unsafe { PostThreadMessageW(thread, WM_QUIT, 0, 0) };
        }
    }
}

fn run_hook_thread() {
    unsafe {
        let hook = SetWindowsHookExW(
            WH_MOUSE_LL,
            Some(mouse_hook),
            GetModuleHandleW(std::ptr::null()),
            0,
        );
        if hook.is_null() {
            log::error!("cannot install the cursor resistance hook");
        } else {
            HOOK_THREAD.store(GetCurrentThreadId(), Ordering::SeqCst);
            // Turned off while we were starting: leave right away.
            if WANTED.load(Ordering::SeqCst) {
                let mut msg: MSG = zeroed();
                // Low-level hooks are called through this thread's message loop.
                while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {}
            }
            HOOK_THREAD.store(0, Ordering::SeqCst);
            UnhookWindowsHookEx(hook);
        }
    }
    RUNNING.store(false, Ordering::SeqCst);
    // Turned back on while we were shutting down.
    if WANTED.load(Ordering::SeqCst) {
        set_cursor_resistance(true);
    }
}

/// Runs for every mouse event of the whole system: keep it to a few arithmetic
/// operations, no allocation, no lock, no logging.
unsafe extern "system" fn mouse_hook(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 && wparam == WM_MOUSEMOVE as WPARAM {
        let info = unsafe { &*(lparam as *const MSLLHOOKSTRUCT) };
        let mut from = POINT { x: 0, y: 0 };
        if info.flags & LLMHF_INJECTED == 0 && unsafe { GetCursorPos(&mut from) } != 0 {
            let rect = load_rect();
            let mut to = (info.pt.x, info.pt.y);
            // Only past the attached edge, so the monitor lookup stays off the hot path.
            if beyond_attached_edge(rect, to) {
                let offscreen =
                    unsafe { MonitorFromPoint(info.pt, MONITOR_DEFAULTTONULL) }.is_null();
                to = clip_to_screen_edge(rect, to, offscreen);
            }
            let verdict = STATE.with_borrow_mut(|state| {
                state.on_move(
                    rect,
                    (from.x, from.y),
                    to,
                    info.time,
                    match BREAKTHROUGH.load(Ordering::Relaxed) {
                        u32::MAX => f64::INFINITY,
                        px => f64::from(px),
                    },
                )
            });
            match verdict {
                Verdict::Hold(x, y) => {
                    unsafe { SetCursorPos(x, y) };
                    return 1;
                }
                Verdict::Enter(b) => {
                    let packed = (1u64 << 63)
                        | (u64::from(b.inner) << 62)
                        | (u64::from(b.events.min(0x3fff_ffff)) << 32)
                        | u64::from(b.duration_ms);
                    LAST_BREAKTHROUGH.store(packed, Ordering::Relaxed);
                }
                Verdict::Allow => {}
            }
        }
    }
    unsafe { CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam) }
}

// --- Bringing a window to the front ---

use windows_sys::Win32::System::Threading::AttachThreadInput;
use windows_sys::Win32::UI::WindowsAndMessaging::{BringWindowToTop, GetWindowThreadProcessId};

/// Windows refuses `SetForegroundWindow` to an app that did not receive the
/// last input (e.g. opened from a second launch or after the tray menu closed):
/// the window then opens behind the others. Attaching to the input queue of
/// the current foreground window lifts that restriction for this call.
pub fn bring_to_front(window: &WebviewWindow) -> tauri::Result<()> {
    let hwnd = window.hwnd()?.0;
    unsafe {
        let foreground = GetForegroundWindow();
        let foreground_thread = GetWindowThreadProcessId(foreground, std::ptr::null_mut());
        let current_thread = GetCurrentThreadId();
        let attached = foreground_thread != 0
            && foreground_thread != current_thread
            && AttachThreadInput(current_thread, foreground_thread, 1) != 0;
        BringWindowToTop(hwnd);
        SetForegroundWindow(hwnd);
        if attached {
            AttachThreadInput(current_thread, foreground_thread, 0);
        }
    }
    Ok(())
}

// --- Volume and media keys taken over from Windows (ADR-0020) ---

use std::sync::Mutex;

use windows_sys::Win32::UI::Input::KeyboardAndMouse::{RegisterHotKey, UnregisterHotKey};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    KBDLLHOOKSTRUCT, WH_KEYBOARD_LL, WM_APP, WM_HOTKEY, WM_KEYDOWN, WM_SYSKEYDOWN,
};

use super::{Key, KeyGroup, KeyHandler};

/// Whether each group (`KeyGroup as usize`) is taken over: read by the hook
/// on every key of the system, without a lock.
static KEYS_ON: [AtomicBool; 2] = [const { AtomicBool::new(false) }; 2];
/// What each group's module does with its keys; locked only for a key taken.
static KEY_HANDLERS: Mutex<[Option<KeyHandler>; 2]> = Mutex::new([None, None]);
/// Thread id of the running keyboard hook thread, 0 when none.
static KEY_THREAD: AtomicU32 = AtomicU32::new(0);
/// A keyboard hook thread has been spawned and has not exited yet.
static KEY_RUNNING: AtomicBool = AtomicBool::new(false);

/// Takes a group of keys over from Windows (`Some`), or gives them back
/// (`None`). The hook runs while any group is taken.
pub fn capture_keys(group: KeyGroup, handler: Option<KeyHandler>) {
    let on = handler.is_some();
    KEY_HANDLERS.lock().unwrap()[group as usize] = handler;
    KEYS_ON[group as usize].store(on, Ordering::SeqCst);
    let thread = KEY_THREAD.load(Ordering::SeqCst);
    if thread != 0 {
        unsafe { PostThreadMessageW(thread, SYNC_HOTKEYS, 0, 0) };
    }
    if keys_wanted() {
        if !KEY_RUNNING.swap(true, Ordering::SeqCst) {
            std::thread::spawn(run_key_thread);
        }
    } else {
        let thread = KEY_THREAD.load(Ordering::SeqCst);
        if thread != 0 {
            unsafe { PostThreadMessageW(thread, WM_QUIT, 0, 0) };
        }
    }
}

fn keys_wanted() -> bool {
    KEYS_ON.iter().any(|on| on.load(Ordering::SeqCst))
}

fn run_key_thread() {
    unsafe {
        let hook = SetWindowsHookExW(
            WH_KEYBOARD_LL,
            Some(key_hook),
            GetModuleHandleW(std::ptr::null()),
            0,
        );
        if hook.is_null() {
            log::error!("cannot install the media keys hook");
        } else {
            KEY_THREAD.store(GetCurrentThreadId(), Ordering::SeqCst);
            log::info!("media keys taken over");
            let mut hotkeys = [false; KEYS.len()];
            // Given back while we were starting: leave right away.
            if keys_wanted() {
                sync_hotkeys(&mut hotkeys);
                let mut msg: MSG = zeroed();
                // Low-level hooks are called through this thread's message
                // loop, which also gets the hotkeys.
                while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
                    match msg.message {
                        WM_HOTKEY => {
                            if let Some(key) = key_of(msg.wParam as u32) {
                                dispatch(key);
                            }
                        }
                        SYNC_HOTKEYS => sync_hotkeys(&mut hotkeys),
                        _ => {}
                    }
                }
            }
            for (&(vk, _), registered) in KEYS.iter().zip(hotkeys) {
                if registered {
                    UnregisterHotKey(std::ptr::null_mut(), vk as i32);
                }
            }
            KEY_THREAD.store(0, Ordering::SeqCst);
            UnhookWindowsHookEx(hook);
            log::info!("media keys given back");
        }
    }
    KEY_RUNNING.store(false, Ordering::SeqCst);
    // Taken again while we were shutting down.
    if keys_wanted() && !KEY_RUNNING.swap(true, Ordering::SeqCst) {
        std::thread::spawn(run_key_thread);
    }
}

/// The keys we can take over, by virtual-key code.
const KEYS: [(u32, Key); 6] = [
    (0xAD, Key::VolumeMute),
    (0xAE, Key::VolumeDown),
    (0xAF, Key::VolumeUp),
    (0xB0, Key::Next),
    (0xB1, Key::Previous),
    (0xB3, Key::PlayPause),
];

/// Asks the key thread to register or free its hotkeys.
const SYNC_HOTKEYS: u32 = WM_APP + 1;

/// While one of our own windows is in front (the settings), Windows does
/// not call our keyboard hook: the key would reach the window and Windows
/// would show its flyout. The keys taken over are also registered as
/// hotkeys of the key thread, which Windows handles before the window gets
/// them; otherwise the hook takes them first. Registered with the key's
/// code as id, on the key thread only.
fn sync_hotkeys(registered: &mut [bool; KEYS.len()]) {
    for (&(vk, key), registered) in KEYS.iter().zip(registered) {
        let wanted = KEYS_ON[key.group() as usize].load(Ordering::SeqCst);
        if wanted && !*registered {
            *registered = unsafe { RegisterHotKey(std::ptr::null_mut(), vk as i32, 0, vk) } != 0;
            if !*registered {
                log::warn!("cannot register the hotkey {vk:#x}");
            }
        } else if !wanted && *registered {
            unsafe { UnregisterHotKey(std::ptr::null_mut(), vk as i32) };
            *registered = false;
        }
    }
}

/// Hands a key to its module, if its group is taken over; false lets it
/// through to Windows. Never waits: a busy lock lets the key through.
fn dispatch(key: Key) -> bool {
    if !KEYS_ON[key.group() as usize].load(Ordering::Relaxed) {
        return false;
    }
    let Ok(handlers) = KEY_HANDLERS.try_lock() else {
        return false;
    };
    let Some(handler) = handlers[key.group() as usize].clone() else {
        return false;
    };
    drop(handlers);
    handler(key);
    true
}

/// The key behind a virtual-key code, if it is one we can take over.
fn key_of(vk: u32) -> Option<Key> {
    KEYS.iter()
        .find(|(code, _)| *code == vk)
        .map(|(_, key)| *key)
}

/// Runs for every key of the whole system: other keys only cost a match on
/// their code. A key taken over goes to its module on the way down, and is
/// kept from Windows both ways (no flyout).
unsafe extern "system" fn key_hook(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        let info = unsafe { &*(lparam as *const KBDLLHOOKSTRUCT) };
        if let Some(key) = key_of(info.vkCode)
            && KEYS_ON[key.group() as usize].load(Ordering::Relaxed)
        {
            let down = wparam == WM_KEYDOWN as WPARAM || wparam == WM_SYSKEYDOWN as WPARAM;
            if !down {
                return 1;
            }
            if dispatch(key) {
                return 1;
            }
        }
    }
    unsafe { CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam) }
}

#[cfg(test)]
mod key_tests {
    use super::*;

    #[test]
    fn only_volume_and_media_keys_are_known() {
        assert_eq!(key_of(0xAF), Some(Key::VolumeUp));
        assert_eq!(key_of(0xB3), Some(Key::PlayPause));
        assert_eq!(key_of(0x41), None);
        assert_eq!(Key::VolumeMute.group(), KeyGroup::Volume);
        assert_eq!(Key::Next.group(), KeyGroup::Media);
    }
}
