//! Modules shipped with the public core (ADR-0009): one folder per module.

pub mod claude_code;
pub mod clock;
pub mod companion;
pub mod media;
pub mod stopwatch;
pub mod timer;
pub mod volume;
/// Start / pause / reset elapsed time, shared by the stopwatch and the timer.
mod watch;
