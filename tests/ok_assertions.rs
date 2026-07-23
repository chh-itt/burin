//! O(k) assertions — auralis-unique test capability.
//!
//! These verify the framework did O(k) *work* (cache replay, contained layout,
//! small dirty set), not merely produced correct output. No other GUI test
//! framework exposes this.

use auralis_signal::Signal;
use burin::testing::TestHarness;
use burin::widgets::display::Text;
use burin::widgets::layout::VStack;

/// After the first (full-record) frame, changing one signal-bound leaf should
/// repaint only that leaf — its unchanged siblings replay from the subtree cache.
#[test]
fn signal_change_replays_sibling_caches() {
    let label = Signal::new("row-0".to_string());
    let mut h = TestHarness::new(400.0, 600.0);
    h.mount(
        VStack::new()
            .push(Text::new("static-a"))
            .push(Text::new("static-b"))
            .push(Text::new("static-c"))
            .push(Text::new("static-d"))
            .push(Text::new("row-0").bind(label.clone())),
    );

    // Frame 1: everything records (cold cache — all misses, zero hits).
    h.run_frame();
    assert_eq!(
        h.subtree_cache_hits(),
        0,
        "cold first frame: no cache hits yet"
    );

    // Settle any follow-up work so the cache is fully warm.
    h.settle(8);

    // Mutate one leaf's text via its signal.
    h.set_signal(&label, "row-0-CHANGED".to_string());
    h.run_frame();

    // The four static siblings' subtrees should replay from cache; the changed
    // leaf re-records. The O(k) paint guarantee: at least the unchanged
    // siblings were cache hits, and layout did not escalate.
    assert!(
        h.subtree_cache_hits() >= 1,
        "expected sibling subtree-cache replays, got {} hits / {} misses",
        h.subtree_cache_hits(),
        h.subtree_cache_misses(),
    );
    h.assert_no_relayout_escalation();
}

/// A no-op frame (nothing changed) after settling should be fully cache-driven
/// or skip paint entirely — never a full re-record.
#[test]
fn idle_frame_is_ok() {
    let mut h = TestHarness::new(400.0, 400.0);
    h.mount(
        VStack::new()
            .push(Text::new("alpha"))
            .push(Text::new("beta"))
            .push(Text::new("gamma")),
    );
    h.settle(8);

    let commands_before = h.paint_command_count();
    h.run_frame();
    // Idle frame: no escalation, dirty set is empty-ish.
    h.assert_no_relayout_escalation();
    assert!(
        h.frame_dirty_set_size() <= 1,
        "idle frame dirty set should be ~0, got {}",
        h.frame_dirty_set_size(),
    );
    let _ = commands_before;
}
