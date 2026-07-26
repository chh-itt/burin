use crate::core::blink::BlinkTimer;
use web_time::Instant;

/// TextInput-specific blink state wrapping BlinkTimer.
#[allow(dead_code)]
pub(crate) struct CursorBlink {
    timer: BlinkTimer,
}

#[allow(dead_code)]
impl CursorBlink {
    pub fn new(period_ms: u64, pause_ms: u64) -> Self {
        Self {
            timer: BlinkTimer::new(period_ms).with_pause(pause_ms),
        }
    }

    pub fn tick(&mut self, now: Instant) -> bool {
        self.timer.tick(now)
    }

    pub fn on_input(&mut self, now: Instant) {
        self.timer.on_input(now);
    }

    pub fn start(&mut self, now: Instant) {
        self.timer.start(now);
    }

    pub fn stop(&mut self) {
        self.timer.stop();
    }
}
