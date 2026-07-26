//! Property-based differential harness for the incremental layout engine.
//!
//! Baseline phase (Task 2): laying out the SAME randomly-generated widget tree
//! twice must produce identical bounds (determinism). Later tasks compare
//! incremental-vs-full layout after mutation.

use proptest::prelude::*;

use crate::core::context::MountContext;
use crate::core::element::ElementId;
use crate::core::widget::Widget;
use crate::layout::taffy_bridge::TaffyBridge;
use crate::style::Rect;
use crate::style::{Dimension, Styled};
use crate::testing::TestHarness;
use crate::widgets::display::Text;
use crate::widgets::layout::{HStack, SizedBox, VStack};
use auralis_signal::Signal;
use std::collections::HashMap;

/// Type-erased wrapper so recursively-built `Box<dyn Widget>` children can be
/// pushed into stacks whose `push`/`child` take `impl Widget + 'static`.
struct BoxedWidget(Box<dyn Widget>);
impl Widget for BoxedWidget {
    fn mount_box(self: Box<Self>, ctx: &mut MountContext<'_>) -> ElementId {
        self.0.mount_box(ctx)
    }
}

#[derive(Debug, Clone, Copy)]
enum Dim {
    Fixed(f32),
    Pct(f32),
    Auto,
}

#[derive(Debug, Clone)]
enum Node {
    Leaf {
        w: f32,
        h: f32,
        grow: bool,
    },
    Stack {
        vertical: bool,
        w: Dim,
        h: Dim,
        gap: f32,
        grow: bool,
        min_w: Option<f32>,
        children: Vec<Node>,
    },
    Grid {
        cols: u32,
        grow: bool,
        children: Vec<Node>,
    },
}

fn arb_dim() -> impl Strategy<Value = Dim> {
    prop_oneof![
        (40f32..300.0).prop_map(Dim::Fixed),
        (0.3f32..1.0).prop_map(Dim::Pct),
        Just(Dim::Auto),
    ]
}

fn arb_node() -> impl Strategy<Value = Node> {
    let leaf = (20f32..120.0, 16f32..48.0, prop::bool::weighted(0.25))
        .prop_map(|(w, h, grow)| Node::Leaf { w, h, grow });
    leaf.prop_recursive(4, 40, 4, |inner| {
        prop_oneof![
            (
                any::<bool>(),
                arb_dim(),
                arb_dim(),
                0f32..12.0,
                prop::bool::weighted(0.25),
                prop::option::weighted(0.3, 40f32..160.0),
                prop::collection::vec(inner.clone(), 1..4)
            )
                .prop_map(|(vertical, w, h, gap, grow, min_w, children)| Node::Stack {
                    vertical,
                    w,
                    h,
                    gap,
                    grow,
                    min_w,
                    children
                }),
            (
                2u32..4,
                prop::bool::weighted(0.25),
                prop::collection::vec(inner, 1..6)
            )
                .prop_map(|(cols, grow, children)| Node::Grid {
                    cols,
                    grow,
                    children
                }),
        ]
    })
}

fn to_dim(d: Dim) -> Dimension {
    match d {
        Dim::Fixed(px) => Dimension::Pixels(px),
        Dim::Pct(p) => Dimension::Percent(p),
        Dim::Auto => Dimension::Auto,
    }
}

fn maybe_grow(w: Box<dyn Widget>, grow: bool) -> Box<dyn Widget> {
    if grow {
        Box::new(crate::widgets::layout::Expanded::new(BoxedWidget(w)))
    } else {
        w
    }
}

fn build_widget(node: &Node) -> Box<dyn Widget> {
    match node {
        Node::Leaf { w, h, grow } => {
            let leaf: Box<dyn Widget> =
                Box::new(SizedBox::new().width(*w).height(*h).child(Text::new("x")));
            maybe_grow(leaf, *grow)
        }
        Node::Stack {
            vertical,
            w,
            h,
            gap,
            grow,
            min_w,
            children,
        } => {
            let stack: Box<dyn Widget> = if *vertical {
                let mut s = VStack::new();
                for c in children {
                    s = s.push(BoxedWidget(build_widget(c)));
                }
                s = s.width(to_dim(*w)).height(to_dim(*h)).gap(*gap);
                if let Some(mw) = min_w {
                    s = s.min_width(*mw);
                }
                Box::new(s)
            } else {
                let mut s = HStack::new();
                for c in children {
                    s = s.push(BoxedWidget(build_widget(c)));
                }
                s = s.width(to_dim(*w)).height(to_dim(*h)).gap(*gap);
                if let Some(mw) = min_w {
                    s = s.min_width(*mw);
                }
                Box::new(s)
            };
            maybe_grow(stack, *grow)
        }
        Node::Grid {
            cols,
            grow,
            children,
        } => {
            use crate::widgets::layout::{GridItem, GridRow};
            let mut g = GridRow::new().columns(*cols);
            for c in children {
                g = g.push(GridItem::new(BoxedWidget(build_widget(c))));
            }
            maybe_grow(Box::new(g), *grow)
        }
    }
}

/// Collect element bounds in deterministic DFS pre-order from the root.
fn ordered_bounds(h: &TestHarness) -> Vec<Rect> {
    fn walk(h: &TestHarness, id: ElementId, out: &mut Vec<Rect>) {
        if let Some(el) = h.find(id) {
            out.push(el.screen_bounds);
            let children = el.children.clone();
            for c in children {
                walk(h, c, out);
            }
        }
    }
    let mut out = Vec::new();
    walk(h, h.root_id(), &mut out);
    out
}

/// Full-tree layout oracle: builds a fresh `TaffyBridge` and computes every
/// element's bounds from the harness's current arena, independent of the
/// incremental gate.
fn compute_full_bounds(harness: &TestHarness) -> HashMap<ElementId, Rect> {
    let mut taffy = TaffyBridge::new();
    let root_id = harness.root_id();
    taffy.clear();
    let root_node = taffy.build_full_tree(harness.root(), root_id);
    let size = harness.size();
    if let Ok(mut s) = taffy.tree.style(root_node).cloned() {
        s.size.width = taffy::style::Dimension::length(size.width);
        s.size.height = taffy::style::Dimension::length(size.height);
        let _ = taffy.tree.set_style(root_node, s);
    }
    taffy.compute_layout(root_node, size).into_iter().collect()
}

/// Collect all ElementIds in DFS pre-order.
fn dfs_ids(h: &TestHarness) -> Vec<ElementId> {
    fn walk(h: &TestHarness, id: ElementId, out: &mut Vec<ElementId>) {
        out.push(id);
        if let Some(el) = h.find(id) {
            let children = el.children.clone();
            for c in children {
                walk(h, c, out);
            }
        }
    }
    let mut out = Vec::new();
    walk(h, h.root_id(), &mut out);
    out
}

proptest! {
    #[test]
    fn full_layout_is_deterministic(node in arb_node()) {
        let mut a = TestHarness::new(800.0, 600.0);
        a.mount(BoxedWidget(build_widget(&node)));
        a.run_frame();

        let mut b = TestHarness::new(800.0, 600.0);
        b.mount(BoxedWidget(build_widget(&node)));
        b.run_frame();

        prop_assert_eq!(ordered_bounds(&a), ordered_bounds(&b));
    }

    #[test]
    fn incremental_matches_full_after_measure_change(node in arb_node()) {
        let sig = Signal::new(String::from("Hi"));
        let mut h = TestHarness::new(800.0, 600.0);
        h.mount(
            SizedBox::new().width(780.0).height(580.0).child(
                VStack::new()
                    .push(BoxedWidget(build_widget(&node)))
                    .push(Text::new(String::new()).bind(sig.clone())),
            ),
        );
        h.run_frame();

        h.set_signal(&sig, String::from("This is a considerably longer piece of text"));
        h.run_frame();
        h.run_frame();

        let full = compute_full_bounds(&h);

        for id in dfs_ids(&h) {
            let inc = h.find(id).unwrap().screen_bounds;
            if let Some(f) = full.get(&id) {
                prop_assert_eq!(inc, *f, "incremental != full at element {:?}", id);
            }
        }
    }

    #[test]
    fn incremental_matches_full_multi_mutation(
        node in arb_node(),
        muts in prop::collection::vec(30usize..260, 1..8),
    ) {
        let sig = Signal::new(String::from("x"));
        let mut h = TestHarness::new(800.0, 600.0);
        h.mount(SizedBox::new().width(780.0).height(580.0).child(
            VStack::new()
                .push(BoxedWidget(build_widget(&node)))
                .push(Text::new(String::new()).bind(sig.clone())),
        ));
        h.run_frame();

        for (i, m) in muts.iter().enumerate() {
            let s = "y".repeat((*m / 8).max(1) + (i % 3));
            h.set_signal(&sig, s);
            h.run_frame();
            h.run_frame();

            let full = compute_full_bounds(&h);
            for id in dfs_ids(&h) {
                if let Some(f) = full.get(&id) {
                    let inc = h.find(id).unwrap().screen_bounds;
                    prop_assert_eq!(inc, *f, "mut#{} incremental != full at {:?}", i, id);
                }
            }
        }
    }

    #[test]
    fn emitted_boundaries_own_size_stable(node in arb_node(), m in 30usize..260) {
        let sig = Signal::new(String::from("x"));
        let mut h = TestHarness::new(800.0, 600.0);
        h.mount(SizedBox::new().width(780.0).height(580.0).child(
            VStack::new()
                .push(BoxedWidget(build_widget(&node)))
                .push(Text::new(String::new()).bind(sig.clone())),
        ));
        h.run_frame();

        let before = compute_full_bounds(&h);
        h.set_signal(&sig, "z".repeat(m / 8 + 2));
        h.run_frame();
        h.run_frame();
        let after = compute_full_bounds(&h);

        for id in dfs_ids(&h) {
            if let (Some(b), Some(a)) = (before.get(&id), after.get(&id)) {
                let size_stable = (b.width - a.width).abs() < 0.001
                    && (b.height - a.height).abs() < 0.001;
                if size_stable {
                    let inc = h.find(id).unwrap().screen_bounds;
                    prop_assert!(
                        (inc.width - a.width).abs() < 0.001 && (inc.height - a.height).abs() < 0.001,
                        "stable-size element {:?} mis-sized by incremental: inc={:?} full={:?}",
                        id, inc, a
                    );
                }
            }
        }
    }

    #[test]
    fn incremental_full_interleaved_with_resize(
        node in arb_node(),
        ops in prop::collection::vec((any::<bool>(), 30usize..260usize, 820u32..1400u32), 1..8),
    ) {
        // Interleave incremental (text-measure) frames with resize frames (which
        // take the full path via root reposition). Validates that switching
        // between the incremental and full paths does not corrupt taffy state
        // (e.g. compute_subtree's temporary style-size override must be restored).
        let sig = Signal::new(String::from("x"));
        let mut h = TestHarness::new(800.0, 600.0);
        h.mount(SizedBox::new().width(780.0).height(580.0).child(
            VStack::new()
                .push(BoxedWidget(build_widget(&node)))
                .push(Text::new(String::new()).bind(sig.clone())),
        ));
        h.run_frame();
        // Settle initial text-measure feedback so op#0 starts from a quiescent
        // state (no pending measure entangled with the first op).
        h.run_frame();
        h.run_frame();

        for (i, (do_resize, m, dim)) in ops.iter().enumerate() {
            if *do_resize {
                // Integer dims: the harness sets the ROOT's own bounds to the
                // raw requested size (not the rounded collect_bounds value), so a
                // fractional size would show a harmless <1px root rounding diff
                // vs the fresh oracle — orthogonal to incremental correctness.
                let rh = (*dim * 3 / 4).max(200);
                h.resize(*dim as f32, rh as f32);
            } else {
                h.set_signal(&sig, "y".repeat(m / 8 + 1));
            }
            h.run_frame();
            h.run_frame();

            let full = compute_full_bounds(&h);
            for id in dfs_ids(&h) {
                if let Some(f) = full.get(&id) {
                    let inc = h.find(id).unwrap().screen_bounds;
                    prop_assert_eq!(inc, *f, "op#{} (resize={}) incremental != full at {:?}", i, do_resize, id);
                }
            }
        }
    }
}

#[test]
fn incremental_path_is_actually_exercised() {
    let sig = Signal::new(String::from("x"));
    let mut h = TestHarness::new(800.0, 600.0);
    h.mount(
        SizedBox::new()
            .width(400.0)
            .height(300.0)
            .child(VStack::new().push(Text::new(String::new()).bind(sig.clone()))),
    );
    h.run_frame();
    let base = h.incremental_taken();
    h.set_signal(&sig, "much longer text string here".into());
    h.run_frame();
    h.run_frame();
    assert!(
        h.incremental_taken() > base,
        "incremental layout path was never taken (base={}, now={}) — differential tests would be vacuous",
        base, h.incremental_taken()
    );
}

#[test]
fn single_axis_row_contained_takes_incremental() {
    use crate::style::{Dimension, Styled};
    let sig = Signal::new(String::from("Hi"));
    let mut h = TestHarness::new(800.0, 600.0);
    // HStack width:100% is a direct child of a definite SizedBox -> x-independent
    // (percent-of-definite), height auto -> single-axis boundary. A single-line
    // width change keeps the row height stable -> contained.
    h.mount(
        SizedBox::new().width(500.0).height(400.0).child(
            HStack::new()
                .width(Dimension::Percent(1.0))
                .push(
                    SizedBox::new()
                        .width(40.0)
                        .height(24.0)
                        .child(Text::new("x")),
                )
                .push(Text::new(String::new()).bind(sig.clone())),
        ),
    );
    h.run_frame();
    h.run_frame();
    let base = h.incremental_taken();
    h.set_signal(&sig, String::from("longer label"));
    h.run_frame();
    h.run_frame();
    println!(
        "[SA] contained: incremental delta = {}",
        h.incremental_taken() - base
    );
    let full = compute_full_bounds(&h);
    for id in dfs_ids(&h) {
        if let Some(f) = full.get(&id) {
            assert_eq!(
                h.find(id).unwrap().screen_bounds,
                *f,
                "contained mismatch at {:?}",
                id
            );
        }
    }
    assert!(
        h.incremental_taken() > base,
        "single-axis row width change should take incremental"
    );
}

#[test]
fn single_axis_wrapping_escalates() {
    use crate::style::{Dimension, Styled};
    let sig = Signal::new(String::from("short"));
    let mut h = TestHarness::new(800.0, 600.0);
    // A row (height stretched to a definite parent) whose child VStack is a
    // shrink-to-content column: the VStack is height-independent (stretched by
    // the row) but WIDTH-dependent (content). A text width change grows the
    // VStack's (dependent) width -> the verify must fail -> escalate.
    h.mount(
        SizedBox::new().width(600.0).height(300.0).child(
            HStack::new()
                .height(Dimension::Percent(1.0))
                .push(VStack::new().push(Text::new(String::new()).bind(sig.clone()))),
        ),
    );
    h.run_frame();
    h.run_frame();
    let base_esc = h.escalation_taken();
    h.set_signal(&sig, "a considerably longer label than before now".into());
    h.run_frame();
    h.run_frame();
    println!(
        "[SA] escalate: escalation delta = {}",
        h.escalation_taken() - base_esc
    );
    let full = compute_full_bounds(&h);
    for id in dfs_ids(&h) {
        if let Some(f) = full.get(&id) {
            assert_eq!(
                h.find(id).unwrap().screen_bounds,
                *f,
                "wrapping mismatch at {:?}",
                id
            );
        }
    }
    assert!(
        h.escalation_taken() > base_esc,
        "multi-line growth in a single-axis column must escalate"
    );
}

#[test]
fn stretch_tree_takes_incremental() {
    let sig = Signal::new(String::from("Hi"));
    let mut h = TestHarness::new(1000.0, 700.0);
    h.mount(
        VStack::new().push(
            HStack::new()
                .push(Text::new(String::new()).bind(sig.clone()))
                .push(Text::new("a"))
                .push(Text::new("b")),
        ),
    );
    h.run_frame();
    h.run_frame();
    let base = h.incremental_taken();
    h.set_signal(&sig, String::from("a much longer label than before"));
    h.run_frame();
    h.run_frame();
    let full = compute_full_bounds(&h);
    for id in dfs_ids(&h) {
        if let Some(f) = full.get(&id) {
            assert_eq!(
                h.find(id).unwrap().screen_bounds,
                *f,
                "stretch mismatch at {:?}",
                id
            );
        }
    }
    assert!(
        h.incremental_taken() > base,
        "stretch tree width change should be contained at a stretch boundary (incremental)"
    );
}
