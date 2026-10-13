//! A countdown built on `Watch`: idle, running, paused, then finished.
//! Pure logic (time is passed in), tested.

use std::time::{Duration, Instant};

use crate::modules::watch::Watch;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Idle,
    Running,
    Paused,
    Finished,
}

#[derive(Debug, Clone, Copy)]
pub struct Countdown {
    pub duration: Duration,
    watch: Watch,
}

impl Countdown {
    pub fn new(duration: Duration) -> Self {
        Self {
            duration,
            watch: Watch::default(),
        }
    }

    pub fn phase(&self, now: Instant) -> Phase {
        if self.watch.idle() {
            Phase::Idle
        } else if self.watch.elapsed(now) >= self.duration {
            Phase::Finished
        } else if self.watch.running() {
            Phase::Running
        } else {
            Phase::Paused
        }
    }

    /// Whole seconds left, rounded up: "05:00" during the first second.
    pub fn remaining_secs(&self, now: Instant) -> u64 {
        let left = self.duration.saturating_sub(self.watch.elapsed(now));
        left.as_secs() + u64::from(left.subsec_nanos() > 0)
    }

    pub fn start(&mut self, now: Instant) {
        if self.phase(now) != Phase::Finished {
            self.watch.start(now);
        }
    }

    pub fn pause(&mut self, now: Instant) {
        self.watch.pause(now);
    }

    pub fn add(&mut self, extra: Duration, now: Instant) {
        if self.phase(now) != Phase::Finished {
            self.duration += extra;
        }
    }

    /// Takes time off a timer not started yet, down to zero.
    pub fn subtract(&mut self, less: Duration, now: Instant) {
        if self.phase(now) == Phase::Idle {
            self.duration = self.duration.saturating_sub(less);
        }
    }

    pub fn reset(&mut self, duration: Duration) {
        *self = Self::new(duration);
    }

    /// Stops the watch once the time is up. Returns true when it just ended.
    pub fn settle(&mut self, now: Instant) -> bool {
        let ended = self.watch.running() && self.phase(now) == Phase::Finished;
        if ended {
            self.watch.pause(now);
        }
        ended
    }

    /// How long until the shown seconds change, while running.
    pub fn until_next_second(&self, now: Instant) -> Option<Duration> {
        self.watch.until_next_second(now)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const fn s(secs: u64) -> Duration {
        Duration::from_secs(secs)
    }

    #[test]
    fn runs_down_then_finishes() {
        let t0 = Instant::now();
        let mut timer = Countdown::new(s(60));
        assert_eq!(timer.phase(t0), Phase::Idle);
        assert_eq!(timer.remaining_secs(t0), 60);
        timer.start(t0);
        assert_eq!(timer.remaining_secs(t0 + Duration::from_millis(400)), 60);
        assert_eq!(timer.remaining_secs(t0 + s(1)), 59);
        timer.pause(t0 + s(20));
        assert_eq!(timer.phase(t0 + s(500)), Phase::Paused);
        assert_eq!(timer.remaining_secs(t0 + s(500)), 40);
        timer.start(t0 + s(500));
        assert!(!timer.settle(t0 + s(539)));
        assert!(timer.settle(t0 + s(540)));
        assert_eq!(timer.phase(t0 + s(999)), Phase::Finished);
        assert_eq!(timer.remaining_secs(t0 + s(999)), 0);
        assert!(!timer.settle(t0 + s(999)));
    }

    #[test]
    fn subtracting_time_only_before_the_start() {
        let t0 = Instant::now();
        let mut timer = Countdown::new(s(60));
        timer.subtract(s(90), t0);
        assert_eq!(timer.remaining_secs(t0), 0);
        timer.reset(s(300));
        timer.start(t0);
        timer.subtract(s(60), t0 + s(10));
        assert_eq!(timer.remaining_secs(t0 + s(10)), 290);
    }

    #[test]
    fn adding_time_extends_but_not_once_finished() {
        let t0 = Instant::now();
        let mut timer = Countdown::new(s(60));
        timer.start(t0);
        timer.add(s(60), t0 + s(30));
        assert_eq!(timer.remaining_secs(t0 + s(30)), 90);
        timer.settle(t0 + s(120));
        timer.add(s(60), t0 + s(130));
        assert_eq!(timer.phase(t0 + s(130)), Phase::Finished);
        timer.reset(s(300));
        assert_eq!(timer.phase(t0 + s(130)), Phase::Idle);
        assert_eq!(timer.remaining_secs(t0), 300);
    }
}
