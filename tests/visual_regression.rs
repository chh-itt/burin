//! Visual regression guards (audit 2026-07-17, perf infrastructure follow-up).
//!
//! Pixel-level comparison is impractical without a GPU in CI, but the
//! `last_text_areas` and `last_scene` vectors produced by TestHarness
//! contain the exact text rows and draw commands rendered each frame.
//! Asserting which rows are visible at known scroll positions catches
//! the exact class of bugs that perf optimizations introduce:
//! frozen rows, mis-placed slots, over-culling, missing text.
//!
//! Tests run in the normal suite — no bench profile or GPU needed.

use burin::testing::probes;
use burin::testing::TestHarness;

/// Extract the row index from a rendered text area whose content follows the
/// probe scene convention `"line {i} — …"`.
fn row_index(ta: &burin::render::wgpu::glyphon_bridge::TextAreaDesc) -> Option<usize> {
    let buf = ta.buffer.borrow();
    let text: String = buf.lines.iter().map(|l| l.text()).collect();
    text.strip_prefix("line ").and_then(|n| {
        let end = n.find(|c: char| !c.is_ascii_digit()).unwrap_or(n.len());
        n[..end].parse::<usize>().ok()
    })
}

/// Scroll a numbered list through `steps` scroll steps of `dy_per_step`,
/// recording which row indices are visible (inside the viewport band) at
/// each step.
fn visible_rows_at_offsets(
    h: &mut TestHarness,
    scroll_target: burin::core::ElementId,
    dy_per_step: f32,
    steps: usize,
) -> Vec<Vec<usize>> {
    let mut result = Vec::with_capacity(steps);
    for _ in 0..steps {
        h.scroll(scroll_target, 0.0, dy_per_step);
        h.run_frame();
        result.push(current_visible_rows(h));
    }
    result
}

fn mount_numbered_list(h: &mut TestHarness, rows: usize) -> burin::core::ElementId {
    let root = probes::build_static_scroll_page(h, rows, 400.0, 400.0);
    for _ in 0..5 {
        h.run_frame();
    }
    let sc = probes::find_tallest_scroll_container(h, root);
    sc
}

#[test]
fn scroll_down_reveals_rows_in_monotonic_order() {
    let mut h = TestHarness::new(500.0, 500.0);
    let sc = mount_numbered_list(&mut h, 200);
    // Each row ~18px (default Text). Scroll 10 rows' worth each step.
    let vis = visible_rows_at_offsets(&mut h, sc, -180.0, 5);
    let first_rows: Vec<_> = vis
        .iter()
        .map(|v| v.first().copied().unwrap_or(0))
        .collect();
    for w in first_rows.windows(2) {
        assert!(
            w[1] > w[0],
            "rows must scroll forward: row {} then row {}",
            w[0],
            w[1]
        );
    }
}

#[test]
fn scroll_sweep_never_loses_previously_visible_rows() {
    let mut h = TestHarness::new(500.0, 500.0);
    let sc = mount_numbered_list(&mut h, 200);
    let vis = visible_rows_at_offsets(&mut h, sc, -36.0, 20); // ~2 rows per step
    for w in vis.windows(2) {
        let overlap: Vec<_> = w[0].iter().filter(|r| w[1].contains(r)).copied().collect();
        assert!(
            !overlap.is_empty() || w[0].is_empty(),
            "adjacent frames must share rows (scroll continuity): {:?} → {:?}",
            &w[0],
            &w[1]
        );
    }
}

/// Rows visible in the LAST painted frame (reads the harness capture
/// directly — do not run extra idle frames, they paint nothing).
fn current_visible_rows(h: &TestHarness) -> Vec<usize> {
    let mut seen: Vec<usize> = h
        .last_text_areas
        .iter()
        .filter(|ta| ta.top > -20.0 && ta.top < 420.0)
        .filter_map(row_index)
        .collect();
    seen.sort_unstable();
    seen.dedup();
    seen
}

#[test]
fn scroll_back_to_top_restores_first_row() {
    let mut h = TestHarness::new(500.0, 500.0);
    let sc = mount_numbered_list(&mut h, 50);
    h.scroll(sc, 0.0, -500.0);
    h.run_frame();
    let mid = current_visible_rows(&h);
    assert!(
        !mid.contains(&0),
        "row 0 must be off-screen after deep scroll (visible: {mid:?})"
    );

    h.scroll(sc, 0.0, 1000.0);
    h.run_frame();
    let top = current_visible_rows(&h);
    assert!(
        top.contains(&0),
        "row 0 must return when scrolling back to top (visible: {top:?})"
    );
}

#[test]
fn edge_rows_straddle_the_viewport() {
    let mut h = TestHarness::new(500.0, 500.0);
    let sc = mount_numbered_list(&mut h, 100);
    // Scroll just enough that row boundaries land mid-viewport.
    h.scroll(sc, 0.0, -85.0); // ~4.7 rows
    h.run_frame();
    assert!(
        !h.last_text_areas.is_empty(),
        "some text must be visible at a fractional offset"
    );
    let rows: Vec<usize> = h.last_text_areas.iter().filter_map(row_index).collect();
    let min = rows.iter().copied().min().unwrap_or(0);
    let max = rows.iter().copied().max().unwrap_or(0);
    assert!(
        max > min,
        "straddling scroll offset must show multiple rows (rows {min}..{max})"
    );
}
