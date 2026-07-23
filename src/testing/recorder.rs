//! Record/replay for `TestHarness`: capture a sequence of interactions
//! as structured events, then replay them on a fresh harness built from
//! the same mount closure.
//!
//! ```ignore
//! let checked = Signal::new(false);
//!
//! // Record a sequence of interactions.
//! let events = {
//!     let mut rec = TestRecorder::new(800.0, 600.0);
//!     let id = rec.harness.mount(Checkbox::new(checked.clone()));
//!     rec.harness.find_mut(id).unwrap().set_test_id("cb");
//!     rec.run_frame();
//!     rec.click_on("cb");
//!     rec.into_events()
//! };
//!
//! // Replay on a fresh harness with an equivalent widget tree.
//! let replayed = replay_events(
//!     |h| {
//!         let id = h.mount(Checkbox::new(checked.clone()));
//!         h.find_mut(id).unwrap().set_test_id("cb");
//!     },
//!     &events,
//! );
//! ```

use crate::event::Modifiers;
use crate::style::Point;
use crate::testing::TestHarness;

/// A single user interaction or harness operation.
#[derive(Clone, Debug)]
pub enum Interaction {
    RunFrame,
    RunFrames {
        n: usize,
    },
    AdvanceTime {
        millis: u64,
    },
    AdvanceToNextDeadline,
    ClickAt {
        x: f32,
        y: f32,
    },
    ClickOnTestId {
        test_id: String,
    },
    HoverAt {
        x: f32,
        y: f32,
    },
    TypeInto {
        test_id: String,
        text: String,
    },
    PressKey {
        key_name: String,
    },
    ReleaseKey {
        key_name: String,
    },
    ScrollOnTestId {
        test_id: String,
        dx: f32,
        dy: f32,
    },
    Drag {
        from_x: f32,
        from_y: f32,
        to_x: f32,
        to_y: f32,
    },
    Settle {
        max_frames: usize,
    },
    Resize {
        width: f32,
        height: f32,
    },
}

/// Wraps `TestHarness` and records every interaction into a log.
pub struct TestRecorder {
    pub harness: TestHarness,
    events: Vec<Interaction>,
}

impl TestRecorder {
    pub fn new(width: f32, height: f32) -> Self {
        Self {
            harness: TestHarness::new(width, height),
            events: Vec::new(),
        }
    }

    pub fn into_events(self) -> Vec<Interaction> {
        self.events
    }

    pub fn run_frame(&mut self) -> &mut Self {
        self.events.push(Interaction::RunFrame);
        self.harness.run_frame();
        self
    }

    pub fn advance_time(&mut self, millis: u64) -> &mut Self {
        self.events.push(Interaction::AdvanceTime { millis });
        self.harness.advance_time(millis);
        self
    }

    pub fn advance_to_next_deadline(&mut self) -> &mut Self {
        self.events.push(Interaction::AdvanceToNextDeadline);
        self.harness.advance_to_next_deadline();
        self
    }

    pub fn click_on(&mut self, test_id: &str) -> &mut Self {
        self.events.push(Interaction::ClickOnTestId {
            test_id: test_id.to_string(),
        });
        let id = self
            .harness
            .find_sel(crate::testing::selector::by_test_id(test_id));
        if let Some(id) = id {
            self.harness.click(id);
        }
        self
    }

    pub fn type_into(&mut self, test_id: &str, text: &str) -> &mut Self {
        self.events.push(Interaction::TypeInto {
            test_id: test_id.to_string(),
            text: text.to_string(),
        });
        let id = self
            .harness
            .find_sel(crate::testing::selector::by_test_id(test_id));
        if let Some(id) = id {
            self.harness.type_text(id, text);
        }
        self
    }

    pub fn settle(&mut self, max_frames: usize) -> &mut Self {
        self.events.push(Interaction::Settle { max_frames });
        self.harness.settle(max_frames);
        self
    }

    pub fn resize(&mut self, width: f32, height: f32) -> &mut Self {
        self.events.push(Interaction::Resize { width, height });
        self.harness.resize(width, height);
        self
    }

    pub fn run_frames(&mut self, n: usize) -> &mut Self {
        self.events.push(Interaction::RunFrames { n });
        self.harness.run_frames(n);
        self
    }

    pub fn hover_at(&mut self, x: f32, y: f32) -> &mut Self {
        self.events.push(Interaction::HoverAt { x, y });
        self.harness.hover_at(Point::new(x, y));
        self
    }

    pub fn click_at(&mut self, x: f32, y: f32) -> &mut Self {
        self.events.push(Interaction::ClickAt { x, y });
        self.harness.click_at(Point::new(x, y));
        self
    }

    pub fn drag(&mut self, from_x: f32, from_y: f32, to_x: f32, to_y: f32) -> &mut Self {
        self.events.push(Interaction::Drag {
            from_x,
            from_y,
            to_x,
            to_y,
        });
        self.harness
            .drag(Point::new(from_x, from_y), Point::new(to_x, to_y));
        self
    }

    pub fn press_key(&mut self, key_name: &str) -> &mut Self {
        self.events.push(Interaction::PressKey {
            key_name: key_name.to_string(),
        });
        self.harness
            .press_key(parse_key(key_name), Modifiers::default());
        self
    }

    pub fn release_key(&mut self, key_name: &str) -> &mut Self {
        self.events.push(Interaction::ReleaseKey {
            key_name: key_name.to_string(),
        });
        self.harness
            .release_key(parse_key(key_name), Modifiers::default());
        self
    }

    pub fn scroll(&mut self, test_id: &str, dx: f32, dy: f32) -> &mut Self {
        self.events.push(Interaction::ScrollOnTestId {
            test_id: test_id.to_string(),
            dx,
            dy,
        });
        let id = self
            .harness
            .find_sel(crate::testing::selector::by_test_id(test_id));
        if let Some(id) = id {
            self.harness.scroll(id, dx, dy);
        }
        self
    }
}

/// Replay a sequence of recorded events on a fresh harness built from
/// `mount`. The `mount` closure receives the new harness and must construct
/// an equivalent widget tree (recorded events reference elements by test_id,
/// which only exist if `mount` recreates them).
/// Panics on any interaction that requires a `test_id` which doesn't exist.
pub fn replay_events(mount: impl FnOnce(&mut TestHarness), events: &[Interaction]) -> TestHarness {
    let mut h = TestHarness::new(800.0, 600.0);
    mount(&mut h);
    h.run_frame(); // initial frame for layout

    for event in events {
        match event {
            Interaction::RunFrame => {
                h.run_frame();
            }
            Interaction::RunFrames { n } => {
                h.run_frames(*n);
            }
            Interaction::AdvanceTime { millis } => {
                h.advance_time(*millis);
            }
            Interaction::AdvanceToNextDeadline => {
                h.advance_to_next_deadline();
            }
            Interaction::ClickAt { x, y } => {
                h.click_at(crate::style::Point::new(*x, *y));
            }
            Interaction::ClickOnTestId { test_id } => {
                let id = h
                    .find_sel(crate::testing::selector::by_test_id(test_id))
                    .unwrap_or_else(|| panic!("replay: test_id '{}' not found", test_id));
                h.click(id);
            }
            Interaction::HoverAt { x, y } => {
                h.hover_at(crate::style::Point::new(*x, *y));
            }
            Interaction::TypeInto { test_id, text } => {
                let id = h
                    .find_sel(crate::testing::selector::by_test_id(test_id))
                    .unwrap_or_else(|| panic!("replay: test_id '{}' not found", test_id));
                h.type_text(id, text);
            }
            Interaction::PressKey { key_name } => {
                let key = parse_key(key_name);
                h.press_key(key, crate::event::Modifiers::default());
            }
            Interaction::ReleaseKey { key_name } => {
                let key = parse_key(key_name);
                h.release_key(key, crate::event::Modifiers::default());
            }
            Interaction::ScrollOnTestId { test_id, dx, dy } => {
                let id = h
                    .find_sel(crate::testing::selector::by_test_id(test_id))
                    .unwrap_or_else(|| panic!("replay: test_id '{}' not found", test_id));
                h.scroll(id, *dx, *dy);
            }
            Interaction::Drag {
                from_x,
                from_y,
                to_x,
                to_y,
            } => {
                h.drag(
                    crate::style::Point::new(*from_x, *from_y),
                    crate::style::Point::new(*to_x, *to_y),
                );
            }
            Interaction::Settle { max_frames } => {
                h.settle(*max_frames);
            }
            Interaction::Resize { width, height } => {
                h.resize(*width, *height);
            }
        }
    }
    h
}

fn parse_key(name: &str) -> crate::event::Key {
    use crate::event::Key;
    match name {
        "Enter" => Key::Enter,
        "Escape" => Key::Escape,
        "Backspace" => Key::Backspace,
        "Tab" => Key::Tab,
        "ArrowUp" => Key::ArrowUp,
        "ArrowDown" => Key::ArrowDown,
        "ArrowLeft" => Key::ArrowLeft,
        "ArrowRight" => Key::ArrowRight,
        "Home" => Key::Home,
        "End" => Key::End,
        "Delete" => Key::Delete,
        "Space" => Key::Space,
        "a" | "A" => Key::Character("a".to_string()),
        _ => Key::Character(name.chars().next().unwrap_or('?').to_string()),
    }
}
