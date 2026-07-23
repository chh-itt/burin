//! Audit benchmarks (2026-07-15 architecture audit).
//!
//! Verifies suspected hot-path costs with measured data:
//!   B1: virtual Table scroll frames — does each scroll frame force a full taffy pass?
//!   B2: same table without surrounding app tree (isolates the full-pass cost).
//!   B3: Calendar frame_tick idle cost (unconditional create_buffer per cell per frame).
//!   B4: snapshot_element_bounds O(N) term — incremental layout time vs total tree size.
//!
//! Run with:
//!   cargo test --profile bench --test audit_bench -- --ignored --nocapture

use auralis_signal::Signal;
use burin::core::context::MountContext;
use burin::core::element::ElementId;
use burin::core::perf::PerfPhase;
use burin::core::widget::Widget;
use burin::style::{Dimension, Styled};
use burin::testing::TestHarness;
use burin::widgets::display::{ColumnWidth, Table, TableColumn, Text};
#[cfg(feature = "ext-jiff")]
use burin::widgets::input::DatePicker;
use burin::widgets::layout::{HStack, ScrollView, SizedBox, VStack};

struct BoxedWidget(Box<dyn Widget>);
impl Widget for BoxedWidget {
    fn mount_box(self: Box<Self>, ctx: &mut MountContext<'_>) -> ElementId {
        self.0.mount_box(ctx)
    }
}
fn bw(w: Box<dyn Widget>) -> BoxedWidget {
    BoxedWidget(w)
}
fn px(w: f32) -> Dimension {
    Dimension::Pixels(w)
}

fn build_balanced(branching: usize, depth: usize) -> Box<dyn Widget> {
    if depth == 0 {
        return Box::new(Text::new("leaf"));
    }
    let use_v = depth % 2 == 0;
    if use_v {
        let mut s = VStack::new().width(px(300.0)).height(px(600.0));
        for _ in 0..branching {
            s = s.push(bw(build_balanced(branching, depth - 1)));
        }
        Box::new(s)
    } else {
        let mut s = HStack::new().width(px(300.0)).height(px(600.0));
        for _ in 0..branching {
            s = s.push(bw(build_balanced(branching, depth - 1)));
        }
        Box::new(s)
    }
}

fn element_count(h: &TestHarness) -> usize {
    fn walk(h: &TestHarness, id: ElementId, n: &mut usize) {
        *n += 1;
        if let Some(el) = h.find(id) {
            for &c in &el.children.clone() {
                walk(h, c, n);
            }
        }
    }
    let mut n = 0;
    walk(h, h.root_id(), &mut n);
    n
}

fn cols3() -> Vec<TableColumn<String>> {
    vec![
        TableColumn::new("A", ColumnWidth::Fixed(120.0)).render(|r: &String, _, _| r.clone()),
        TableColumn::new("B", ColumnWidth::Fixed(80.0)).render(|_: &String, ri, _| format!("{ri}")),
        TableColumn::new("C", ColumnWidth::Fixed(100.0))
            .render(|_: &String, ri, _| format!("c{ri}")),
    ]
}

struct PhaseAvg {
    layout: Vec<u64>,
    prepass: Vec<u64>,
    deferred: Vec<u64>,
    paint: Vec<u64>,
    dirty: Vec<u64>,
    total: Vec<u64>,
}
impl PhaseAvg {
    fn new() -> Self {
        Self {
            layout: vec![],
            prepass: vec![],
            deferred: vec![],
            paint: vec![],
            dirty: vec![],
            total: vec![],
        }
    }
    fn record(&mut self, h: &TestHarness) {
        let t = h.frame_timing();
        self.layout.push(t.phases[PerfPhase::Layout as usize]);
        self.prepass.push(t.phases[PerfPhase::Prepass as usize]);
        self.deferred
            .push(t.phases[PerfPhase::DeferredActions as usize]);
        self.paint.push(t.phases[PerfPhase::Paint as usize]);
        self.dirty.push(t.phases[PerfPhase::ProcessDirty as usize]);
        self.total.push(t.phases.iter().sum());
    }
    fn print(&self, name: &str) {
        fn stats(v: &[u64]) -> (u64, u64) {
            if v.is_empty() {
                return (0, 0);
            }
            (
                v.iter().sum::<u64>() / v.len() as u64,
                *v.iter().max().unwrap(),
            )
        }
        let (l_avg, l_max) = stats(&self.layout);
        let (pp_avg, pp_max) = stats(&self.prepass);
        let (d_avg, d_max) = stats(&self.deferred);
        let (p_avg, p_max) = stats(&self.paint);
        let (di_avg, di_max) = stats(&self.dirty);
        let (t_avg, t_max) = stats(&self.total);
        println!("── {name} ({} frames) ──", self.total.len());
        println!("  layout   avg {l_avg:>6}us  max {l_max:>6}us");
        println!("  prepass  avg {pp_avg:>6}us  max {pp_max:>6}us");
        println!("  deferred avg {d_avg:>6}us  max {d_max:>6}us");
        println!("  dirty    avg {di_avg:>6}us  max {di_max:>6}us");
        println!("  paint    avg {p_avg:>6}us  max {p_max:>6}us");
        println!("  TOTAL    avg {t_avg:>6}us  max {t_max:>6}us");
    }
}

fn bench_table_scroll(with_app_tree: bool, label: &str) {
    let rows = Signal::new((0..10_000).map(|i| format!("Row {i}")).collect::<Vec<_>>());
    let mut h = TestHarness::new(1600.0, 1200.0);
    h.enable_perf();

    let table = Table::new(rows)
        .columns(cols3())
        .row_height(28.0)
        .virtual_threshold(20);
    let table_box = SizedBox::new().width(700.0).height(400.0).child(table);

    let mounted = if with_app_tree {
        h.mount(HStack::new().push(bw(build_balanced(6, 4))).push(table_box))
    } else {
        h.mount(table_box)
    };
    // Settle mount.
    for _ in 0..5 {
        h.run_frame();
    }
    let n = element_count(&h);

    // Find the table's scroll container: the element whose content_bounds is
    // the tallest (the ScrollBundle container synced from VirtualContentBounds).
    let target = {
        let mut best: Option<(ElementId, f32)> = None;
        let mut stack = vec![mounted];
        while let Some(id) = stack.pop() {
            if let Some(sc) = h.root().comp_scroll(id) {
                let cb_h = sc.content_bounds.get().height;
                if best.map_or(true, |(_, bh)| cb_h > bh) {
                    best = Some((id, cb_h));
                }
            }
            if let Some(el) = h.find(id) {
                for &c in &el.children {
                    stack.push(c);
                }
            }
        }
        best.expect("no scroll container found under table").0
    };

    let incr_before = h.incremental_taken();
    let mut acc = PhaseAvg::new();
    for _ in 0..60 {
        h.scroll(target, 0.0, -28.0);
        h.run_frame(); // scroll dirty → repaint; tick sees new offset
        acc.record(&h);
        h.run_frame(); // deferred remap → structural → layout
        acc.record(&h);
    }
    let incr_after = h.incremental_taken();
    let final_offset = h.root().comp_scroll(target).map(|s| s.scroll_offset.get());
    println!("  final scroll offset: {final_offset:?}");

    println!();
    println!("═══ {label}: elements={n} ═══");
    acc.print(label);
    println!(
        "  incremental_layouts_taken during scroll: {}",
        incr_after - incr_before
    );
}

#[test]
#[ignore]
fn b1_virtual_table_scroll_in_app_tree() {
    bench_table_scroll(true, "B1 virtual table scroll + 1.5k-element app tree");
}

#[test]
#[ignore]
fn b2_virtual_table_scroll_alone() {
    bench_table_scroll(false, "B2 virtual table scroll alone");
}

#[test]
#[ignore]
#[cfg(feature = "ext-jiff")]
fn b3_calendar_idle_prepass() {
    // Control: same-size static tree, no calendar.
    let mut h0 = TestHarness::new(800.0, 600.0);
    h0.enable_perf();
    h0.mount(bw(build_balanced(4, 3)));
    for _ in 0..5 {
        h0.run_frame();
    }
    let mut acc0 = PhaseAvg::new();
    for _ in 0..60 {
        h0.run_frame();
        acc0.record(&h0);
    }
    println!();
    println!(
        "═══ B3 control (no calendar), elements={} ═══",
        element_count(&h0)
    );
    acc0.print("B3 control idle");

    // DatePicker (embeds Calendar in its dropdown), opened, idle frames.
    let sel = Signal::new(None::<jiff::civil::Date>);
    let mut h = TestHarness::new(800.0, 600.0);
    h.enable_perf();
    let mounted = h.mount(
        VStack::new()
            .push(bw(build_balanced(4, 3)))
            .push(DatePicker::new(sel)),
    );
    for _ in 0..5 {
        h.run_frame();
    }
    // Open the dropdown: click the trigger (2nd child of the mounted VStack).
    let trigger = *h.find(mounted).unwrap().children.last().unwrap();
    h.click(trigger);
    for _ in 0..5 {
        h.run_frame();
    }
    let mut acc = PhaseAvg::new();
    for _ in 0..60 {
        h.run_frame();
        acc.record(&h);
    }
    println!();
    println!(
        "═══ B3 date_picker(open) idle, elements={} ═══",
        element_count(&h)
    );
    acc.print("B3 calendar idle");
}

#[test]
#[ignore]
fn b4_incremental_layout_vs_tree_size() {
    // A single contained text change; layout time should be O(k), not O(N).
    // If it scales with N, an O(N) term (e.g. snapshot_element_bounds) dominates.
    println!();
    println!(
        "{:>14} {:>8} {:>12} {:>12} {:>8}",
        "tree", "nodes", "layout_avg", "layout_max", "incr"
    );
    for (b, d) in [(4usize, 4usize), (6, 4), (8, 4)] {
        let sig = Signal::new(String::from("a"));
        let mut h = TestHarness::new(1600.0, 1200.0);
        h.enable_perf();
        h.mount(
            VStack::new().push(bw(build_balanced(b, d))).push(
                SizedBox::new()
                    .width(200.0)
                    .height(30.0)
                    .child(Text::new(String::new()).bind(sig.clone())),
            ),
        );
        for _ in 0..5 {
            h.run_frame();
        }
        let n = element_count(&h);
        let incr_before = h.incremental_taken();
        let mut layouts = vec![];
        for i in 0..40 {
            h.set_signal(&sig, format!("t{i}"));
            h.run_frame();
            let t = h.frame_timing();
            layouts.push(t.phases[PerfPhase::Layout as usize]);
            h.run_frame();
        }
        let incr_after = h.incremental_taken();
        let avg = layouts.iter().sum::<u64>() / layouts.len() as u64;
        let max = *layouts.iter().max().unwrap();
        println!(
            "{:>14} {:>8} {:>10}us {:>10}us {:>8}",
            format!("balanced_{b}x{d}"),
            n,
            avg,
            max,
            incr_after - incr_before
        );
    }
}

#[test]
#[ignore]
fn b5_static_scrollview_scroll_control() {
    // Control for B1/B2: a plain ScrollView with static content.
    // Expected: scroll frames are repaint-only (layout ≈ 0).
    let mut content = VStack::new();
    for i in 0..200 {
        content = content.push(Text::new(format!("line {i}")));
    }
    let mut h = TestHarness::new(800.0, 600.0);
    h.enable_perf();
    let mounted = h.mount(
        SizedBox::new()
            .width(400.0)
            .height(400.0)
            .child(ScrollView::new().child(content)),
    );
    for _ in 0..5 {
        h.run_frame();
    }
    let mut acc = PhaseAvg::new();
    for _ in 0..60 {
        h.scroll(mounted, 0.0, 10.0);
        h.run_frame();
        acc.record(&h);
    }
    println!();
    println!(
        "═══ B5 static ScrollView scroll, elements={} ═══",
        element_count(&h)
    );
    acc.print("B5 static scroll");
}

#[test]
#[ignore]
fn b6_text_rebuild_paint_cost() {
    // Audit round 3, ③: cost of the lazy-label buffer rebuild path.
    // 40 bound Text elements; every frame each signal gets a new string,
    // forcing a buffer rebuild per element per frame. Measures the paint
    // phase where create_buffer/reuse happens.
    use auralis_signal::Signal;
    let sigs: Vec<Signal<String>> = (0..40).map(|i| Signal::new(format!("cell {i}"))).collect();
    let mut root = VStack::new().width(px(700.0)).height(px(600.0));
    for sig in &sigs {
        root = root.push(Text::new(String::new()).bind(sig.clone()));
    }
    let mut h = TestHarness::new(800.0, 600.0);
    h.enable_perf();
    h.mount(root);
    for _ in 0..5 {
        h.run_frame();
    }
    let mut acc = PhaseAvg::new();
    for f in 0..120u32 {
        for (i, sig) in sigs.iter().enumerate() {
            sig.set(format!("cell {i} frame {f}"));
        }
        h.run_frame();
        acc.record(&h);
    }
    println!();
    println!("═══ B6 lazy-label rebuild (40 texts × 120 frames) ═══");
    acc.print("B6 text rebuild");
}

// ── Structural guards (machine-independent, run in the normal suite) ──
// These lock in the O(k) shape of virtual-scroll frames without depending on
// wall-clock numbers: mid-scroll remaps must stay on the incremental layout
// path with a bounded dirty set, and never escalate to a full pass.

#[test]
fn guard_virtual_scroll_stays_incremental() {
    let rows = Signal::new((0..5_000).map(|i| format!("Row {i}")).collect::<Vec<_>>());
    let mut h = TestHarness::new(700.0, 400.0);
    let mounted = h.mount(
        SizedBox::new().width(700.0).height(400.0).child(
            Table::new(rows)
                .columns(cols3())
                .row_height(28.0)
                .virtual_threshold(16),
        ),
    );
    for _ in 0..6 {
        h.run_frame();
    }

    // Find the scroll container (tallest content bounds).
    let target = {
        let mut best: Option<(ElementId, f32)> = None;
        let mut stack = vec![mounted];
        while let Some(id) = stack.pop() {
            if let Some(sc) = h.root().comp_scroll(id) {
                let cb = sc.content_bounds.get().height;
                if best.map_or(true, |(_, b)| cb > b) {
                    best = Some((id, cb));
                }
            }
            if let Some(el) = h.find(id) {
                for &c in &el.children {
                    stack.push(c);
                }
            }
        }
        best.unwrap().0
    };

    // Warm up past the first pool wrap.
    h.scroll(target, 0.0, -(20.0 * 28.0));
    for _ in 0..4 {
        h.run_frame();
    }

    let esc_before = h.escalation_taken();
    let incr_before = h.incremental_taken();
    // Steady mid-scroll: 1-row steps. Each remap frame must be incremental
    // with a bounded dirty set (ring reuse: ~1 slot × cells + ancestors).
    for _ in 0..10 {
        h.scroll(target, 0.0, -28.0);
        h.run_frame(); // scroll → repaint & tick
        h.run_frame(); // remap → contained REPOSITION layout
        assert!(
            h.frame_dirty_set_size() <= 40,
            "mid-scroll dirty set must stay bounded (ring reuse), got {}",
            h.frame_dirty_set_size()
        );
    }
    assert_eq!(
        h.escalation_taken() - esc_before,
        0,
        "mid-scroll must never escalate to a full pass"
    );
    assert!(
        h.incremental_taken() > incr_before,
        "remap frames must take the incremental layout path"
    );
}
