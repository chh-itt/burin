//! Manual benchmark: is the full-tree `compute_layout` a real bottleneck vs a
//! boundary-isolated `compute_subtree`, given Taffy's per-node input-keyed cache?
//!
//! Run with:
//!   cargo test --release --test layout_bench -- --ignored --nocapture
//!
//! This informs the strategic decision (A: build real incremental layout, vs
//! B: full compute_layout + taffy cache is good enough).

use std::time::Instant;

use crate::core::element::{ElementArena, ElementId};
use crate::layout::taffy_bridge::TaffyBridge;
use crate::style::{Rect, Size};
use crate::testing::TestHarness;
use crate::widgets::display::Text;
use crate::widgets::layout::{HStack, SizedBox, VStack};

fn row(l: usize) -> HStack {
    let mut h = HStack::new();
    for _ in 0..l {
        h = h.push(
            SizedBox::new()
                .width(40.0)
                .height(20.0)
                .child(Text::new("x")),
        );
    }
    h
}

fn section(m: usize, l: usize) -> VStack {
    let mut v = VStack::new();
    for _ in 0..m {
        v = v.push(row(l));
    }
    v
}

fn tree(k: usize, m: usize, l: usize) -> VStack {
    let mut v = VStack::new();
    for _ in 0..k {
        v = v.push(section(m, l));
    }
    v
}

/// DFS collect (id, depth, parent) from the arena root.
fn collect(arena: &ElementArena, root: ElementId) -> Vec<(ElementId, u32, Option<ElementId>)> {
    let mut out = Vec::new();
    fn walk(
        arena: &ElementArena,
        id: ElementId,
        depth: u32,
        parent: Option<ElementId>,
        out: &mut Vec<(ElementId, u32, Option<ElementId>)>,
    ) {
        out.push((id, depth, parent));
        if let Some(el) = arena.get(id) {
            for &c in &el.children.clone() {
                walk(arena, c, depth + 1, Some(id), out);
            }
        }
    }
    walk(arena, root, 0, None, &mut out);
    out
}

fn bench_case(k: usize, m: usize, l: usize) {
    let mut h = TestHarness::new(1600.0, 1200.0);
    h.mount(tree(k, m, l));
    h.run_frame();

    let root_id = h.root_id();
    let arena = h.root();
    let nodes = collect(arena, root_id);
    let n = nodes.len();

    // Build a persistent TaffyBridge (mirrors the real per-frame tree).
    let mut taffy = TaffyBridge::new();
    let root_node = taffy.build_full_tree(arena, root_id);
    let size = Size::new(1600.0, 1200.0);
    // Warm compute → bounds map (also warms taffy's per-node cache).
    let warm: std::collections::HashMap<ElementId, Rect> =
        taffy.compute_layout(root_node, size).into_iter().collect();

    // Pick a deep leaf to dirty, and a mid-level "boundary" ancestor (depth 2 =
    // a section container). Subtree size ≈ n/k.
    let max_depth = nodes.iter().map(|(_, d, _)| *d).max().unwrap_or(0);
    let leaf = nodes
        .iter()
        .rev()
        .find(|(_, d, _)| *d == max_depth)
        .map(|(id, _, _)| *id)
        .unwrap();
    let boundary = nodes
        .iter()
        .find(|(_, d, _)| *d == 2)
        .map(|(id, _, _)| *id)
        .unwrap();
    let sub_size = nodes.iter().filter(|(_, d, _)| *d >= 2).count(); // rough

    let leaf_node = taffy.node_for(leaf).unwrap();
    let boundary_node = taffy.node_for(boundary).unwrap();
    let br = warm
        .get(&boundary)
        .copied()
        .unwrap_or(Rect::new(0.0, 0.0, 100.0, 100.0));
    let frozen = Size::new(br.width, br.height);
    let origin = (br.x, br.y);

    const ITERS: u32 = 300;

    // FULL: dirty one leaf, recompute from root (taffy cache skips unchanged).
    let t0 = Instant::now();
    for _ in 0..ITERS {
        taffy.tree.mark_dirty(leaf_node).unwrap();
        let _ = taffy.compute_layout(root_node, size);
    }
    let full = t0.elapsed() / ITERS;

    // SUBTREE: dirty one leaf, recompute only the boundary subtree.
    let t1 = Instant::now();
    for _ in 0..ITERS {
        taffy.tree.mark_dirty(leaf_node).unwrap();
        let _ = taffy.compute_subtree(
            boundary_node,
            crate::ecs::components::AxisPair::both(true),
            frozen,
            origin,
        );
    }
    let sub = t1.elapsed() / ITERS;

    println!(
        "N={:>6} (subtree≈{:>5})  full={:>8.1}µs  subtree={:>8.1}µs  speedup={:>5.1}x",
        n,
        sub_size,
        full.as_secs_f64() * 1e6,
        sub.as_secs_f64() * 1e6,
        full.as_secs_f64() / sub.as_secs_f64().max(1e-9),
    );
}

#[test]
#[ignore]
fn bench_full_vs_subtree_layout() {
    println!("\n=== layout: full compute_layout(root) vs compute_subtree(boundary) ===");
    println!("(one deep leaf dirtied per iter; persistent tree; taffy cache warm)\n");
    bench_case(5, 5, 4);
    bench_case(10, 10, 5);
    bench_case(15, 15, 6);
    bench_case(20, 20, 8);
    bench_case(30, 30, 10);
}

/// Single-axis boundary probe: a width-fixed/height-auto HStack row containing
/// FIXED-height items. A width-only item change keeps row height stable => the
/// row could be a *width-boundary* under speculative containment.
fn bench_single_axis(k: usize, m: usize, l: usize) {
    use std::collections::HashMap as Map;
    let mut h = TestHarness::new(1600.0, 1200.0);
    h.mount(tree(k, m, l));
    h.run_frame();

    let root_id = h.root_id();
    let arena = h.root();
    let all = collect(arena, root_id);
    let n = all.len();

    let boundary = all
        .iter()
        .find(|(_, d, _)| *d == 2)
        .map(|(id, _, _)| *id)
        .unwrap();
    let mut sub: Vec<(ElementId, u32)> = Vec::new();
    fn dfs(arena: &ElementArena, id: ElementId, d: u32, out: &mut Vec<(ElementId, u32)>) {
        out.push((id, d));
        if let Some(el) = arena.get(id) {
            for &c in &el.children.clone() {
                dfs(arena, c, d + 1, out);
            }
        }
    }
    dfs(arena, boundary, 0, &mut sub);
    let leaf = sub
        .iter()
        .max_by_key(|(_, d)| *d)
        .map(|(id, _)| *id)
        .unwrap();
    let sub_n = sub.len();

    let mut taffy = TaffyBridge::new();
    let root_node = taffy.build_full_tree(arena, root_id);
    let size = Size::new(1600.0, 1200.0);
    let warm: Map<ElementId, Rect> = taffy.compute_layout(root_node, size).into_iter().collect();

    let row_r = warm.get(&boundary).copied().unwrap();
    let leaf_node = taffy.node_for(leaf).unwrap();
    let boundary_node = taffy.node_for(boundary).unwrap();

    const ITERS: u32 = 300;

    let t0 = std::time::Instant::now();
    for _ in 0..ITERS {
        taffy.tree.mark_dirty(leaf_node).unwrap();
        let _ = taffy.compute_layout(root_node, size);
    }
    let full = t0.elapsed() / ITERS;

    let avail = taffy::geometry::Size {
        width: taffy::style::AvailableSpace::Definite(row_r.width),
        height: taffy::style::AvailableSpace::MaxContent,
    };
    let t1 = std::time::Instant::now();
    for _ in 0..ITERS {
        taffy.tree.mark_dirty(leaf_node).unwrap();
        taffy.tree.compute_layout(boundary_node, avail).unwrap();
    }
    let spec = t1.elapsed() / ITERS;

    let row_h_before = row_r.height;
    {
        let mut s = taffy.tree.style(leaf_node).unwrap().clone();
        s.size.width = taffy::style::Dimension::length(300.0);
        taffy.tree.set_style(leaf_node, s).unwrap();
    }
    taffy.compute_layout(root_node, size);
    let row_h_after = taffy.tree.layout(boundary_node).unwrap().size.height;
    let contained = (row_h_before - row_h_after).abs() < 0.01;

    println!(
        "N={:>6} row_sub={:>4}  full={:>8.1}us  spec={:>7.1}us  speedup={:>5.1}x  | width-change row_h {:.1}->{:.1} contained={}",
        n, sub_n,
        full.as_secs_f64() * 1e6,
        spec.as_secs_f64() * 1e6,
        full.as_secs_f64() / spec.as_secs_f64().max(1e-9),
        row_h_before, row_h_after, contained,
    );
}

#[test]
#[ignore]
fn bench_single_axis_row() {
    println!("\n=== single-axis (width-fixed, height-auto HStack row) boundary probe ===");
    bench_single_axis(10, 10, 5);
    bench_single_axis(15, 15, 6);
    bench_single_axis(20, 20, 8);
    bench_single_axis(30, 30, 10);
}

#[test]
#[ignore]
fn verify_partial_freeze_reproduces_full() {
    // Build a width-fixed / height-auto HStack row with fixed-height items.
    // Full pass vs partial-freeze (width Definite, height MaxContent) must give
    // identical SIZES for every element in the row subtree.
    let mut h = TestHarness::new(1600.0, 1200.0);
    h.mount(tree(6, 6, 5));
    h.run_frame();
    let root_id = h.root_id();
    let arena = h.root();
    let all = collect(arena, root_id);
    let boundary = all
        .iter()
        .find(|(_, d, _)| *d == 2)
        .map(|(id, _, _)| *id)
        .unwrap();
    let mut sub: Vec<(ElementId, u32)> = Vec::new();
    fn dfs(arena: &ElementArena, id: ElementId, d: u32, out: &mut Vec<(ElementId, u32)>) {
        out.push((id, d));
        if let Some(el) = arena.get(id) {
            for &c in &el.children.clone() {
                dfs(arena, c, d + 1, out);
            }
        }
    }
    dfs(arena, boundary, 0, &mut sub);

    let mut taffy = TaffyBridge::new();
    let root_node = taffy.build_full_tree(arena, root_id);
    let size = Size::new(1600.0, 1200.0);
    let _ = taffy.compute_layout(root_node, size);
    // record full-pass sizes for the row subtree
    let full_sizes: Vec<(f32, f32)> = sub
        .iter()
        .map(|(id, _)| {
            let n = taffy.node_for(*id).unwrap();
            let l = taffy.tree.layout(n).unwrap();
            (l.size.width, l.size.height)
        })
        .collect();
    let row_r = {
        let n = taffy.node_for(boundary).unwrap();
        *taffy.tree.layout(n).unwrap()
    };

    // partial-freeze recompute of just the row
    let boundary_node = taffy.node_for(boundary).unwrap();
    taffy.tree.mark_dirty(boundary_node).unwrap();
    let avail = taffy::geometry::Size {
        width: taffy::style::AvailableSpace::Definite(row_r.size.width),
        height: taffy::style::AvailableSpace::MaxContent,
    };
    taffy.tree.compute_layout(boundary_node, avail).unwrap();
    let part_sizes: Vec<(f32, f32)> = sub
        .iter()
        .map(|(id, _)| {
            let n = taffy.node_for(*id).unwrap();
            let l = taffy.tree.layout(n).unwrap();
            (l.size.width, l.size.height)
        })
        .collect();

    let mut mism = 0;
    for (i, ((fw, fh), (pw, ph))) in full_sizes.iter().zip(part_sizes.iter()).enumerate() {
        if (fw - pw).abs() > 0.01 || (fh - ph).abs() > 0.01 {
            println!(
                "[VP] mismatch idx {}: full=({:.1},{:.1}) partial=({:.1},{:.1})",
                i, fw, fh, pw, ph
            );
            mism += 1;
        }
    }
    println!("[VP] row subtree elems={} mismatches={}", sub.len(), mism);
    assert_eq!(mism, 0, "partial-freeze did not reproduce full-pass sizes");
}

#[test]
#[ignore]
fn measure_stretch_incremental_gap() {
    use crate::widgets::display::Text;
    use crate::widgets::layout::{HStack, VStack};

    use auralis_signal::Signal;

    // Realistic "stretch-only" tree: nested VStack/HStack (default align:stretch),
    // Text leaves, NO explicit SizedBox/percent/scroll boundaries. One bound text.
    fn hrow(l: usize, bind: Option<&Signal<String>>) -> HStack {
        let mut h = HStack::new();
        for i in 0..l {
            if i == 0 {
                if let Some(s) = bind {
                    h = h.push(Text::new(String::new()).bind(s.clone()));
                    continue;
                }
            }
            h = h.push(Text::new("item"));
        }
        h
    }
    fn sect(m: usize, l: usize, bind_row: Option<usize>, sig: &Signal<String>) -> VStack {
        let mut v = VStack::new();
        for i in 0..m {
            let b = if Some(i) == bind_row { Some(sig) } else { None };
            v = v.push(hrow(l, b));
        }
        v
    }

    let sig = Signal::new(String::from("Hi"));
    let mut h = TestHarness::new(1200.0, 900.0);
    let mut top = VStack::new();
    for i in 0..8 {
        let bind_row = if i == 3 { Some(2) } else { None };
        top = top.push(sect(6, 5, bind_row, &sig));
    }
    h.mount(top);
    h.run_frame();
    h.run_frame();

    // find the bound text element
    let text_id = h
        .find_all_sel(crate::testing::selector::by_role(accesskit::Role::Label))
        .into_iter()
        .next();
    // fallback: DFS for an element whose text is "Hi"
    let text_id = text_id.or_else(|| {
        collect(h.root(), h.root_id())
            .into_iter()
            .map(|(id, _, _)| id)
            .find(|id| h.find(*id).and_then(|e| e.accessible_label()).as_deref() == Some("Hi"))
    });

    let all = collect(h.root(), h.root_id());
    let n = all.len();
    // Count containers (have children) and how many are CURRENT boundary candidates.
    let mut containers = 0usize;
    let mut cur_boundaries = 0usize;
    let mut stretch_auto = 0usize; // containers with an auto cross axis (would-be stretch boundary)
    for (id, _, _) in &all {
        let has_children = h.find(*id).map(|e| !e.children.is_empty()).unwrap_or(false);
        if !has_children {
            continue;
        }
        containers += 1;
        let affected_false = !crate::core::dirty_registry::affected_by_child_size(*id);
        let indep = crate::layout::taffy_bridge::size_independent_of_children(h.root(), *id);
        if affected_false || indep.x || indep.y {
            cur_boundaries += 1;
        }
        // auto on at least one axis (candidate that stretch-detection could rescue)
        if !indep.x || !indep.y {
            stretch_auto += 1;
        }
    }
    println!(
        "[GAP] tree N={} containers={} current_boundaries={} containers_with_auto_axis={}",
        n, containers, cur_boundaries, stretch_auto
    );

    if let Some(tid) = text_id {
        // walk ancestors, print boundary status
        let mut cur = crate::core::dirty_registry::parent_of(tid);
        let mut chain = Vec::new();
        while let Some(p) = cur {
            let affected_false = !crate::core::dirty_registry::affected_by_child_size(p);
            let indep = crate::layout::taffy_bridge::size_independent_of_children(h.root(), p);
            chain.push(format!(
                "{:?}: !affected={} indep=({},{})",
                p, affected_false, indep.x, indep.y
            ));
            cur = crate::core::dirty_registry::parent_of(p);
        }
        println!("[GAP] ancestor chain of bound text (leaf->root):");
        for c in &chain {
            println!("  {}", c);
        }
    } else {
        println!("[GAP] bound text not found");
    }

    let base = h.incremental_taken();
    let base_esc = h.escalation_taken();
    h.set_signal(&sig, String::from("a considerably longer label now"));
    h.run_frame();
    h.run_frame();
    println!(
        "[GAP] after text mutation: incremental delta={} escalation delta={}",
        h.incremental_taken() - base,
        h.escalation_taken() - base_esc
    );
    println!(
        "[GAP] => incremental fired? {}",
        h.incremental_taken() > base
    );
}
