//! Hover-invalidation guards (audit 2026-07-17 round 3, Finding C).
//!
//! `set_state_dirty(HOVERED, _)` skips repaint + subtree-gen invalidation for
//! elements whose `StateStyle.hovered` variant is empty — crossing element
//! boundaries over static content must not register dirty or drive paint.
//! Elements WITH hover visuals (Checkbox etc.) must keep repainting, and
//! hover callbacks (MouseRegion) must keep firing regardless.

use std::cell::Cell;
use std::rc::Rc;

use auralis_signal::Signal;
use burin::core::dirty_registry;
use burin::style::{Color, Point, Styled};
use burin::testing::TestHarness;
use burin::widgets::display::Text;
use burin::widgets::input::Checkbox;
use burin::widgets::layout::{HStack, SizedBox, VStack};

fn static_panels() -> HStack {
    let mut left = VStack::new();
    let mut right = VStack::new();
    for i in 0..10 {
        left = left.push(
            SizedBox::new()
                .height(30.0)
                .child(Text::new(format!("left {i}")))
                .background(Color::rgba8(38, 38, 46, 255)),
        );
        right = right.push(
            SizedBox::new()
                .height(30.0)
                .child(Text::new(format!("right {i}")))
                .background(Color::rgba8(46, 38, 38, 255)),
        );
    }
    HStack::new()
        .push(SizedBox::new().width(300.0).height(400.0).child(left))
        .push(SizedBox::new().width(300.0).height(400.0).child(right))
}

#[test]
fn hover_crossing_static_content_registers_zero_dirty() {
    let mut h = TestHarness::new(700.0, 500.0);
    h.mount(static_panels());
    for _ in 0..3 {
        h.run_frame();
    }
    assert!(!dirty_registry::has_pending_dirty(), "settled before hover");

    // Cross panels / rows repeatedly — pure static content, no hover styles.
    for (x, y) in [
        (150.0, 100.0),
        (450.0, 100.0),
        (150.0, 200.0),
        (150.0, 230.0),
    ] {
        h.hover_at(Point::new(x, y));
        assert!(
            !dirty_registry::has_pending_dirty(),
            "hover crossing static content at ({x},{y}) must register zero dirty"
        );
        h.run_frame();
        assert_eq!(h.frame_dirty_set_size(), 0, "no dirty processed either");
    }
}

#[test]
fn hover_over_state_styled_widget_still_repaints() {
    let mut h = TestHarness::new(700.0, 500.0);
    let checked = Signal::new(false);
    h.mount(
        VStack::new().push(
            SizedBox::new()
                .width(200.0)
                .height(40.0)
                .child(Checkbox::new(checked)),
        ),
    );
    for _ in 0..3 {
        h.run_frame();
    }

    // Hover onto the checkbox: its StateStyle.hovered has a background,
    // so dirty MUST be registered.
    h.hover_at(Point::new(15.0, 15.0));
    assert!(
        dirty_registry::has_pending_dirty(),
        "hovering a state-styled widget must register dirty"
    );
    h.run_frame();

    // And leaving must repaint again.
    h.unhover();
    assert!(
        dirty_registry::has_pending_dirty(),
        "unhovering a state-styled widget must register dirty"
    );
    h.run_frame();
}

#[test]
fn hover_callbacks_fire_even_without_hover_visuals() {
    use burin::widgets::decoration::MouseRegion;

    let mut h = TestHarness::new(700.0, 500.0);
    let enters = Rc::new(Cell::new(0u32));
    let leaves = Rc::new(Cell::new(0u32));
    let e = enters.clone();
    let l = leaves.clone();
    h.mount(
        VStack::new().push(
            MouseRegion::new(
                SizedBox::new()
                    .width(200.0)
                    .height(60.0)
                    .child(Text::new("plain region")),
            )
            .on_hover_enter(move || e.set(e.get() + 1))
            .on_hover_leave(move || l.set(l.get() + 1)),
        ),
    );
    for _ in 0..3 {
        h.run_frame();
    }

    h.hover_at(Point::new(50.0, 30.0));
    h.run_frame();
    assert_eq!(enters.get(), 1, "hover enter callback fired");

    h.hover_at(Point::new(650.0, 450.0));
    h.run_frame();
    assert_eq!(leaves.get(), 1, "hover leave callback fired");
}

/// SEAM-0 parity: hovering a nested target flips HOVERED along the whole
/// ancestor chain (production window semantics), not just the leaf.
#[test]
fn hover_chain_flips_ancestors_not_just_leaf() {
    use burin::core::config::StateFlags;

    let mut h = TestHarness::new(700.0, 500.0);
    let page = h.mount(static_panels());
    for _ in 0..3 {
        h.run_frame();
    }

    h.hover_at(Point::new(150.0, 100.0));
    // Count hovered elements: must exceed 1 (chain, not leaf-only).
    let mut hovered = 0;
    let mut stack = vec![page];
    while let Some(id) = stack.pop() {
        if let Some(el) = h.find(id) {
            if el.state.get().contains(StateFlags::HOVERED) {
                hovered += 1;
            }
            for &c in &el.children {
                stack.push(c);
            }
        }
    }
    assert!(
        hovered > 1,
        "HOVERED must flip along the ancestor chain (got {hovered})"
    );
}
