//! Exploratory probe (audit 2026-07-17): does a static ScrollView scroll
//! frame actually repaint, and what does it cost?
//!
//! Run with:
//!   cargo test --profile bench --test scroll_paint_probe -- --ignored --nocapture --test-threads 1

use burin::core::element::ElementId;
use burin::core::perf::PerfPhase;
use burin::testing::TestHarness;
use burin::widgets::display::Text;
use burin::widgets::layout::{ScrollView, SizedBox, VStack};

fn find_scroll_container(h: &TestHarness, mounted: ElementId) -> ElementId {
    let mut stack = vec![mounted];
    let mut best: Option<(ElementId, f32)> = None;
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
    best.expect("no scroll container").0
}

fn run_probe(lines: usize, label: &str) {
    let mut content = VStack::new();
    for i in 0..lines {
        content = content.push(Text::new(format!(
            "line {i} — the quick brown fox jumps over the lazy dog"
        )));
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
    let target = find_scroll_container(&h, mounted);

    let mut paints = vec![];
    let mut totals = vec![];
    let mut hits = vec![];
    let mut misses = vec![];
    for _ in 0..60 {
        h.scroll(target, 0.0, -10.0); // downward (same convention as B1)
        h.run_frame();
        let t = h.frame_timing();
        paints.push(t.phases[PerfPhase::Paint as usize]);
        totals.push(t.phases.iter().sum::<u64>());
        hits.push(h.subtree_cache_hits());
        misses.push(h.subtree_cache_misses());
    }
    let offset = h.root().comp_scroll(target).map(|s| s.scroll_offset.get());
    let avg = |v: &[u64]| v.iter().sum::<u64>() / v.len().max(1) as u64;
    println!("═══ {label} ═══");
    println!("  final offset: {offset:?}  (must be > 0 or the probe is broken)");
    println!(
        "  paint avg {}us max {}us | TOTAL avg {}us | cache hits avg {} misses avg {}",
        avg(&paints),
        paints.iter().max().unwrap(),
        avg(&totals),
        avg(&hits),
        avg(&misses),
    );

    // Idle control: no scroll, same tree.
    let mut idle_paints = vec![];
    for _ in 0..30 {
        h.run_frame();
        let t = h.frame_timing();
        idle_paints.push(t.phases[PerfPhase::Paint as usize]);
    }
    println!("  idle paint avg {}us", avg(&idle_paints));
}

#[test]
#[ignore]
fn scroll_paint_probe() {
    run_probe(200, "probe A: 200-line static ScrollView, 60 scroll frames");
    run_probe(
        1000,
        "probe B: 1000-line static ScrollView, 60 scroll frames",
    );
}
