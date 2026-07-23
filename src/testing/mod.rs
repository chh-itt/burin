//! Headless test harness for full-frame simulation without a window or GPU.
//!
//! The `TestHarness` composes all the subsystems that `Window::on_frame()` uses
//! (taffy layout, event registry, focus, gesture, animation) and drives them
//! directly in memory. This enables:
//!
//! - Semantic assertions on element state (text, visibility, bounds, focus, dirty flags)
//! - Simulated user interactions (click, hover, keyboard input)
//! - Full frame lifecycle control (single step or batched)
//! - Signal manipulation followed by frame advancement
//!
//! No winit event loop, no GPU, no window — everything runs on the CPU.

pub mod fixture;
pub mod invariant;
#[cfg(feature = "backend-tiny-skia")]
pub mod pixel;
pub mod probes;
pub mod recorder;
pub mod selector;
#[cfg(feature = "backend-tiny-skia")]
pub mod snapshot;
pub mod test_harness;
pub mod widget_test_ext;
pub use selector::Selector;
pub use test_harness::TestHarness;
pub use widget_test_ext::WidgetTestExt;
