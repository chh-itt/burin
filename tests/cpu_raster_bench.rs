//! CPU raster throughput benchmark — permanent regression bench.
//!
//! Measures the REAL production raster path (`TinySkiaRenderer::render_damage`
//! headless, including clip/shadow caches and text) across the primitive mix.
//! Run:
//!   cargo test --release --test cpu_raster_bench -- --ignored --nocapture --test-threads 1
//!
//! Baseline history: docs/perf/cpu-audit-2026-07-16.md
#![cfg(feature = "backend-tiny-skia")]

use burin::core::context::MountContext;
use burin::core::element::ElementId as EId;
use burin::core::widget::Widget;
use burin::core::ElementId;
use burin::render::wgpu::glyphon_bridge::{self, TextAreaDesc};
use burin::render::{ClipInfo, DrawCommand, TinySkiaRenderer};
use burin::style::{Color, CornerRadii, Dimension, LinearGradient, Rect, Styled, TextAlign};
use burin::testing::TestHarness;
use burin::widgets::display::Text;
use burin::widgets::layout::{HStack, ScrollView, VStack};
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Instant;

fn renderer(w: f32, h: f32) -> TinySkiaRenderer {
    let mut r = TinySkiaRenderer::new_headless(w, h, 1.0);
    r.set_clear_color(Color::rgba8(26, 26, 31, 255));
    r
}

/// Time `frames` render_damage calls (after 1 warmup frame = cold caches
/// excluded → steady-state per-frame cost). Returns ms per frame.
fn bench_frames(
    r: &mut TinySkiaRenderer,
    cmds: &[DrawCommand],
    tas: &[TextAreaDesc],
    damage: &[Rect],
    frames: u32,
) -> f64 {
    let mut c = cmds.to_vec();
    let mut t = tas.to_vec();
    r.render_damage(damage, &mut c, &mut t, &[]); // warmup / cold
    r.end_frame();
    let start = Instant::now();
    for _ in 0..frames {
        let mut c = cmds.to_vec();
        let mut t = tas.to_vec();
        r.render_damage(damage, &mut c, &mut t, &[]);
        r.end_frame();
    }
    start.elapsed().as_secs_f64() * 1000.0 / frames as f64
}

fn grid(count: usize, w: f32, h: f32) -> Vec<(f32, f32, f32, f32)> {
    let cols = (count as f32).sqrt().ceil() as usize;
    let cw = w / cols as f32;
    let ch = h / cols as f32;
    (0..count)
        .map(|i| ((i % cols) as f32 * cw, (i / cols) as f32 * ch, cw, ch))
        .collect()
}

fn solid_rects(count: usize, w: f32, h: f32, rounded: bool) -> Vec<DrawCommand> {
    grid(count, w, h)
        .into_iter()
        .enumerate()
        .map(|(i, (x, y, cw, ch))| DrawCommand::FillRect {
            rect: Rect::new(x, y, cw, ch),
            color: Color::rgba8((i % 255) as u8, 120, 200, 255),
            radius: if rounded {
                CornerRadii::all(8.0)
            } else {
                CornerRadii::ZERO
            },
            clip: ClipInfo::new(Rect::new(0.0, 0.0, w, h)),
            transform: glam::Affine2::IDENTITY,
            z_index: 0,
            blend_mode: 0,
        })
        .collect()
}

fn clipped_rects(count: usize, w: f32, h: f32) -> Vec<DrawCommand> {
    // Each command under a DIFFERENT rounded clip covering its own cell —
    // the mask-stress case (N distinct rounded scroll/overflow containers).
    grid(count, w, h)
        .into_iter()
        .enumerate()
        .map(|(i, (x, y, cw, ch))| DrawCommand::FillRect {
            rect: Rect::new(x, y, cw, ch),
            color: Color::rgba8(200, 120, (i % 255) as u8, 255),
            radius: CornerRadii::ZERO,
            clip: ClipInfo::with_radius(Rect::new(x, y, cw, ch), CornerRadii::all(6.0)),
            transform: glam::Affine2::IDENTITY,
            z_index: 0,
            blend_mode: 0,
        })
        .collect()
}

fn shadows(count: usize, w: f32, h: f32) -> Vec<DrawCommand> {
    grid(count, w, h)
        .into_iter()
        .map(|(x, y, cw, ch)| DrawCommand::FillShadow {
            rect: Rect::new(x + 12.0, y + 12.0, cw - 24.0, ch - 24.0),
            color: Color::rgba8(0, 0, 0, 40),
            radius: CornerRadii::all(12.0),
            shadow: burin::style::styled::Shadow {
                color: Color::rgba8(0, 0, 0, 40),
                offset_x: 0.0,
                offset_y: 4.0,
                blur: 12.0,
            },
            elem_size: (cw - 24.0, ch - 24.0),
            clip: ClipInfo::new(Rect::new(0.0, 0.0, w, h)),
            transform: glam::Affine2::IDENTITY,
            z_index: 0,
        })
        .collect()
}

fn gradients(count: usize, w: f32, h: f32, _kind: burin::style::GradientKind) -> Vec<DrawCommand> {
    grid(count, w, h)
        .into_iter()
        .map(|(x, y, cw, ch)| DrawCommand::FillLinearGradient {
            rect: Rect::new(x, y, cw, ch),
            gradient: LinearGradient::new(
                (0.0, 0.0),
                (1.0, 1.0),
                &[
                    (Color::rgba8(255, 0, 100, 255), 0.0),
                    (Color::rgba8(0, 100, 255, 255), 1.0),
                ],
            ),
            radius: CornerRadii::all(8.0),
            stroke_width: 0.0,
            clip: ClipInfo::new(Rect::new(0.0, 0.0, w, h)),
            transform: glam::Affine2::IDENTITY,
            z_index: 0,
        })
        .collect()
}

fn text_areas(count: usize, w: f32, h: f32, generation: u64) -> Vec<TextAreaDesc> {
    let buffer = Rc::new(RefCell::new(glyphon_bridge::create_buffer(
        "The quick brown fox 0123456789",
        14.0,
        1.3,
        400,
        None,
        Some(w),
        TextAlign::Left,
    )));
    grid(count, w, h)
        .into_iter()
        .enumerate()
        .map(|(i, (x, y, cw, ch))| TextAreaDesc {
            buffer: buffer.clone(),
            element_id: ElementId::SENTINEL,
            generation: if generation > 0 {
                generation + i as u64
            } else {
                0
            },
            left: x + 2.0,
            top: y + 2.0,
            scale: 1.0,
            color: Color::rgba8(230, 230, 235, 255),
            scroll_x: 0.0,
            scroll_y: 0.0,
            clip_rect: Some(Rect::new(x, y, cw, ch)),
            z_index: 0,
        })
        .collect()
}

/// Simulated scroll: rows inside ONE rounded clip container; each frame the
/// transform translates by one more row (device clip stable → mask cache
/// should hit; rows fully inside → tier-1 unclipped).
fn bench_scroll(r: &mut TinySkiaRenderer, w: f32, h: f32, frames: u32) -> f64 {
    let row_h = 28.0f32;
    let n_rows = ((h / row_h) as usize) + 2;
    let clip = ClipInfo::with_radius(
        Rect::new(8.0, 8.0, w - 16.0, h - 16.0),
        CornerRadii::all(6.0),
    );
    let build = |offset: f32| -> Vec<DrawCommand> {
        let xf = glam::Affine2::from_translation(glam::Vec2::new(0.0, -offset));
        (0..n_rows)
            .map(|i| DrawCommand::FillRect {
                rect: Rect::new(
                    8.0,
                    8.0 + offset.floor() + i as f32 * row_h,
                    w - 16.0,
                    row_h - 2.0,
                ),
                color: if i % 2 == 0 {
                    Color::rgba8(40, 40, 48, 255)
                } else {
                    Color::rgba8(50, 50, 60, 255)
                },
                radius: CornerRadii::ZERO,
                clip,
                transform: xf,
                z_index: 0,
                blend_mode: 0,
            })
            .collect()
    };
    let mut warm = build(0.0);
    r.render_damage(&[Rect::new(0.0, 0.0, w, h)], &mut warm, &mut vec![], &[]);
    r.end_frame();
    let start = Instant::now();
    for f in 0..frames {
        let mut cmds = build(f as f32 * 3.0);
        r.render_damage(&[Rect::new(0.0, 0.0, w, h)], &mut cmds, &mut vec![], &[]);
        r.end_frame();
    }
    start.elapsed().as_secs_f64() * 1000.0 / frames as f64
}

#[test]
#[ignore]
fn raster_throughput() {
    for (pw, ph) in [(960u32, 900u32), (1920, 1080), (2560, 1440)] {
        let w = pw as f32;
        let h = ph as f32;
        let full = vec![Rect::new(0.0, 0.0, w, h)];
        let frames = 20;

        let mut r = renderer(w, h);
        let t_clear = bench_frames(&mut r, &[], &[], &full, frames);
        let t100 = bench_frames(&mut r, &solid_rects(100, w, h, false), &[], &full, frames);
        let t1000 = bench_frames(&mut r, &solid_rects(1000, w, h, false), &[], &full, frames);
        let t1000r = bench_frames(&mut r, &solid_rects(1000, w, h, true), &[], &full, frames);
        let tclip = bench_frames(&mut r, &clipped_rects(100, w, h), &[], &full, frames);
        let tsh = bench_frames(&mut r, &shadows(50, w, h), &[], &full, frames);
        let tgr = bench_frames(
            &mut r,
            &gradients(50, w, h, burin::style::GradientKind::Linear),
            &[],
            &full,
            frames,
        );
        let ttx_atlas = bench_frames(&mut r, &[], &text_areas(50, w, h, 0), &full, frames);
        let ttx_surf = bench_frames(&mut r, &[], &text_areas(50, w, h, 1), &full, frames);
        let tscroll = bench_scroll(&mut r, w, h, 60);

        println!(
            "== {}x{} (steady-state ms/frame, full-frame damage) ==",
            pw, ph
        );
        println!("  clear only               : {t_clear:.2}");
        println!("  100 solid rects          : {t100:.2}");
        println!("  1000 solid rects         : {t1000:.2}");
        println!("  1000 rounded rects       : {t1000r:.2}");
        println!("  100 rects, 100 rnd clips : {tclip:.2}");
        println!("  50 shadows (blur 12)     : {tsh:.2}");
        println!("  50 gradients             : {tgr:.2}");
        println!("  50 texts (atlas path)    : {ttx_atlas:.2}");
        println!("  50 texts (surface path)  : {ttx_surf:.2}");
        println!("  scroll sim (rows+clip)   : {tscroll:.2}");
        // Swizzle micro-probe: the present() boundary conversion (R/B swap).
        let swizzle_ms = bench_cmds_swizzle(pw * ph);
        println!(
            "  full-frame swizzle (memcpy+swap) : {swizzle_ms:.2} ms  ({:.0} MPx/s)",
            (pw * ph) as f64 / (swizzle_ms * 1000.0)
        );
    }
}
fn bench_cmds_swizzle(n: u32) -> f64 {
    let mut pixels: Vec<u32> = vec![0xAABBCCDD_u32; n as usize];
    let reps = 5;
    let t = Instant::now();
    for _ in 0..reps {
        let mut work = pixels.clone();
        for p in &mut work {
            *p = ((*p) & 0xFF00_FF00) | (((*p) & 0x0000_00FF) << 16) | ((*p >> 16) & 0x0000_00FF);
        }
        pixels = work;
    }
    t.elapsed().as_secs_f64() * 1000.0 / reps as f64
}

// ── Real-workload bench: representative page driven via TestHarness ──

fn px(v: f32) -> Dimension {
    Dimension::Pixels(v)
}

struct BoxedWidget(Box<dyn Widget>);
impl Widget for BoxedWidget {
    fn mount_box(self: Box<Self>, ctx: &mut MountContext<'_>) -> EId {
        self.0.mount_box(ctx)
    }
}

/// 80 zebra text rows, every 8th a shadowed rounded card, in a ScrollView.
fn demo_page(w: f32, h: f32) -> impl Widget {
    let mut col = VStack::new().width(px(w)).gap(0.0);
    for i in 0..80 {
        let zebra = if i % 2 == 0 {
            Color::rgba8(40, 40, 48, 255)
        } else {
            Color::rgba8(50, 50, 60, 255)
        };
        let row: Box<dyn Widget> = if i % 8 == 0 {
            Box::new(
                HStack::new()
                    .width(px(w - 32.0))
                    .height(px(64.0))
                    .background(Color::rgba8(60, 60, 72, 255))
                    .corner_radius(CornerRadii::all(12.0))
                    .shadow(Color::rgba8(0, 0, 0, 60), 0.0, 4.0, 12.0)
                    .push(Text::new(format!("Card section {i}")))
                    .push(Text::new("detail value")),
            )
        } else {
            Box::new(
                HStack::new()
                    .width(px(w))
                    .height(px(36.0))
                    .background(zebra)
                    .push(Text::new(format!("Row item number {i} with a label")))
                    .push(Text::new(format!("{}", i * 137))),
            )
        };
        col = col.push(BoxedWidget(row));
    }
    ScrollView::new().child(col).width(px(w)).height(px(h))
}

fn find_leaf(h: &TestHarness, id: EId) -> Option<EId> {
    let el = h.find(id)?;
    if el.children.is_empty() {
        return Some(id);
    }
    for &c in &el.children.clone() {
        if let Some(t) = find_leaf(h, c) {
            return Some(t);
        }
    }
    None
}

#[test]
#[ignore]
fn real_page_scroll() {
    let (w, h) = (1280.0f32, 960.0f32);
    let mut harness = TestHarness::new(w, h);
    harness.mount(demo_page(w, h));
    harness.run_frame();
    let leaf = find_leaf(&harness, harness.root_id()).expect("leaf");

    let mut r = renderer(w, h);
    let full = vec![Rect::new(0.0, 0.0, w, h)];
    harness.scroll(leaf, 0.0, -3.0);
    harness.run_frame();
    let mut cmds = harness.last_scene.clone();
    let mut tas = harness.last_text_areas.clone();
    r.render_damage(&full, &mut cmds, &mut tas, &[]);
    r.end_frame();

    let frames = 60;
    let mut gen_ms = 0.0f64;
    let mut raster_ms = 0.0f64;
    for _ in 0..frames {
        let t0 = Instant::now();
        harness.scroll(leaf, 0.0, -4.0);
        harness.run_frame();
        gen_ms += t0.elapsed().as_secs_f64() * 1000.0;
        let mut cmds = harness.last_scene.clone();
        let mut tas = harness.last_text_areas.clone();
        let t1 = Instant::now();
        r.render_damage(&full, &mut cmds, &mut tas, &[]);
        r.end_frame();
        raster_ms += t1.elapsed().as_secs_f64() * 1000.0;
    }
    println!(
        "real-page scroll @{w}x{h}: gen {:.2} ms/frame, raster {:.2} ms/frame ({} cmds, {} texts)",
        gen_ms / frames as f64,
        raster_ms / frames as f64,
        harness.last_scene.len(),
        harness.last_text_areas.len(),
    );
}
