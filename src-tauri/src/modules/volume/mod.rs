//! Volume module (DF-0013): the volume of the default audio output, shown
//! and set from the notch. Windows only for now: Core Audio tells it about
//! every change (keys, Windows mixer, other apps), so it never polls.

#[cfg(windows)]
mod endpoint;

use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex, OnceLock};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::module::{Action, Compact, Host, Icon, Item, Module, Slider};
use crate::platform::{self, Key, KeyGroup};
use crate::t;

/// How long the closed notch shows a volume changed elsewhere.
const SPOTLIGHT: std::time::Duration = std::time::Duration::from_millis(1500);

/// The module's settings.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub(crate) struct Settings {
    /// The volume keys are handled here and Windows shows nothing
    /// (ADR-0020). Windows only.
    pub replace_windows: bool,
}

/// The output's volume, as shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Level {
    /// 0 to 100.
    pub percent: u8,
    pub muted: bool,
}

/// What the notch asks the audio thread to do.
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) enum Command {
    Set(u8),
    ToggleMute,
    /// A volume key taken over from Windows.
    Key(Key),
    /// The default output changed: follow the new one.
    DeviceChanged,
    Stop,
}

#[derive(Default)]
pub struct Volume {
    inner: Arc<Inner>,
}

#[derive(Default)]
struct Inner {
    host: OnceLock<Host>,
    settings: Mutex<Settings>,
    /// `None`: no audio output (or not read yet).
    level: Mutex<Option<Level>>,
    /// To the audio thread, while the module runs.
    commands: Mutex<Option<Sender<Command>>>,
}

impl Inner {
    /// A new level from the audio thread; `external`: not set from the notch,
    /// which then shows it briefly.
    fn changed(self: &Arc<Self>, level: Option<Level>, external: bool) {
        let had = std::mem::replace(&mut *self.level.lock().unwrap(), level).is_some();
        // An output appeared or went away: the keys follow.
        if had != level.is_some() {
            self.capture_keys();
        }
        if let Some(host) = self.host.get() {
            host.refresh();
            if external && level.is_some() {
                host.spotlight(SPOTLIGHT);
            }
        }
    }

    /// Takes the volume keys over while asked to and there is an output to
    /// act on; else Windows keeps them (ADR-0020).
    fn capture_keys(self: &Arc<Self>) {
        // One lock at a time (CLAUDE.md, Verrous).
        let asked = self.settings.lock().unwrap().replace_windows;
        let running = self.commands.lock().unwrap().is_some();
        let output = self.level.lock().unwrap().is_some();
        let wanted = asked && running && output;
        let inner = Arc::clone(self);
        platform::capture_keys(
            KeyGroup::Volume,
            wanted.then(|| -> platform::KeyHandler {
                Arc::new(move |key| inner.send(Command::Key(key)))
            }),
        );
    }

    fn send(&self, command: Command) {
        if let Some(commands) = self.commands.lock().unwrap().as_ref() {
            let _ = commands.send(command);
        }
    }
}

impl Module for Volume {
    fn id(&self) -> &'static str {
        "volume"
    }

    fn name(&self) -> String {
        crate::t!("volume.name")
    }

    fn description(&self) -> String {
        crate::t!("volume.description")
    }

    fn enabled_by_default(&self) -> bool {
        false
    }

    fn start(&self, host: &Host) {
        let _ = self.inner.host.set(host.clone());
        *self.inner.settings.lock().unwrap() = host.load_settings();
        #[cfg(windows)]
        {
            let inner = Arc::clone(&self.inner);
            let commands = endpoint::spawn(move |level, external| inner.changed(level, external));
            *self.inner.commands.lock().unwrap() = Some(commands);
        }
        self.inner.capture_keys();
        host.refresh();
    }

    fn stop(&self) {
        self.inner.send(Command::Stop);
        *self.inner.commands.lock().unwrap() = None;
        *self.inner.level.lock().unwrap() = None;
        self.inner.capture_keys();
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
        }
        Ok(Value::Null)
    }

    fn items(&self) -> Vec<Item> {
        let level = *self.inner.level.lock().unwrap();
        level.map(item).into_iter().collect()
    }

    fn placeholder(&self) -> Option<String> {
        Some(if cfg!(windows) {
            t!("volume.no_output")
        } else {
            t!("volume.windows_only")
        })
    }

    /// `mute` toggles the sound, `set:<0-100>` comes from the slider.
    fn item_action(&self, _item: &str, action: &str) {
        match action.split_once(':') {
            None if action == "mute" => self.inner.send(Command::ToggleMute),
            Some(("set", value)) => {
                if let Ok(value) = value.parse::<f64>() {
                    self.inner
                        .send(Command::Set(value.round().clamp(0.0, 100.0) as u8));
                }
            }
            _ => {}
        }
    }
}

/// The speaker matching the level, like Windows: crossed out when silent,
/// more waves as it gets louder.
fn level_icon(level: Level) -> Icon {
    match level.percent {
        _ if level.muted => Icon::Muted,
        0 => Icon::Muted,
        1..=33 => Icon::VolumeLow,
        34..=66 => Icon::VolumeMedium,
        _ => Icon::VolumeHigh,
    }
}

fn item(level: Level) -> Item {
    let icon = level_icon(level);
    let tooltip = if level.muted {
        t!("volume.unmute")
    } else {
        t!("volume.mute")
    };
    Item {
        id: "level".into(),
        title: t!("volume.name"),
        label: if level.muted {
            t!("volume.muted")
        } else {
            t!("volume.percent", percent = level.percent)
        },
        slider: Some(Slider {
            value: f64::from(level.percent),
            min: 0.0,
            max: 100.0,
            step: 1.0,
            readonly: false,
        }),
        // The level's icon is the mute button, wherever it shows.
        icon_action: Some(Action::icon("mute", icon, &tooltip)),
        // Muted: the compact views say so instead of the level (the closed
        // notch: instead of the bar and the number, DF-0027).
        compact: level.muted.then(|| Compact {
            label: t!("volume.muted"),
            title: String::new(),
        }),
        // The compact views show the icon, and the value only for a moment
        // after a change (`Host::spotlight`), like the stopwatch at rest.
        quiet: true,
        // Compact views: icon, bar and number, or "Muted" (DF-0027).
        meter: true,
        icon: Some(icon),
        ..Item::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_item_shows_the_percentage_or_muted() {
        let on = item(Level {
            percent: 45,
            muted: false,
        });
        assert_eq!(on.label, "45%");
        assert_eq!(on.slider.as_ref().map(|s| s.value), Some(45.0));
        assert!(on.actions.is_empty());
        assert_eq!(on.icon_action.as_ref().map(|a| a.id.as_str()), Some("mute"));
        assert_eq!(on.icon, Some(Icon::VolumeMedium));
        assert!(on.quiet);
        assert!(on.meter);
        let off = item(Level {
            percent: 45,
            muted: true,
        });
        assert_eq!(off.label, "Muted");
        assert_eq!(
            off.compact.as_ref().map(|c| c.label.as_str()),
            Some("Muted")
        );
        assert!(on.compact.is_none());
        assert_eq!(
            off.icon_action.as_ref().and_then(|a| a.icon),
            Some(Icon::Muted)
        );
    }

    #[test]
    fn the_icon_follows_the_level() {
        let at = |percent, muted| level_icon(Level { percent, muted });
        assert_eq!(at(0, false), Icon::Muted);
        assert_eq!(at(80, true), Icon::Muted);
        assert_eq!(at(1, false), Icon::VolumeLow);
        assert_eq!(at(33, false), Icon::VolumeLow);
        assert_eq!(at(34, false), Icon::VolumeMedium);
        assert_eq!(at(66, false), Icon::VolumeMedium);
        assert_eq!(at(67, false), Icon::VolumeHigh);
        assert_eq!(at(100, false), Icon::VolumeHigh);
    }
}
