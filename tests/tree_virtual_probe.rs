//! Probe: Tree virtualization cost profile (audit 2026-07-17 round 5
//! follow-up). Compares mount / expand / scroll-frame costs for a large
//! tree with virtualization ON (threshold 16) vs OFF (threshold usize::MAX).
//!
//! Run: cargo test --profile bench --test tree_virtual_probe -- --ignored --nocapture --test-threads 1

use std::collections::HashSet;
use std::time::Instant;

use auralis_signal::Signal;
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
        label: "Parent".into(),
        children: (0..50)
            .map(|i| Node {
                id: 9100 + i,
                label: format!("Child {i}"),
                children: Vec::new(),
            })
            .collect(),
    });
    roots
}

fn run_scenario(n: u32, threshold: usize, tag: &str) {
    let roots = Signal::new(make_roots(n));
    let expanded: Signal<HashSet<u32>> = Signal::new(HashSet::new());
    let mut h = TestHarness::new(600.0, 400.0);

    let t0 = Instant::now();
    let mounted = h.mount(
        SizedBox::new().width(600.0).height(400.0).child(
            Tree::new(roots.clone())
                .expanded(expanded.clone())
                .row_height(ROW_H)
                .virtual_threshold(threshold),
        ),
    );
    for _ in 0..3 {
        h.run_frame();
    }
    let mount = t0.elapsed();

    // Expand + collapse (the reconcile path).
    let t1 = Instant::now();
    expanded.update(|s| {
        s.insert(9000);
    });
    for _ in 0..3 {
        h.run_frame();
    }
    expanded.update(|s| {
        s.clear();
    });
    for _ in 0..3 {
        h.run_frame();
    }
    let toggle = t1.elapsed();

    // 60 scroll frames of 1 row each.
    let target = {
        let mut best: Option<(burin::core::ElementId, f32)> = None;
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
        best.expect("scroll container").0
    };
    let t2 = Instant::now();
    for _ in 0..60 {
        h.scroll(target, 0.0, -ROW_H);
        h.run_frame();
    }
    let scroll = t2.elapsed() / 60;

    eprintln!(
        "{tag:>12} | mount {mount:>9.2?} | expand+collapse {toggle:>9.2?} | scroll frame avg {scroll:>9.2?}"
    );
}

#[test]
#[ignore]
fn tree_virtualization_cost_profile() {
    run_scenario(1_000, 16, "1000 virtual");
    // Eager beyond ~2k rows trips the harness dirty-count invariant
    // (>10k dirty in one frame) — which is precisely the pathology
    // virtualization removes. Compare at 1k, then show virtual scales.
    run_scenario(1_000, usize::MAX, "1000 eager");
    run_scenario(5_000, 16, "5000 virtual");
    run_scenario(20_000, 16, "20000 virtual");
}
