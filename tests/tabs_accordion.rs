//! Tabs and Accordion contract tests.
//!
//! Tests against the specification contract, not implementation details.

use auralis_signal::Signal;
use burin::core::config::StateFlags;
use burin::testing::selector::by_text;
use burin::testing::TestHarness;
use burin::widgets::composite::{Accordion, AccordionSection, Tab, TabBar, TabPanel};
use burin::widgets::display::Text;
use std::collections::HashSet;

// ═══════════════════════════════════════════════════════════════════
// Tabs
// ═══════════════════════════════════════════════════════════════════

#[test]
fn tab_bar_mounts_multiple_tabs() {
    let mut h = TestHarness::new(400.0, 300.0);
    let active = Signal::new(0usize);
    let id = h.mount(
        TabBar::new(active.clone())
            .tab("First")
            .tab("Second")
            .tab("Third"),
    );
    h.run_frame();

    h.assert_visible(id);
    h.assert_child_count(id, 3);
    let el = h.find(id).unwrap();
    for child in &el.children {
        let child_el = h.find(*child).unwrap();
        assert!(
            child_el.screen_bounds.width > 0.0,
            "each tab should have non-zero width",
        );
    }
}

#[test]
fn tab_selection_changes_active_signal() {
    let mut h = TestHarness::new(600.0, 400.0);
    let active = Signal::new(0usize);

    let _tab_bar = h.mount(
        TabBar::new(active.clone())
            .tab("First")
            .tab("Second")
            .tab("Third"),
    );
    let panel0 = h.mount(TabPanel::new(0, active.clone(), Text::new("Panel 0")));
    let panel1 = h.mount(TabPanel::new(1, active.clone(), Text::new("Panel 1")));
    h.run_frame();

    // Initially active = 0 → panel0 visible, panel1 hidden
    assert!(
        !h.find(panel0).unwrap().slot_inactive.get(),
        "panel 0 should be visible at active=0"
    );
    assert!(
        h.find(panel1).unwrap().slot_inactive.get(),
        "panel 1 should be hidden at active=0"
    );

    // Set active to 1 → panel1 visible, panel0 hidden
    h.set_signal(&active, 1);
    h.run_frame();

    assert!(
        h.find(panel0).unwrap().slot_inactive.get(),
        "panel 0 should be hidden at active=1"
    );
    assert!(
        !h.find(panel1).unwrap().slot_inactive.get(),
        "panel 1 should be visible at active=1"
    );
}

#[test]
fn tab_disabled_has_state_flag() {
    let mut h = TestHarness::new(400.0, 300.0);
    let active = Signal::new(0usize);
    let id = h.mount(
        TabBar::new(active.clone())
            .tab("First")
            .tab_full(Tab::new("Second").disabled())
            .tab("Third"),
    );
    h.run_frame();

    let tab_ids = h.find(id).unwrap().children.clone();
    assert!(
        tab_ids.len() >= 3,
        "TabBar should have 3 child tab elements"
    );

    h.assert_state(tab_ids[1], StateFlags::DISABLED, true);
    h.assert_state(tab_ids[0], StateFlags::DISABLED, false);
    h.assert_state(tab_ids[2], StateFlags::DISABLED, false);
}

#[test]
fn tab_panel_visibility_respects_active_index() {
    let mut h = TestHarness::new(600.0, 400.0);
    let active = Signal::new(0usize);

    let _tab_bar = h.mount(TabBar::new(active.clone()).tab("First").tab("Second"));
    let panel0 = h.mount(TabPanel::new(0, active.clone(), Text::new("Content 0")));
    let panel1 = h.mount(TabPanel::new(1, active.clone(), Text::new("Content 1")));
    h.run_frame();

    // Active = 0 → panel 0 visible, panel 1 hidden
    assert!(
        !h.find(panel0).unwrap().slot_inactive.get(),
        "panel 0 should be visible"
    );
    assert!(
        h.find(panel1).unwrap().slot_inactive.get(),
        "panel 1 should be hidden"
    );

    // Active = 1 → panel 1 visible, panel 0 hidden
    h.set_signal(&active, 1);
    h.run_frame();

    assert!(
        h.find(panel0).unwrap().slot_inactive.get(),
        "panel 0 should be hidden"
    );
    assert!(
        !h.find(panel1).unwrap().slot_inactive.get(),
        "panel 1 should be visible"
    );
}

// ═══════════════════════════════════════════════════════════════════
// Accordion
// ═══════════════════════════════════════════════════════════════════

#[test]
fn accordion_mounts_with_sections() {
    let mut h = TestHarness::new(400.0, 600.0);
    let open = Signal::new(HashSet::new());
    let id = h.mount(
        Accordion::new(open)
            .section("Section 1", Text::new("Content A"))
            .section("Section 2", Text::new("Content B")),
    );
    h.run_frame();

    h.assert_visible(id);
    let el = h.find(id).unwrap();
    assert!(
        el.children.len() >= 2,
        "Accordion should have at least 2 section children"
    );
}

#[test]
fn accordion_section_disabled_sets_flag() {
    let mut h = TestHarness::new(400.0, 600.0);
    let open = Signal::new(HashSet::new());
    h.mount(
        Accordion::new(open)
            .section("Section 1", Text::new("Content A"))
            .section_with(AccordionSection::new("Section 2", Text::new("Content B")).disabled()),
    );
    h.run_frame();

    // Accordion wraps each section header in a Button whose mount sets
    // StateFlags::DISABLED on the button element itself (the element
    // identified by its a11y label).
    let btn = h
        .find_sel(by_text("Section 2"))
        .expect("header button for section 2 should be mounted");
    h.assert_state(btn, StateFlags::DISABLED, true);

    // Section 1 (not disabled) should NOT have DISABLED
    let btn1 = h
        .find_sel(by_text("Section 1"))
        .expect("header button for section 1 should be mounted");
    h.assert_state(btn1, StateFlags::DISABLED, false);
}

#[test]
fn accordion_toggle_changes_open_signal() {
    let mut h = TestHarness::new(400.0, 600.0);
    let open = Signal::new(HashSet::new());
    let id = h.mount(
        Accordion::new(open.clone())
            .section("Section 1", Text::new("Content A"))
            .section("Section 2", Text::new("Content B")),
    );
    h.run_frame();

    h.assert_visible(id);

    // Initially the open set is empty
    assert!(open.read().is_empty(), "open set should be empty initially");

    // Add index 0 to the open set — simulate programmatic expansion
    let mut set = HashSet::new();
    set.insert(0usize);
    h.set_signal(&open, set);
    h.run_frame();
}
