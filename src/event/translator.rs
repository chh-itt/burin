//! Translates raw winit events into burin high-level events.

use crate::event::types::{Event, Key, KeyHeldInfo, Modifiers, MouseButton};
use crate::style::Point;
use rustc_hash::FxHashMap;
use web_time::Instant;

/// Translation parameters for gesture detection.
/// Single source of truth lives in the recognizer module (audit 2026-07-19).
const DRAG_THRESHOLD: f32 = crate::event::recognizer::TAP_DRAG_THRESHOLD;

/// State tracker for synthesizing high-level events from raw input.
pub struct EventTranslator {
    pointer_pos: Point,
    pointer_down: FxHashMap<u64, (Point, MouseButton)>,
    has_dragged: FxHashMap<u64, bool>,
    modifiers: Modifiers,
    held_keys: FxHashMap<Key, HeldKeyState>,
}

struct HeldKeyState {
    since: Instant,
    repeat_count: u32,
}

impl EventTranslator {
    pub fn new() -> Self {
        Self {
            pointer_pos: Point::ZERO,
            pointer_down: FxHashMap::default(),
            has_dragged: FxHashMap::default(),
            modifiers: Modifiers::NONE,
            held_keys: FxHashMap::default(),
        }
    }

    /// Process a winit `CursorMoved` event.
    pub fn cursor_moved(&mut self, x: f32, y: f32, finger_id: Option<u64>) -> Vec<Event> {
        let pid = finger_id.unwrap_or(0);
        let pos = Point::new(x, y);
        let mut events = vec![Event::PointerMove {
            position: pos,
            finger_id,
        }];

        if let Some(&(down_pos, btn)) = self.pointer_down.get(&pid) {
            let dragged = *self.has_dragged.get(&pid).unwrap_or(&false);
            if !dragged && pos.distance(&down_pos) > DRAG_THRESHOLD {
                self.has_dragged.insert(pid, true);
                events.push(Event::DragStart {
                    position: down_pos,
                    button: btn,
                    finger_id,
                });
            }
            if *self.has_dragged.get(&pid).unwrap_or(&false) {
                let dx = pos.x - self.pointer_pos.x;
                let dy = pos.y - self.pointer_pos.y;
                events.push(Event::DragMove {
                    position: pos,
                    delta_x: dx,
                    delta_y: dy,
                    button: btn,
                    finger_id,
                });
            }
        }

        self.pointer_pos = pos;
        events
    }

    /// Process a winit `MouseInput` event.
    pub fn mouse_input(
        &mut self,
        pressed: bool,
        button: MouseButton,
        finger_id: Option<u64>,
    ) -> Vec<Event> {
        let pid = finger_id.unwrap_or(0);
        let pos = self.pointer_pos;
        if pressed {
            self.pointer_down.insert(pid, (pos, button));
            self.has_dragged.insert(pid, false);
            vec![Event::PointerDown {
                position: pos,
                button,
                finger_id,
            }]
        } else {
            let mut events = vec![Event::PointerUp {
                position: pos,
                button,
                finger_id,
            }];
            if let Some((_down_pos, _btn)) = self.pointer_down.remove(&pid) {
                let dragged = self.has_dragged.remove(&pid).unwrap_or(false);
                if !dragged {
                    events.push(Event::Click {
                        position: pos,
                        button,
                        finger_id,
                        modifiers: self.modifiers,
                    });
                } else {
                    events.push(Event::DragEnd {
                        position: pos,
                        button,
                        finger_id,
                    });
                }
            }
            events
        }
    }

    /// Process a winit `MouseWheel` event.
    pub fn mouse_wheel(&mut self, dx: f32, dy: f32) -> Vec<Event> {
        vec![Event::Scroll {
            delta_x: dx,
            delta_y: dy,
        }]
    }

    /// Process a winit `KeyboardInput` event.
    pub fn keyboard_input(&mut self, pressed: bool, key: Key) -> Vec<Event> {
        if pressed {
            vec![Event::KeyDown {
                key,
                modifiers: self.modifiers,
            }]
        } else {
            self.held_keys.remove(&key);
            vec![Event::KeyUp {
                key,
                modifiers: self.modifiers,
            }]
        }
    }

    /// Track a key press/repeat for held-key acceleration queries.
    /// Called before action dispatch by window_event.
    pub fn track_press(&mut self, key: &Key, repeat: bool) {
        if repeat {
            if let Some(s) = self.held_keys.get_mut(key) {
                s.repeat_count += 1;
            }
        } else {
            self.held_keys.insert(
                key.clone(),
                HeldKeyState {
                    since: Instant::now(),
                    repeat_count: 0,
                },
            );
        }
    }

    /// Query held-key info for implementing acceleration.
    /// Returns `None` if the key is not currently pressed.
    pub fn key_held_info(&self, key: &Key) -> Option<KeyHeldInfo> {
        self.held_keys.get(key).map(|s| KeyHeldInfo {
            held_duration: s.since.elapsed(),
            repeat_count: s.repeat_count,
        })
    }

    /// Update the current modifier state.
    pub fn set_modifiers(&mut self, modifiers: Modifiers) {
        self.modifiers = modifiers;
    }

    /// Get the current modifier state.
    pub fn modifiers(&self) -> Modifiers {
        self.modifiers
    }

    /// Get the current pointer position.
    pub fn pointer_position(&self) -> Point {
        self.pointer_pos
    }
}

impl Default for EventTranslator {
    fn default() -> Self {
        Self::new()
    }
}
