//! Media module (DF-0017): what is playing on the computer, from whichever
//! app (Spotify, browsers, players), with its cover, and controlled from the
//! notch. Windows only for now: its media transport controls tell the module
//! about every change, so it never polls (but for the progress bar, which
//! moves every second while playing).

#[cfg(windows)]
mod session;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use crate::module::{Action, Activity, Compact, Host, Icon, Item, Layout, Module, Slider, Tone};
use crate::platform::{self, Key, KeyGroup};
use crate::t;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The module's settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub(crate) struct Settings {
    /// Closed notch and pin: a "Title – Artist" too long scrolls.
    pub scroll: bool,
    /// Closed notch: only the cover; the text shows for a moment when the
    /// track changes.
    pub hide_text: bool,
    /// The same in a pin.
    pub hide_text_pin: bool,
    /// Closed notch: the track's progress as a line along its outline.
    pub progress: bool,
    /// The media keys are handled here and Windows shows nothing
    /// (ADR-0020). Windows only.
    pub replace_windows: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            scroll: true,
            hide_text: false,
            hide_text_pin: true,
            progress: true,
            replace_windows: false,
        }
    }
}

/// How long the closed notch shows a new track.
const SPOTLIGHT: Duration = Duration::from_secs(3);

/// What is playing, as shown.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Track {
    pub title: String,
    pub artist: String,
    /// `data:` URL of the cover, if the app gives one.
    pub cover: Option<String>,
    pub playing: bool,
    pub can_previous: bool,
    pub can_next: bool,
    pub can_toggle: bool,
    /// Only if the app gives it (browsers often do not).
    pub timeline: Option<Timeline>,
    /// The app gave a progress before: while a new track loads (no timeline
    /// yet), an empty bar keeps its place, so the texts do not jump.
    pub expects_timeline: bool,
}

/// Where the track is, as read at `at`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Timeline {
    pub position: Duration,
    pub duration: Duration,
    pub at: Instant,
    pub can_seek: bool,
}

impl Track {
    /// The position now: it moves on by itself while playing.
    fn position(&self, timeline: &Timeline, now: Instant) -> Duration {
        let moved = if self.playing {
            now.saturating_duration_since(timeline.at)
        } else {
            Duration::ZERO
        };
        (timeline.position + moved).min(timeline.duration)
    }

    /// Same track, whatever its state: another one shows briefly.
    fn same_as(&self, other: &Track) -> bool {
        self.title == other.title && self.artist == other.artist
    }
}

/// What the notch asks the media thread to do.
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) enum Command {
    /// Something changed in Windows: read the current track again.
    Refresh,
    Previous,
    TogglePlayPause,
    Next,
    Seek(Duration),
    Stop,
}

#[derive(Default)]
pub struct Media {
    inner: Arc<Inner>,
}

#[derive(Default)]
struct Inner {
    host: OnceLock<Host>,
    settings: Mutex<Settings>,
    /// `None`: nothing to show (no media app, or not read yet).
    track: Mutex<Option<Track>>,
    running: AtomicBool,
    /// To the media thread, while the module runs.
    commands: Mutex<Option<Sender<Command>>>,
    ticker_started: AtomicBool,
    /// The media thread gave a first state since the module started: the
    /// next tracks are new ones, shown briefly.
    read_once: AtomicBool,
}

impl Inner {
    /// A new state from the media thread.
    fn changed(self: &Arc<Self>, track: Option<Track>) {
        let previous = std::mem::replace(&mut *self.track.lock().unwrap(), track.clone());
        // Something to control appeared or went away: the keys follow.
        if previous.is_some() != track.is_some() {
            self.capture_keys();
        }
        let started = self.read_once.swap(true, Ordering::Relaxed);
        let Some(host) = self.host.get() else { return };
        host.refresh();
        // Another track, or one starting after nothing played: shown briefly
        // (not the very first state, at launch).
        if let Some(now) = &track
            && started
            && !now.title.is_empty()
            && previous.as_ref().is_none_or(|before| !now.same_as(before))
        {
            host.spotlight(SPOTLIGHT);
        }
    }

    /// Takes the media keys over while asked to and something is there to
    /// control; else Windows keeps them (ADR-0020).
    fn capture_keys(self: &Arc<Self>) {
        // One lock at a time (CLAUDE.md, Verrous).
        let asked = self.settings.lock().unwrap().replace_windows;
        let running = self.commands.lock().unwrap().is_some();
        let playing = self.track.lock().unwrap().is_some();
        let inner = Arc::clone(self);
        platform::capture_keys(
            KeyGroup::Media,
            (asked && running && playing)
                .then(|| -> platform::KeyHandler { Arc::new(move |key| inner.key(key)) }),
        );
    }

    /// A media key taken over from Windows: exactly like the notch's buttons.
    /// A new track shows briefly as always; play / pause only changes the
    /// shown state (the faded cover). Over a fullscreen app, which hides the
    /// notch, it shows briefly in Windows' stead. Called on the key hook's
    /// thread: the notch is shown from a thread of its own.
    fn key(&self, key: Key) {
        self.send(match key {
            Key::PlayPause => Command::TogglePlayPause,
            Key::Next => Command::Next,
            Key::Previous => Command::Previous,
            _ => return,
        });
        if let Some(host) = self.host.get().cloned()
            && host.hidden_by_fullscreen()
        {
            std::thread::spawn(move || {
                // The app's new state first.
                std::thread::sleep(Duration::from_millis(150));
                host.reveal(SPOTLIGHT);
            });
        }
    }

    fn send(&self, command: Command) {
        if let Some(commands) = self.commands.lock().unwrap().as_ref() {
            let _ = commands.send(command);
        }
    }
}

impl Module for Media {
    fn id(&self) -> &'static str {
        "media"
    }

    fn name(&self) -> String {
        t!("media.name")
    }

    fn description(&self) -> String {
        t!("media.description")
    }

    fn start(&self, host: &Host) {
        let _ = self.inner.host.set(host.clone());
        *self.inner.settings.lock().unwrap() = host.load_settings();
        self.inner.running.store(true, Ordering::Relaxed);
        #[cfg(windows)]
        {
            let inner = Arc::clone(&self.inner);
            let commands = session::spawn(move |track| inner.changed(track));
            *self.inner.commands.lock().unwrap() = Some(commands);
        }
        self.inner.capture_keys();
        if !self.inner.ticker_started.swap(true, Ordering::Relaxed) {
            spawn_ticker(Arc::clone(&self.inner));
        }
        host.refresh();
    }

    fn stop(&self) {
        self.inner.running.store(false, Ordering::Relaxed);
        self.inner.read_once.store(false, Ordering::Relaxed);
        self.inner.send(Command::Stop);
        *self.inner.commands.lock().unwrap() = None;
        *self.inner.track.lock().unwrap() = None;
        self.inner.capture_keys();
    }

    fn items(&self) -> Vec<Item> {
        if !cfg!(windows) || !self.inner.running.load(Ordering::Relaxed) {
            return Vec::new();
        }
        let track = self.inner.track.lock().unwrap().clone();
        let settings = self.inner.settings.lock().unwrap().clone();
        match track {
            Some(track) => items(&track, &settings, Instant::now()),
            None => vec![idle()],
        }
    }

    fn placeholder(&self) -> Option<String> {
        Some(if cfg!(windows) {
            t!("media.none")
        } else {
            t!("media.windows_only")
        })
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
            serde_json::from_value(args).map_err(|e| t!("settings.invalid", error = e))?;
        *self.inner.settings.lock().unwrap() = settings.clone();
        self.inner.capture_keys();
        if let Some(host) = self.inner.host.get() {
            host.save_settings(&settings);
            host.settings_changed();
            host.refresh();
        }
        Ok(Value::Null)
    }

    /// `previous`, `toggle`, `next`, and `set:<seconds>` from the progress bar.
    fn item_action(&self, _item: &str, action: &str) {
        let command = match action.split_once(':') {
            Some(("set", value)) => match value.parse::<f64>() {
                Ok(seconds) if seconds >= 0.0 => Command::Seek(Duration::from_secs_f64(seconds)),
                _ => return,
            },
            _ => match action {
                "previous" => Command::Previous,
                "toggle" => Command::TogglePlayPause,
                "next" => Command::Next,
                _ => return,
            },
        };
        self.inner.send(command);
    }
}

/// Nothing playing: the module stays, with its icon (closed notch, pin) and
/// "No media" by it when open.
fn idle() -> Item {
    Item {
        id: "idle".into(),
        title: t!("media.none"),
        // Compact views: the module's icon alone; open: the icon in the
        // cover's place and "No media".
        quiet: true,
        compact: Some(Compact::default()),
        layout: Layout::Featured,
        ..Item::default()
    }
}

/// The track's line, and its progress line when the app gives a timeline.
fn items(track: &Track, settings: &Settings, now: Instant) -> Vec<Item> {
    let title = if track.title.is_empty() {
        t!("media.untitled")
    } else {
        track.title.clone()
    };
    let mut actions = Vec::new();
    if track.can_previous {
        actions.push(Action::icon(
            "previous",
            Icon::Previous,
            &t!("media.previous"),
        ));
    }
    if track.can_toggle {
        actions.push(if track.playing {
            Action::icon("toggle", Icon::Pause, &t!("action.pause"))
        } else {
            Action::icon("toggle", Icon::Play, &t!("media.play"))
        });
    }
    if track.can_next {
        actions.push(Action::icon("next", Icon::Next, &t!("media.next")));
    }
    // Where the track is, when the app tells it (browsers often do not).
    let timeline = track.timeline.filter(|t| !t.duration.is_zero());
    let position = timeline.map(|t| track.position(&t, now));
    vec![Item {
        id: "track".into(),
        title: title.clone(),
        label: track.artist.clone(),
        detail: match timeline.zip(position) {
            Some((t, p)) => Some(t!(
                "media.position",
                position = clock(p),
                duration = clock(t.duration)
            )),
            None if track.expects_timeline => Some(t!(
                "media.position",
                position = clock(Duration::ZERO),
                duration = clock(Duration::ZERO)
            )),
            None => None,
        },
        tone: if track.playing {
            Tone::Active
        } else {
            Tone::Neutral
        },
        actions,
        // Without a cover, the module's icon in its place.
        image: Some(track.cover.clone().unwrap_or_default()),
        dimmed: !track.playing,
        activity: track.playing.then_some(Activity::Music),
        // Closed notch and pin: the cover and "Title – Artist".
        compact: Some(Compact {
            label: compact_text(&title, &track.artist),
            title: String::new(),
        }),
        scroll: settings.scroll,
        // The cover alone; a new track still shows its text (spotlight).
        quiet: settings.hide_text,
        pin_quiet: Some(settings.hide_text_pin),
        outline: settings.progress,
        // Always the bar; it only moves the track if the app lets it seek.
        slider: match timeline.zip(position) {
            Some((t, p)) => Some(Slider {
                value: p.as_secs() as f64,
                min: 0.0,
                max: t.duration.as_secs() as f64,
                step: 1.0,
                readonly: !t.can_seek,
            }),
            // A new track loading: the empty bar keeps its place.
            None if track.expects_timeline => Some(Slider {
                value: 0.0,
                min: 0.0,
                max: 1.0,
                step: 1.0,
                readonly: true,
            }),
            None => None,
        },
        // Open notch: the large cover, then title, artist, progress, buttons.
        layout: Layout::Featured,
        ..Item::default()
    }]
}

/// "Title – Artist", or the title alone.
fn compact_text(title: &str, artist: &str) -> String {
    if artist.is_empty() {
        title.to_owned()
    } else {
        format!("{title} – {artist}")
    }
}

/// "3:07", "1:02:03".
fn clock(duration: Duration) -> String {
    let s = duration.as_secs();
    let (h, m, s) = (s / 3600, s / 60 % 60, s % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

/// Moves the progress bar every second while a track plays.
fn spawn_ticker(inner: Arc<Inner>) {
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(Duration::from_secs(1));
            let moving = inner
                .track
                .lock()
                .unwrap()
                .as_ref()
                .is_some_and(|t| t.playing && t.timeline.is_some());
            if moving
                && inner.running.load(Ordering::Relaxed)
                && let Some(host) = inner.host.get()
            {
                host.refresh();
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(playing: bool) -> Track {
        Track {
            title: "Bohemian Rhapsody".into(),
            artist: "Queen".into(),
            cover: None,
            playing,
            can_previous: true,
            can_next: true,
            can_toggle: true,
            timeline: None,
            expects_timeline: false,
        }
    }

    #[test]
    fn a_playing_track_shows_pause_and_title_with_artist_when_compact() {
        let items = items(&track(true), &Settings::default(), Instant::now());
        assert_eq!(items.len(), 1);
        let ids: Vec<_> = items[0].actions.iter().map(|a| a.id.as_str()).collect();
        assert_eq!(ids, ["previous", "toggle", "next"]);
        assert_eq!(items[0].actions[1].icon, Some(Icon::Pause));
        assert_eq!(
            items[0].compact.as_ref().unwrap().label,
            "Bohemian Rhapsody – Queen"
        );
        assert!(items[0].scroll);
        assert!(!items[0].dimmed);
    }

    #[test]
    fn a_paused_track_is_dimmed_with_play() {
        let items = items(&track(false), &Settings::default(), Instant::now());
        assert!(items[0].dimmed);
        assert_eq!(items[0].actions[1].icon, Some(Icon::Play));
    }

    #[test]
    fn buttons_the_app_refuses_are_hidden() {
        let mut first = track(true);
        first.can_previous = false;
        let ids: Vec<_> = items(&first, &Settings::default(), Instant::now())[0]
            .actions
            .iter()
            .map(|a| a.id.clone())
            .collect();
        assert_eq!(ids, ["toggle", "next"]);
    }

    #[test]
    fn the_position_moves_while_playing_and_stops_at_the_end() {
        let at = Instant::now();
        let timeline = Timeline {
            position: Duration::from_secs(60),
            duration: Duration::from_secs(200),
            at,
            can_seek: true,
        };
        let later = at + Duration::from_secs(5);
        assert_eq!(
            track(true).position(&timeline, later),
            Duration::from_secs(65)
        );
        assert_eq!(
            track(false).position(&timeline, later),
            Duration::from_secs(60)
        );
        let much_later = at + Duration::from_secs(500);
        assert_eq!(
            track(true).position(&timeline, much_later),
            Duration::from_secs(200)
        );
    }

    /// The module's texts, as Minim Notch adds them at launch (tests run in English).

    #[test]
    fn the_progress_line_shows_where_the_track_is() {
        let at = Instant::now();
        let mut playing = track(true);
        playing.timeline = Some(Timeline {
            position: Duration::from_secs(83),
            duration: Duration::from_secs(225),
            at,
            can_seek: true,
        });
        let items = items(&playing, &Settings::default(), at);
        assert_eq!(items[0].detail.as_deref(), Some("1:23 / 3:45"));
        assert_eq!(items[0].slider.as_ref().map(|s| s.max), Some(225.0));
    }

    #[test]
    fn clock_writes_hours_only_when_needed() {
        assert_eq!(clock(Duration::from_secs(187)), "3:07");
        assert_eq!(clock(Duration::from_secs(3723)), "1:02:03");
    }

    #[test]
    fn only_another_track_counts_as_new() {
        let playing = track(true);
        assert!(playing.same_as(&track(false)));
        let mut next = track(true);
        next.title = "Under Pressure".into();
        assert!(!playing.same_as(&next));
    }

    #[test]
    fn hiding_the_text_leaves_the_cover_alone() {
        let settings = Settings {
            hide_text: true,
            ..Settings::default()
        };
        for playing in [true, false] {
            let item = &items(&track(playing), &settings, Instant::now())[0];
            assert!(item.quiet);
            // Kept for the moment a new track shows.
            assert_eq!(
                item.compact.as_ref().unwrap().label,
                "Bohemian Rhapsody – Queen"
            );
        }
        assert!(!items(&track(true), &Settings::default(), Instant::now())[0].quiet);
    }

    #[test]
    fn the_progress_bar_shows_even_when_the_app_cannot_seek() {
        let at = Instant::now();
        let mut playing = track(true);
        playing.timeline = Some(Timeline {
            position: Duration::from_secs(10),
            duration: Duration::from_secs(100),
            at,
            can_seek: false,
        });
        let slider = items(&playing, &Settings::default(), at)[0].slider.clone();
        assert!(slider.is_some_and(|s| s.readonly));
    }

    #[test]
    fn a_loading_track_keeps_an_empty_bar_if_the_app_gives_a_progress() {
        let mut loading = track(true);
        loading.expects_timeline = true;
        let item = &items(&loading, &Settings::default(), Instant::now())[0];
        assert_eq!(item.detail.as_deref(), Some("0:00 / 0:00"));
        assert!(
            item.slider
                .as_ref()
                .is_some_and(|s| s.readonly && s.value == 0.0)
        );
        let browser = &items(&track(true), &Settings::default(), Instant::now())[0];
        assert!(browser.slider.is_none() && browser.detail.is_none());
    }

    #[test]
    fn nothing_playing_keeps_the_module_with_its_icon() {
        let item = idle();
        assert!(item.quiet);
        assert_eq!(item.image, None);
        assert_eq!(item.title, "No media");
        assert_eq!(item.layout, Layout::Featured);
    }
}
