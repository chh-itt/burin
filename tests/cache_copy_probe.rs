//! Probe (audit 2026-07-17 follow-up): cost of the per-ancestor-level
//! subtree-cache re-insert copy in `paint_children_sorted`.
//!
//! Every painted child's command slice is `.to_vec()`'d into the cache at
//! each enclosing level, so a leaf's commands are copied O(depth) times per
//! repainting frame. Fixed leaf work (40 bound Texts re-shaped per frame),
//! sweep the container chain depth: if the copy dominates, paint grows
//! linearly with depth.
//!
//! Run with:
//!   cargo test --profile bench --test cache_copy_probe -- --ignored --nocapture --test-threads 1

use auralis_signal::Signal;
use burin::core::context::MountContext;
use burin::core::element::ElementId;
use burin::core::perf::PerfPhase;
use burin::core::widget::Widget;
use burin::style::{Dimension, Styled};
use burin::testing::TestHarness;
use burin::widgets::display::Text;
use burin::widgets::layout::VStack;

struct BoxedWidget(Box<dyn Widget>);
impl Widget for BoxedWidget {
    fn mount_box(self: Box<Self>, ctx: &mut MountContext<'_>) -> ElementId {
        self.0.mount_box(ctx)
    }
}

fn chain(depth: usize, leaves: &[Signal<String>]) -> Box<dyn Widget> {
    if depth == 0 {
        let mut v = VStack::new();
        for sig in leaves {
            v = v.push(Text::new(String::new()).bind(sig.clone()));
        }
        return Box::new(v);
    }
    Box::new(
        VStack::new()
            .width(Dimension::Pixels(700.0))
            .height(Dimension::Pixels(900.0))
            .push(BoxedWidget(chain(depth - 1, leaves))),
    )
}

#[test]
#[ignore]
fn cache_copy_cost_vs_depth() {
    println!();
    println!("{:>7} {:>12} {:>12}", "depth", "paint_avg", "paint_max");
    for depth in [2usize, 8, 16, 32, 64] {
        let sigs: Vec<Signal<String>> = (0..40).map(|i| Signal::new(format!("cell {i}"))).collect();
        let mut h = TestHarness::new(1000.0, 950.0);
        h.enable_perf();
        h.mount(BoxedWidget(chain(depth, &sigs)));
        for _ in 0..5 {
            h.run_frame();
        }
        let mut paints = vec![];
        for f in 0..80u32 {
            for (i, sig) in sigs.iter().enumerate() {
                sig.set(format!("cell {i} frame {f}"));
            }
            h.run_frame();
            let t = h.frame_timing();
            paints.push(t.phases[PerfPhase::Paint as usize]);
        }
        let avg = paints.iter().sum::<u64>() / paints.len() as u64;
        let max = *paints.iter().max().unwrap();
        println!("{:>7} {:>10}us {:>10}us", depth, avg, max);
    }
}
