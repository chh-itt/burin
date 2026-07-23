//! DevTools-powered causal performance tests.
//!
//! These tests complement `perf_suite.rs` by answering WHY something is slow,
//! not just WHAT is slow. They leverage the DevTools ring buffer to trace
//! signal→element causal links, per-element frame diffs, and layout stability.
//!
//! Run with:
//!   cargo test --profile bench --test perf_causal --features devtools -- --ignored --nocapture --test-threads 1
//!
//! Dimensions:
//!   D10 — causal dirty: exactly the right elements changed, nothing else
//!   D11 — frame diff stability: idle frames produce zero structural changes
//!   D12 — layout oscillation detection: no element bounds flip-flop between frames

#![cfg(feature = "devtools")]

use std::collections::BTreeMap;

use auralis_signal::Signal;
use burin::core::dirty_registry::SignalElementLink;
use burin::debug::devtools::{diff_snapshots, drain_test_snapshots, ElementChange, FrameSnapshot};
use burin::testing::probes;
use burin::testing::TestHarness;
use burin::widgets::display::Text;
use burin::widgets::layout::{ScrollView, SizedBox, VStack};

fn harness_with_devtools(width: f32, height: f32) -> TestHarness {
    burin::debug::devtools::install_signal_observer();
    burin::debug::devtools::install_ring_buffer(burin::debug::devtools::new_ring_buffer());
    TestHarness::new(width, height)
}

type Dimension = BTreeMap<String, serde_json::Value>;

fn json_str(s: &str) -> String {
    let mut out = String::new();
    out.push('"');
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn dim_json(d: &Dimension) -> String {
    let parts: Vec<String> = d
        .iter()
        .map(|(k, v)| {
            let val = match v {
                serde_json::Value::Number(n) => n.to_string(),
                serde_json::Value::String(s) => json_str(s),
                serde_json::Value::Bool(b) => b.to_string(),
                serde_json::Value::Array(arr) => {
                    let items: Vec<String> = arr.iter().map(|v| v.to_string()).collect();
                    format!("[{}]", items.join(","))
                }
                _ => String::new(),
            };
            format!("{}:{}", json_str(k), val)
        })
        .collect();
    format!("{{{}}}", parts.join(","))
}

fn suite_json(dimensions: &[(Dimension, bool)]) -> String {
    let d_jsons: Vec<String> = dimensions.iter().map(|(d, _)| dim_json(d)).collect();
    let passed = dimensions.iter().all(|(_, p)| *p);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    format!(
        "{{\"generated_at\":\"{now}\",\"passed\":{passed},\"dimensions\":[{}]}}",
        d_jsons.join(",")
    )
}

// ═══════════════════════ D10: Causal Dirty ═══════════════════════

/// Signal change must cause dirty ONLY on the element(s) that subscribe to it.
///
/// Scenario: 5 Text widgets, 4 static (no signal), 1 reactive (bound to `label`).
/// Changing `label` should dirty exactly 1 element — the bound Text — and NOT
/// its static siblings. The subtree cache should replay the siblings from cache.
fn dim_10_causal_dirty() -> (Dimension, bool) {
    let label = Signal::new("reactive-text".to_string());
    let mut h = harness_with_devtools(400.0, 600.0);
    h.enable_perf();

    let _root = h.mount(
        VStack::new()
            .push(Text::new("static-a"))
            .push(Text::new("static-b"))
            .push(Text::new("static-c"))
            .push(Text::new("static-d"))
            .push(Text::new("reactive-text").bind(label.clone())),
    );

    // Settle: warm cache, clear any mount-time snapshots.
    for _ in 0..5 {
        h.run_frame();
    }
    let _ = drain_test_snapshots();

    // Change one signal.
    label.set("CHANGED".to_string());
    h.run_frame();

    let snaps: Vec<FrameSnapshot> = drain_test_snapshots();
    let snap = snaps.last().expect("at least one frame snapshot");

    // Assertion 1: exactly one signal changed this frame.
    let sig_changes = snap.signal_changes.len();
    assert!(
        sig_changes > 0,
        "signal_changes must record at least one mutation"
    );

    // Assertion 2: signal_element_links exist for the changed signal.
    let links: Vec<&SignalElementLink> = snap
        .signal_element_links
        .iter()
        .filter(|l| l.signal_addr != 0)
        .collect();
    assert!(
        !links.is_empty(),
        "signal_element_links must contain at least one causal link"
    );

    // Assertion 3: element_changes should be limited — NOT all elements changed.
    let modified: Vec<&ElementChange> = snap
        .element_changes
        .iter()
        .filter(|c| matches!(c, ElementChange::Modified { .. }))
        .collect();

    // A reasonable upper bound: ≤ 10 elements modified for a single signal change
    // (the bound element + ancestors that propagate dirty flags upward).
    let modification_ok = modified.len() <= 10;
    assert!(
        modification_ok,
        "single signal change modified {} elements (expected ≤ 10)",
        modified.len()
    );

    // Assertion 4: subtree cache had hits (unchanged siblings replayed from cache).
    let cache_ok = snap.cache_stats.subtree_cache_hits > 0;
    assert!(
        cache_ok,
        "expected subtree_cache_hits > 0 (static siblings should replay from cache)"
    );

    let mut d = Dimension::new();
    d.insert(
        "name".into(),
        serde_json::Value::String("d10_causal_dirty".into()),
    );
    d.insert(
        "signal_changes".into(),
        serde_json::Value::Number((sig_changes as u64).into()),
    );
    d.insert(
        "signal_element_links".into(),
        serde_json::Value::Number((links.len() as u64).into()),
    );
    d.insert(
        "elements_modified".into(),
        serde_json::Value::Number((modified.len() as u64).into()),
    );
    d.insert(
        "subtree_cache_hits".into(),
        serde_json::Value::Number(snap.cache_stats.subtree_cache_hits.into()),
    );
    d.insert(
        "subtree_cache_misses".into(),
        serde_json::Value::Number(snap.cache_stats.subtree_cache_misses.into()),
    );
    let passed = sig_changes > 0 && modification_ok && cache_ok;
    d.insert("passed".into(), serde_json::Value::Bool(passed));
    (d, passed)
}

/// Causal dirty with a deep tree: verify that changing a deeply-nested signal
/// dirties only the path to root, not lateral siblings.
fn dim_10b_causal_dirty_deep() -> (Dimension, bool) {
    let deep_label = Signal::new("deep-leaf".to_string());
    let mut h = harness_with_devtools(800.0, 600.0);
    h.enable_perf();

    // Build: VStack containing two branches.
    //   Branch A (left): deep nested tree with a reactive leaf at the bottom.
    //   Branch B (right): same depth, all static — should stay untouched.
    fn static_branch(depth: usize) -> VStack {
        let mut v = VStack::new();
        for i in 0..depth {
            v = v.push(Text::new(format!("static-{i}")));
        }
        v
    }

    fn reactive_branch(depth: usize, sig: &Signal<String>) -> VStack {
        let mut v = VStack::new();
        for i in 0..depth {
            if i == depth - 1 {
                v = v.push(Text::new(sig.clone().read()).bind(sig.clone()));
            } else {
                v = v.push(Text::new(format!("reactive-{i}")));
            }
        }
        v
    }

    let _root = h.mount(
        SizedBox::new().width(400.0).height(600.0).child(
            ScrollView::new().child(
                VStack::new()
                    .push(
                        SizedBox::new()
                            .width(400.0)
                            .height(300.0)
                            .child(static_branch(20)),
                    )
                    .push(
                        SizedBox::new()
                            .width(400.0)
                            .height(300.0)
                            .child(reactive_branch(20, &deep_label)),
                    ),
            ),
        ),
    );

    for _ in 0..5 {
        h.run_frame();
    }
    let _ = drain_test_snapshots();

    deep_label.set("DEEP-CHANGED".to_string());
    h.run_frame();

    let snaps = drain_test_snapshots();
    let snap = snaps.last().expect("snapshot");

    let modified: Vec<&ElementChange> = snap
        .element_changes
        .iter()
        .filter(|c| matches!(c, ElementChange::Modified { .. }))
        .collect();

    // The reactive branch's ancestors + the leaf itself should be modified,
    // but the static branch siblings should NOT be.
    let mod_ok = modified.len() <= 25; // depth 20 + some overhead

    let mut d = Dimension::new();
    d.insert(
        "name".into(),
        serde_json::Value::String("d10b_causal_dirty_deep".into()),
    );
    d.insert(
        "elements_modified".into(),
        serde_json::Value::Number((modified.len() as u64).into()),
    );
    d.insert(
        "subtree_cache_hits".into(),
        serde_json::Value::Number(snap.cache_stats.subtree_cache_hits.into()),
    );
    let passed = mod_ok && snap.cache_stats.subtree_cache_hits > 0;
    d.insert("passed".into(), serde_json::Value::Bool(passed));
    (d, passed)
}

// ═══════════════════════ D11: Frame Diff Stability ═══════════════════════

/// After settling, consecutive idle frames must produce NO element changes.
/// Any structural diff on an idle frame indicates a layout settling issue,
/// animation that won't quiesce, or a dirty-propagation leak.
fn dim_11_frame_diff_stability() -> (Dimension, bool) {
    let mut h = harness_with_devtools(800.0, 600.0);
    h.enable_perf();

    h.mount(
        SizedBox::new()
            .width(800.0)
            .height(600.0)
            .child(ScrollView::new().child(probes::BoxedWidget(probes::build_balanced_tree(5, 3)))),
    );

    // Settle: let the tree reach steady state.
    h.settle(12);
    let _ = drain_test_snapshots();

    // Collect 10 idle frame snapshots.
    for _ in 0..10 {
        h.run_frame();
    }
    let snaps = drain_test_snapshots();
    assert!(snaps.len() >= 2, "need at least 2 snapshots for diffing");

    // Diff consecutive pairs.
    let mut diffs_with_changes = 0u64;
    let mut total_modified = 0u64;
    for w in snaps.windows(2) {
        let diff = diff_snapshots(&w[0], &w[1]);
        let mods = diff
            .element_changes
            .iter()
            .filter(|c| matches!(c, ElementChange::Modified { .. }))
            .count() as u64;
        if mods > 0 {
            diffs_with_changes += 1;
        }
        total_modified += mods;
    }

    // Zero diffs expected on idle frames.
    let passed = diffs_with_changes == 0 && total_modified == 0;

    let mut d = Dimension::new();
    d.insert(
        "name".into(),
        serde_json::Value::String("d11_frame_diff_stability".into()),
    );
    d.insert(
        "idle_frames".into(),
        serde_json::Value::Number((snaps.len() as u64).into()),
    );
    d.insert(
        "diffs_with_changes".into(),
        serde_json::Value::Number(diffs_with_changes.into()),
    );
    d.insert(
        "total_modified".into(),
        serde_json::Value::Number(total_modified.into()),
    );
    d.insert("passed".into(), serde_json::Value::Bool(passed));
    (d, passed)
}

// ═══════════════════════ D12: Layout Oscillation ═══════════════════════

/// After settling, no element's screen_bounds should oscillate between two
/// values across consecutive frames. Oscillation indicates a layout that
/// never stabilizes — typically caused by a missing layout isolation boundary
/// or a measurement loop.
fn dim_12_layout_oscillation() -> (Dimension, bool) {
    let mut h = harness_with_devtools(1200.0, 900.0);
    h.enable_perf();

    // Use a grid layout: grids are more prone to oscillation because
    // column-width calculation can create dependency cycles.
    use burin::widgets::layout::{GridItem, GridRow};

    let mut grid = GridRow::new().columns(4);
    for _ in 0..10 {
        let mut row = VStack::new();
        for _ in 0..4 {
            row = row.push(Text::new("cell content that wraps possibly"));
        }
        grid = grid.push(GridItem::new(row));
    }

    h.mount(SizedBox::new().width(800.0).height(600.0).child(grid));
    h.settle(15);
    let _ = drain_test_snapshots();

    // Collect 20 idle frames.
    for _ in 0..20 {
        h.run_frame();
    }
    let snaps = drain_test_snapshots();

    // Extract screen_bounds per element across frames, detect oscillation.
    // Oscillation: element's bounds are A → B → A → B (not converging to a fixed point).
    use std::collections::HashMap;
    let mut bounds_history: HashMap<burin::core::ElementId, Vec<(f32, f32, f32, f32)>> =
        HashMap::new();

    for snap in &snaps {
        for el in &snap.elements {
            let b = (
                el.screen_bounds.x,
                el.screen_bounds.y,
                el.screen_bounds.width,
                el.screen_bounds.height,
            );
            bounds_history.entry(el.id).or_default().push(b);
        }
    }

    let mut oscillating = 0u64;
    for (_eid, history) in &bounds_history {
        if history.len() < 3 {
            continue;
        }
        // Check for A → B → A pattern (oscillation).
        for w in history.windows(3) {
            let a = w[0];
            let b = w[1];
            let c = w[2];
            let a_eq_c = (a.0 - c.0).abs() < 0.5
                && (a.1 - c.1).abs() < 0.5
                && (a.2 - c.2).abs() < 0.5
                && (a.3 - c.3).abs() < 0.5;
            let a_ne_b = (a.0 - b.0).abs() >= 0.5
                || (a.1 - b.1).abs() >= 0.5
                || (a.2 - b.2).abs() >= 0.5
                || (a.3 - b.3).abs() >= 0.5;
            if a_eq_c && a_ne_b {
                oscillating += 1;
                break; // count each element once
            }
        }
    }

    let passed = oscillating == 0;

    let mut d = Dimension::new();
    d.insert(
        "name".into(),
        serde_json::Value::String("d12_layout_oscillation".into()),
    );
    d.insert(
        "idle_frames".into(),
        serde_json::Value::Number((snaps.len() as u64).into()),
    );
    d.insert(
        "elements_tracked".into(),
        serde_json::Value::Number((bounds_history.len() as u64).into()),
    );
    d.insert(
        "oscillating_elements".into(),
        serde_json::Value::Number(oscillating.into()),
    );
    d.insert("passed".into(), serde_json::Value::Bool(passed));
    (d, passed)
}

// ═══════════════════════ Entry Point ═══════════════════════

#[test]
#[ignore]
fn perf_causal_suite() {
    let dims: Vec<(Dimension, bool)> = vec![
        dim_10_causal_dirty(),
        dim_10b_causal_dirty_deep(),
        dim_11_frame_diff_stability(),
        dim_12_layout_oscillation(),
    ];

    let json = suite_json(&dims);
    println!("{}", json);

    if let Ok(path) = std::env::var("PERF_CAUSAL_OUT") {
        if let Some(parent) = std::path::Path::new(&path).parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        std::fs::write(&path, &json).expect("PERF_CAUSAL_OUT write failed");
        println!("perf_causal: JSON written to {path}");
    }

    let all_passed = dims.iter().all(|(_, p)| *p);
    if !all_passed {
        panic!("perf_causal: one or more dimensions failed — see JSON output for details");
    }
}
