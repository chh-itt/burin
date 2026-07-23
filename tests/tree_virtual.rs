//! Virtual Tree correctness (audit 2026-07-17 round 5 follow-up): the Tree
//! widget adopts the List virtualization model — viewport-sized pool,
//! `VirtualSlotY` absolute positioning, ring remap on scroll, NaN-forced
//! reconcile on expand/collapse.
//!
//! Guards:
//! 1. Pool is O(viewport), not O(rows).
//! 2. Scrolled rows land at their virtual content-space Y and show the
//!    right labels (no frozen/duplicated rows).
//! 3. Expand/collapse while scrolled re-reconciles and clamps the offset.
//! 4. Click on a scrolled row resolves the correct data row.

use std::collections::HashSet;

use auralis_signal::Signal;
use burin::core::ElementId;
use burin::testing::TestHarness;
use burin::widgets::display::{Tree, TreeNode};
use burin::widgets::layout::SizedBox;

const ROW_H: f32 = 30.0;

#[derive(Clone)]
struct Node {
    id: u32,
    label: String,
    children: Vec<Node>,
}

impl TreeNode for Node {
    type Id = u32;
    fn id(&self) -> u32 {
        self.id
    }
    fn label(&self) -> String {
        self.label.clone()
    }
    fn children(&self) -> &[Node] {
        &self.children
    }
}

/// `n` leaf roots + one expandable root ("Parent 9000") with 5 children.
fn make_roots(n: u32) -> Vec<Node> {
    let mut roots: Vec<Node> = (0..n)
        .map(|i| Node {
            id: i,
            label: format!("Node {i}"),
            children: Vec::new(),
        })
        .collect();
    roots.push(Node {
        id: 9000,
        label: "Parent 9000".into(),
        children: (0..5)
            .map(|i| Node {
                id: 9100 + i,
                label: format!("Child {i}"),
                children: Vec::new(),
            })
            .collect(),
    });
    roots
}

fn mount_tree(
    n: u32,
) -> (
    TestHarness,
    ElementId,
    ElementId,
    Signal<Vec<Node>>,
    Signal<HashSet<u32>>,
    Signal<Option<u32>>,
) {
    let roots = Signal::new(make_roots(n));
    let expanded: Signal<HashSet<u32>> = Signal::new(HashSet::new());
    let selected: Signal<Option<u32>> = Signal::new(None);
    let mut h = TestHarness::new(600.0, 400.0);
    let mounted = h.mount(
        SizedBox::new().width(600.0).height(400.0).child(
            Tree::new(roots.clone())
                .expanded(expanded.clone())
                .selected(selected.clone())
                .row_height(ROW_H)
                .virtual_threshold(16),
        ),
    );
    for _ in 0..5 {
        h.run_frame();
    }
    let target = scroll_container(&h, mounted);
    (h, mounted, target, roots, expanded, selected)
}

/// The scroll container = element with the tallest content_bounds.
fn scroll_container(h: &TestHarness, root: ElementId) -> ElementId {
    let mut best: Option<(ElementId, f32)> = None;
    let mut stack = vec![root];
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
    best.expect("scroll container").0
}

/// Active TreeItem rows sorted by screen y: (id, bounds, label text).
fn active_rows(h: &TestHarness, root: ElementId) -> Vec<(ElementId, burin::style::Rect, String)> {
    let mut out = Vec::new();
    let mut stack = vec![root];
    while let Some(id) = stack.pop() {
        if let Some(el) = h.find(id) {
            if el.accessible_role() == Some(accesskit::Role::TreeItem) && !el.slot_inactive.get() {
                let label = el
                    .lazy_label()
                    .map(|l| {
                        let s = l.take();
                        l.set(s.clone());
                        s
                    })
                    .unwrap_or_default();
                out.push((id, el.screen_bounds, label));
            }
            for &c in &el.children {
                stack.push(c);
            }
        }
    }
    out.sort_by(|a, b| a.1.y.partial_cmp(&b.1.y).unwrap());
    out
}

#[test]
fn virtual_tree_pool_is_viewport_bounded() {
    let (h, mounted, _target, _roots, _exp, _sel) = mount_tree(5000);
    let mut total = 0usize;
    let mut stack = vec![mounted];
    while let Some(id) = stack.pop() {
        if let Some(el) = h.find(id) {
            if el.accessible_role() == Some(accesskit::Role::TreeItem) {
                total += 1;
            }
            for &c in &el.children {
                stack.push(c);
            }
        }
    }
    // viewport cover = ceil(4320/30)+2 = 146 — the pool must be O(viewport),
    // nowhere near the 5001 data rows.
    assert!(
        total <= 200,
        "virtual Tree pool must be viewport-bounded, got {total} row elements for 5001 rows"
    );
}

#[test]
fn virtual_tree_rows_reposition_and_relabel_after_scroll() {
    let (mut h, mounted, target, _roots, _exp, _sel) = mount_tree(1000);

    // Scroll down 300 rows — far past one pool height.
    let scroll_rows = 300.0;
    h.scroll(target, 0.0, -(scroll_rows * ROW_H));
    for _ in 0..3 {
        h.run_frame();
    }

    let sc = h.root().comp_scroll(target).unwrap();
    assert_eq!(
        sc.scroll_offset.get().y,
        scroll_rows * ROW_H,
        "offset applied"
    );

    // screen_bounds live in layout/content space (paint subtracts the scroll
    // offset later): after the remap the pool must host the consecutive
    // window starting at data row 300, at content-space Y = vi × ROW_H.
    let rows = active_rows(&h, mounted);
    assert!(rows.len() >= 12, "pool rows present, got {}", rows.len());

    let first_label = &rows[0].2;
    assert_eq!(
        first_label, "   Node 300",
        "topmost pooled row must host data row 300 (leaf prefix + label), got {first_label:?}"
    );
    // Consecutive labels at consecutive positions (pitch == ROW_H).
    for w in rows.windows(2) {
        let dy = w[1].1.y - w[0].1.y;
        assert!(
            (dy - ROW_H).abs() < 0.5,
            "row pitch must be ROW_H, got {dy}"
        );
    }
    // Labels follow the virtual indices consecutively.
    for (i, (_, _, label)) in rows.iter().enumerate().take(20) {
        let expected = format!("   Node {}", 300 + i);
        assert_eq!(
            label, &expected,
            "row {i} label must track its virtual index"
        );
    }
}

#[test]
fn virtual_tree_expand_collapse_reconciles_while_scrolled() {
    let (mut h, mounted, target, _roots, expanded, _sel) = mount_tree(1000);

    // Scroll to the bottom (row 1001 area).
    h.scroll(target, 0.0, -(1500.0 * ROW_H)); // over-scroll clamps
    for _ in 0..3 {
        h.run_frame();
    }
    let max_off = h.root().comp_scroll(target).unwrap().scroll_offset.get().y;
    assert!(max_off > 0.0, "clamped offset");

    // Expand "Parent 9000" (last row) — 5 children enter the flat list.
    expanded.update(|s| {
        s.insert(9000);
    });
    for _ in 0..4 {
        h.run_frame();
    }
    let rows = active_rows(&h, mounted);
    assert!(
        rows.iter().any(|(_, _, l)| l.contains("Child 0")),
        "expanded children must appear after reconcile: {:?}",
        rows.iter()
            .map(|(_, _, l)| l.clone())
            .take(20)
            .collect::<Vec<_>>()
    );

    // Collapse again — children leave, content shrinks, offset clamps.
    expanded.update(|s| {
        s.clear();
    });
    for _ in 0..4 {
        h.run_frame();
    }
    let rows = active_rows(&h, mounted);
    assert!(
        !rows.iter().any(|(_, _, l)| l.contains("Child")),
        "collapsed children must vanish after reconcile"
    );
    let off_after = h.root().comp_scroll(target).unwrap().scroll_offset.get().y;
    let cb_h = h
        .root()
        .comp_scroll(target)
        .unwrap()
        .content_bounds
        .get()
        .height;
    assert!(
        off_after <= (cb_h - 400.0).max(0.0) + 0.5,
        "offset must be clamped to shrunken content: off={off_after} cb={cb_h}"
    );
}

#[test]
fn virtual_tree_click_resolves_scrolled_row() {
    let (mut h, mounted, target, _roots, _exp, selected) = mount_tree(1000);

    h.scroll(target, 0.0, -(500.0 * ROW_H));
    for _ in 0..3 {
        h.run_frame();
    }

    // Rows live in content space; the spatial hit-test folds the scroll
    // offset back in, so clicking a row's content-space center resolves it
    // exactly like a viewport click over the visually-corresponding point.
    let container_top = h.find(target).expect("container").screen_bounds.y;
    let rows = active_rows(&h, mounted);
    let (rid, rect, label) = rows
        .iter()
        .find(|(_, r, _)| r.y >= container_top - 0.5)
        .expect("visible row")
        .clone();
    let expected_idx: u32 = label
        .trim()
        .trim_start_matches("Node ")
        .parse()
        .expect("label idx");

    let _ = rid;
    h.click_at(burin::style::Point::new(
        rect.x + rect.width * 0.5,
        rect.y + rect.height * 0.5,
    ));
    for _ in 0..2 {
        h.run_frame();
    }

    assert_eq!(
        selected.read().clone(),
        Some(expected_idx),
        "click on a scrolled row must select the row the user sees (slot→virtual mapping)"
    );
}

#[test]
fn non_virtual_tree_unaffected() {
    // Below the threshold the legacy path must behave exactly as before.
    let roots = Signal::new(make_roots(8));
    let expanded: Signal<HashSet<u32>> = Signal::new(HashSet::new());
    let mut h = TestHarness::new(600.0, 400.0);
    let mounted = h.mount(
        SizedBox::new().width(600.0).height(400.0).child(
            Tree::new(roots.clone())
                .expanded(expanded.clone())
                .row_height(ROW_H)
                .virtual_threshold(16),
        ),
    );
    for _ in 0..3 {
        h.run_frame();
    }
    let rows = active_rows(&h, mounted);
    assert_eq!(rows.len(), 9, "8 leaves + 1 parent visible");

    expanded.update(|s| {
        s.insert(9000);
    });
    for _ in 0..3 {
        h.run_frame();
    }
    let rows = active_rows(&h, mounted);
    assert_eq!(rows.len(), 14, "5 children enter the flat list");
}

// ── row_render tests ──

#[test]
fn row_render_custom_label_non_virtual() {
    let roots = Signal::new(vec![Node {
        id: 1,
        label: "Root".into(),
        children: vec![Node {
            id: 2,
            label: "Child".into(),
            children: vec![],
        }],
    }]);
    let expanded: Signal<HashSet<u32>> = Signal::new(HashSet::new());
    let mut h = TestHarness::new(600.0, 400.0);
    let mounted = h.mount(
        SizedBox::new().width(600.0).height(400.0).child(
            Tree::new(roots.clone())
                .expanded(expanded.clone())
                .row_height(ROW_H)
                .row_render(|node, depth, is_expanded| {
                    format!(
                        "[{depth}] {} (id:{}) expanded={}",
                        node.label, node.id, is_expanded
                    )
                }),
        ),
    );
    h.run_frames(3);

    let rows = active_rows(&h, mounted);
    assert_eq!(rows.len(), 1, "collapsed: only root visible");
    assert!(
        rows[0].2.contains("[0] Root (id:1)"),
        "custom label with depth=0, id=1, actual: {}",
        rows[0].2
    );
    assert!(
        rows[0].2.contains("expanded=false"),
        "expanded=false in label"
    );

    // Expand
    expanded.update(|s| {
        s.insert(1);
    });
    h.run_frames(3);
    let rows = active_rows(&h, mounted);
    assert_eq!(rows.len(), 2, "expanded: root + child");
    assert!(
        rows[1].2.contains("[1] Child (id:2)"),
        "child depth=1, actual: {}",
        rows[1].2
    );
    assert!(rows[1].2.contains("expanded=false"), "child not expandable");
}

#[test]
fn row_render_custom_label_virtual() {
    let n = 50u32;
    let roots = Signal::new(make_roots(n));
    let expanded: Signal<HashSet<u32>> = Signal::new(HashSet::new());
    let mut h = TestHarness::new(600.0, 400.0);
    let mounted = h.mount(
        SizedBox::new().width(600.0).height(400.0).child(
            Tree::new(roots.clone())
                .expanded(expanded.clone())
                .row_height(ROW_H)
                .virtual_threshold(10)
                .row_render(|node, depth, is_expanded| {
                    format!("custom-{} d={} exp={}", node.label, depth, is_expanded)
                }),
        ),
    );
    h.run_frames(3);

    let rows = active_rows(&h, mounted);
    assert!(rows.len() > 0, "should have visible rows");
    for (i, (_, _, label)) in rows.iter().enumerate() {
        assert!(
            label.contains("custom-"),
            "row {} label should contain 'custom-', got: {}",
            i,
            label
        );
    }

    // Scroll down and verify custom labels persist
    let sc = scroll_container(&h, mounted);
    h.scroll(sc, 0.0, ROW_H * 10.0);
    h.run_frames(3);
    let rows2 = active_rows(&h, mounted);
    for (i, (_, _, label)) in rows2.iter().enumerate() {
        assert!(
            label.contains("custom-"),
            "after scroll, row {} label should contain 'custom-', got: {}",
            i,
            label
        );
    }
}

#[test]
fn row_render_preserves_accessible_label() {
    let roots = Signal::new(vec![Node {
        id: 1,
        label: "Root".into(),
        children: vec![],
    }]);
    let mut h = TestHarness::new(400.0, 200.0);
    let mounted = h.mount(
        SizedBox::new().width(400.0).height(200.0).child(
            Tree::new(roots)
                .row_height(ROW_H)
                .row_render(|node, _d, _e| format!("RENDERED: {}", node.label)),
        ),
    );
    h.run_frames(3);
    let rows = active_rows(&h, mounted);
    assert_eq!(rows.len(), 1);
    // Row text contains the custom label
    assert!(rows[0].2.contains("RENDERED: Root"));
    // Accessible label should still use TreeNode::label()
    let el = h.find(rows[0].0).unwrap();
    let a11y = el.accessible_label().unwrap_or_default();
    assert_eq!(
        a11y, "Root",
        "accessible label uses TreeNode::label() not custom text"
    );
}
