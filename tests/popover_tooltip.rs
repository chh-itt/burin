use auralis_signal::Signal;
use burin::testing::selector::*;
use burin::testing::test_harness::TestHarness;
use burin::widgets::display::Text;
use burin::widgets::overlay::{Popover, Tooltip};

#[test]
fn popover_mounts_closed() {
    let open = Signal::new(false);
    let mut h = TestHarness::new(400.0, 300.0);
    h.mount(Popover::new(
        open.clone(),
        Text::new("Anchor"),
        Text::new("Content"),
    ));
    h.run_frame();
    assert!(
        h.find_sel(by_text("Anchor")).is_some(),
        "Anchor should be visible when closed"
    );
}

#[test]
fn tooltip_mounts_with_anchor() {
    let mut h = TestHarness::new(400.0, 300.0);
    h.mount(Tooltip::new(
        Text::new("Hover me"),
        Text::new("Tooltip text"),
    ));
    h.run_frame();
    assert!(
        h.find_sel(by_text("Hover me")).is_some(),
        "Tooltip anchor visible at mount"
    );
}

#[test]
fn tooltip_unhover_no_panic() {
    let mut h = TestHarness::new(400.0, 300.0);
    h.mount(Tooltip::new(Text::new("Target"), Text::new("Details")));
    h.run_frame();
    h.unhover();
    h.run_frame();
}
