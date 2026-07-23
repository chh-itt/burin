//! Text-pipeline dissection probe (audit 2026-07-17, round 2).
//!
//! B6 measured ~23.5 us per changed bound-Text per frame. This probe breaks
//! that cost into its components at the glyphon_bridge level:
//!
//!   1. Buffer::new alloc + set_size (no text)          — allocation floor
//!   2. create_buffer, unique texts                      — mount path
//!   3. create_buffer, same text                         — OS/font caches only
//!   4. reuse_buffer, changed text                       — today's rebuild path
//!   5. reuse_buffer, same text                          — pure re-shape (no alloc)
//!   6. measure_text_width, unique texts                 — the SECOND shape per rebuild
//!   7. Basic vs Advanced shaping                        — rustybuzz share
//!
//! Run with:
//!   cargo test --profile bench --test text_shaping_probe -- --ignored --nocapture --test-threads 1

use burin::render::text::measure_text_width;
use burin::render::wgpu::glyphon_bridge::{create_buffer, reuse_buffer, FONT_SYSTEM};
use burin::style::TextAlign;
use std::time::Instant;

const ITERS: usize = 2000;

fn bench<F: FnMut(usize)>(label: &str, mut f: F) -> f64 {
    // warmup
    for i in 0..50 {
        f(i);
    }
    let t0 = Instant::now();
    for i in 0..ITERS {
        f(i);
    }
    let us = t0.elapsed().as_secs_f64() * 1e6 / ITERS as f64;
    println!("  {label:<52} {us:>8.2} us/op");
    us
}

fn texts(len: usize) -> Vec<String> {
    let base = "the quick brown fox jumps over the lazy dog 0123456789 ";
    (0..ITERS + 50)
        .map(|i| {
            let mut s = format!("{i:06} ");
            while s.len() < len {
                s.push_str(base);
            }
            s.truncate(len);
            s
        })
        .collect()
}

#[test]
#[ignore]
fn dissect_text_pipeline() {
    // Warm the font system (mirrors the production warmup).
    let _ = create_buffer("warmup", 14.0, 1.5, 400, None, None, TextAlign::Start);

    for len in [12usize, 40, 200] {
        println!("── text length {len} ──");
        let uniq = texts(len);

        // 1. Allocation floor: Buffer::new + set_size, no text.
        bench("Buffer::new + set_size (no text)", |_| {
            FONT_SYSTEM.with(|fs_cell| {
                let mut fs_opt = fs_cell.borrow_mut();
                let fs = fs_opt.as_mut().expect("font system not initialized");
                let m = cosmic_text::Metrics::new(14.0, 21.0);
                let mut b = cosmic_text::Buffer::new(&mut *fs, m);
                b.set_size(None, Some(2100.0));
                std::hint::black_box(&b);
            });
        });

        // 2. Mount path: fresh buffer, unique text.
        bench("create_buffer (unique text)", |i| {
            let b = create_buffer(&uniq[i], 14.0, 1.5, 400, None, None, TextAlign::Start);
            std::hint::black_box(&b);
        });

        // 3. Fresh buffer, SAME text every time (font/OS caches warm).
        bench("create_buffer (same text)", |_| {
            let b = create_buffer(&uniq[0], 14.0, 1.5, 400, None, None, TextAlign::Start);
            std::hint::black_box(&b);
        });

        // 4. Today's rebuild path: reuse existing buffer, changed text.
        {
            let mut buf = create_buffer(&uniq[0], 14.0, 1.5, 400, None, None, TextAlign::Start);
            bench("reuse_buffer (changed text)  [rebuild path]", |i| {
                reuse_buffer(
                    &mut buf,
                    &uniq[i],
                    14.0,
                    1.5,
                    400,
                    None,
                    None,
                    TextAlign::Start,
                );
                std::hint::black_box(&buf);
            });
        }

        // 5. Reuse buffer, SAME text (cosmic-text line reuse short-circuit?).
        {
            let mut buf = create_buffer(&uniq[0], 14.0, 1.5, 400, None, None, TextAlign::Start);
            bench("reuse_buffer (same text)", |_| {
                reuse_buffer(
                    &mut buf,
                    &uniq[0],
                    14.0,
                    1.5,
                    400,
                    None,
                    None,
                    TextAlign::Start,
                );
                std::hint::black_box(&buf);
            });
        }

        // 6. The second shape per rebuild: measure_text_width.
        bench("measure_text_width (unique text)", |i| {
            let w = measure_text_width(&uniq[i], 14.0, 400, None);
            std::hint::black_box(w);
        });

        // 6b. The REAL rebuild sequence: reuse_buffer then measure the SAME
        // text — with shape-run-cache the measure's runs should be warm.
        {
            let mut buf = create_buffer(&uniq[0], 14.0, 1.5, 400, None, None, TextAlign::Start);
            bench("reuse_buffer + measure (rebuild sequence)", |i| {
                reuse_buffer(
                    &mut buf,
                    &uniq[i],
                    14.0,
                    1.5,
                    400,
                    None,
                    None,
                    TextAlign::Start,
                );
                let w = measure_text_width(&uniq[i], 14.0, 400, None);
                std::hint::black_box((&buf, w));
            });
        }

        // 7a/7b. Shaping mode split: Basic vs Advanced on a fresh buffer.
        bench("raw set_text+shape Advanced (unique)", |i| {
            FONT_SYSTEM.with(|fs_cell| {
                let mut fs_opt = fs_cell.borrow_mut();
                let fs = fs_opt.as_mut().expect("font system not initialized");
                let m = cosmic_text::Metrics::new(14.0, 21.0);
                let mut b = cosmic_text::Buffer::new(&mut *fs, m);
                b.set_size(None, Some(2100.0));
                b.set_text(
                    &uniq[i],
                    &cosmic_text::Attrs::new().family(cosmic_text::Family::SansSerif),
                    cosmic_text::Shaping::Advanced,
                    None,
                );
                b.shape_until_scroll(&mut *fs, false);
                std::hint::black_box(&b);
            });
        });
        bench("raw set_text+shape Basic (unique)", |i| {
            FONT_SYSTEM.with(|fs_cell| {
                let mut fs_opt = fs_cell.borrow_mut();
                let fs = fs_opt.as_mut().expect("font system not initialized");
                let m = cosmic_text::Metrics::new(14.0, 21.0);
                let mut b = cosmic_text::Buffer::new(&mut *fs, m);
                b.set_size(None, Some(2100.0));
                b.set_text(
                    &uniq[i],
                    &cosmic_text::Attrs::new().family(cosmic_text::Family::SansSerif),
                    cosmic_text::Shaping::Basic,
                    None,
                );
                b.shape_until_scroll(&mut *fs, false);
                std::hint::black_box(&b);
            });
        });

        // 8. set_text only (no shape_until_scroll) — line splitting cost.
        bench("raw set_text only, no shape (unique)", |i| {
            FONT_SYSTEM.with(|fs_cell| {
                let mut fs_opt = fs_cell.borrow_mut();
                let fs = fs_opt.as_mut().expect("font system not initialized");
                let m = cosmic_text::Metrics::new(14.0, 21.0);
                let mut b = cosmic_text::Buffer::new(&mut *fs, m);
                b.set_size(None, Some(2100.0));
                b.set_text(
                    &uniq[i],
                    &cosmic_text::Attrs::new().family(cosmic_text::Family::SansSerif),
                    cosmic_text::Shaping::Advanced,
                    None,
                );
                std::hint::black_box(&b);
            });
        });
        println!();
    }
}

/// Equivalence check for the planned single-line fast path: for texts whose
/// shaped buffer has exactly one layout run, the max glyph extent read from
/// the buffer must equal measure_text_width's fresh-shape result.
#[test]
#[ignore]
fn single_line_width_from_buffer_equals_measure() {
    let samples = [
        "OK",
        "Cancel",
        "Row 4123",
        "cell 17 frame 99",
        "the quick brown fox",
        "line 42 - the quick brown fox jumps",
        "1234567890",
        "Save As...",
        "混合中文 English text 123",
    ];
    let mut max_dev = 0.0f32;
    for t in samples {
        let buf = create_buffer(t, 14.0, 1.5, 400, None, None, TextAlign::Start);
        let runs: Vec<_> = buf.layout_runs().collect();
        assert_eq!(runs.len(), 1, "expected single line for {t:?}");
        let (mut min_x, mut max_x) = (f32::MAX, 0.0f32);
        for g in runs[0].glyphs.iter() {
            min_x = min_x.min(g.x);
            max_x = max_x.max(g.x + g.w);
        }
        let from_buf = if min_x < max_x {
            (max_x - min_x) + 2.0
        } else {
            16.0
        };
        let from_measure = measure_text_width(t, 14.0, 400, None);
        let dev = (from_buf - from_measure).abs();
        max_dev = max_dev.max(dev);
        println!("  {t:<40} buf={from_buf:>8.2} measure={from_measure:>8.2} dev={dev:.3}");
        assert!(
            dev < 0.5,
            "width divergence for {t:?}: {from_buf} vs {from_measure}"
        );
    }
    println!("  max deviation: {max_dev:.4}px");
}
