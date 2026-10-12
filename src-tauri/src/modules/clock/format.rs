//! How the time and the date are written, from the module's settings.
//! Pure logic, tested.

use chrono::{Datelike, Locale, NaiveDateTime, Timelike};
use serde::{Deserialize, Serialize};

/// What the notch shows.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Show {
    #[default]
    TimeAndDate,
    Time,
    Date,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DateStyle {
    /// "lundi 5 octobre", "Monday, October 5"
    #[default]
    Long,
    /// "lun. 5 oct.", "Mon, Oct 5"
    Short,
    /// "05/10/2026", "10/05/2026"
    Numeric,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    /// In the open notch.
    pub show: Show,
    /// In the closed notch and the pin (DF-0016).
    pub compact: Show,
    /// "2:32 PM" instead of "14:32".
    pub hour12: bool,
    pub seconds: bool,
    pub date_style: DateStyle,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            show: Show::TimeAndDate,
            // The compact views stay short: the time only.
            compact: Show::Time,
            hour12: false,
            seconds: false,
            date_style: DateStyle::default(),
        }
    }
}

/// How dates are written in a language: the patterns come from the
/// translations (`clock.date.*`, chrono's `%` codes with `{day}`), the day
/// and month names from chrono's locales.
pub struct Words {
    long: String,
    short: String,
    numeric: String,
    /// The 1st of the month, e.g. "1er" in French.
    first_day: String,
    locale: Locale,
}

impl Words {
    pub fn of(language: &str) -> Self {
        let t = |key| crate::i18n::translate_in(language, key, &[]);
        Self {
            long: t("clock.date.long"),
            short: t("clock.date.short"),
            numeric: t("clock.date.numeric"),
            first_day: t("clock.date.first_day"),
            locale: match language {
                "de" => Locale::de_DE,
                "es" => Locale::es_ES,
                "fr" => Locale::fr_FR,
                "ja" => Locale::ja_JP,
                _ => Locale::en_US,
            },
        }
    }
}

/// The item's label (main text) and title (secondary text, may be empty).
pub fn format(now: NaiveDateTime, settings: &Settings, words: &Words) -> (String, String) {
    format_as(now, settings.show, settings, words)
}

/// Like `format`, showing `show` instead of the open notch's choice.
pub fn format_as(
    now: NaiveDateTime,
    show: Show,
    settings: &Settings,
    words: &Words,
) -> (String, String) {
    let time = time(now, settings);
    let date = date(now, settings.date_style, words);
    match show {
        Show::TimeAndDate => (time, date),
        Show::Time => (time, String::new()),
        Show::Date => (date, String::new()),
    }
}

/// The time as short lines, hours over minutes (then the seconds and AM /
/// PM, if shown): the thin compact views of the left / right edges.
pub fn stacked(now: NaiveDateTime, settings: &Settings) -> Vec<String> {
    let mut lines = if settings.hour12 {
        vec![now.hour12().1.to_string()]
    } else {
        vec![format!("{:02}", now.hour())]
    };
    lines.push(format!("{:02}", now.minute()));
    if settings.seconds {
        lines.push(format!("{:02}", now.second()));
    }
    if settings.hour12 {
        lines.push(if now.hour12().0 { "PM" } else { "AM" }.into());
    }
    lines
}

fn time(now: NaiveDateTime, settings: &Settings) -> String {
    let seconds = if settings.seconds {
        format!(":{:02}", now.second())
    } else {
        String::new()
    };
    if settings.hour12 {
        let (pm, hour) = now.hour12();
        let suffix = if pm { "PM" } else { "AM" };
        format!("{hour}:{:02}{seconds} {suffix}", now.minute())
    } else {
        format!("{:02}:{:02}{seconds}", now.hour(), now.minute())
    }
}

fn date(now: NaiveDateTime, style: DateStyle, words: &Words) -> String {
    let pattern = match style {
        DateStyle::Long => &words.long,
        DateStyle::Short => &words.short,
        DateStyle::Numeric => &words.numeric,
    };
    let day = match now.day() {
        1 => words.first_day.clone(),
        d => d.to_string(),
    };
    now.date()
        .format_localized(&pattern.replace("{day}", &day), words.locale)
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn at(y: i32, m: u32, d: u32, h: u32, min: u32, s: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(y, m, d)
            .unwrap()
            .and_hms_opt(h, min, s)
            .unwrap()
    }

    fn format_fr(now: NaiveDateTime, settings: &Settings) -> (String, String) {
        format(now, settings, &Words::of("fr"))
    }

    fn pair(a: &str, b: &str) -> (String, String) {
        (a.into(), b.into())
    }

    #[test]
    fn default_is_24h_time_and_long_date() {
        let settings = Settings::default();
        assert_eq!(
            format_fr(at(2026, 10, 5, 14, 32, 7), &settings),
            pair("14:32", "lundi 5 octobre")
        );
        assert_eq!(
            format_fr(at(2026, 2, 1, 9, 5, 0), &settings),
            pair("09:05", "dimanche 1er février")
        );
    }

    #[test]
    fn time_only_or_date_only() {
        let now = at(2026, 10, 5, 14, 32, 7);
        let time = Settings {
            show: Show::Time,
            ..Settings::default()
        };
        assert_eq!(format_fr(now, &time), pair("14:32", ""));
        let date = Settings {
            show: Show::Date,
            ..Settings::default()
        };
        assert_eq!(format_fr(now, &date), pair("lundi 5 octobre", ""));
    }

    #[test]
    fn the_time_stacked() {
        let now = at(2026, 10, 5, 14, 2, 7);
        assert_eq!(stacked(now, &Settings::default()), ["14", "02"]);
        let all = Settings {
            hour12: true,
            seconds: true,
            ..Settings::default()
        };
        assert_eq!(stacked(now, &all), ["2", "02", "07", "PM"]);
    }

    #[test]
    fn twelve_hours_and_seconds() {
        let settings = Settings {
            show: Show::Time,
            hour12: true,
            seconds: true,
            ..Settings::default()
        };
        assert_eq!(
            format_fr(at(2026, 10, 5, 14, 32, 7), &settings).0,
            "2:32:07 PM"
        );
        assert_eq!(
            format_fr(at(2026, 10, 5, 0, 5, 0), &settings).0,
            "12:05:00 AM"
        );
    }

    #[test]
    fn short_and_numeric_dates() {
        let now = at(2026, 10, 5, 14, 32, 0);
        let short = Settings {
            show: Show::Date,
            date_style: DateStyle::Short,
            ..Settings::default()
        };
        assert_eq!(format_fr(now, &short).0, "lun. 5 oct.");
        let numeric = Settings {
            date_style: DateStyle::Numeric,
            ..short
        };
        assert_eq!(format_fr(now, &numeric).0, "05/10/2026");
    }

    #[test]
    fn english_dates() {
        let now = at(2026, 10, 5, 14, 32, 0);
        let english = |date_style| {
            let settings = Settings {
                show: Show::Date,
                date_style,
                ..Settings::default()
            };
            format(now, &settings, &Words::of("en")).0
        };
        assert_eq!(english(DateStyle::Long), "Monday, October 5");
        assert_eq!(english(DateStyle::Short), "Mon, Oct 5");
        assert_eq!(english(DateStyle::Numeric), "10/05/2026");
    }

    /// The examples shown in the settings are what the notch really writes.
    #[test]
    fn every_language_writes_its_examples() {
        let now = at(2026, 10, 5, 14, 32, 0);
        for (language, _) in crate::i18n::LANGUAGES {
            let words = Words::of(language);
            for (style, example) in [
                (DateStyle::Long, "clock.example.long"),
                (DateStyle::Short, "clock.example.short"),
                (DateStyle::Numeric, "clock.example.numeric"),
            ] {
                let settings = Settings {
                    show: Show::Date,
                    date_style: style,
                    ..Settings::default()
                };
                assert_eq!(
                    format(now, &settings, &words).0,
                    crate::i18n::translate_in(language, example, &[]),
                    "{language} {style:?}"
                );
            }
        }
    }

    #[test]
    fn the_compact_views_have_their_own_choice() {
        let now = at(2026, 10, 5, 14, 32, 0);
        let settings = Settings::default();
        let fr = Words::of("fr");
        assert_eq!(
            format_as(now, settings.compact, &settings, &fr),
            pair("14:32", "")
        );
        assert_eq!(
            format_as(now, Show::Date, &settings, &fr),
            pair("lundi 5 octobre", "")
        );
    }
}
