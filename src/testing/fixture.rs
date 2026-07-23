//! Test fixtures: pre-configured harnesses for common test scenarios.

use crate::testing::TestHarness;

/// Create a baseline 800×600 harness, mount a root-level container,
/// run the first frame, and return it.
pub fn basic_harness() -> TestHarness {
    let mut h = TestHarness::new(800.0, 600.0);
    h.run_frame();
    h
}

/// Create a harness with a pre-mounted widget tree and one initial frame.
pub fn with_widget(widget: impl crate::core::widget::Widget) -> TestHarness {
    let mut h = TestHarness::new(800.0, 600.0);
    h.mount(widget);
    h.run_frame();
    h
}
