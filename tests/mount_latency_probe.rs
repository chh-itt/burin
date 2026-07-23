//! Mount / first-frame latency probe (audit 2026-07-18).
//!
//! Run: cargo test --profile bench --test mount_latency_probe -- --ignored --nocapture --test-threads 1

use auralis_signal::Signal;
use burin::style::styled::Styled;
use burin::testing::TestHarness;
use burin::widgets::display::{List, Text};
use burin::widgets::input::{Button, TextInput};
use burin::widgets::layout::{Center, HStack, VStack};
use std::time::Instant;

fn format_us(us: u64) -> String {
    let v = us as f64;
    if v >= 1000.0 {
        format!("{:.2} ms", v / 1000.0)
    } else {
        format!("{:.0} us", v)
    }
}
fn format_us_128(us: u128) -> String {
    let v = us as f64;
    if v >= 1000.0 {
        format!("{:.2} ms", v / 1000.0)
    } else {
        format!("{:.0} us", v)
    }
}

fn element_count(h: &TestHarness) -> usize {
    let mut count = 0;
    count_elements(h, h.root_id(), &mut count);
    count
}
fn count_elements(h: &TestHarness, id: burin::core::ElementId, count: &mut usize) {
    *count += 1;
    for &cid in &h.arena.get(id).unwrap().children {
        count_elements(h, cid, count);
    }
}

#[test]
#[ignore]
fn mount_latency_suite() {
    println!(
        "{:<30} {:>6} {:>10} {:>10} {:>10} {:>9}",
        "scenario", "elems", "mount", "first_ff", "steady", "ratio"
    );
    println!("{}", "-".repeat(85));

    {
        let mut h = TestHarness::new(1600.0, 1200.0);
        let t0 = Instant::now();
        h.mount(
            VStack::new()
                .push(Text::new("Hello"))
                .push(Text::new("World"))
                .push(Text::new("Foo"))
                .push(Text::new("Bar"))
                .push(Text::new("Baz")),
        );
        let mount_us = t0.elapsed().as_micros();
        h.run_frame();
        let t_first: u64 = h.frame_timing().phases.iter().sum();
        h.run_frame();
        let t_steady: u64 = h.frame_timing().phases.iter().sum();
        let elems = element_count(&h);
        println!(
            "{:<30} {:>6} {:>10} {:>10} {:>10} {:>9.1}x",
            "5 texts only",
            elems,
            format_us_128(mount_us),
            format_us(t_first),
            format_us(t_steady),
            t_first as f64 / t_steady.max(1) as f64
        );
    }

    for _ in 0..3 {
        let mut h = TestHarness::new(1600.0, 1200.0);
        let mut v = VStack::new();
        for i in 0..200 {
            v = v.push(Text::new(format!("Label {i}: content {}", (i % 40) + 20)));
        }
        let t0 = Instant::now();
        h.mount(Center::new(v));
        let mount_us = t0.elapsed().as_micros();
        h.run_frame();
        let t_first: u64 = h.frame_timing().phases.iter().sum();
        h.run_frame();
        let _t_steady: u64 = h.frame_timing().phases.iter().sum();
        let elems = element_count(&h);
        println!(
            "{:<30} {:>6} {:>10} {:>10} {:>10} {:>9.1}x",
            "200 static texts",
            elems,
            format_us_128(mount_us),
            format_us(t_first),
            0u64,
            0.0
        );
    }

    for _ in 0..3 {
        let mut h = TestHarness::new(1600.0, 1200.0);
        let t0 = Instant::now();
        let mut v = VStack::new().padding(burin::style::Padding::all(16.0));
        for i in 0..20 {
            v = v.push(
                HStack::new()
                    .push(Text::new(format!("Field {}", i + 1)).font_size(13.0))
                    .push(TextInput::new(Signal::new(String::new()))),
            );
        }
        v = v.push(
            HStack::new()
                .push(Button::new("Submit").primary())
                .push(Button::new("Cancel")),
        );
        h.mount(v);
        let mount_us = t0.elapsed().as_micros();
        h.run_frame();
        let t_first: u64 = h.frame_timing().phases.iter().sum();
        let elems = element_count(&h);
        println!(
            "{:<30} {:>6} {:>10} {:>10} {:>10} {:>9.1}x",
            "form: 20 fields",
            elems,
            format_us_128(mount_us),
            format_us(t_first),
            0u64,
            0.0
        );
    }

    {
        let items: Vec<String> = (0..500)
            .map(|i| format!("Item #{i} — quick brown fox"))
            .collect();
        let mut h = TestHarness::new(1600.0, 1200.0);
        let data = Signal::new(items);
        let t0 = Instant::now();
        h.mount(
            VStack::new()
                .push(Text::new("List heading").font_size(18.0))
                .push(List::new(data).item_height(28.0))
                .push(Text::new("footer").font_size(12.0)),
        );
        let mount_us = t0.elapsed().as_micros();
        h.run_frame();
        let t_first: u64 = h.frame_timing().phases.iter().sum();
        let elems = element_count(&h);
        println!(
            "{:<30} {:>6} {:>10} {:>10} {:>10} {:>9.1}x",
            "500-item List",
            elems,
            format_us_128(mount_us),
            format_us(t_first),
            0u64,
            0.0
        );
    }
}
