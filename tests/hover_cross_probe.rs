//! Exploratory probe (audit 2026-07-17 round 3): what does a hover-chain
//! crossing cost on STATIC content (no hover styles anywhere)?
//!
//! Uses `TestHarness::hover_at`, which (post-SEAM-0 alignment) mirrors the
//! production PointerMoved hover semantics: hit_test → chain diff →
//! set_state_dirty(HOVERED) per entered/left element.
//!
//! Pre-fix numbers (manual chain replay, identical semantics):
//!   S1 cross-panel 100 rows: paint avg 44us TOTAL avg 53us, 5 cache misses
//!   S2 adjacent-row 100 rows: paint avg 28us TOTAL avg 34us, 3 cache misses
//! Post-fix (Finding C: conditional hover invalidation): expected ~0.
//!
//! Run with:
//!   cargo test --profile bench --test hover_cross_probe -- --ignored --nocapture --test-threads 1

use burin::core::perf::PerfPhase;
use burin::style::styled::Styled;
use burin::style::{Color, Point};
use burin::testing::TestHarness;
use burin::widgets::display::Text;
use burin::widgets::layout::{HStack, SizedBox, VStack};

fn build_panel(rows: usize, tag: &str) -> VStack {
    let mut v = VStack::new();
    for i in 0..rows {
        v = v.push(
            HStack::new()
                .background(Color::rgba8(38, 38, 46, 255))
                .push(Text::new(format!("{tag} row {i} — label")))
                .push(Text::new("value 42")),
        );
    }
    v
}

fn avg(v: &[u64]) -> u64 {
    v.iter().sum::<u64>() / v.len().max(1) as u64
}

fn run_scenario(rows: usize, label: &str, pos_a: Point, pos_b: Point) {
    let mut h = TestHarness::new(1200.0, 800.0);
    h.enable_perf();
    let _root = h.mount(
        HStack::new()
            .push(
                SizedBox::new().width(560.0).height(760.0).child(
                    VStack::new()
                        .background(Color::rgba8(26, 26, 31, 255))
                        .push(build_panel(rows, "L")),
                ),
            )
            .push(
                SizedBox::new().width(560.0).height(760.0).child(
                    VStack::new()
                        .background(Color::rgba8(31, 26, 26, 255))
                        .push(build_panel(rows, "R")),
                ),
            ),
    );
    for _ in 0..5 {
        h.run_frame();
    }

    // Idle control first: no hover activity at all.
    let mut idle_paint = vec![];
    let mut idle_total = vec![];
    for _ in 0..30 {
        h.run_frame();
        let t = h.frame_timing();
        idle_paint.push(t.phases[PerfPhase::Paint as usize]);
        idle_total.push(t.phases.iter().sum::<u64>());
    }

    // Hover crossing: alternate A <-> B, one crossing per frame.
    let mut paints = vec![];
    let mut totals = vec![];
    let mut misses = vec![];
    for i in 0..120 {
        let pos = if i % 2 == 0 { pos_a } else { pos_b };
        h.hover_at(pos);
        h.run_frame();
        let t = h.frame_timing();
        paints.push(t.phases[PerfPhase::Paint as usize]);
        totals.push(t.phases.iter().sum::<u64>());
        misses.push(h.subtree_cache_misses());
    }

    println!("═══ {label} ({rows} rows/panel) ═══");
    println!(
        "  idle:    paint avg {}us | TOTAL avg {}us",
        avg(&idle_paint),
        avg(&idle_total)
    );
    println!(
        "  hover-X: paint avg {}us max {}us | TOTAL avg {}us | cache misses avg {}",
        avg(&paints),
        paints.iter().max().unwrap(),
        avg(&totals),
        avg(&misses),
    );
}

#[test]
#[ignore]
fn hover_cross_probe() {
    // Scenario 1: cross-panel (worst realistic case: chain diff spans both
    // panel subtrees). Cursor at mid-height of each panel.
    run_scenario(
        30,
        "S1 cross-panel crossing",
        Point::new(280.0, 400.0),
        Point::new(840.0, 400.0),
    );
    run_scenario(
        100,
        "S1 cross-panel crossing",
        Point::new(280.0, 400.0),
        Point::new(840.0, 400.0),
    );

    // Scenario 2: adjacent-row crossing inside one panel (the common
    // mouse-down-a-list case; rows are ~2x text height apart).
    run_scenario(
        30,
        "S2 adjacent-row crossing",
        Point::new(280.0, 300.0),
        Point::new(280.0, 330.0),
    );
    run_scenario(
        100,
        "S2 adjacent-row crossing",
        Point::new(280.0, 300.0),
        Point::new(280.0, 330.0),
    );
}
