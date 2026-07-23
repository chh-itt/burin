//! Phase 3.9 guard: MEASURE-class dirty must not pre-stain ancestors
//! with REPAINT on the upward walk.
//!
//! Rationale: `layout_phase` already diffs old/new bounds after taffy and
//! marks REPAINT on exactly the elements whose rect actually changed
//! (frame_pipeline.rs "old-bounds diff"). Merging REPAINT into the
//! ancestor chain during propagation therefore double-books the work:
//! ancestors whose bounds end up unchanged re-record their own scene
//! every frame (surface/decor generations untouched) instead of replaying
//! it — the exact pattern the over-render detector exists to catch.
//!
//! Repro shape: a per-frame text update (frame counter, timer, typing
//! indicator) inside a fixed-size container. The text's MEASURE climbs
//! the chain; ancestor bounds never change.

use auralis_signal::Signal;
use burin::testing::TestHarness;
use burin::widgets::display::Text;
use burin::widgets::layout::{HStack, ScrollView, VStack};

fn drain_over_render_warnings() -> Vec<String> {
    burin::debug::OVER_RENDER.with(|d| d.borrow_mut().drain_warnings())
}

/// 40 frames of per-frame text-binding notifications inside an all-auto
/// container chain (no relayout boundary between the text and the root —
/// the worst case: MEASURE climbs the whole chain every frame). The text
/// value never changes, so no ancestor's bounds ever change: none of them
/// may trip the over-render detector (threshold: 30 consecutive unchanged
/// re-records).
#[test]
fn per_frame_text_update_does_not_rerecord_clean_ancestors() {
    let mut h = TestHarness::new(400.0, 300.0);
    let label = Signal::new(String::from("tick"));
    // Mirror the animation_gallery shape: a ScrollView content chain is
    // content-sized on both axes (no relayout boundary), so per-frame
    // MEASURE climbs all the way up.
    h.mount(
        ScrollView::new()
            .child(VStack::new().push(HStack::new().push(Text::new("tick").bind(label.clone())))),
    );
    h.run_frame();
    h.run_frame();
    drain_over_render_warnings(); // reset any mount noise

    for _ in 0..40 {
        // Same value: a coarse binding that notifies without deduping —
        // REPAINT|MEASURE fires each frame, bounds never change.
        label.set(String::from("tick"));
        h.run_frame();
    }

    let warnings = drain_over_render_warnings();
    assert!(
        warnings.is_empty(),
        "clean ancestors must replay, not re-record, during per-frame text updates:\n{}",
        warnings.join("\n")
    );
}
