//! Module contract, see docs/adr/0009-systeme-de-modules.md. The core only
//! draws the notch: everything it shows comes from modules, as generic items.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager};

use crate::config::PinSide;
use crate::{AppState, config};

/// How an item looks and how urgent it is: decides its colour, which item
/// the compact notch shows first, and the sound when an item enters it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tone {
    #[default]
    Neutral,
    /// Something is running: the dot pulses.
    Active,
    /// The user must act (e.g. a permission).
    Attention,
    /// The user is asked a question.
    Question,
    Success,
    Error,
}

/// Icons the front knows: for action buttons, and for an item in the compact
/// views (`Item::icon`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Icon {
    Play,
    Pause,
    Reset,
    /// A speaker without waves: the sound is low.
    VolumeLow,
    /// A speaker with one wave.
    VolumeMedium,
    /// A speaker with two waves: the sound is loud.
    VolumeHigh,
    /// A crossed-out speaker: no sound.
    Muted,
    /// Back to the previous track.
    Previous,
    /// On to the next track.
    Next,
}

/// What an item is busy with, for whoever reacts to the notch (the
/// Companion module, DF-0024).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Activity {
    /// Music or a video playing.
    Music,
    /// Something running (a stopwatch).
    Running,
}

/// A slider in an item's row of the open notch (e.g. the volume). Moving it
/// calls `Module::item_action` with `set:<value>`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Slider {
    pub value: f64,
    pub min: f64,
    pub max: f64,
    pub step: f64,
    /// Shows the value only, it cannot be moved (e.g. the progress of a
    /// track the app does not let seek).
    pub readonly: bool,
}

/// A button at the end of an item's row in the open notch (e.g. start a
/// timer). Clicking it calls `Module::item_action`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Action {
    /// Passed back to `Module::item_action`.
    pub id: String,
    /// Tooltip of an icon button, text of a button without icon ("+1 min").
    pub label: String,
    pub icon: Option<Icon>,
}

impl Action {
    pub fn icon(id: &str, icon: Icon, label: &str) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            icon: Some(icon),
        }
    }

    pub fn text(id: &str, label: &str) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            icon: None,
        }
    }
}

/// One line of the notch: a Claude Code session, a song, the weather…
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    /// Unique within its module; the core prefixes it with the module id.
    pub id: String,
    /// Main text, e.g. the project name.
    pub title: String,
    /// Short state, e.g. "Au travail".
    pub label: String,
    /// Extra detail, e.g. the running tool.
    pub detail: Option<String>,
    pub tone: Tone,
    /// Shows a dot in the tone's colour (e.g. a session's state). Items that
    /// only display information, like the time, go without.
    pub dot: bool,
    /// Draws this companion instead of the dot (DF-0020): a little animated
    /// creature, in the tone's colour, whose animation follows the tone.
    pub companion: Option<crate::companion::Choice>,
    /// Buttons shown in the open notch, in order.
    pub actions: Vec<Action>,
    /// A slider shown in the open notch, between the title and the label.
    pub slider: Option<Slider>,
    /// Nothing worth showing yet (e.g. a stopwatch at zero): the compact
    /// views (closed notch, pin) show the module's icon instead of the label,
    /// or the item's `image` if it has one. A spotlight (`Host::spotlight`)
    /// shows the label for a moment.
    pub quiet: bool,
    /// In a pin, instead of `quiet` (Media hides its text in the closed
    /// notch and in a pin separately). `None`: as `quiet`.
    pub pin_quiet: Option<bool>,
    /// Replaces the module's icon in the compact views (e.g. the volume's
    /// level).
    pub icon: Option<Icon>,
    /// Makes the icon shown for the item a button, wherever it is (the
    /// open notch's module icon, the closed notch, a pin): clicking it calls
    /// `Module::item_action` with its id, its label as tooltip (e.g. mute).
    pub icon_action: Option<Action>,
    /// Rings (e.g. a timer whose time is up): an alarm repeats and the shape
    /// showing it pulses until the user acknowledges it.
    pub ringing: bool,
    /// What the compact views (closed notch, pin) write instead of the label
    /// and title, e.g. only the time when the open notch shows the date too.
    pub compact: Option<Compact>,
    /// A small square picture before the texts, everywhere the item shows
    /// (e.g. an album cover): a `data:` URL, never fetched from the network.
    /// An empty string keeps the place without a picture (a track without
    /// cover): the module's icon stands in, as for a picture that fails.
    pub image: Option<String>,
    /// Shown faded, e.g. a paused track.
    pub dimmed: bool,
    /// What it is busy with, if anything (DF-0024): music playing, running.
    pub activity: Option<Activity>,
    /// Its compact text is worth seeing all along (a stopwatch or timer in
    /// use): on the left / right edges, the thin compact views show it in a
    /// small horizontal bubble, as for a spotlight (DF-0027).
    pub bubble: bool,
    /// Short lines shown one above the other in the thin compact views of
    /// the left / right edges, instead of the icon: the clock's hours over
    /// its minutes (DF-0027).
    pub stack: Vec<String>,
    /// The compact views show its slider as a level: its icon, a bar and the
    /// number (the volume, DF-0027), or its compact text instead (muted).
    pub meter: bool,
    /// The closed notch draws its slider as a thin line along its outline
    /// (a track's progress, DF-0033).
    pub outline: bool,
    /// In the compact views, a text too long for its place scrolls instead
    /// of being cut (DF-0017).
    pub scroll: bool,
    /// How the open notch draws the item.
    pub layout: Layout,
}

/// How the open notch draws an item.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Layout {
    /// One line: title, slider, label · detail, buttons.
    #[default]
    Row,
    /// A large picture on the left; on its right, stacked: the title, the
    /// label, the slider with the detail, the buttons (a media player).
    Featured,
}

/// An item's texts in the compact views (`Item::compact`).
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Compact {
    pub label: String,
    /// Shown fainter after the label; may be empty.
    pub title: String,
}

/// Choices a module adds to the right-click menu of one of its items, as a
/// submenu (e.g. a session's companion, DF-0020).
#[derive(Debug, Clone, PartialEq)]
pub struct ItemMenu {
    /// The submenu's name.
    pub title: String,
    pub choices: Vec<MenuChoice>,
}

/// One choice of an `ItemMenu`: picking it calls `Module::item_action` with
/// its id.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MenuChoice {
    pub id: String,
    pub label: String,
    /// The current choice, shown ticked.
    pub checked: bool,
    /// Drawn after a separator line, apart from the choices above.
    pub separated: bool,
}

/// A feature shown in the notch. Modules are compiled in and handed to
/// `crate::run`; only enabled modules are started.
pub trait Module: Send + Sync + 'static {
    /// Stable identifier, also the key of the module's settings in config.json.
    fn id(&self) -> &'static str;
    /// Name shown to the user.
    /// In the current language (`t!`), like every text a module shows.
    fn name(&self) -> String;
    /// One sentence shown under the name in the settings' module list.
    fn description(&self) -> String {
        String::new()
    }
    /// The module's own translations (ADR-0012): (language, flat JSON) pairs,
    /// keys prefixed with the module id. Added to Minim Notch's at launch.
    fn locales(&self) -> &'static [(&'static str, &'static str)] {
        &[]
    }
    /// Whether the module runs until the user turns it off or on.
    fn enabled_by_default(&self) -> bool {
        true
    }
    /// Called at launch when enabled, and when the user enables the module.
    fn start(&self, host: &Host);
    /// Called when the user disables the module (not when Minim Notch quits).
    fn stop(&self);
    /// What the notch shows for this module, in display order.
    fn items(&self) -> Vec<Item>;
    /// Shown in the open notch when the module has no item, e.g. its
    /// connection state. `None` = nothing to say.
    fn placeholder(&self) -> Option<String> {
        None
    }
    /// The user clicked the notch: they have seen what it shows.
    fn acknowledge(&self) {}
    /// The user clicked one item in the open notch: they have seen it. By
    /// default, the whole module is acknowledged.
    fn acknowledge_item(&self, _item: &str) {
        self.acknowledge()
    }
    /// The user clicked one of an item's buttons (`Item::actions`).
    fn item_action(&self, _item: &str, _action: &str) {}
    /// Whether its pin opens to the whole module when hovered; `false` keeps
    /// the pin's small shape (the Companion module, DF-0024).
    fn pin_opens(&self) -> bool {
        true
    }
    /// Shows nothing, and the closed notch stops at it (the Blank notch
    /// module, DF-0032): modules after it in the user's order only show in
    /// the closed notch for an alert. Never drawn in the open notch, never
    /// pinned.
    fn blank(&self) -> bool {
        false
    }
    /// Submenus of choices for the right-click menu of an item, above the
    /// module's own entries (pin, unpin); none by default.
    fn item_menu(&self, _item: &str) -> Vec<ItemMenu> {
        Vec::new()
    }
    /// Data for the module's section of the settings window.
    fn settings(&self) -> Value {
        Value::Null
    }
    /// Action requested by the module's section of the settings window.
    fn call(&self, action: &str, _args: Value) -> Result<Value, String> {
        Err(format!("unknown action {action}"))
    }
}

/// What a module may ask of the core. Cheap to clone.
///
/// Never call `refresh` or `settings_changed` while holding one of the
/// module's own locks: they call back into `Module::items` / `settings`.
#[derive(Clone)]
pub struct Host {
    app: AppHandle,
    id: &'static str,
}

impl Host {
    pub(crate) fn new(app: AppHandle, id: &'static str) -> Self {
        Self { app, id }
    }

    /// The module's items changed: redraws the notch.
    pub fn refresh(&self) {
        crate::emit_content(&self.app);
    }

    /// The module's settings data changed: updates the settings window.
    pub fn settings_changed(&self) {
        crate::settings::changed(&self.app);
    }

    /// Something changed that the user did elsewhere (the volume keys, the
    /// next track): the closed notch shows this module for `duration`
    /// (DF-0013, DF-0017).
    pub fn spotlight(&self, duration: std::time::Duration) {
        let _ = self.app.emit(
            "spotlight",
            serde_json::json!({ "module": self.id, "ms": duration.as_millis() as u64 }),
        );
    }

    /// Like `spotlight`, for a key taken over from Windows (ADR-0020): the
    /// notch replaces Windows' flyout, so it also shows while hidden for a
    /// fullscreen app, for that time.
    pub fn reveal(&self, duration: std::time::Duration) {
        self.spotlight(duration);
        crate::notch::reveal(&self.app, self.id, duration);
    }

    /// Whether the notch is hidden for a fullscreen app (DF-0002).
    pub fn hidden_by_fullscreen(&self) -> bool {
        crate::notch::hidden_by_fullscreen()
    }

    /// Short message shown in the notch for a few seconds.
    pub fn notice(&self, message: &str) {
        let _ = self.app.emit("notice", message);
    }

    /// Minim Notch's config dir, for files of the module's own.
    pub fn config_dir(&self) -> PathBuf {
        self.app.state::<AppState>().config_dir.clone()
    }

    pub fn home_dir(&self) -> Option<PathBuf> {
        self.app.path().home_dir().ok()
    }

    /// The module's settings, defaults for missing or invalid fields.
    pub fn load_settings<T: DeserializeOwned + Default>(&self) -> T {
        let state = self.app.state::<AppState>();
        let entry = state.config.lock().unwrap().modules.get(self.id).cloned();
        entry
            .and_then(|e| serde_json::from_value(Value::Object(e.settings)).ok())
            .unwrap_or_default()
    }

    pub fn save_settings<T: Serialize>(&self, settings: &T) {
        let Ok(Value::Object(map)) = serde_json::to_value(settings) else {
            log::error!("settings of module {} are not an object", self.id);
            return;
        };
        crate::settings::update_config(&self.app, |c| {
            c.modules.entry(self.id.to_owned()).or_default().settings = map;
        });
    }
}

/// What one enabled module shows: its own part of the notch (DF-0011).
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Section {
    /// Id of the module: the front picks its icon from it.
    pub module: String,
    pub items: Vec<Item>,
    /// The module's placeholder, when it has no item.
    pub note: Option<String>,
    /// Pinned: shown in its own mini-notch on this side (DF-0012).
    pub pin: Option<Side>,
    /// Its pin opens when hovered (`Module::pin_opens`).
    pub pin_opens: bool,
    /// Where the closed notch stops (`Module::blank`): no items, not drawn.
    pub blank: bool,
}

/// Where a pinned module's mini-notch is, next to the notch. On the left and
/// right edges of the screen: above and below.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Side {
    Left,
    Right,
}

/// Everything the notch draws: one section per enabled module with
/// something to show, in the user's order (DF-0011).
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Content {
    pub sections: Vec<Section>,
    /// Shown in the open notch when no module is enabled.
    pub note: Option<String>,
}

/// A duration as a module would show it: "04:05", "1:02:03" past an hour.
pub fn format_duration(seconds: u64) -> String {
    let (h, m, s) = (seconds / 3600, seconds / 60 % 60, seconds % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m:02}:{s:02}")
    }
}

pub(crate) fn is_enabled(module: &dyn Module, config: &config::Config) -> bool {
    config.module_enabled(module.id(), module.enabled_by_default())
}

/// The modules in the user's order: those listed in `order` first, the
/// others (e.g. newly installed ones) after, in their declaration order.
/// The first enabled module in the user's order: the notch itself, which
/// cannot be pinned (DF-0012). A blank module (DF-0032) shows nothing: the
/// next one is.
pub(crate) fn first_enabled<'a>(
    modules: &'a [Box<dyn Module>],
    config: &config::Config,
) -> Option<&'a dyn Module> {
    ordered(modules, &config.module_order)
        .into_iter()
        .find(|m| is_enabled(*m, config) && !m.blank())
}

pub(crate) fn ordered<'a>(modules: &'a [Box<dyn Module>], order: &[String]) -> Vec<&'a dyn Module> {
    let mut sorted: Vec<&dyn Module> = modules.iter().map(AsRef::as_ref).collect();
    // Stable sort: unlisted modules keep their relative order, at the end.
    sorted.sort_by_key(|m| {
        order
            .iter()
            .position(|id| id == m.id())
            .unwrap_or(usize::MAX)
    });
    sorted
}

/// Side of each module, `ids` being the enabled modules in display order
/// (DF-0012). The first one is the notch itself: never pinned. A forced side
/// is kept; each `Auto` pin goes, in order, to the side with fewer pins (the
/// right one when even).
pub(crate) fn pin_sides(ids: &[&str], pins: &BTreeMap<String, PinSide>) -> Vec<Option<Side>> {
    let wanted: Vec<Option<PinSide>> = ids
        .iter()
        .enumerate()
        .map(|(i, id)| if i == 0 { None } else { pins.get(*id).copied() })
        .collect();
    let forced = |side| wanted.iter().filter(|w| **w == Some(side)).count();
    let (mut left, mut right) = (forced(PinSide::Left), forced(PinSide::Right));
    wanted
        .into_iter()
        .map(|w| match w? {
            PinSide::Left => Some(Side::Left),
            PinSide::Right => Some(Side::Right),
            PinSide::Auto if left < right => {
                left += 1;
                Some(Side::Left)
            }
            PinSide::Auto => {
                right += 1;
                Some(Side::Right)
            }
        })
        .collect()
}

pub(crate) fn content(modules: &[Box<dyn Module>], config: &config::Config) -> Content {
    let mut content = Content::default();
    let enabled: Vec<&dyn Module> = ordered(modules, &config.module_order)
        .into_iter()
        .filter(|m| is_enabled(*m, config))
        .collect();
    // A blank module is never pinned, nor the notch itself (DF-0032).
    let ids: Vec<&str> = enabled
        .iter()
        .filter(|m| !m.blank())
        .map(|m| m.id())
        .collect();
    let mut sides = pin_sides(&ids, &config.pins).into_iter();
    let any_enabled = !enabled.is_empty();
    for module in enabled {
        if module.blank() {
            content.sections.push(Section {
                module: module.id().into(),
                blank: true,
                ..Section::default()
            });
            continue;
        }
        let pin = sides.next().flatten();
        let items: Vec<Item> = module
            .items()
            .into_iter()
            .map(|item| Item {
                id: format!("{}:{}", module.id(), item.id),
                ..item
            })
            .collect();
        let note = if items.is_empty() {
            module.placeholder()
        } else {
            None
        };
        if !items.is_empty() || note.is_some() {
            content.sections.push(Section {
                module: module.id().into(),
                items,
                note,
                pin,
                pin_opens: module.pin_opens(),
                blank: false,
            });
        }
    }
    if !any_enabled {
        content.note = Some(crate::t!("notch.no_module"));
    }
    content
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fake {
        id: &'static str,
        items: Vec<Item>,
    }

    impl Module for Fake {
        fn id(&self) -> &'static str {
            self.id
        }
        fn name(&self) -> String {
            "Fake".into()
        }
        fn start(&self, _: &Host) {}
        fn stop(&self) {}
        fn items(&self) -> Vec<Item> {
            self.items.clone()
        }
        fn placeholder(&self) -> Option<String> {
            Some(format!("{} : rien", self.id))
        }
    }

    fn item(id: &str) -> Item {
        Item {
            id: id.into(),
            title: "t".into(),
            label: "l".into(),
            detail: None,
            dot: true,
            ..Item::default()
        }
    }

    fn modules() -> Vec<Box<dyn Module>> {
        vec![
            Box::new(Fake {
                id: "a",
                items: vec![item("1")],
            }),
            Box::new(Fake {
                id: "b",
                items: vec![],
            }),
        ]
    }

    #[test]
    fn durations_get_hours_only_when_needed() {
        assert_eq!(format_duration(0), "00:00");
        assert_eq!(format_duration(245), "04:05");
        assert_eq!(format_duration(3723), "1:02:03");
    }

    #[test]
    fn items_are_prefixed_and_empty_modules_give_their_placeholder() {
        let content = content(&modules(), &config::Config::default());
        assert_eq!(content.sections.len(), 2);
        assert_eq!(content.sections[0].module, "a");
        assert_eq!(content.sections[0].items[0].id, "a:1");
        assert_eq!(content.sections[0].note, None);
        assert!(content.sections[1].items.is_empty());
        assert_eq!(content.sections[1].note.as_deref(), Some("b : rien"));
        assert_eq!(content.note, None);
    }

    #[test]
    fn sections_follow_the_user_order_unlisted_modules_last() {
        let mut modules = modules();
        modules.push(Box::new(Fake {
            id: "c",
            items: vec![item("1")],
        }));
        let config = config::Config {
            module_order: vec!["c".into(), "a".into()],
            ..config::Config::default()
        };
        let ids: Vec<_> = content(&modules, &config)
            .sections
            .into_iter()
            .map(|s| s.module)
            .collect();
        assert_eq!(ids, ["c", "a", "b"]);
    }

    #[test]
    fn disabled_modules_show_nothing() {
        let mut config = config::Config::default();
        config.modules.entry("a".into()).or_default().enabled = Some(false);
        let content = content(&modules(), &config);
        assert_eq!(content.sections.len(), 1);
        assert_eq!(content.sections[0].module, "b");
        assert_eq!(content.note, None);

        config.modules.entry("b".into()).or_default().enabled = Some(false);
        let content = super::content(&modules(), &config);
        assert!(content.sections.is_empty());
        assert_eq!(content.note.as_deref(), Some("No module enabled"));
    }

    #[test]
    fn a_blank_module_keeps_its_place_and_is_never_pinned() {
        let mut modules = modules();
        modules.insert(0, Box::new(crate::modules::blank::Blank));
        let mut config = config::Config::default();
        config.modules.entry("blank".into()).or_default().enabled = Some(true);
        for id in ["blank", "a", "b"] {
            config.pins.insert(id.into(), PinSide::Auto);
        }
        let content = content(&modules, &config);
        let shown: Vec<_> = content
            .sections
            .iter()
            .map(|s| (s.module.as_str(), s.blank, s.pin))
            .collect();
        // The blank module first: "a" is the notch itself, not pinned.
        assert_eq!(
            shown,
            [
                ("blank", true, None),
                ("a", false, None),
                ("b", false, Some(Side::Right))
            ]
        );
        assert!(content.sections[0].items.is_empty());
        assert_eq!(content.sections[0].note, None);
        assert_eq!(first_enabled(&modules, &config).map(|m| m.id()), Some("a"));
    }

    #[test]
    fn auto_pins_balance_left_and_right_the_first_module_never_pinned() {
        let pins: BTreeMap<String, PinSide> = ["a", "b", "c", "d"]
            .into_iter()
            .map(|id| (id.to_string(), PinSide::Auto))
            .collect();
        assert_eq!(
            pin_sides(&["a", "b", "c", "d"], &pins),
            vec![None, Some(Side::Right), Some(Side::Left), Some(Side::Right)]
        );
        // Unpinned modules stay in the notch.
        assert_eq!(
            pin_sides(&["a", "x", "b"], &pins),
            vec![None, None, Some(Side::Right)]
        );
    }

    #[test]
    fn forced_sides_are_kept_and_counted_by_auto_pins() {
        let pins: BTreeMap<String, PinSide> = [
            ("b", PinSide::Auto),
            ("c", PinSide::Right),
            ("d", PinSide::Auto),
        ]
        .into_iter()
        .map(|(id, side)| (id.to_string(), side))
        .collect();
        // The forced right pin counts first: both auto pins go left, then right.
        assert_eq!(
            pin_sides(&["a", "b", "c", "d"], &pins),
            vec![None, Some(Side::Left), Some(Side::Right), Some(Side::Right)]
        );
    }
}
