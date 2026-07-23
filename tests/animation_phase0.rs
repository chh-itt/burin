//! Phase 0 animation-architecture guards (audit 2026-07-18 animation pass).
//!
//! - Toast `Visible` (static hold) period must be quiescent: no per-frame
//!   MEASURE/REPAINT dirty, no paint output. The 300ms enter / 200ms exit
//!   transitions are the only continuously-painting windows.
//! - The animation timeline (`clock::animation_millis`) follows the virtual
//!   clock, so periodic visuals (spinner sweep, shimmer) are deterministic.

use burin::core::clock;
use burin::testing::TestHarness;
use burin::widgets::display::Text;
use burin::widgets::layout::VStack;
use burin::widgets::overlay::{toast, ToastContainer, ToastKind};

/// Drive a toast through Entering into its Visible hold, then assert the
/// hold is fully quiescent (no repaint churn while nothing moves).
#[test]
fn toast_visible_hold_is_quiescent() {
    let mut h = TestHarness::new(800.0, 600.0);
    h.mount(
        VStack::new()
            .push(Text::new("app content"))
            .push(ToastContainer::new()),
    );
    h.run_frame();

    toast::show("saved", ToastKind::Info);
    h.run_frame(); // dequeue → Entering starts

    // Cross the 300ms enter transition on the virtual clock.
    h.advance_time(400);
    h.run_frame(); // Entering(t>=1) → Visible { deadline }
    h.run_frame(); // first full Visible frame (residual settle allowed)

    // Static hold: repeated frames with no time advance and no input must
    // not paint. Before the fix the frame_tick set `changed = true`
    // unconditionally → MEASURE+REPAINT every frame for the whole hold.
    for i in 0..3 {
        h.run_frame();
        assert!(
            !h.last_painted(),
            "toast Visible hold painted on idle frame {i} — per-frame dirty churn"
        );
    }

    // Sanity: the toast is still on screen (its exit deadline is 4s away).
    h.advance_time(1000);
    h.run_frame();
    assert!(!h.last_painted(), "1s into the hold, still quiescent");
}

/// After the hold deadline passes, the toast must exit and paint again
/// (the quiescence fix must not freeze the exit transition).
#[test]
fn toast_exits_after_hold_deadline() {
    let mut h = TestHarness::new(800.0, 600.0);
    h.mount(ToastContainer::new());
    h.run_frame();

    toast::show_duration("bye", ToastKind::Info, 500);
    h.run_frame();
    h.advance_time(400); // finish enter (300ms)
    h.run_frame();
    h.run_frame();

    h.advance_time(600); // cross the 500ms hold deadline
    h.run_frame(); // Visible → Exiting
    h.advance_time(100); // mid-exit (200ms total)
    h.run_frame();
    assert!(h.last_painted(), "exit transition must repaint");

    h.advance_time(300); // finish exit
    h.run_frame();
    h.run_frame();
    assert!(!h.last_painted(), "after exit completes, quiescent again");
}

/// The animation timeline is virtual-clock driven and 0-based.
#[test]
fn animation_timeline_is_deterministic() {
    let _h = TestHarness::new(100.0, 100.0); // installs the virtual clock
    let t0 = clock::animation_millis();
    clock::advance(std::time::Duration::from_millis(250));
    assert_eq!(clock::animation_millis() - t0, 250);
}
