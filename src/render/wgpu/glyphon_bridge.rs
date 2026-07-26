//! Glyphon integration: GPU glyph atlas + instanced text rendering.
//!
//! Replaces the old CPU-rasterization → individual-texture → textured-quad
//! pipeline with glyphon's shared atlas and instanced triangle-strip rendering.

use std::cell::RefCell;
use std::rc::Rc;

use cosmic_text::{
    Attrs, Buffer, Color as CosmicColor, Family, FontSystem, Metrics, Shaping, SwashCache, Weight,
};
use glyphon::{
    Cache, ColorMode, Resolution, TextArea, TextAtlas, TextBounds, TextRenderer, Viewport,
};

use crate::core::error::{panic_to_string, push_error, UiError};
use crate::style::{Color, Rect, TextAlign};

/// Text rendering instruction produced during the paint phase and consumed
/// by [`GlyphonBridge::prepare`] during the GPU execution phase.
#[derive(Clone)]
pub struct TextAreaDesc {
    /// Shaped text buffer to render.
    pub buffer: Rc<RefCell<Buffer>>,
    /// Element ID for surface cache key.
    pub element_id: crate::core::ElementId,
    /// Text generation counter for surface cache invalidation.
    pub generation: u64,
    /// Physical pixel position on screen (left edge of text).
    pub left: f32,
    /// Physical pixel position on screen (top edge of text).
    pub top: f32,
    /// Scale factor (window scale_factor, e.g. 1.0, 2.0 for Retina).
    pub scale: f32,
    /// Default text colour.
    pub color: Color,
    /// Scroll offset X applied to text position.
    pub scroll_x: f32,
    /// Scroll offset Y applied to text position.
    pub scroll_y: f32,
    /// Clipping rectangle in logical pixels (None = no clipping).
    pub clip_rect: Option<Rect>,
    /// Z-index for paint ordering. Higher values render later (on top).
    pub z_index: i32,
}

impl TextAreaDesc {
    pub fn offset(&self, dx: f32, dy: f32) -> Self {
        Self {
            buffer: self.buffer.clone(),
            element_id: self.element_id,
            generation: self.generation,
            left: self.left + dx,
            top: self.top + dy,
            scale: self.scale,
            color: self.color,
            scroll_x: self.scroll_x,
            scroll_y: self.scroll_y,
            clip_rect: self.clip_rect.map(|r| r.offset(dx, dy)),
            z_index: self.z_index,
        }
    }
}

/// Encapsulates the full glyphon lifecycle: atlas management, glyph
/// rasterisation, GPU upload, and render-pass drawing.
pub struct GlyphonBridge {
    #[allow(dead_code)]
    cache: Cache,
    atlas: TextAtlas,
    viewport: Viewport,
    /// One `TextRenderer` per z-layer rendered this frame (Iced pattern). A
    /// single renderer's instance buffer is overwritten by the next prepare()
    /// within the same render pass, so each z-layer needs its own renderer;
    /// they all share this bridge's single atlas.
    renderers: Vec<TextRenderer>,
    /// Cursor into `renderers` for the current frame; reset to 0 in `trim()`.
    next_renderer: usize,
    /// Kept so extra pooled renderers can be created on demand.
    multisample: wgpu::MultisampleState,
    pub font_system: Option<FontSystem>,
    pub font_system_ok: bool,
    swash_cache: SwashCache,
}

impl GlyphonBridge {
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
        width: u32,
        height: u32,
        multisample: wgpu::MultisampleState,
    ) -> Self {
        let cache = Cache::new(device);
        let mut viewport = Viewport::new(device, &cache);
        viewport.update(queue, Resolution { width, height });

        let mut atlas =
            TextAtlas::with_color_mode(device, queue, &cache, format, ColorMode::Accurate);
        let renderer = TextRenderer::new(&mut atlas, device, multisample, None);

        let (font_system, font_system_ok) = match std::panic::catch_unwind(FontSystem::new) {
            Ok(fs) => (Some(fs), true),
            Err(panic) => {
                let msg = panic_to_string(&panic);
                push_error(UiError::FontLoad(format!(
                    "GlyphonBridge FontSystem::new panicked: {msg}"
                )));
                (None, false)
            }
        };
        let swash_cache = SwashCache::new();

        Self {
            cache,
            atlas,
            viewport,
            renderers: vec![renderer],
            next_renderer: 0,
            multisample,
            font_system,
            font_system_ok,
            swash_cache,
        }
    }

    /// Resize the viewport.  Call on every window resize.
    pub fn resize(&mut self, queue: &wgpu::Queue, width: u32, height: u32) {
        self.viewport.update(queue, Resolution { width, height });
    }

    /// Trim unused glyphs from the atlas and reset the per-layer renderer
    /// cursor.  Call once per frame (after submit).
    pub fn trim(&mut self) {
        self.atlas.trim();
        self.next_renderer = 0;
    }

    /// Invalidate all cached glyph data. Call when the GPU surface is
    /// destroyed and recreated (mobile lifecycle: onPause → onResume).
    pub fn invalidate_cache(&mut self) {
        self.atlas.trim();
        self.renderers.clear();
        self.next_renderer = 0;
    }

    /// Prepare one z-layer's text into a freshly-allocated pooled
    /// [`TextRenderer`] and return its index (pass it to [`render_layer`]).
    ///
    /// Each call advances the pool cursor so distinct layers never share a
    /// renderer — their instance buffers must not clobber each other within a
    /// single render pass. Returns `None` when there is nothing to draw.
    pub fn prepare_layer(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        descs: &[TextAreaDesc],
    ) -> Option<usize> {
        if !self.font_system_ok {
            return None;
        }
        if descs.is_empty() {
            return None;
        }

        // Sort by z_index so higher-z text renders last within this layer.
        let mut descs_sorted: Vec<&TextAreaDesc> = descs.iter().collect();
        descs_sorted.sort_by_key(|d| d.z_index);

        // Borrow all buffers first so they live for the entire prepare call.
        let bufs: Vec<std::cell::Ref<cosmic_text::Buffer>> =
            descs_sorted.iter().map(|d| d.buffer.borrow()).collect();

        // Build glyphon TextAreas (may be called multiple times on retry).
        let build_text_areas = || -> Vec<TextArea> {
            descs_sorted
                .iter()
                .zip(bufs.iter())
                .filter_map(|(d, buf)| {
                    if buf.layout_runs().count() == 0 {
                        return None;
                    }

                    let left = (d.left - d.scroll_x) * d.scale;
                    let top = (d.top - d.scroll_y) * d.scale;

                    let (mut l, mut t, mut r, b) = d
                        .clip_rect
                        .map(|c| {
                            let sf = d.scale;
                            let l = (c.x * sf).round() as i32;
                            let t = (c.y * sf).round() as i32;
                            let r = ((c.x + c.width) * sf).round() as i32;
                            let b = ((c.y + c.height) * sf).round() as i32;
                            (l, t, r, b)
                        })
                        .unwrap_or((i32::MIN, i32::MIN, i32::MAX, i32::MAX));

                    // Guard 1: text entirely to the left/above screen.
                    // glyphon's internal max(0) would invert these bounds.
                    if r <= 0 || b <= 0 {
                        return None;
                    }

                    // Guard 2: inward-shift by 1 physical px (left/top/right only)
                    // to prevent glyph boundary-straddling (glyphon 0.11 bug:
                    // clipped glyph height → negative i32 → u16::MAX quads).
                    // Bottom edge is NOT shifted inward — descenders (g, j, p, q, y)
                    // need that extra pixel to avoid being visibly clipped, especially
                    // when an element has background+padding extending past the clip.
                    l += 1;
                    t += 1;
                    r = (r - 1).max(l);

                    // Guard 3: skip after inward shift collapsed the bounds
                    if r <= l || b <= t {
                        return None;
                    }

                    Some(TextArea {
                        buffer: buf,
                        left,
                        top,
                        scale: d.scale,
                        bounds: TextBounds {
                            left: l,
                            top: t,
                            right: r,
                            bottom: b,
                        },
                        default_color: CosmicColor::rgba(
                            (d.color.r * 255.0) as u8,
                            (d.color.g * 255.0) as u8,
                            (d.color.b * 255.0) as u8,
                            (d.color.a * 255.0) as u8,
                        ),
                        custom_glyphs: &[],
                    })
                })
                .collect()
        };

        let text_areas = build_text_areas();
        if text_areas.is_empty() {
            return None;
        }

        // Hand out the next pooled renderer (grow on demand).
        let idx = self.next_renderer;
        if idx >= self.renderers.len() {
            let r = TextRenderer::new(&mut self.atlas, device, self.multisample, None);
            self.renderers.push(r);
        }
        self.next_renderer += 1;

        let Self {
            renderers,
            atlas,
            viewport,
            font_system,
            swash_cache,
            ..
        } = self;
        let fs = font_system
            .as_mut()
            .expect("font_system_ok guard ensures Some");
        let result =
            renderers[idx].prepare(device, queue, fs, atlas, viewport, text_areas, swash_cache);

        match result {
            Ok(()) => Some(idx),
            Err(glyphon::PrepareError::AtlasFull) => {
                eprintln!("[GLYPHON:ATLAS] atlas full — trimming + retry");
                atlas.trim();
                let mut new_r = TextRenderer::new(atlas, device, self.multisample, None);
                let retry_areas = build_text_areas();
                if retry_areas.is_empty() {
                    return None;
                }
                match new_r.prepare(device, queue, fs, atlas, viewport, retry_areas, swash_cache) {
                    Ok(()) => {
                        renderers[idx] = new_r;
                        Some(idx)
                    }
                    Err(_) => {
                        eprintln!("[GLYPHON:ATLAS] retry also failed — skipping layer");
                        None
                    }
                }
            }
        }
    }

    /// Render a layer previously prepared via [`prepare_layer`], by its index.
    /// Must be called inside an active render pass.
    pub fn render_layer(&self, idx: usize, pass: &mut wgpu::RenderPass<'_>) {
        let Some(renderer) = self.renderers.get(idx) else {
            return;
        };
        match renderer.render(&self.atlas, &self.viewport, pass) {
            Ok(()) => {}
            Err(glyphon::RenderError::RemovedFromAtlas) => {
                #[cfg(feature = "tracing")]
                tracing::warn!("glyphon: glyph removed from atlas during render");
            }
            Err(glyphon::RenderError::ScreenResolutionChanged) => {
                #[cfg(feature = "tracing")]
                tracing::debug!("glyphon: screen resolution changed");
            }
        }
    }
}

/// Create a shaped [`Buffer`] from a text string.
///
/// Uses the global thread-local `FontSystem`.  Returns a buffer that has
/// been shaped (`set_text` + `shape_until_scroll`).
/// `text_align` controls per-line alignment; [`TextAlign::Start`] and
/// [`TextAlign::End`] respect the text direction (auto-flip for RTL).
pub fn create_buffer(
    text: &str,
    font_size: f32,
    line_height: f32,
    font_weight: u16,
    font_family: Option<&str>,
    max_width: Option<f32>,
    text_align: TextAlign,
) -> Buffer {
    ensure_font_system();
    FONT_SYSTEM.with(|fs| {
        let mut guard = fs.borrow_mut();
        let fs = guard.as_mut().expect(
            "ensure_font_system guarantees Some; if the panic-once fallback also \
             failed the process has deeper font issues and should not continue",
        );
        let metrics = Metrics::new(font_size, font_size * line_height);
        let mut buffer = Buffer::new(&mut *fs, metrics);

        let mut buf = buffer.borrow_with(&mut *fs);
        buf.set_size(max_width, Some(font_size * line_height * 100.0));
        let mut attrs = Attrs::new().weight(Weight(font_weight));
        if let Some(f) = font_family {
            attrs = attrs.family(Family::Name(f));
        } else {
            attrs = attrs.family(Family::SansSerif);
        }
        buf.set_text(text, &attrs, Shaping::Advanced, None);
        drop(buf);

        buffer.shape_until_scroll(&mut *fs, false);

        let cosmic_align = match text_align {
            TextAlign::Left => Some(cosmic_text::Align::Left),
            // cosmic_text's Align::Center miscomputes emoji advance-width.
            // For emoji text we do manual centering in record_element_text.
            TextAlign::Center if text.chars().any(|c| c as u32 > 0x1F000) => None,
            TextAlign::Center => Some(cosmic_text::Align::Center),
            TextAlign::Right => Some(cosmic_text::Align::Right),
            TextAlign::Justify => Some(cosmic_text::Align::Justified),
            TextAlign::End => Some(cosmic_text::Align::End),
            TextAlign::Start => None,
        };
        for line in &mut buffer.lines {
            line.set_align(cosmic_align);
        }
        buffer.shape_until_scroll(&mut *fs, false);

        buffer
    })
}

/// Re-shape an existing buffer in place with new text/params — reuses the
/// `Buffer` allocation instead of building a fresh one (audit 2026-07-16
/// round 3, ③). Behaviour matches [`create_buffer`] (including the emoji
/// center-align workaround), but shapes **once**: alignment is applied to
/// the text lines before the single shape pass, where `create_buffer`
/// historically shaped, aligned, then shaped again.
pub fn reuse_buffer(
    buffer: &mut Buffer,
    text: &str,
    font_size: f32,
    line_height: f32,
    font_weight: u16,
    font_family: Option<&str>,
    max_width: Option<f32>,
    text_align: TextAlign,
) {
    if !ensure_font_system() {
        return;
    }
    FONT_SYSTEM.with(|fs| {
        let mut guard = fs.borrow_mut();
        let fs = guard.as_mut().unwrap();
        let metrics = Metrics::new(font_size, font_size * line_height);
        let mut buf = buffer.borrow_with(fs);
        buf.set_metrics(metrics);
        buf.set_size(max_width, Some(font_size * line_height * 100.0));
        let mut attrs = Attrs::new().weight(Weight(font_weight));
        if let Some(f) = font_family {
            attrs = attrs.family(Family::Name(f));
        } else {
            attrs = attrs.family(Family::SansSerif);
        }
        buf.set_text(text, &attrs, Shaping::Advanced, None);
        drop(buf);

        let cosmic_align = match text_align {
            TextAlign::Left => Some(cosmic_text::Align::Left),
            // cosmic_text's Align::Center miscomputes emoji advance-width.
            // For emoji text we do manual centering in record_element_text.
            TextAlign::Center if text.chars().any(|c| c as u32 > 0x1F000) => None,
            TextAlign::Center => Some(cosmic_text::Align::Center),
            TextAlign::Right => Some(cosmic_text::Align::Right),
            TextAlign::Justify => Some(cosmic_text::Align::Justified),
            TextAlign::End => Some(cosmic_text::Align::End),
            TextAlign::Start => None,
        };
        for line in &mut buffer.lines {
            line.set_align(cosmic_align);
        }
        buffer.shape_until_scroll(&mut *fs, false);
    })
}

/// Initialise the thread-local [`FONT_SYSTEM`] on first access.
/// Returns `true` when the font system is usable.
pub(crate) fn ensure_font_system() -> bool {
    FONT_SYSTEM.with(|cell| {
        if cell.borrow().is_none() {
            let init = std::panic::catch_unwind(FontSystem::new);
            match init {
                Ok(fs) => *cell.borrow_mut() = Some(fs),
                Err(p) => {
                    let msg = panic_to_string(&p);
                    push_error(UiError::FontLoad(format!(
                        "Thread-local FontSystem::new panicked: {msg}"
                    )));
                    // Last-resort fallback: empty font database with no system
                    // font scanning — guarantees a usable (but font-free) FontSystem.
                    let fallback = std::panic::catch_unwind(|| {
                        FontSystem::new_with_locale_and_db("en".into(), fontdb::Database::new())
                    });
                    if let Ok(fs) = fallback {
                        *cell.borrow_mut() = Some(fs);
                    }
                }
            }
        }
        cell.borrow().is_some()
    })
}

// Thread-local font system shared by all text operations in this thread.
thread_local! {
    pub static FONT_SYSTEM: RefCell<Option<FontSystem>> = const { RefCell::new(None) };
}

/// Intrinsic width read directly from an already-shaped buffer, avoiding a
/// second full shaping pass (audit 2026-07-17 round 2: the rebuild path
/// paid `reuse_buffer` + `measure_text_width` — two shapes per change).
///
/// Returns `Some(width)` only when the buffer laid out as a SINGLE run:
/// a single line means the text did not wrap under the buffer's width
/// constraint, so the glyph extent equals the unconstrained intrinsic width
/// (verified: 0.000 px deviation across ASCII/CJK/mixed samples).
/// Multi-line (wrapped or `\n`) returns `None` — caller falls back to a
/// fresh unconstrained measure. Callers must also gate on Start/Left
/// alignment: Justified stretches inter-word gaps and Center/Right offsets
/// are irrelevant only because we take max-min, but Justify changes extent.
pub(crate) fn intrinsic_width_from_buffer(buffer: &Buffer, font_size: f32) -> Option<f32> {
    let mut runs = buffer.layout_runs();
    let run = runs.next()?;
    if runs.next().is_some() {
        return None;
    }
    let (mut min_x, mut max_x) = (f32::MAX, 0.0f32);
    for g in run.glyphs.iter() {
        min_x = min_x.min(g.x);
        max_x = max_x.max(g.x + g.w);
    }
    if min_x < max_x && max_x > 0.0 {
        Some((max_x - min_x) + 2.0)
    } else {
        Some(font_size + 2.0)
    }
}

/// Trim the cosmic-text shape-run cache (audit 2026-07-17 round 2).
///
/// The `shape-run-cache` feature memoizes shaped glyph runs per
/// `(attrs, text-run)` inside the FontSystem — a 2-3x win on every shaping
/// call (words repeat heavily across UI strings) — but the cache grows
/// without bound unless aged out. Called once per PAINTED frame by the
/// frame driver; entries untouched for `keep_ages` trims are dropped, so
/// the cache holds only the runs shaped in the last few painted frames.
pub(crate) fn trim_shape_run_cache(keep_ages: u64) {
    if !ensure_font_system() {
        return;
    }
    FONT_SYSTEM.with(|fs| {
        if let Some(fs) = fs.borrow_mut().as_mut() {
            fs.shape_run_cache.trim(keep_ages);
        }
    });
}
