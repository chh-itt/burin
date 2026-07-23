//! Re-entrancy guards (audit 2026-07-17 round 3 follow-up, Finding G).
//!
//! Callback-holding registries (`OVERLAY_STACK`, `ON_DIRTY`,
//! `RECOGNIZER_REGISTRY`) previously invoked user callbacks while holding
//! their own RefCell borrow — a callback touching the same registry
//! (dismiss chains, gesture wins that mutate the tree, dirty hooks that
//! re-register) panicked with a double borrow. All of them now take the
//! callback/entry out first and invoke it borrow-free. These tests lock
//! that contract by doing the nastiest re-entrant thing each API allows.

use std::cell::Cell;
use std::rc::Rc;

use burin::core::element::ElementId;
use burin::event::overlay::{self, OverlayEntry, OverlayLayer};
use burin::event::{
    process_pointer_event, register_recognizer, unregister_recognizer, GesturePhase,
    LongPressRecognizer,
};
use burin::style::Point;
use burin::testing::TestHarness;

fn entry(eid: ElementId, on_dismiss: Option<Box<dyn FnOnce()>>) -> OverlayEntry {
    OverlayEntry {
        element_id: eid,
        layer: OverlayLayer::Popover,
        barrier_color: None,
        dismiss_on_click_outside: false,
        dismiss_on_escape: false,
        trap_focus: false,
        autofocus_first: false,
        previous_focus: None,
        on_dismiss,
    }
}

/// on_dismiss pushes ANOTHER overlay while the stack is being mutated.
#[test]
fn overlay_dismiss_may_push_reentrantly() {
    let mut h = TestHarness::new(200.0, 200.0);
    let a = h.arena.allocate();
    let b = h.arena.allocate();

    let pushed = Rc::new(Cell::new(false));
    let p = pushed.clone();
    overlay::push(entry(
        a,
        Some(Box::new(move || {
            overlay::push(entry(b, None));
            p.set(true);
        })),
    ));

    overlay::remove(a); // fires on_dismiss -> re-entrant push
    assert!(pushed.get(), "re-entrant push executed");
    assert_eq!(
        overlay::top(),
        Some(b),
        "re-entrantly pushed overlay on top"
    );
    overlay::remove(b);
}

/// on_dismiss pops/removes other overlays (dismiss chain).
#[test]
fn overlay_dismiss_chain_does_not_panic() {
    let mut h = TestHarness::new(200.0, 200.0);
    let a = h.arena.allocate();
    let b = h.arena.allocate();
    let c = h.arena.allocate();

    overlay::push(entry(c, None));
    overlay::push(entry(b, None));
    overlay::push(entry(
        a,
        Some(Box::new(move || {
            // Chain: removing A dismisses B and pops C re-entrantly.
            overlay::remove(b);
            overlay::pop();
        })),
    ));

    overlay::remove(a);
    assert!(!overlay::is_active(), "all overlays dismissed via chain");
}

/// remove_layer with an on_dismiss that pushes into the same layer.
#[test]
fn overlay_remove_layer_reentrant_push() {
    let mut h = TestHarness::new(200.0, 200.0);
    let a = h.arena.allocate();
    let b = h.arena.allocate();

    overlay::push(entry(
        a,
        Some(Box::new(move || {
            overlay::push(entry(b, None));
        })),
    ));
    overlay::remove_layer(OverlayLayer::Popover);
    // The re-entrantly pushed overlay was added AFTER the sweep snapshot;
    // it must survive on the stack (no panic, no lost entry).
    assert_eq!(overlay::top(), Some(b));
    overlay::remove(b);
}

/// A gesture win callback (fire_on_accept) that unregisters ITS OWN
/// recognizer and registers a new one — the mount/teardown-from-gesture
/// pattern.
#[test]
fn gesture_accept_may_mutate_recognizer_registry() {
    let mut h = TestHarness::new(200.0, 200.0);
    let target = h.arena.allocate();
    let other = h.arena.allocate();

    let fired = Rc::new(Cell::new(0u32));
    let f = fired.clone();
    register_recognizer(
        target,
        10,
        burin::event::RecognizerKind::LongPress,
        Box::new(LongPressRecognizer::new()),
        Some(Box::new(move |eid, _phase| {
            f.set(f.get() + 1);
            // Nastiest allowed: mutate the registry from inside the win.
            unregister_recognizer(eid);
            register_recognizer(
                other,
                5,
                burin::event::RecognizerKind::LongPress,
                Box::new(LongPressRecognizer::new()),
                None,
            );
        })),
    );

    let path = vec![target];
    process_pointer_event(
        &path,
        GesturePhase::Started,
        Point::new(10.0, 10.0),
        1,
        true,
    );
    // Advance past the long-press duration; the timeout pass resolves the win.
    h.advance_time(800);
    burin::event::recognizer::process_timeouts();

    assert_eq!(fired.get(), 1, "accept callback fired exactly once");
    unregister_recognizer(other);
    unregister_recognizer(target);
    let _ = h;
}

/// Per-app on_dirty hook that re-enters the dirty registry (registers more
/// dirty and swaps the hook itself) — must not panic on RefCell borrows.
#[test]
fn on_dirty_hook_may_reenter_registry() {
    use burin::core::element::DirtyFlags;

    let mut h = TestHarness::new(200.0, 200.0);
    let a = h.arena.allocate();
    let b = h.arena.allocate();

    let calls = Rc::new(Cell::new(0u32));
    let c = calls.clone();
    let app = h.app().clone();
    let app_inner = app.clone();
    app.set_on_dirty(Rc::new(move || {
        c.set(c.get() + 1);
        // Re-enter: register more dirty (must not double-borrow), and
        // replace the hook itself (must not panic on the on_dirty borrow).
        app_inner.register_dirty(b, DirtyFlags::REPAINT);
        app_inner.set_on_dirty(Rc::new(|| {}));
    }));

    app.reset_dirty_redraw();
    app.register_dirty(a, DirtyFlags::REPAINT);
    assert_eq!(calls.get(), 1, "hook fired once (gated), re-entrantly safe");

    h.run_frame();
}
