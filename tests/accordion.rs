use auralis_signal::Signal;
use burin::testing::selector::*;
use burin::testing::test_harness::TestHarness;
use burin::widgets::composite::Accordion;
use burin::widgets::display::Text;
use std::collections::HashSet;

#[test]
fn accordion_mounts_with_sections() {
    let mut h = TestHarness::new(400.0, 600.0);
    let open = Signal::new(HashSet::new());
    h.mount(
        Accordion::new(open)
            .section("Section 1", Text::new("Content A"))
            .section("Section 2", Text::new("Content B")),
    );
    h.run_frame();
    assert!(
        h.find_sel(by_text("Section 1")).is_some(),
        "Section 1 header visible"
    );
    assert!(
        h.find_sel(by_text("Section 2")).is_some(),
        "Section 2 header visible"
    );
}

#[test]
fn accordion_section_expand_collapse() {
    let mut h = TestHarness::new(400.0, 600.0);
    let open = Signal::new(HashSet::new());
    h.mount(
        Accordion::new(open.clone())
            .section("S1", Text::new("C1"))
            .section("S2", Text::new("C2")),
    );
    h.run_frame();

    // Click section 1 header to expand
    let s1 = h.find_sel(by_text("S1")).expect("Section 1 header");
    h.click(s1);
    h.run_frame();
    h.run_frame();

    // Content should be visible
    let c1 = h.find_sel(by_text("C1"));
    assert!(c1.is_some(), "Section 1 content visible after expand");

    // Click again to collapse
    h.click(s1);
    h.run_frame();
    h.run_frame();
}

#[test]
fn accordion_rapid_toggle_no_panic() {
    let mut h = TestHarness::new(400.0, 600.0);
    let open = Signal::new(HashSet::new());
    h.mount(
        Accordion::new(open)
            .allow_multiple()
            .section("Toggle", Text::new("Inside")),
    );
    h.run_frame();

    let header = h.find_sel(by_text("Toggle")).expect("Header");
    for _ in 0..5 {
        h.click(header);
        h.run_frame();
    }
}
