// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use minim_notch_lib::modules::claude_code::ClaudeCode;
use minim_notch_lib::modules::clock::Clock;
use minim_notch_lib::modules::companion::DeskCompanion;
use minim_notch_lib::modules::media::Media;
use minim_notch_lib::modules::stopwatch::Stopwatch;
use minim_notch_lib::modules::timer::Timer;
use minim_notch_lib::modules::volume::Volume;

fn main() {
    // Display order: at equal urgency, the first module's items come first.
    minim_notch_lib::run(
        tauri::generate_context!(),
        vec![
            Box::new(ClaudeCode::default()),
            Box::new(Clock::default()),
            Box::new(Stopwatch::default()),
            Box::new(Timer::default()),
            Box::new(Volume::default()),
            Box::new(DeskCompanion::default()),
            Box::new(Media::default()),
        ],
    );
}
