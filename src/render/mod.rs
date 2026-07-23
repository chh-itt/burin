//! Rendering system: Painter API, draw commands, and backend dispatch.

#[cfg(feature = "backend-tiny-skia")]
pub mod cpu;
#[cfg(all(feature = "ssr", feature = "backend-tiny-skia"))]
pub mod ssr;
#[cfg(feature = "backend-wgpu")]
pub mod wgpu;

pub(crate) mod paint_tree;
mod painter;
pub mod path;
pub mod text;

pub use painter::{
    BackdropRegion, ClipInfo, DrawCommand, LocalDrawItem, Painter, OUTLINE_Z_OFFSET,
};
pub use text::{
    ci_at_visual_x, expanded_to_raw, glyph_char_x, glyph_pos_at_ci, measure_text_width,
    move_visual_row, raw_to_expanded, shape_text, visual_row_at_exp_ci, visual_row_count,
    visual_row_from_y, TextGlyph, TextMeasurer,
};

#[cfg(feature = "backend-tiny-skia")]
pub use cpu::TinySkiaRenderer;
#[cfg(feature = "backend-wgpu")]
pub use wgpu::WgpuRenderer;

use crate::style::{Color, Size};
/// Cached text area in local element coordinates.
/// Converted to TextAreaDesc at replay time with current geometry + scroll offset.
#[derive(Clone)]
pub struct LocalTextArea {
    pub buffer: std::rc::Rc<std::cell::RefCell<cosmic_text::Buffer>>,
    pub generation: u64,
    pub scale: f32,
    pub color: crate::style::Color,
    pub scroll_x: f32,
    pub scroll_y: f32,
    pub local_left: f32,
    pub local_top: f32,
    pub clip_local_x: f32,
    pub clip_local_y: f32,
    pub clip_w: f32,
    pub clip_h: f32,
}

impl LocalTextArea {
    pub fn to_world(
        &self,
        x: f32,
        y: f32,
        scroll_ox: f32,
        scroll_oy: f32,
        z: i32,
        eid: crate::core::ElementId,
    ) -> crate::render::wgpu::glyphon_bridge::TextAreaDesc {
        self.to_world_clipped(x, y, scroll_ox, scroll_oy, z, eid, None)
    }

    /// Convert to world-space TextAreaDesc with optional parent clip.
    /// Always returns a TextAreaDesc — even if the clip rect degenerates
    /// to zero area. The glyphon bridge's own guards (offscreen check,
    /// inward-shrink + collapse) handle invalid TextBounds at the GPU
    /// layer. Returning None would silently drop text from the cache,
    /// causing text to disappear when subtree caches replay.
    pub fn to_world_clipped(
        &self,
        x: f32,
        y: f32,
        scroll_ox: f32,
        scroll_oy: f32,
        z: i32,
        eid: crate::core::ElementId,
        parent_clip: Option<crate::style::Rect>,
    ) -> crate::render::wgpu::glyphon_bridge::TextAreaDesc {
        let mut clip = crate::style::Rect::new(
            x + self.clip_local_x - scroll_ox,
            y + self.clip_local_y - scroll_oy,
            self.clip_w,
            self.clip_h,
        );
        if let Some(pc) = parent_clip {
            let ix = clip.x.max(pc.x);
            let iy = clip.y.max(pc.y);
            let iw = ((clip.x + clip.width).min(pc.x + pc.width) - ix).max(0.0);
            let ih = ((clip.y + clip.height).min(pc.y + pc.height) - iy).max(0.0);
            clip = crate::style::Rect::new(ix, iy, iw, ih);
        }
        crate::render::wgpu::glyphon_bridge::TextAreaDesc {
            buffer: self.buffer.clone(),
            element_id: eid,
            generation: self.generation,
            left: x + self.local_left - scroll_ox,
            top: y + self.local_top - scroll_oy,
            scale: self.scale,
            color: self.color,
            scroll_x: self.scroll_x,
            scroll_y: self.scroll_y,
            clip_rect: Some(clip),
            z_index: z,
        }
    }
}

/// Cached draw commands for a single element's own visual content.
///
/// Commands are stored monolithically (surface world-cmds + decor world-cmds)
/// for bug-for-bug identical replay.  `decor_start` marks the index where
/// decor commands begin, enabling partial re-recording of individual layers.
#[derive(Clone)]
pub struct CachedScene {
    pub local_items: Vec<painter::LocalDrawItem>,
    pub commands: Vec<DrawCommand>,
    pub local_text_areas: Vec<LocalTextArea>,
    /// Fine-grained invalidation keys.
    pub surface_gen: u64,
    pub text_gen: u64,
    pub decor_gen: u64,
    /// Index in `commands` where decor (selection/cursor/…) starts.
    pub decor_start: usize,
    /// Index in `local_text_areas` where decor text (error/tooltip) starts.
    pub decor_text_start: usize,
    /// Scroll offset at record time — invalidates on scroll change.
    pub scroll_x: f32,
    pub scroll_y: f32,
}

/// Cached scene for an entire subtree, enabling O(k) paint skip.
/// Stores world-space draw commands with the subtree root's position
/// and accumulated scroll offset at record time.
///
/// **Validity**: cache hit requires ALL of:
/// 1. `subtree_gen` matches (content unchanged)
/// 2. `scroll_gen` matches (parent scroll container hasn't scrolled)
/// 3. `scroll_ox/oy` matches accumulated scroll offset (geometry unchanged)
///
/// On replay, commands are offset by both the screen_bounds delta (layout
/// changes) and the scroll delta (parent scrolling). This decouples content
/// invalidation from geometry: scroll only shifts positions, it doesn't
/// require re-recording draw commands.
#[derive(Clone)]
pub struct CachedSubtree {
    pub commands: Vec<DrawCommand>,
    pub text_areas: Vec<crate::render::wgpu::glyphon_bridge::TextAreaDesc>,
    /// Backdrop-blur regions produced by this subtree (screen-effect ops that
    /// must be re-emitted on cached replay, else the effect vanishes on static
    /// frames). Stored in document space + transform, like when first emitted.
    pub backdrop_regions: Vec<BackdropRegion>,
    pub root_x: f32,
    pub root_y: f32,
    /// Accumulated scroll offset at cache time — used on replay to compute
    /// the scroll delta and shift commands accordingly.
    pub scroll_ox: f32,
    pub scroll_oy: f32,
    pub content_gen: u64,
    pub layout_gen: u64,
    pub scroll_gen: u64,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RendererChoice {
    Auto,
    Gpu,
    Cpu,
}

/// Trait for custom rendering backends.
///
/// Third-party crates can implement this trait and inject it via
/// `BackendRenderer::Custom(Box::new(my_backend))`.
pub trait RenderBackend: 'static {
    fn set_clear_color(&mut self, c: Color);
}

/// Backend-renderer dispatching enum.
///
/// Zero dynamic-dispatch overhead for built-in backends: every method
/// uses match dispatch that the compiler can inline and branch-predict.
pub enum BackendRenderer {
    #[cfg(feature = "backend-wgpu")]
    Gpu(WgpuRenderer),
    #[cfg(feature = "backend-tiny-skia")]
    Cpu(TinySkiaRenderer),
    Custom(Box<dyn RenderBackend>),
}

macro_rules! renderer_dispatch {
    ($self:expr, |$r:ident| $body:expr) => {
        match $self {
            #[cfg(feature = "backend-wgpu")]
            BackendRenderer::Gpu($r) => $body,
            #[cfg(feature = "backend-tiny-skia")]
            BackendRenderer::Cpu($r) => $body,
            BackendRenderer::Custom($r) => $body,
        }
    };
}

impl BackendRenderer {
    pub fn set_clear_color(&mut self, c: Color) {
        renderer_dispatch!(self, |r| r.set_clear_color(c));
    }

    /// Create or recreate the rendering surface. On desktop this is called
    /// once at startup; on mobile it is called on each onResume cycle.
    #[cfg(feature = "backend-wgpu")]
    pub fn create_surface(
        &mut self,
        window: &std::sync::Arc<dyn winit::window::Window>,
    ) -> Result<(), crate::core::error::UiError> {
        match self {
            BackendRenderer::Gpu(r) => r.create_surface(window).map_err(|e| {
                crate::core::error::UiError::GpuInit(match e {
                    crate::render::wgpu::RenderError::NoAdapter => {
                        crate::core::error::GpuErrorKind::NoAdapter
                    }
                    crate::render::wgpu::RenderError::Surface => {
                        crate::core::error::GpuErrorKind::Surface
                    }
                    crate::render::wgpu::RenderError::Device => {
                        crate::core::error::GpuErrorKind::Device
                    }
                })
            }),
            BackendRenderer::Cpu(_) => Ok(()),
            BackendRenderer::Custom(_) => Ok(()),
        }
    }

    /// Destroy the rendering surface, preserving Device/Queue/Pipeline resources.
    /// Called when the native window is destroyed (mobile: onPause/onStop).
    pub fn destroy_surface(&mut self) {
        match self {
            #[cfg(feature = "backend-wgpu")]
            BackendRenderer::Gpu(r) => r.destroy_surface(),
            _ => {}
        }
    }

    /// Check whether the rendering surface is currently available.
    pub fn surface_ready(&self) -> bool {
        match self {
            #[cfg(feature = "backend-wgpu")]
            BackendRenderer::Gpu(r) => r.has_surface(),
            #[cfg(feature = "backend-tiny-skia")]
            BackendRenderer::Cpu(_) => true,
            BackendRenderer::Custom(_) => true,
        }
    }

    #[cfg(feature = "backend-wgpu")]
    pub fn begin_frame(&mut self) -> Result<wgpu::Frame, wgpu::RenderError> {
        match self {
            BackendRenderer::Gpu(r) => r.begin_frame(),
            BackendRenderer::Cpu(_) => Err(wgpu::RenderError::Surface),
            BackendRenderer::Custom(_) => Err(wgpu::RenderError::Surface),
        }
    }

    #[cfg(feature = "backend-wgpu")]
    pub fn draw_commands(
        &mut self,
        frame: &mut wgpu::Frame,
        commands: &[DrawCommand],
        text_areas: &[wgpu::TextAreaDesc],
        backdrop_regions: &[painter::BackdropRegion],
        hint: Size,
    ) {
        match self {
            BackendRenderer::Gpu(r) => {
                r.draw_commands(frame, commands, text_areas, backdrop_regions, hint)
            }
            BackendRenderer::Cpu(_) => {}
            BackendRenderer::Custom(_) => {}
        }
    }

    #[cfg(feature = "backend-wgpu")]
    pub fn end_frame(&mut self, frame: wgpu::Frame) {
        match self {
            BackendRenderer::Gpu(r) => r.end_frame(frame),
            BackendRenderer::Cpu(_) => {}
            BackendRenderer::Custom(_) => {}
        }
    }

    #[cfg(feature = "backend-wgpu")]
    pub fn resize_gpu(&mut self, w: u32, h: u32) {
        match self {
            BackendRenderer::Gpu(r) => r.resize(w, h),
            BackendRenderer::Cpu(_) => {}
            BackendRenderer::Custom(_) => {}
        }
    }

    #[cfg(feature = "backend-wgpu")]
    pub fn set_scale_factor_gpu(&mut self, sf: f64) {
        match self {
            BackendRenderer::Gpu(r) => r.set_scale_factor(sf),
            BackendRenderer::Cpu(_) => {}
            BackendRenderer::Custom(_) => {}
        }
    }

    #[cfg(feature = "backend-tiny-skia")]
    pub fn resize_cpu(&mut self, logical_w: f32, logical_h: f32, sf: f32) {
        match self {
            BackendRenderer::Cpu(r) => r.resize(logical_w, logical_h, sf),
            BackendRenderer::Gpu(_) => {}
            BackendRenderer::Custom(_) => {}
        }
    }
}
