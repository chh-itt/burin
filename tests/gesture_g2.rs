//! G2a guards: drag authority inversion — the gesture arena gates drag
//! event synthesis (audit 2026-07-19).
//!
//! - Default `DragArbitration::Eager`: drag_start fires at PointerDown,
//!   drag_update from the first move — the historical zero-threshold
//!   feel that slider / text-selection / color-picker depend on.
//! - Opt-in `DragArbitration::Threshold`: drag events are gated until
//!   the arena's DragRecognizer wins (6px) — tap-vs-drag disambiguation
//!   for reorderable rows etc.
//! - Winner capture: once a drag wins, updates route to the captured
//!   element regardless of where the pointer wanders (no hit-path
//!   dependence mid-drag).

use burin::event::DragArbitration;
use burin::style::Point;
use burin::testing::TestHarness;
use burin::widgets::decoration::MouseRegion;
use burin::widgets::display::Text;
use std::cell::Cell;
use std::rc::Rc;

fn center(h: &TestHarness, id: burin::core::ElementId) -> Point {
    let b = h.find(id).unwrap().screen_bounds;
    Point::new(b.x + b.width / 2.0, b.y + b.height / 2.0)
}

/// Eager (default): updates flow from the very first pixel of motion.
/// This is the historical contract sliders and text selection rely on.
#[test]
fn eager_drag_fires_from_first_move() {
    let mut h = TestHarness::new(300.0, 200.0);
    let updates: Rc<Cell<u32>> = Rc::new(Cell::new(0));
    let starts: Rc<Cell<u32>> = Rc::new(Cell::new(0));
    let u = updates.clone();
    let s = starts.clone();
    let id = h.mount(
        MouseRegion::new(Text::new("slider-like"))
            .on_drag_start(move |_, _| s.set(s.get() + 1))
            .on_drag_update(move |_, _| u.set(u.get() + 1)),
    );
    h.run_frame();
    let pos = center(&h, id);

    h.pointer_down_at(pos);
    assert_eq!(starts.get(), 1, "eager: drag_start at PointerDown");
    h.pointer_move_at(Point::new(pos.x + 2.0, pos.y)); // 2px — under any threshold
    assert_eq!(updates.get(), 1, "eager: update on a 2px move");
    h.pointer_up_at(Point::new(pos.x + 2.0, pos.y));
}

/// Threshold (opt-in): no drag events until the arena's 6px verdict;
/// then drag_start + updates flow.
#[test]
fn threshold_drag_gates_updates_until_the_arena_verdict() {
    let mut h = TestHarness::new(300.0, 200.0);
    let updates: Rc<Cell<u32>> = Rc::new(Cell::new(0));
    let starts: Rc<Cell<u32>> = Rc::new(Cell::new(0));
    let u = updates.clone();
    let s = starts.clone();
    let id = h.mount(
        MouseRegion::new(Text::new("reorder-like"))
            .drag_arbitration(DragArbitration::Threshold)
            .on_drag_start(move |_, _| s.set(s.get() + 1))
            .on_drag_update(move |_, _| u.set(u.get() + 1)),
    );
    h.run_frame();
    let pos = center(&h, id);

    h.pointer_down_at(pos);
    assert_eq!(starts.get(), 0, "threshold: no drag_start at press");
    h.pointer_move_at(Point::new(pos.x + 3.0, pos.y)); // under 6px
    assert_eq!(updates.get(), 0, "threshold: 3px jitter is NOT a drag");

    h.pointer_move_at(Point::new(pos.x + 10.0, pos.y)); // past 6px — verdict
    assert_eq!(
        starts.get(),
        1,
        "threshold: drag_start fires on the verdict"
    );
    assert!(
        updates.get() >= 1,
        "threshold: updates flow after the verdict"
    );

    let before_up = updates.get();
    h.pointer_up_at(Point::new(pos.x + 12.0, pos.y));
    let _ = before_up;
}

/// Once a drag wins, updates route to the CAPTURED element even when the
/// pointer leaves its bounds (mid-drag hit paths are irrelevant).
#[test]
fn drag_capture_survives_leaving_the_element() {
    let mut h = TestHarness::new(400.0, 300.0);
    let updates: Rc<Cell<u32>> = Rc::new(Cell::new(0));
    let u = updates.clone();
    let id = h.mount(
        MouseRegion::new(Text::new("grab me")).on_drag_update(move |_, _| u.set(u.get() + 1)),
    );
    h.run_frame();
    let pos = center(&h, id);

    h.pointer_down_at(pos);
    h.pointer_move_at(Point::new(pos.x + 5.0, pos.y));
    let inside = updates.get();
    assert!(inside >= 1, "updates while inside");

    // Wander far outside the element's bounds.
    h.pointer_move_at(Point::new(pos.x + 150.0, pos.y + 100.0));
    assert!(
        updates.get() > inside,
        "updates must keep flowing to the captured element outside its bounds"
    );
    h.pointer_up_at(Point::new(pos.x + 150.0, pos.y + 100.0));
}

/// A threshold-arbitrated element that never crosses 6px gets NO drag
/// events at all — the press-release is a clean tap.
#[test]
fn threshold_drag_never_fires_on_a_clean_tap() {
    let mut h = TestHarness::new(300.0, 200.0);
    let any_drag: Rc<Cell<u32>> = Rc::new(Cell::new(0));
    let a1 = any_drag.clone();
    let a2 = any_drag.clone();
    let a3 = any_drag.clone();
    let id = h.mount(
        MouseRegion::new(Text::new("tappable row"))
            .drag_arbitration(DragArbitration::Threshold)
            .on_drag_start(move |_, _| a1.set(a1.get() + 1))
            .on_drag_update(move |_, _| a2.set(a2.get() + 1))
            .on_drag_end(move |_, _| a3.set(a3.get() + 1)),
    );
    h.run_frame();
    let pos = center(&h, id);

    h.pointer_down_at(pos);
    h.pointer_move_at(Point::new(pos.x + 2.0, pos.y));
    h.pointer_up_at(Point::new(pos.x + 2.0, pos.y));
    h.run_frame();

    assert_eq!(
        any_drag.get(),
        0,
        "a clean tap on a threshold element fires no drag events"
    );
}
