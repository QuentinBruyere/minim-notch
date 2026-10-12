//! Timer module: counts down from a chosen duration, set and controlled from
//! its buttons in the open notch; rings when the time is up. Default and
//! favourite durations come from its settings. Off by default; not kept when
//! Minim Notch quits.

mod countdown;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::module::{Action, Compact, Host, Icon, Item, Module, Tone, format_duration};
use crate::t;
use countdown::{Countdown, Phase};

/// Check interval while stopped: nothing to redraw, just notice a start.
const IDLE_WAIT: Duration = Duration::from_millis(500);
const MAX_MINUTES: u64 = 600;
const MAX_FAVORITES: usize = 5;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct Settings {
    /// Duration of a new timer.
    default_minutes: u64,
    /// One-click durations, shown in the open notch while the timer is idle.
    favorites: Vec<u64>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            default_minutes: 5,
            favorites: vec![5, 15, 25],
        }
    }
}

impl Settings {
    fn default_duration(&self) -> Duration {
        minutes(self.default_minutes)
    }

    fn validate(mut self) -> Result<Self, String> {
        let valid = |m: u64| (1..=MAX_MINUTES).contains(&m);
        // A timer may start from zero: its minutes are then added in the notch.
        if self.default_minutes > MAX_MINUTES {
            return Err(t!("timer.duration_range", max = MAX_MINUTES));
        }
        if !self.favorites.iter().all(|&m| valid(m)) {
            return Err(t!("timer.favorites_range", max = MAX_MINUTES));
        }
        let mut seen = Vec::new();
        self.favorites.retain(|m| {
            let new = !seen.contains(m);
            seen.push(*m);
            new
        });
        if self.favorites.len() > MAX_FAVORITES {
            return Err(t!("timer.favorites_max", max = MAX_FAVORITES));
        }
        Ok(self)
    }
}

fn minutes(m: u64) -> Duration {
    Duration::from_secs(m * 60)
}

pub struct Timer {
    inner: Arc<Inner>,
}

impl Default for Timer {
    fn default() -> Self {
        let settings = Settings::default();
        Self {
            inner: Arc::new(Inner {
                host: OnceLock::new(),
                countdown: Mutex::new(Countdown::new(settings.default_duration())),
                settings: Mutex::new(settings),
                running: AtomicBool::new(false),
                ticker_started: AtomicBool::new(false),
            }),
        }
    }
}

struct Inner {
    host: OnceLock<Host>,
    settings: Mutex<Settings>,
    countdown: Mutex<Countdown>,
    running: AtomicBool,
    ticker_started: AtomicBool,
}

impl Inner {
    fn refresh(&self) {
        if let Some(host) = self.host.get() {
            host.refresh();
        }
    }

    fn reset(&self) {
        let duration = self.settings.lock().unwrap().default_duration();
        self.countdown.lock().unwrap().reset(duration);
    }
}

impl Module for Timer {
    fn id(&self) -> &'static str {
        "timer"
    }

    fn name(&self) -> String {
        crate::t!("timer.name")
    }

    fn description(&self) -> String {
        crate::t!("timer.description")
    }

    fn enabled_by_default(&self) -> bool {
        false
    }

    fn start(&self, host: &Host) {
        let inner = &self.inner;
        let _ = inner.host.set(host.clone());
        let settings: Settings = host.load_settings();
        let settings = settings.validate().unwrap_or_default();
        *inner.settings.lock().unwrap() = settings;
        inner.reset();
        inner.running.store(true, Ordering::Relaxed);
        if !inner.ticker_started.swap(true, Ordering::Relaxed) {
            spawn_ticker(Arc::clone(inner));
        }
        host.refresh();
    }

    fn stop(&self) {
        self.inner.running.store(false, Ordering::Relaxed);
        self.inner.reset();
        self.inner.refresh();
    }

    fn items(&self) -> Vec<Item> {
        if !self.inner.running.load(Ordering::Relaxed) {
            return Vec::new();
        }
        let countdown = *self.inner.countdown.lock().unwrap();
        let settings = self.inner.settings.lock().unwrap().clone();
        items(&countdown, &settings, Instant::now())
    }

    /// Clicking the notch silences a finished timer.
    fn acknowledge(&self) {
        let finished =
            self.inner.countdown.lock().unwrap().phase(Instant::now()) == Phase::Finished;
        if finished {
            self.inner.reset();
            self.inner.refresh();
        }
    }

    fn item_action(&self, _item: &str, action: &str) {
        let now = Instant::now();
        let default = self.inner.settings.lock().unwrap().default_duration();
        {
            let mut countdown = self.inner.countdown.lock().unwrap();
            match action.split_once(':') {
                None if action == "start" => countdown.start(now),
                None if action == "pause" => countdown.pause(now),
                None if action == "reset" => countdown.reset(default),
                Some(("add", m)) => {
                    if let Ok(m) = m.parse() {
                        countdown.add(minutes(m), now);
                    }
                }
                Some(("set", m)) => {
                    if let Ok(m) = m.parse() {
                        countdown.reset(minutes(m));
                    }
                }
                _ => return,
            }
        }
        self.inner.refresh();
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
        let settings = settings.validate()?;
        *self.inner.settings.lock().unwrap() = settings.clone();
        // A timer not started yet takes the new default duration.
        let idle = self.inner.countdown.lock().unwrap().phase(Instant::now()) == Phase::Idle;
        if idle {
            self.inner.reset();
        }
        if let Some(host) = self.inner.host.get() {
            host.save_settings(&settings);
            host.settings_changed();
            host.refresh();
        }
        Ok(Value::Null)
    }
}

fn items(countdown: &Countdown, settings: &Settings, now: Instant) -> Vec<Item> {
    let add = |m: u64| Action::text(&format!("add:{m}"), &t!("timer.add", minutes = m));
    let start = Action::icon("start", Icon::Play, &t!("action.start"));
    let pause = Action::icon("pause", Icon::Pause, &t!("action.pause"));
    let reset = Action::icon("reset", Icon::Reset, &t!("action.reset"));
    let remaining = format_duration(countdown.remaining_secs(now));

    let phase = countdown.phase(now);
    let (label, detail, tone, actions) = match phase {
        Phase::Idle => {
            let mut actions = vec![add(1), add(5)];
            // Nothing to count down from zero: add minutes first.
            if !countdown.duration.is_zero() {
                actions.push(start);
            }
            if countdown.duration != settings.default_duration() {
                actions.push(reset);
            }
            (remaining, None, Tone::Neutral, actions)
        }
        Phase::Running => (remaining, None, Tone::Active, vec![add(1), pause, reset]),
        Phase::Paused => (
            remaining,
            Some(t!("item.paused")),
            Tone::Neutral,
            vec![start, reset],
        ),
        // "Time's up · 25:00": what just ran out.
        Phase::Finished => (
            t!("timer.finished"),
            Some(format_duration(countdown.duration.as_secs())),
            Tone::Attention,
            vec![reset],
        ),
    };
    let mut items = vec![Item {
        id: "timer".into(),
        title: t!("timer.name"),
        label,
        detail,
        tone,
        dot: false,
        actions,
        // Not started: the compact views show the timer icon.
        quiet: phase == Phase::Idle,
        ringing: phase == Phase::Finished,
        // In use: the time left (or "Time's up") in a bubble on the sides.
        bubble: phase != Phase::Idle,
        // Closed notch and pin: the duration after "Time's up", fainter.
        compact: (phase == Phase::Finished).then(|| Compact {
            label: t!("timer.finished"),
            title: format_duration(countdown.duration.as_secs()),
        }),
        ..Item::default()
    }];
    if phase == Phase::Idle && !settings.favorites.is_empty() {
        items.push(Item {
            id: "favorites".into(),
            title: t!("timer.favorites"),
            actions: settings
                .favorites
                .iter()
                .map(|m| Action::text(&format!("set:{m}"), &t!("timer.minutes", minutes = m)))
                .collect(),
            ..Item::default()
        });
    }
    items
}

/// Redraws the notch every second while the timer runs, and once more when
/// the time is up (the notch then opens and rings).
fn spawn_ticker(inner: Arc<Inner>) {
    std::thread::spawn(move || {
        loop {
            let wait = inner
                .countdown
                .lock()
                .unwrap()
                .until_next_second(Instant::now());
            match wait {
                Some(wait) => {
                    std::thread::sleep(wait);
                    inner.countdown.lock().unwrap().settle(Instant::now());
                    if inner.running.load(Ordering::Relaxed) {
                        inner.refresh();
                    }
                }
                None => std::thread::sleep(IDLE_WAIT),
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(item: &Item) -> Vec<&str> {
        item.actions.iter().map(|a| a.id.as_str()).collect()
    }

    #[test]
    fn idle_timer_offers_durations() {
        let settings = Settings::default();
        let items = items(&Countdown::new(minutes(5)), &settings, Instant::now());
        assert_eq!(items[0].label, "05:00");
        assert_eq!(ids(&items[0]), ["add:1", "add:5", "start"]);
        assert_eq!(ids(&items[1]), ["set:5", "set:15", "set:25"]);
        assert_eq!(items[1].actions[1].label, "15 min");
    }

    #[test]
    fn finished_timer_asks_for_attention() {
        let t0 = Instant::now();
        let mut countdown = Countdown::new(minutes(1));
        countdown.start(t0);
        countdown.settle(t0 + minutes(1));
        let items = items(&countdown, &Settings::default(), t0 + minutes(2));
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].label, "Time's up");
        assert_eq!(items[0].detail.as_deref(), Some("01:00"));
        assert_eq!(items[0].tone, Tone::Attention);
        assert!(items[0].ringing);
        assert_eq!(ids(&items[0]), ["reset"]);
    }

    #[test]
    fn settings_are_checked() {
        let zero = Settings {
            default_minutes: 0,
            ..Settings::default()
        };
        assert!(zero.validate().is_ok());
        let bad = Settings {
            default_minutes: MAX_MINUTES + 1,
            ..Settings::default()
        };
        assert!(bad.validate().is_err());
        let duplicates = Settings {
            favorites: vec![5, 5, 10],
            ..Settings::default()
        };
        assert_eq!(duplicates.validate().unwrap().favorites, [5, 10]);
    }

    #[test]
    fn a_timer_at_zero_only_offers_to_add_minutes() {
        let t0 = Instant::now();
        let settings = Settings {
            default_minutes: 0,
            favorites: Vec::new(),
        };
        let mut countdown = Countdown::new(settings.default_duration());
        let at_zero = &items(&countdown, &settings, t0)[0];
        assert_eq!(at_zero.label, "00:00");
        assert_eq!(ids(at_zero), ["add:1", "add:5"]);
        assert!(at_zero.quiet);
        countdown.add(minutes(1), t0);
        let one_minute = &items(&countdown, &settings, t0)[0];
        assert_eq!(ids(one_minute), ["add:1", "add:5", "start", "reset"]);
        assert!(one_minute.quiet);
        countdown.start(t0);
        assert!(!items(&countdown, &settings, t0)[0].quiet);
    }
}
