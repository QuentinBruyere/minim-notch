//! Date and time module: one item with the local time and/or date, written
//! as the user chose. Off by default, the notch stays empty until the user
//! turns it on (ADR-0009).

mod format;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use chrono::{Local, Timelike};
use serde_json::Value;

use crate::module::{Compact, Host, Item, Module};
use format::Settings;

/// Longest wait between two checks of the clock: a sleeping computer may
/// overrun a single sleep until the next minute.
const MAX_WAIT: Duration = Duration::from_secs(5);

#[derive(Default)]
pub struct Clock {
    inner: Arc<Inner>,
}

#[derive(Default)]
struct Inner {
    host: OnceLock<Host>,
    settings: Mutex<Settings>,
    running: AtomicBool,
    ticker_started: AtomicBool,
}

impl Module for Clock {
    fn id(&self) -> &'static str {
        "clock"
    }

    fn name(&self) -> String {
        crate::t!("clock.name")
    }

    fn description(&self) -> String {
        crate::t!("clock.description")
    }

    fn enabled_by_default(&self) -> bool {
        false
    }

    fn start(&self, host: &Host) {
        let inner = &self.inner;
        let _ = inner.host.set(host.clone());
        *inner.settings.lock().unwrap() = host.load_settings();
        inner.running.store(true, Ordering::Relaxed);
        if !inner.ticker_started.swap(true, Ordering::Relaxed) {
            spawn_ticker(Arc::clone(inner));
        }
        host.refresh();
    }

    fn stop(&self) {
        self.inner.running.store(false, Ordering::Relaxed);
        if let Some(host) = self.inner.host.get() {
            host.refresh();
        }
    }

    fn items(&self) -> Vec<Item> {
        if !self.inner.running.load(Ordering::Relaxed) {
            return Vec::new();
        }
        let settings = self.inner.settings.lock().unwrap().clone();
        let now = Local::now().naive_local();
        let words = format::Words::of(crate::i18n::language());
        let (label, title) = format::format(now, &settings, &words);
        let (compact_label, compact_title) =
            format::format_as(now, settings.compact, &settings, &words);
        vec![Item {
            id: "now".into(),
            title,
            label,
            detail: None,
            compact: Some(Compact {
                label: compact_label,
                title: compact_title,
            }),
            // The time only: on the sides, hours over minutes instead of the
            // icon (DF-0027).
            stack: if settings.compact == format::Show::Time {
                format::stacked(now, &settings)
            } else {
                Vec::new()
            },
            ..Item::default()
        }]
    }

    fn settings(&self) -> Value {
        serde_json::to_value(&*self.inner.settings.lock().unwrap()).unwrap_or(Value::Null)
    }

    /// `update`: replaces the settings with `args` (the whole object).
    fn call(&self, action: &str, args: Value) -> Result<Value, String> {
        if action != "update" {
            return Err(format!("unknown action {action}"));
        }
        let settings: Settings =
            serde_json::from_value(args).map_err(|e| crate::t!("settings.invalid", error = e))?;
        *self.inner.settings.lock().unwrap() = settings.clone();
        if let Some(host) = self.inner.host.get() {
            host.save_settings(&settings);
            host.settings_changed();
            host.refresh();
        }
        Ok(Value::Null)
    }
}

/// Redraws the notch when the shown time changes (every minute, or every
/// second when seconds are shown), only while the module runs.
fn spawn_ticker(inner: Arc<Inner>) {
    std::thread::spawn(move || {
        let mut shown = None;
        loop {
            let seconds = inner.settings.lock().unwrap().seconds;
            let now = Local::now();
            let tick = (
                now.date_naive(),
                now.hour(),
                now.minute(),
                if seconds { now.second() } else { 0 },
            );
            if inner.running.load(Ordering::Relaxed) && shown != Some(tick) {
                shown = Some(tick);
                if let Some(host) = inner.host.get() {
                    host.refresh();
                }
            }
            let wait = if seconds {
                // Wake just after the next second starts.
                Duration::from_nanos(u64::from(1_000_000_000 - now.nanosecond() % 1_000_000_000))
            } else {
                Duration::from_secs(u64::from(60 - now.second()))
            };
            std::thread::sleep(wait.min(MAX_WAIT));
        }
    });
}
