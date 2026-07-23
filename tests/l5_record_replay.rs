//! L5 integration tests: record/replay + crash-context (run_frame_safe / settle_safe).
//!
//! These tests deliberately expose known gaps in the current L5 implementation:
//!  - B1: `replay_events` does not rebuild the widget tree → panics on test_id lookups
//!  - B2: `Interaction::SetBoolSignal` replay is a no-op dead variant
//!  - B3: `TestRecorder` missing methods for hover/scroll/drag/key/signal

use auralis_signal::Signal;
use burin::animation::{request_anim, AnimatedProperty, AnimatedValue, Animation, EasingCurve};
use burin::core::dirty_registry;
use burin::testing::recorder::{replay_events, Interaction, TestRecorder};
use burin::testing::{TestHarness, WidgetTestExt};
use burin::widgets::display::Text;
use burin::widgets::input::Checkbox;

/// Count of CallbackPanic records currently in the error buffer.
fn callback_panic_count() -> u64 {
    burin::core::error::error_counts()
        .into_iter()
        .find(|(name, _)| *name == "CallbackPanic")
        .map_or(0, |(_, n)| n)
}

// ── Crash-context: run_frame_safe ───────────────────────────────────

#[test]
fn run_frame_safe_returns_ok_for_simple_widget() {
    let mut h = TestHarness::new(400.0, 200.0);
    h.mount(Text::new("hello safe"));
    h.run_frame(); // settle first frame

    let result = h.run_frame_safe();
    assert!(
        result.is_ok(),
        "run_frame_safe should return Ok for normal widget"
    );

    // Harness is still usable after safe call.
    let h2 = result.unwrap();
    assert_eq!(h2.dirty_count(), 0);
}

#[test]
fn run_frame_safe_captures_deferred_action_panic() {
    let mut h = TestHarness::new(400.0, 200.0);
    h.mount(Text::new("ok"));
    h.run_frame();

    let panics_before = callback_panic_count();

    // Inject a deferred action that panics during the next frame's take_actions phase.
    dirty_registry::defer_action(|_arena, _root, _reg| panic!("simulated widget callback crash"));

    // Audit 2026-07-17 round 5, C1: deferred-action panics are isolated
    // in-place (catch_unwind per action + push_error), matching the fire_*
    // contract. The frame completes normally instead of unwinding — an
    // unwind here would leave FramePhase dangling and poison RefCells.
    let result = h.run_frame_safe();
    assert!(
        result.is_ok(),
        "frame must complete — the panicking action is isolated, not propagated"
    );
    assert_eq!(
        callback_panic_count(),
        panics_before + 1,
        "the isolated panic must be reported through the error buffer"
    );

    // Harness stays usable.
    h.run_frame_safe()
        .expect("harness should survive post-panic frame");
}

#[test]
fn settle_safe_returns_frames_consumed_on_normal_sequence() {
    let mut h = TestHarness::new(400.0, 200.0);
    h.mount(Text::new("settle test"));
    h.run_frame();
    h.run_frame(); // second idle frame — should be quiescent

    let (frames, err) = h.settle_safe(10);
    assert!(
        err.is_none(),
        "settle_safe should find no panic in normal usage"
    );
    assert!(frames <= 2, "should settle quickly, not use max frames");
}

#[test]
fn settle_safe_stops_on_first_panic() {
    let mut h = TestHarness::new(400.0, 200.0);
    let id = h.mount(Text::new("settle panic"));
    h.run_frame();

    // Create ongoing work so settle_safe actually enters its frame loop
    // (otherwise has_active_work() short-circuits before any frame runs).
    request_anim(
        id,
        AnimatedProperty::Opacity,
        AnimatedValue::Float(0.0),
        AnimatedValue::Float(1.0),
        Animation {
            curve: EasingCurve::Linear,
            duration_secs: 1.0,
        },
    );
    h.run_frame(); // drain anim request into the driver

    let panics_before = callback_panic_count();
    dirty_registry::defer_action(|_arena, _root, _reg| panic!("boom at frame boundary"));

    // C1 contract: the panicking action is isolated in-place; settling
    // continues (animation still runs) and the panic is reported through
    // the error buffer instead of aborting the settle loop.
    let (_frames, err) = h.settle_safe(10);
    assert!(
        err.is_none(),
        "settle_safe must not see an unwind — the action panic is isolated"
    );
    assert_eq!(
        callback_panic_count(),
        panics_before + 1,
        "the isolated panic must be reported through the error buffer"
    );
}

/// Exposes a real bug: `has_active_work()` ignores pending deferred
/// actions queued in the dirty_registry. A system with queued tree
/// mutations is NOT quiescent, but `has_active_work()` reports it as such,
/// causing `settle()`/`settle_safe()` to terminate one frame too early.
#[test]
fn has_active_work_detects_pending_deferred_action() {
    let mut h = TestHarness::new(400.0, 200.0);
    h.mount(Text::new("idle"));
    h.run_frame();
    assert!(!h.has_active_work(), "should be quiescent after settling");

    dirty_registry::defer_action(|_arena, _root, _reg| { /* queued work */ });

    assert!(
        h.has_active_work(),
        "has_active_work() must detect pending deferred actions as outstanding work",
    );

    // Drain so the queued action doesn't leak into other tests.
    h.run_frame();
}

/// Regression: a `Signal` mutation must be reflected in `has_active_work()`.
/// `Text::bind` routes through a path that marks the element Cell, so
/// `has_active_work()` correctly reports outstanding work after a signal
/// change (guards against a future regression where signal-driven dirty
/// stops being detected).
#[test]
fn has_active_work_detects_signal_driven_dirty() {
    let mut h = TestHarness::new(400.0, 200.0);
    let label = Signal::new(String::from("A"));
    h.mount(Text::new("").bind(label.clone()));
    h.run_frame();
    h.run_frame(); // settle to quiescence
    assert!(!h.has_active_work(), "should be quiescent after settling");

    // Signal mutation: registry HashMap gets the dirty entry, element Cell does not.
    label.set(String::from("B"));

    assert!(
        h.has_active_work(),
        "has_active_work() must detect signal-driven pending dirty",
    );
}

// ── Record/replay: current behaviour ────────────────────────────────

#[test]
fn replay_runframe_and_resize_work_without_widget_tree() {
    // Events that don't reference test_ids should replay fine even on an empty tree.
    let events = vec![
        Interaction::RunFrame,
        Interaction::Resize {
            width: 300.0,
            height: 200.0,
        },
        Interaction::RunFrame,
    ];
    let h = replay_events(|_h| {}, &events);
    // Basic sanity: the harness should have processed the frames.
    // replay_events runs an auto-initial frame (id=1), then our 2 RunFrames → id=3.
    assert_eq!(
        h.frame_id(),
        3,
        "should have run 2 frames plus auto-initial"
    );
    assert_eq!(h.size().width, 300.0);
    assert_eq!(h.size().height, 200.0);
}

#[test]
#[should_panic(expected = "test_id 'btn' not found")]
fn replay_with_missing_test_id_panics() {
    // A test_id lookup that the mounted tree doesn't contain must still panic.
    let events = vec![Interaction::ClickOnTestId {
        test_id: "btn".into(),
    }];
    replay_events(
        |h| {
            h.mount(Text::new("no such id"));
        },
        &events,
    );
}

// ── Recorder: current capabilities ──────────────────────────────────

#[test]
fn recorder_records_interaction_sequence() {
    let mut rec = TestRecorder::new(400.0, 200.0);
    rec.run_frame();
    rec.advance_time(100);
    rec.run_frame();
    rec.advance_to_next_deadline();

    let events = rec.into_events();
    assert_eq!(events.len(), 4);
    assert!(matches!(events[0], Interaction::RunFrame));
    assert!(matches!(
        events[1],
        Interaction::AdvanceTime { millis: 100 }
    ));
    assert!(matches!(events[2], Interaction::RunFrame));
    assert!(matches!(events[3], Interaction::AdvanceToNextDeadline));
}

#[test]
fn recorder_settles_and_resizes() {
    let mut rec = TestRecorder::new(400.0, 200.0);
    rec.settle(5);
    rec.resize(1024.0, 768.0);

    let events = rec.into_events();
    assert_eq!(events.len(), 2);
    assert!(matches!(events[0], Interaction::Settle { max_frames: 5 }));
    assert!(matches!(
        events[1],
        Interaction::Resize {
            width: 1024.0,
            height: 768.0
        }
    ));
}

#[test]
fn recorder_click_and_type_in() {
    let mut rec = TestRecorder::new(400.0, 200.0);
    rec.run_frame();
    rec.click_on("submit");
    rec.type_into("email", "test@example.com");

    let events = rec.into_events();
    assert_eq!(events.len(), 3);
    assert!(matches!(events[1], Interaction::ClickOnTestId { .. }));
    assert!(matches!(events[2], Interaction::TypeInto { .. }));
    if let Interaction::TypeInto { test_id, text } = &events[2] {
        assert_eq!(test_id, "email");
        assert_eq!(text, "test@example.com");
    }
}

// ── End-to-end: record → replay ─────────────────────────────────────

#[test]
fn end_to_end_record_replay_with_checkbox() {
    // Record a click-on-checkbox sequence, then replay it on a fresh harness
    // built from the same mount closure and assert the signal toggled.
    let checked = Signal::new(false);

    // Record
    let checked_rec = checked.clone();
    let mut rec = TestRecorder::new(400.0, 200.0);
    let id = rec.harness.mount(Checkbox::new(checked_rec));
    rec.harness.find_mut(id).unwrap().set_test_id("cb");
    rec.run_frame();
    rec.click_on("cb");
    rec.run_frame();
    assert!(
        rec.harness.read_signal(&checked),
        "recording should toggle the checkbox"
    );

    let events = rec.into_events();

    // Replay on a fresh harness with the same widget tree.
    let replay_checked = Signal::new(false);
    let mount_checked = replay_checked.clone();
    let replayed = replay_events(
        move |h| {
            let id = h.mount(Checkbox::new(mount_checked.clone()));
            h.find_mut(id).unwrap().set_test_id("cb");
        },
        &events,
    );

    assert!(
        replayed.read_signal(&replay_checked),
        "replay must reproduce the checkbox toggle",
    );
}

#[test]
fn replay_end_to_end_uses_widget_test_ext() {
    // Same as above but uses the WidgetTestExt blanket trait to chain .test_id()
    // instead of find_mut().set_test_id().
    let checked = Signal::new(false);
    let checked_rec = checked.clone();
    let events = {
        let mut rec = TestRecorder::new(400.0, 200.0);
        rec.harness.mount(Checkbox::new(checked_rec).test_id("cb"));
        rec.run_frame();
        rec.click_on("cb");
        rec.run_frame();
        assert!(rec.harness.read_signal(&checked));
        rec.into_events()
    };
    let replay_checked = Signal::new(false);
    let mount_checked = replay_checked.clone();
    let replayed = replay_events(
        move |h| {
            h.mount(Checkbox::new(mount_checked.clone()).test_id("cb"));
        },
        &events,
    );
    assert!(replayed.read_signal(&replay_checked));
}

// ── Recorder completeness (B3) ──────────────────────────────────────

#[test]
fn recorder_records_pointer_and_key_events() {
    let mut rec = TestRecorder::new(400.0, 200.0);
    rec.run_frame();
    rec.hover_at(10.0, 20.0);
    rec.click_at(50.0, 50.0);
    rec.drag(0.0, 0.0, 100.0, 50.0);
    rec.press_key("Enter");
    rec.release_key("Enter");
    rec.run_frames(3);

    let events = rec.into_events();
    assert_eq!(events.len(), 7, "should have 7 interactions");
    assert!(matches!(
        events[1],
        Interaction::HoverAt { x: 10.0, y: 20.0 }
    ));
    assert!(matches!(
        events[2],
        Interaction::ClickAt { x: 50.0, y: 50.0 }
    ));
    assert!(matches!(
        events[3],
        Interaction::Drag {
            from_x: 0.0,
            from_y: 0.0,
            to_x: 100.0,
            to_y: 50.0
        }
    ));
    assert!(matches!(events[4], Interaction::PressKey { .. }));
    assert!(matches!(events[5], Interaction::ReleaseKey { .. }));
    assert!(matches!(events[6], Interaction::RunFrames { n: 3 }));
}

#[test]
fn recorder_records_scroll() {
    let mut rec = TestRecorder::new(400.0, 200.0);
    rec.run_frame();
    rec.scroll("list", 0.0, 50.0);

    let events = rec.into_events();
    assert_eq!(events.len(), 2);
    assert!(matches!(
        events[1],
        Interaction::ScrollOnTestId {
            dx: 0.0,
            dy: 50.0,
            ..
        }
    ));
}

// ── WidgetTestExt blanket trait (B4) ────────────────────────────────

#[test]
fn widget_test_ext_sets_test_id_on_any_widget() {
    use burin::testing::WidgetTestExt;
    let mut h = TestHarness::new(400.0, 200.0);
    let id = h.mount(Text::new("tagged").test_id("t1"));
    h.run_frame();
    assert_eq!(h.find(id).unwrap().test_id().as_deref(), Some("t1"));
}

#[test]
fn widget_test_ext_chains_test_id_and_name() {
    use burin::testing::WidgetTestExt;
    let mut h = TestHarness::new(400.0, 200.0);
    let id = h.mount(Text::new("named").test_id("tid").name("iname"));
    h.run_frame();
    let el = h.find(id).unwrap();
    assert_eq!(el.test_id().as_deref(), Some("tid"));
    assert_eq!(el.name().as_deref(), Some("iname"));
}
