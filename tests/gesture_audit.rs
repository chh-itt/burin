//! Gesture-arena audit probes (2026-07-19): verify suspected defects
//! through the PRODUCTION event flow (hit test → gesture arena →
//! propagation). The harness's `click(id)` / `long_press(id)` helpers are
//! synthetic direct-fires that bypass the arena entirely — they cannot
//! exercise these paths.
//!
//! Suspect #1 — single-member fast path: `process_pointer_event` declares
//! the arena winner on PointerDown when exactly one element in the hit
//! path has a recognizer. For a LongPressRecognizer-only element this
//! fires the long-press ON PRESS, without the 500ms hold.
//!
//! Suspect #2 — `process_timeouts` has zero callers: a motionless hold
//! never fires (only release or micro-jitter can trigger acceptance).

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

/// A quick press-release (a click) must NOT fire on_long_press.
///
/// Was a CONFIRMED BUG (audit 2026-07-19): the single-member arena fast
/// path declared the winner on PointerDown — fired long-press at press
/// time. Fixed by the G1 arena rewrite (no fast path; sweep semantics).
#[test]
fn quick_click_does_not_fire_long_press() {
    let mut h = TestHarness::new(300.0, 200.0);
    let fired: Rc<Cell<u32>> = Rc::new(Cell::new(0));
    let f = fired.clone();
    let id =
        h.mount(MouseRegion::new(Text::new("hold me")).on_long_press(move || f.set(f.get() + 1)));
    h.run_frame();
    let pos = center(&h, id);

    h.pointer_down_at(pos);
    h.advance_time(50); // released well under LONG_PRESS_DURATION_MS=500
    h.pointer_up_at(pos);
    h.run_frame();
    h.run_frame();

    assert_eq!(
        fired.get(),
        0,
        "a 50ms click must not trigger long-press (single-member arena fast path fires on press?)"
    );
}

/// A motionless 600ms hold MUST fire long-press while still held —
/// the touch/desktop standard.
///
/// Was a CONFIRMED BUG (audit 2026-07-19): `process_timeouts` had zero
/// callers. Fixed by the G1 rewrite: wired into `run_pre_passes`, with a
/// discrete scheduler deadline so it fires from a sleeping loop.
#[test]
fn motionless_hold_fires_long_press_while_held() {
    let mut h = TestHarness::new(300.0, 200.0);
    let fired: Rc<Cell<u32>> = Rc::new(Cell::new(0));
    let f = fired.clone();
    let id =
        h.mount(MouseRegion::new(Text::new("hold me")).on_long_press(move || f.set(f.get() + 1)));
    h.run_frame();
    let pos = center(&h, id);

    h.pointer_down_at(pos);
    h.run_frame();
    assert_eq!(
        fired.get(),
        0,
        "long-press must NOT fire at press time (only after the 500ms hold)"
    );

    h.advance_time(600); // past LONG_PRESS_DURATION_MS, no movement at all
    h.run_frame(); // frame tick — where timeout acceptance must run
    h.run_frame();

    assert_eq!(
        fired.get(),
        1,
        "600ms motionless hold fires long-press while held (process_timeouts wired?)"
    );

    // Release must not double-fire.
    h.pointer_up_at(pos);
    h.run_frame();
    assert_eq!(
        fired.get(),
        1,
        "release after an accepted long-press must not re-fire"
    );
}

/// Multiple recognizers per element coexist and compete (the old registry
/// was a single-slot map: registering drag + long-press overwrote drag).
#[test]
fn drag_and_long_press_coexist_on_one_element() {
    use burin::event::{
        register_recognizer, DragRecognizer, GesturePhase, LongPressRecognizer, RecognizerKind,
    };

    let mut h = TestHarness::new(300.0, 200.0);
    let drag_won: Rc<Cell<u32>> = Rc::new(Cell::new(0));
    let lp_won: Rc<Cell<u32>> = Rc::new(Cell::new(0));
    let id = h.mount(Text::new("both"));
    h.run_frame();

    let d = drag_won.clone();
    register_recognizer(
        id,
        100,
        RecognizerKind::Drag,
        Box::new(DragRecognizer::new()),
        Some(Box::new(move |_, _: GesturePhase| d.set(d.get() + 1))),
    );
    let l = lp_won.clone();
    register_recognizer(
        id,
        50,
        RecognizerKind::LongPress,
        Box::new(LongPressRecognizer::new()),
        Some(Box::new(move |_, _: GesturePhase| l.set(l.get() + 1))),
    );

    let pos = center(&h, id);

    // Sequence 1: drag 20px — the drag recognizer must win (with the old
    // single-slot registry it had been overwritten by long-press and could
    // never win), and long-press must NOT fire.
    h.pointer_down_at(pos);
    h.pointer_move_at(Point::new(pos.x + 20.0, pos.y));
    h.pointer_up_at(Point::new(pos.x + 20.0, pos.y));
    h.run_frame();
    assert_eq!(
        drag_won.get(),
        1,
        "20px drag: DragRecognizer wins the arena"
    );
    assert_eq!(
        lp_won.get(),
        0,
        "20px drag: long-press eliminated (moved too far)"
    );

    // Sequence 2: motionless 600ms hold — long-press wins, drag does not.
    h.pointer_down_at(pos);
    h.advance_time(600);
    h.run_frame();
    assert_eq!(
        lp_won.get(),
        1,
        "600ms hold: LongPressRecognizer wins via timeout"
    );
    assert_eq!(drag_won.get(), 1, "600ms hold: drag must not win");
    h.pointer_up_at(pos);
    h.run_frame();
}

/// PointerDown with a pending long-press must schedule a discrete
/// scheduler deadline, so the hold fires even from a sleeping event loop.
#[test]
fn long_press_arena_schedules_discrete_wake() {
    let mut h = TestHarness::new(300.0, 200.0);
    let id = h.mount(MouseRegion::new(Text::new("hold me")).on_long_press(|| {}));
    h.run_frame();
    let pos = center(&h, id);

    assert!(
        burin::core::scheduler::next_deadline().is_none(),
        "idle: no deadline"
    );
    h.pointer_down_at(pos);
    assert!(
        burin::core::scheduler::next_deadline().is_some(),
        "press with long-press member must schedule the timeout wake"
    );
    h.pointer_up_at(pos);
    h.run_frame();
    assert!(
        burin::core::scheduler::next_deadline().is_none(),
        "release resolves the arena and cancels the wake"
    );
}
