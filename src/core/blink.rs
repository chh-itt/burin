//! Reusable blink timer driven by `clock::now()`. Deterministic in tests via virtual clock.

use crate::core::clock;
use web_time::{Duration, Instant};

/// A generic blink/pause timer. Call `.tick(now)` every frame;
/// returns whether the indicator is currently visible.
pub struct BlinkTimer {
    period_ms: u64,
    pause_duration_ms: u64,
    last_input: Instant,
    pause_until: Instant,
    visible: bool,
}

impl BlinkTimer {
    pub fn new(period_ms: u64) -> Self {
        debug_assert!(period_ms > 0, "BlinkTimer period must be > 0");
        Self {
            period_ms,
            pause_duration_ms: 300,
            last_input: clock::now(),
            pause_until: clock::now(),
            visible: false,
        }
    }

    pub fn with_pause(mut self, pause_ms: u64) -> Self {
        self.pause_duration_ms = pause_ms;
        self
    }

    /// Returns current visibility. Call every frame.
    pub fn tick(&mut self, now: Instant) -> bool {
        if now < self.pause_until {
            return self.visible;
        }
        let elapsed_ms = (now - self.last_input).as_millis() as u64;
        self.visible = (elapsed_ms / self.period_ms).is_multiple_of(2);
        self.visible
    }

    /// Call on user input (key, click, focus change).
    pub fn on_input(&mut self, now: Instant) {
        self.last_input = now;
        self.pause_until = now + Duration::from_millis(self.pause_duration_ms);
        self.visible = true;
    }

    /// Call on focus gained.
    pub fn start(&mut self, now: Instant) {
        self.visible = true;
        self.last_input = now;
        self.pause_until = now;
    }

    /// Call on focus lost.
    pub fn stop(&mut self) {
        self.visible = false;
        let now = clock::now();
        self.pause_until = now + Duration::from_millis(self.pause_duration_ms);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::clock;

    #[test]
    fn blink_alternates_each_period() {
        clock::install_virtual();
        let mut bt = BlinkTimer::new(500);
        bt.start(clock::now());
        assert!(bt.tick(clock::now()));
        clock::advance(Duration::from_millis(600));
        // 600ms / 500 = 1, 1%2=1 → OFF
        assert!(!bt.tick(clock::now()));

        clock::advance(Duration::from_millis(500)); // 1100 total
                                                    // 1100ms / 500 = 2, 2%2=0 → ON
        assert!(bt.tick(clock::now()));
        clock::reset_to_wall();
    }

    #[test]
    fn pause_after_input() {
        clock::install_virtual();
        let mut bt = BlinkTimer::new(500);
        bt.start(clock::now());
        assert!(bt.tick(clock::now()));
        bt.on_input(clock::now());
        assert!(bt.tick(clock::now()));
        clock::advance(Duration::from_millis(200));
        assert!(bt.tick(clock::now()));
        clock::advance(Duration::from_millis(200)); // 400 total from input
        assert!(bt.tick(clock::now()));
        clock::reset_to_wall();
    }

    #[test]
    fn stop_makes_invisible() {
        clock::install_virtual();
        let mut bt = BlinkTimer::new(500);
        bt.start(clock::now());
        bt.stop();
        assert!(!bt.tick(clock::now()));
        clock::reset_to_wall();
    }
}
