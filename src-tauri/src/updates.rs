//! Updates (ADR-0013): Tauri's updater reads a manifest whose address is in
//! each version's own tauri.conf.json (GitHub Releases for the public version,
//! the website for the official one). A found update is only shown: it is
//! downloaded, checked against its signature and installed on the user's
//! click, never on its own.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_updater::{Update, UpdaterExt};

use crate::{AppState, t};

/// First automatic check, once Minim Notch has settled after startup.
const FIRST_CHECK: Duration = Duration::from_secs(30);
/// Then once a day.
const CHECK_EVERY: Duration = Duration::from_secs(24 * 60 * 60);
/// How often the watcher wakes up to see if a check is due: a computer that
/// slept through the day still checks soon after waking up.
const WAKE_EVERY: Duration = Duration::from_secs(60 * 60);

/// What the settings show about updates.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", tag = "state")]
pub enum Status {
    /// Not checked yet, or the last automatic check failed (silently).
    #[default]
    Idle,
    Checking,
    UpToDate,
    Available {
        version: String,
        notes: Option<String>,
    },
    /// Downloading `version`, in % once the size is known.
    Downloading {
        version: String,
        percent: Option<u8>,
    },
}

#[derive(Default)]
pub struct Updates {
    status: Mutex<Status>,
    /// The update behind `Status::Available`, installed on the user's click.
    pending: Mutex<Option<Update>>,
    last_check: Mutex<Option<SystemTime>>,
    /// A check or an install is running: never two at once.
    busy: AtomicBool,
}

/// The development version never checks (ADR-0013), unless
/// `MINIM_NOTCH_TEST_UPDATES` is set to try the whole path.
pub fn enabled() -> bool {
    !cfg!(debug_assertions) || std::env::var_os("MINIM_NOTCH_TEST_UPDATES").is_some()
}

pub fn status(app: &AppHandle) -> Status {
    app.state::<Updates>().status.lock().unwrap().clone()
}

fn set_status(app: &AppHandle, status: Status) {
    let before = std::mem::replace(
        &mut *app.state::<Updates>().status.lock().unwrap(),
        status.clone(),
    );
    let _ = app.emit("update-changed", &status);
    // The tray menu lists an available update.
    let available = |s: &Status| matches!(s, Status::Available { .. });
    if available(&before) != available(&status)
        && let Err(e) = crate::tray::retranslate(app)
    {
        log::warn!("cannot update the tray menu: {e}");
    }
}

/// A check is due when automatic checks are on and the last one is a day old.
fn due(auto: bool, last: Option<SystemTime>, now: SystemTime) -> bool {
    auto && last.is_none_or(|last| {
        // A clock set back counts as due.
        now.duration_since(last)
            .map_or(true, |elapsed| elapsed >= CHECK_EVERY)
    })
}

/// Checks soon after startup, then once a day, while the setting is on.
pub fn spawn_watcher(app: AppHandle) {
    if !enabled() {
        return;
    }
    std::thread::spawn(move || {
        std::thread::sleep(FIRST_CHECK);
        loop {
            let auto = app.state::<AppState>().config.lock().unwrap().check_updates;
            let last = *app.state::<Updates>().last_check.lock().unwrap();
            if due(auto, last, SystemTime::now()) {
                // Failures stay silent here: only a check the user asked for
                // shows them.
                let _ = tauri::async_runtime::block_on(check(&app, false));
            }
            std::thread::sleep(WAKE_EVERY);
        }
    });
}

/// `manual`: asked by the user, who sees the progress and the failures.
async fn check(app: &AppHandle, manual: bool) -> Result<(), String> {
    let updates = app.state::<Updates>();
    if updates.busy.swap(true, Ordering::AcqRel) {
        return Ok(());
    }
    let before = status(app);
    if manual {
        set_status(app, Status::Checking);
    }
    let result = match app.updater() {
        Ok(updater) => updater.check().await,
        Err(e) => Err(e),
    };
    *updates.last_check.lock().unwrap() = Some(SystemTime::now());
    let outcome = match result {
        Ok(Some(update)) => {
            log::info!("update available: {}", update.version);
            let status = Status::Available {
                version: update.version.clone(),
                notes: update.body.clone().filter(|notes| !notes.trim().is_empty()),
            };
            *updates.pending.lock().unwrap() = Some(update);
            set_status(app, status);
            Ok(())
        }
        Ok(None) => {
            *updates.pending.lock().unwrap() = None;
            set_status(app, Status::UpToDate);
            Ok(())
        }
        Err(e) => {
            log::warn!("cannot check for updates: {e}");
            set_status(app, before);
            Err(t!("updates.check_failed"))
        }
    };
    updates.busy.store(false, Ordering::Release);
    if manual { outcome } else { Ok(()) }
}

/// "Check now", available even with automatic checks off.
#[tauri::command]
pub async fn check_updates(app: AppHandle) -> Result<(), String> {
    if !enabled() {
        return Err(t!("settings.updates.dev"));
    }
    check(&app, true).await
}

/// Downloads the update found, checks its signature, installs it and
/// restarts. On Windows the installer takes over and Minim Notch quits.
#[tauri::command]
pub async fn install_update(app: AppHandle) -> Result<(), String> {
    let updates = app.state::<Updates>();
    let Some(update) = updates.pending.lock().unwrap().clone() else {
        return Err(t!("updates.none"));
    };
    if updates.busy.swap(true, Ordering::AcqRel) {
        return Ok(());
    }
    let before = status(&app);
    let version = update.version.clone();
    set_status(
        &app,
        Status::Downloading {
            version: version.clone(),
            percent: None,
        },
    );
    let mut received = 0u64;
    let mut shown = None;
    let result = update
        .download_and_install(
            |chunk, total| {
                received += chunk as u64;
                let percent = total
                    .filter(|&total| total > 0)
                    .map(|total| (received * 100 / total).min(100) as u8);
                if percent != shown {
                    shown = percent;
                    set_status(
                        &app,
                        Status::Downloading {
                            version: version.clone(),
                            percent,
                        },
                    );
                }
            },
            || log::info!("update {version} downloaded, installing"),
        )
        .await;
    match result {
        // Reached on macOS and Linux only: Windows has quit for the installer.
        Ok(()) => app.restart(),
        Err(e) => {
            log::error!("cannot install the update: {e}");
            set_status(&app, before);
            updates.busy.store(false, Ordering::Release);
            Err(t!("updates.install_failed"))
        }
    }
}

/// The update found, for the tray menu.
pub fn available_version(app: &AppHandle) -> Option<String> {
    match status(app) {
        Status::Available { version, .. } => Some(version),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOUR: Duration = Duration::from_secs(3600);

    #[test]
    fn first_check_is_due() {
        assert!(due(true, None, SystemTime::now()));
    }

    #[test]
    fn never_due_when_off() {
        assert!(!due(false, None, SystemTime::now()));
    }

    #[test]
    fn due_once_a_day() {
        let now = SystemTime::now();
        assert!(!due(true, Some(now - 23 * HOUR), now));
        assert!(due(true, Some(now - 24 * HOUR), now));
    }

    #[test]
    fn clock_set_back_is_due() {
        let now = SystemTime::now();
        assert!(due(true, Some(now + HOUR), now));
    }
}
