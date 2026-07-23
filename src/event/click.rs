use crate::style::Point;
use web_time::Instant;

// Single source of truth for thresholds lives in the recognizer module
// (audit 2026-07-19: was split 6px/8px + 400ms/300ms across three systems).
pub const DRAG_THRESHOLD: f32 = crate::event::recognizer::TAP_DRAG_THRESHOLD;
pub const MAX_CLICK_DURATION_MS: u64 = crate::event::recognizer::TAP_TIMEOUT_MS;
pub const DOUBLE_CLICK_INTERVAL_MS: u64 = crate::event::recognizer::DOUBLE_TAP_INTERVAL_MS;

pub struct ClickCounter {
    last_click: Option<(Instant, Point)>,
    click_count: u32,
    press_start: Option<(Instant, Point)>,
}

pub enum ClickResult {
    None,
    Single { position: Point },
    Double { position: Point },
    Triple { position: Point },
}

impl ClickCounter {
    pub fn new() -> Self {
        Self {
            last_click: None,
            click_count: 1,
            press_start: None,
        }
    }

    pub fn pointer_down(&mut self, position: Point, now: Instant) {
        let is_consecutive = self.last_click.as_ref().is_some_and(|(t, p)| {
            (now - *t).as_millis() as u64 <= DOUBLE_CLICK_INTERVAL_MS
                && p.distance(&position) <= DRAG_THRESHOLD
        });

        if is_consecutive {
            self.click_count = self.click_count % 3 + 1;
        } else {
            self.click_count = 1;
        }

        self.press_start = Some((now, position));
    }

    pub fn pointer_up(&mut self, position: Point, now: Instant) -> ClickResult {
        let result = match self.press_start.take() {
            Some((start_time, start_pos)) => {
                let elapsed_ms = (now - start_time).as_millis() as u64;
                if elapsed_ms <= MAX_CLICK_DURATION_MS
                    && position.distance(&start_pos) <= DRAG_THRESHOLD
                {
                    self.last_click = Some((now, position));
                    match self.click_count {
                        1 => ClickResult::Single { position },
                        2 => ClickResult::Double { position },
                        _ => ClickResult::Triple { position },
                    }
                } else {
                    ClickResult::None
                }
            }
            None => ClickResult::None,
        };

        // Reset count on non-click or after triple-click
        if matches!(result, ClickResult::None | ClickResult::Triple { .. }) {
            self.click_count = 1;
        }

        result
    }
}

impl Default for ClickCounter {
    fn default() -> Self {
        Self::new()
    }
}
