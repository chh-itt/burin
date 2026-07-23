//! CPU software rendering backend using tiny-skia.
//!
//! Maintains a full-frame pixel buffer.  Each frame, only damage rectangles
//! are cleared and repainted, then copied to the window surface via
//! softbuffer.
//!
//! ## Canonical pixel format (SSOT)
//!
//! `TinySkiaRenderer::pixels` is **always** tiny-skia's native format:
//! premultiplied RGBA bytes in memory (`[r, g, b, a]`), which as a
//! little-endian `u32` is `A<<24 | B<<16 | G<<8 | R`.  Every writer —
//! tiny-skia draw calls, the glyph-atlas blitter, image blits and
//! `clear_rect` — MUST produce this layout.  The one and only conversion
//! to softbuffer's `0RGB` (`R` in bits 16-23) happens in [`TinySkiaRenderer::present`]
//! via [`rgba_u32_to_softbuffer`].
//!
//! (Audit 2026-07-16: previously the buffer mixed two layouts — tiny-skia
//! wrote RGBA while text/images/clear wrote 0RGB — so every rect, gradient,
//! shadow and path displayed with red/blue swapped.)

pub(crate) mod clip;
pub mod damage;
pub mod glyph_atlas;
pub mod glyph_cache;
mod image;
pub(crate) mod shadow;
mod surface;
mod surface_cache;

use crate::core::error::{push_error, GpuErrorKind, UiError};
use crate::render::painter::DrawCommand;
use crate::render::path::{bezpath_bounds, bezpath_to_tiny_skia};
use crate::render::wgpu;
use crate::render::wgpu::glyphon_bridge::{self, TextAreaDesc};
use crate::style::Brush;
use crate::style::{Color, CornerRadii, Rect};
use glam::Affine2;

use glyph_atlas::GlyphAtlas;
use glyph_cache::GlyphCache;
use image::ImageCache;
use shadow::box_blur_rgba;
use surface::WindowSurface;
use surface_cache::TextSurfaceCache;

use std::collections::HashMap;
use std::rc::Rc;

pub use damage::DamageTracker;

#[derive(Debug)]
pub enum RendererError {
    Surface,
    NoAdapter,
}

impl std::fmt::Display for RendererError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Surface => write!(f, "CPU surface error"),
            Self::NoAdapter => write!(f, "no rendering adapter"),
        }
    }
}

impl std::error::Error for RendererError {}

pub struct TinySkiaRenderer {
    pixels: Vec<u32>,
    physical_size: (u32, u32),
    logical_size: (f32, f32),
    scale_factor: f32,

    /// `None` in headless mode (tests / benchmarks) or when the softbuffer
    /// surface could not be initialised: rasterisation runs identically,
    /// `present` becomes a no-op.
    surface: Option<WindowSurface>,

    /// Set when `WindowSurface::new()` failed.  When `true`, frame
    /// rendering is skipped entirely (no pixels work at all), but the
    /// window still responds to events.
    surface_init_failed: bool,

    pub glyph_cache: GlyphCache,
    pub swash_cache: cosmic_text::SwashCache,
    pub surface_cache: TextSurfaceCache,
    pub glyph_atlas: GlyphAtlas,

    image_cache: ImageCache,
    shadow_cache: shadow::ShadowCache,

    /// Damage tracking (dirty bounds → inflated, disjoint repaint rects).
    pub damage_tracker: DamageTracker,

    frame_count: u64,
    clear_color: Color,

    /// Cross-frame LRU cache of device-space clip masks. Keyed by the
    /// **device-space** clip geometry (transformed AABB + radii + optional
    /// damage intersection), so scroll frames — where the raw clip and the
    /// transform change in lockstep but the on-screen clip is fixed — hit the
    /// cache instead of re-rasterising a full-frame mask (~0.45 ms @1080p).
    /// Bounded by [`Self::MASK_CACHE_CAP`]; cleared on resize.
    clip_mask_cache: HashMap<[u32; 12], (Rc<tiny_skia::Mask>, u64)>,
    mask_tick: u64,
}

/// Result of resolving a clip + damage pair for one draw command.
enum ResolvedClip {
    /// Fully inside (damage ∩ clip), away from rounded corners: draw as-is.
    Unclipped,
    /// Nothing visible — skip the draw.
    Skip,
    /// Axis-aligned rect clip (raw space): intersect geometry where the
    /// primitive supports it, else convert to a mask via [`TinySkiaRenderer::rc_mask`].
    Rect(Rect),
    /// The primitive fully covers a rounded clip: the visible region IS the
    /// rounded clip rect. Rect-shaped primitives draw it geometrically
    /// (rounded fill, zero masks — the common "card/container background"
    /// case); others convert to a mask.
    RoundedRect(Rect, CornerRadii),
    /// Rounded-corner interaction: apply this device-space alpha mask.
    Mask(Rc<tiny_skia::Mask>),
}

impl TinySkiaRenderer {
    /// Cross-frame clip-mask LRU capacity. Full-frame masks are ~2 MB @1080p;
    /// a typical screen has 1-4 distinct rounded clip containers.
    const MASK_CACHE_CAP: usize = 8;

    pub fn new(
        window: std::sync::Arc<dyn winit::window::Window>,
        logical_width: f32,
        logical_height: f32,
        scale_factor: f32,
    ) -> Result<Self, RendererError> {
        let pw = (logical_width * scale_factor).ceil() as u32;
        let ph = (logical_height * scale_factor).ceil() as u32;

        let (surface, surface_init_failed) = match WindowSurface::new(window, pw, ph) {
            Ok(s) => (Some(s), false),
            Err(e) => {
                push_error(UiError::GpuInit(GpuErrorKind::Other(format!(
                    "WindowSurface::new failed: {e}"
                ))));
                (None, true)
            }
        };

        Ok(Self::build(
            surface,
            logical_width,
            logical_height,
            scale_factor,
            surface_init_failed,
        ))
    }

    /// Headless renderer: same rasterisation pipeline, no window/softbuffer.
    /// Used by pixel-accurate tests and performance benchmarks so they
    /// exercise the REAL production raster path (including text).
    pub fn new_headless(logical_width: f32, logical_height: f32, scale_factor: f32) -> Self {
        Self::build(None, logical_width, logical_height, scale_factor, false)
    }

    fn build(
        surface: Option<WindowSurface>,
        logical_width: f32,
        logical_height: f32,
        scale_factor: f32,
        surface_init_failed: bool,
    ) -> Self {
        let pw = (logical_width * scale_factor).ceil() as u32;
        let ph = (logical_height * scale_factor).ceil() as u32;
        Self {
            pixels: vec![0u32; pw as usize * ph as usize],
            physical_size: (pw, ph),
            logical_size: (logical_width, logical_height),
            scale_factor,
            surface,
            surface_init_failed,
            glyph_cache: GlyphCache::new(),
            swash_cache: cosmic_text::SwashCache::new(),
            image_cache: ImageCache::new(),
            shadow_cache: shadow::ShadowCache::new(),
            damage_tracker: DamageTracker::new(),
            surface_cache: TextSurfaceCache::new(),
            glyph_atlas: GlyphAtlas::new(2048, 2048),
            frame_count: 0,
            clear_color: Color::rgba8(26, 26, 31, 255),
            clip_mask_cache: HashMap::new(),
            mask_tick: 0,
        }
    }

    pub fn set_clear_color(&mut self, c: Color) {
        self.clear_color = c;
    }

    pub fn logical_size(&self) -> (f32, f32) {
        self.logical_size
    }

    /// Transform the damage rect (logical screen space) into a command's raw
    /// (pre-transform) space, as an AABB. All culling and clipping happens in
    /// raw space — the same space primitive rects and `clip_world_rect` live in.
    fn raw_damage(xform: Affine2, dmg: Rect) -> Rect {
        if xform == Affine2::IDENTITY {
            return dmg;
        }
        let det = xform.matrix2.determinant();
        if det.abs() < 1e-9 {
            // Degenerate (scale-0 animation frame): content is invisible;
            // return a huge rect so nothing is wrongly culled.
            return Rect::new(
                f32::MIN / 4.0,
                f32::MIN / 4.0,
                f32::MAX / 2.0,
                f32::MAX / 2.0,
            );
        }
        let inv = xform.inverse();
        let c = [
            inv.transform_point2(glam::Vec2::new(dmg.x, dmg.y)),
            inv.transform_point2(glam::Vec2::new(dmg.x + dmg.width, dmg.y)),
            inv.transform_point2(glam::Vec2::new(dmg.x + dmg.width, dmg.y + dmg.height)),
            inv.transform_point2(glam::Vec2::new(dmg.x, dmg.y + dmg.height)),
        ];
        let min_x = c.iter().map(|p| p.x).fold(f32::MAX, f32::min);
        let min_y = c.iter().map(|p| p.y).fold(f32::MAX, f32::min);
        let max_x = c.iter().map(|p| p.x).fold(f32::MIN, f32::max);
        let max_y = c.iter().map(|p| p.y).fold(f32::MIN, f32::max);
        Rect::new(min_x, min_y, max_x - min_x, max_y - min_y)
    }

    /// Does `prim` avoid all rounded-corner squares of the clip rect `abs`?
    /// When true, clipping `prim` to `abs` degenerates to an axis-aligned
    /// rect intersection — no alpha mask needed.
    fn corner_free(prim: &Rect, abs: &Rect, radius: &CornerRadii) -> bool {
        let hits = |cx: f32, cy: f32, r: f32| -> bool {
            r > 0.0
                && prim.x < cx + r
                && prim.x + prim.width > cx - r
                && prim.y < cy + r
                && prim.y + prim.height > cy - r
        };
        // Corner squares centred on the corner points, size r (the rounded
        // region is within r of the corner).
        !(hits(abs.x, abs.y, radius.top_left)
            || hits(abs.x + abs.width, abs.y, radius.top_right)
            || hits(abs.x + abs.width, abs.y + abs.height, radius.bottom_right)
            || hits(abs.x, abs.y + abs.height, radius.bottom_left))
    }

    /// Resolve a draw command's clip against both the element clip and the
    /// current damage rectangle (raw space).
    ///
    /// This is the CPU analogue of the GPU shader's per-fragment `clip_alpha`,
    /// with three cost tiers (audit 2026-07-16):
    /// 1. **Unclipped** — the primitive lies fully inside the (damage ∩ clip)
    ///    region and touches no rounded corner: the vast majority of draws.
    /// 2. **Rect** — axis-aligned intersection suffices (rect-shaped content
    ///    under an unrounded portion of the clip): geometric, zero masks.
    /// 3. **Mask** — rounded-corner interaction or non-rect content on the
    ///    clip boundary: full-frame alpha mask, cached **across frames** with
    ///    a device-space-stable key (scrolling reuses the same mask; the old
    ///    per-frame transform-keyed cache rebuilt a ~2 MB mask every frame,
    ///    ~0.45 ms each @1080p).
    ///
    /// Clipping to the damage rect is a *correctness* requirement, not just an
    /// optimisation: damage passes re-execute the whole command list, so a
    /// translucent command straddling two damage rects would otherwise be
    /// composited twice (audit issue C5).
    fn resolve_clip(
        &mut self,
        clip: &crate::render::painter::ClipInfo,
        xform: Affine2,
        prim: Rect,
        dmg_raw: Rect,
        dmg_covers_all: bool,
    ) -> ResolvedClip {
        let abs = clip::clip_world_rect(clip);
        if abs.width <= 0.0 || abs.height <= 0.0 {
            return ResolvedClip::Skip;
        }
        let eff = if dmg_covers_all {
            abs
        } else {
            match abs.intersection(&dmg_raw) {
                Some(r) => r,
                None => return ResolvedClip::Skip,
            }
        };
        if !prim.intersects(&eff) {
            return ResolvedClip::Skip;
        }
        let cf = Self::corner_free(&prim, &abs, &clip.radius);
        let contained = cf
            && prim.x >= eff.x
            && prim.y >= eff.y
            && prim.x + prim.width <= eff.x + eff.width
            && prim.y + prim.height <= eff.y + eff.height;
        if contained {
            return ResolvedClip::Unclipped;
        }
        if cf {
            // Straddles the rect boundary but not a rounded corner.
            return ResolvedClip::Rect(eff);
        }
        // Rounded-corner interaction. If the primitive fully covers the clip
        // rect (container backgrounds filling their own rounded clip — the
        // dominant case), the visible region IS the rounded clip rect: rect
        // primitives can draw it geometrically, no mask needed.
        if eff == abs
            && prim.x <= abs.x
            && prim.y <= abs.y
            && prim.x + prim.width >= abs.x + abs.width
            && prim.y + prim.height >= abs.y + abs.height
        {
            return ResolvedClip::RoundedRect(abs, clip.radius);
        }
        // General case → mask (cached, device-space key).
        let mask = self.clip_mask(
            abs,
            clip.radius,
            xform,
            if eff == abs { None } else { Some(eff) },
        );
        match mask {
            Some(m) => ResolvedClip::Mask(m),
            None => ResolvedClip::Skip,
        }
    }

    /// Fetch or build a device-space clip mask, LRU-cached across frames.
    ///
    /// The key is the **device-space** AABB of the transformed clip (plus
    /// radii and the optional damage intersection): scroll frames translate
    /// the raw clip and the transform in lockstep, so the device position —
    /// and therefore the mask — is identical, and the cache hits every frame.
    fn clip_mask(
        &mut self,
        abs: Rect,
        radius: CornerRadii,
        xform: Affine2,
        intersect_raw: Option<Rect>,
    ) -> Option<Rc<tiny_skia::Mask>> {
        let sf = self.scale_factor;
        let m = xform.matrix2;
        let axis_aligned = m.x_axis.y == 0.0 && m.y_axis.x == 0.0;
        let p0 = xform.transform_point2(glam::Vec2::new(abs.x, abs.y));
        let p1 = xform.transform_point2(glam::Vec2::new(abs.x + abs.width, abs.y + abs.height));
        let (ix0, ix1) = intersect_raw.map_or((0u32, 0u32), |r| {
            let q0 = xform.transform_point2(glam::Vec2::new(r.x, r.y));
            let q1 = xform.transform_point2(glam::Vec2::new(r.x + r.width, r.y + r.height));
            (
                (q0.x * sf).to_bits() ^ (q0.y * sf).to_bits().rotate_left(16),
                (q1.x * sf).to_bits() ^ (q1.y * sf).to_bits().rotate_left(16),
            )
        });
        let mhash = if axis_aligned {
            m.x_axis.x.to_bits() ^ m.y_axis.y.to_bits().rotate_left(16)
        } else {
            m.x_axis.x.to_bits()
                ^ m.x_axis.y.to_bits().rotate_left(8)
                ^ m.y_axis.x.to_bits().rotate_left(16)
                ^ m.y_axis.y.to_bits().rotate_left(24)
        };
        let key = [
            (p0.x * sf).to_bits(),
            (p0.y * sf).to_bits(),
            (p1.x * sf).to_bits(),
            (p1.y * sf).to_bits(),
            radius.top_left.to_bits(),
            radius.top_right.to_bits(),
            radius.bottom_right.to_bits(),
            radius.bottom_left.to_bits(),
            ix0,
            ix1,
            mhash,
            if axis_aligned { 0 } else { 1 },
        ];
        self.mask_tick += 1;
        if let Some(entry) = self.clip_mask_cache.get_mut(&key) {
            entry.1 = self.mask_tick;
            return Some(entry.0.clone());
        }
        let (pw, ph) = self.physical_size;
        match clip::build_clip_mask_rect(abs, radius, xform, intersect_raw, pw, ph, sf) {
            clip::ClipMask::Mask(mask) => {
                if self.clip_mask_cache.len() >= Self::MASK_CACHE_CAP {
                    if let Some(&k) = self
                        .clip_mask_cache
                        .iter()
                        .min_by_key(|(_, (_, t))| *t)
                        .map(|(k, _)| k)
                    {
                        self.clip_mask_cache.remove(&k);
                    }
                }
                let rc = Rc::new(mask);
                self.clip_mask_cache
                    .insert(key, (rc.clone(), self.mask_tick));
                Some(rc)
            }
            clip::ClipMask::None => None,
            clip::ClipMask::Skip => None,
        }
    }

    /// Render damage: clear rects, then execute commands, text areas and
    /// backdrop-blur regions interleaved by z_index.
    pub fn render_damage(
        &mut self,
        damage_rects: &[Rect],
        commands: &mut Vec<DrawCommand>,
        text_areas: &mut Vec<TextAreaDesc>,
        backdrop_regions: &[crate::render::BackdropRegion],
    ) {
        if self.surface_init_failed {
            return;
        }
        let bg_argb = color_to_rgba_u32(self.clear_color);
        commands.sort_by_key(|c| c.z_index());
        text_areas.sort_by_key(|a| a.z_index);
        let mut backdrops: Vec<&crate::render::BackdropRegion> = backdrop_regions.iter().collect();
        backdrops.sort_by_key(|b| b.z_index);

        for damage_rect in damage_rects {
            self.clear_rect(*damage_rect, bg_argb);
            self.execute_commands_interleaved(commands, text_areas, &backdrops, *damage_rect);
        }
    }

    /// Execute commands, text areas and backdrop regions in z_index order so
    /// that backdrops blur exactly the content below them and overlays
    /// correctly cover content behind them.
    fn execute_commands_interleaved(
        &mut self,
        commands: &[DrawCommand],
        text_areas: &[TextAreaDesc],
        backdrops: &[&crate::render::BackdropRegion],
        damage_rect: Rect,
    ) {
        let mut ci = 0usize;
        let mut ti = 0usize;
        let mut bi = 0usize;
        let dmg_phys = logical_to_skia_rect_impl(damage_rect, self.scale_factor);
        let (lw, lh) = self.logical_size;
        let dmg_covers_all = damage_rect.x <= 0.5
            && damage_rect.y <= 0.5
            && damage_rect.x + damage_rect.width >= lw - 0.5
            && damage_rect.y + damage_rect.height >= lh - 0.5;

        while ci < commands.len() || ti < text_areas.len() {
            let cmd_z = commands.get(ci).map(|c| c.z_index());
            let ta_z = text_areas.get(ti).map(|a| a.z_index);

            let do_cmd = match (cmd_z, ta_z) {
                (Some(cz), Some(tz)) => cz <= tz,
                (Some(_), None) => true,
                (None, Some(_)) => false,
                (None, None) => break,
            };
            // Backdrop regions blur everything strictly below their z; apply
            // any due region before drawing the next item at z >= region.z.
            let next_z = if do_cmd {
                cmd_z.unwrap()
            } else {
                ta_z.unwrap()
            };
            while bi < backdrops.len() && backdrops[bi].z_index <= next_z {
                self.apply_backdrop(backdrops[bi], damage_rect);
                bi += 1;
            }

            if do_cmd {
                let cmd = &commands[ci];
                ci += 1;
                let xform = *match cmd {
                    DrawCommand::FillRect { transform, .. } => transform,
                    DrawCommand::StrokeRect { transform, .. } => transform,
                    DrawCommand::FillShadow { transform, .. } => transform,
                    DrawCommand::FillLinearGradient { transform, .. } => transform,
                    DrawCommand::DrawImage { transform, .. } => transform,
                    DrawCommand::FillPath { transform, .. } => transform,
                    DrawCommand::StrokePath { transform, .. } => transform,
                };
                // Primitive AABB in raw space (used for culling + clip tiering).
                let prim = match cmd {
                    DrawCommand::FillRect { rect, .. } => *rect,
                    DrawCommand::StrokeRect { rect, width, .. } => rect.expand(*width),
                    DrawCommand::FillShadow { rect, .. } => *rect,
                    DrawCommand::FillLinearGradient {
                        rect, stroke_width, ..
                    } => rect.expand(*stroke_width),
                    DrawCommand::DrawImage { rect, .. } => *rect,
                    DrawCommand::FillPath { path, .. } => {
                        match crate::render::path::bezpath_bounds(path) {
                            Some(b) => b,
                            None => continue,
                        }
                    }
                    DrawCommand::StrokePath { path, stroke, .. } => {
                        match crate::render::path::bezpath_bounds(path) {
                            Some(b) => b.expand(stroke.width as f32),
                            None => continue,
                        }
                    }
                };
                let dmg_raw = Self::raw_damage(xform, damage_rect);
                if !prim.intersects(&dmg_raw) {
                    continue;
                }
                let clip = cmd.clip();
                let rc = self.resolve_clip(&clip, xform, prim, dmg_raw, dmg_covers_all);
                if matches!(rc, ResolvedClip::Skip) {
                    continue;
                }

                match cmd {
                    DrawCommand::FillRect {
                        rect,
                        color,
                        radius,
                        blend_mode,
                        ..
                    } => {
                        match &rc {
                            ResolvedClip::Rect(eff) => {
                                if let Some((r2, rad2)) =
                                    Self::clip_rounded_rect_geometric(*rect, *radius, *eff)
                                {
                                    self.fill_rect_cpu(r2, *color, rad2, xform, None, *blend_mode);
                                } else {
                                    let mask = self.rc_mask(&rc, xform);
                                    self.fill_rect_cpu(
                                        *rect,
                                        *color,
                                        *radius,
                                        xform,
                                        mask.as_deref(),
                                        *blend_mode,
                                    );
                                }
                            }
                            ResolvedClip::RoundedRect(abs, cr) => {
                                // Primitive covers the rounded clip: the visible
                                // region is the clip rect rounded by the combined
                                // radii (exact when corners coincide or the prim's
                                // rounding falls outside the clip; else mask).
                                if let Some(rad2) =
                                    Self::combine_covering_radii(rect, radius, abs, cr)
                                {
                                    self.fill_rect_cpu(
                                        *abs,
                                        *color,
                                        rad2,
                                        xform,
                                        None,
                                        *blend_mode,
                                    );
                                } else {
                                    let mask = self.rc_mask(&rc, xform);
                                    self.fill_rect_cpu(
                                        *rect,
                                        *color,
                                        *radius,
                                        xform,
                                        mask.as_deref(),
                                        *blend_mode,
                                    );
                                }
                            }
                            _ => {
                                let mask = self.rc_mask(&rc, xform);
                                self.fill_rect_cpu(
                                    *rect,
                                    *color,
                                    *radius,
                                    xform,
                                    mask.as_deref(),
                                    *blend_mode,
                                );
                            }
                        }
                    }
                    DrawCommand::StrokeRect {
                        rect,
                        color,
                        width,
                        radius,
                        blend_mode,
                        ..
                    } => {
                        let mask = self.rc_mask(&rc, xform);
                        self.stroke_rect_cpu(
                            *rect,
                            *color,
                            *width,
                            *radius,
                            xform,
                            mask.as_deref(),
                            *blend_mode,
                        );
                    }
                    DrawCommand::FillShadow {
                        rect,
                        radius,
                        shadow,
                        ..
                    } => {
                        // The geometric device-clip shortcut is only valid for
                        // pure-translation transforms; otherwise use a mask.
                        let identity_m = xform.matrix2 == glam::Mat2::IDENTITY;
                        let (mask, clip_dev) = match &rc {
                            ResolvedClip::Rect(eff) if identity_m => {
                                let sf = self.scale_factor;
                                let t = xform.translation;
                                (
                                    None,
                                    Some((
                                        ((eff.x + t.x) * sf).floor() as i32,
                                        ((eff.y + t.y) * sf).floor() as i32,
                                        ((eff.x + eff.width + t.x) * sf).ceil() as i32,
                                        ((eff.y + eff.height + t.y) * sf).ceil() as i32,
                                    )),
                                )
                            }
                            _ => (self.rc_mask(&rc, xform), None),
                        };
                        let (pw, ph) = self.physical_size;
                        let sf = self.scale_factor;
                        shadow::draw_shadow_clipped(
                            &mut self.shadow_cache,
                            bytemuck::cast_slice_mut(&mut self.pixels),
                            pw,
                            ph,
                            *rect,
                            *radius,
                            shadow,
                            xform,
                            sf,
                            mask.as_deref(),
                            clip_dev,
                        );
                    }
                    DrawCommand::FillLinearGradient {
                        rect,
                        gradient,
                        radius,
                        stroke_width,
                        ..
                    } => match &rc {
                        ResolvedClip::Rect(eff) if radius.is_zero() && *stroke_width == 0.0 => {
                            if let Some(r) = rect.intersection(eff) {
                                self.draw_gradient_cpu(
                                    *rect,
                                    r,
                                    *gradient,
                                    *radius,
                                    *stroke_width,
                                    xform,
                                    None,
                                );
                            }
                        }
                        ResolvedClip::RoundedRect(abs, cr)
                            if radius.is_zero() && *stroke_width == 0.0 =>
                        {
                            self.draw_gradient_cpu(
                                *rect,
                                *abs,
                                *gradient,
                                *cr,
                                *stroke_width,
                                xform,
                                None,
                            );
                        }
                        _ => {
                            let mask = self.rc_mask(&rc, xform);
                            self.draw_gradient_cpu(
                                *rect,
                                *rect,
                                *gradient,
                                *radius,
                                *stroke_width,
                                xform,
                                mask.as_deref(),
                            );
                        }
                    },
                    DrawCommand::DrawImage {
                        hash,
                        rect,
                        opacity,
                        content_fit,
                        ..
                    } => match &rc {
                        ResolvedClip::Rect(eff) => {
                            self.draw_image_cpu(
                                *hash,
                                *rect,
                                Some(*eff),
                                *opacity,
                                *content_fit,
                                xform,
                                None,
                            );
                        }
                        _ => {
                            let mask = self.rc_mask(&rc, xform);
                            self.draw_image_cpu(
                                *hash,
                                *rect,
                                None,
                                *opacity,
                                *content_fit,
                                xform,
                                mask.as_deref(),
                            );
                        }
                    },
                    DrawCommand::FillPath { path, brush, .. } => {
                        let mask = self.rc_mask(&rc, xform);
                        self.fill_path_cpu(path, brush, xform, mask.as_deref());
                    }
                    DrawCommand::StrokePath {
                        path,
                        stroke,
                        brush,
                        ..
                    } => {
                        let mask = self.rc_mask(&rc, xform);
                        self.stroke_path_cpu(path, stroke.clone(), brush, xform, mask.as_deref());
                    }
                }
            } else {
                // Render single text area
                self.render_single_text_area(&text_areas[ti], &[dmg_phys]);
                ti += 1;
            }
        }
        while bi < backdrops.len() {
            self.apply_backdrop(backdrops[bi], damage_rect);
            bi += 1;
        }
    }

    /// Combine the radii of a rounded primitive that fully covers a rounded
    /// clip rect. Per corner: coincident corners round by `max(prim, clip)`;
    /// prim corners whose rounded square lies outside the clip contribute
    /// nothing; anything else needs a mask (`None`).
    fn combine_covering_radii(
        prim: &Rect,
        prim_r: &CornerRadii,
        abs: &Rect,
        clip_r: &CornerRadii,
    ) -> Option<CornerRadii> {
        let eps = 0.01f32;
        let px1 = prim.x + prim.width;
        let py1 = prim.y + prim.height;
        let ax1 = abs.x + abs.width;
        let ay1 = abs.y + abs.height;
        let corner = |rp: f32,
                      rc: f32,
                      pcx: f32,
                      pcy: f32,
                      acx: f32,
                      acy: f32,
                      on_left: bool,
                      on_top: bool|
         -> Option<f32> {
            if (pcx - acx).abs() < eps && (pcy - acy).abs() < eps {
                return Some(rp.max(rc));
            }
            if rp <= 0.0 {
                return Some(rc);
            }
            // Prim's rounded corner square must lie fully outside the clip.
            let (sx0, sx1) = if on_left {
                (prim.x, prim.x + rp)
            } else {
                (px1 - rp, px1)
            };
            let (sy0, sy1) = if on_top {
                (prim.y, prim.y + rp)
            } else {
                (py1 - rp, py1)
            };
            let outside =
                sx1 <= abs.x + eps || sx0 >= ax1 - eps || sy1 <= abs.y + eps || sy0 >= ay1 - eps;
            if outside {
                Some(rc)
            } else {
                None
            }
        };
        Some(CornerRadii {
            top_left: corner(
                prim_r.top_left,
                clip_r.top_left,
                prim.x,
                prim.y,
                abs.x,
                abs.y,
                true,
                true,
            )?,
            top_right: corner(
                prim_r.top_right,
                clip_r.top_right,
                px1,
                prim.y,
                ax1,
                abs.y,
                false,
                true,
            )?,
            bottom_right: corner(
                prim_r.bottom_right,
                clip_r.bottom_right,
                px1,
                py1,
                ax1,
                ay1,
                false,
                false,
            )?,
            bottom_left: corner(
                prim_r.bottom_left,
                clip_r.bottom_left,
                prim.x,
                py1,
                abs.x,
                ay1,
                true,
                false,
            )?,
        })
    }

    /// Geometric clip of a rounded rect by an axis-aligned rect. Returns the
    /// intersected rect + surviving per-corner radii, or `None` when the clip
    /// boundary cuts through a rounded corner (mask required for exactness).
    fn clip_rounded_rect_geometric(
        prim: Rect,
        radius: CornerRadii,
        eff: Rect,
    ) -> Option<(Rect, CornerRadii)> {
        let i = prim.intersection(&eff)?;
        let ix1 = i.x + i.width;
        let iy1 = i.y + i.height;
        let px1 = prim.x + prim.width;
        let py1 = prim.y + prim.height;
        let eps = 0.01f32;
        // For each corner: survives (intersection corner coincides with the
        // prim corner), or its r×r square is fully clipped away (→ radius 0),
        // or it is partially cut (→ None, caller uses a mask).
        let corner = |r: f32, on_left: bool, on_top: bool, icx: f32, icy: f32| -> Option<f32> {
            if r <= 0.0 {
                return Some(0.0);
            }
            let cx = if on_left { prim.x } else { px1 };
            let cy = if on_top { prim.y } else { py1 };
            if (cx - icx).abs() < eps && (cy - icy).abs() < eps {
                return Some(r); // untouched
            }
            let (sx0, sx1) = if on_left {
                (prim.x, prim.x + r)
            } else {
                (px1 - r, px1)
            };
            let (sy0, sy1) = if on_top {
                (prim.y, prim.y + r)
            } else {
                (py1 - r, py1)
            };
            let outside =
                sx1 <= i.x + eps || sx0 >= ix1 - eps || sy1 <= i.y + eps || sy0 >= iy1 - eps;
            if outside {
                Some(0.0)
            } else {
                None
            }
        };
        let tl = corner(radius.top_left, true, true, i.x, i.y)?;
        let tr = corner(radius.top_right, false, true, ix1, i.y)?;
        let br = corner(radius.bottom_right, false, false, ix1, iy1)?;
        let bl = corner(radius.bottom_left, true, false, i.x, iy1)?;
        Some((
            i,
            CornerRadii {
                top_left: tl,
                top_right: tr,
                bottom_right: br,
                bottom_left: bl,
            },
        ))
    }

    /// Convert a [`ResolvedClip`] to a drawable mask. `Rect` variants (from
    /// primitives that can't be clipped geometrically) build a radius-free
    /// mask via the same LRU cache.
    fn rc_mask(&mut self, rc: &ResolvedClip, xform: Affine2) -> Option<Rc<tiny_skia::Mask>> {
        match rc {
            ResolvedClip::Mask(m) => Some(m.clone()),
            ResolvedClip::Rect(eff) => self.clip_mask(*eff, CornerRadii::ZERO, xform, None),
            ResolvedClip::RoundedRect(abs, cr) => self.clip_mask(*abs, *cr, xform, None),
            _ => None,
        }
    }

    /// Backdrop-filter (blur + tint behind an element) — CPU implementation.
    /// Copies the region, applies a 3-pass box blur (≈ gaussian, matching the
    /// GPU's separable blur), tints, then composites back through the rounded
    /// -corner coverage. Confined to the current damage rect.
    fn apply_backdrop(&mut self, region: &crate::render::BackdropRegion, damage_rect: Rect) {
        if region.blur_radius <= 0.0 && region.tint.is_none() {
            return;
        }
        let sf = self.scale_factor;
        let (pw, ph) = self.physical_size;
        // Device AABB of the (transformed) region.
        let c = [
            region
                .transform
                .transform_point2(glam::Vec2::new(region.rect.x, region.rect.y)),
            region.transform.transform_point2(glam::Vec2::new(
                region.rect.x + region.rect.width,
                region.rect.y + region.rect.height,
            )),
        ];
        let x0 = ((c[0].x.min(c[1].x) * sf).floor().max(0.0)) as usize;
        let y0 = ((c[0].y.min(c[1].y) * sf).floor().max(0.0)) as usize;
        let x1 = ((c[0].x.max(c[1].x) * sf).ceil()).min(pw as f32) as usize;
        let y1 = ((c[0].y.max(c[1].y) * sf).ceil()).min(ph as f32) as usize;
        if x1 <= x0 || y1 <= y0 {
            return;
        }
        // Damage confinement (device px).
        let dx0 = ((damage_rect.x * sf).floor().max(0.0)) as usize;
        let dy0 = ((damage_rect.y * sf).floor().max(0.0)) as usize;
        let dx1 = (((damage_rect.x + damage_rect.width) * sf).ceil()).min(pw as f32) as usize;
        let dy1 = (((damage_rect.y + damage_rect.height) * sf).ceil()).min(ph as f32) as usize;

        let w = x1 - x0;
        let h = y1 - y0;
        // Snapshot the region (blur source: content below this z).
        let mut src: Vec<u32> = Vec::with_capacity(w * h);
        for y in y0..y1 {
            let row = y * pw as usize;
            src.extend_from_slice(&self.pixels[row + x0..row + x1]);
        }
        let blurred = if region.blur_radius > 0.0 {
            box_blur_rgba(&src, w, h, region.blur_radius * sf * 0.5)
        } else {
            src.clone()
        };
        // Rounded-corner coverage (device space, AA).
        let coverage = {
            let mut cov = tiny_skia::Mask::new(w as u32, h as u32);
            match cov.as_mut() {
                Some(m) => {
                    let r = tiny_skia::Rect::from_xywh(0.0, 0.0, w as f32, h as f32).unwrap();
                    let radii_dev = CornerRadii {
                        top_left: region.corner_radius.top_left * sf,
                        top_right: region.corner_radius.top_right * sf,
                        bottom_right: region.corner_radius.bottom_right * sf,
                        bottom_left: region.corner_radius.bottom_left * sf,
                    };
                    let path = rounded_skia_rect(r, radii_dev, 1.0);
                    m.fill_path(
                        &path,
                        tiny_skia::FillRule::Winding,
                        true,
                        tiny_skia::Transform::identity(),
                    );
                }
                None => return,
            }
            cov.unwrap()
        };
        let cov_data = coverage.data();
        // Tint (premultiplied source-over on the blurred content).
        let (tr, tg, tb, ta) = region.tint.map_or((0, 0, 0, 0u32), |t| {
            (
                (t.r * t.a * 255.0) as u32,
                (t.g * t.a * 255.0) as u32,
                (t.b * t.a * 255.0) as u32,
                (t.a * 255.0) as u32,
            )
        });
        for y in y0..y1 {
            if y < dy0 || y >= dy1 {
                continue;
            }
            let row = y * pw as usize;
            let ly = y - y0;
            for x in x0.max(dx0)..x1.min(dx1) {
                let lx = x - x0;
                let cv = cov_data[ly * w + lx] as u32;
                if cv == 0 {
                    continue;
                }
                let mut b = blurred[ly * w + lx];
                if ta > 0 {
                    let inv = 255 - ta;
                    let br = (b & 0xFF) * inv / 255 + tr;
                    let bg = ((b >> 8) & 0xFF) * inv / 255 + tg;
                    let bb = ((b >> 16) & 0xFF) * inv / 255 + tb;
                    let ba = ((b >> 24) & 0xFF) * inv / 255 + ta;
                    b = (ba.min(255) << 24)
                        | (bb.min(255) << 16)
                        | (bg.min(255) << 8)
                        | br.min(255);
                }
                if cv == 255 {
                    self.pixels[row + x] = b;
                } else {
                    let o = self.pixels[row + x];
                    let icv = 255 - cv;
                    let lr = ((b & 0xFF) * cv + (o & 0xFF) * icv) / 255;
                    let lg = (((b >> 8) & 0xFF) * cv + ((o >> 8) & 0xFF) * icv) / 255;
                    let lb = (((b >> 16) & 0xFF) * cv + ((o >> 16) & 0xFF) * icv) / 255;
                    let la = (((b >> 24) & 0xFF) * cv + ((o >> 24) & 0xFF) * icv) / 255;
                    self.pixels[row + x] = (la << 24) | (lb << 16) | (lg << 8) | lr;
                }
            }
        }
    }

    /// Render a single text area (extracted from render_text_areas for interleaved execution).
    fn render_single_text_area(&mut self, area: &TextAreaDesc, dmg_phys: &[tiny_skia::Rect]) {
        let sf = self.scale_factor;
        let (pw_u, ph_u) = self.physical_size;
        let clip_phys = area.clip_rect.as_ref().and_then(|cr| {
            tiny_skia::Rect::from_xywh(
                cr.x * sf,
                cr.y * sf,
                (cr.width * sf).max(0.01),
                (cr.height * sf).max(0.01),
            )
        });

        // Effective visible region for this damage pass: the text's own clip
        // intersected with the current damage rect. Outside this region, the
        // text is either unchanged (no repaint needed) or has already been
        // cleared to the background — drawing there would alpha-blend on top of
        // old glyphs, causing the "bold text" artifact.
        let text_region = clip_phys.unwrap_or_else(|| {
            tiny_skia::Rect::from_xywh(0.0, 0.0, pw_u as f32, ph_u as f32)
                .unwrap_or_else(|| tiny_skia::Rect::from_xywh(0.0, 0.0, 1.0, 1.0).unwrap())
        });
        let eff_clip = match dmg_phys.first() {
            Some(dmg) => {
                let ix0 = text_region.x().max(dmg.x());
                let iy0 = text_region.y().max(dmg.y());
                let ix1 = (text_region.x() + text_region.width()).min(dmg.x() + dmg.width());
                let iy1 = (text_region.y() + text_region.height()).min(dmg.y() + dmg.height());
                if ix1 > ix0 && iy1 > iy0 {
                    tiny_skia::Rect::from_xywh(ix0, iy0, ix1 - ix0, iy1 - iy0)
                } else {
                    None
                }
            }
            None => None,
        };

        // Nothing of this text area falls inside the current damage rect.
        let Some(eff) = eff_clip else {
            return;
        };

        // Does the effective clip cover the whole surface?
        let covers = eff.x() <= 0.5
            && eff.y() <= 0.5
            && eff.x() + eff.width() >= pw_u as f32 - 0.5
            && eff.y() + eff.height() >= ph_u as f32 - 0.5;

        // Surface cache fast path (generation > 0)
        if area.generation > 0 {
            let clip_dev: Option<(i32, i32, i32, i32)> = if covers {
                None
            } else {
                Some((
                    eff.x().floor() as i32,
                    eff.y().floor() as i32,
                    (eff.x() + eff.width()).ceil() as i32,
                    (eff.y() + eff.height()).ceil() as i32,
                ))
            };
            if !glyphon_bridge::ensure_font_system() {
                return;
            }
            glyphon_bridge::FONT_SYSTEM.with(|fs_cell| {
                let mut font_system = fs_cell.borrow_mut();
                let fs = font_system
                    .as_mut()
                    .expect("ensure_font_system guarantees Some");
                let surf = self.surface_cache.get_or_render(
                    area.element_id,
                    area.generation,
                    area.color,
                    || {
                        Self::render_full_text_area(
                            &mut self.glyph_cache,
                            &mut self.swash_cache,
                            area,
                            sf,
                            fs,
                        )
                    },
                );
                if let Some((crop_dx, crop_dy, pixmap)) = surf {
                    // The surface is cropped to the ink extents; crop offsets
                    // are relative to (area.left, area.top) in device pixels.
                    let px = (area.left * sf) as i32 + crop_dx as i32;
                    let py = (area.top * sf) as i32 + crop_dy as i32;
                    // Clipped source-over blit (canonical RGBA): replaces the
                    // former full-frame text mask (~2 MB alloc per text clip).
                    let (pw, ph) = self.physical_size;
                    blit_text_surface(
                        bytemuck::cast_slice_mut(&mut self.pixels),
                        pw as usize,
                        ph as usize,
                        &pixmap,
                        px,
                        py,
                        area.color.a,
                        clip_dev,
                    );
                }
            });
            return;
        }

        // Fallback: per-glyph via atlas. Clip each glyph to (text clip ∩ damage).
        let clip_bounds: Option<(i32, i32, i32, i32)> = if covers {
            None
        } else {
            Some((
                eff.x().floor() as i32,
                eff.y().floor() as i32,
                (eff.x() + eff.width()).ceil() as i32,
                (eff.y() + eff.height()).ceil() as i32,
            ))
        };
        if !glyphon_bridge::ensure_font_system() {
            return;
        }
        glyphon_bridge::FONT_SYSTEM.with(|fs_cell| {
            let mut font_system = fs_cell.borrow_mut();
            let font_system = font_system
                .as_mut()
                .expect("ensure_font_system guarantees Some");
            let left = area.left - area.scroll_x;
            let top = area.top - area.scroll_y;
            let buf = area.buffer.borrow();
            let color_rgba: [u8; 4] = [
                (area.color.r * 255.0) as u8,
                (area.color.g * 255.0) as u8,
                (area.color.b * 255.0) as u8,
                (area.color.a * 255.0) as u8,
            ];
            let atlas = &mut self.glyph_atlas;
            let swash_cache = &mut self.swash_cache;
            let pixels: &mut [u32] = bytemuck::cast_slice_mut(&mut self.pixels);
            let (pw, ph) = self.physical_size;
            let scale = area.scale;
            for run in buf.layout_runs() {
                let run_offset = (left * scale, (top + run.line_y) * scale);
                for glyph in run.glyphs {
                    let phys = glyph.physical(run_offset, scale);
                    let gx = phys.x;
                    let gy = phys.y;
                    if !dmg_phys.iter().any(|d| point_in_rect(gx, gy, *d)) {
                        continue;
                    }
                    // Coarse pre-cull against the clip rect (a generous glyph
                    // bbox); straddling glyphs survive and are clipped per-pixel
                    // by `blit_to` below.
                    if let Some(ref cp) = clip_phys {
                        let approx_w = 2.0 * scale;
                        let approx_h = 2.0 * scale;
                        if gx as f32 + approx_w < cp.x()
                            || gy as f32 + approx_h < cp.y()
                            || gx as f32 > cp.x() + cp.width()
                            || gy as f32 > cp.y() + cp.height()
                        {
                            continue;
                        }
                    }
                    let key = glyph_atlas::AtlasKey::from_cache_key(
                        phys.cache_key,
                        phys.cache_key.font_id,
                    );
                    let entry = atlas.get_or_insert(key, || {
                        let image =
                            swash_cache.get_image_uncached(&mut *font_system, phys.cache_key)?;
                        let iw = image.placement.width;
                        let ih = image.placement.height;
                        if iw == 0 || ih == 0 {
                            return None;
                        }
                        match image.content {
                            cosmic_text::SwashContent::Mask => Some((
                                image.data.to_vec(),
                                iw,
                                ih,
                                image.placement.left,
                                image.placement.top,
                            )),
                            cosmic_text::SwashContent::SubpixelMask => {
                                let num = (iw * ih) as usize;
                                let mut alpha = vec![0u8; num];
                                for i in 0..num {
                                    let r = image.data[i * 3];
                                    let g = image.data[i * 3 + 1];
                                    let b = image.data[i * 3 + 2];
                                    alpha[i] = ((r as u16 + g as u16 + b as u16) / 3) as u8;
                                }
                                Some((alpha, iw, ih, image.placement.left, image.placement.top))
                            }
                            cosmic_text::SwashContent::Color => {
                                let mut alpha = vec![0u8; (iw * ih) as usize];
                                for i in 0..(iw * ih) as usize {
                                    alpha[i] = image.data[i * 4 + 3];
                                }
                                Some((alpha, iw, ih, image.placement.left, image.placement.top))
                            }
                        }
                    });
                    if let Some(entry) = entry {
                        atlas.blit_to(
                            &entry,
                            gx + entry.left,
                            gy - entry.top,
                            pixels,
                            pw,
                            ph,
                            color_rgba,
                            clip_bounds,
                        );
                    }
                }
            }
        });
    }

    /// Render a text area into a pixmap **cropped to the ink extents**.
    ///
    /// Returns `(pixmap, crop_dx, crop_dy)`: the pixmap's device-pixel offset
    /// relative to `(area.left, area.top) * sf`. Pre-audit the pixmap was
    /// allocated at the full clip width (a 1280-wide row → 33 k mostly-empty
    /// pixels per label, ~5 ms of blend work for 50 labels per frame);
    /// cropping to the glyph bounding box makes the blit proportional to the
    /// actual ink.
    fn render_full_text_area(
        glyph_cache: &mut GlyphCache,
        swash_cache: &mut cosmic_text::SwashCache,
        area: &TextAreaDesc,
        sf: f32,
        font_system: &mut cosmic_text::FontSystem,
    ) -> Option<(tiny_skia::Pixmap, f32, f32)> {
        let left = area.left - area.scroll_x;
        let top = area.top - area.scroll_y;
        let buf = area.buffer.borrow();
        let color_rgb = [
            (area.color.r * 255.0) as u8,
            (area.color.g * 255.0) as u8,
            (area.color.b * 255.0) as u8,
        ];

        // Pass 1: rasterise glyphs (cached) and collect placements + ink bbox.
        let mut placed: Vec<(std::rc::Rc<tiny_skia::Pixmap>, i32, i32)> = Vec::new();
        let (mut min_x, mut min_y) = (i32::MAX, i32::MAX);
        let (mut max_x, mut max_y) = (i32::MIN, i32::MIN);
        for run in buf.layout_runs() {
            let run_offset = (left * sf, (top + run.line_y) * sf);
            for glyph in run.glyphs {
                let phys = glyph.physical(run_offset, sf);
                let gx = phys.x as f32 - left * sf;
                let gy = phys.y as f32 - top * sf;
                let font_size = f32::from_bits(phys.cache_key.font_size_bits);
                let cached = glyph_cache.rasterize(
                    phys.cache_key.font_id,
                    phys.cache_key.glyph_id,
                    font_size,
                    color_rgb,
                    || {
                        let image = swash_cache.get_image_uncached(font_system, phys.cache_key)?;
                        let iw = image.placement.width;
                        let ih = image.placement.height;
                        if iw == 0 || ih == 0 {
                            return None;
                        }
                        let mut gp = tiny_skia::Pixmap::new(iw, ih)?;
                        let dst = bytemuck::cast_slice_mut::<u8, u32>(gp.data_mut());
                        // Canonical tiny-skia premultiplied RGBA: u32 = A<<24|B<<16|G<<8|R.
                        match image.content {
                            cosmic_text::SwashContent::Mask => {
                                for i in 0..(iw * ih) as usize {
                                    let a = image.data[i] as u32;
                                    let r = color_rgb[0] as u32 * a / 255;
                                    let g = color_rgb[1] as u32 * a / 255;
                                    let b = color_rgb[2] as u32 * a / 255;
                                    dst[i] = (a << 24) | (b << 16) | (g << 8) | r;
                                }
                            }
                            cosmic_text::SwashContent::Color => {
                                // Swash colour glyphs are straight-alpha RGBA; premultiply
                                // for tiny-skia. Rendered as-is (no text-colour tint),
                                // matching the GPU/glyphon behaviour for emoji.
                                let mut si = 0;
                                for di in 0..(iw * ih) as usize {
                                    let cr = image.data[si] as u32;
                                    let cg = image.data[si + 1] as u32;
                                    let cb = image.data[si + 2] as u32;
                                    let ca = image.data[si + 3] as u32;
                                    let pr = cr * ca / 255;
                                    let pg = cg * ca / 255;
                                    let pb = cb * ca / 255;
                                    dst[di] = (ca << 24) | (pb << 16) | (pg << 8) | pr;
                                    si += 4;
                                }
                            }
                            cosmic_text::SwashContent::SubpixelMask => {
                                for i in 0..(iw * ih) as usize {
                                    let sr = image.data[i * 3];
                                    let sg = image.data[i * 3 + 1];
                                    let sb = image.data[i * 3 + 2];
                                    let a = (sr as u32 + sg as u32 + sb as u32) / 3;
                                    let tr = color_rgb[0] as u32 * a / 255;
                                    let tg = color_rgb[1] as u32 * a / 255;
                                    let tb = color_rgb[2] as u32 * a / 255;
                                    dst[i] = (a << 24) | (tb << 16) | (tg << 8) | tr;
                                }
                            }
                        }
                        Some(glyph_cache::CachedGlyph {
                            pixmap: std::rc::Rc::new(gp),
                            left: image.placement.left,
                            top: image.placement.top,
                            color_rgb,
                        })
                    },
                );
                if let Some(cached) = cached {
                    let px = (gx + cached.left as f32) as i32;
                    let py = (gy - cached.top as f32) as i32;
                    min_x = min_x.min(px);
                    min_y = min_y.min(py);
                    max_x = max_x.max(px + cached.pixmap.width() as i32);
                    max_y = max_y.max(py + cached.pixmap.height() as i32);
                    placed.push((cached.pixmap.clone(), px, py));
                }
            }
        }
        if placed.is_empty() || max_x <= min_x || max_y <= min_y {
            return None;
        }

        // Pass 2: composite into the tight pixmap.
        let w = ((max_x - min_x) as u32).min(8192);
        let h = ((max_y - min_y) as u32).min(8192);
        let mut pixmap = tiny_skia::Pixmap::new(w, h)?;
        for (gp, px, py) in placed {
            pixmap.draw_pixmap(
                px - min_x,
                py - min_y,
                (*gp).as_ref(),
                &tiny_skia::PixmapPaint {
                    opacity: 1.0,
                    ..Default::default()
                },
                tiny_skia::Transform::identity(),
                None,
            );
        }
        Some((pixmap, min_x as f32, min_y as f32))
    }

    /// Copy damage rectangles to the softbuffer surface and present.
    pub fn present(&mut self, damage_rects: &[Rect]) {
        if self.surface_init_failed {
            return;
        }
        let Some(surface) = self.surface.as_mut() else {
            return;
        };
        let mut buffer = match surface.buffer_mut() {
            Ok(b) => b,
            Err(_e) => {
                #[cfg(feature = "tracing")]
                tracing::warn!("softbuffer buffer_mut failed: {_e:?}");
                return;
            }
        };

        let pw = self.physical_size.0;
        let ph = self.physical_size.1;
        let sf = self.scale_factor;
        let buf_len = buffer.len().min(self.pixels.len());

        for dr in damage_rects {
            let x0 = ((dr.x * sf).max(0.0) as u32).min(pw);
            let y0 = ((dr.y * sf).max(0.0) as u32).min(ph);
            let x1 = (((dr.x + dr.width) * sf).ceil().max(0.0) as u32).min(pw);
            let y1 = (((dr.y + dr.height) * sf).ceil().max(0.0) as u32).min(ph);
            for y in y0..y1 {
                let src_start = (y as usize * pw as usize + x0 as usize).min(buf_len);
                let len = ((x1 - x0) as usize).min(buf_len.saturating_sub(src_start));
                if len > 0 {
                    // Boundary conversion: canonical RGBA-mem → softbuffer 0RGB.
                    // (R/B swap per pixel; bandwidth-bound, same cost as copy.)
                    let src = &self.pixels[src_start..src_start + len];
                    let dst = &mut buffer[src_start..src_start + len];
                    for (d, &s) in dst.iter_mut().zip(src.iter()) {
                        *d = rgba_u32_to_softbuffer(s);
                    }
                }
            }
        }

        if let Err(_e) = buffer.present() {
            #[cfg(feature = "tracing")]
            tracing::warn!("softbuffer present failed: {_e:?}");
        }
    }

    pub fn end_frame(&mut self) {
        self.frame_count += 1;
        self.glyph_cache.trim(self.frame_count);
        self.surface_cache.end_frame();
        // NOTE: no periodic atlas clear — `GlyphAtlas::allocate` already
        // clears-and-retries when full; the old 600-frame forced clear only
        // produced a visible re-rasterisation hitch (audit P7).
    }

    pub fn resize(&mut self, logical_w: f32, logical_h: f32, sf: f32) {
        let sf_changed = (self.scale_factor - sf).abs() > 0.001;

        self.scale_factor = sf;
        self.logical_size = (logical_w, logical_h);

        let pw = (logical_w * sf).ceil() as u32;
        let ph = (logical_h * sf).ceil() as u32;

        if sf_changed {
            self.glyph_cache.clear_bitmaps();
        }

        if (pw, ph) != self.physical_size {
            self.physical_size = (pw, ph);
            self.pixels.resize(pw as usize * ph as usize, 0);
            // Masks are device-sized — all invalid after resize.
            self.clip_mask_cache.clear();
            if let Some(surface) = self.surface.as_mut() {
                let _ = surface.resize(pw, ph);
            }
        }
    }

    /// Raw frame-buffer pixels (canonical format — see `present` for the
    /// softbuffer boundary conversion). Headless tests read these directly.
    pub fn pixels(&self) -> &[u32] {
        &self.pixels
    }

    /// Physical (device) pixel dimensions of the frame buffer.
    pub fn physical_size(&self) -> (u32, u32) {
        self.physical_size
    }

    /// Frame buffer converted to softbuffer `0RGB` u32s — bit-for-bit what
    /// `present` writes to the window. Headless tests assert on this to lock
    /// the canonical-format → softbuffer boundary conversion.
    pub fn softbuffer_pixels(&self) -> Vec<u32> {
        self.pixels
            .iter()
            .map(|&p| rgba_u32_to_softbuffer(p))
            .collect()
    }

    fn clear_rect(&mut self, rect: Rect, argb: u32) {
        let x0 = (rect.x * self.scale_factor).max(0.0) as u32;
        let y0 = (rect.y * self.scale_factor).max(0.0) as u32;
        let x1 = ((rect.x + rect.width) * self.scale_factor)
            .min(self.physical_size.0 as f32)
            .ceil() as u32;
        let y1 = ((rect.y + rect.height) * self.scale_factor)
            .min(self.physical_size.1 as f32)
            .ceil() as u32;

        let stride = self.physical_size.0 as usize;
        for y in y0..y1 {
            let off = y as usize * stride + x0 as usize;
            let end = off + (x1 - x0) as usize;
            if end <= self.pixels.len() {
                self.pixels[off..end].fill(argb);
            }
        }
    }

    fn fill_rect_cpu(
        &mut self,
        rect: Rect,
        color: Color,
        radius: CornerRadii,
        xform: Affine2,
        mask: Option<&tiny_skia::Mask>,
        blend_mode: u8,
    ) {
        let sf = self.scale_factor;
        let mut pm = self.pixmap_mut();
        fill_rect_skia(&mut pm, rect, color, radius, xform, sf, mask, blend_mode);
    }

    fn pixmap_mut(&mut self) -> tiny_skia::PixmapMut<'_> {
        tiny_skia::PixmapMut::from_bytes(
            bytemuck::cast_slice_mut(&mut self.pixels),
            self.physical_size.0,
            self.physical_size.1,
        )
        .unwrap()
    }

    fn stroke_rect_cpu(
        &mut self,
        rect: Rect,
        color: Color,
        width: f32,
        radius: CornerRadii,
        xform: Affine2,
        mask: Option<&tiny_skia::Mask>,
        blend_mode: u8,
    ) {
        let sf = self.scale_factor;
        let mut pm = self.pixmap_mut();
        stroke_rect_skia(
            &mut pm, rect, color, width, radius, xform, sf, mask, blend_mode,
        );
    }

    /// Draw an image via a tiny-skia `Pattern` shader: bilinear-filtered,
    /// clipped by the shared clip-mask machinery (identical rounded-corner
    /// semantics to every other primitive) and correctly transformed
    /// (scroll/rotation/scale), aligning CPU with the GPU image path.
    ///
    /// `geom_clip`: optional raw-space rect the destination is intersected
    /// with (the cheap tier-2 clip from [`ResolvedClip::Rect`]).
    fn draw_image_cpu(
        &mut self,
        hash: u64,
        rect: Rect,
        geom_clip: Option<Rect>,
        opacity: f32,
        content_fit: crate::widgets::display::ContentFit,
        xform: Affine2,
        mask: Option<&tiny_skia::Mask>,
    ) {
        let sf = self.scale_factor;

        let (full_sw, full_sh) = match wgpu::lookup_image(hash) {
            Some((w, h, _)) => (w as f32, h as f32),
            None => return,
        };

        // ContentFit sees the intended dest size regardless of partial visibility.
        let render_rect = content_fit_rect(content_fit, rect, full_sw, full_sh);
        if render_rect.width <= 0.0 || render_rect.height <= 0.0 {
            return;
        }

        // Mip selection by device-space footprint (incl. transform scale).
        let tsx = xform.matrix2.x_axis.length().max(0.0001);
        let tsy = xform.matrix2.y_axis.length().max(0.0001);
        let dev_w = ((render_rect.width * sf * tsx).round() as u32).max(1);
        let dev_h = ((render_rect.height * sf * tsy).round() as u32).max(1);
        let image = match self.image_cache.ensure_mip(hash, dev_w, dev_h) {
            Some(img) => img,
            None => return,
        };

        // Cover/None overflow is clipped to the element rect; the ancestor
        // clip (incl. rounded corners) is applied via `mask` or `geom_clip`.
        let Some(mut dest) = render_rect.intersection(&rect) else {
            return;
        };
        if let Some(gc) = geom_clip {
            match dest.intersection(&gc) {
                Some(d) => dest = d,
                None => return,
            }
        }
        let skia_rect = logical_to_skia_rect_impl(dest, sf);
        let mut pb = tiny_skia::PathBuilder::new();
        pb.push_rect(skia_rect);
        let Some(path) = pb.finish() else {
            return;
        };

        // Pattern transform: image (mip) pixels → pre-scaled logical space.
        // The draw transform below is applied to BOTH path and shader by
        // tiny-skia, so this composes exactly like fill_rect_skia.
        let pat_ts = tiny_skia::Transform::from_row(
            render_rect.width * sf / image.width() as f32,
            0.0,
            0.0,
            render_rect.height * sf / image.height() as f32,
            render_rect.x * sf,
            render_rect.y * sf,
        );
        let paint = tiny_skia::Paint {
            shader: tiny_skia::Pattern::new(
                image.as_ref().as_ref(),
                tiny_skia::SpreadMode::Pad,
                tiny_skia::FilterQuality::Bilinear,
                opacity,
                pat_ts,
            ),
            anti_alias: false,
            ..Default::default()
        };
        let skia_xform = glam_to_skia_transform(xform, sf);
        let mut pm = tiny_skia::PixmapMut::from_bytes(
            bytemuck::cast_slice_mut(&mut self.pixels),
            self.physical_size.0,
            self.physical_size.1,
        )
        .unwrap();
        pm.fill_path(
            &path,
            &paint,
            tiny_skia::FillRule::Winding,
            skia_xform,
            mask,
        );
    }
}

impl TinySkiaRenderer {
    #[allow(clippy::too_many_arguments)]
    fn draw_gradient_cpu(
        &mut self,
        geom_rect: Rect,
        paint_rect: Rect,
        gradient: crate::style::LinearGradient,
        radius: CornerRadii,
        stroke_width: f32,
        xform: Affine2,
        mask: Option<&tiny_skia::Mask>,
    ) {
        let sf = self.scale_factor;
        let mut pm = self.pixmap_mut();
        draw_gradient_skia(
            &mut pm,
            geom_rect,
            paint_rect,
            gradient,
            radius,
            stroke_width,
            xform,
            sf,
            mask,
        );
    }
}

impl TinySkiaRenderer {
    fn fill_path_cpu(
        &mut self,
        path: &kurbo::BezPath,
        brush: &Brush,
        xform: glam::Affine2,
        mask: Option<&tiny_skia::Mask>,
    ) {
        let sf = self.scale_factor;
        let mut pm = self.pixmap_mut();
        fill_path_skia(&mut pm, path, brush, xform, sf, mask);
    }

    fn stroke_path_cpu(
        &mut self,
        path: &kurbo::BezPath,
        stroke: kurbo::Stroke,
        brush: &Brush,
        xform: glam::Affine2,
        mask: Option<&tiny_skia::Mask>,
    ) {
        let sf = self.scale_factor;
        let mut pm = self.pixmap_mut();
        stroke_path_skia(&mut pm, path, stroke, brush, xform, sf, mask);
    }
}

// ── Shared drawing primitives (free functions, usable by tests) ──────

/// Map the framework's blend-mode byte (see `style::BlendMode::to_u8`) to a
/// tiny-skia blend mode. The GPU effect pipeline implements the same table.
pub(crate) fn blend_from_u8(mode: u8) -> tiny_skia::BlendMode {
    match mode {
        1 => tiny_skia::BlendMode::Multiply,
        2 => tiny_skia::BlendMode::Screen,
        3 => tiny_skia::BlendMode::Overlay,
        _ => tiny_skia::BlendMode::SourceOver,
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn fill_rect_skia(
    pm: &mut tiny_skia::PixmapMut<'_>,
    rect: Rect,
    color: Color,
    radius: CornerRadii,
    xform: Affine2,
    sf: f32,
    mask: Option<&tiny_skia::Mask>,
    blend_mode: u8,
) {
    // Translation-only transforms are baked into the rect coordinates so
    // tiny-skia takes its identity-transform fast path (a non-identity
    // transform forces a path-transform pass: measured +20 µs on a
    // full-width row — and every scrolled element carries one).
    let identity_m = xform.matrix2 == glam::Mat2::IDENTITY;
    let (draw_rect, skia_xform) = if identity_m {
        (
            rect.offset(xform.translation.x, xform.translation.y),
            tiny_skia::Transform::identity(),
        )
    } else {
        (rect, glam_to_skia_transform(xform, sf))
    };
    let skia_rect = logical_to_skia_rect_impl(draw_rect, sf);
    if skia_rect.width() < 0.5 || skia_rect.height() < 0.5 {
        return;
    }
    // Manual span-fill fast path (round 2, O1): opaque SourceOver fills with
    // no matrix and no mask — the overwhelming majority (every background).
    // Beats tiny-skia's per-path overhead (~10-15 µs) at ~1-2 µs per rect;
    // rounded corners come from cached AA alpha templates.
    if color.a >= 1.0 && blend_mode == 0 && identity_m && mask.is_none() {
        fill_rounded_rect_manual(pm, skia_rect, color, radius, sf);
        return;
    }
    let paint = tiny_skia::Paint {
        shader: tiny_skia::Shader::SolidColor(to_skia_color(color)),
        anti_alias: !radius.is_zero(),
        blend_mode: blend_from_u8(blend_mode),
        ..Default::default()
    };
    if radius.is_zero() {
        let mut pb = tiny_skia::PathBuilder::new();
        pb.push_rect(skia_rect);
        let path = pb.finish().unwrap();
        pm.fill_path(
            &path,
            &paint,
            tiny_skia::FillRule::Winding,
            skia_xform,
            mask,
        );
        return;
    }
    // ── Manual rounded-rect fill (audit round 2, O1 — data-corrected) ──
    //
    // Measured: tiny-skia's cost for a rounded rect is dominated by per-path
    // fixed overhead (curve flattening + AA edge machinery, ~10-15 µs per
    // rect regardless of area — interior spans are already fast). A banded
    // decomposition through tiny-skia paths just pays that overhead twice.
    // Instead: manual row-span fills (hard straight edges, pixel-centre rule
    // — the same treatment radius-0 rects already get) + tiny cached AA
    // corner-alpha templates. ~2-3 µs per rect.
    let path = rounded_skia_rect(skia_rect, radius, sf);
    pm.fill_path(
        &path,
        &paint,
        tiny_skia::FillRule::Winding,
        skia_xform,
        mask,
    );
}

thread_local! {
    /// Corner coverage templates keyed by device-radius bits: `r×r` alpha of
    /// a top-left rounded corner (AA, rendered once by tiny-skia).
    static CORNER_TPL: std::cell::RefCell<HashMap<u32, Rc<Vec<u8>>>> =
        std::cell::RefCell::new(HashMap::new());
}

fn corner_alpha_template(r_dev: f32) -> (Rc<Vec<u8>>, usize) {
    let size = r_dev.ceil().max(1.0) as usize;
    let key = r_dev.to_bits();
    CORNER_TPL.with(|c| {
        let mut c = c.borrow_mut();
        if let Some(t) = c.get(&key) {
            return (t.clone(), size);
        }
        // Rasterise the TL corner wedge once with AA.
        let mut pmap = tiny_skia::Pixmap::new(size as u32, size as u32)
            .unwrap_or_else(|| tiny_skia::Pixmap::new(1, 1).unwrap());
        let mut pb = tiny_skia::PathBuilder::new();
        let (s, r) = (size as f32, r_dev);
        pb.move_to(r, 0.0);
        pb.quad_to(0.0, 0.0, 0.0, r);
        pb.line_to(0.0, s);
        pb.line_to(s, s);
        pb.line_to(s, 0.0);
        pb.close();
        if let Some(path) = pb.finish() {
            let paint = tiny_skia::Paint {
                shader: tiny_skia::Shader::SolidColor(tiny_skia::Color::WHITE),
                anti_alias: true,
                ..Default::default()
            };
            pmap.fill_path(
                &path,
                &paint,
                tiny_skia::FillRule::Winding,
                tiny_skia::Transform::identity(),
                None,
            );
        }
        let alpha: Rc<Vec<u8>> = Rc::new(pmap.data().iter().skip(3).step_by(4).copied().collect());
        if c.len() > 64 {
            c.clear();
        }
        c.insert(key, alpha.clone());
        (alpha, size)
    })
}

/// Manual opaque rounded-rect fill: integer row spans + cached AA corner
/// templates. Preconditions: opaque colour, SourceOver, no transform matrix,
/// no mask (checked by the caller).
fn fill_rounded_rect_manual(
    pm: &mut tiny_skia::PixmapMut<'_>,
    r: tiny_skia::Rect,
    color: Color,
    radius: CornerRadii,
    sf: f32,
) {
    let pw = pm.width() as usize;
    let ph = pm.height() as usize;
    let pixels: &mut [u32] = bytemuck::cast_slice_mut(pm.data_mut());
    let px_color = color_to_rgba_u32(color);

    let max_dim = r.width().min(r.height()) * 0.5;
    let tl = (radius.top_left * sf).clamp(0.0, max_dim);
    let tr = (radius.top_right * sf).clamp(0.0, max_dim);
    let br = (radius.bottom_right * sf).clamp(0.0, max_dim);
    let bl = (radius.bottom_left * sf).clamp(0.0, max_dim);

    let (x0, x1) = px_range(r.x(), r.x() + r.width(), pw);
    let (y0, y1) = px_range(r.y(), r.y() + r.height(), ph);
    if x1 <= x0 || y1 <= y0 {
        return;
    }

    // Corner blits (AA alpha templates over the fill colour).
    let (cr_, cg, cb) = (
        (px_color & 0xFF),
        (px_color >> 8) & 0xFF,
        (px_color >> 16) & 0xFF,
    );
    let mut blit_corner = |rad: f32, left: bool, top: bool| -> (usize, usize) {
        if rad <= 0.0 {
            return (0, 0);
        }
        let (tpl, size) = corner_alpha_template(rad);
        // Clamp the corner box inside the rect's pixel bounds.
        let bw = size.min(x1 - x0);
        let bh = size.min(y1 - y0);
        for j in 0..bh {
            let ty = if top { y0 + j } else { y1 - 1 - j };
            if ty >= ph {
                continue;
            }
            let row = ty * pw;
            for i in 0..bw {
                let tx = if left { x0 + i } else { x1 - 1 - i };
                if tx >= pw {
                    continue;
                }
                // Template is TL-oriented; mirror indices for other corners.
                let a = tpl[j * size + i] as u32;
                if a == 0 {
                    continue;
                }
                let di = row + tx;
                if a >= 255 {
                    pixels[di] = px_color;
                } else {
                    let d = pixels[di];
                    let inv = 255 - a;
                    let sr2 = cr_ * a / 255;
                    let sg2 = cg * a / 255;
                    let sb2 = cb * a / 255;
                    let sa2 = a; // opaque colour: premult alpha = coverage
                    let or_ = sr2 + (d & 0xFF) * inv / 255;
                    let og = sg2 + ((d >> 8) & 0xFF) * inv / 255;
                    let ob = sb2 + ((d >> 16) & 0xFF) * inv / 255;
                    let oa = sa2 + ((d >> 24) & 0xFF) * inv / 255;
                    pixels[di] = (oa << 24) | (ob << 16) | (og << 8) | or_;
                }
            }
        }
        (bw, bh)
    };
    let (tlw, tlh) = blit_corner(tl, true, true);
    let (trw, trh) = blit_corner(tr, false, true);
    let (brw, brh) = blit_corner(br, false, false);
    let (blw, blh) = blit_corner(bl, true, false);

    // Row spans around the corner boxes.
    for y in y0..y1 {
        let (lx, rx) = {
            let from_top = y - y0;
            let from_bot = y1 - 1 - y;
            let l = if from_top < tlh {
                x0 + tlw
            } else if from_bot < blh {
                x0 + blw
            } else {
                x0
            };
            let r_ = if from_top < trh {
                x1 - trw
            } else if from_bot < brh {
                x1 - brw
            } else {
                x1
            };
            (l, r_)
        };
        if rx > lx {
            pixels[y * pw + lx..y * pw + rx].fill(px_color);
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn stroke_rect_skia(
    pm: &mut tiny_skia::PixmapMut<'_>,
    rect: Rect,
    color: Color,
    width: f32,
    radius: CornerRadii,
    xform: Affine2,
    sf: f32,
    mask: Option<&tiny_skia::Mask>,
    blend_mode: u8,
) {
    let skia_rect = logical_to_skia_rect_impl(rect, sf);
    if skia_rect.width() < 0.5 || skia_rect.height() < 0.5 {
        return;
    }
    let skia_xform = glam_to_skia_transform(xform, sf);
    let paint = tiny_skia::Paint {
        shader: tiny_skia::Shader::SolidColor(to_skia_color(color)),
        anti_alias: !radius.is_zero(),
        blend_mode: blend_from_u8(blend_mode),
        ..Default::default()
    };
    let stroke = tiny_skia::Stroke {
        width: width * sf,
        ..Default::default()
    };
    if radius.is_zero() {
        let mut pb = tiny_skia::PathBuilder::new();
        pb.push_rect(skia_rect);
        let path = pb.finish().unwrap();
        pm.stroke_path(&path, &paint, &stroke, skia_xform, mask);
    } else {
        let path = rounded_skia_rect(skia_rect, radius, sf);
        pm.stroke_path(&path, &paint, &stroke, skia_xform, mask);
    }
}

/// Gradient fill. `geom_rect` defines the gradient geometry (start/end/// mapping); `paint_rect` is the rect actually filled — they differ when the
/// tier-2 geometric clip shrank the visible area (the gradient mapping must
/// not shift when clipped).
///
/// The element transform is applied to BOTH path and shader (audit C2: the
/// pre-audit CPU path ignored it entirely, so gradients inside scroll
/// containers stayed frozen at their unscrolled position; the GPU applies it
/// in `gradient_to_vertices`).
#[allow(clippy::too_many_arguments)]
pub(crate) fn draw_gradient_skia(
    pm: &mut tiny_skia::PixmapMut<'_>,
    geom_rect: Rect,
    paint_rect: Rect,
    gradient: crate::style::LinearGradient,
    radius: CornerRadii,
    stroke_width: f32,
    xform: Affine2,
    sf: f32,
    mask: Option<&tiny_skia::Mask>,
) {
    let n = gradient.stop_count.min(4) as usize;
    if n < 2 {
        return;
    }
    let skia_rect = logical_to_skia_rect_impl(paint_rect, sf);
    if skia_rect.width() < 0.5 || skia_rect.height() < 0.5 {
        return;
    }

    let rect = geom_rect;
    let (x1, y1) = (
        gradient.start.0 * rect.width + rect.x,
        gradient.start.1 * rect.height + rect.y,
    );
    let (x2, y2) = (
        gradient.end.0 * rect.width + rect.x,
        gradient.end.1 * rect.height + rect.y,
    );

    // ── Axis-aligned linear gradient fast path (audit round 2, O3) ──
    // tiny-skia evaluates the gradient shader per pixel (~8 ns/px, measured
    // 0.46 ms per card-sized gradient @1080p). For axis-aligned linear
    // gradients the colour is constant along one axis: fill row spans
    // (vertical) or replicate one precomputed row (horizontal) directly.
    if matches!(gradient.kind, crate::style::GradientKind::Linear)
        && radius.is_zero()
        && stroke_width == 0.0
        && mask.is_none()
        && xform.matrix2 == glam::Mat2::IDENTITY
    {
        let dx = (gradient.start.0 - gradient.end.0).abs();
        let dy = (gradient.start.1 - gradient.end.1).abs();
        let vertical = dx < 1e-6 && dy > 1e-6;
        let horizontal = dy < 1e-6 && dx > 1e-6;
        if vertical || horizontal {
            draw_axis_gradient(
                pm,
                paint_rect,
                xform.translation,
                sf,
                vertical,
                if vertical { (y1, y2) } else { (x1, x2) },
                &gradient.stops[..n],
            );
            return;
        }
    }

    // ── General manual gradient fill (round 3) ──
    // Handles: diagonal/rounded/masked linear, true conic.
    // Radial, stroked and rotated gradients fall through to tiny-skia.
    if stroke_width == 0.0
        && xform.matrix2 == glam::Mat2::IDENTITY
        && (matches!(
            gradient.kind,
            crate::style::GradientKind::Linear | crate::style::GradientKind::Conic
        ))
    {
        manual_gradient_fill(
            pm,
            paint_rect,
            geom_rect,
            &gradient,
            radius,
            xform.translation,
            sf,
            mask,
        );
        return;
    }

    // ── tiny-skia fallback: radial, stroked, rotated transforms ──
    let stops: Vec<tiny_skia::GradientStop> = gradient.stops[..n]
        .iter()
        .map(|s| tiny_skia::GradientStop::new(s.offset, to_skia_color(s.color)))
        .collect();

    let shader = match gradient.kind {
        crate::style::GradientKind::Linear => tiny_skia::LinearGradient::new(
            tiny_skia::Point::from_xy(x1 * sf, y1 * sf),
            tiny_skia::Point::from_xy(x2 * sf, y2 * sf),
            stops,
            tiny_skia::SpreadMode::Pad,
            tiny_skia::Transform::identity(),
        ),
        crate::style::GradientKind::Radial | crate::style::GradientKind::Conic => {
            let radius = (((x2 - x1) * (x2 - x1) + (y2 - y1) * (y2 - y1)).sqrt() * sf).max(0.5);
            let center = tiny_skia::Point::from_xy(x1 * sf, y1 * sf);
            tiny_skia::RadialGradient::new(
                center,
                0.0,
                center,
                radius,
                stops,
                tiny_skia::SpreadMode::Pad,
                tiny_skia::Transform::identity(),
            )
        }
    };
    let Some(shader) = shader else {
        return;
    };

    // tiny-skia applies the draw transform to the shader as well, so the
    // pre-scaled gradient points track the (scrolled/transformed) content.
    let skia_xform = glam_to_skia_transform(xform, sf);
    let paint = tiny_skia::Paint {
        shader,
        anti_alias: false,
        ..Default::default()
    };
    let path = if radius.is_zero() {
        let mut pb = tiny_skia::PathBuilder::new();
        pb.push_rect(skia_rect);
        pb.finish().unwrap()
    } else {
        rounded_skia_rect(skia_rect, radius, sf)
    };

    if stroke_width > 0.0 {
        let mut sp = paint.clone();
        sp.anti_alias = true;
        let stroke = tiny_skia::Stroke {
            width: stroke_width * sf,
            ..Default::default()
        };
        pm.stroke_path(&path, &sp, &stroke, skia_xform, mask);
    } else {
        pm.fill_path(
            &path,
            &paint,
            tiny_skia::FillRule::Winding,
            skia_xform,
            mask,
        );
    }
}

/// Precompute a 256-entry premultiplied canonical-RGBA LUT from gradient
/// stops (Pad mode). The manual fill paths sample this once per pixel
/// instead of re-running the stop interpolation.
fn make_gradient_lut(stops: &[crate::style::GradientStop]) -> [(u32, u32); 256] {
    let mut lut = [(0u32, 0u32); 256];
    for i in 0..256 {
        lut[i] = gradient_px_at(stops, i as f32 / 255.0);
    }
    lut
}

/// Source-over blend a full pixel row with a constant premultiplied source.
fn blend_row_constant(row: &mut [u32], px: u32, a: u32) {
    if a == 0 {
        return;
    }
    if a >= 255 {
        row.fill(px);
        return;
    }
    let (sr, sg, sb) = (px & 0xFF, (px >> 8) & 0xFF, (px >> 16) & 0xFF);
    let inv = 255 - a;
    for d in row.iter_mut() {
        let v = *d;
        *d = (a + ((v >> 24) & 0xFF) * inv / 255) << 24
            | (sb + ((v >> 16) & 0xFF) * inv / 255) << 16
            | (sg + ((v >> 8) & 0xFF) * inv / 255) << 8
            | (sr + (v & 0xFF) * inv / 255);
    }
}

/// Source-over blend one canonical-RGBA pixel.
#[inline(always)]
fn blend_pixel(dst: &mut u32, px: u32, a: u32) {
    if a == 0 {
        return;
    }
    if a >= 255 {
        *dst = px;
        return;
    }
    let (sr, sg, sb) = (px & 0xFF, (px >> 8) & 0xFF, (px >> 16) & 0xFF);
    let v = *dst;
    let inv = 255 - a;
    *dst = (a + ((v >> 24) & 0xFF) * inv / 255) << 24
        | (sb + ((v >> 16) & 0xFF) * inv / 255) << 16
        | (sg + ((v >> 8) & 0xFF) * inv / 255) << 8
        | (sr + (v & 0xFF) * inv / 255);
}

/// Rounded-rect corner coverage in device px. Returns 0 (inside corner arc,
/// fully clipped), 1 (inside rect), or a fractional value near the arc edge.
fn corner_cov(
    px: usize,
    py: usize,
    x0: usize,
    y0: usize,
    x1: usize,
    y1: usize,
    tl: f32,
    tr: f32,
    br: f32,
    bl: f32,
) -> f32 {
    let dx_tl = px as f32 + 0.5 - (x0 as f32 + tl);
    let dy_tl = py as f32 + 0.5 - (y0 as f32 + tl);
    if dx_tl < 0.0 && dy_tl < 0.0 && tl > 0.0 {
        return (dx_tl * dx_tl + dy_tl * dy_tl <= tl * tl) as u32 as f32;
    }
    let dx_tr = (x1 as f32 - tr) - (px as f32 + 0.5);
    if dx_tr < 0.0 && py < y0 + tr as usize && tr > 0.0 {
        let dy_tr = (y0 as f32 + tr) - (py as f32 + 0.5);
        return (dx_tr * dx_tr + dy_tr * dy_tr <= tr * tr) as u32 as f32;
    }
    let dx_br = (x1 as f32 - br) - (px as f32 + 0.5);
    let dy_br = (y1 as f32 - br) - (py as f32 + 0.5);
    if dx_br < 0.0 && dy_br < 0.0 && br > 0.0 {
        return (dx_br * dx_br + dy_br * dy_br <= br * br) as u32 as f32;
    }
    let dx_bl = px as f32 + 0.5 - (x0 as f32 + bl);
    let dy_bl = (y1 as f32 - bl) - (py as f32 + 0.5);
    if dx_bl < 0.0 && dy_bl < 0.0 && bl > 0.0 {
        return (dx_bl * dx_bl + dy_bl * dy_bl <= bl * bl) as u32 as f32;
    }
    1.0
}

/// General manual gradient fill: per-pixel LUT evaluation with optional
/// rounded-corner coverage and mask. Handles linear (dot-product stepping),
/// conic (angle sweep — GPU parity), and axis-aligned (span-optimised).
/// Radial and stroked gradients fall back to tiny-skia below.
#[allow(clippy::too_many_arguments)]
fn manual_gradient_fill(
    pm: &mut tiny_skia::PixmapMut<'_>,
    paint_rect: Rect,
    geom_rect: Rect,
    gradient: &crate::style::LinearGradient,
    radius: CornerRadii,
    translation: glam::Vec2,
    sf: f32,
    mask: Option<&tiny_skia::Mask>,
) {
    let pw = pm.width() as usize;
    let ph = pm.height() as usize;
    let pixels: &mut [u32] = bytemuck::cast_slice_mut(pm.data_mut());
    let dev = Rect::new(
        (paint_rect.x + translation.x) * sf,
        (paint_rect.y + translation.y) * sf,
        paint_rect.width * sf,
        paint_rect.height * sf,
    );
    let (x0, x1) = px_range(dev.x, dev.x + dev.width, pw);
    let (y0, y1) = px_range(dev.y, dev.y + dev.height, ph);
    if x1 <= x0 || y1 <= y0 {
        return;
    }
    let mdata = mask.map(|m| m.data());
    let stops = &gradient.stops[..gradient.stop_count.min(4) as usize];
    let lut = make_gradient_lut(stops);

    let max_dim = dev.width.min(dev.height) * 0.5;
    let tl = (radius.top_left * sf).clamp(0.0, max_dim);
    let tr = (radius.top_right * sf).clamp(0.0, max_dim);
    let br = (radius.bottom_right * sf).clamp(0.0, max_dim);
    let bl = (radius.bottom_left * sf).clamp(0.0, max_dim);
    let has_radius = !radius.is_zero();

    let rect = geom_rect;
    let (sx, sy) = (
        gradient.start.0 * rect.width + rect.x + translation.x,
        gradient.start.1 * rect.height + rect.y + translation.y,
    );
    let (ex, ey) = (
        gradient.end.0 * rect.width + rect.x + translation.x,
        gradient.end.1 * rect.height + rect.y + translation.y,
    );

    match gradient.kind {
        crate::style::GradientKind::Linear => {
            let ddx = (ex - sx) * sf;
            let ddy = (ey - sy) * sf;
            let dot_len = ddx * ddx + ddy * ddy;
            if dot_len < 1e-6 {
                return;
            }
            let inv_dot = 1.0 / dot_len;
            let axis_vertical = ddx.abs() < 1e-6 && ddy.abs() > 1e-6;
            let axis_horizontal = ddy.abs() < 1e-6 && ddx.abs() > 1e-6;
            if !has_radius && mask.is_none() && (axis_vertical || axis_horizontal) {
                if axis_vertical {
                    let a0 = sy * sf;
                    let a1 = ey * sf;
                    let inv_span = 1.0 / (a1 - a0).max(1e-6).copysign(a1 - a0);
                    for y in y0..y1 {
                        let t = ((y as f32 + 0.5) - a0) * inv_span;
                        let (px, a) = lut[(t.clamp(0.0, 1.0) * 255.0) as usize];
                        let row = &mut pixels[y * pw + x0..y * pw + x1];
                        if a >= 255 {
                            row.fill(px);
                        } else if a > 0 {
                            blend_row_constant(row, px, a);
                        }
                    }
                    return;
                }
                if axis_horizontal {
                    let a0 = sx * sf;
                    let a1 = ex * sf;
                    let inv_span = 1.0 / (a1 - a0).max(1e-6).copysign(a1 - a0);
                    let strip: Vec<(u32, u32)> = (x0..x1)
                        .map(|x| lut[(((x as f32 + 0.5) - a0) * inv_span).clamp(0.0, 1.0) as usize])
                        .collect();
                    for y in y0..y1 {
                        let row = &mut pixels[y * pw + x0..y * pw + x1];
                        for (d, &(px, a)) in row.iter_mut().zip(strip.iter()) {
                            if a == 0 {
                                continue;
                            }
                            if a >= 255 {
                                *d = px;
                                continue;
                            }
                            blend_pixel(d, px, a);
                        }
                    }
                    return;
                }
            }
            // Diagonal / rounded / masked: per-row t0 + step.
            let dtx = ddx * inv_dot;
            let dty = ddy * inv_dot;
            let cx = sx * sf;
            let cy = sy * sf;
            for y in y0..y1 {
                let ry = y as f32 + 0.5 - cy;
                let mut t = (x0 as f32 + 0.5 - cx) * dtx + ry * dty;
                let row = &mut pixels[y * pw + x0..y * pw + x1];
                for x in 0..(x1 - x0) {
                    let ti = t.clamp(0.0, 1.0);
                    t += dtx;
                    let (px, a) = lut[(ti * 255.0) as usize];
                    if a == 0 {
                        continue;
                    }
                    let cov = if has_radius {
                        corner_cov(x0 + x, y, x0, y0, x1, y1, tl, tr, br, bl)
                    } else {
                        1.0
                    };
                    if cov <= 0.0 {
                        continue;
                    }
                    let eff_a = (a as f32 * cov) as u32;
                    if mdata.is_some() {
                        let mv = mdata.unwrap()[y * pw + x0 + x] as f32 / 255.0;
                        if mv <= 0.0 {
                            continue;
                        }
                        blend_pixel(&mut row[x], px, (eff_a as f32 * mv) as u32);
                    } else {
                        blend_pixel(&mut row[x], px, eff_a);
                    }
                }
            }
        }
        crate::style::GradientKind::Conic => {
            let cx = sx * sf;
            let cy = sy * sf;
            let ref_ang = (ey * sf - cy).atan2(ex * sf - cx);
            let two_pi: f32 = 6.283_185_5;
            for y in y0..y1 {
                let row = &mut pixels[y * pw + x0..y * pw + x1];
                for x in 0..(x1 - x0) {
                    let px = x0 + x;
                    let mut angle = (y as f32 + 0.5 - cy).atan2(px as f32 + 0.5 - cx) - ref_ang;
                    if angle < 0.0 {
                        angle += two_pi;
                    }
                    let t = angle / two_pi;
                    let (lut_px, a) = lut[(t.clamp(0.0, 1.0) * 255.0) as usize];
                    if a == 0 {
                        continue;
                    }
                    let cov = if has_radius {
                        corner_cov(px, y, x0, y0, x1, y1, tl, tr, br, bl)
                    } else {
                        1.0
                    };
                    if cov <= 0.0 {
                        continue;
                    }
                    let eff_a = (a as f32 * cov) as u32;
                    if mdata.is_some() {
                        let mv = mdata.unwrap()[y * pw + px] as f32 / 255.0;
                        if mv <= 0.0 {
                            continue;
                        }
                        blend_pixel(&mut row[x], lut_px, (eff_a as f32 * mv) as u32);
                    } else {
                        blend_pixel(&mut row[x], lut_px, eff_a);
                    }
                }
            }
        }
        _ => { /* Radial falls back to tiny-skia */ }
    }
}

/// Interpolate a gradient colour at parameter `t` (Pad spread), returning a
/// premultiplied canonical-RGBA pixel + its alpha byte.
fn gradient_px_at(stops: &[crate::style::GradientStop], t: f32) -> (u32, u32) {
    let t = t.clamp(0.0, 1.0);
    let mut c = stops[0].color;
    if t >= stops[stops.len() - 1].offset {
        c = stops[stops.len() - 1].color;
    } else if t > stops[0].offset {
        for w in stops.windows(2) {
            if t <= w[1].offset {
                let span = (w[1].offset - w[0].offset).max(1e-6);
                let k = (t - w[0].offset) / span;
                c = Color {
                    r: w[0].color.r + (w[1].color.r - w[0].color.r) * k,
                    g: w[0].color.g + (w[1].color.g - w[0].color.g) * k,
                    b: w[0].color.b + (w[1].color.b - w[0].color.b) * k,
                    a: w[0].color.a + (w[1].color.a - w[0].color.a) * k,
                };
                break;
            }
        }
    }
    (color_to_rgba_u32(c), (c.a * 255.0 + 0.5) as u32)
}

/// Non-AA pixel range for a device-space interval (pixel-centre rule,
/// matching tiny-skia's unantialiased fills).
fn px_range(v0: f32, v1: f32, limit: usize) -> (usize, usize) {
    let a = (v0 - 0.5).ceil().max(0.0) as usize;
    let b = ((v1 - 0.5).ceil().max(0.0) as usize).min(limit);
    (a.min(limit), b)
}

/// Axis-aligned linear-gradient fill: rows of constant colour (vertical) or
/// one precomputed row replicated (horizontal). `axis` is the gradient's
/// (start, end) coordinate along the varying axis in raw-space logical px.
fn draw_axis_gradient(
    pm: &mut tiny_skia::PixmapMut<'_>,
    paint_rect: Rect,
    translation: glam::Vec2,
    sf: f32,
    vertical: bool,
    axis: (f32, f32),
    stops: &[crate::style::GradientStop],
) {
    let pw = pm.width() as usize;
    let ph = pm.height() as usize;
    let pixels: &mut [u32] = bytemuck::cast_slice_mut(pm.data_mut());
    let dev = Rect::new(
        (paint_rect.x + translation.x) * sf,
        (paint_rect.y + translation.y) * sf,
        paint_rect.width * sf,
        paint_rect.height * sf,
    );
    let (x0, x1) = px_range(dev.x, dev.x + dev.width, pw);
    let (y0, y1) = px_range(dev.y, dev.y + dev.height, ph);
    if x1 <= x0 || y1 <= y0 {
        return;
    }
    // Axis start/end in device px (translation applies to the geometry too).
    let tr = if vertical {
        translation.y
    } else {
        translation.x
    };
    let a0 = (axis.0 + tr) * sf;
    let a1 = (axis.1 + tr) * sf;
    let inv_span = 1.0 / (a1 - a0).max(1e-6).copysign(a1 - a0);

    if vertical {
        for y in y0..y1 {
            let t = ((y as f32 + 0.5) - a0) * inv_span;
            let (px, a) = gradient_px_at(stops, t);
            let row = &mut pixels[y * pw + x0..y * pw + x1];
            if a >= 255 {
                row.fill(px);
            } else if a > 0 {
                let (sr, sg, sb) = (px & 0xFF, (px >> 8) & 0xFF, (px >> 16) & 0xFF);
                let inv = 255 - a;
                for d in row.iter_mut() {
                    let v = *d;
                    let or = sr + (v & 0xFF) * inv / 255;
                    let og = sg + ((v >> 8) & 0xFF) * inv / 255;
                    let ob = sb + ((v >> 16) & 0xFF) * inv / 255;
                    let oa = a + ((v >> 24) & 0xFF) * inv / 255;
                    *d = (oa << 24) | (ob << 16) | (og << 8) | or;
                }
            }
        }
    } else {
        // Precompute one row, then replicate.
        let strip: Vec<(u32, u32)> = (x0..x1)
            .map(|x| {
                let t = ((x as f32 + 0.5) - a0) * inv_span;
                gradient_px_at(stops, t)
            })
            .collect();
        let opaque = strip.iter().all(|&(_, a)| a >= 255);
        if opaque {
            let row_px: Vec<u32> = strip.iter().map(|&(p, _)| p).collect();
            for y in y0..y1 {
                pixels[y * pw + x0..y * pw + x1].copy_from_slice(&row_px);
            }
        } else {
            for y in y0..y1 {
                let row = &mut pixels[y * pw + x0..y * pw + x1];
                for (d, &(px, a)) in row.iter_mut().zip(strip.iter()) {
                    if a == 0 {
                        continue;
                    }
                    let (sr, sg, sb) = (px & 0xFF, (px >> 8) & 0xFF, (px >> 16) & 0xFF);
                    let inv = 255 - a;
                    let v = *d;
                    let or = sr + (v & 0xFF) * inv / 255;
                    let og = sg + ((v >> 8) & 0xFF) * inv / 255;
                    let ob = sb + ((v >> 16) & 0xFF) * inv / 255;
                    let oa = a + ((v >> 24) & 0xFF) * inv / 255;
                    *d = (oa << 24) | (ob << 16) | (og << 8) | or;
                }
            }
        }
    }
}

pub(crate) fn fill_path_skia(
    pm: &mut tiny_skia::PixmapMut<'_>,
    path: &kurbo::BezPath,
    brush: &Brush,
    xform: glam::Affine2,
    sf: f32,
    mask: Option<&tiny_skia::Mask>,
) {
    let Some(skia_path) = bezpath_to_tiny_skia(path) else {
        return;
    };
    let skia_xform = glam_to_skia_transform_logical(xform, sf);
    let paint = match brush {
        Brush::Solid(color) => tiny_skia::Paint {
            shader: tiny_skia::Shader::SolidColor(to_skia_color(*color)),
            anti_alias: true,
            ..Default::default()
        },
        Brush::Gradient(grad) => {
            let bbox = match bezpath_bounds(path) {
                Some(b) => b,
                None => return,
            };
            let n = grad.stop_count.min(4) as usize;
            let stops: Vec<tiny_skia::GradientStop> = grad.stops[..n]
                .iter()
                .map(|s| tiny_skia::GradientStop::new(s.offset, to_skia_color(s.color)))
                .collect();
            let (x1, y1) = (
                (bbox.x + grad.start.0 * bbox.width) * sf,
                (bbox.y + grad.start.1 * bbox.height) * sf,
            );
            let (x2, y2) = (
                (bbox.x + grad.end.0 * bbox.width) * sf,
                (bbox.y + grad.end.1 * bbox.height) * sf,
            );
            tiny_skia::Paint {
                shader: tiny_skia::LinearGradient::new(
                    tiny_skia::Point::from_xy(x1, y1),
                    tiny_skia::Point::from_xy(x2, y2),
                    stops,
                    tiny_skia::SpreadMode::Pad,
                    tiny_skia::Transform::identity(),
                )
                .unwrap_or(tiny_skia::Shader::SolidColor(tiny_skia::Color::BLACK)),
                anti_alias: true,
                ..Default::default()
            }
        }
    };
    pm.fill_path(
        &skia_path,
        &paint,
        tiny_skia::FillRule::Winding,
        skia_xform,
        mask,
    );
}

pub(crate) fn stroke_path_skia(
    pm: &mut tiny_skia::PixmapMut<'_>,
    path: &kurbo::BezPath,
    stroke: kurbo::Stroke,
    brush: &Brush,
    xform: glam::Affine2,
    sf: f32,
    mask: Option<&tiny_skia::Mask>,
) {
    let Some(skia_path) = bezpath_to_tiny_skia(path) else {
        return;
    };
    let skia_stroke = tiny_skia::Stroke {
        width: (stroke.width as f32 * sf).max(0.5),
        line_cap: match stroke.start_cap {
            kurbo::Cap::Butt => tiny_skia::LineCap::Butt,
            kurbo::Cap::Round => tiny_skia::LineCap::Round,
            kurbo::Cap::Square => tiny_skia::LineCap::Square,
        },
        line_join: match stroke.join {
            kurbo::Join::Bevel => tiny_skia::LineJoin::Bevel,
            kurbo::Join::Round => tiny_skia::LineJoin::Round,
            kurbo::Join::Miter => tiny_skia::LineJoin::Miter,
        },
        ..Default::default()
    };
    let paint = match brush {
        Brush::Solid(color) => tiny_skia::Paint {
            shader: tiny_skia::Shader::SolidColor(to_skia_color(*color)),
            anti_alias: true,
            ..Default::default()
        },
        Brush::Gradient(grad) => {
            let bbox = match bezpath_bounds(path) {
                Some(b) => b,
                None => return,
            };
            let n = grad.stop_count.min(4) as usize;
            let stops: Vec<tiny_skia::GradientStop> = grad.stops[..n]
                .iter()
                .map(|s| tiny_skia::GradientStop::new(s.offset, to_skia_color(s.color)))
                .collect();
            let (x1, y1) = (
                (bbox.x + grad.start.0 * bbox.width) * sf,
                (bbox.y + grad.start.1 * bbox.height) * sf,
            );
            let (x2, y2) = (
                (bbox.x + grad.end.0 * bbox.width) * sf,
                (bbox.y + grad.end.1 * bbox.height) * sf,
            );
            tiny_skia::Paint {
                shader: tiny_skia::LinearGradient::new(
                    tiny_skia::Point::from_xy(x1, y1),
                    tiny_skia::Point::from_xy(x2, y2),
                    stops,
                    tiny_skia::SpreadMode::Pad,
                    tiny_skia::Transform::identity(),
                )
                .unwrap_or(tiny_skia::Shader::SolidColor(tiny_skia::Color::BLACK)),
                anti_alias: true,
                ..Default::default()
            }
        }
    };
    let skia_xform = glam_to_skia_transform_logical(xform, sf);
    pm.stroke_path(&skia_path, &paint, &skia_stroke, skia_xform, mask);
}

fn content_fit_rect(
    fit: crate::widgets::display::ContentFit,
    dest: crate::style::Rect,
    img_w: f32,
    img_h: f32,
) -> crate::style::Rect {
    if img_w <= 0.0 || img_h <= 0.0 {
        return dest;
    }
    match fit {
        crate::widgets::display::ContentFit::Fill => dest,
        crate::widgets::display::ContentFit::Contain => {
            let scale = (dest.width / img_w).min(dest.height / img_h);
            let rw = img_w * scale;
            let rh = img_h * scale;
            crate::style::Rect::new(
                dest.x + (dest.width - rw) * 0.5,
                dest.y + (dest.height - rh) * 0.5,
                rw,
                rh,
            )
        }
        crate::widgets::display::ContentFit::Cover => {
            let scale = (dest.width / img_w).max(dest.height / img_h);
            let rw = img_w * scale;
            let rh = img_h * scale;
            crate::style::Rect::new(
                dest.x + (dest.width - rw) * 0.5,
                dest.y + (dest.height - rh) * 0.5,
                rw,
                rh,
            )
        }
        crate::widgets::display::ContentFit::None => crate::style::Rect::new(
            dest.x,
            dest.y,
            img_w.min(dest.width),
            img_h.min(dest.height),
        ),
        crate::widgets::display::ContentFit::ScaleDown => {
            let scale = (dest.width / img_w).min(dest.height / img_h).min(1.0);
            let rw = img_w * scale;
            let rh = img_h * scale;
            crate::style::Rect::new(
                dest.x + (dest.width - rw) * 0.5,
                dest.y + (dest.height - rh) * 0.5,
                rw,
                rh,
            )
        }
    }
}

/// Convert a [`Color`] to the canonical frame-buffer format: tiny-skia
/// premultiplied RGBA bytes read as a little-endian `u32`
/// (`A<<24 | B<<16 | G<<8 | R`). See the module docs — every writer of
/// `TinySkiaRenderer::pixels` MUST use this layout; `present()` converts
/// to softbuffer's `0RGB` at the boundary.
pub(crate) fn color_to_rgba_u32(c: Color) -> u32 {
    let a = (c.a * 255.0 + 0.5) as u32;
    // Premultiply to match tiny-skia's premultiplied pixmap contents.
    let r = (c.r * c.a * 255.0 + 0.5) as u32;
    let g = (c.g * c.a * 255.0 + 0.5) as u32;
    let b = (c.b * c.a * 255.0 + 0.5) as u32;
    (a << 24) | (b << 16) | (g << 8) | r
}

/// Convert one canonical (RGBA-mem) pixel to softbuffer `0RGB`
/// (`R` in bits 16-23, `G` in 8-15, `B` in 0-7). Alpha is dropped —
/// softbuffer ignores the top byte.
#[inline(always)]
pub(crate) fn rgba_u32_to_softbuffer(px: u32) -> u32 {
    (px & 0xFF00_FF00) | ((px & 0x0000_00FF) << 16) | ((px >> 16) & 0x0000_00FF)
}

pub(crate) fn to_skia_color(c: Color) -> tiny_skia::Color {
    tiny_skia::Color::from_rgba(c.r, c.g, c.b, c.a).unwrap()
}

pub(crate) fn rounded_skia_rect(
    rect: tiny_skia::Rect,
    radii: CornerRadii,
    sf: f32,
) -> tiny_skia::Path {
    let max_dim = rect.width().min(rect.height()) * 0.5;
    let tl = (radii.top_left * sf).min(max_dim);
    let tr = (radii.top_right * sf).min(max_dim);
    let br = (radii.bottom_right * sf).min(max_dim);
    let bl = (radii.bottom_left * sf).min(max_dim);
    let mut pb = tiny_skia::PathBuilder::new();
    if tl <= 0.0 && tr <= 0.0 && br <= 0.0 && bl <= 0.0 {
        pb.push_rect(rect);
    } else {
        let x = rect.x();
        let y = rect.y();
        let w = rect.width();
        let h = rect.height();
        pb.move_to(x + tl, y);
        pb.line_to(x + w - tr, y);
        pb.quad_to(x + w, y, x + w, y + tr);
        pb.line_to(x + w, y + h - br);
        pb.quad_to(x + w, y + h, x + w - br, y + h);
        pb.line_to(x + bl, y + h);
        pb.quad_to(x, y + h, x, y + h - bl);
        pb.line_to(x, y + tl);
        pb.quad_to(x, y, x + tl, y);
        pb.close();
    }
    pb.finish().unwrap()
}

pub(crate) fn logical_to_skia_rect_impl(rect: Rect, sf: f32) -> tiny_skia::Rect {
    let x = rect.x * sf;
    let y = rect.y * sf;
    let w = (rect.width * sf).max(0.5);
    let h = (rect.height * sf).max(0.5);
    tiny_skia::Rect::from_xywh(x, y, w, h)
        .unwrap_or(tiny_skia::Rect::from_xywh(0.0, 0.0, 1.0, 1.0).unwrap())
}

fn point_in_rect(x: i32, y: i32, r: tiny_skia::Rect) -> bool {
    x >= r.x() as i32
        && x <= (r.x() + r.width()) as i32
        && y >= r.y() as i32
        && y <= (r.y() + r.height()) as i32
}

/// Source-over blit of a premultiplied canonical-RGBA text surface with a
/// global opacity multiplier and optional device clip rect. Replaces the
/// pre-audit `draw_pixmap + full-frame Mask` path for cached text surfaces.
fn blit_text_surface(
    pixels: &mut [u32],
    pw: usize,
    ph: usize,
    src: &tiny_skia::Pixmap,
    dx: i32,
    dy: i32,
    opacity: f32,
    clip_dev: Option<(i32, i32, i32, i32)>,
) {
    let op = (opacity.clamp(0.0, 1.0) * 255.0 + 0.5) as u32;
    if op == 0 {
        return;
    }
    let sw = src.width() as i32;
    let sh = src.height() as i32;
    let sdata = bytemuck::cast_slice::<u8, u32>(src.data());
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
            let mut sr = s & 0xFF;
            let mut sg = (s >> 8) & 0xFF;
            let mut sb = (s >> 16) & 0xFF;
            if op < 255 {
                sa = sa * op / 255;
                sr = sr * op / 255;
                sg = sg * op / 255;
                sb = sb * op / 255;
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

pub(crate) fn glam_to_skia_transform(xform: glam::Affine2, sf: f32) -> tiny_skia::Transform {
    let m = xform.matrix2;
    let t = xform.translation;
    tiny_skia::Transform {
        sx: m.x_axis.x,
        ky: m.x_axis.y,
        kx: m.y_axis.x,
        sy: m.y_axis.y,
        tx: t.x * sf,
        ty: t.y * sf,
    }
}

/// Same as `glam_to_skia_transform` but also scales matrix components by `sf`.
/// Used for paths whose coordinates are in logical pixels (after viewBox→element transform).
/// Rectangles are pre-scaled to physical pixels via `logical_to_skia_rect_impl` so they use
/// `glam_to_skia_transform` directly.
pub(crate) fn glam_to_skia_transform_logical(
    xform: glam::Affine2,
    sf: f32,
) -> tiny_skia::Transform {
    let m = xform.matrix2;
    let t = xform.translation;
    tiny_skia::Transform {
        sx: m.x_axis.x * sf,
        ky: m.x_axis.y * sf,
        kx: m.y_axis.x * sf,
        sy: m.y_axis.y * sf,
        tx: t.x * sf,
        ty: t.y * sf,
    }
}
