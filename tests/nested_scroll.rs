//! Nested scroll contract tests.
//!
//! Sign convention (unified -dy):
//!   scroll_by(dx, dy): dy>0 = scroll up (offset decreases), dy<0 = scroll down
//!   TestHarness::scroll: uses do_scroll which does o.y -= dy — dy<0 = scroll down

use std::cell::Cell;
use std::rc::Rc;

use burin::core::context::MountContext;
use burin::core::dirty_registry;
use burin::core::widget::Widget;
use burin::core::ElementId;
use burin::style::Dimension;
use burin::testing::TestHarness;
use burin::widgets::bundle::scroll;
use burin::widgets::layout::{ScrollView, SizedBox, VStack};

struct IdCapture<W: Widget> {
    inner: W,
    cell: Rc<Cell<ElementId>>,
}

impl<W: Widget> Widget for IdCapture<W> {
    fn component_mask(&self) -> u64 {
        self.inner.component_mask()
    }
    fn mount_box(self: Box<Self>, ctx: &mut MountContext<'_>) -> ElementId {
        let id = Box::new(self.inner).mount_box(ctx);
        self.cell.set(id);
        id
    }
}

fn capture<W: Widget>(inner: W) -> (Rc<Cell<ElementId>>, impl Widget) {
    let cell = Rc::new(Cell::new(ElementId::SENTINEL));
    (cell.clone(), IdCapture { inner, cell })
}

// ── Helpers ───────────────────────────────────────────────────────

fn scroll_offset_of(h: &TestHarness, eid: ElementId) -> burin::style::Vec2 {
    h.arena
        .comp_scroll(eid)
        .map(|sc| sc.scroll_offset.get())
        .unwrap_or(burin::style::Vec2::ZERO)
}

fn scroll_direct(h: &TestHarness, eid: ElementId, dx: f32, dy: f32) -> (f32, f32) {
    let vp = h
        .arena
        .get(eid)
        .map_or(burin::style::Rect::ZERO, |el| el.screen_bounds);
    scroll::try_scroll_by(&h.arena, eid, dx, dy, vp.height, vp.width).unwrap_or((dx, dy))
}

// ── Single scroll regression ──────────────────────────────────────

#[test]
fn single_scroll_view_scrolls_normally() {
    let mut h = TestHarness::new(400.0, 200.0);
    let scroll_id = h.mount(
        ScrollView::new().child(
            VStack::new()
                .push(SizedBox::new().height(200.0))
                .push(SizedBox::new().height(200.0))
                .push(SizedBox::new().height(200.0))
                .push(SizedBox::new().height(200.0)),
        ),
    );
    h.run_frame();
    let before = scroll_offset_of(&h, scroll_id);
    // do_scroll: o.y -= dy, so negative dy → offset increases → scrolls down
    h.scroll(scroll_id, 0.0, -100.0);
    h.run_frame();
    assert!(scroll_offset_of(&h, scroll_id).y > before.y + 50.0);
}

#[test]
fn scroll_view_at_boundary_stops() {
    let mut h = TestHarness::new(400.0, 200.0);
    let scroll_id = h.mount(ScrollView::new().child(SizedBox::new().height(200.0)));
    h.run_frame();
    h.scroll(scroll_id, 0.0, -50.0);
    h.run_frame();
    assert!((scroll_offset_of(&h, scroll_id).y - 0.0).abs() < 0.5);
}

// ── scroll_by unconsumed delta ────────────────────────────────────

#[test]
fn scroll_by_unconsumed_at_boundary() {
    let mut h = TestHarness::new(400.0, 200.0);
    let id = h.mount(ScrollView::new().child(SizedBox::new().height(200.0)));
    h.run_frame();
    // Scroll up past top — physics rejects all
    let (_, uy) = scroll_direct(&h, id, 0.0, -50.0);
    assert!(uy < -40.0, "unconsumed y={:.1} should be ~ -50", uy);
}

#[test]
fn scroll_by_zero_unconsumed_within_bounds() {
    let mut h = TestHarness::new(400.0, 200.0);
    let id = h.mount(
        ScrollView::new().child(
            VStack::new()
                .push(SizedBox::new().height(400.0))
                .push(SizedBox::new().height(400.0)),
        ),
    );
    h.run_frame();
    // Scroll down within bounds — all consumed (negative dy = scroll down)
    let (_, uy) = scroll_direct(&h, id, 0.0, -100.0);
    assert!(uy.abs() < 0.5, "unconsumed y={:.1} should be 0", uy);
    assert!(scroll_offset_of(&h, id).y > 50.0);
}

// ── Nested scroll chain (spatial_scroll_chain + try_scroll_by) ────

fn setup_nested(h: &mut TestHarness) -> (ElementId, ElementId) {
    // Outer (400×400): SizedBox200 + Inner(150vp) + SizedBox200 = 550 → outer can scroll 150
    // Inner (400×150): 3×SizedBox(150) = 450 → inner can scroll 300
    let (inner_cell, inner_wrapped) = capture(
        ScrollView::new().height(Dimension::Pixels(150.0)).child(
            VStack::new()
                .push(SizedBox::new().height(150.0))
                .push(SizedBox::new().height(150.0))
                .push(SizedBox::new().height(150.0)),
        ),
    );
    let outer_id = h.mount(
        ScrollView::new().child(
            VStack::new()
                .push(SizedBox::new().height(200.0))
                .push(inner_wrapped)
                .push(SizedBox::new().height(200.0)),
        ),
    );
    h.run_frame();
    let inner_id = inner_cell.get();
    assert_ne!(inner_id, ElementId::SENTINEL);
    (outer_id, inner_id)
}

#[test]
fn nested_inner_at_boundary_passes_unconsumed_to_outer() {
    let mut h = TestHarness::new(400.0, 400.0);
    let (outer_id, inner_id) = setup_nested(&mut h);

    // Scroll inner to max (content 450, vp 150, max=300)
    h.scroll(inner_id, 0.0, -300.0);
    h.run_frame();
    let inner_off = scroll_offset_of(&h, inner_id);
    assert!(
        inner_off.y > 280.0,
        "inner should be near max, got y={}",
        inner_off.y
    );

    let outer_before = scroll_offset_of(&h, outer_id);
    assert!(outer_before.y < 0.5, "outer should start at 0");

    // Verify the chain sees both scrollables
    let inner_bounds = h.arena.get(inner_id).unwrap().screen_bounds;
    let pt = burin::style::Point::new(inner_bounds.x + 10.0, inner_bounds.y + 10.0);
    let chain = dirty_registry::spatial_scroll_chain(&h.arena, pt);
    assert_eq!(chain.len(), 2, "chain should have inner + outer");
    assert_eq!(chain[0], inner_id);
    assert_eq!(chain[1], outer_id);

    // Simulate window.rs fallback: iterate chain, pass unconsumed
    let mut rx = 0.0_f32;
    let mut ry = -100.0_f32;
    for &eid in &chain {
        if rx == 0.0 && ry == 0.0 {
            break;
        }
        if let Some(el) = h.arena.get(eid) {
            let vp = el.screen_bounds;
            if let Some((ux, uy)) =
                scroll::try_scroll_by(&h.arena, eid, rx, ry, vp.height, vp.width)
            {
                rx = ux;
                ry = uy;
            }
        }
    }
    h.run_frame();

    let outer_after = scroll_offset_of(&h, outer_id);
    assert!(
        outer_after.y > 20.0,
        "outer should scroll when inner is at boundary (before={:.1}, after={:.1})",
        outer_before.y,
        outer_after.y
    );
}

#[test]
fn nested_inner_within_bounds_only_scrolls_inner() {
    let mut h = TestHarness::new(400.0, 400.0);
    let (outer_id, inner_id) = setup_nested(&mut h);

    let inner_bounds = h.arena.get(inner_id).unwrap().screen_bounds;
    let pt = burin::style::Point::new(inner_bounds.x + 10.0, inner_bounds.y + 10.0);
    let chain = dirty_registry::spatial_scroll_chain(&h.arena, pt);
    assert_eq!(chain[0], inner_id);

    let inner_before = scroll_offset_of(&h, inner_id);
    let outer_before = scroll_offset_of(&h, outer_id);

    let mut rx = 0.0_f32;
    let mut ry = -50.0_f32;
    for &eid in &chain {
        if rx == 0.0 && ry == 0.0 {
            break;
        }
        if let Some(el) = h.arena.get(eid) {
            let vp = el.screen_bounds;
            if let Some((ux, uy)) =
                scroll::try_scroll_by(&h.arena, eid, rx, ry, vp.height, vp.width)
            {
                rx = ux;
                ry = uy;
            }
        }
    }
    h.run_frame();

    assert!(
        scroll_offset_of(&h, inner_id).y > inner_before.y + 20.0,
        "inner should scroll within bounds"
    );
    assert!(
        (scroll_offset_of(&h, outer_id).y - outer_before.y).abs() < 0.5,
        "outer should NOT scroll when inner has room"
    );
}

// ── spatial_scroll_chain ordering ─────────────────────────────────

#[test]
fn spatial_scroll_chain_innermost_first() {
    let mut h = TestHarness::new(400.0, 600.0);
    let (outer_id, inner_id) = setup_nested(&mut h);
    let ib = h.arena.get(inner_id).unwrap().screen_bounds;
    let pt = burin::style::Point::new(ib.x + 10.0, ib.y + 10.0);
    let chain = dirty_registry::spatial_scroll_chain(&h.arena, pt);
    assert_eq!(chain[0], inner_id);
    assert!(chain.contains(&outer_id));
}

#[test]
fn spatial_scroll_chain_excludes_outside_element() {
    let mut h = TestHarness::new(400.0, 600.0);
    let (_outer_id, inner_id) = setup_nested(&mut h);
    // Point is above inner's bounds
    let chain =
        dirty_registry::spatial_scroll_chain(&h.arena, burin::style::Point::new(200.0, 50.0));
    assert!(!chain.contains(&inner_id));
}
