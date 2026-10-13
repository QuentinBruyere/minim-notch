//! Blank notch module (DF-0032): shows nothing. Placed in the module order,
//! it is where the closed notch stops: when no module above it has something
//! going on, the closed notch stays black instead of showing the modules
//! below, which are still there in the open notch.

use crate::module::{Host, Item, Module};

#[derive(Default)]
pub struct Blank;

impl Module for Blank {
    fn id(&self) -> &'static str {
        "blank"
    }

    fn name(&self) -> String {
        crate::t!("blank.name")
    }

    fn description(&self) -> String {
        crate::t!("blank.description")
    }

    fn enabled_by_default(&self) -> bool {
        false
    }

    fn start(&self, _host: &Host) {}

    fn stop(&self) {}

    fn items(&self) -> Vec<Item> {
        Vec::new()
    }

    fn blank(&self) -> bool {
        true
    }
}
