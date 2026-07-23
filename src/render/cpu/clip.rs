//! Unified clip primitive for the CPU backend.
//!
//! The GPU backend clips every primitive per-fragment in the shader via
//! `clip_alpha` (a rounded-box SDF, see `render/wgpu/pipeline.rs`). This module
//! is the CPU equivalent: it rasterises the (scroll-adjusted, optionally
//! rounded) clip rectangle into a tiny-skia [`tiny_skia::Mask`] in device space,
//! which is then handed to every tiny-skia draw call. Black (0) blocks, white
//! (255) allows, with anti-aliased edges — exactly mirroring the GPU's
//! per-pixel alpha multiply.
//!
//! A single source of truth used by both the live renderer
//! (`TinySkiaRenderer`) and the headless test rasteriser (`testing::pixel`),
//! so clipping behaves identically in tests and on screen.

use crate::render::painter::ClipInfo;
use crate::style::{CornerRadii, Rect};

/// World-space clip rectangle, with the clip's accumulated scroll offset folded
/// in. Mirrors the GPU's `clip.rect + scroll_offset` (`render/wgpu/mod.rs`).
/// This is the clip in the primitive's **raw (pre-transform) space** — the same
/// space the primitive's `rect` is expressed in.
pub(crate) fn clip_world_rect(clip: &ClipInfo) -> Rect {
    Rect::new(
        clip.rect.x + clip.scroll_offset[0],
        clip.rect.y + clip.scroll_offset[1],
        clip.rect.width,
        clip.rect.height,
    )
}

/// Outcome of resolving a [`ClipInfo`] into a CPU draw mask.
pub(crate) enum ClipMask {
    /// No clipping required. Draw the primitive unmasked.
    None,
    /// Clip region is empty — nothing should be drawn.
    Skip,
    /// A device-space alpha mask to pass to tiny-skia draw calls.
    Mask(tiny_skia::Mask),
}

/// Build a device-space clip mask.
///
/// `abs` is the clip rectangle in **raw (pre-transform) space** (see
/// [`clip_world_rect`]); `radius` its corner rounding. The content is drawn
/// through `xform` (which carries the scroll translation), so the mask is
/// rasterised through the *same* logical→device transform to land on the
/// on-screen viewport.
///
/// `intersect_raw`: optional additional axis-aligned rectangle (raw space) the
/// mask is intersected with — used to confine straddling draws to the current
/// damage rectangle (correctness: translucent commands must not be composited
/// twice across damage passes).
///
/// `phys_w`/`phys_h` must equal the target pixmap dimensions: tiny-skia masks
/// are indexed in device coordinates aligned 1:1 with the pixmap.
pub(crate) fn build_clip_mask_rect(
    abs: Rect,
    radius: CornerRadii,
    xform: glam::Affine2,
    intersect_raw: Option<Rect>,
    phys_w: u32,
    phys_h: u32,
    sf: f32,
) -> ClipMask {
    if abs.width <= 0.0 || abs.height <= 0.0 {
        return ClipMask::Skip;
    }
    let mut mask = match tiny_skia::Mask::new(phys_w.max(1), phys_h.max(1)) {
        Some(m) => m,
        // Degenerate target — fall back to no clip rather than dropping draws.
        None => return ClipMask::None,
    };
    // Rounded clip path in LOGICAL coordinates (radii un-scaled). The
    // `*_logical` device transform applies both `xform` and the scale factor,
    // mapping it to `sf * xform(abs)` — exactly where the content lands.
    let logical_rect =
        tiny_skia::Rect::from_xywh(abs.x, abs.y, abs.width.max(0.01), abs.height.max(0.01))
            .unwrap_or_else(|| tiny_skia::Rect::from_xywh(0.0, 0.0, 1.0, 1.0).unwrap());
    let path = super::rounded_skia_rect(logical_rect, radius, 1.0);
    let dev_xform = super::glam_to_skia_transform_logical(xform, sf);
    // Anti-aliased coverage → soft rounded edges, matching the GPU SDF clip.
    mask.fill_path(&path, tiny_skia::FillRule::Winding, true, dev_xform);
    if let Some(ir) = intersect_raw {
        let Some(sr) =
            tiny_skia::Rect::from_xywh(ir.x, ir.y, ir.width.max(0.01), ir.height.max(0.01))
        else {
            return ClipMask::Skip;
        };
        let mut pb = tiny_skia::PathBuilder::new();
        pb.push_rect(sr);
        if let Some(p) = pb.finish() {
            mask.intersect_path(&p, tiny_skia::FillRule::Winding, false, dev_xform);
        }
    }
    ClipMask::Mask(mask)
}

/// Compatibility wrapper: resolve a [`ClipInfo`] directly (used by the
/// headless test rasteriser). Includes the whole-viewport no-op fast path.
pub(crate) fn build_clip_mask(
    clip: &ClipInfo,
    xform: glam::Affine2,
    phys_w: u32,
    phys_h: u32,
    logical_w: f32,
    logical_h: f32,
    sf: f32,
) -> ClipMask {
    let abs = clip_world_rect(clip);
    if abs.width <= 0.0 || abs.height <= 0.0 {
        return ClipMask::Skip;
    }
    // No-op fast path: only when the clip is the whole viewport, axis-aligned
    // (identity transform) and unrounded.
    let is_identity = xform == glam::Affine2::IDENTITY;
    if clip.radius.is_zero()
        && is_identity
        && abs.x <= 0.5
        && abs.y <= 0.5
        && abs.x + abs.width >= logical_w - 0.5
        && abs.y + abs.height >= logical_h - 0.5
    {
        return ClipMask::None;
    }
    build_clip_mask_rect(abs, clip.radius, xform, None, phys_w, phys_h, sf)
}
