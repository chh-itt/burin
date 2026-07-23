//! StickyHeader behavior + lifecycle regression tests (audit 2026-07-17
//! round 3, Finding B — rewrite of the previously non-functional widget).
//!
//! Locks:
//! 1. A top sticky header sticks once the scroll passes it (offset == scroll_y
//!    for a header at content top).
//! 2. Multi-stack: the second stuck header sits exactly below the first
//!    (stacked by REAL height, not by the old top_offset approximation).
//! 3. Unsticking: scrolling back releases the offset to 0.
//! 4. Idle frames after a scroll register zero dirty (change guard).
//! 5. Teardown reclaims registry entries (Finding A protocol).

use burin::core::element::ElementId;
use burin::style::{Color, Styled};
use burin::testing::TestHarness;
use burin::widgets::display::Text;
use burin::widgets::layout::sticky_header;
use burin::widgets::layout::{ScrollView, SizedBox, StickyHeader, VStack};

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

fn header(label: &str) -> StickyHeader {
    StickyHeader::new(
        SizedBox::new()
            .height(30.0)
            .child(Text::new(label))
            .background(Color::rgba8(40, 40, 48, 255)),
    )
}

fn rows(n: usize, tag: &str) -> VStack {
    let mut v = VStack::new();
    for i in 0..n {
        v = v.push(
            SizedBox::new()
                .height(30.0)
                .child(Text::new(format!("{tag} row {i}"))),
        );
    }
    v
}

/// Scroll page: [header A, 20 rows, header B, 20 rows] in a 400px viewport.
fn mount_page(h: &mut TestHarness) -> (ElementId, ElementId) {
    let mounted = h.mount(
        SizedBox::new().width(400.0).height(400.0).child(
            ScrollView::new().child(
                VStack::new()
                    .push(header("Section A"))
                    .push(rows(20, "a"))
                    .push(header("Section B"))
                    .push(rows(20, "b")),
            ),
        ),
    );
    for _ in 0..5 {
        h.run_frame();
    }
    let container = find_scroll_container(h, mounted);
    (mounted, container)
}

#[test]
fn top_header_sticks_and_unsticks() {
    let mut h = TestHarness::new(500.0, 500.0);
    let (_page, container) = mount_page(&mut h);

    let offsets = sticky_header::debug_applied_offsets();
    assert_eq!(offsets.len(), 2, "two sticky entries registered");
    assert_eq!(offsets[0].1, 0.0, "header A unstuck before scrolling");

    // Scroll down 200px — header A (content_y = 0) must be pinned at the top.
    h.scroll(container, 0.0, -200.0);
    h.run_frame();
    let offsets = sticky_header::debug_applied_offsets();
    let scroll_y = h
        .root()
        .comp_scroll(container)
        .map(|s| s.scroll_offset.get().y)
        .unwrap();
    assert!(scroll_y > 0.0, "scroll moved");
    assert!(
        (offsets[0].1 - scroll_y).abs() < 0.5,
        "header A pinned: offset {} == scroll_y {}",
        offsets[0].1,
        scroll_y
    );

    // Scroll back to the top — offset must release to 0.
    h.scroll(container, 0.0, 400.0);
    h.run_frame();
    let offsets = sticky_header::debug_applied_offsets();
    assert_eq!(offsets[0].1, 0.0, "header A released at scroll 0");
}

#[test]
fn second_header_stacks_below_first_by_real_height() {
    let mut h = TestHarness::new(500.0, 500.0);
    let (_page, container) = mount_page(&mut h);

    // Header B sits at content_y = 30 (A) + 20*30 (rows) = 630. Scroll far
    // past it so both stick.
    h.scroll(container, 0.0, -800.0);
    h.run_frame();

    let offsets = sticky_header::debug_applied_offsets();
    let (a_id, a_off) = offsets[0];
    let (b_id, b_off) = offsets[1];
    assert!(a_off > 0.0 && b_off > 0.0, "both headers stuck");

    let scroll_y = h
        .root()
        .comp_scroll(container)
        .map(|s| s.scroll_offset.get().y)
        .unwrap();
    let container_y = h.find(container).unwrap().screen_bounds.y;
    let vp = |id: ElementId, off: f32| {
        let b = h.find(id).unwrap().screen_bounds;
        (b.y - container_y) - scroll_y + off
    };
    let a_h = h.find(a_id).unwrap().screen_bounds.height;
    assert!(
        (vp(a_id, a_off) - 0.0).abs() < 0.5,
        "header A pinned at viewport top"
    );
    assert!(
        (vp(b_id, b_off) - a_h).abs() < 0.5,
        "header B pinned exactly below A (y = {}, expected {})",
        vp(b_id, b_off),
        a_h
    );
}

#[test]
fn idle_frames_after_scroll_register_zero_dirty() {
    let mut h = TestHarness::new(500.0, 500.0);
    let (_page, container) = mount_page(&mut h);

    h.scroll(container, 0.0, -200.0);
    h.run_frame();
    h.run_frame(); // settle

    // With the sticky headers stuck, idle frames must not re-dirty them.
    for _ in 0..5 {
        h.run_frame();
        assert_eq!(
            h.frame_dirty_set_size(),
            0,
            "idle frame with stuck headers must register zero dirty"
        );
    }
}

#[test]
fn sticky_entries_reclaimed_on_teardown() {
    let mut h = TestHarness::new(500.0, 500.0);
    let baseline = sticky_header::debug_entry_len();
    let arena_baseline = h.arena.len();

    for _ in 0..3 {
        let (page, container) = mount_page(&mut h);
        h.scroll(container, 0.0, -100.0);
        h.run_frame();
        assert_eq!(
            sticky_header::debug_entry_len(),
            baseline + 2,
            "entries live while mounted"
        );
        h.arena.remove(page);
        h.run_frame();
        h.run_frame();
    }

    assert_eq!(
        sticky_header::debug_entry_len(),
        baseline,
        "sticky entries reclaimed after unmount cycles"
    );
    assert_eq!(h.arena.len(), arena_baseline, "no element leaks");
}

// ── Push-up mode (iOS-style section headers) ──

fn mount_pushup_page(h: &mut TestHarness) -> (ElementId, ElementId) {
    let hdr = |label: &str| {
        StickyHeader::new(
            SizedBox::new()
                .height(30.0)
                .child(Text::new(label))
                .background(Color::rgba8(40, 40, 48, 255)),
        )
        .push_up()
    };
    let mounted = h.mount(
        SizedBox::new().width(400.0).height(400.0).child(
            ScrollView::new().child(
                VStack::new()
                    .push(hdr("Section A"))
                    .push(rows(20, "a"))
                    .push(hdr("Section B"))
                    .push(rows(20, "b")),
            ),
        ),
    );
    for _ in 0..5 {
        h.run_frame();
    }
    let container = find_scroll_container(h, mounted);
    (mounted, container)
}

/// Viewport-space y of a sticky element (its rendered position relative to
/// the scroll container's top edge).
fn viewport_y(h: &TestHarness, container: ElementId, id: ElementId, off: f32) -> f32 {
    let scroll_y = h
        .root()
        .comp_scroll(container)
        .map(|s| s.scroll_offset.get().y)
        .unwrap();
    let container_y = h.find(container).unwrap().screen_bounds.y;
    let b = h.find(id).unwrap().screen_bounds;
    (b.y - container_y) - scroll_y + off
}

#[test]
fn pushup_far_header_pins_normally() {
    let mut h = TestHarness::new(500.0, 500.0);
    let (_page, container) = mount_pushup_page(&mut h);

    // Header B is at content_y 630 — far away. Scroll 200: A pins at 0.
    h.scroll(container, 0.0, -200.0);
    h.run_frame();
    let offsets = sticky_header::debug_applied_offsets();
    let (a_id, a_off) = offsets[0];
    let vy = viewport_y(&h, container, a_id, a_off);
    assert!(
        (vy - 0.0).abs() < 0.5,
        "far from B, push-up A pins at the top: vy={vy}"
    );
}

#[test]
fn pushup_next_header_pushes_previous_out() {
    let mut h = TestHarness::new(500.0, 500.0);
    let (_page, container) = mount_pushup_page(&mut h);

    // Header B natural content_y = 30 + 20*30 = 630, A height = 30.
    // Scroll so B's viewport_y = 15 (< A height): A must be pushed up to
    // nvy - h = 15 - 30 = -15 (half out of the viewport).
    h.scroll(container, 0.0, -615.0);
    h.run_frame();
    let offsets = sticky_header::debug_applied_offsets();
    let (a_id, a_off) = offsets[0];
    let a_vy = viewport_y(&h, container, a_id, a_off);
    assert!(
        (a_vy - (-15.0)).abs() < 0.5,
        "A pushed up by approaching B: viewport_y {} expected -15",
        a_vy
    );

    // Scroll further: B fully sticks at 0, A fully out (vy <= -height).
    h.scroll(container, 0.0, -100.0);
    h.run_frame();
    let offsets = sticky_header::debug_applied_offsets();
    let (a_id, a_off) = offsets[0];
    let (b_id, b_off) = offsets[1];
    assert!(
        (viewport_y(&h, container, b_id, b_off) - 0.0).abs() < 0.5,
        "B pinned at top"
    );
    assert!(
        viewport_y(&h, container, a_id, a_off) <= -29.5,
        "A fully pushed out above the viewport"
    );
}
