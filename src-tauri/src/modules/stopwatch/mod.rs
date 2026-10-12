//! Stopwatch module: counts up, started, paused and reset from its buttons
//! in the open notch. Off by default; not kept when Minim Notch quits.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use super::watch::Watch;
use crate::module::{Action, Activity, Host, Icon, Item, Module, Tone, format_duration};
use crate::t;

/// Check interval while stopped: nothing to redraw, just notice a start.
const IDLE_WAIT: Duration = Duration::from_millis(500);

#[derive(Default)]
pub struct Stopwatch {
    inner: Arc<Inner>,
}

#[derive(Default)]
struct Inner {
    host: OnceLock<Host>,
    watch: Mutex<Watch>,
    running: AtomicBool,
    ticker_started: AtomicBool,
}

impl Module for Stopwatch {
    fn id(&self) -> &'static str {
        "stopwatch"
    }

    fn name(&self) -> String {
        crate::t!("stopwatch.name")
    }

    fn description(&self) -> String {
        crate::t!("stopwatch.description")
    }

    fn enabled_by_default(&self) -> bool {
        false
    }

    fn start(&self, host: &Host) {
        let inner = &self.inner;
        let _ = inner.host.set(host.clone());
        inner.running.store(true, Ordering::Relaxed);
        if !inner.ticker_started.swap(true, Ordering::Relaxed) {
            spawn_ticker(Arc::clone(inner));
        }
        host.refresh();
    }

    fn stop(&self) {
        self.inner.running.store(false, Ordering::Relaxed);
        self.inner.watch.lock().unwrap().reset();
        if let Some(host) = self.inner.host.get() {
            host.refresh();
        }
    }

    fn items(&self) -> Vec<Item> {
        if !self.inner.running.load(Ordering::Relaxed) {
            return Vec::new();
        }
        let watch = *self.inner.watch.lock().unwrap();
        vec![item(&watch, Instant::now())]
    }

    fn item_action(&self, _item: &str, action: &str) {
        let now = Instant::now();
        {
            let mut watch = self.inner.watch.lock().unwrap();
            match action {
                "start" => watch.start(now),
                "pause" => watch.pause(now),
                "reset" => watch.reset(),
                _ => return,
            }
        }
        if let Some(host) = self.inner.host.get() {
            host.refresh();
        }
    }
}

fn item(watch: &Watch, now: Instant) -> Item {
    let mut actions = Vec::new();
    if watch.running() {
        actions.push(Action::icon("pause", Icon::Pause, &t!("action.pause")));
    } else {
        actions.push(Action::icon("start", Icon::Play, &t!("action.start")));
    }
    if !watch.idle() {
        actions.push(Action::icon("reset", Icon::Reset, &t!("action.reset")));
    }
    let paused = !watch.running() && !watch.idle();
    Item {
        id: "stopwatch".into(),
        title: t!("stopwatch.name"),
        label: format_duration(watch.elapsed(now).as_secs()),
        detail: paused.then(|| t!("item.paused")),
        tone: if watch.running() {
            Tone::Active
        } else {
            Tone::Neutral
        },
        dot: false,
        actions,
        // Never started: the compact views show the stopwatch icon.
        quiet: watch.idle(),
        activity: watch.running().then_some(Activity::Running),
        // In use (running or paused): its time in a bubble on the sides.
        bubble: !watch.idle(),
        ..Item::default()
    }
}

/// Redraws the notch every second while the stopwatch runs.
fn spawn_ticker(inner: Arc<Inner>) {
    std::thread::spawn(move || {
        loop {
            let watch = *inner.watch.lock().unwrap();
            match watch.until_next_second(Instant::now()) {
                Some(wait) => {
                    std::thread::sleep(wait);
                    if inner.running.load(Ordering::Relaxed)
                        && let Some(host) = inner.host.get()
                    {
                        host.refresh();
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
    fn buttons_follow_the_state() {
        let t0 = Instant::now();
        let mut watch = Watch::default();
        assert_eq!(ids(&item(&watch, t0)), ["start"]);
        watch.start(t0);
        let running = item(&watch, t0 + Duration::from_secs(65));
        assert_eq!(ids(&running), ["pause", "reset"]);
        assert_eq!(running.label, "01:05");
        assert_eq!(running.tone, Tone::Active);
        watch.pause(t0 + Duration::from_secs(65));
        let paused = item(&watch, t0 + Duration::from_secs(99));
        assert_eq!(ids(&paused), ["start", "reset"]);
        assert_eq!(paused.detail.as_deref(), Some("paused"));
    }
}
