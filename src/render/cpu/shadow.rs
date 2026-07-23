//! Cached drop-shadow rendering for the CPU backend.
//!
//! The audit (2026-07-16) measured the previous approach — six concentric
//! anti-aliased rounded-rect fills through a full-frame clip mask — at
//! **~2.2 ms per shadow** @1080p. It also applied `shadow.offset` twice
//! (the painter already bakes the offset into the `FillShadow` rect) and its
//! layered profile didn't match the GPU's gaussian.
//!
//! New approach:
//! 1. Rasterise the element's rounded-rect coverage once, apply a 3-pass box
//!    blur (≈ gaussian, σ = blur/2 — CSS-like semantics, matching the GPU
//!    shader's falloff), producing a small alpha template.
//! 2. Cache the template as a **9-patch**: 4 colorized corner pixmaps + 4
//!    1-D edge profiles. The patch is independent of the element size, so
//!    size-animated shadows reuse the same entry.
//! 3. Blit: corners via `draw_pixmap`, edges/centre via constant-alpha row
//!    spans — all mask-aware (scroll-container clipping keeps working).
//!
//! The blur profile along a straight edge is translation-invariant, so the
//! 9-patch reconstruction is exact (bit-identical to a full-size render, up
//! to AA rounding) — locked by `tests/cpu_render_semantics.rs`.

use crate::style::{Color, CornerRadii, Rect};
use glam::Affine2;
use std::collections::HashMap;

use super::rounded_skia_rect;

// ── Blur core ────────────────────────────────────────────────────────

/// Box half-width for a 3-pass box blur approximating gaussian σ.
/// Each pass has variance (w²-1)/12; 3 passes: σ² = (w²-1)/4 → w = √(4σ²/3+1).
fn box_half_for_sigma(sigma: f32) -> usize {
    if sigma <= 0.0 {
        return 0;
    }
    let w = (4.0 * sigma * sigma / 3.0 + 1.0).sqrt();
    ((w - 1.0) * 0.5).round() as usize
}

/// One horizontal box-blur pass (zero padding outside), width = 2*half+1.
fn box_pass_h(src: &[u8], dst: &mut [u8], w: usize, h: usize, half: usize) {
    if half == 0 {
        dst.copy_from_slice(src);
        return;
    }
    let win = (2 * half + 1) as u32;
    for y in 0..h {
        let row = &src[y * w..(y + 1) * w];
        let out = &mut dst[y * w..(y + 1) * w];
        let mut sum: u32 = 0;
        for x in 0..=half.min(w - 1) {
            sum += row[x] as u32;
        }
        for x in 0..w {
            out[x] = (sum / win) as u8;
            let add = x + half + 1;
            if add < w {
                sum += row[add] as u32;
            }
            if x >= half {
                sum -= row[x - half] as u32;
            }
        }
    }
}

/// One vertical box-blur pass (zero padding outside).
fn box_pass_v(src: &[u8], dst: &mut [u8], w: usize, h: usize, half: usize) {
    if half == 0 {
        dst.copy_from_slice(src);
        return;
    }
    let win = (2 * half + 1) as u32;
    for x in 0..w {
        let mut sum: u32 = 0;
        for y in 0..=half.min(h - 1) {
            sum += src[y * w + x] as u32;
        }
        for y in 0..h {
            dst[y * w + x] = (sum / win) as u8;
            let add = y + half + 1;
            if add < h {
                sum += src[add * w + x] as u32;
            }
            if y >= half {
                sum -= src[(y - half) * w + x] as u32;
            }
        }
    }
}

/// Total blur reach in pixels for 3 passes of half-width `half`.
fn blur_pad(half: usize) -> usize {
    3 * half
}

/// One horizontal box pass with clamped (extended) edges — used for backdrop
/// blur where zero padding would darken the region borders.
fn box_pass_h_clamp(src: &[u8], dst: &mut [u8], w: usize, h: usize, half: usize) {
    if half == 0 {
        dst.copy_from_slice(src);
        return;
    }
    let win = (2 * half + 1) as u32;
    for y in 0..h {
        let row = &src[y * w..(y + 1) * w];
        let out = &mut dst[y * w..(y + 1) * w];
        let at = |x: i32| -> u32 { row[x.clamp(0, w as i32 - 1) as usize] as u32 };
        let mut sum: u32 = 0;
        for x in -(half as i32)..=(half as i32) {
            sum += at(x);
        }
        for x in 0..w as i32 {
            out[x as usize] = (sum / win) as u8;
            sum += at(x + half as i32 + 1);
            sum -= at(x - half as i32);
        }
    }
}

/// One vertical box pass with clamped (extended) edges.
fn box_pass_v_clamp(src: &[u8], dst: &mut [u8], w: usize, h: usize, half: usize) {
    if half == 0 {
        dst.copy_from_slice(src);
        return;
    }
    let win = (2 * half + 1) as u32;
    for x in 0..w {
        let at = |y: i32| -> u32 { src[y.clamp(0, h as i32 - 1) as usize * w + x] as u32 };
        let mut sum: u32 = 0;
        for y in -(half as i32)..=(half as i32) {
            sum += at(y);
        }
        for y in 0..h as i32 {
            dst[y as usize * w + x] = (sum / win) as u8;
            sum += at(y + half as i32 + 1);
            sum -= at(y - half as i32);
        }
    }
}

/// 3-pass box blur (≈ gaussian σ = `sigma`) of a premultiplied canonical-RGBA
/// region, clamped edges. Used by the CPU backdrop-filter implementation.
pub(crate) fn box_blur_rgba(src: &[u32], w: usize, h: usize, sigma: f32) -> Vec<u32> {
    let half = box_half_for_sigma(sigma);
    if half == 0 || w == 0 || h == 0 {
        return src.to_vec();
    }
    let n = w * h;
    let mut ch = [vec![0u8; n], vec![0u8; n], vec![0u8; n], vec![0u8; n]];
    for (i, &px) in src.iter().enumerate() {
        ch[0][i] = (px & 0xFF) as u8;
        ch[1][i] = ((px >> 8) & 0xFF) as u8;
        ch[2][i] = ((px >> 16) & 0xFF) as u8;
        ch[3][i] = ((px >> 24) & 0xFF) as u8;
    }
    let mut tmp = vec![0u8; n];
    for c in ch.iter_mut() {
        box_pass_h_clamp(c, &mut tmp, w, h, half);
        box_pass_h_clamp(&tmp, c, w, h, half);
        box_pass_h_clamp(c, &mut tmp, w, h, half);
        box_pass_v_clamp(&tmp, c, w, h, half);
        box_pass_v_clamp(c, &mut tmp, w, h, half);
        box_pass_v_clamp(&tmp, c, w, h, half);
    }
    let mut out = vec![0u32; n];
    for i in 0..n {
        out[i] = ((ch[3][i] as u32) << 24)
            | ((ch[2][i] as u32) << 16)
            | ((ch[1][i] as u32) << 8)
            | ch[0][i] as u32;
    }
    out
}

/// Render the blurred-alpha plane for an `ew × eh` device-pixel rounded rect
/// with device radii `radii_dev` and gaussian σ = `blur_dev / 2`.
/// Returns (alpha, plane_w, plane_h, pad).
fn render_alpha_plane(
    ew: u32,
    eh: u32,
    radii_dev: CornerRadii,
    blur_dev: f32,
) -> Option<(Vec<u8>, usize, usize, usize)> {
    let half = box_half_for_sigma(blur_dev * 0.5);
    let pad = blur_pad(half);
    let w = ew as usize + 2 * pad;
    let h = eh as usize + 2 * pad;

    // Rounded-rect coverage via tiny-skia (anti-aliased corners).
    let mut cov = tiny_skia::Pixmap::new(w as u32, h as u32)?;
    let rect = tiny_skia::Rect::from_xywh(pad as f32, pad as f32, ew as f32, eh as f32)?;
    let path = rounded_skia_rect(rect, radii_dev, 1.0);
    let paint = tiny_skia::Paint {
        shader: tiny_skia::Shader::SolidColor(tiny_skia::Color::WHITE),
        anti_alias: true,
        ..Default::default()
    };
    cov.fill_path(
        &path,
        &paint,
        tiny_skia::FillRule::Winding,
        tiny_skia::Transform::identity(),
        None,
    );

    let mut a: Vec<u8> = cov.data().iter().skip(3).step_by(4).copied().collect();
    if half > 0 {
        let mut b = vec![0u8; w * h];
        box_pass_h(&a, &mut b, w, h, half);
        box_pass_h(&b, &mut a, w, h, half);
        box_pass_h(&a, &mut b, w, h, half);
        box_pass_v(&b, &mut a, w, h, half);
        box_pass_v(&a, &mut b, w, h, half);
        box_pass_v(&b, &mut a, w, h, half);
    }
    Some((a, w, h, pad))
}

/// Colorize an alpha sub-region into a premultiplied canonical-RGBA pixmap.
fn colorize_region(
    alpha: &[u8],
    plane_w: usize,
    x0: usize,
    y0: usize,
    w: usize,
    h: usize,
    color: Color,
) -> Option<tiny_skia::Pixmap> {
    let mut pm = tiny_skia::Pixmap::new(w as u32, h as u32)?;
    let dst = bytemuck::cast_slice_mut::<u8, u32>(pm.data_mut());
    let cr = (color.r * 255.0) as u32;
    let cg = (color.g * 255.0) as u32;
    let cb = (color.b * 255.0) as u32;
    let ca = (color.a * 255.0) as u32;
    for y in 0..h {
        for x in 0..w {
            let a = alpha[(y0 + y) * plane_w + (x0 + x)] as u32 * ca / 255;
            if a == 0 {
                continue;
            }
            let r = cr * a / 255;
            let g = cg * a / 255;
            let b = cb * a / 255;
            dst[y * w + x] = (a << 24) | (b << 16) | (g << 8) | r;
        }
    }
    Some(pm)
}

// ── Cache ────────────────────────────────────────────────────────────

#[derive(Hash, PartialEq, Eq, Clone, Copy)]
struct Key {
    /// Exact device size for small elements; (0, 0) for the 9-patch entry.
    ew: u32,
    eh: u32,
    radii: [u32; 4],
    blur: u32,
    color: [u8; 4],
}

enum Patch {
    /// Element too small for a 9-patch: the full colorized shadow pixmap.
    Exact {
        pixmap: tiny_skia::Pixmap,
        pad: usize,
    },
    /// Size-independent 9-patch.
    Nine {
        /// Corner square size (device px): 2·pad + ceil(max radius) + 1.
        c: usize,
        /// Colorized corner pixmaps: TL, TR, BL, BR (each c × c).
        corners: [tiny_skia::Pixmap; 4],
        /// Edge alpha profiles (length c): top (per row), bottom, left, right.
        top: Vec<u8>,
        bottom: Vec<u8>,
        left: Vec<u8>,
        right: Vec<u8>,
    },
}

struct Entry {
    patch: Patch,
    last_used: u64,
}

pub(crate) struct ShadowCache {
    entries: HashMap<Key, Entry>,
    tick: u64,
}

const MAX_ENTRIES: usize = 32;

impl ShadowCache {
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
            tick: 0,
        }
    }

    fn get_or_build(
        &mut self,
        key: Key,
        ew: u32,
        eh: u32,
        radii_dev: CornerRadii,
        blur_dev: f32,
        color: Color,
    ) -> Option<&Patch> {
        self.tick += 1;
        if !self.entries.contains_key(&key) {
            if self.entries.len() >= MAX_ENTRIES {
                if let Some(&k) = self
                    .entries
                    .iter()
                    .min_by_key(|(_, e)| e.last_used)
                    .map(|(k, _)| k)
                {
                    self.entries.remove(&k);
                }
            }
            let patch = build_patch(key, ew, eh, radii_dev, blur_dev, color)?;
            self.entries.insert(
                key,
                Entry {
                    patch,
                    last_used: self.tick,
                },
            );
        }
        let e = self.entries.get_mut(&key)?;
        e.last_used = self.tick;
        Some(&e.patch)
    }
}

fn build_patch(
    key: Key,
    ew: u32,
    eh: u32,
    radii_dev: CornerRadii,
    blur_dev: f32,
    color: Color,
) -> Option<Patch> {
    if key.ew != 0 {
        // Exact-size template.
        let (alpha, w, h, pad) = render_alpha_plane(ew, eh, radii_dev, blur_dev)?;
        let pixmap = colorize_region(&alpha, w, 0, 0, w, h, color)?;
        return Some(Patch::Exact { pixmap, pad });
    }
    // 9-patch: template element is (2c+1) × (2c+1).
    let half = box_half_for_sigma(blur_dev * 0.5);
    let pad = blur_pad(half);
    let max_r = radii_dev
        .top_left
        .max(radii_dev.top_right)
        .max(radii_dev.bottom_right)
        .max(radii_dev.bottom_left)
        .ceil() as usize;
    let c = pad + max_r + 1;
    let tpl_e = (2 * c + 1) as u32;
    let (alpha, w, h, _pad) = render_alpha_plane(tpl_e, tpl_e, radii_dev, blur_dev)?;
    // w == h == tpl_e + 2*pad == 2c+1+2pad; corner square = c+pad? No:
    // the corner region of the TARGET is c_t = pad + max_r + 1 measured from
    // the shadow AABB corner. In the template the same region starts at (0,0)
    // and spans ct = pad + c ... — the template's own corner span is
    // pad (outside) + c (inside corner incl. radius+1 safety) = pad + c.
    let ct = pad + c;
    let corners = [
        colorize_region(&alpha, w, 0, 0, ct, ct, color)?, // TL
        colorize_region(&alpha, w, w - ct, 0, ct, ct, color)?, // TR
        colorize_region(&alpha, w, 0, h - ct, ct, ct, color)?, // BL
        colorize_region(&alpha, w, w - ct, h - ct, ct, ct, color)?, // BR
    ];
    // Edge profiles sampled at the template midline (beyond corner influence).
    let mid = w / 2;
    let top: Vec<u8> = (0..ct).map(|y| alpha[y * w + mid]).collect();
    let bottom: Vec<u8> = (0..ct).map(|y| alpha[(h - ct + y) * w + mid]).collect();
    let left: Vec<u8> = (0..ct).map(|x| alpha[mid * w + x]).collect();
    let right: Vec<u8> = (0..ct).map(|x| alpha[mid * w + (w - ct + x)]).collect();
    Some(Patch::Nine {
        c: ct,
        corners,
        top,
        bottom,
        left,
        right,
    })
}

// ── Blitting ─────────────────────────────────────────────────────────

/// Source-over blend a constant premultiplied colour over a horizontal span,
/// with optional mask and optional device clip rect.
#[allow(clippy::too_many_arguments)]
fn blend_span(
    pixels: &mut [u32],
    pw: usize,
    ph: usize,
    x0: i32,
    x1: i32,
    y: i32,
    sr: u32,
    sg: u32,
    sb: u32,
    sa: u32,
    mask: Option<&tiny_skia::Mask>,
    clip_dev: Option<(i32, i32, i32, i32)>,
) {
    if y < 0 || y >= ph as i32 || sa == 0 {
        return;
    }
    let (mut x0, mut x1) = (x0, x1);
    if let Some((cx0, cy0, cx1, cy1)) = clip_dev {
        if y < cy0 || y >= cy1 {
            return;
        }
        x0 = x0.max(cx0);
        x1 = x1.min(cx1);
    }
    let xs = x0.max(0) as usize;
    let xe = (x1.min(pw as i32)).max(0) as usize;
    if xs >= xe {
        return;
    }
    let row = y as usize * pw;
    let mdata = mask.map(|m| m.data());
    for x in xs..xe {
        let (a, r, g, b) = if let Some(md) = mdata {
            let mv = md[row + x] as u32;
            if mv == 0 {
                continue;
            }
            (sa * mv / 255, sr * mv / 255, sg * mv / 255, sb * mv / 255)
        } else {
            (sa, sr, sg, sb)
        };
        let d = pixels[row + x];
        let inv = 255 - a;
        let dr = d & 0xFF;
        let dg = (d >> 8) & 0xFF;
        let db = (d >> 16) & 0xFF;
        let da = (d >> 24) & 0xFF;
        let or = r + dr * inv / 255;
        let og = g + dg * inv / 255;
        let ob = b + db * inv / 255;
        let oa = a + da * inv / 255;
        pixels[row + x] = (oa << 24) | (ob << 16) | (og << 8) | or;
    }
}

/// Draw a drop shadow for a `FillShadow` command (no extra clip rect).
/// Used by the headless test rasteriser — identical output to the live path.
#[allow(clippy::too_many_arguments)]
pub(crate) fn draw_shadow(
    cache: &mut ShadowCache,
    pixels: &mut [u32],
    phys_w: u32,
    phys_h: u32,
    sr: Rect,
    radius: CornerRadii,
    shadow: &crate::style::styled::Shadow,
    xform: Affine2,
    sf: f32,
    mask: Option<&tiny_skia::Mask>,
) {
    draw_shadow_clipped(
        cache, pixels, phys_w, phys_h, sr, radius, shadow, xform, sf, mask, None,
    );
}

/// Draw a drop shadow for a `FillShadow` command.
///
/// `sr` is the command rect: the element rect expanded by `blur` on each side
/// and already shifted by the shadow offset (see `LocalDrawItem::to_world_accum`
/// — the offset must NOT be applied again here; the pre-audit code did, pushing
/// CPU shadows twice as far as GPU ones).
///
/// `clip_dev`: optional device-space clip rect (x0, y0, x1, y1) — the cheap
/// geometric form of clipping used when the shadow straddles a damage or
/// clip boundary away from rounded corners.
#[allow(clippy::too_many_arguments)]
pub(crate) fn draw_shadow_clipped(
    cache: &mut ShadowCache,
    pixels: &mut [u32],
    phys_w: u32,
    phys_h: u32,
    sr: Rect,
    radius: CornerRadii,
    shadow: &crate::style::styled::Shadow,
    xform: Affine2,
    sf: f32,
    mask: Option<&tiny_skia::Mask>,
    clip_dev: Option<(i32, i32, i32, i32)>,
) {
    if shadow.color.a <= 0.0 {
        return;
    }
    // Element rect (raw space): centred in sr, inset by blur.
    let ew_l = (sr.width - 2.0 * shadow.blur).max(1.0);
    let eh_l = (sr.height - 2.0 * shadow.blur).max(1.0);
    let ex = sr.x + shadow.blur;
    let ey = sr.y + shadow.blur;

    let m = xform.matrix2;
    let axis_aligned_unit = (m.x_axis.x - 1.0).abs() < 1e-3
        && m.x_axis.y.abs() < 1e-6
        && m.y_axis.x.abs() < 1e-6
        && (m.y_axis.y - 1.0).abs() < 1e-3;
    if !axis_aligned_unit {
        // Rare path (rotated/scaled shadows): exact-size template drawn via
        // tiny-skia through the full transform.
        draw_shadow_transformed(
            cache, pixels, phys_w, phys_h, ex, ey, ew_l, eh_l, radius, shadow, xform, sf, mask,
        );
        return;
    }

    let t = xform.translation;
    let ew = ((ew_l * sf).round() as u32).max(1);
    let eh = ((eh_l * sf).round() as u32).max(1);
    let blur_dev = shadow.blur * sf;
    let max_dim = (ew.min(eh) as f32) * 0.5;
    let radii_dev = CornerRadii {
        top_left: (radius.top_left * sf).min(max_dim),
        top_right: (radius.top_right * sf).min(max_dim),
        bottom_right: (radius.bottom_right * sf).min(max_dim),
        bottom_left: (radius.bottom_left * sf).min(max_dim),
    };
    let color = shadow.color;
    let color_key = [
        (color.r * 255.0) as u8,
        (color.g * 255.0) as u8,
        (color.b * 255.0) as u8,
        (color.a * 255.0) as u8,
    ];
    let radii_key = [
        radii_dev.top_left.to_bits(),
        radii_dev.top_right.to_bits(),
        radii_dev.bottom_right.to_bits(),
        radii_dev.bottom_left.to_bits(),
    ];

    // Probe 9-patch viability: corner span ct = pad + ceil(max_r) + 1 + pad.
    let half = box_half_for_sigma(blur_dev * 0.5);
    let pad = blur_pad(half);
    let max_r = radii_dev
        .top_left
        .max(radii_dev.top_right)
        .max(radii_dev.bottom_right)
        .max(radii_dev.bottom_left)
        .ceil() as usize;
    let ct = pad + (pad + max_r + 1);
    let nine_ok = ew as usize + 2 * pad > 2 * ct && eh as usize + 2 * pad > 2 * ct;

    let key = if nine_ok {
        Key {
            ew: 0,
            eh: 0,
            radii: radii_key,
            blur: blur_dev.to_bits(),
            color: color_key,
        }
    } else {
        Key {
            ew,
            eh,
            radii: radii_key,
            blur: blur_dev.to_bits(),
            color: color_key,
        }
    };

    // Device-space top-left of the shadow AABB.
    let dev_x = ((ex + t.x) * sf).round() as i32 - pad as i32;
    let dev_y = ((ey + t.y) * sf).round() as i32 - pad as i32;
    let tw = ew as usize + 2 * pad;
    let th = eh as usize + 2 * pad;

    let Some(patch) = cache.get_or_build(key, ew, eh, radii_dev, blur_dev, color) else {
        return;
    };

    let pw = phys_w as usize;
    let ph = phys_h as usize;
    let cr = (color.r * 255.0) as u32;
    let cg = (color.g * 255.0) as u32;
    let cb = (color.b * 255.0) as u32;
    let ca = (color.a * 255.0) as u32;

    match patch {
        Patch::Exact { pixmap, .. } => {
            blit_pixmap(pixels, pw, ph, pixmap, dev_x, dev_y, mask, clip_dev);
        }
        Patch::Nine {
            c,
            corners,
            top,
            bottom,
            left,
            right,
        } => {
            let c = *c;
            // Corners (disjoint from edges/centre by construction).
            blit_pixmap(pixels, pw, ph, &corners[0], dev_x, dev_y, mask, clip_dev);
            blit_pixmap(
                pixels,
                pw,
                ph,
                &corners[1],
                dev_x + (tw - c) as i32,
                dev_y,
                mask,
                clip_dev,
            );
            blit_pixmap(
                pixels,
                pw,
                ph,
                &corners[2],
                dev_x,
                dev_y + (th - c) as i32,
                mask,
                clip_dev,
            );
            blit_pixmap(
                pixels,
                pw,
                ph,
                &corners[3],
                dev_x + (tw - c) as i32,
                dev_y + (th - c) as i32,
                mask,
                clip_dev,
            );
            // Top / bottom edge strips.
            let ex0 = dev_x + c as i32;
            let ex1 = dev_x + (tw - c) as i32;
            for (row, &av) in top.iter().enumerate() {
                let a = av as u32 * ca / 255;
                blend_span(
                    pixels,
                    pw,
                    ph,
                    ex0,
                    ex1,
                    dev_y + row as i32,
                    cr * a / 255,
                    cg * a / 255,
                    cb * a / 255,
                    a,
                    mask,
                    clip_dev,
                );
            }
            for (row, &av) in bottom.iter().enumerate() {
                let a = av as u32 * ca / 255;
                blend_span(
                    pixels,
                    pw,
                    ph,
                    ex0,
                    ex1,
                    dev_y + (th - c + row) as i32,
                    cr * a / 255,
                    cg * a / 255,
                    cb * a / 255,
                    a,
                    mask,
                    clip_dev,
                );
            }
            // Left / right edge strips + centre rows.
            let cy0 = dev_y + c as i32;
            let cy1 = dev_y + (th - c) as i32;
            let a_full = ca;
            let (fr, fg, fb) = (cr * a_full / 255, cg * a_full / 255, cb * a_full / 255);
            for y in cy0..cy1 {
                for (col, &av) in left.iter().enumerate() {
                    let a = av as u32 * ca / 255;
                    blend_span(
                        pixels,
                        pw,
                        ph,
                        dev_x + col as i32,
                        dev_x + col as i32 + 1,
                        y,
                        cr * a / 255,
                        cg * a / 255,
                        cb * a / 255,
                        a,
                        mask,
                        clip_dev,
                    );
                }
                for (col, &av) in right.iter().enumerate() {
                    let x = dev_x + (tw - c + col) as i32;
                    let a = av as u32 * ca / 255;
                    blend_span(
                        pixels,
                        pw,
                        ph,
                        x,
                        x + 1,
                        y,
                        cr * a / 255,
                        cg * a / 255,
                        cb * a / 255,
                        a,
                        mask,
                        clip_dev,
                    );
                }
                // Centre span (full shadow colour).
                blend_span(
                    pixels, pw, ph, ex0, ex1, y, fr, fg, fb, a_full, mask, clip_dev,
                );
            }
        }
    }
}

/// Source-over blit a canonical-RGBA pixmap at integer device position.
#[allow(clippy::too_many_arguments)]
fn blit_pixmap(
    pixels: &mut [u32],
    pw: usize,
    ph: usize,
    src: &tiny_skia::Pixmap,
    dx: i32,
    dy: i32,
    mask: Option<&tiny_skia::Mask>,
    clip_dev: Option<(i32, i32, i32, i32)>,
) {
    let sw = src.width() as i32;
    let sh = src.height() as i32;
    let sdata = bytemuck::cast_slice::<u8, u32>(src.data());
    let mdata = mask.map(|m| m.data());
    let mut x0 = dx.max(0);
    let mut y0 = dy.max(0);
    let mut x1 = (dx + sw).min(pw as i32);
    let mut y1 = (dy + sh).min(ph as i32);
    if let Some((cx0, cy0, cx1, cy1)) = clip_dev {
        x0 = x0.max(cx0);
        y0 = y0.max(cy0);
        x1 = x1.min(cx1);
        y1 = y1.min(cy1);
    }
    for y in y0..y1 {
        let srow = ((y - dy) * sw) as usize;
        let drow = y as usize * pw;
        for x in x0..x1 {
            let s = sdata[srow + (x - dx) as usize];
            let mut sa = (s >> 24) & 0xFF;
            if sa == 0 {
                continue;
            }
            let (mut sr, mut sg, mut sb) = (s & 0xFF, (s >> 8) & 0xFF, (s >> 16) & 0xFF);
            if let Some(md) = mdata {
                let mv = md[drow + x as usize] as u32;
                if mv == 0 {
                    continue;
                }
                sa = sa * mv / 255;
                sr = sr * mv / 255;
                sg = sg * mv / 255;
                sb = sb * mv / 255;
            }
            let d = pixels[drow + x as usize];
            let inv = 255 - sa;
            let or = sr + (d & 0xFF) * inv / 255;
            let og = sg + ((d >> 8) & 0xFF) * inv / 255;
            let ob = sb + ((d >> 16) & 0xFF) * inv / 255;
            let oa = sa + ((d >> 24) & 0xFF) * inv / 255;
            pixels[drow + x as usize] = (oa << 24) | (ob << 16) | (og << 8) | or;
        }
    }
}

/// Fallback for rotated/scaled shadows: exact-size colorized template drawn
/// through the full transform by tiny-skia.
#[allow(clippy::too_many_arguments)]
fn draw_shadow_transformed(
    cache: &mut ShadowCache,
    pixels: &mut [u32],
    phys_w: u32,
    phys_h: u32,
    ex: f32,
    ey: f32,
    ew_l: f32,
    eh_l: f32,
    radius: CornerRadii,
    shadow: &crate::style::styled::Shadow,
    xform: Affine2,
    sf: f32,
    mask: Option<&tiny_skia::Mask>,
) {
    let ew = ((ew_l * sf).round() as u32).max(1);
    let eh = ((eh_l * sf).round() as u32).max(1);
    let blur_dev = shadow.blur * sf;
    let radii_dev = CornerRadii {
        top_left: radius.top_left * sf,
        top_right: radius.top_right * sf,
        bottom_right: radius.bottom_right * sf,
        bottom_left: radius.bottom_left * sf,
    };
    let color = shadow.color;
    let key = Key {
        ew,
        eh,
        radii: [
            radii_dev.top_left.to_bits(),
            radii_dev.top_right.to_bits(),
            radii_dev.bottom_right.to_bits(),
            radii_dev.bottom_left.to_bits(),
        ],
        blur: blur_dev.to_bits(),
        color: [
            (color.r * 255.0) as u8,
            (color.g * 255.0) as u8,
            (color.b * 255.0) as u8,
            (color.a * 255.0) as u8,
        ],
    };
    let Some(Patch::Exact { pixmap, pad }) =
        cache.get_or_build(key, ew, eh, radii_dev, blur_dev, color)
    else {
        return;
    };
    let pad_l = *pad as f32 / sf;
    let mut pm =
        match tiny_skia::PixmapMut::from_bytes(bytemuck::cast_slice_mut(pixels), phys_w, phys_h) {
            Some(pm) => pm,
            None => return,
        };
    // Pattern space (template px) → pre-scaled logical space.
    let pat_ts =
        tiny_skia::Transform::from_row(1.0, 0.0, 0.0, 1.0, (ex - pad_l) * sf, (ey - pad_l) * sf);
    let paint = tiny_skia::Paint {
        shader: tiny_skia::Pattern::new(
            pixmap.as_ref(),
            tiny_skia::SpreadMode::Pad,
            tiny_skia::FilterQuality::Bilinear,
            1.0,
            pat_ts,
        ),
        anti_alias: false,
        ..Default::default()
    };
    let Some(dest) = tiny_skia::Rect::from_xywh(
        (ex - pad_l) * sf,
        (ey - pad_l) * sf,
        (ew_l + 2.0 * pad_l) * sf,
        (eh_l + 2.0 * pad_l) * sf,
    ) else {
        return;
    };
    let mut pb = tiny_skia::PathBuilder::new();
    pb.push_rect(dest);
    let Some(path) = pb.finish() else {
        return;
    };
    let skia_xform = super::glam_to_skia_transform(xform, sf);
    pm.fill_path(
        &path,
        &paint,
        tiny_skia::FillRule::Winding,
        skia_xform,
        mask,
    );
}
