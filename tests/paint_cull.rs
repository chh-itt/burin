//! Paint-culling regression guards (audit 2026-07-17 L2).
//!
//! The subtree-AABB cull in `paint_children_sorted` must:
//!   1. keep scroll-frame paint work O(visible), not O(N_content)
//!   2. never cull content that is actually inside the viewport
//!      (correctness across arbitrary scroll offsets)
//!   3. keep every element reachable again after it scrolls back in

use burin::core::element::ElementId;
use burin::testing::TestHarness;
use burin::widgets::display::Text;
use burin::widgets::layout::{ScrollView, SizedBox, VStack};

const LINE_H: f32 = 22.0; // Text default line height in this theme (measured)

fn mount_list(h: &mut TestHarness, lines: usize) -> ElementId {
    let mut content = VStack::new();
    for i in 0..lines {
        content = content.push(Text::new(format!("line {i}")));
    }
    let mounted = h.mount(
        SizedBox::new()
            .width(400.0)
            .height(400.0)
            .child(ScrollView::new().child(content)),
    );
    for _ in 0..5 {
        h.run_frame();
    }
    mounted
}

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

/// Visible text rows for the current frame, derived from the harness's
/// captured text areas (top is screen-space).
fn visible_rows(h: &TestHarness) -> Vec<i64> {
    h.last_text_areas
        .iter()
        .filter(|ta| ta.top > -LINE_H && ta.top < 400.0 + LINE_H)
        .map(|ta| ta.top as i64)
        .collect()
}

/// Structural guard: a 1000-line list scroll frame must re-record only the
/// on-screen subtrees (bounded), not the whole content (O(N)).
#[test]
fn scroll_frame_paint_work_is_bounded_by_viewport() {
    let mut h = TestHarness::new(800.0, 600.0);
    let mounted = mount_list(&mut h, 1000);
    let target = find_scroll_container(&h, mounted);

    for _ in 0..10 {
        h.scroll(target, 0.0, -50.0);
        h.run_frame();
        let misses = h.subtree_cache_misses();
        // viewport 400px / ~22px lines ≈ 19 visible rows; allow generous slack
        // (AABB grow, scroll chrome, container levels) but far below 1000.
        assert!(
            misses <= 80,
            "scroll-frame subtree work must be O(visible); got {misses} cache misses for 1000 rows"
        );
    }
}

/// Correctness sweep: at every scroll offset, the rows inside the viewport
/// must actually be painted (present in the emitted text areas). Catches
/// over-aggressive AABB culling.
#[test]
fn scrolled_content_is_never_missing_from_paint() {
    let mut h = TestHarness::new(800.0, 600.0);
    let mounted = mount_list(&mut h, 300);
    let target = find_scroll_container(&h, mounted);

    // Sweep through a screenful in odd increments to hit boundary cases.
    for step in 0..40 {
        h.scroll(target, 0.0, -(7.0 + (step % 3) as f32));
        h.run_frame();
        let rows = visible_rows(&h);
        // ~19 rows fit in the 400px viewport; require a healthy floor so a
        // culling bug (missing rows) trips the assert.
        assert!(
            rows.len() >= 15,
            "step {step}: expected >= 15 visible text rows, got {} — content culled wrongly",
            rows.len()
        );
    }

    // Scroll all the way back: original content must fully reappear.
    let offset = h
        .root()
        .comp_scroll(target)
        .map(|s| s.scroll_offset.get())
        .unwrap();
    h.scroll(target, 0.0, offset.y);
    h.run_frame();
    let rows = visible_rows(&h);
    assert!(
        rows.len() >= 15,
        "after scrolling back to top: got {} visible rows",
        rows.len()
    );
}

/// A row exactly at the viewport edge (partially visible) must paint.
#[test]
fn partially_visible_edge_rows_paint() {
    let mut h = TestHarness::new(800.0, 600.0);
    let mounted = mount_list(&mut h, 100);
    let target = find_scroll_container(&h, mounted);

    // Scroll by half a line so both edges have a partially-visible row.
    h.scroll(target, 0.0, -(LINE_H * 0.5));
    h.run_frame();
    let rows = visible_rows(&h);
    // Top edge: a row straddling y=0 must be present (top < 0).
    assert!(
        rows.iter().any(|&t| t < 0),
        "no straddling top-edge row painted; rows: {rows:?}"
    );
}
