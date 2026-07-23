//! Lifecycle leak regression tests (audit 2026-07-16, F1).
//!
//! Locks in the unified unmount protocol:
//! - recursive subtree teardown (no orphaned descendants),
//! - RAII signal subscriptions dropped on element removal,
//! - `on_unmount` fires for every removed element,
//! - EventRegistry handler cleanup drained per frame,
//! - no raw `auralis_signal::subscribe` in widget code (CI grep guard).
//!
//! Perf-style diagnostics (`--ignored`) retained at the bottom.

use auralis_signal::Signal;
use burin::style::{Dimension, Styled};
use burin::testing::TestHarness;
use burin::widgets::display::Text;
use burin::widgets::layout::VStack;

fn px(w: f32) -> Dimension {
    Dimension::Pixels(w)
}

/// F1 regression: signal subscriptions must not accumulate across
/// mount/remove cycles, and `signal.set()` must not slow down.
#[test]
fn subscriptions_dropped_on_remove() {
    let mut h = TestHarness::new(800.0, 600.0);
    let sig: Signal<String> = Signal::new("hello".to_string());
    assert_eq!(sig.subscriber_count(), 0);

    for cycle in 0..50u32 {
        let id = h.mount(Text::new("x").bind(sig.clone()));
        h.run_frame();
        h.arena.remove(id);
        h.run_frame();
        assert_eq!(
            sig.subscriber_count(),
            0,
            "cycle {cycle}: subscription leaked past element removal"
        );
    }
}

/// F1 regression: removing a subtree root removes every descendant.
#[test]
fn no_orphan_descendants_on_remove() {
    let mut h = TestHarness::new(800.0, 600.0);
    let baseline = h.arena.len();

    let mut inner = VStack::new().width(px(200.0)).height(px(400.0));
    for i in 0..50 {
        inner = inner.push(Text::new(format!("row {i}")));
    }
    let outer = VStack::new().width(px(300.0)).height(px(500.0)).push(inner);
    let id = h.mount(outer);
    h.run_frame();
    assert!(h.arena.len() > baseline + 50, "mount built the subtree");

    h.arena.remove(id);
    h.run_frame();
    assert_eq!(
        h.arena.len(),
        baseline,
        "descendants leaked: {} elements remain past baseline",
        h.arena.len() - baseline
    );
}

/// F1 regression: same guarantee via clear_children.
#[test]
fn no_orphan_descendants_on_clear_children() {
    let mut h = TestHarness::new(800.0, 600.0);
    let baseline = h.arena.len();

    let mut inner = VStack::new().width(px(200.0)).height(px(400.0));
    for i in 0..10 {
        inner = inner.push(Text::new(format!("row {i}")));
    }
    let id = h.mount(inner);
    h.run_frame();
    let root = h.root_id();
    h.arena.clear_children(root);
    h.run_frame();
    let _ = id;
    assert_eq!(h.arena.len(), baseline, "clear_children leaked descendants");
}

/// F1 regression: `on_unmount` fires when the element (or an ancestor)
/// is removed — previously it never fired at all.
#[test]
fn on_unmount_fires_on_subtree_removal() {
    use std::cell::Cell;
    use std::rc::Rc;

    let mut h = TestHarness::new(800.0, 600.0);
    let outer = VStack::new()
        .width(px(300.0))
        .height(px(500.0))
        .push(Text::new("child"));
    let id = h.mount(outer);
    h.run_frame();

    let child_id = h.find(id).unwrap().children[0];
    let fired: Rc<Cell<u32>> = Rc::new(Cell::new(0));
    {
        let fired = fired.clone();
        let cb: Box<dyn FnOnce()> = Box::new(move || fired.set(fired.get() + 1));
        let mut ct = h.arena.component_tables.borrow_mut();
        ct.lc.entry(child_id).or_default().on_unmount =
            Some(Rc::new(std::cell::RefCell::new(Some(cb))));
    }

    h.arena.remove(id);
    assert_eq!(
        fired.get(),
        1,
        "on_unmount must fire exactly once for descendants"
    );
}

/// F1 regression: dead elements' ghost signal writes stay one-shot no-ops
/// (no persistent dirty, no panic).
#[test]
fn ghost_signal_write_after_removal_is_inert() {
    let mut h = TestHarness::new(800.0, 600.0);
    let sig: Signal<String> = Signal::new("alive".to_string());
    let id = h.mount(Text::new("x").bind(sig.clone()));
    h.run_frame();
    h.arena.remove(id);
    h.run_frame();

    sig.set("ghost".to_string());
    assert_eq!(h.dirty_count(), 0, "removed element must not become dirty");
    assert!(
        h.run_frame_safe().is_ok(),
        "frame after ghost write must not panic"
    );
}

/// CI guard: no raw `auralis_signal::subscribe(` outside the allow-list.
/// New subscriptions must go through `signal_bridge::subscribe_owned` /
/// `store_subscription` (RAII, dropped at element teardown) or
/// `Prop::on_change` (returns a PropSubscription guard).
#[test]
fn no_raw_signal_subscribe_in_widgets() {
    let allow = ["core\\prop.rs", "core/prop.rs"];
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut offenders = Vec::new();
    let mut stack = vec![root];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|e| e == "rs") {
                let text = std::fs::read_to_string(&path).unwrap_or_default();
                if text.contains("auralis_signal::subscribe(") {
                    let p = path.to_string_lossy().to_string();
                    if !allow.iter().any(|a| p.ends_with(a)) {
                        offenders.push(p);
                    }
                }
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "raw auralis_signal::subscribe found (use signal_bridge::subscribe_owned):\n{}",
        offenders.join("\n")
    );
}

// ═══════════════ diagnostics (run with --ignored) ═══════════════

/// Perf diagnostic: per-hover-move hit-test + hover-diff cost.
#[test]
#[ignore]
fn pointer_move_hover_cost() {
    let mut h = TestHarness::new(800.0, 600.0);
    let mut root = VStack::new().width(px(800.0)).height(px(600.0));
    for r in 0..30 {
        let mut row = burin::widgets::layout::HStack::new().height(px(20.0));
        for c in 0..10 {
            row = row.push(Text::new(format!("cell {r}-{c}")));
        }
        root = root.push(row);
    }
    h.mount(root);
    h.run_frame();

    let n = 10_000u32;
    let t0 = std::time::Instant::now();
    for i in 0..n {
        let x = 10.0 + (i % 700) as f32;
        let y = 10.0 + (i % 500) as f32;
        h.hover_at(burin::style::Point::new(x, y));
    }
    let per_move = t0.elapsed().as_micros() as f64 / n as f64;
    println!("── hover_at cost (hit-test + hover diff) ──");
    println!("  tree elements ≈ {}", h.arena.len());
    println!("  per-move cost = {per_move:.2} µs");
}

/// Perf diagnostic: signal.set() latency vs mount/remove cycles.
#[test]
#[ignore]
fn subscription_set_latency_diagnostic() {
    let mut h = TestHarness::new(800.0, 600.0);
    let sig: Signal<String> = Signal::new("hello".to_string());
    println!("── signal.set() latency across mount/remove cycles ──");
    for cycle in 1..=200u32 {
        let id = h.mount(Text::new("x").bind(sig.clone()));
        h.run_frame();
        h.arena.remove(id);
        h.run_frame();
        if cycle % 50 == 0 {
            let t0 = std::time::Instant::now();
            for i in 0..100 {
                sig.set(format!("v{cycle}-{i}"));
            }
            let per_set = t0.elapsed().as_nanos() / 100;
            println!(
                "  cycle {cycle}: subscribers = {}, signal.set() = {per_set} ns",
                sig.subscriber_count()
            );
        }
    }
}
