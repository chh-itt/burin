//! Headless pixel rasteriser for `TestHarness` — converts `DrawCommand`
//! output into a tiny-skia `Pixmap` without any window or softbuffer, so tests
//! can assert on actual rendered pixel colours.
//!
//! All drawing primitives delegate to the **same** `crate::render::cpu` free
//! functions that the live CPU renderer uses — single source of truth for
//! CPU-side rasterisation. Clip masks use the shared `cpu::clip` module.
//!
//! Scope: `FillRect`, `StrokeRect`, `FillShadow`, `FillLinearGradient`,
//! and `FillPath`/`StrokePath` are all supported via the shared primitives.
//! `DrawImage` is skipped (requires image cache lifecycle).
//!
//! Behind the `backend-tiny-skia` feature gate.

use crate::render::cpu::clip::{build_clip_mask, ClipMask};
use crate::render::cpu::shadow;
use crate::render::cpu::{
    draw_gradient_skia, fill_path_skia, fill_rect_skia, stroke_path_skia, stroke_rect_skia,
};
use crate::render::DrawCommand;
use crate::style::Color;
use glam::Affine2;

/// A rendered frame's pixels. Wraps a tiny-skia `Pixmap`; read colours via
/// [`PixelBuffer::pixel_color`].
pub struct PixelBuffer {
    pixmap: tiny_skia::Pixmap,
}

impl PixelBuffer {
    pub fn width(&self) -> u32 {
        self.pixmap.width()
    }

    pub fn height(&self) -> u32 {
        self.pixmap.height()
    }

    /// Read the (un-premultiplied) colour at physical pixel `(x, y)`.
    /// Returns `None` if the point is outside the buffer.
    pub fn pixel_color(&self, x: u32, y: u32) -> Option<Color> {
        self.pixmap.pixel(x, y).map(|p| {
            let c = p.demultiply();
            Color::rgba8(c.red(), c.green(), c.blue(), c.alpha())
        })
    }

    /// Raw premultiplied-RGBA8 pixel bytes (`width*height*4`).
    pub fn rgba_premul(&self) -> &[u8] {
        self.pixmap.data()
    }

    /// Encode the buffer to PNG bytes.
    pub fn encode_png(&self) -> Result<Vec<u8>, String> {
        self.pixmap.encode_png().map_err(|e| e.to_string())
    }

    /// Save the buffer to a PNG file at `path`.
    pub fn save_png(&self, path: &std::path::Path) -> Result<(), String> {
        self.pixmap.save_png(path).map_err(|e| e.to_string())
    }

    /// Load a `PixelBuffer` from a PNG file.
    pub fn load_png(path: &std::path::Path) -> Result<Self, String> {
        tiny_skia::Pixmap::load_png(path)
            .map(|pixmap| PixelBuffer { pixmap })
            .map_err(|e| e.to_string())
    }

    /// Construct a solid-colour diff buffer of the same dimensions, marking
    /// differing pixels. Used to write `<name>.diff.png` on snapshot failure.
    pub(crate) fn new_rgba(width: u32, height: u32, rgba_premul: &[u8]) -> Option<Self> {
        let mut pixmap = tiny_skia::Pixmap::new(width, height)?;
        pixmap.data_mut().copy_from_slice(rgba_premul);
        Some(PixelBuffer { pixmap })
    }
}

/// Rasterise `commands` onto a fresh `width_px × height_px` pixmap cleared to
/// `bg_color`. Commands are z-sorted internally (CPU has no depth buffer).
/// `scale` is the device scale factor (logical → physical).
///
/// Drawing is delegated to the `crate::render::cpu::*_skia` free functions —
/// the same primitives the live CPU renderer uses.
pub fn rasterize_commands(
    commands: &[DrawCommand],
    width_px: u32,
    height_px: u32,
    scale: f32,
    bg_color: Color,
) -> PixelBuffer {
    let mut pixmap =
        tiny_skia::Pixmap::new(width_px.max(1), height_px.max(1)).expect("valid pixmap dimensions");
    pixmap.fill(to_skia_color(bg_color));

    let sf = scale.max(0.0001);
    let logical_w = width_px as f32 / sf;
    let logical_h = height_px as f32 / sf;

    let mut order: Vec<usize> = (0..commands.len()).collect();
    order.sort_by_key(|&i| commands[i].z_index());

    // Transient shadow cache: identical output to the live renderer's cached
    // path (single source of truth for the shadow profile).
    let mut shadow_cache = shadow::ShadowCache::new();

    {
        let mut pm = pixmap.as_mut();
        for &i in &order {
            let cmd = &commands[i];
            let clip = cmd.clip();
            let clip_xform = match cmd {
                DrawCommand::FillRect { transform, .. }
                | DrawCommand::FillShadow { transform, .. }
                | DrawCommand::StrokeRect { transform, .. } => *transform,
                _ => Affine2::IDENTITY,
            };
            let mask = match build_clip_mask(
                &clip, clip_xform, width_px, height_px, logical_w, logical_h, scale,
            ) {
                ClipMask::Skip => continue,
                ClipMask::None => None,
                ClipMask::Mask(m) => Some(m),
            };
            let mask_ref = mask.as_ref();
            match cmd {
                DrawCommand::FillRect {
                    rect,
                    color,
                    radius,
                    transform,
                    blend_mode,
                    ..
                } => {
                    fill_rect_skia(
                        &mut pm,
                        *rect,
                        *color,
                        *radius,
                        *transform,
                        scale,
                        mask_ref,
                        *blend_mode,
                    );
                }
                DrawCommand::FillShadow {
                    rect,
                    radius,
                    shadow: sh,
                    transform,
                    ..
                } => {
                    shadow::draw_shadow(
                        &mut shadow_cache,
                        bytemuck::cast_slice_mut(pm.data_mut()),
                        width_px,
                        height_px,
                        *rect,
                        *radius,
                        sh,
                        *transform,
                        scale,
                        mask_ref,
                    );
                }
                DrawCommand::StrokeRect {
                    rect,
                    color,
                    width,
                    radius,
                    transform,
                    blend_mode,
                    ..
                } => {
                    stroke_rect_skia(
                        &mut pm,
                        *rect,
                        *color,
                        *width,
                        *radius,
                        *transform,
                        scale,
                        mask_ref,
                        *blend_mode,
                    );
                }
                DrawCommand::FillLinearGradient {
                    rect,
                    gradient,
                    radius,
                    stroke_width,
                    transform,
                    ..
                } => {
                    draw_gradient_skia(
                        &mut pm,
                        *rect,
                        *rect,
                        *gradient,
                        *radius,
                        *stroke_width,
                        *transform,
                        scale,
                        mask_ref,
                    );
                }
                DrawCommand::FillPath {
                    path,
                    brush,
                    transform,
                    ..
                } => {
                    fill_path_skia(&mut pm, path, brush, *transform, scale, mask_ref);
                }
                DrawCommand::StrokePath {
                    path,
                    stroke,
                    brush,
                    transform,
                    ..
                } => {
                    stroke_path_skia(
                        &mut pm,
                        path,
                        stroke.clone(),
                        brush,
                        *transform,
                        scale,
                        mask_ref,
                    );
                }
                _ => {} // DrawImage skipped
            }
        }
    }

    PixelBuffer { pixmap }
}

fn to_skia_color(c: Color) -> tiny_skia::Color {
    tiny_skia::Color::from_rgba(
        c.r.clamp(0.0, 1.0),
        c.g.clamp(0.0, 1.0),
        c.b.clamp(0.0, 1.0),
        c.a.clamp(0.0, 1.0),
    )
    .unwrap_or(tiny_skia::Color::TRANSPARENT)
}
