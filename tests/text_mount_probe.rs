//! Mount-time probe (audit 2026-07-17 round 2, L3): static Text mount cost.
//! Run: cargo test --profile bench --test text_mount_probe -- --ignored --nocapture --test-threads 1

use burin::testing::TestHarness;
use burin::widgets::display::Text;
use burin::widgets::layout::VStack;
use std::time::Instant;

#[test]
#[ignore]
fn mount_500_static_texts() {
    // warmup harness + fonts
    {
        let mut h = TestHarness::new(800.0, 600.0);
        h.mount(Text::new("warmup"));
        h.run_frame();
    }
    let mut times = vec![];
    for round in 0..5 {
        let mut v = VStack::new();
        for i in 0..500 {
            v = v.push(Text::new(format!(
                "static label {i} round {round} — quick brown fox"
            )));
        }
        let mut h = TestHarness::new(800.0, 600.0);
        let t0 = Instant::now();
        h.mount(v);
        let mount_us = t0.elapsed().as_micros();
        h.run_frame();
        times.push(mount_us);
    }
    println!(
        "mount 500 texts: {:?} us (min {})",
        times,
        times.iter().min().unwrap()
    );
}
