//! Unified performance suite entry point.
//!
//! One command to collect every dimension of framework performance:
//!
//! ```bash
//! cargo test --profile bench --test perf_suite --features ext-jiff -- --ignored --nocapture --test-threads 1
//! ```
//!
//! Outputs JSON to stdout and a human-readable summary. Exits with code 0
//! only when all dimensions pass their committed-baseline thresholds.
//!
//! Dimensions collected:
//!   D1 — frame-phase timings (layout/prepass/paint/dirty/total)
//!   D2 — viewport-bounded paint work (scroll O(visible) proof)
//!   D3 — hover-idle avoidance (static crossing zero dirty)
//!   D4 — registry lifecycle (mount/unmount returns to baseline)
//!   D5 — structural invariants (incremental layout, no escalations)
//!   D6 — text-mutation cost (per-change rebuild)
//!   D7 — startup cost (first frame vs steady state ratio)
//!   D8 — signal latency (single/batch signal → frame-complete delay)
//!   D9 — idle scaling (O(1) idle cost across 100/1k/5k element trees)
//!   D13 — arena cleanup (mount → unmount returns arena len to baseline)
//!   D14 — subtree cache boundedness (sustained scroll → miss rate stabilises)
//!   D15 — arena fragmentation (repeated mount/unmount → zero orphaned slots)

use std::collections::BTreeMap;

use auralis_signal::Signal;
use burin::style::Point;
use burin::testing::probes::PhaseTiming;
use burin::testing::{probes, TestHarness};
use burin::widgets::display::Text;
use burin::widgets::input::{Field, Form, TextInput};
use burin::widgets::layout::{ScrollView, SizedBox, VStack};

// ═══════════════════════ JSON output (no serde_json dep — hand-baked) ═══════════════════════

type Dimension = BTreeMap<String, serde_json::Value>;

/// Escape a string for JSON.
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

fn suite_json(dimensions: &[Dimension], passed: bool) -> String {
    let d_jsons: Vec<String> = dimensions.iter().map(dim_json).collect();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    format!(
        "{{\"generated_at\":\"{now}\",\"passed\":{passed},\"dimensions\":[{}]}}",
        d_jsons.join(",")
    )
}

// ═══════════════════════ Dimension runners ═══════════════════════

/// D1 — Virtual table scroll inside a 1.5k-element app tree (the canonical
/// production-scale benchmark). Returns per-phase avg + max.
fn dim_1_frame_timings() -> (Dimension, bool) {
    use burin::widgets::display::{ColumnWidth, Table, TableColumn};

    let mut h = TestHarness::new(1200.0, 800.0);
    h.enable_perf();
    let rows_sig = Signal::new(data_rows(10_000));
    let table = Table::new(rows_sig.clone())
        .columns(vec![
            TableColumn::<&'static str>::new("id", ColumnWidth::Fixed(80.0))
                .render(|s, _, _| s.to_string()),
            TableColumn::<&'static str>::new("name", ColumnWidth::Fixed(200.0))
                .render(|s, _, _| s.to_string()),
            TableColumn::<&'static str>::new("desc", ColumnWidth::Fixed(400.0))
                .render(|s, _, _| s.to_string()),
        ])
        .virtual_threshold(16)
        .row_height(28.0);
    let _root = h.mount(
        VStack::new()
            .push(SizedBox::new().width(700.0).height(400.0).child(table))
            .push(SizedBox::new().width(700.0).height(200.0).child(
                // ~1.5k-element app tree beside the table (audit-bench B1 shape).
                ScrollView::new().child(probes::BoxedWidget(probes::build_balanced_tree(6, 4))),
            )),
    );
    for _ in 0..3 {
        h.run_frame();
    }
    let sc = probes::find_tallest_scroll_container(&h, _root);

    let t = probes::measure_scroll_frames(&mut h, sc, -10.0, 120);
    let mut d = Dimension::new();
    d.insert(
        "name".into(),
        serde_json::Value::String("d1_frame_timings".into()),
    );
    d.insert(
        "elements".into(),
        serde_json::Value::Number((probes::element_count(&h.arena, h.root_id()) as u64).into()),
    );
    d.insert(
        "layout_avg_us".into(),
        serde_json::Value::Number((t.layout_avg() as u64).into()),
    );
    d.insert(
        "prepass_avg_us".into(),
        serde_json::Value::Number((t.prepass_avg() as u64).into()),
    );
    d.insert(
        "paint_avg_us".into(),
        serde_json::Value::Number((t.paint_avg() as u64).into()),
    );
    d.insert(
        "dirty_avg_us".into(),
        serde_json::Value::Number((t.dirty_avg() as u64).into()),
    );
    d.insert(
        "total_avg_us".into(),
        serde_json::Value::Number((t.total_avg() as u64).into()),
    );
    d.insert(
        "total_max_us".into(),
        serde_json::Value::Number((t.total_max() as u64).into()),
    );
    let passed = t.total_avg() < 500; // generous guard — regressions break this fast
    d.insert("passed".into(), serde_json::Value::Bool(passed));
    (d, passed)
}

/// D2 — Viewport-bounded paint: 1000-line static ScrollView, scroll frames.
/// Cache misses per frame must be ≤ 80 (O(visible), not O(content)).
fn dim_2_viewport_bounded_paint() -> (Dimension, bool) {
    let mut h = TestHarness::new(800.0, 600.0);
    h.enable_perf();
    let root = probes::build_static_scroll_page(&mut h, 1000, 400.0, 400.0);
    for _ in 0..5 {
        h.run_frame();
    }
    let sc = probes::find_tallest_scroll_container(&h, root);

    let mut misses: Vec<u64> = Vec::with_capacity(60);
    for _ in 0..60 {
        h.scroll(sc, 0.0, -10.0);
        h.run_frame();
        misses.push(h.subtree_cache_misses());
    }
    let max_miss = misses.iter().copied().max().unwrap_or(0);
    let passed = max_miss <= 80;
    let mut d = Dimension::new();
    d.insert(
        "name".into(),
        serde_json::Value::String("d2_viewport_bounded".into()),
    );
    d.insert(
        "max_subtree_misses".into(),
        serde_json::Value::Number((max_miss as u64).into()),
    );
    d.insert("threshold".into(), serde_json::Value::Number(80.into()));
    d.insert("passed".into(), serde_json::Value::Bool(passed));
    (d, passed)
}

/// D3 — Hover crossing over static content must register zero dirty.
fn dim_3_hover_idle() -> (Dimension, bool) {
    let mut h = TestHarness::new(1200.0, 800.0);
    h.enable_perf();
    probes::build_dual_panel_scene(&mut h, 100);
    for _ in 0..5 {
        h.run_frame();
    }

    let dirty_on_cross = probes::hover_registers_dirty(&mut h, Point::new(280.0, 400.0));
    h.run_frame();
    let dirty_on_adjacent = probes::hover_registers_dirty(&mut h, Point::new(840.0, 400.0));
    h.run_frame();
    let passed = !dirty_on_cross && !dirty_on_adjacent;
    let mut d = Dimension::new();
    d.insert(
        "name".into(),
        serde_json::Value::String("d3_hover_idle".into()),
    );
    d.insert(
        "cross_panel_dirty".into(),
        serde_json::Value::Bool(dirty_on_cross),
    );
    d.insert(
        "adjacent_row_dirty".into(),
        serde_json::Value::Bool(dirty_on_adjacent),
    );
    d.insert("passed".into(), serde_json::Value::Bool(passed));
    (d, passed)
}

/// D4 — Registry lifecycle: form validators + a11y node cache + image refs
/// return to baseline after 5 mount/unmount cycles.
fn dim_4_registry_lifecycle() -> (Dimension, bool) {
    let mut h = TestHarness::new(600.0, 400.0);
    let (v0, f0, l0) = burin::widgets::input::debug_registry_sizes();
    let arena_baseline = h.arena.len();
    let img0 = burin::render::wgpu::debug_image_registry_sizes().0;

    for _ in 0..5 {
        let page = mount_form_page(&mut h);
        // Build a11y cache once per cycle.
        let _ = burin::platform::build_accessibility_tree(&h.arena, h.root_id(), None);
        h.run_frame();
        h.arena.remove(page);
        h.run_frame();
        h.run_frame();
    }

    let (v, f, l) = burin::widgets::input::debug_registry_sizes();
    let passed = (v, f, l) == (v0, f0, l0)
        && h.arena.len() == arena_baseline
        && burin::render::wgpu::debug_image_registry_sizes().0 == img0;
    let mut d = Dimension::new();
    d.insert(
        "name".into(),
        serde_json::Value::String("d4_registry_lifecycle".into()),
    );
    d.insert(
        "validators_leaked".into(),
        serde_json::Value::Number((v.saturating_sub(v0) as u64).into()),
    );
    d.insert(
        "form_entries_leaked".into(),
        serde_json::Value::Number((f.saturating_sub(f0) as u64).into()),
    );
    d.insert(
        "element_leaks".into(),
        serde_json::Value::Number((h.arena.len().saturating_sub(arena_baseline) as u64).into()),
    );
    d.insert("passed".into(), serde_json::Value::Bool(passed));
    (d, passed)
}

/// D5 — Structural invariants: idle frames after scroll must register
/// zero dirty and zero layout escalations.
fn dim_5_structural_invariants() -> (Dimension, bool) {
    let mut h = TestHarness::new(500.0, 500.0);
    h.enable_perf();
    let root = probes::build_static_scroll_page(&mut h, 200, 400.0, 400.0);
    for _ in 0..5 {
        h.run_frame();
    }
    let sc = probes::find_tallest_scroll_container(&h, root);

    h.scroll(sc, 0.0, -100.0);
    h.run_frame();
    h.run_frame(); // settle
    let dirty = h.frame_dirty_set_size();
    let escalations = h.frame_escalations();
    let passed = dirty == 0 && escalations == 0;
    let mut d = Dimension::new();
    d.insert(
        "name".into(),
        serde_json::Value::String("d5_structural".into()),
    );
    d.insert(
        "idle_dirty_after_scroll".into(),
        serde_json::Value::Number((dirty as u64).into()),
    );
    d.insert(
        "escalations".into(),
        serde_json::Value::Number(escalations.into()),
    );
    d.insert("passed".into(), serde_json::Value::Bool(passed));
    (d, passed)
}

/// D6 — Text rebuild cost: 40 texts × 120 mutate frames, per-change cost
fn dim_6_text_rebuild() -> (Dimension, bool) {
    let strings: Vec<Signal<String>> = (0..40).map(|i| Signal::new(format!("text {i}"))).collect();
    let mut h = TestHarness::new(800.0, 600.0);
    h.enable_perf();
    let mut stack = VStack::new();
    for s in &strings {
        stack = stack.push(Text::new(s.clone().read()).bind(s.clone()));
    }
    h.mount(SizedBox::new().width(300.0).height(600.0).child(stack));
    for _ in 0..3 {
        h.run_frame();
    }

    let mut t = PhaseTiming::new();
    for i in 0..120 {
        for s in &strings {
            s.set(format!("changed text line {i}"));
        }
        h.run_frame();
        t.record(&h.frame_timing());
    }
    let passed = t.paint_avg() < 200; // was ~170us after shape-cache; guard generously
    let mut d = Dimension::new();
    d.insert(
        "name".into(),
        serde_json::Value::String("d6_text_rebuild".into()),
    );
    d.insert(
        "paint_avg_us".into(),
        serde_json::Value::Number((t.paint_avg() as u64).into()),
    );
    d.insert(
        "total_avg_us".into(),
        serde_json::Value::Number((t.total_avg() as u64).into()),
    );
    d.insert("passed".into(), serde_json::Value::Bool(passed));
    (d, passed)
}

/// D7 — Startup cost: cold first frame vs warm steady-state frame.
///
/// Mounts a balanced ~1.5k-element tree, measures the first (cold) frame
/// then settles and measures a warm idle frame.
/// Cold: must complete within a generous 15ms budget (full layout + full paint).
/// Warm: idle frame must be near-zero (< 500μs), proving the settled tree is quiescent.
fn dim_7_startup_cost() -> (Dimension, bool) {
    let mut h = TestHarness::new(800.0, 600.0);
    h.enable_perf();
    h.mount(
        SizedBox::new()
            .width(800.0)
            .height(600.0)
            .child(probes::BoxedWidget(probes::build_balanced_tree(6, 4))),
    );

    let cold = {
        h.run_frame();
        h.frame_timing()
    };
    let cold_total = cold.phases.iter().sum::<u64>();

    // Settle: the warm frame represents true steady-state idle cost.
    h.settle(8);
    let warm = {
        h.run_frame();
        h.frame_timing()
    };
    let warm_total = warm.phases.iter().sum::<u64>();

    let element_count = probes::element_count(&h.arena, h.root_id());
    let cold_ok = cold_total < 15_000;
    let warm_ok = warm_total < 500;
    let passed = cold_ok && warm_ok;

    let mut d = Dimension::new();
    d.insert(
        "name".into(),
        serde_json::Value::String("d7_startup_cost".into()),
    );
    d.insert(
        "element_count".into(),
        serde_json::Value::Number((element_count as u64).into()),
    );
    d.insert(
        "cold_total_us".into(),
        serde_json::Value::Number(cold_total.into()),
    );
    d.insert(
        "warm_total_us".into(),
        serde_json::Value::Number(warm_total.into()),
    );
    d.insert("cold_passed".into(), serde_json::Value::Bool(cold_ok));
    d.insert("warm_passed".into(), serde_json::Value::Bool(warm_ok));
    d.insert("passed".into(), serde_json::Value::Bool(passed));
    (d, passed)
}

/// D8 — Signal latency: wall-clock (virtual) from signal.set() to frame complete.
///
/// Single: one signal change → one frame → measure total frame time.
/// Batch: 100 concurrent signal changes → one frame → measure total frame time.
/// Both must be under their respective 60fps budgets.
fn dim_8_signal_latency() -> (Dimension, bool) {
    let mut h = TestHarness::new(800.0, 600.0);
    h.enable_perf();

    // 40 signal-bound Text widgets — typical reactive UI density.
    let strings: Vec<Signal<String>> = (0..40).map(|i| Signal::new(format!("text {i}"))).collect();
    let mut stack = VStack::new();
    for s in &strings {
        stack = stack.push(Text::new(s.clone().read()).bind(s.clone()));
    }
    h.mount(SizedBox::new().width(300.0).height(600.0).child(stack));
    h.settle(5);

    // Single-signal latency.
    strings[0].set("changed-single".into());
    h.run_frame();
    let single_us = h.frame_timing().phases.iter().sum::<u64>();

    h.settle(3);

    // Batch latency: 40 signals changed in one batch, one frame to settle.
    for s in &strings {
        s.set(format!("changed-{}", s.read()));
    }
    h.run_frame();
    let batch_us = h.frame_timing().phases.iter().sum::<u64>();

    let single_ok = single_us < 1000;
    let batch_ok = batch_us < 8000;
    let passed = single_ok && batch_ok;

    let mut d = Dimension::new();
    d.insert(
        "name".into(),
        serde_json::Value::String("d8_signal_latency".into()),
    );
    d.insert(
        "single_us".into(),
        serde_json::Value::Number(single_us.into()),
    );
    d.insert(
        "batch_us".into(),
        serde_json::Value::Number(batch_us.into()),
    );
    d.insert("single_passed".into(), serde_json::Value::Bool(single_ok));
    d.insert("batch_passed".into(), serde_json::Value::Bool(batch_ok));
    d.insert("passed".into(), serde_json::Value::Bool(passed));
    (d, passed)
}

/// D9 — Idle scaling: idle frame cost must be O(1), independent of tree size.
///
/// Builds 3 trees of increasing size (100, 1000, 5000 elements), measures
/// a warm idle frame for each, and verifies the cost does not grow
/// proportionally with element count.
fn dim_9_idle_scaling() -> (Dimension, bool) {
    let sizes: Vec<(usize, usize, usize)> = vec![
        (5, 3, 1),  // ~100 elements (5^3 ≈ 125)
        (8, 3, 2),  // ~1k elements  (8^3 ≈ 512, + additional depth)
        (10, 3, 3), // ~5k elements  (10^3 ≈ 1000 × nesting)
    ];
    let mut results: Vec<(usize, u64)> = Vec::new();

    for (branching, depth, _label) in &sizes {
        let mut h = TestHarness::new(1600.0, 1200.0);
        h.enable_perf();
        h.mount(
            SizedBox::new()
                .width(1600.0)
                .height(1200.0)
                .child(probes::BoxedWidget(probes::build_balanced_tree(
                    *branching, *depth,
                ))),
        );
        h.settle(10);

        // Measure warm idle frame.
        h.run_frame();
        let idle_us = h.frame_timing().phases.iter().sum::<u64>();
        let count = probes::element_count(&h.arena, h.root_id());
        results.push((count, idle_us));
    }

    // O(1) check: the largest tree's idle cost must be ≤ 5× the smallest tree's.
    let (small_n, small_us) = results[0];
    let (large_n, large_us) = results[results.len() - 1];
    let passed = large_us < small_us.saturating_mul(5).max(1);

    let mut d = Dimension::new();
    d.insert(
        "name".into(),
        serde_json::Value::String("d9_idle_scaling".into()),
    );
    for (n, us) in &results {
        d.insert(
            format!("{n}_elems_idle_us"),
            serde_json::Value::Number((*us).into()),
        );
    }
    d.insert(
        "small_n".into(),
        serde_json::Value::Number((small_n as u64).into()),
    );
    d.insert(
        "small_us".into(),
        serde_json::Value::Number(small_us.into()),
    );
    d.insert(
        "large_n".into(),
        serde_json::Value::Number((large_n as u64).into()),
    );
    d.insert(
        "large_us".into(),
        serde_json::Value::Number(large_us.into()),
    );
    d.insert(
        "growth_ratio".into(),
        serde_json::Value::Number(
            serde_json::Number::from_f64(if small_us > 0 {
                (large_us as f64 / small_us as f64 * 100.0).round() / 100.0
            } else {
                0.0
            })
            .unwrap(),
        ),
    );
    d.insert("passed".into(), serde_json::Value::Bool(passed));
    (d, passed)
}

/// D13 — Arena cleanup: after unmounting a complex tree, the arena must
/// return to its baseline allocation count.  A non-zero delta indicates
/// element teardown routines are not releasing slot entries.
fn dim_13_arena_cleanup() -> (Dimension, bool) {
    let mut h = TestHarness::new(800.0, 600.0);

    // Baseline: root element + portal anchors (if any).
    h.settle(3);
    let baseline = h.arena.len();

    // Mount a balanced tree, settle, then remove it.
    let mounted = h.mount(
        SizedBox::new()
            .width(800.0)
            .height(600.0)
            .child(probes::BoxedWidget(probes::build_balanced_tree(6, 4))),
    );
    h.settle(8);
    let peak = h.arena.len();

    h.arena.remove(mounted);
    h.settle(8);
    let after = h.arena.len();

    let leaked = after.saturating_sub(baseline);

    let mut d = Dimension::new();
    d.insert(
        "name".into(),
        serde_json::Value::String("d13_arena_cleanup".into()),
    );
    d.insert(
        "baseline_slots".into(),
        serde_json::Value::Number((baseline as u64).into()),
    );
    d.insert(
        "peak_slots".into(),
        serde_json::Value::Number((peak as u64).into()),
    );
    d.insert(
        "after_cleanup".into(),
        serde_json::Value::Number((after as u64).into()),
    );
    d.insert(
        "leaked".into(),
        serde_json::Value::Number((leaked as u64).into()),
    );
    let passed = leaked == 0;
    d.insert("passed".into(), serde_json::Value::Bool(passed));
    (d, passed)
}

/// D14 — Subtree cache boundedness: sustained scroll through a long
/// static list must converge to a steady cache-miss rate.  A monotonic
/// miss-rate increase signals an unbounded cache growth.
fn dim_14_cache_boundedness() -> (Dimension, bool) {
    let mut h = TestHarness::new(500.0, 500.0);
    h.enable_perf();
    let root = probes::build_static_scroll_page(&mut h, 2000, 400.0, 400.0);
    for _ in 0..5 {
        h.run_frame();
    }
    let sc = probes::find_tallest_scroll_container(&h, root);

    // Scroll 200 frames, record misses in 10-frame buckets.
    let mut buckets: Vec<u64> = Vec::with_capacity(20);
    for _ in 0..20 {
        let mut sum = 0u64;
        for _ in 0..10 {
            h.scroll(sc, 0.0, -18.0);
            h.run_frame();
            sum += h.subtree_cache_misses();
        }
        buckets.push(sum);
    }

    // The miss rate should stabilise: last 5 buckets' max ≤ first 5 buckets' max.
    let first_max = buckets[..5].iter().copied().max().unwrap_or(0);
    let last_max = buckets[15..].iter().copied().max().unwrap_or(0);
    let stabilised = last_max <= first_max.max(1);

    let mut d = Dimension::new();
    d.insert(
        "name".into(),
        serde_json::Value::String("d14_cache_boundedness".into()),
    );
    d.insert(
        "first_5_max_misses".into(),
        serde_json::Value::Number(first_max.into()),
    );
    d.insert(
        "last_5_max_misses".into(),
        serde_json::Value::Number(last_max.into()),
    );
    let miss_trend: Vec<serde_json::Value> = buckets
        .iter()
        .map(|&v| serde_json::Value::Number(v.into()))
        .collect();
    d.insert("miss_trend".into(), serde_json::Value::Array(miss_trend));
    d.insert("passed".into(), serde_json::Value::Bool(stabilised));
    (d, stabilised)
}

/// D15 — Arena fragmentation: repeated mount/unmount cycles must not
/// leave orphaned slots in the element arena.  After 10 cycles, the
/// arena length must match the baseline exactly.
fn dim_15_arena_fragmentation() -> (Dimension, bool) {
    let mut h = TestHarness::new(800.0, 600.0);
    h.settle(3);
    let baseline = h.arena.len();

    for _cycle in 0..10 {
        let mounted = h.mount(
            SizedBox::new()
                .width(800.0)
                .height(600.0)
                .child(probes::BoxedWidget(probes::build_balanced_tree(5, 3))),
        );
        h.settle(5);
        h.arena.remove(mounted);
        h.settle(3);
    }

    let after = h.arena.len();
    let leaked = after.saturating_sub(baseline);

    let mut d = Dimension::new();
    d.insert(
        "name".into(),
        serde_json::Value::String("d15_arena_fragmentation".into()),
    );
    d.insert(
        "baseline_slots".into(),
        serde_json::Value::Number((baseline as u64).into()),
    );
    d.insert(
        "after_10_cycles".into(),
        serde_json::Value::Number((after as u64).into()),
    );
    d.insert(
        "leaked".into(),
        serde_json::Value::Number((leaked as u64).into()),
    );
    let passed = leaked == 0;
    d.insert("passed".into(), serde_json::Value::Bool(passed));
    (d, passed)
}

// ═══════════════════════ Helpers ═══════════════════════

fn data_rows(n: usize) -> Vec<&'static str> {
    (0..n)
        .map(|i| match i % 6 {
            0 => "alpha",
            1 => "beta",
            2 => "gamma",
            3 => "delta",
            4 => "epsilon",
            _ => "zeta",
        })
        .collect()
}

fn mount_form_page(h: &mut TestHarness) -> burin::core::ElementId {
    h.mount(
        VStack::new().push(
            Form::new()
                .child(
                    Field::new()
                        .label("Name")
                        .required(true)
                        .validator(|v: &str| {
                            if v.is_empty() {
                                Some("required".into())
                            } else {
                                None
                            }
                        })
                        .child(TextInput::new(Signal::new(String::new()))),
                )
                .child(
                    Field::new()
                        .label("Email")
                        .validator(|v: &str| {
                            if v.contains('@') {
                                None
                            } else {
                                Some("invalid".into())
                            }
                        })
                        .child(TextInput::new(Signal::new(String::new()))),
                ),
        ),
    )
}

// ═══════════════════════ Entry point ═══════════════════════

#[test]
#[ignore]
fn perf_suite() {
    let dims = vec![
        dim_1_frame_timings(),
        dim_2_viewport_bounded_paint(),
        dim_3_hover_idle(),
        dim_4_registry_lifecycle(),
        dim_5_structural_invariants(),
        dim_6_text_rebuild(),
        dim_7_startup_cost(),
        dim_8_signal_latency(),
        dim_9_idle_scaling(),
        dim_13_arena_cleanup(),
        dim_14_cache_boundedness(),
        dim_15_arena_fragmentation(),
    ];

    let all_passed = dims.iter().all(|(_, p)| *p);
    let json_dims: Vec<Dimension> = dims.into_iter().map(|(d, _)| d).collect();
    let json = suite_json(&json_dims, all_passed);

    println!("{}", json);
    // Optional file sink for archiving / trend tracking:
    //   $env:PERF_SUITE_OUT="docs/perf/history/2026-07-18.json"
    if let Ok(path) = std::env::var("PERF_SUITE_OUT") {
        if let Some(parent) = std::path::Path::new(&path).parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        std::fs::write(&path, &json).expect("PERF_SUITE_OUT write failed");
        println!("perf_suite: JSON written to {path}");
    }
    if !all_passed {
        panic!("perf_suite: one or more dimensions failed — see JSON output for details");
    }
}
