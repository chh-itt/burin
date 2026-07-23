//! CPU backend rendering-semantics regression tests (audit 2026-07-16).
//!
//! Locks the fixes for:
//! - C2: gradients must follow the element transform (scroll)
//! - C3: blend modes (multiply/screen/overlay) on the CPU backend
//! - C4: backdrop-filter blur
//! - C5: no double-blending of translucent commands across damage rects
//! - C7: text surface cache invalidates on colour-only changes
//! - shadow: offset applied once, smooth monotonic falloff, 9-patch symmetry
#![cfg(feature = "backend-tiny-skia")]

use burin::core::ElementId;
use burin::render::wgpu::glyphon_bridge::{self, TextAreaDesc};
use burin::render::{BackdropRegion, ClipInfo, DrawCommand, TinySkiaRenderer};
use burin::style::{Color, CornerRadii, Rect, TextAlign};
use std::cell::RefCell;
use std::rc::Rc;

fn sb_rgb(px: u32) -> (u32, u32, u32) {
    ((px >> 16) & 0xFF, (px >> 8) & 0xFF, px & 0xFF)
}

fn renderer(w: f32, h: f32) -> TinySkiaRenderer {
    let mut r = TinySkiaRenderer::new_headless(w, h, 1.0);
    r.set_clear_color(Color::rgba8(0, 0, 0, 255));
    r
}

fn full_clip(w: f32, h: f32) -> ClipInfo {
    ClipInfo::new(Rect::new(0.0, 0.0, w, h))
}

fn fill(rect: Rect, color: Color, clip: ClipInfo, xf: glam::Affine2, blend: u8) -> DrawCommand {
    DrawCommand::FillRect {
        rect,
        color,
        radius: CornerRadii::ZERO,
        clip,
        transform: xf,
        z_index: 0,
        blend_mode: blend,
    }
}

// ── C2: gradients follow the element transform ───────────────────────

#[test]
fn gradient_follows_scroll_transform() {
    let (w, h) = (64.0, 64.0);
    let mut r = renderer(w, h);
    // Two hard stops: left half red, right half blue, on a 32-wide rect at
    // x=0 in raw space, translated +24 by the transform (≙ scrolled content).
    let grad = burin::style::LinearGradient::new(
        (0.0, 0.5),
        (1.0, 0.5),
        &[
            (Color::rgba8(255, 0, 0, 255), 0.0),
            (Color::rgba8(255, 0, 0, 255), 0.5),
            (Color::rgba8(0, 0, 255, 255), 0.5),
            (Color::rgba8(0, 0, 255, 255), 1.0),
        ],
    );
    let mut cmds = vec![DrawCommand::FillLinearGradient {
        rect: Rect::new(0.0, 16.0, 32.0, 32.0),
        gradient: grad,
        radius: CornerRadii::ZERO,
        stroke_width: 0.0,
        clip: full_clip(w, h),
        transform: glam::Affine2::from_translation(glam::Vec2::new(24.0, 0.0)),
        z_index: 0,
    }];
    r.render_damage(&[Rect::new(0.0, 0.0, w, h)], &mut cmds, &mut vec![], &[]);
    let sb = r.softbuffer_pixels();
    // Untransformed position (x=4) must be background (black).
    let (br, _, bb) = sb_rgb(sb[32 * 64 + 4]);
    assert!(
        br < 10 && bb < 10,
        "gradient painted at unscrolled position (pre-audit bug)"
    );
    // Transformed position: red at x=24+8, blue at x=24+24.
    let (rr, _, rb) = sb_rgb(sb[32 * 64 + 32]);
    assert!(
        rr > 200 && rb < 50,
        "left gradient half not red at transformed pos: R={rr} B={rb}"
    );
    let (lr, _, lb) = sb_rgb(sb[32 * 64 + 48]);
    assert!(
        lb > 200 && lr < 50,
        "right gradient half not blue at transformed pos: R={lr} B={lb}"
    );
}

// ── C3: blend modes ──────────────────────────────────────────────────

#[test]
fn blend_mode_multiply_and_screen() {
    let (w, h) = (64.0, 64.0);
    let mut r = renderer(w, h);
    let mut cmds = vec![
        // White base on left half, black base on right half (z 0).
        fill(
            Rect::new(0.0, 0.0, 32.0, 64.0),
            Color::rgba8(255, 255, 255, 255),
            full_clip(w, h),
            glam::Affine2::IDENTITY,
            0,
        ),
        // Multiply red over the white half: white × red = red.
        DrawCommand::FillRect {
            rect: Rect::new(0.0, 0.0, 32.0, 64.0),
            color: Color::rgba8(255, 0, 0, 255),
            radius: CornerRadii::ZERO,
            clip: full_clip(w, h),
            transform: glam::Affine2::IDENTITY,
            z_index: 1,
            blend_mode: 1, // Multiply
        },
        // Screen green over the black half: black ∨ green = green.
        DrawCommand::FillRect {
            rect: Rect::new(32.0, 0.0, 32.0, 64.0),
            color: Color::rgba8(0, 255, 0, 255),
            radius: CornerRadii::ZERO,
            clip: full_clip(w, h),
            transform: glam::Affine2::IDENTITY,
            z_index: 1,
            blend_mode: 2, // Screen
        },
    ];
    r.render_damage(&[Rect::new(0.0, 0.0, w, h)], &mut cmds, &mut vec![], &[]);
    let sb = r.softbuffer_pixels();
    let (mr, mg, mb) = sb_rgb(sb[32 * 64 + 16]);
    assert!(
        mr > 200 && mg < 30 && mb < 30,
        "multiply(white, red) should be red, got R={mr} G={mg} B={mb}"
    );
    let (sr, sg, _) = sb_rgb(sb[32 * 64 + 48]);
    assert!(
        sg > 200 && sr < 30,
        "screen(black, green) should be green, got R={sr} G={sg}"
    );
}

// ── C5: no double-blend across damage rects ──────────────────────────

#[test]
fn translucent_rect_not_double_blended_across_damage_rects() {
    let (w, h) = (64.0, 64.0);
    let mut r = renderer(w, h);
    // 50%-alpha white rect spanning both damage rects.
    let mut cmds = vec![fill(
        Rect::new(0.0, 0.0, 64.0, 64.0),
        Color::rgba8(255, 255, 255, 128),
        full_clip(w, h),
        glam::Affine2::IDENTITY,
        0,
    )];
    // Two disjoint damage rects (left / right halves).
    let damage = vec![
        Rect::new(0.0, 0.0, 32.0, 64.0),
        Rect::new(32.0, 0.0, 32.0, 64.0),
    ];
    r.render_damage(&damage, &mut cmds, &mut vec![], &[]);
    let sb = r.softbuffer_pixels();
    let (l, _, _) = sb_rgb(sb[32 * 64 + 8]);
    let (rt, _, _) = sb_rgb(sb[32 * 64 + 56]);
    // Both halves must be single-blended (~128 over black) and EQUAL.
    assert!(
        (l as i32 - rt as i32).abs() <= 2,
        "halves differ (double blend on one side): left={l} right={rt}"
    );
    assert!(
        l > 100 && l < 160,
        "expected single 50% blend (~128), got {l}"
    );
}

// ── Shadow semantics ─────────────────────────────────────────────────

/// Find the darkest (max coverage) pixel column along a row.
fn column_alpha_profile(sb: &[u32], stride: usize, row: usize, w: usize) -> Vec<u32> {
    (0..w).map(|x| sb_rgb(sb[row * stride + x]).0).collect()
}

#[test]
fn shadow_offset_applied_once() {
    let (w, h) = (128.0, 128.0);
    let mut r = renderer(w, h);
    r.set_clear_color(Color::rgba8(255, 255, 255, 255));
    // Element 40×40 at (44, 44); shadow offset +20 x, blur 8.
    // Painter semantics: FillShadow rect = elem + offset, expanded by blur.
    let blur = 8.0f32;
    let (ex, ey, ew, eh) = (44.0f32, 44.0f32, 40.0f32, 40.0f32);
    let (ox, oy) = (20.0f32, 0.0f32);
    let sr = Rect::new(
        ex + ox - blur,
        ey + oy - blur,
        ew + blur * 2.0,
        eh + blur * 2.0,
    );
    let mut cmds = vec![DrawCommand::FillShadow {
        rect: sr,
        color: Color::rgba8(0, 0, 0, 255),
        radius: CornerRadii::ZERO,
        shadow: burin::style::styled::Shadow {
            color: Color::rgba8(0, 0, 0, 255),
            offset_x: ox,
            offset_y: oy,
            blur,
        },
        elem_size: (ew, eh),
        clip: full_clip(w, h),
        transform: glam::Affine2::IDENTITY,
        z_index: 0,
    }];
    r.render_damage(&[Rect::new(0.0, 0.0, w, h)], &mut cmds, &mut vec![], &[]);
    let sb = r.softbuffer_pixels();
    // The shadow core (full darkness) must be centred at elem + offset
    // (x ∈ [64+8, 104-8] fully dark), NOT at elem + 2×offset (pre-audit bug).
    let row = 64usize;
    let profile = column_alpha_profile(&sb, 128, row, 128);
    // Fully dark inside the offset element core:
    assert!(
        profile[84] < 10,
        "shadow core should be dark at x=84, got {}",
        profile[84]
    );
    // At x = elem_x (44) + 2*offset (40) + ew = 124-ish nothing; more precise:
    // the pre-audit double offset would leave x=70 (left edge of correct
    // shadow area) light. Correct: x=70 inside core (64..96) → dark.
    assert!(
        profile[70] < 30,
        "left of shadow core should be dark (single offset), got {}",
        profile[70]
    );
    // Beyond the correct right edge + blur (104+8=112) must be white.
    assert!(
        profile[120] > 240,
        "beyond shadow must be background, got {}",
        profile[120]
    );
}

#[test]
fn shadow_falloff_monotonic_and_symmetric() {
    let (w, h) = (160.0, 160.0);
    let mut r = renderer(w, h);
    r.set_clear_color(Color::rgba8(255, 255, 255, 255));
    let blur = 12.0f32;
    let (ex, ey, ew, eh) = (40.0f32, 40.0f32, 80.0f32, 80.0f32); // big → 9-patch
    let sr = Rect::new(ex - blur, ey - blur, ew + blur * 2.0, eh + blur * 2.0);
    let mut cmds = vec![DrawCommand::FillShadow {
        rect: sr,
        color: Color::rgba8(0, 0, 0, 255),
        radius: CornerRadii::all(8.0),
        shadow: burin::style::styled::Shadow {
            color: Color::rgba8(0, 0, 0, 255),
            offset_x: 0.0,
            offset_y: 0.0,
            blur,
        },
        elem_size: (ew, eh),
        clip: full_clip(w, h),
        transform: glam::Affine2::IDENTITY,
        z_index: 0,
    }];
    r.render_damage(&[Rect::new(0.0, 0.0, w, h)], &mut cmds, &mut vec![], &[]);
    let sb = r.softbuffer_pixels();
    let mid = 80usize;
    let profile = column_alpha_profile(&sb, 160, mid, 160);
    // Monotonic darkening approaching the element from the left edge strip.
    for x in 20..40 {
        assert!(
            profile[x] + 1 >= profile[x + 1],
            "falloff not monotonic at x={x}: {} -> {}",
            profile[x],
            profile[x + 1]
        );
    }
    // Horizontal symmetry (left vs right edge strips of the 9-patch).
    // Element spans x ∈ [40, 120): the mirror of pixel 40 is pixel 119.
    for d in 0..20 {
        let l = profile[40 - d] as i32;
        let r_ = profile[119 + d] as i32;
        assert!(
            (l - r_).abs() <= 6,
            "9-patch asymmetry at ±{d}: left={l} right={r_}"
        );
    }
    // Core fully dark.
    assert!(
        profile[80] < 8,
        "shadow core should be fully dark, got {}",
        profile[80]
    );
}

// ── C4: backdrop blur ────────────────────────────────────────────────

#[test]
fn backdrop_blur_mixes_content_below() {
    let (w, h) = (64.0, 64.0);
    let mut r = renderer(w, h);
    // Sharp black/white halves below; blur region across the boundary.
    let mut cmds = vec![fill(
        Rect::new(0.0, 0.0, 32.0, 64.0),
        Color::rgba8(255, 255, 255, 255),
        full_clip(w, h),
        glam::Affine2::IDENTITY,
        0,
    )];
    let regions = vec![BackdropRegion {
        rect: Rect::new(16.0, 16.0, 32.0, 32.0),
        transform: glam::Affine2::IDENTITY,
        corner_radius: CornerRadii::ZERO,
        blur_radius: 8.0,
        tint: None,
        z_index: 1,
    }];
    r.render_damage(
        &[Rect::new(0.0, 0.0, w, h)],
        &mut cmds,
        &mut vec![],
        &regions,
    );
    let sb = r.softbuffer_pixels();
    // At the boundary inside the region, the value must be mid-grey (mixed).
    let (v, _, _) = sb_rgb(sb[32 * 64 + 32]);
    assert!(
        v > 60 && v < 200,
        "backdrop blur missing: boundary pixel not mixed, got {v}"
    );
    // Outside the region the boundary stays sharp.
    let (sharp_w, _, _) = sb_rgb(sb[4 * 64 + 30]);
    let (sharp_b, _, _) = sb_rgb(sb[4 * 64 + 34]);
    assert!(
        sharp_w > 240 && sharp_b < 15,
        "content outside backdrop region must stay sharp"
    );
}

// ── C7: text colour invalidates the surface cache ────────────────────

fn text_area(color: Color, generation: u64, eid: ElementId) -> TextAreaDesc {
    let buffer =
        glyphon_bridge::create_buffer("████", 20.0, 1.2, 700, None, Some(60.0), TextAlign::Left);
    TextAreaDesc {
        buffer: Rc::new(RefCell::new(buffer)),
        element_id: eid,
        generation,
        left: 2.0,
        top: 2.0,
        scale: 1.0,
        color,
        scroll_x: 0.0,
        scroll_y: 0.0,
        clip_rect: Some(Rect::new(0.0, 0.0, 64.0, 64.0)),
        z_index: 0,
    }
}

#[test]
fn text_surface_cache_invalidates_on_color_change() {
    let (w, h) = (64.0, 64.0);
    let mut r = renderer(w, h);
    let eid = ElementId::SENTINEL;
    // Frame 1: red text, generation 1 (surface cached).
    let mut tas = vec![text_area(Color::rgba8(255, 0, 0, 255), 1, eid)];
    r.render_damage(&[Rect::new(0.0, 0.0, w, h)], &mut vec![], &mut tas, &[]);
    // Frame 2: SAME generation, blue colour (state layer / theme switch).
    let mut tas = vec![text_area(Color::rgba8(0, 0, 255, 255), 1, eid)];
    r.render_damage(&[Rect::new(0.0, 0.0, w, h)], &mut vec![], &mut tas, &[]);
    let sb = r.softbuffer_pixels();
    let mut found_blue = false;
    let mut found_red = false;
    for &px in &sb {
        let (red, _, blue) = sb_rgb(px);
        if blue > 150 && red < 60 {
            found_blue = true;
        }
        if red > 150 && blue < 60 {
            found_red = true;
        }
    }
    assert!(found_blue, "text did not re-render in the new colour");
    assert!(
        !found_red,
        "stale red text still present (surface cache ignored colour)"
    );
}

// ── Banded rounded-rect fill (round 2, O1) ───────────────────────────

#[test]
fn banded_rounded_fill_is_seamless_and_symmetric() {
    let (w, h) = (400.0, 200.0);
    let mut r = renderer(w, h);
    // Large opaque rounded rect → banded fast path (bands + corner wedges).
    let mut cmds = vec![DrawCommand::FillRect {
        rect: Rect::new(20.0, 20.0, 360.0, 160.0),
        color: Color::rgba8(200, 60, 30, 255),
        radius: CornerRadii::all(16.0),
        clip: full_clip(w, h),
        transform: glam::Affine2::IDENTITY,
        z_index: 0,
        blend_mode: 0,
    }];
    r.render_damage(&[Rect::new(0.0, 0.0, w, h)], &mut cmds, &mut vec![], &[]);
    let sb = r.softbuffer_pixels();
    let stride = 400usize;
    // Interior (away from AA corners/edges) must be EXACTLY the fill colour —
    // any band/wedge seam would show as an off-colour line.
    for y in 40..160 {
        for x in 40..360 {
            let (red, green, blue) = sb_rgb(sb[y * stride + x]);
            assert!(
                red == 200 && green == 60 && blue == 30,
                "seam/hole at ({x},{y}): R={red} G={green} B={blue}"
            );
        }
    }
    // Rows crossing the corner-band boundary (y = 20+16 = 36) as well.
    for x in 40..360 {
        let (red, green, blue) = sb_rgb(sb[36 * stride + x]);
        assert!(
            red == 200 && green == 60 && blue == 30,
            "seam at corner-band boundary x={x}"
        );
    }
    // Corner AA symmetry: TL vs TR mirrored (rect spans x in [20, 380)).
    for d in 0..16 {
        let (l, _, _) = sb_rgb(sb[28 * stride + (24 + d)]);
        let (rr, _, _) = sb_rgb(sb[28 * stride + (375 - d)]);
        assert!(
            (l as i32 - rr as i32).abs() <= 6,
            "corner asymmetry at d={d}: left={l} right={rr}"
        );
    }
    // Outside the corner arc must remain background (black).
    let (or_, og_, ob_) = sb_rgb(sb[22 * stride + 22]);
    assert!(
        or_ < 60 && og_ < 60 && ob_ < 60,
        "corner not rounded: R={or_} G={og_} B={ob_}"
    );
}

// ── Axis-aligned gradient fast path (round 2, O3) ────────────────────

#[test]
fn axis_gradient_matches_expected_ramp() {
    let (w, h) = (64.0, 256.0);
    let mut r = renderer(w, h);
    let mut cmds = vec![DrawCommand::FillLinearGradient {
        rect: Rect::new(0.0, 0.0, 64.0, 256.0),
        gradient: burin::style::LinearGradient::new(
            (0.0, 0.0),
            (0.0, 1.0),
            &[
                (Color::rgba8(0, 0, 0, 255), 0.0),
                (Color::rgba8(255, 255, 255, 255), 1.0),
            ],
        ),
        radius: CornerRadii::ZERO,
        stroke_width: 0.0,
        clip: full_clip(w, h),
        transform: glam::Affine2::IDENTITY,
        z_index: 0,
    }];
    r.render_damage(&[Rect::new(0.0, 0.0, w, h)], &mut cmds, &mut vec![], &[]);
    let sb = r.softbuffer_pixels();
    // Monotonic vertical ramp, horizontally constant.
    let mut prev = 0u32;
    for y in (8..256).step_by(16) {
        let (v, _, _) = sb_rgb(sb[y * 64 + 32]);
        assert!(v + 2 >= prev, "ramp not monotonic at y={y}: {prev} -> {v}");
        let (v2, _, _) = sb_rgb(sb[y * 64 + 5]);
        assert!(
            (v as i32 - v2 as i32).abs() <= 1,
            "row not constant at y={y}"
        );
        prev = v;
    }
    // Endpoints roughly black / white.
    let (t, _, _) = sb_rgb(sb[2 * 64 + 32]);
    let (b, _, _) = sb_rgb(sb[253 * 64 + 32]);
    assert!(t < 8, "top should be ~black, got {t}");
    assert!(b > 247, "bottom should be ~white, got {b}");
}

// ── Conic gradient (round 3) ─────────────────────────────────────────

#[test]
fn conic_gradient_has_correct_angular_sweep() {
    let (w, h) = (64.0, 64.0);
    let mut r = renderer(w, h);
    let mut cmds = vec![DrawCommand::FillLinearGradient {
        rect: Rect::new(0.0, 0.0, 64.0, 64.0),
        gradient: burin::style::LinearGradient::conic(
            (0.5, 0.5),
            (0.6, 0.5),
            &[
                (Color::rgba8(0, 0, 255, 255), 0.0),
                (Color::rgba8(255, 0, 0, 255), 0.5),
                (Color::rgba8(0, 0, 255, 255), 1.0),
            ],
        ),
        radius: CornerRadii::ZERO,
        stroke_width: 0.0,
        clip: full_clip(w, h),
        transform: glam::Affine2::IDENTITY,
        z_index: 0,
    }];
    r.render_damage(&[Rect::new(0.0, 0.0, w, h)], &mut cmds, &mut vec![], &[]);
    let sb = r.softbuffer_pixels();
    // 0-degree direction = (0.6-0.5, 0.5-0.5) = right -> pixel (40,32) is at
    // angle 0 with ref_ang=0 -> t ~~ 0 -> blue. Pixel (24,32) is at angle pi ->
    // t ~~ 0.5 -> red. Confirm: these two pixels differ.
    let (cr, _cg, cb) = sb_rgb(sb[32 * 64 + 40]);
    assert!(
        cb > 200 && cr < 40,
        "0-degree pixel should be blue, got R={cr} B={cb}"
    );
    let (lr, _lg, lb) = sb_rgb(sb[32 * 64 + 24]);
    assert!(
        lr > 200 && lb < 40,
        "180-degree pixel should be red, got R={lr} B={lb}"
    );
}
