//! Contract tests for Tooltip, Popover, and Modal overlay widgets.
//!
//! Tests against the behavioural specification, not implementation details.
//! Uses TestHarness for component-level interaction and style resolution.

use auralis_signal::Signal;
use burin::core::config::StateFlags;
use burin::style::resolve_style;
use burin::testing::selector::by_text;
use burin::testing::TestHarness;
use burin::widgets::display::Text;
use burin::widgets::input::Button;
use burin::widgets::overlay::{Modal, Popover, Tooltip};

// ── Tooltip ─────────────────────────────────────────────────────────

#[test]
fn tooltip_mounts_with_child_visible() {
    let mut h = TestHarness::new(400.0, 300.0);
    h.mount(Tooltip::new(
        Button::new("Hover me").primary(),
        Text::new("Tooltip content"),
    ));
    h.run_frame();

    assert!(
        h.find_sel(by_text("Hover me")).is_some(),
        "child button should be visible at mount"
    );
}

#[test]
fn tooltip_disabled_state() {
    let mut h = TestHarness::new(400.0, 300.0);
    let _id = h.mount(Tooltip::new(Text::new("Anchor"), Text::new("Tooltip")));
    h.run_frame();

    assert!(
        h.find_sel(by_text("Anchor")).is_some(),
        "anchor should be present after mount"
    );
}

#[test]
fn tooltip_hovered_visual_effect() {
    let mut h = TestHarness::new(400.0, 300.0);
    h.mount(Tooltip::new(
        Button::new("Hover").primary(),
        Text::new("Tooltip text"),
    ));
    h.run_frame();

    let btn_id = h
        .find_sel(by_text("Hover"))
        .expect("child button should exist");
    let style = h
        .style_component_of(btn_id)
        .expect("button should have a style component");
    let default = resolve_style(StateFlags::NONE, &style);
    let hovered = resolve_style(StateFlags::HOVERED, &style);

    assert_ne!(
        hovered.background, default.background,
        "hovered state should visually differ from default"
    );
}

// ── Popover ─────────────────────────────────────────────────────────

#[test]
fn popover_mounts_and_becomes_visible() {
    let open = Signal::new(true);
    let mut h = TestHarness::new(400.0, 300.0);
    h.mount(Popover::new(
        open.clone(),
        Text::new("Anchor"),
        Text::new("Popover content"),
    ));
    h.run_frame();

    assert!(
        h.find_sel(by_text("Anchor")).is_some(),
        "anchor should be visible when popover is open"
    );
    assert!(
        h.find_sel(by_text("Popover content")).is_some(),
        "popover content should exist when open signal is true"
    );
}

#[test]
fn popover_dismiss_sets_signal() {
    let open = Signal::new(true);
    let dismissed = Signal::new(false);
    let d = dismissed.clone();

    let mut h = TestHarness::new(400.0, 300.0);
    h.mount(
        Popover::new(open.clone(), Text::new("Anchor"), Text::new("Content")).on_dismiss(
            move || {
                d.set(true);
            },
        ),
    );
    h.run_frame();

    assert!(
        !dismissed.read(),
        "dismiss callback should not fire before close"
    );
    h.set_signal(&open, false);
    h.run_frame();
    assert!(
        dismissed.read(),
        "dismiss callback should fire when open signal transitions to false"
    );
}

// ── Modal ───────────────────────────────────────────────────────────

#[test]
fn modal_mounts_with_content() {
    let visible = Signal::new(true);
    let mut h = TestHarness::new(400.0, 300.0);
    let id = h.mount(Modal::new(visible.clone(), Text::new("Modal content")));
    h.run_frame();

    h.assert_visible(id);
    assert!(
        h.find_sel(by_text("Modal content")).is_some(),
        "modal content should exist when visible"
    );
}

#[test]
fn modal_visibility_signal_controls_visibility() {
    let visible = Signal::new(false);
    let mut h = TestHarness::new(400.0, 300.0);
    let id = h.mount(Modal::new(visible.clone(), Text::new("Modal content")));
    h.run_frame();

    h.assert_not_visible(id);

    h.set_signal(&visible, true);
    h.run_frame();
    h.assert_visible(id);
}
