//! Regression: the paint-build debug trace truncated label text by BYTE index
//! (`&text_str[..len.min(40)]`), which panicked when byte 40 landed inside a
//! multibyte UTF-8 char (e.g. Chinese). Truncation is now char-based. This test
//! drives a signal-bound Text through the lazy paint-rebuild path with a long
//! Chinese string and asserts no panic.

use auralis_signal::Signal;
use burin::testing::TestHarness;
use burin::widgets::display::Text;

#[test]
fn long_multibyte_label_does_not_panic_on_paint_rebuild() {
    let label = Signal::new(String::from("short"));
    let mut h = TestHarness::new(400.0, 200.0);
    h.mount(Text::new("short").bind(label.clone()));
    h.run_frame();

    // Update to a long Chinese string (>40 bytes; each 汉字 is 3 bytes).
    // 14 chars = 42 bytes; byte 40 falls inside the 14th char's bytes.
    label.set("中文中文中文中文中文中文中文中".to_string());
    h.run_frame();
    h.run_frame();

    // Reaching here without a panic is the assertion.
    let scene = h.drain_scene();
    let _ = scene; // rendered without panic
}
