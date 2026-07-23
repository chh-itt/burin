//! CPU backend pixel-format regression tests (audit 2026-07-16).
//!
//! Locks the canonical-format contract: `TinySkiaRenderer::pixels` is
//! tiny-skia premultiplied RGBA (`u32 = A<<24|B<<16|G<<8|R`), and
//! `present`/`softbuffer_pixels` converts to softbuffer `0RGB`
//! (`R` in bits 16-23). Before the audit the buffer mixed two layouts and
//! every tiny-skia-drawn primitive displayed with red/blue swapped.
#![cfg(feature = "backend-tiny-skia")]

use burin::core::ElementId;
use burin::render::wgpu::glyphon_bridge::{self, TextAreaDesc};
use burin::render::{ClipInfo, DrawCommand, TinySkiaRenderer};
use burin::style::{Color, CornerRadii, Rect, TextAlign};
use std::cell::RefCell;
use std::rc::Rc;

const W: f32 = 64.0;
const H: f32 = 64.0;

fn sb_rgb(px: u32) -> (u32, u32, u32) {
    ((px >> 16) & 0xFF, (px >> 8) & 0xFF, px & 0xFF)
}

fn full_clip() -> ClipInfo {
    ClipInfo::new(Rect::new(0.0, 0.0, W, H))
}

fn full_damage() -> Vec<Rect> {
    vec![Rect::new(0.0, 0.0, W, H)]
}

fn renderer() -> TinySkiaRenderer {
    let mut r = TinySkiaRenderer::new_headless(W, H, 1.0);
    r.set_clear_color(Color::rgba8(0, 0, 0, 255));
    r
}

#[test]
fn clear_color_reaches_softbuffer_unswapped() {
    let mut r = renderer();
    r.set_clear_color(Color::rgba8(10, 200, 30, 255));
    r.render_damage(&full_damage(), &mut vec![], &mut vec![], &[]);
    let px = r.softbuffer_pixels()[32 * 64 + 32];
    let (red, green, blue) = sb_rgb(px);
    assert_eq!(
        (red, green, blue),
        (10, 200, 30),
        "clear color swapped: got 0x{px:08X}"
    );
}

#[test]
fn fill_rect_reaches_softbuffer_unswapped() {
    let mut r = renderer();
    let mut cmds = vec![DrawCommand::FillRect {
        rect: Rect::new(0.0, 0.0, W, H),
        color: Color::rgba8(255, 0, 0, 255),
        radius: CornerRadii::ZERO,
        clip: full_clip(),
        transform: glam::Affine2::IDENTITY,
        z_index: 0,
        blend_mode: 0,
    }];
    r.render_damage(&full_damage(), &mut cmds, &mut vec![], &[]);
    let (red, green, blue) = sb_rgb(r.softbuffer_pixels()[32 * 64 + 32]);
    assert!(
        red > 250 && green < 5 && blue < 5,
        "red rect displayed as R={red} G={green} B={blue}"
    );
}

#[test]
fn rounded_rect_and_gradient_unswapped() {
    let mut r = renderer();
    let mut cmds = vec![
        DrawCommand::FillRect {
            rect: Rect::new(0.0, 0.0, 32.0, 64.0),
            color: Color::rgba8(0, 0, 255, 255),
            radius: CornerRadii::all(4.0),
            clip: full_clip(),
            transform: glam::Affine2::IDENTITY,
            z_index: 0,
            blend_mode: 0,
        },
        DrawCommand::FillLinearGradient {
            rect: Rect::new(32.0, 0.0, 32.0, 64.0),
            gradient: burin::style::LinearGradient::new(
                (0.0, 0.0),
                (0.0, 1.0),
                &[
                    (Color::rgba8(255, 0, 0, 255), 0.0),
                    (Color::rgba8(255, 0, 0, 255), 1.0),
                ],
            ),
            radius: CornerRadii::ZERO,
            stroke_width: 0.0,
            clip: full_clip(),
            transform: glam::Affine2::IDENTITY,
            z_index: 0,
        },
    ];
    r.render_damage(&full_damage(), &mut cmds, &mut vec![], &[]);
    let sb = r.softbuffer_pixels();
    let (lr, _, lb) = sb_rgb(sb[32 * 64 + 16]); // blue rounded rect interior
    assert!(
        lb > 250 && lr < 5,
        "blue rounded rect displayed as R={lr} B={lb}"
    );
    let (rr, _, rb) = sb_rgb(sb[32 * 64 + 48]); // red gradient
    assert!(
        rr > 250 && rb < 5,
        "red gradient displayed as R={rr} B={rb}"
    );
}

fn red_text_area(generation: u64) -> TextAreaDesc {
    let buffer =
        glyphon_bridge::create_buffer("██████", 24.0, 1.2, 700, None, Some(60.0), TextAlign::Left);
    TextAreaDesc {
        buffer: Rc::new(RefCell::new(buffer)),
        element_id: ElementId::SENTINEL,
        generation,
        left: 2.0,
        top: 2.0,
        scale: 1.0,
        color: Color::rgba8(255, 0, 0, 255),
        scroll_x: 0.0,
        scroll_y: 0.0,
        clip_rect: Some(Rect::new(0.0, 0.0, W, H)),
        z_index: 0,
    }
}

fn assert_red_text(r: &TinySkiaRenderer, path: &str) {
    let sb = r.softbuffer_pixels();
    let mut found = false;
    for &px in &sb {
        let (red, _g, blue) = sb_rgb(px);
        if red > 150 && blue < 60 {
            found = true;
            break;
        }
    }
    assert!(
        found,
        "{path}: no red text pixel found — text channels swapped or text missing"
    );
}

#[test]
fn text_atlas_path_unswapped() {
    let mut r = renderer();
    let mut tas = vec![red_text_area(0)]; // generation 0 → per-glyph atlas path
    r.render_damage(&full_damage(), &mut vec![], &mut tas, &[]);
    assert_red_text(&r, "atlas path");
}

#[test]
fn text_surface_cache_path_unswapped() {
    let mut r = renderer();
    let mut tas = vec![red_text_area(1)]; // generation > 0 → surface cache path
    r.render_damage(&full_damage(), &mut vec![], &mut tas, &[]);
    assert_red_text(&r, "surface path");
}

#[test]
fn image_path_unswapped_and_clipped() {
    // 8×8 pure-red source image.
    let hash = 0xC0FFEE_u64;
    let mut data = Vec::with_capacity(8 * 8 * 4);
    for _ in 0..(8 * 8) {
        data.extend_from_slice(&[255u8, 0, 0, 255]);
    }
    burin::render::wgpu::register_image(hash, 8, 8, Rc::new(data));

    let mut r = renderer();
    let mut cmds = vec![DrawCommand::DrawImage {
        hash,
        rect: Rect::new(8.0, 8.0, 32.0, 32.0),
        opacity: 1.0,
        content_fit: burin::widgets::display::ContentFit::Fill,
        clip: ClipInfo::new(Rect::new(0.0, 0.0, 24.0, 24.0)), // clips right/bottom half
        transform: glam::Affine2::IDENTITY,
        z_index: 0,
    }];
    r.render_damage(&full_damage(), &mut cmds, &mut vec![], &[]);
    let sb = r.softbuffer_pixels();
    let (red, _g, blue) = sb_rgb(sb[16 * 64 + 16]); // inside image ∩ clip
    assert!(red > 250 && blue < 5, "image displayed as R={red} B={blue}");
    let (cr, _cg, _cb) = sb_rgb(sb[30 * 64 + 30]); // inside image, outside clip
    assert!(
        cr < 5,
        "image not clipped by ancestor clip rect (R={cr} at clipped pixel)"
    );
}
