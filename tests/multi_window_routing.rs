//! Multi-window dirty routing + per-window widget-domain isolation
//! (audit 2026-07-18, the "A4 multi-window design pass").
//!
//! Locks the routing contract:
//! 1. Dirty registered on a window's AppContext wakes THAT window
//!    (per-app `on_dirty`, not a process-global last-writer slot).
//! 2. Dirty entries that land in the wrong window's registry (bridge-routed
//!    callbacks running under a stale `current_app`) are parked in a
//!    `foreign` bucket for redistribution — never ping-ponged back into the
//!    wrong window's own queue.
//! 3. Widget domains that used to be process-global thread_locals
//!    (overlay stack, toast queue) are per-AppContext: window A's overlays
//!    are invisible to window B.

use std::cell::Cell;
use std::rc::Rc;

use auralis_signal::Signal;
use burin::core::app_context::{set_current_app, AppContext};
use burin::core::element::DirtyFlags;
use burin::core::ElementId;
use burin::event::overlay::{self, OverlayEntry, OverlayLayer};
use burin::testing::TestHarness;
use burin::widgets::display::Text;
use burin::widgets::overlay::toast;

#[test]
fn dirty_on_an_app_wakes_that_app() {
    let app_a = Rc::new(AppContext::new());
    let app_b = Rc::new(AppContext::new());

    let woke_a = Rc::new(Cell::new(false));
    let woke_b = Rc::new(Cell::new(false));
    {
        let w = woke_a.clone();
        app_a.set_on_dirty(Rc::new(move || w.set(true)));
    }
    {
        let w = woke_b.clone();
        app_b.set_on_dirty(Rc::new(move || w.set(true)));
    }

    // Window A is handling an event (current), but a Weak-routed signal
    // subscription belonging to window B fires — exactly what bind_* /
    // subscribe_owned closures do for a cross-window shared Signal.
    set_current_app(&app_a);
    app_b.register_dirty(ElementId::allocate(), DirtyFlags::REPAINT);

    assert!(
        woke_b.get(),
        "dirty registered on window B's AppContext must invoke B's on_dirty wake"
    );
    assert!(
        !woke_a.get(),
        "window A must not be woken for B's dirty (wrong-window redraw)"
    );
}

#[test]
fn wake_is_coalesced_until_reset() {
    let app = Rc::new(AppContext::new());
    let wakes = Rc::new(Cell::new(0u32));
    {
        let w = wakes.clone();
        app.set_on_dirty(Rc::new(move || w.set(w.get() + 1)));
    }

    app.register_dirty(ElementId::allocate(), DirtyFlags::REPAINT);
    app.register_dirty(ElementId::allocate(), DirtyFlags::REPAINT);
    app.register_dirty(ElementId::allocate(), DirtyFlags::REPAINT);
    assert_eq!(
        wakes.get(),
        1,
        "one wake per event-loop turn (redraw_sent gate)"
    );

    app.reset_dirty_redraw();
    app.register_dirty(ElementId::allocate(), DirtyFlags::REPAINT);
    assert_eq!(wakes.get(), 2, "gate reopens after reset_dirty_redraw");
}

#[test]
fn foreign_dirty_is_parked_for_redistribution_not_ping_ponged() {
    let mut ha = TestHarness::new(200.0, 200.0);
    let mut hb = TestHarness::new(200.0, 200.0);
    let b_text = hb.mount(Text::new("hello"));
    hb.run_frame();

    // A bridge-routed callback runs while window A is current (stale
    // current_app scenario: App::about_to_wait's top-of-loop drains).
    set_current_app(ha.app());
    burin::core::dirty_registry::register_dirty(b_text, DirtyFlags::REPAINT);

    // Window A's frame: the entry's element is not in A's arena.
    ha.run_frame();

    let foreign = ha.app().take_foreign_dirty();
    assert!(
        foreign.iter().any(|(id, _)| *id == b_text),
        "entry for B's element must be parked in A's foreign bucket, got {foreign:?}"
    );

    // A's own registry must not still hold it (no ping-pong).
    set_current_app(ha.app());
    let a_pending = burin::core::dirty_registry::take_dirty();
    assert!(
        !a_pending.iter().any(|(id, _)| *id == b_text),
        "foreign entry must not remain in A's own queue"
    );

    // Redistribution: the App layer hands it to B, whose frame consumes it.
    for (id, flags) in foreign {
        hb.app().register_dirty(id, flags);
    }
    hb.run_frame();
    let leftover = hb.app().take_foreign_dirty();
    assert!(
        leftover.is_empty(),
        "B consumed its own entry; nothing re-parked"
    );
}

#[test]
fn overlay_stack_is_per_window() {
    let ha = TestHarness::new(200.0, 200.0);
    let hb = TestHarness::new(200.0, 200.0);

    set_current_app(ha.app());
    overlay::push(OverlayEntry {
        element_id: ElementId::allocate(),
        layer: OverlayLayer::Dialog,
        barrier_color: None,
        dismiss_on_click_outside: true,
        dismiss_on_escape: true,
        trap_focus: false,
        autofocus_first: false,
        previous_focus: None,
        on_dismiss: None,
    });
    assert_eq!(overlay::debug_stack_len(), 1, "window A sees its overlay");
    assert!(overlay::is_active());

    set_current_app(hb.app());
    assert_eq!(
        overlay::debug_stack_len(),
        0,
        "window B must not see window A's overlay stack"
    );
    assert!(
        !overlay::is_active(),
        "Escape/click-outside in window B must not consult A's overlays"
    );

    // Cleanup in A still works.
    set_current_app(ha.app());
    overlay::pop();
    assert_eq!(overlay::debug_stack_len(), 0);
}

#[test]
fn toast_queue_is_per_window() {
    let ha = TestHarness::new(200.0, 200.0);
    let hb = TestHarness::new(200.0, 200.0);

    set_current_app(ha.app());
    toast::show("window A toast", toast::ToastKind::Info);
    assert_eq!(toast::queue_len(), 1);

    set_current_app(hb.app());
    assert_eq!(
        toast::queue_len(),
        0,
        "window B's toast container must not drain A's queue"
    );

    set_current_app(ha.app());
    assert_eq!(toast::queue_len(), 1);
}

#[test]
fn cross_window_shared_signal_updates_both_windows() {
    // End-to-end: one Signal bound in BOTH windows; a set() issued while
    // window A is current must dirty (and wake) each window's own element.
    let counter = Signal::new(String::from("0"));

    let mut ha = TestHarness::new(200.0, 200.0);
    let a_id = ha.mount(Text::new("0").bind(counter.clone()));
    ha.run_frame();

    let mut hb = TestHarness::new(200.0, 200.0);
    let b_id = hb.mount(Text::new("0").bind(counter.clone()));
    hb.run_frame();

    let woke_b = Rc::new(Cell::new(false));
    {
        let w = woke_b.clone();
        hb.app().set_on_dirty(Rc::new(move || w.set(true)));
    }

    // Event happens in window A.
    set_current_app(ha.app());
    counter.set(String::from("7"));
    auralis_task::drain_deferred_signal_callbacks();

    assert!(
        woke_b.get(),
        "shared-signal update from window A must wake window B (Weak-routed dirty)"
    );

    ha.run_frame();
    hb.run_frame();
    hb.assert_text(b_id, "7");
    ha.assert_text(a_id, "7");
}
