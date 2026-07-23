//! Phase 1 animation-architecture guards: keyed continuous-wake
//! subscriptions (audit 2026-07-18 animation pass).
//!
//! The indeterminate Progress spinner is the canonical "continuously
//! visible animation with no external dirty source". Before Phase 1 it
//! froze on idle windows: nothing marked it dirty, nothing held a frame
//! wake, so the sweep only advanced when the user happened to interact.

use auralis_signal::Signal;
use burin::core::scheduler;
use burin::testing::TestHarness;
use burin::widgets::display::{Progress, ProgressKind, Text};
use burin::widgets::layout::VStack;

/// An indeterminate spinner must repaint on every idle frame and hold a
/// continuous scheduler subscription so the real event loop keeps pumping.
#[test]
fn indeterminate_spinner_repaints_on_idle_frames() {
    let mut h = TestHarness::new(300.0, 200.0);
    h.mount(
        VStack::new().push(Text::new("loading…")).push(
            Progress::new(Signal::new(0.0))
                .kind(ProgressKind::Circular)
                .indeterminate(),
        ),
    );
    h.run_frame();

    for i in 0..3 {
        h.advance_time(30);
        h.run_frame();
        assert!(
            h.last_painted(),
            "idle frame {i}: spinner must repaint (frozen-spinner bug)"
        );
    }
    assert!(
        scheduler::has_continuous(),
        "spinner must hold a continuous wake so the window keeps pumping"
    );
}

/// A determinate Progress is a static visual — it must NOT subscribe to
/// continuous wakes or repaint on idle frames.
#[test]
fn determinate_progress_stays_quiescent() {
    let mut h = TestHarness::new(300.0, 200.0);
    h.mount(Progress::new(Signal::new(40.0)));
    h.run_frame();
    h.run_frame();

    h.advance_time(30);
    h.run_frame();
    assert!(
        !h.last_painted(),
        "determinate progress must stay quiescent"
    );
    assert!(
        !scheduler::has_continuous(),
        "no continuous wake for static progress"
    );
}

/// Removing the spinner element must release its wake subscription —
/// otherwise a dismissed loading view pins the event loop forever.
#[test]
fn spinner_releases_wake_on_unmount() {
    let mut h = TestHarness::new(300.0, 200.0);
    let id = h.mount(Progress::new(Signal::new(0.0)).indeterminate());
    h.run_frame();
    assert!(
        scheduler::has_continuous(),
        "mounted spinner holds the wake"
    );

    h.arena.remove(id);
    h.run_frame();
    assert!(
        !scheduler::has_continuous(),
        "unmounted spinner must release its continuous wake"
    );
}
