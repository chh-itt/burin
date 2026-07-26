//! wgpu GPU rendering backend.

pub mod glyphon_bridge;
mod pipeline;

use std::collections::HashMap;

use crate::core::error::{push_error, GpuErrorKind, UiError};
use crate::render::ClipInfo;
use glam::Affine2;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use pipeline::{
    create_atlas_bind_group, create_blend_bind_group_layout, create_blend_pipeline,
    create_blit_bind_group_layout, create_blit_pipeline, create_blur_bind_group_layout,
    create_blur_pipeline, create_composite_pipeline, create_gradient_pipeline,
    create_path_pipeline, create_rect_pipeline, create_text_bind_group_layout,
    create_text_pipeline, BlitVertex, BlurUniforms, GradientVertex, PathVertex, RectVertex,
    TextVertex,
};

use crate::render::painter::DrawCommand;
use crate::render::path::{bezpath_bounds, bezpath_to_lyon, hash_bezpath};
use crate::style::Brush;
use crate::style::{Color, Rect, Size};
use lyon::tessellation::{
    BuffersBuilder, FillOptions, FillTessellator, FillVertex, StrokeOptions, StrokeTessellator,
    VertexBuffers,
};

pub use glyphon_bridge::{create_buffer, GlyphonBridge, TextAreaDesc};

const MAX_IMAGE_DIM: u32 = 4096;

/// Pre-computed mip chain for a single image.
pub(crate) struct CpuImageMips {
    pub width: u32,
    pub height: u32,
    /// mip 0 = full resolution, each subsequent level halves both dims.
    pub levels: Vec<Rc<Vec<u8>>>,
}

fn mip_level_count(width: u32, height: u32) -> u32 {
    let max_dim = width.max(height) as f64;
    (max_dim.log2().floor() as u32).max(1)
}

/// Box-filter downsample RGBA pixels from src_w×src_h to dst_w×dst_h.
fn box_filter(src: &[u8], src_w: u32, src_h: u32, dst_w: u32, dst_h: u32) -> Vec<u8> {
    let mut dst = vec![0u8; (dst_w * dst_h * 4) as usize];
    let x_ratio = src_w as f64 / dst_w as f64;
    let y_ratio = src_h as f64 / dst_h as f64;
    for dy in 0..dst_h {
        let y0 = (dy as f64 * y_ratio).floor() as u32;
        let y1 = (((dy as f64 + 1.0) * y_ratio).ceil() as u32).min(src_h);
        for dx in 0..dst_w {
            let x0 = (dx as f64 * x_ratio).floor() as u32;
            let x1 = (((dx as f64 + 1.0) * x_ratio).ceil() as u32).min(src_w);
            let mut r = 0u64;
            let mut g = 0u64;
            let mut b = 0u64;
            let mut a = 0u64;
            let mut cnt = 0u64;
            for sy in y0..y1 {
                for sx in x0..x1 {
                    let si = (sy * src_w + sx) as usize * 4;
                    if si + 4 <= src.len() {
                        r += src[si] as u64;
                        g += src[si + 1] as u64;
                        b += src[si + 2] as u64;
                        a += src[si + 3] as u64;
                        cnt += 1;
                    }
                }
            }
            if cnt > 0 {
                let di = (dy * dst_w + dx) as usize * 4;
                dst[di] = (r / cnt) as u8;
                dst[di + 1] = (g / cnt) as u8;
                dst[di + 2] = (b / cnt) as u8;
                dst[di + 3] = (a / cnt) as u8;
            }
        }
    }
    dst
}

fn build_mip_chain(pixels: &[u8], width: u32, height: u32) -> Vec<Rc<Vec<u8>>> {
    let count = mip_level_count(width, height);
    let mut levels = Vec::with_capacity(count as usize);
    levels.push(Rc::new(pixels.to_vec()));
    let mut mw = width;
    let mut mh = height;
    for _ in 1..count {
        let nw = (mw / 2).max(1);
        let nh = (mh / 2).max(1);
        let mip = box_filter(levels.last().unwrap(), mw, mh, nw, nh);
        levels.push(Rc::new(mip));
        mw = nw;
        mh = nh;
    }
    levels
}

struct ImageEntry {
    width: u32,
    height: u32,
    full_res: Rc<Vec<u8>>,
    mips: Option<Rc<CpuImageMips>>,
}

// Thread-local registry mapping image hashes to their data.
// NOTE: intentionally NOT migrated to AppContext. `ImageEntry`/`CpuImageMips`
// hold `Rc<Vec<u8>>` (render-layer, !Send/!Sync) so this cannot become a
// process-level singleton without an Rc->Arc rewrite of the whole image path.
// More importantly it is a content-addressed IMMUTABLE cache (hash -> pixels):
// sharing the same image across windows is correct de-duplication, not state
// that needs per-window isolation. Kept in the render layer by design.
thread_local! {
static IMAGE_REGISTRY: RefCell<HashMap<u64, ImageEntry>> = RefCell::new(HashMap::new());
/// hash → (element refcount, pinned). Pinned entries (registered without an
/// owning element via `register_image`) are never evicted — old behavior.
/// Element-owned entries (`register_image_for`) are freed when the last
/// referencing element is torn down (audit 2026-07-17 round 3, Finding E).
static IMAGE_REFS: RefCell<HashMap<u64, (usize, bool)>> = RefCell::new(HashMap::new());
/// element → image hashes it registered (usually 1).
static ELEMENT_IMAGES: RefCell<HashMap<crate::core::ElementId, Vec<u64>>> = RefCell::new(HashMap::new());
}

/// Register image data for GPU/CPU rendering. Called from widget mount.
///
/// If the image exceeds MAX_IMAGE_DIM (4096) in either dimension it is
/// box-filtered down on registration.  Mip chain generation is deferred
/// to `lookup_image_mips` (first render time), so images that are
/// registered but never rendered incur no mip cost.
///
/// Entries registered through this anonymous form are PINNED (never evicted).
/// Prefer `register_image_for` from widget mounts so pixel data is freed
/// when the last referencing element is torn down.
pub fn register_image(hash: u64, width: u32, height: u32, pixels: Rc<Vec<u8>>) {
    IMAGE_REFS.with(|r| r.borrow_mut().entry(hash).or_insert((0, false)).1 = true);
    register_image_inner(hash, width, height, pixels);
}

/// Register image data owned by `eid`: a per-hash refcount tracks how many
/// live elements reference the pixels; the core teardown protocol decrements
/// it and frees the entry (pixels + mips) when it reaches zero.
pub(crate) fn register_image_for(
    eid: crate::core::ElementId,
    hash: u64,
    width: u32,
    height: u32,
    pixels: Rc<Vec<u8>>,
) {
    crate::core::dirty_registry::register_teardown_hook(image_teardown_cleanup);
    let newly_referenced = ELEMENT_IMAGES.with(|m| {
        let mut m = m.borrow_mut();
        let hashes = m.entry(eid).or_default();
        if hashes.contains(&hash) {
            false
        } else {
            hashes.push(hash);
            true
        }
    });
    if newly_referenced {
        IMAGE_REFS.with(|r| r.borrow_mut().entry(hash).or_insert((0, false)).0 += 1);
    }
    register_image_inner(hash, width, height, pixels);
}

fn register_image_inner(hash: u64, width: u32, height: u32, pixels: Rc<Vec<u8>>) {
    // Content-addressed: an existing entry for this hash is already correct
    // (and may carry a built mip chain) — skip the re-insert and, for
    // oversized images, the redundant box_filter.
    if IMAGE_REGISTRY.with(|reg| reg.borrow().contains_key(&hash)) {
        return;
    }
    let (w, h, data) = if width > MAX_IMAGE_DIM || height > MAX_IMAGE_DIM {
        let scale = (MAX_IMAGE_DIM as f64 / width.max(height) as f64).min(1.0);
        let nw = (width as f64 * scale) as u32;
        let nh = (height as f64 * scale) as u32;
        (nw, nh, Rc::new(box_filter(&pixels, width, height, nw, nh)))
    } else {
        (width, height, pixels)
    };
    IMAGE_REGISTRY.with(|reg| {
        reg.borrow_mut().insert(
            hash,
            ImageEntry {
                width: w,
                height: h,
                full_res: data,
                mips: None,
            },
        );
    });
}

fn image_teardown_cleanup(id: crate::core::ElementId) {
    let hashes = ELEMENT_IMAGES.with(|m| m.borrow_mut().remove(&id));
    let Some(hashes) = hashes else { return };
    for hash in hashes {
        let evict = IMAGE_REFS.with(|r| {
            let mut r = r.borrow_mut();
            if let Some(entry) = r.get_mut(&hash) {
                entry.0 = entry.0.saturating_sub(1);
                if entry.0 == 0 && !entry.1 {
                    r.remove(&hash);
                    return true;
                }
            }
            false
        });
        if evict {
            IMAGE_REGISTRY.with(|reg| {
                reg.borrow_mut().remove(&hash);
            });
        }
    }
}

/// Test-only introspection: (registry entries, ref entries, element links).
#[doc(hidden)]
pub fn debug_image_registry_sizes() -> (usize, usize, usize) {
    (
        IMAGE_REGISTRY.with(|r| r.borrow().len()),
        IMAGE_REFS.with(|r| r.borrow().len()),
        ELEMENT_IMAGES.with(|m| m.borrow().len()),
    )
}

/// Look up full-resolution image data by hash.  Used by CPU ImageCache.
pub fn lookup_image(hash: u64) -> Option<(u32, u32, Rc<Vec<u8>>)> {
    IMAGE_REGISTRY.with(|reg| {
        let reg = reg.borrow();
        reg.get(&hash)
            .map(|e| (e.width, e.height, e.full_res.clone()))
    })
}

/// Look up (or lazily generate) the mip chain for an image.
pub(crate) fn lookup_image_mips(hash: u64) -> Option<Rc<CpuImageMips>> {
    IMAGE_REGISTRY.with(|reg| {
        let mut reg = reg.borrow_mut();
        let entry = reg.get_mut(&hash)?;
        match &entry.mips {
            Some(mips) => Some(Rc::clone(mips)),
            None => {
                let levels = build_mip_chain(&entry.full_res, entry.width, entry.height);
                let mips = Rc::new(CpuImageMips {
                    width: entry.width,
                    height: entry.height,
                    levels,
                });
                entry.mips = Some(Rc::clone(&mips));
                Some(mips)
            }
        }
    })
}

pub struct WgpuRenderer {
    instance: wgpu::Instance,
    adapter: wgpu::Adapter,
    device: wgpu::Device,
    queue: wgpu::Queue,
    surface: Option<wgpu::Surface<'static>>,
    config: Option<wgpu::SurfaceConfiguration>,
    rect_pipeline: wgpu::RenderPipeline,
    gradient_pipeline: wgpu::RenderPipeline,
    text_pipeline: wgpu::RenderPipeline,
    text_bind_group_layout: wgpu::BindGroupLayout,
    pub glyphon: GlyphonBridge,
    /// Separate glyphon instance for overlay/z>0 text.  Two independent
    /// atlases prevent prepare() conflicts within a single render pass.
    glyphon_overlay: Option<GlyphonBridge>,
    /// Image texture cache: (hash → texture, view, bind_group, w, h, last_used_frame)
    image_cache: HashMap<
        u64,
        (
            wgpu::Texture,
            wgpu::TextureView,
            wgpu::BindGroup,
            f32,
            f32,
            u64,
        ),
    >,
    image_cache_frame: u64,
    screen_size: Size,
    scale_factor: f32,
    clear_color: crate::style::Color,
    /// Persistent retained framebuffer — avoids swapchain LoadOp::Load unreliability.
    persistent_tex: Option<wgpu::Texture>,
    persistent_view: Option<wgpu::TextureView>,
    persistent_bind_group: Option<wgpu::BindGroup>,
    msaa_tex: Option<wgpu::Texture>,
    msaa_view: Option<wgpu::TextureView>,
    /// Sample count for MSAA (same value used for pipelines and glyphon).
    sample_count: u32,
    persistent_dirty: bool,
    blit_pipeline: wgpu::RenderPipeline,
    blit_bind_group_layout: wgpu::BindGroupLayout,
    image_sampler: wgpu::Sampler,
    blit_sampler: wgpu::Sampler,
    belt: wgpu::util::StagingBelt,
    rect_vbuf: Option<(wgpu::Buffer, u64)>,
    rect_ibuf: Option<(wgpu::Buffer, u64)>,
    effect_vbuf: Option<(wgpu::Buffer, u64)>,
    effect_ibuf: Option<(wgpu::Buffer, u64)>,
    grad_vbuf: Option<(wgpu::Buffer, u64)>,
    grad_ibuf: Option<(wgpu::Buffer, u64)>,
    image_vbuf: Option<(wgpu::Buffer, u64)>,
    image_ibuf: Option<(wgpu::Buffer, u64)>,
    blit_vbuf: Option<wgpu::Buffer>,
    blit_ibuf: Option<wgpu::Buffer>,
    path_pipeline: wgpu::RenderPipeline,
    path_cache: rustc_hash::FxHashMap<PathCacheKey, CachedPathMesh>,
    path_cache_frame: u64,
    path_vbuf: Option<(wgpu::Buffer, u64)>,
    path_ibuf: Option<(wgpu::Buffer, u64)>,
    // ── Effect compositing (blend modes; P3 adds backdrop blur) ──
    // snapshot_tex is a copy of the persistent (resolved) texture taken right
    // before an effect pass, so the effect shader can read the backdrop (dst)
    // without a feedback loop on the live attachment.
    snapshot_tex: Option<wgpu::Texture>,
    snapshot_view: Option<wgpu::TextureView>,
    snapshot_bind_group: Option<wgpu::BindGroup>,
    blend_pipeline: wgpu::RenderPipeline,
    blend_bind_group_layout: wgpu::BindGroupLayout,
    blend_sampler: wgpu::Sampler,
    // ── Backdrop blur (P3) ──
    blur_pipeline: wgpu::RenderPipeline,
    blur_bind_group_layout: wgpu::BindGroupLayout,
    composite_pipeline: wgpu::RenderPipeline,
    effect_h_tex: Option<wgpu::Texture>,
    effect_h_view: Option<wgpu::TextureView>,
    effect_v_tex: Option<wgpu::Texture>,
    effect_v_view: Option<wgpu::TextureView>,
    effect_v_bind_group: Option<wgpu::BindGroup>,
    /// Persistent blur uniform buffers + bind groups (audit 2026-07-17
    /// round 5, B5): previously re-created per blur region per frame
    /// (2 create_buffer + 2 create_bind_group each). The bind groups
    /// reference the persistent snapshot/effect views, so they are only
    /// invalidated on resize (views recreated → set to None).
    blur_h_res: Option<(wgpu::Buffer, wgpu::BindGroup)>,
    blur_v_res: Option<(wgpu::Buffer, wgpu::BindGroup)>,
    backdrop_vbuf: Option<(wgpu::Buffer, u64)>,
    backdrop_ibuf: Option<(wgpu::Buffer, u64)>,
}

impl WgpuRenderer {
    pub async fn new(window: Arc<dyn winit::window::Window>) -> Result<Self, RenderError> {
        #[cfg(any(feature = "devtools", feature = "file-logging"))]
        let t_total = std::time::Instant::now();
        let size = window.surface_size();
        let sf = window.scale_factor() as f32;
        let screen_size = Size::new(size.width as f32, size.height as f32);

        #[cfg(any(feature = "devtools", feature = "file-logging"))]
        let t0 = std::time::Instant::now();
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::PRIMARY,
            flags: wgpu::InstanceFlags::default(),
            backend_options: wgpu::BackendOptions::default(),
            memory_budget_thresholds: Default::default(),
            display: None,
        });
        let surface = match instance.create_surface(Arc::clone(&window)) {
            Ok(s) => s,
            Err(e) => {
                push_error(UiError::GpuInit(crate::core::error::GpuErrorKind::Other(
                    e.to_string(),
                )));
                return Err(RenderError::Surface);
            }
        };
        #[cfg(any(feature = "devtools", feature = "file-logging"))]
        tracing::debug!(target: "wgpu", "instance+surface: {:?}", t0.elapsed());

        #[cfg(any(feature = "devtools", feature = "file-logging"))]
        let t1 = std::time::Instant::now();
        let adapter = match instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                ..Default::default()
            })
            .await
        {
            Ok(a) => a,
            Err(e) => {
                push_error(UiError::GpuInit(crate::core::error::GpuErrorKind::Other(
                    e.to_string(),
                )));
                return Err(RenderError::NoAdapter);
            }
        };
        #[cfg(any(feature = "devtools", feature = "file-logging"))]
        tracing::debug!(target: "wgpu", "request_adapter: {:?}", t1.elapsed());

        #[cfg(any(feature = "devtools", feature = "file-logging"))]
        let t2 = std::time::Instant::now();
        let (device, queue) = match adapter
            .request_device(&wgpu::DeviceDescriptor::default())
            .await
        {
            Ok(dq) => dq,
            Err(e) => {
                push_error(UiError::GpuInit(crate::core::error::GpuErrorKind::Other(
                    e.to_string(),
                )));
                return Err(RenderError::Device);
            }
        };
        #[cfg(any(feature = "devtools", feature = "file-logging"))]
        tracing::debug!(target: "wgpu", "request_device: {:?}", t2.elapsed());

        #[cfg(any(feature = "devtools", feature = "file-logging"))]
        let t3 = std::time::Instant::now();
        let config = match surface.get_default_config(&adapter, size.width, size.height) {
            Some(c) => c,
            None => {
                push_error(UiError::GpuInit(crate::core::error::GpuErrorKind::Surface));
                return Err(RenderError::Surface);
            }
        };
        surface.configure(&device, &config);

        let sample_count = 4u32;
        let rect_pipeline = create_rect_pipeline(&device, config.format, sample_count);
        let gradient_pipeline = create_gradient_pipeline(&device, config.format, sample_count);
        let text_bind_group_layout = create_text_bind_group_layout(&device);
        let text_pipeline = create_text_pipeline(
            &device,
            config.format,
            &text_bind_group_layout,
            sample_count,
        );
        #[cfg(any(feature = "devtools", feature = "file-logging"))]
        tracing::debug!(target: "wgpu", "pipelines: {:?}", t3.elapsed());

        #[cfg(any(feature = "devtools", feature = "file-logging"))]
        let t4 = std::time::Instant::now();
        let glyphon = GlyphonBridge::new(
            &device,
            &queue,
            config.format,
            size.width,
            size.height,
            wgpu::MultisampleState {
                count: sample_count,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
        );
        #[cfg(any(feature = "devtools", feature = "file-logging"))]
        tracing::debug!(target: "wgpu", "glyphon bridge: {:?}", t4.elapsed());

        #[cfg(any(feature = "devtools", feature = "file-logging"))]
        let t5 = std::time::Instant::now();
        let blit_bind_group_layout = create_blit_bind_group_layout(&device);
        let blit_pipeline =
            create_blit_pipeline(&device, config.format, &blit_bind_group_layout, 1);
        let path_pipeline = create_path_pipeline(&device, config.format, sample_count);
        let image_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("image sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..Default::default()
        });
        let blit_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("blit sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        // ── Effect (blend) infrastructure ──
        let blend_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("blend sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let blend_bind_group_layout = create_blend_bind_group_layout(&device);
        let blend_pipeline = create_blend_pipeline(
            &device,
            config.format,
            &blend_bind_group_layout,
            sample_count,
        );
        let blur_bind_group_layout = create_blur_bind_group_layout(&device);
        let blur_pipeline = create_blur_pipeline(&device, config.format, &blur_bind_group_layout);
        // Composite samples a single texture+sampler (same layout shape as blit/blend tex+sampler).
        let composite_pipeline = create_composite_pipeline(
            &device,
            config.format,
            &blend_bind_group_layout,
            sample_count,
        );
        #[cfg(any(feature = "devtools", feature = "file-logging"))]
        tracing::debug!(target: "wgpu", "blit pipeline: {:?}", t5.elapsed());

        #[cfg(any(feature = "devtools", feature = "file-logging"))]
        tracing::debug!(target: "wgpu", "total: {:?}", t_total.elapsed());

        let blit_vbuf = {
            let verts: [BlitVertex; 4] = [
                BlitVertex {
                    position: [-1.0, -1.0],
                    uv: [0.0, 1.0],
                },
                BlitVertex {
                    position: [1.0, -1.0],
                    uv: [1.0, 1.0],
                },
                BlitVertex {
                    position: [1.0, 1.0],
                    uv: [1.0, 0.0],
                },
                BlitVertex {
                    position: [-1.0, 1.0],
                    uv: [0.0, 0.0],
                },
            ];
            let idx: [u32; 6] = [0, 1, 2, 0, 2, 3];
            let v = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("blit vbuf"),
                size: std::mem::size_of_val(&verts) as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: true,
            });
            if let Ok(mut mapped) = v.slice(..).get_mapped_range_mut() {
                mapped.copy_from_slice(bytemuck::cast_slice(&verts));
            } else {
                crate::core::error::push_error(crate::core::error::UiError::GpuRender(
                    "Failed to map blit vertex buffer".into(),
                ));
            }
            v.unmap();
            let i = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("blit ibuf"),
                size: std::mem::size_of_val(&idx) as u64,
                usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: true,
            });
            if let Ok(mut mapped) = i.slice(..).get_mapped_range_mut() {
                mapped.copy_from_slice(bytemuck::cast_slice(&idx));
            } else {
                crate::core::error::push_error(crate::core::error::UiError::GpuRender(
                    "Failed to map blit index buffer".into(),
                ));
            }
            i.unmap();
            (v, i)
        };

        let belt = wgpu::util::StagingBelt::new(device.clone(), 1_000_000);

        Ok(Self {
            instance,
            adapter,
            device,
            queue,
            surface: Some(surface),
            config: Some(config),
            rect_pipeline,
            gradient_pipeline,
            text_pipeline,
            text_bind_group_layout,
            glyphon,
            glyphon_overlay: None,
            image_cache: HashMap::new(),
            image_cache_frame: 0,
            screen_size,
            scale_factor: sf,
            clear_color: crate::style::Color::rgba8(26, 26, 31, 255),
            persistent_tex: None,
            persistent_view: None,
            persistent_bind_group: None,
            msaa_tex: None,
            msaa_view: None,
            persistent_dirty: true,
            sample_count,
            blit_pipeline,
            blit_bind_group_layout,
            blit_sampler,
            image_sampler,
            belt,
            rect_vbuf: None,
            rect_ibuf: None,
            effect_vbuf: None,
            effect_ibuf: None,
            grad_vbuf: None,
            grad_ibuf: None,
            image_vbuf: None,
            image_ibuf: None,
            blit_vbuf: Some(blit_vbuf.0),
            blit_ibuf: Some(blit_vbuf.1),
            path_pipeline,
            path_cache: rustc_hash::FxHashMap::default(),
            path_cache_frame: 0,
            path_vbuf: None,
            path_ibuf: None,
            snapshot_tex: None,
            snapshot_view: None,
            snapshot_bind_group: None,
            blend_pipeline,
            blend_bind_group_layout,
            blend_sampler,
            blur_pipeline,
            blur_bind_group_layout,
            composite_pipeline,
            effect_h_tex: None,
            effect_h_view: None,
            effect_v_tex: None,
            effect_v_view: None,
            effect_v_bind_group: None,
            blur_h_res: None,
            blur_v_res: None,
            backdrop_vbuf: None,
            backdrop_ibuf: None,
        })
    }

    pub fn set_clear_color(&mut self, c: crate::style::Color) {
        self.clear_color = c;
    }

    #[allow(dead_code)]
    fn surface_config(&self) -> Option<&wgpu::SurfaceConfiguration> {
        self.config.as_ref()
    }

    pub fn has_surface(&self) -> bool {
        self.surface.is_some()
    }

    pub fn begin_frame(&mut self) -> Result<Frame, RenderError> {
        let surface = self.surface.as_ref().ok_or(RenderError::Surface)?;
        let (texture, view) = match surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(t) => {
                let v = t
                    .texture
                    .create_view(&wgpu::TextureViewDescriptor::default());
                (t, v)
            }
            wgpu::CurrentSurfaceTexture::Suboptimal(t) => {
                let v = t
                    .texture
                    .create_view(&wgpu::TextureViewDescriptor::default());
                (t, v)
            }
            _ => return Err(RenderError::Surface),
        };
        let encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("burin frame"),
            });
        Ok(Frame {
            texture,
            view,
            encoder,
        })
    }

    fn ensure_persistent_texture(&mut self) {
        let cfg = match &self.config {
            Some(c) => c,
            None => return,
        };
        let w = cfg.width;
        let h = cfg.height;
        if w == 0 || h == 0 {
            return;
        }
        let fmt = cfg.format;
        let msaa = 4u32;
        if let Some(ref tex) = self.persistent_tex {
            if tex.width() == w && tex.height() == h {
                return;
            }
        }

        let msaa_tex = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("msaa persistent"),
            size: wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: msaa,
            dimension: wgpu::TextureDimension::D2,
            format: fmt,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let msaa_view = msaa_tex.create_view(&wgpu::TextureViewDescriptor::default());

        let resolve_tex = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("resolve persistent"),
            size: wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: fmt,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_DST
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let resolve_view = resolve_tex.create_view(&wgpu::TextureViewDescriptor::default());
        let bg = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("blit bind group"),
            layout: &self.blit_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&resolve_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.blit_sampler),
                },
            ],
        });

        // Snapshot texture: a COPY_DST target holding a copy of the persistent
        // texture, sampled by effect shaders as the backdrop (dst).
        let snapshot_tex = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("effect snapshot"),
            size: wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: fmt,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let snapshot_view = snapshot_tex.create_view(&wgpu::TextureViewDescriptor::default());
        let snapshot_bg = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("effect snapshot bind group"),
            layout: &self.blend_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&snapshot_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.blend_sampler),
                },
            ],
        });

        // Effect ping-pong textures for separable Gaussian blur (backdrop).
        let effect_desc = wgpu::TextureDescriptor {
            label: Some("effect blur"),
            size: wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: fmt,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        };
        let effect_h_tex = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("effect blur h"),
            ..effect_desc.clone()
        });
        let effect_h_view = effect_h_tex.create_view(&wgpu::TextureViewDescriptor::default());
        let effect_v_tex = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("effect blur v"),
            ..effect_desc
        });
        let effect_v_view = effect_v_tex.create_view(&wgpu::TextureViewDescriptor::default());
        // Bind group for the composite pass: samples the final (vertical) blur result.
        let effect_v_bg = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("effect blur v bind group"),
            layout: &self.blend_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&effect_v_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.blend_sampler),
                },
            ],
        });

        self.msaa_tex = Some(msaa_tex);
        self.msaa_view = Some(msaa_view);
        self.persistent_tex = Some(resolve_tex);
        self.persistent_view = Some(resolve_view);
        self.persistent_bind_group = Some(bg);
        self.snapshot_tex = Some(snapshot_tex);
        self.snapshot_view = Some(snapshot_view);
        self.snapshot_bind_group = Some(snapshot_bg);
        self.effect_h_tex = Some(effect_h_tex);
        self.effect_h_view = Some(effect_h_view);
        self.effect_v_tex = Some(effect_v_tex);
        self.effect_v_view = Some(effect_v_view);
        self.effect_v_bind_group = Some(effect_v_bg);
        // Blur uniforms/bind groups reference the OLD views — rebuild lazily (B5).
        self.blur_h_res = None;
        self.blur_v_res = None;
        self.persistent_dirty = true;
    }

    pub fn draw_commands(
        &mut self,
        frame: &mut Frame,
        commands: &[DrawCommand],
        text_areas: &[TextAreaDesc],
        backdrop_regions: &[crate::render::BackdropRegion],
        _hint: Size,
    ) {
        self.ensure_persistent_texture();

        let (cfg_w, cfg_h, cfg_fmt) = {
            let Some(cfg) = self.config.as_ref() else {
                crate::core::error::push_error(crate::core::error::UiError::GpuRender(
                    "draw_commands: no active surface".into(),
                ));
                return;
            };
            (cfg.width, cfg.height, cfg.format)
        };

        let screen = Size::new(
            self.screen_size.width / self.scale_factor,
            self.screen_size.height / self.scale_factor,
        );

        // ── Build per-z-layer vertex data ──
        // Commands and text areas are grouped by z_index so that each
        // layer's surfaces can be drawn before its text, and higher-z
        // layers naturally occlude lower-z layers (no z-culling needed).

        // Sort text_areas by z_index so we can iterate with a cursor
        // inside the z-layer loop instead of allocating a per-frame
        // HashMap<i32, Vec<TextAreaDesc>> (audit 2026-07-18).
        // `commands` is already sorted by z_index so input order is
        // preserved for the caller; we sort a local clone for per-layer
        // consumption.
        let mut text_by_z: Vec<TextAreaDesc> = text_areas.to_vec();
        text_by_z.sort_unstable_by_key(|t| t.z_index);

        self.image_cache_frame += 1;
        let max_images: usize = 64;

        // Helper: compute padded bytes_per_row aligned to 256 (WebGPU requirement)
        let padded_bpr = |width: u32| -> u32 {
            let bpr = width * 4;
            (bpr + 255) & !255
        };

        // Phase 1: Image upload (with lazy mip chain)
        for cmd in commands {
            if let DrawCommand::DrawImage { hash, .. } = cmd {
                if !self.image_cache.contains_key(hash) {
                    if self.image_cache.len() >= max_images {
                        let oldest_hash = match self.image_cache.iter().min_by_key(|(_, v)| v.5) {
                            Some((&h, _)) => h,
                            None => continue,
                        };
                        self.image_cache.remove(&oldest_hash);
                    }
                    if let Some(mips) = lookup_image_mips(*hash) {
                        let mip_count = mips.levels.len() as u32;
                        let tex_size = wgpu::Extent3d {
                            width: mips.width,
                            height: mips.height,
                            depth_or_array_layers: 1,
                        };
                        let tex = self.device.create_texture(&wgpu::TextureDescriptor {
                            label: Some("image"),
                            size: tex_size,
                            mip_level_count: mip_count,
                            sample_count: 1,
                            dimension: wgpu::TextureDimension::D2,
                            format: wgpu::TextureFormat::Rgba8UnormSrgb,
                            usage: wgpu::TextureUsages::TEXTURE_BINDING
                                | wgpu::TextureUsages::COPY_DST,
                            view_formats: &[],
                        });
                        for (i, level_data) in mips.levels.iter().enumerate() {
                            let lw = (mips.width >> i).max(1);
                            let lh = (mips.height >> i).max(1);
                            let src_bpr = lw * 4;
                            let dst_bpr = padded_bpr(lw);
                            if src_bpr == dst_bpr {
                                self.queue.write_texture(
                                    wgpu::TexelCopyTextureInfo {
                                        texture: &tex,
                                        mip_level: i as u32,
                                        origin: wgpu::Origin3d::ZERO,
                                        aspect: wgpu::TextureAspect::All,
                                    },
                                    level_data,
                                    wgpu::TexelCopyBufferLayout {
                                        offset: 0,
                                        bytes_per_row: Some(src_bpr),
                                        rows_per_image: Some(lh),
                                    },
                                    wgpu::Extent3d {
                                        width: lw,
                                        height: lh,
                                        depth_or_array_layers: 1,
                                    },
                                );
                            } else {
                                let mut padded = Vec::with_capacity((dst_bpr * lh) as usize);
                                for row in 0..lh {
                                    let start = (row * src_bpr) as usize;
                                    padded.extend_from_slice(
                                        &level_data[start..start + src_bpr as usize],
                                    );
                                    padded.resize(padded.len() + (dst_bpr - src_bpr) as usize, 0);
                                }
                                self.queue.write_texture(
                                    wgpu::TexelCopyTextureInfo {
                                        texture: &tex,
                                        mip_level: i as u32,
                                        origin: wgpu::Origin3d::ZERO,
                                        aspect: wgpu::TextureAspect::All,
                                    },
                                    &padded,
                                    wgpu::TexelCopyBufferLayout {
                                        offset: 0,
                                        bytes_per_row: Some(dst_bpr),
                                        rows_per_image: Some(lh),
                                    },
                                    wgpu::Extent3d {
                                        width: lw,
                                        height: lh,
                                        depth_or_array_layers: 1,
                                    },
                                );
                            }
                        }
                        let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
                        let bg = create_atlas_bind_group(
                            &self.device,
                            &self.text_bind_group_layout,
                            &view,
                            Some(&self.image_sampler),
                        );
                        self.image_cache.insert(
                            *hash,
                            (
                                tex,
                                view,
                                bg,
                                mips.width as f32,
                                mips.height as f32,
                                self.image_cache_frame,
                            ),
                        );
                    }
                }
            }
        }

        // ── Pass 1: Batch collect + single render pass ──
        let mut accessed_image_hashes: Vec<u64> = Vec::new();
        {
            let mut rect_verts: Vec<RectVertex> = Vec::with_capacity(commands.len() * 4);
            let mut rect_indices: Vec<u32> = Vec::with_capacity(commands.len() * 6);
            // Effect rects (blend_mode != 0): collected separately so they can be
            // drawn in their own render pass, inserted at the correct z-boundary
            // between segments. P1 draws them as normal rects (proves segmentation
            // + z-order); P2 swaps in the blend pipeline sampling a backdrop snapshot.
            let mut effect_verts: Vec<RectVertex> = Vec::new();
            let mut effect_indices: Vec<u32> = Vec::new();
            struct EffectOp {
                z: i32,
                i_start: u32,
                i_count: u32,
                #[allow(dead_code)]
                blend_mode: u8,
            }
            let mut effect_ops: Vec<EffectOp> = Vec::new();
            // Backdrop-blur regions become composite quads (RectVertex) drawn after
            // a per-region separable Gaussian blur of the backdrop snapshot.
            let mut backdrop_verts: Vec<RectVertex> = Vec::new();
            let mut backdrop_indices: Vec<u32> = Vec::new();
            struct BackdropOp {
                z: i32,
                i_start: u32,
                i_count: u32,
                blur_radius: f32,
            }
            let mut backdrop_ops: Vec<BackdropOp> = Vec::new();
            let mut grad_verts: Vec<GradientVertex> = Vec::with_capacity(16);
            let mut grad_indices: Vec<u32> = Vec::with_capacity(24);
            let mut image_verts: Vec<TextVertex> = Vec::with_capacity(8);
            let mut image_indices: Vec<u32> = Vec::with_capacity(12);
            let mut image_bind_groups: Vec<&wgpu::BindGroup> = Vec::new();
            let mut path_verts: Vec<PathVertex> = Vec::with_capacity(256);
            let mut path_indices: Vec<u32> = Vec::with_capacity(512);

            let mut cmd_indices: Vec<usize> = (0..commands.len()).collect();
            cmd_indices.sort_by_key(|&i| commands[i].z_index());
            let mut cur_z: Option<i32> = None;
            let (
                mut rv0,
                mut ris0,
                mut gv0,
                mut gis0,
                mut iv0,
                mut iis0,
                mut ibo0,
                mut pv0,
                mut pis0,
            ) = (0u32, 0u32, 0u32, 0u32, 0u32, 0u32, 0u32, 0u32, 0u32);
            #[allow(dead_code)] // per-primitive counters; only the vertex/index starts are consumed today
            struct ZRange {
                z: i32,
                rv: u32,
                rc: u32,
                ris: u32,
                ric: u32,
                gv: u32,
                gc: u32,
                gis: u32,
                gic: u32,
                iv: u32,
                ic: u32,
                iis: u32,
                iic: u32,
                ibo: u32,
                pv: u32,
                pc: u32,
                pis: u32,
                pic: u32,
            }
            let mut z_layers: Vec<ZRange> = Vec::new();
            for &i in &cmd_indices {
                let cmd = &commands[i];
                let z = cmd.z_index();
                if Some(z) != cur_z {
                    if let Some(pz) = cur_z {
                        z_layers.push(ZRange {
                            z: pz,
                            rv: rv0,
                            rc: rect_verts.len() as u32 - rv0,
                            ris: ris0,
                            ric: rect_indices.len() as u32 - ris0,
                            gv: gv0,
                            gc: grad_verts.len() as u32 - gv0,
                            gis: gis0,
                            gic: grad_indices.len() as u32 - gis0,
                            iv: iv0,
                            ic: image_verts.len() as u32 - iv0,
                            iis: iis0,
                            iic: image_indices.len() as u32 - iis0,
                            ibo: ibo0,
                            pv: pv0,
                            pc: path_verts.len() as u32 - pv0,
                            pis: pis0,
                            pic: path_indices.len() as u32 - pis0,
                        });
                    }
                    rv0 = rect_verts.len() as u32;
                    ris0 = rect_indices.len() as u32;
                    gv0 = grad_verts.len() as u32;
                    gis0 = grad_indices.len() as u32;
                    iv0 = image_verts.len() as u32;
                    iis0 = image_indices.len() as u32;
                    ibo0 = image_bind_groups.len() as u32;
                    pv0 = path_verts.len() as u32;
                    pis0 = path_indices.len() as u32;
                    cur_z = Some(z);
                }
                let clip = match cmd {
                    DrawCommand::FillRect { clip, .. } => clip,
                    DrawCommand::StrokeRect { clip, .. } => clip,
                    DrawCommand::FillShadow { clip, .. } => clip,
                    DrawCommand::FillLinearGradient { clip, .. } => clip,
                    DrawCommand::DrawImage { clip, .. } => clip,
                    DrawCommand::FillPath { clip, .. } => clip,
                    DrawCommand::StrokePath { clip, .. } => clip,
                };
                let xform = match cmd {
                    DrawCommand::FillRect { transform, .. } => transform,
                    DrawCommand::StrokeRect { transform, .. } => transform,
                    DrawCommand::FillShadow { transform, .. } => transform,
                    DrawCommand::FillLinearGradient { transform, .. } => transform,
                    DrawCommand::DrawImage { transform, .. } => transform,
                    DrawCommand::FillPath { transform, .. } => transform,
                    DrawCommand::StrokePath { transform, .. } => transform,
                };
                let (_sx, _sy, sw, sh) = clip_to_scissor(*clip, screen, self.scale_factor);
                if sw == 0 || sh == 0 {
                    continue;
                }

                match cmd {
                    DrawCommand::FillRect {
                        rect,
                        color,
                        radius,
                        clip,
                        blend_mode,
                        ..
                    } => {
                        if *blend_mode == 0 {
                            let base = rect_verts.len() as u32;
                            let v = rect_to_vertices(
                                *rect, screen, *color, *radius, 0.0, *xform, *clip, [0.0; 2],
                                [0.0; 4], 0.0, 0.0,
                            );
                            rect_verts.extend_from_slice(&v);
                            rect_indices.extend_from_slice(&[
                                base,
                                base + 1,
                                base + 2,
                                base,
                                base + 2,
                                base + 3,
                            ]);
                        } else {
                            let base = effect_verts.len() as u32;
                            let i_start = effect_indices.len() as u32;
                            let mut v = rect_to_vertices(
                                *rect, screen, *color, *radius, 0.0, *xform, *clip, [0.0; 2],
                                [0.0; 4], 0.0, 0.0,
                            );
                            // Bake blend mode into is_shadow (see BLEND_SHADER field
                            // repurposing). Snapshot UV is derived from NDC in the shader.
                            for vert in v.iter_mut() {
                                vert.is_shadow = *blend_mode as f32;
                            }
                            effect_verts.extend_from_slice(&v);
                            effect_indices.extend_from_slice(&[
                                base,
                                base + 1,
                                base + 2,
                                base,
                                base + 2,
                                base + 3,
                            ]);
                            effect_ops.push(EffectOp {
                                z,
                                i_start,
                                i_count: 6,
                                blend_mode: *blend_mode,
                            });
                        }
                    }
                    DrawCommand::StrokeRect {
                        rect,
                        color,
                        width,
                        radius,
                        clip,
                        blend_mode,
                        ..
                    } => {
                        if *blend_mode == 0 {
                            let base = rect_verts.len() as u32;
                            let v = rect_to_vertices(
                                *rect, screen, *color, *radius, *width, *xform, *clip, [0.0; 2],
                                [0.0; 4], 0.0, 0.0,
                            );
                            rect_verts.extend_from_slice(&v);
                            rect_indices.extend_from_slice(&[
                                base,
                                base + 1,
                                base + 2,
                                base,
                                base + 2,
                                base + 3,
                            ]);
                        } else {
                            let base = effect_verts.len() as u32;
                            let i_start = effect_indices.len() as u32;
                            let mut v = rect_to_vertices(
                                *rect, screen, *color, *radius, *width, *xform, *clip, [0.0; 2],
                                [0.0; 4], 0.0, 0.0,
                            );
                            for vert in v.iter_mut() {
                                vert.is_shadow = *blend_mode as f32;
                            }
                            effect_verts.extend_from_slice(&v);
                            effect_indices.extend_from_slice(&[
                                base,
                                base + 1,
                                base + 2,
                                base,
                                base + 2,
                                base + 3,
                            ]);
                            effect_ops.push(EffectOp {
                                z,
                                i_start,
                                i_count: 6,
                                blend_mode: *blend_mode,
                            });
                        }
                    }
                    DrawCommand::FillShadow {
                        rect,
                        color,
                        radius,
                        shadow,
                        elem_size: _,
                        clip,
                        ..
                    } => {
                        let base = rect_verts.len() as u32;
                        let center: [f32; 2] =
                            [rect.x + rect.width * 0.5, rect.y + rect.height * 0.5];
                        let sc = shadow.color.to_linear_array();
                        let v = rect_to_vertices(
                            *rect,
                            screen,
                            *color,
                            *radius,
                            0.0,
                            *xform,
                            *clip,
                            center,
                            sc,
                            shadow.blur,
                            1.0,
                        );
                        rect_verts.extend_from_slice(&v);
                        rect_indices.extend_from_slice(&[
                            base,
                            base + 1,
                            base + 2,
                            base,
                            base + 2,
                            base + 3,
                        ]);
                    }
                    DrawCommand::FillLinearGradient {
                        rect,
                        gradient,
                        radius,
                        stroke_width,
                        clip,
                        ..
                    } => {
                        let base = grad_verts.len() as u32;
                        let v = gradient_to_vertices(
                            *rect,
                            screen,
                            *gradient,
                            *radius,
                            *stroke_width,
                            *xform,
                            *clip,
                        );
                        grad_verts.extend_from_slice(&v);
                        grad_indices.extend_from_slice(&[
                            base,
                            base + 1,
                            base + 2,
                            base,
                            base + 2,
                            base + 3,
                        ]);
                    }
                    DrawCommand::DrawImage {
                        hash,
                        rect,
                        opacity,
                        content_fit,
                        clip,
                        ..
                    } => {
                        if *opacity <= 0.0 {
                            continue;
                        }
                        accessed_image_hashes.push(*hash);
                        if let Some(&(_, _, ref bg, iw, ih, _)) = self.image_cache.get(hash) {
                            let render_rect = content_fit_rect(*content_fit, *rect, iw, ih);
                            let tp = xform
                                .transform_point2(glam::Vec2::new(render_rect.x, render_rect.y));
                            let base = image_verts.len() as u32;
                            let image_clip_rect: [f32; 4] = [
                                clip.rect.x,
                                clip.rect.y,
                                clip.rect.x + clip.rect.width,
                                clip.rect.y + clip.rect.height,
                            ];
                            let image_clip_radius: [f32; 4] = [
                                clip.radius.top_left,
                                clip.radius.top_right,
                                clip.radius.bottom_right,
                                clip.radius.bottom_left,
                            ];
                            let v = image_quad_vertices(
                                tp.x,
                                tp.y,
                                render_rect.width,
                                render_rect.height,
                                screen,
                                image_clip_rect,
                                image_clip_radius,
                                *opacity,
                            );
                            image_verts.extend_from_slice(&v);
                            image_indices.extend_from_slice(&[
                                base,
                                base + 1,
                                base + 2,
                                base,
                                base + 2,
                                base + 3,
                            ]);
                            while image_bind_groups.len() < (image_verts.len() / 4) {
                                image_bind_groups.push(bg);
                            }
                        }
                    }
                    DrawCommand::FillPath {
                        path,
                        brush,
                        clip,
                        transform,
                        ..
                    } => {
                        let ph = hash_bezpath(path);
                        let bh = hash_brush(brush);
                        let xh = hash_linear_transform(*transform);
                        let key = PathCacheKey {
                            path_hash: ph,
                            brush_hash: bh,
                            stroke_hash: 0,
                            xform_hash: xh,
                        };

                        let ka = glam_to_kurbo_affine(*transform);
                        let (tx, ty) = (transform.translation.x, transform.translation.y);

                        let (gs, ge) = match brush {
                            Brush::Gradient(g) => {
                                let bbox = match bezpath_bounds(path) {
                                    Some(b) => b,
                                    None => continue,
                                };
                                let sx = (bbox.x + g.start.0 * bbox.width) as f64;
                                let sy = (bbox.y + g.start.1 * bbox.height) as f64;
                                let ex = (bbox.x + g.end.0 * bbox.width) as f64;
                                let ey = (bbox.y + g.end.1 * bbox.height) as f64;
                                let sp = ka * kurbo::Point::new(sx, sy);
                                let ep = ka * kurbo::Point::new(ex, ey);
                                ((sp.x, sp.y), (ep.x, ep.y))
                            }
                            Brush::Solid(_) => ((0.0, 0.0), (0.0, 0.0)),
                        };

                        let sx_calc = 2.0 / screen.width;
                        let sy_calc = -2.0 / screen.height;

                        if !self.path_cache.contains_key(&key) {
                            let mut linear = *transform;
                            linear.translation = glam::Vec2::ZERO;
                            let transformed = glam_to_kurbo_affine(linear) * (**path).clone();
                            let lyon_p = bezpath_to_lyon(&transformed);

                            let mut buffers = VertexBuffers::<[f32; 2], u32>::new();
                            let options = FillOptions::default();
                            if FillTessellator::new()
                                .tessellate_path(
                                    &lyon_p,
                                    &options,
                                    &mut BuffersBuilder::new(&mut buffers, |v: FillVertex| {
                                        let pos = v.position();
                                        [pos.x, pos.y]
                                    }),
                                )
                                .is_ok()
                            {
                                self.path_cache.insert(
                                    key,
                                    CachedPathMesh {
                                        logical_vertices: buffers.vertices,
                                        indices: buffers.indices,
                                        last_frame: self.path_cache_frame,
                                    },
                                );
                            }
                        }
                        if let Some(cached) = self.path_cache.get_mut(&key) {
                            cached.last_frame = self.path_cache_frame;
                            let base = path_verts.len() as u32;
                            let cr = clip.rect;
                            let clip_r = [cr.x, cr.y, cr.x + cr.width, cr.y + cr.height];
                            let clip_rad = [
                                clip.radius.top_left,
                                clip.radius.top_right,
                                clip.radius.bottom_right,
                                clip.radius.bottom_left,
                            ];
                            for &v in &cached.logical_vertices {
                                let px = v[0] + tx;
                                let py = v[1] + ty;
                                let ndc_x = px * sx_calc - 1.0;
                                let ndc_y = py * sy_calc + 1.0;
                                let color = brush_color_at_pos(brush, px as f64, py as f64, gs, ge);
                                path_verts.push(PathVertex {
                                    position: [ndc_x, ndc_y],
                                    color,
                                    world_pos: [px, py],
                                    clip_rect: clip_r,
                                    clip_radius: clip_rad,
                                });
                            }
                            for &i in &cached.indices {
                                path_indices.push(base + i);
                            }
                        }
                    }
                    DrawCommand::StrokePath {
                        path,
                        stroke,
                        brush,
                        clip,
                        transform,
                        ..
                    } => {
                        let ph = hash_bezpath(path);
                        let bh = hash_brush(brush);
                        let sh = hash_stroke(stroke);
                        // Translation-invariant key (B1) — same as FillPath.
                        let xh = hash_linear_transform(*transform);
                        let key = PathCacheKey {
                            path_hash: ph,
                            brush_hash: bh,
                            stroke_hash: sh,
                            xform_hash: xh,
                        };

                        let ka = glam_to_kurbo_affine(*transform);
                        let (tx, ty) = (transform.translation.x, transform.translation.y);

                        let (gs, ge) = match brush {
                            Brush::Gradient(g) => {
                                let bbox = match bezpath_bounds(path) {
                                    Some(b) => b,
                                    None => continue,
                                };
                                let sx = (bbox.x + g.start.0 * bbox.width) as f64;
                                let sy = (bbox.y + g.start.1 * bbox.height) as f64;
                                let ex = (bbox.x + g.end.0 * bbox.width) as f64;
                                let ey = (bbox.y + g.end.1 * bbox.height) as f64;
                                let sp = ka * kurbo::Point::new(sx, sy);
                                let ep = ka * kurbo::Point::new(ex, ey);
                                ((sp.x, sp.y), (ep.x, ep.y))
                            }
                            Brush::Solid(_) => ((0.0, 0.0), (0.0, 0.0)),
                        };

                        let sx_calc = 2.0 / screen.width;
                        let sy_calc = -2.0 / screen.height;

                        if !self.path_cache.contains_key(&key) {
                            let mut linear = *transform;
                            linear.translation = glam::Vec2::ZERO;
                            let transformed = glam_to_kurbo_affine(linear) * (**path).clone();
                            let lyon_p = bezpath_to_lyon(&transformed);

                            let mut buffers = VertexBuffers::<[f32; 2], u32>::new();
                            let line_width = stroke.width as f32;
                            let options = StrokeOptions::default()
                                .with_line_width(line_width.max(0.5))
                                .with_line_cap(match stroke.start_cap {
                                    kurbo::Cap::Butt => lyon::tessellation::LineCap::Butt,
                                    kurbo::Cap::Round => lyon::tessellation::LineCap::Round,
                                    kurbo::Cap::Square => lyon::tessellation::LineCap::Square,
                                })
                                .with_line_join(match stroke.join {
                                    kurbo::Join::Bevel => lyon::tessellation::LineJoin::Bevel,
                                    kurbo::Join::Round => lyon::tessellation::LineJoin::Round,
                                    kurbo::Join::Miter => lyon::tessellation::LineJoin::Miter,
                                });
                            if StrokeTessellator::new()
                                .tessellate_path(
                                    &lyon_p,
                                    &options,
                                    &mut BuffersBuilder::new(
                                        &mut buffers,
                                        |v: lyon::tessellation::StrokeVertex| {
                                            let pos = v.position();
                                            [pos.x, pos.y]
                                        },
                                    ),
                                )
                                .is_ok()
                            {
                                self.path_cache.insert(
                                    key,
                                    CachedPathMesh {
                                        logical_vertices: buffers.vertices,
                                        indices: buffers.indices,
                                        last_frame: self.path_cache_frame,
                                    },
                                );
                            }
                        }
                        if let Some(cached) = self.path_cache.get_mut(&key) {
                            cached.last_frame = self.path_cache_frame;
                            let base = path_verts.len() as u32;
                            let cr = clip.rect;
                            let clip_r = [cr.x, cr.y, cr.x + cr.width, cr.y + cr.height];
                            let clip_rad = [
                                clip.radius.top_left,
                                clip.radius.top_right,
                                clip.radius.bottom_right,
                                clip.radius.bottom_left,
                            ];
                            for &v in &cached.logical_vertices {
                                let px = v[0] + tx;
                                let py = v[1] + ty;
                                let ndc_x = px * sx_calc - 1.0;
                                let ndc_y = py * sy_calc + 1.0;
                                let color = brush_color_at_pos(brush, px as f64, py as f64, gs, ge);
                                path_verts.push(PathVertex {
                                    position: [ndc_x, ndc_y],
                                    color,
                                    world_pos: [px, py],
                                    clip_rect: clip_r,
                                    clip_radius: clip_rad,
                                });
                            }
                            for &i in &cached.indices {
                                path_indices.push(base + i);
                            }
                        }
                    }
                }
            }

            if let Some(z) = cur_z {
                z_layers.push(ZRange {
                    z,
                    rv: rv0,
                    rc: rect_verts.len() as u32 - rv0,
                    ris: ris0,
                    ric: rect_indices.len() as u32 - ris0,
                    gv: gv0,
                    gc: grad_verts.len() as u32 - gv0,
                    gis: gis0,
                    gic: grad_indices.len() as u32 - gis0,
                    iv: iv0,
                    ic: image_verts.len() as u32 - iv0,
                    iis: iis0,
                    iic: image_indices.len() as u32 - iis0,
                    ibo: ibo0,
                    pv: pv0,
                    pc: path_verts.len() as u32 - pv0,
                    pis: pis0,
                    pic: path_indices.len() as u32 - pis0,
                });
            }

            // ── Backdrop-blur composite quads ──
            // Each region → a rounded RectVertex quad (tint baked into color, tint
            // strength in color.a). Drawn after a per-region separable Gaussian blur
            // of the backdrop snapshot; sampled in the composite shader via NDC UV.
            for br in backdrop_regions {
                let i_start = backdrop_indices.len() as u32;
                let base = backdrop_verts.len() as u32;
                let (tint_rgb, tint_a) = match br.tint {
                    Some(c) => ([c.r, c.g, c.b], c.a),
                    None => ([0.0, 0.0, 0.0], 0.0),
                };
                let tint = crate::style::Color {
                    r: tint_rgb[0],
                    g: tint_rgb[1],
                    b: tint_rgb[2],
                    a: tint_a,
                };
                let mut v = rect_to_vertices(
                    br.rect,
                    screen,
                    tint,
                    br.corner_radius,
                    0.0,
                    br.transform,
                    br.rect.into(),
                    [0.0; 2],
                    [0.0; 4],
                    0.0,
                    0.0,
                );
                for vert in v.iter_mut() {
                    vert.is_shadow = 0.0;
                }
                backdrop_verts.extend_from_slice(&v);
                backdrop_indices.extend_from_slice(&[
                    base,
                    base + 1,
                    base + 2,
                    base,
                    base + 2,
                    base + 3,
                ]);
                backdrop_ops.push(BackdropOp {
                    z: br.z_index,
                    i_start,
                    i_count: 6,
                    blur_radius: br.blur_radius,
                });
            }

            // ── Belt uploads: encode copy_buffer_to_buffer before render pass ──
            let rect_vbuf: Option<&wgpu::Buffer> = if !rect_verts.is_empty() {
                let data = bytemuck::cast_slice(&rect_verts);
                let vbuf = Self::ensure_buffer(
                    &mut self.rect_vbuf,
                    data.len() as u64,
                    wgpu::BufferUsages::VERTEX,
                    &self.device,
                );
                self.belt
                    .write_buffer(
                        &mut frame.encoder,
                        vbuf,
                        0,
                        wgpu::BufferSize::new(data.len() as u64).unwrap(),
                    )
                    .copy_from_slice(data);
                let idata = bytemuck::cast_slice(&rect_indices);
                let ibuf = Self::ensure_buffer(
                    &mut self.rect_ibuf,
                    idata.len() as u64,
                    wgpu::BufferUsages::INDEX,
                    &self.device,
                );
                self.belt
                    .write_buffer(
                        &mut frame.encoder,
                        ibuf,
                        0,
                        wgpu::BufferSize::new(idata.len() as u64).unwrap(),
                    )
                    .copy_from_slice(idata);
                Some(vbuf)
            } else {
                None
            };
            let rect_ibuf_ref: Option<&wgpu::Buffer> = if !rect_verts.is_empty() {
                self.rect_ibuf.as_ref().map(|(b, _)| b)
            } else {
                None
            };

            let grad_vbuf_ref: Option<&wgpu::Buffer> = if !grad_verts.is_empty() {
                let data = bytemuck::cast_slice(&grad_verts);
                let vbuf = Self::ensure_buffer(
                    &mut self.grad_vbuf,
                    data.len() as u64,
                    wgpu::BufferUsages::VERTEX,
                    &self.device,
                );
                self.belt
                    .write_buffer(
                        &mut frame.encoder,
                        vbuf,
                        0,
                        wgpu::BufferSize::new(data.len() as u64).unwrap(),
                    )
                    .copy_from_slice(data);
                let idata = bytemuck::cast_slice(&grad_indices);
                let ibuf = Self::ensure_buffer(
                    &mut self.grad_ibuf,
                    idata.len() as u64,
                    wgpu::BufferUsages::INDEX,
                    &self.device,
                );
                self.belt
                    .write_buffer(
                        &mut frame.encoder,
                        ibuf,
                        0,
                        wgpu::BufferSize::new(idata.len() as u64).unwrap(),
                    )
                    .copy_from_slice(idata);
                Some(vbuf)
            } else {
                None
            };
            let grad_ibuf_ref: Option<&wgpu::Buffer> = if !grad_verts.is_empty() {
                self.grad_ibuf.as_ref().map(|(b, _)| b)
            } else {
                None
            };

            let image_vbuf_ref: Option<&wgpu::Buffer> = if !image_verts.is_empty() {
                let data = bytemuck::cast_slice(&image_verts);
                let vbuf = Self::ensure_buffer(
                    &mut self.image_vbuf,
                    data.len() as u64,
                    wgpu::BufferUsages::VERTEX,
                    &self.device,
                );
                self.belt
                    .write_buffer(
                        &mut frame.encoder,
                        vbuf,
                        0,
                        wgpu::BufferSize::new(data.len() as u64).unwrap(),
                    )
                    .copy_from_slice(data);
                let idata = bytemuck::cast_slice(&image_indices);
                let ibuf = Self::ensure_buffer(
                    &mut self.image_ibuf,
                    idata.len() as u64,
                    wgpu::BufferUsages::INDEX,
                    &self.device,
                );
                self.belt
                    .write_buffer(
                        &mut frame.encoder,
                        ibuf,
                        0,
                        wgpu::BufferSize::new(idata.len() as u64).unwrap(),
                    )
                    .copy_from_slice(idata);
                Some(vbuf)
            } else {
                None
            };
            let image_ibuf_ref: Option<&wgpu::Buffer> = if !image_verts.is_empty() {
                self.image_ibuf.as_ref().map(|(b, _)| b)
            } else {
                None
            };

            let path_vbuf_ref: Option<&wgpu::Buffer> = if !path_verts.is_empty() {
                let pvdata = bytemuck::cast_slice(&path_verts);
                let pvbuf = Self::ensure_buffer(
                    &mut self.path_vbuf,
                    pvdata.len() as u64,
                    wgpu::BufferUsages::VERTEX,
                    &self.device,
                );
                self.belt
                    .write_buffer(
                        &mut frame.encoder,
                        pvbuf,
                        0,
                        wgpu::BufferSize::new(pvdata.len() as u64).unwrap(),
                    )
                    .copy_from_slice(pvdata);
                let pidata = bytemuck::cast_slice(&path_indices);
                let pibuf = Self::ensure_buffer(
                    &mut self.path_ibuf,
                    pidata.len() as u64,
                    wgpu::BufferUsages::INDEX,
                    &self.device,
                );
                self.belt
                    .write_buffer(
                        &mut frame.encoder,
                        pibuf,
                        0,
                        wgpu::BufferSize::new(pidata.len() as u64).unwrap(),
                    )
                    .copy_from_slice(pidata);
                Some(pvbuf)
            } else {
                None
            };
            let path_ibuf_ref: Option<&wgpu::Buffer> = if !path_verts.is_empty() {
                self.path_ibuf.as_ref().map(|(b, _)| b)
            } else {
                None
            };

            let effect_vbuf_ref: Option<&wgpu::Buffer> = if !effect_verts.is_empty() {
                let data = bytemuck::cast_slice(&effect_verts);
                let vbuf = Self::ensure_buffer(
                    &mut self.effect_vbuf,
                    data.len() as u64,
                    wgpu::BufferUsages::VERTEX,
                    &self.device,
                );
                self.belt
                    .write_buffer(
                        &mut frame.encoder,
                        vbuf,
                        0,
                        wgpu::BufferSize::new(data.len() as u64).unwrap(),
                    )
                    .copy_from_slice(data);
                let idata = bytemuck::cast_slice(&effect_indices);
                let ibuf = Self::ensure_buffer(
                    &mut self.effect_ibuf,
                    idata.len() as u64,
                    wgpu::BufferUsages::INDEX,
                    &self.device,
                );
                self.belt
                    .write_buffer(
                        &mut frame.encoder,
                        ibuf,
                        0,
                        wgpu::BufferSize::new(idata.len() as u64).unwrap(),
                    )
                    .copy_from_slice(idata);
                Some(vbuf)
            } else {
                None
            };
            let effect_ibuf_ref: Option<&wgpu::Buffer> = if !effect_verts.is_empty() {
                self.effect_ibuf.as_ref().map(|(b, _)| b)
            } else {
                None
            };

            let backdrop_vbuf_ref: Option<&wgpu::Buffer> = if !backdrop_verts.is_empty() {
                let data = bytemuck::cast_slice(&backdrop_verts);
                let vbuf = Self::ensure_buffer(
                    &mut self.backdrop_vbuf,
                    data.len() as u64,
                    wgpu::BufferUsages::VERTEX,
                    &self.device,
                );
                self.belt
                    .write_buffer(
                        &mut frame.encoder,
                        vbuf,
                        0,
                        wgpu::BufferSize::new(data.len() as u64).unwrap(),
                    )
                    .copy_from_slice(data);
                let idata = bytemuck::cast_slice(&backdrop_indices);
                let ibuf = Self::ensure_buffer(
                    &mut self.backdrop_ibuf,
                    idata.len() as u64,
                    wgpu::BufferUsages::INDEX,
                    &self.device,
                );
                self.belt
                    .write_buffer(
                        &mut frame.encoder,
                        ibuf,
                        0,
                        wgpu::BufferSize::new(idata.len() as u64).unwrap(),
                    )
                    .copy_from_slice(idata);
                Some(vbuf)
            } else {
                None
            };
            let backdrop_ibuf_ref: Option<&wgpu::Buffer> = if !backdrop_verts.is_empty() {
                self.backdrop_ibuf.as_ref().map(|(b, _)| b)
            } else {
                None
            };

            // ── Z-layered persistent render pass(es) ──
            // Segmented rendering: when blend/backdrop effects are present, the
            // single pass is split at effect z-boundaries so each effect can be
            // composited in its own pass (P2/P3 sample a backdrop snapshot of the
            // persistent texture). MSAA is preserved across segments via
            // LoadOp::Load + StoreOp::Store. With NO effects this reduces to exactly
            // one pass with StoreOp::Discard — bit-identical to the pre-segmentation
            // path (zero regression on the common case).
            {
                let Some(pv) = self.persistent_view.as_ref() else {
                    push_error(UiError::GpuRender("missing persistent view".into()));
                    return;
                };
                let Some(mv) = self.msaa_view.as_ref() else {
                    push_error(UiError::GpuRender("missing msaa view".into()));
                    return;
                };
                // Effect compositing resources (only needed on the segmented path).
                let snapshot_bg = self.snapshot_bind_group.as_ref();
                let snapshot_tex = self.snapshot_tex.as_ref();
                let snapshot_view = self.snapshot_view.as_ref();
                let persistent_tex = self.persistent_tex.as_ref();
                let effect_h_view = self.effect_h_view.as_ref();
                let effect_v_view = self.effect_v_view.as_ref();
                let effect_v_bg = self.effect_v_bind_group.as_ref();
                if !rect_verts.is_empty()
                    || !grad_verts.is_empty()
                    || !image_verts.is_empty()
                    || !path_verts.is_empty()
                    || !text_areas.is_empty()
                    || !effect_verts.is_empty()
                    || !backdrop_verts.is_empty()
                {
                    self.persistent_dirty = true;
                }
                let has_effects = !effect_ops.is_empty() || !backdrop_ops.is_empty();
                let first_load = if self.persistent_dirty {
                    self.persistent_dirty = false;
                    let c = self.clear_color.to_linear();
                    wgpu::LoadOp::Clear(wgpu::Color {
                        r: c.r as f64,
                        g: c.g as f64,
                        b: c.b as f64,
                        a: c.a as f64,
                    })
                } else {
                    wgpu::LoadOp::Load
                };
                let scr_w = self.screen_size.width as u32;
                let scr_h = self.screen_size.height as u32;

                // Draws one z-layer's surfaces (rect/grad/image/path/text) into the
                // given pass. Kept as a macro (not a method) so disjoint-field
                // borrows survive alongside the &mut self.glyphon text call.
                macro_rules! draw_surface_layer {
                    ($pass:expr, $layer:expr, $texts:expr) => {{
                        let layer = $layer;
                        let texts: &[TextAreaDesc] = $texts;
                        if layer.ric > 0 {
                            $pass.set_pipeline(&self.rect_pipeline);
                            $pass.set_vertex_buffer(0, rect_vbuf.unwrap().slice(..));
                            $pass.set_index_buffer(
                                rect_ibuf_ref.unwrap().slice(..),
                                wgpu::IndexFormat::Uint32,
                            );
                            $pass.set_scissor_rect(0, 0, scr_w, scr_h);
                            $pass.draw_indexed(layer.ris..layer.ris + layer.ric, 0, 0..1);
                        }
                        if let Some(vbuf) = grad_vbuf_ref {
                            if layer.gic > 0 {
                                $pass.set_pipeline(&self.gradient_pipeline);
                                $pass.set_vertex_buffer(0, vbuf.slice(..));
                                $pass.set_index_buffer(
                                    grad_ibuf_ref.unwrap().slice(..),
                                    wgpu::IndexFormat::Uint32,
                                );
                                $pass.set_scissor_rect(0, 0, scr_w, scr_h);
                                $pass.draw_indexed(layer.gis..layer.gis + layer.gic, 0, 0..1);
                            }
                        }
                        if let Some(vbuf) = image_vbuf_ref {
                            if layer.iic > 0 {
                                $pass.set_pipeline(&self.text_pipeline);
                                $pass.set_vertex_buffer(0, vbuf.slice(..));
                                $pass.set_index_buffer(
                                    image_ibuf_ref.unwrap().slice(..),
                                    wgpu::IndexFormat::Uint32,
                                );
                                let quads = layer.ic as usize / 4;
                                for q in 0..quads {
                                    let bg_idx = layer.ibo as usize + q;
                                    if bg_idx < image_bind_groups.len() {
                                        $pass.set_bind_group(
                                            0,
                                            Some(image_bind_groups[bg_idx]),
                                            &[],
                                        );
                                        let base = layer.iis + (q * 6) as u32;
                                        $pass.draw_indexed(base..base + 6, 0, 0..1);
                                    }
                                }
                            }
                        }
                        if let Some(vbuf) = path_vbuf_ref {
                            if layer.pic > 0 {
                                $pass.set_pipeline(&self.path_pipeline);
                                $pass.set_vertex_buffer(0, vbuf.slice(..));
                                $pass.set_index_buffer(
                                    path_ibuf_ref.unwrap().slice(..),
                                    wgpu::IndexFormat::Uint32,
                                );
                                $pass.set_scissor_rect(0, 0, scr_w, scr_h);
                                $pass.draw_indexed(layer.pis..layer.pis + layer.pic, 0, 0..1);
                            }
                        }
                        if !texts.is_empty() {
                            if layer.z == 0 {
                                if let Some(idx) =
                                    self.glyphon.prepare_layer(&self.device, &self.queue, texts)
                                {
                                    self.glyphon.render_layer(idx, &mut $pass);
                                }
                            } else {
                                let go = self.glyphon_overlay.get_or_insert_with(|| {
                                    GlyphonBridge::new(
                                        &self.device,
                                        &self.queue,
                                        cfg_fmt,
                                        self.screen_size.width as u32,
                                        self.screen_size.height as u32,
                                        wgpu::MultisampleState {
                                            count: self.sample_count,
                                            mask: !0,
                                            alpha_to_coverage_enabled: false,
                                        },
                                    )
                                });
                                if let Some(idx) =
                                    go.prepare_layer(&self.device, &self.queue, texts)
                                {
                                    go.render_layer(idx, &mut $pass);
                                }
                            }
                        }
                    }};
                }

                // Draws one effect (blend rect) into the given pass using the blend
                // pipeline, which samples the pre-copied backdrop snapshot bound at
                // group 0. Blend mode + snapshot UV are baked into the vertex data.
                // (P3 will extend this for backdrop blur.)
                macro_rules! draw_effect {
                    ($pass:expr, $op:expr, $snap_bg:expr) => {{
                        let op = $op;
                        if let (Some(vbuf), Some(ibuf)) = (effect_vbuf_ref, effect_ibuf_ref) {
                            $pass.set_pipeline(&self.blend_pipeline);
                            $pass.set_bind_group(0, $snap_bg, &[]);
                            $pass.set_vertex_buffer(0, vbuf.slice(..));
                            $pass.set_index_buffer(ibuf.slice(..), wgpu::IndexFormat::Uint32);
                            $pass.set_scissor_rect(0, 0, scr_w, scr_h);
                            $pass.draw_indexed(op.i_start..op.i_start + op.i_count, 0, 0..1);
                        }
                    }};
                }

                // Runs the two separable-Gaussian blur passes for one backdrop op:
                // snapshot → effect_h (horizontal) → effect_v (vertical). Assumes the
                // snapshot already holds the current backdrop. Leaves the result in
                // effect_v (sampled later by the composite pass via effect_v_bg).
                //
                // Uniform buffers + bind groups are persistent (B5) — created once,
                // re-written per region via queue.write_buffer. They reference the
                // persistent snapshot/effect views (invalidated only on resize).
                macro_rules! run_backdrop_blur {
                    ($radius:expr) => {{
                        let radius = ($radius).max(0.5);
                        let texel = [1.0 / cfg_w.max(1) as f32, 1.0 / cfg_h.max(1) as f32];
                        if let (Some(snap_view), Some(eh_view), Some(ev_view)) =
                            (snapshot_view, effect_h_view, effect_v_view)
                        {
                            if self.blur_h_res.is_none() {
                                let buf = self.device.create_buffer(&wgpu::BufferDescriptor {
                                    label: Some("blur uniform h"),
                                    size: std::mem::size_of::<BlurUniforms>() as u64,
                                    usage: wgpu::BufferUsages::UNIFORM
                                        | wgpu::BufferUsages::COPY_DST,
                                    mapped_at_creation: false,
                                });
                                let bg =
                                    self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                                        label: Some("blur bg h"),
                                        layout: &self.blur_bind_group_layout,
                                        entries: &[
                                            wgpu::BindGroupEntry {
                                                binding: 0,
                                                resource: wgpu::BindingResource::TextureView(
                                                    snap_view,
                                                ),
                                            },
                                            wgpu::BindGroupEntry {
                                                binding: 1,
                                                resource: wgpu::BindingResource::Sampler(
                                                    &self.blend_sampler,
                                                ),
                                            },
                                            wgpu::BindGroupEntry {
                                                binding: 2,
                                                resource: buf.as_entire_binding(),
                                            },
                                        ],
                                    });
                                self.blur_h_res = Some((buf, bg));
                            }
                            if self.blur_v_res.is_none() {
                                let buf = self.device.create_buffer(&wgpu::BufferDescriptor {
                                    label: Some("blur uniform v"),
                                    size: std::mem::size_of::<BlurUniforms>() as u64,
                                    usage: wgpu::BufferUsages::UNIFORM
                                        | wgpu::BufferUsages::COPY_DST,
                                    mapped_at_creation: false,
                                });
                                let bg =
                                    self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                                        label: Some("blur bg v"),
                                        layout: &self.blur_bind_group_layout,
                                        entries: &[
                                            wgpu::BindGroupEntry {
                                                binding: 0,
                                                resource: wgpu::BindingResource::TextureView(
                                                    eh_view,
                                                ),
                                            },
                                            wgpu::BindGroupEntry {
                                                binding: 1,
                                                resource: wgpu::BindingResource::Sampler(
                                                    &self.blend_sampler,
                                                ),
                                            },
                                            wgpu::BindGroupEntry {
                                                binding: 2,
                                                resource: buf.as_entire_binding(),
                                            },
                                        ],
                                    });
                                self.blur_v_res = Some((buf, bg));
                            }
                            // Horizontal: snapshot → effect_h
                            let u_h = BlurUniforms {
                                direction: [1.0, 0.0],
                                texel,
                                radius,
                                _pad: [0.0; 3],
                            };
                            let (ref buf_h, ref bg_h) = *self.blur_h_res.as_ref().unwrap();
                            self.queue
                                .write_buffer(buf_h, 0, bytemuck::cast_slice(&[u_h]));
                            {
                                let mut bp =
                                    frame
                                        .encoder
                                        .begin_render_pass(&wgpu::RenderPassDescriptor {
                                            label: Some("blur h"),
                                            color_attachments: &[Some(
                                                wgpu::RenderPassColorAttachment {
                                                    view: eh_view,
                                                    resolve_target: None,
                                                    ops: wgpu::Operations {
                                                        load: wgpu::LoadOp::Clear(
                                                            wgpu::Color::TRANSPARENT,
                                                        ),
                                                        store: wgpu::StoreOp::Store,
                                                    },
                                                    depth_slice: None,
                                                },
                                            )],
                                            depth_stencil_attachment: None,
                                            timestamp_writes: None,
                                            occlusion_query_set: None,
                                            multiview_mask: None,
                                        });
                                bp.set_pipeline(&self.blur_pipeline);
                                bp.set_bind_group(0, bg_h, &[]);
                                bp.set_vertex_buffer(0, self.blit_vbuf.as_ref().unwrap().slice(..));
                                bp.set_index_buffer(
                                    self.blit_ibuf.as_ref().unwrap().slice(..),
                                    wgpu::IndexFormat::Uint32,
                                );
                                bp.draw_indexed(0..6, 0, 0..1);
                            }
                            // Vertical: effect_h → effect_v
                            let u_v = BlurUniforms {
                                direction: [0.0, 1.0],
                                texel,
                                radius,
                                _pad: [0.0; 3],
                            };
                            let (ref buf_v, ref bg_v) = *self.blur_v_res.as_ref().unwrap();
                            self.queue
                                .write_buffer(buf_v, 0, bytemuck::cast_slice(&[u_v]));
                            {
                                let mut bp =
                                    frame
                                        .encoder
                                        .begin_render_pass(&wgpu::RenderPassDescriptor {
                                            label: Some("blur v"),
                                            color_attachments: &[Some(
                                                wgpu::RenderPassColorAttachment {
                                                    view: ev_view,
                                                    resolve_target: None,
                                                    ops: wgpu::Operations {
                                                        load: wgpu::LoadOp::Clear(
                                                            wgpu::Color::TRANSPARENT,
                                                        ),
                                                        store: wgpu::StoreOp::Store,
                                                    },
                                                    depth_slice: None,
                                                },
                                            )],
                                            depth_stencil_attachment: None,
                                            timestamp_writes: None,
                                            occlusion_query_set: None,
                                            multiview_mask: None,
                                        });
                                bp.set_pipeline(&self.blur_pipeline);
                                bp.set_bind_group(0, bg_v, &[]);
                                bp.set_vertex_buffer(0, self.blit_vbuf.as_ref().unwrap().slice(..));
                                bp.set_index_buffer(
                                    self.blit_ibuf.as_ref().unwrap().slice(..),
                                    wgpu::IndexFormat::Uint32,
                                );
                                bp.draw_indexed(0..6, 0, 0..1);
                            }
                        }
                    }};
                }

                // Draws one backdrop composite quad into the current MSAA pass,
                // sampling the blurred result (effect_v) with a rounded mask + tint.
                macro_rules! draw_backdrop {
                    ($pass:expr, $op:expr, $ev_bg:expr) => {{
                        let op = $op;
                        if let (Some(vbuf), Some(ibuf)) = (backdrop_vbuf_ref, backdrop_ibuf_ref) {
                            $pass.set_pipeline(&self.composite_pipeline);
                            $pass.set_bind_group(0, $ev_bg, &[]);
                            $pass.set_vertex_buffer(0, vbuf.slice(..));
                            $pass.set_index_buffer(ibuf.slice(..), wgpu::IndexFormat::Uint32);
                            $pass.set_scissor_rect(0, 0, scr_w, scr_h);
                            $pass.draw_indexed(op.i_start..op.i_start + op.i_count, 0, 0..1);
                        }
                    }};
                }

                if !has_effects {
                    // ── Fast path: single pass (StoreOp::Discard) ──
                    let mut pass = frame
                        .encoder
                        .begin_render_pass(&wgpu::RenderPassDescriptor {
                            label: Some("persistent"),
                            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                                view: mv,
                                resolve_target: Some(pv),
                                ops: wgpu::Operations {
                                    load: first_load,
                                    store: wgpu::StoreOp::Discard,
                                },
                                depth_slice: None,
                            })],
                            depth_stencil_attachment: None,
                            timestamp_writes: None,
                            occlusion_query_set: None,
                            multiview_mask: None,
                        });
                    let mut text_cursor = 0usize;
                    for layer in &z_layers {
                        let count = text_by_z[text_cursor..]
                            .iter()
                            .take_while(|t| t.z_index == layer.z)
                            .count();
                        let texts = &text_by_z[text_cursor..text_cursor + count];
                        text_cursor += count;
                        draw_surface_layer!(pass, layer, texts);
                    }
                } else {
                    // ── Segmented path: interleave surface layers, blend effects,
                    // and backdrop-blur composites by z. Surfaces at z <= boundary
                    // composite first; then effects/backdrops at that z composite over
                    // them. MSAA persists across passes (Load + Store).
                    let mut effect_i = 0usize;
                    let mut backdrop_i = 0usize;
                    let mut layer_i = 0usize;
                    let mut text_cursor = 0usize;
                    let mut first = true;
                    let snapshot_copy = |enc: &mut wgpu::CommandEncoder, cfg_w: u32, cfg_h: u32| {
                        if let (Some(src), Some(dst)) = (persistent_tex, snapshot_tex) {
                            enc.copy_texture_to_texture(
                                wgpu::TexelCopyTextureInfo {
                                    texture: src,
                                    mip_level: 0,
                                    origin: wgpu::Origin3d::ZERO,
                                    aspect: wgpu::TextureAspect::All,
                                },
                                wgpu::TexelCopyTextureInfo {
                                    texture: dst,
                                    mip_level: 0,
                                    origin: wgpu::Origin3d::ZERO,
                                    aspect: wgpu::TextureAspect::All,
                                },
                                wgpu::Extent3d {
                                    width: cfg_w,
                                    height: cfg_h,
                                    depth_or_array_layers: 1,
                                },
                            );
                        }
                    };
                    loop {
                        let next_e = effect_ops.get(effect_i).map(|e| e.z);
                        let next_b = backdrop_ops.get(backdrop_i).map(|b| b.z);
                        let boundary_z = match (next_e, next_b) {
                            (Some(a), Some(b)) => Some(a.min(b)),
                            (Some(a), None) => Some(a),
                            (None, Some(b)) => Some(b),
                            (None, None) => None,
                        };
                        if boundary_z.is_none() && layer_i >= z_layers.len() {
                            break;
                        }

                        // Segment: layers with z <= boundary_z (or all remaining).
                        let seg_start = layer_i;
                        while layer_i < z_layers.len() {
                            match boundary_z {
                                Some(bz) if z_layers[layer_i].z > bz => break,
                                _ => layer_i += 1,
                            }
                        }
                        if layer_i > seg_start {
                            let load = if first {
                                first_load
                            } else {
                                wgpu::LoadOp::Load
                            };
                            first = false;
                            let mut pass =
                                frame
                                    .encoder
                                    .begin_render_pass(&wgpu::RenderPassDescriptor {
                                        label: Some("persistent-seg"),
                                        color_attachments: &[Some(
                                            wgpu::RenderPassColorAttachment {
                                                view: mv,
                                                resolve_target: Some(pv),
                                                ops: wgpu::Operations {
                                                    load,
                                                    store: wgpu::StoreOp::Store,
                                                },
                                                depth_slice: None,
                                            },
                                        )],
                                        depth_stencil_attachment: None,
                                        timestamp_writes: None,
                                        occlusion_query_set: None,
                                        multiview_mask: None,
                                    });
                            for li in seg_start..layer_i {
                                let layer = &z_layers[li];
                                let count = text_by_z[text_cursor..]
                                    .iter()
                                    .take_while(|t| t.z_index == layer.z)
                                    .count();
                                let texts = &text_by_z[text_cursor..text_cursor + count];
                                text_cursor += count;
                                draw_surface_layer!(pass, layer, texts);
                            }
                        }

                        let bz = match boundary_z {
                            Some(z) => z,
                            None => break,
                        };

                        // ── Backdrop composites at bz (blur first, then composite) ──
                        while backdrop_i < backdrop_ops.len() && backdrop_ops[backdrop_i].z == bz {
                            let radius = backdrop_ops[backdrop_i].blur_radius;
                            // Snapshot current backdrop, blur it (separate passes), then
                            // composite the rounded region into the MSAA target.
                            snapshot_copy(&mut frame.encoder, cfg_w, cfg_h);
                            run_backdrop_blur!(radius);
                            if let Some(ev_bg) = effect_v_bg {
                                let load = if first {
                                    first_load
                                } else {
                                    wgpu::LoadOp::Load
                                };
                                first = false;
                                let mut pass =
                                    frame
                                        .encoder
                                        .begin_render_pass(&wgpu::RenderPassDescriptor {
                                            label: Some("backdrop-composite"),
                                            color_attachments: &[Some(
                                                wgpu::RenderPassColorAttachment {
                                                    view: mv,
                                                    resolve_target: Some(pv),
                                                    ops: wgpu::Operations {
                                                        load,
                                                        store: wgpu::StoreOp::Store,
                                                    },
                                                    depth_slice: None,
                                                },
                                            )],
                                            depth_stencil_attachment: None,
                                            timestamp_writes: None,
                                            occlusion_query_set: None,
                                            multiview_mask: None,
                                        });
                                draw_backdrop!(pass, &backdrop_ops[backdrop_i], ev_bg);
                            }
                            backdrop_i += 1;
                        }

                        // ── Blend effects at bz ──
                        if effect_i < effect_ops.len() && effect_ops[effect_i].z == bz {
                            snapshot_copy(&mut frame.encoder, cfg_w, cfg_h);
                            let load = if first {
                                first_load
                            } else {
                                wgpu::LoadOp::Load
                            };
                            first = false;
                            let mut pass =
                                frame
                                    .encoder
                                    .begin_render_pass(&wgpu::RenderPassDescriptor {
                                        label: Some("effect-seg"),
                                        color_attachments: &[Some(
                                            wgpu::RenderPassColorAttachment {
                                                view: mv,
                                                resolve_target: Some(pv),
                                                ops: wgpu::Operations {
                                                    load,
                                                    store: wgpu::StoreOp::Store,
                                                },
                                                depth_slice: None,
                                            },
                                        )],
                                        depth_stencil_attachment: None,
                                        timestamp_writes: None,
                                        occlusion_query_set: None,
                                        multiview_mask: None,
                                    });
                            if let Some(snap_bg) = snapshot_bg {
                                while effect_i < effect_ops.len() && effect_ops[effect_i].z == bz {
                                    draw_effect!(pass, &effect_ops[effect_i], snap_bg);
                                    effect_i += 1;
                                }
                            } else {
                                while effect_i < effect_ops.len() && effect_ops[effect_i].z == bz {
                                    effect_i += 1;
                                }
                            }
                        }
                    }
                }
            }
        }

        for &hash in &accessed_image_hashes {
            if let Some(entry) = self.image_cache.get_mut(&hash) {
                entry.5 = self.image_cache_frame;
            }
        }

        // ── Pass 2: Blit persistent texture to swapchain ──
        let Some(persistent_bg) = self.persistent_bind_group.as_ref() else {
            push_error(UiError::GpuRender("missing persistent bind group".into()));
            return;
        };
        let Some(blit_vbuf) = self.blit_vbuf.as_ref() else {
            push_error(UiError::GpuRender("missing blit vertex buffer".into()));
            return;
        };
        let Some(blit_ibuf) = self.blit_ibuf.as_ref() else {
            push_error(UiError::GpuRender("missing blit index buffer".into()));
            return;
        };
        {
            let c = self.clear_color.to_linear();
            let mut pass = frame
                .encoder
                .begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("blit"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &frame.view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color {
                                r: c.r as f64,
                                g: c.g as f64,
                                b: c.b as f64,
                                a: c.a as f64,
                            }),
                            store: wgpu::StoreOp::Store,
                        },
                        depth_slice: None,
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                });

            pass.set_pipeline(&self.blit_pipeline);
            pass.set_bind_group(0, persistent_bg, &[]);
            pass.set_vertex_buffer(0, blit_vbuf.slice(..));
            pass.set_index_buffer(blit_ibuf.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..6, 0, 0..1);
        }
    }

    pub fn end_frame(&mut self, frame: Frame) {
        self.belt.finish();
        self.queue.submit([frame.encoder.finish()]);
        self.belt.recall();
        self.queue.present(frame.texture);
        // Trim path cache (LRU)
        self.path_cache_frame += 1;
        // Trim stale entries every 30 frames
        if self.path_cache_frame.is_multiple_of(30) {
            let threshold = self.path_cache_frame.saturating_sub(60);
            self.path_cache.retain(|_, v| v.last_frame >= threshold);
        }
        // Hard limit to prevent unbounded growth
        const MAX_PATH_CACHE: usize = 256;
        if self.path_cache.len() > MAX_PATH_CACHE {
            let threshold = self.path_cache_frame.saturating_sub(30);
            self.path_cache.retain(|_, v| v.last_frame >= threshold);
        }
        self.glyphon.trim();
        if let Some(ref mut go) = self.glyphon_overlay {
            go.trim();
        }
    }

    pub fn resize(&mut self, w: u32, h: u32) {
        if w > 0 && h > 0 {
            if let (Some(surface), Some(config)) = (self.surface.as_ref(), self.config.as_mut()) {
                config.width = w;
                config.height = h;
                self.screen_size = Size::new(w as f32, h as f32);
                surface.configure(&self.device, config);
                self.glyphon.resize(&self.queue, w, h);
                if let Some(ref mut go) = self.glyphon_overlay {
                    go.resize(&self.queue, w, h);
                }
            }
            self.persistent_tex = None;
            self.persistent_view = None;
            self.persistent_bind_group = None;
            self.msaa_tex = None;
            self.msaa_view = None;
        }
    }

    pub fn set_scale_factor(&mut self, sf: f64) {
        self.scale_factor = sf as f32;
    }

    pub fn create_surface(
        &mut self,
        window: &Arc<dyn winit::window::Window>,
    ) -> Result<(), RenderError> {
        let size = window.surface_size();
        let surface = self
            .instance
            .create_surface(Arc::clone(window))
            .map_err(|e| {
                push_error(UiError::GpuInit(GpuErrorKind::Other(e.to_string())));
                RenderError::Surface
            })?;
        let config = match surface.get_default_config(&self.adapter, size.width, size.height) {
            Some(c) => c,
            None => {
                push_error(UiError::GpuInit(GpuErrorKind::Surface));
                return Err(RenderError::Surface);
            }
        };
        surface.configure(&self.device, &config);
        self.screen_size = Size::new(size.width as f32, size.height as f32);
        self.surface = Some(surface);
        self.config = Some(config);
        self.persistent_dirty = true;
        Ok(())
    }

    pub fn destroy_surface(&mut self) {
        self.surface = None;
        self.config = None;
        self.persistent_tex = None;
        self.persistent_view = None;
        self.persistent_bind_group = None;
        self.msaa_tex = None;
        self.msaa_view = None;
        self.snapshot_tex = None;
        self.snapshot_view = None;
        self.snapshot_bind_group = None;
        self.effect_h_tex = None;
        self.effect_h_view = None;
        self.effect_v_tex = None;
        self.effect_v_view = None;
        self.effect_v_bind_group = None;
        self.blur_h_res = None;
        self.blur_v_res = None;
        self.glyphon.invalidate_cache();
        if let Some(ref mut go) = self.glyphon_overlay {
            go.invalidate_cache();
        }
    }

    fn ensure_buffer<'a>(
        buf: &'a mut Option<(wgpu::Buffer, u64)>,
        required: u64,
        usage: wgpu::BufferUsages,
        device: &wgpu::Device,
    ) -> &'a wgpu::Buffer {
        let cap = buf.as_ref().map_or(0, |(_, c)| *c);
        if cap >= required {
            return &buf.as_ref().unwrap().0;
        }
        let new_size = required.next_power_of_two().max(256);
        let new = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: new_size,
            usage: usage | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        *buf = Some((new, new_size));
        &buf.as_ref().unwrap().0
    }
}

fn brush_color_at_pos(brush: &Brush, x: f64, y: f64, gs: (f64, f64), ge: (f64, f64)) -> [f32; 4] {
    match brush {
        Brush::Solid(c) => c.to_linear_array(),
        Brush::Gradient(g) => {
            let n = g.stop_count.min(4) as usize;
            if n < 2 {
                return [1.0, 1.0, 1.0, 1.0];
            }
            let dx = ge.0 - gs.0;
            let dy = ge.1 - gs.1;
            let len_sq = dx * dx + dy * dy;
            if len_sq < 0.0001 {
                return g.stops[0].color.to_linear_array();
            }
            let t = ((x - gs.0) * dx + (y - gs.1) * dy) / len_sq;
            let t = t.clamp(0.0, 1.0) as f32;
            for i in 0..n - 1 {
                let o0 = g.stops[i].offset;
                let o1 = g.stops[i + 1].offset;
                if t >= o0 && t <= o1 {
                    let local_t = if o1 > o0 { (t - o0) / (o1 - o0) } else { 0.0 };
                    return g.stops[i]
                        .color
                        .lerp(&g.stops[i + 1].color, local_t)
                        .to_linear_array();
                }
            }
            g.stops[n - 1].color.to_linear_array()
        }
    }
}

fn hash_brush(brush: &Brush) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = rustc_hash::FxHasher::default();
    match brush {
        Brush::Solid(c) => {
            0u64.hash(&mut h);
            c.r.to_bits().hash(&mut h);
            c.g.to_bits().hash(&mut h);
            c.b.to_bits().hash(&mut h);
            c.a.to_bits().hash(&mut h);
        }
        Brush::Gradient(g) => {
            1u64.hash(&mut h);
            g.start.0.to_bits().hash(&mut h);
            g.start.1.to_bits().hash(&mut h);
            g.end.0.to_bits().hash(&mut h);
            g.end.1.to_bits().hash(&mut h);
        }
    }
    h.finish()
}

fn hash_stroke(stroke: &kurbo::Stroke) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = rustc_hash::FxHasher::default();
    stroke.width.to_bits().hash(&mut h);
    (match stroke.start_cap {
        kurbo::Cap::Butt => 0u64,
        kurbo::Cap::Round => 1u64,
        kurbo::Cap::Square => 2u64,
    })
    .hash(&mut h);
    (match stroke.join {
        kurbo::Join::Bevel => 0u64,
        kurbo::Join::Round => 1u64,
        kurbo::Join::Miter => 2u64,
    })
    .hash(&mut h);
    h.finish()
}

/// Hash only the LINEAR part (scale/rotate/shear) of a transform.
///
/// Path-cache keys deliberately exclude the translation (audit 2026-07-17
/// round 5, B1): tessellated geometry is translation-invariant, and keying
/// on the full transform meant every scrolled icon re-tessellated every
/// frame (measured 2–10 µs per icon per miss). The translation is applied
/// at vertex-fill time instead.
fn hash_linear_transform(xf: glam::Affine2) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = rustc_hash::FxHasher::default();
    xf.matrix2.x_axis.x.to_bits().hash(&mut h);
    xf.matrix2.x_axis.y.to_bits().hash(&mut h);
    xf.matrix2.y_axis.x.to_bits().hash(&mut h);
    xf.matrix2.y_axis.y.to_bits().hash(&mut h);
    h.finish()
}

fn glam_to_kurbo_affine(xform: glam::Affine2) -> kurbo::Affine {
    kurbo::Affine::new([
        xform.matrix2.x_axis.x as f64,
        xform.matrix2.x_axis.y as f64,
        xform.matrix2.y_axis.x as f64,
        xform.matrix2.y_axis.y as f64,
        xform.translation.x as f64,
        xform.translation.y as f64,
    ])
}

#[derive(Hash, PartialEq, Eq, Clone, Copy, Debug)]
struct PathCacheKey {
    path_hash: u64,
    brush_hash: u64,
    stroke_hash: u64,
    xform_hash: u64,
}

/// Tessellated path geometry in *untranslated* logical space: the linear
/// part of the draw transform is baked in, the translation is NOT (B1).
struct CachedPathMesh {
    logical_vertices: Vec<[f32; 2]>,
    indices: Vec<u32>,
    last_frame: u64,
}

pub struct Frame {
    pub texture: wgpu::SurfaceTexture,
    pub view: wgpu::TextureView,
    pub encoder: wgpu::CommandEncoder,
}

#[derive(Debug)]
pub enum RenderError {
    NoAdapter,
    Surface,
    Device,
}
impl std::fmt::Display for RenderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "render error")
    }
}
impl std::error::Error for RenderError {}

fn rect_to_vertices(
    rect: Rect,
    screen: Size,
    color: Color,
    radii: crate::style::CornerRadii,
    stroke_width: f32,
    xform: Affine2,
    clip: ClipInfo,
    shadow_offset: [f32; 2],
    shadow_color: [f32; 4],
    shadow_blur: f32,
    is_shadow: f32,
) -> [RectVertex; 4] {
    let sx = 2.0 / screen.width;
    let sy = -2.0 / screen.height;
    // Linearize for the sRGB render target (see Color::to_linear). Text (glyphon
    // ColorMode::Accurate) and images (sRGB texture auto-decode) handle this
    // themselves, so only geometry shaders linearize here.
    let c = color.to_linear_array();
    let rs = [
        radii.top_left,
        radii.top_right,
        radii.bottom_right,
        radii.bottom_left,
    ];
    let so = clip.scroll_offset;
    let cr = [
        clip.rect.x + so[0],
        clip.rect.y + so[1],
        clip.rect.x + clip.rect.width + so[0],
        clip.rect.y + clip.rect.height + so[1],
    ];
    let cra = [
        clip.radius.top_left,
        clip.radius.top_right,
        clip.radius.bottom_right,
        clip.radius.bottom_left,
    ];

    let raw00 = glam::Vec2::new(rect.x, rect.y);
    let raw10 = glam::Vec2::new(rect.x + rect.width, rect.y);
    let raw11 = glam::Vec2::new(rect.x + rect.width, rect.y + rect.height);
    let raw01 = glam::Vec2::new(rect.x, rect.y + rect.height);
    let tp00 = xform.transform_point2(raw00);
    let tp10 = xform.transform_point2(raw10);
    let tp11 = xform.transform_point2(raw11);
    let tp01 = xform.transform_point2(raw01);

    // Shadow uses document-space world_pos + center (same space as normal rects
    // and as clip_rect), so clip_alpha compares consistently. Position NDC still
    // uses the transformed corners (tp00..) below. Using transformed world_pos
    // here would break clipping under scroll (clip_rect is document space).
    let (wp00, wp10, wp11, wp01): ([f32; 2], [f32; 2], [f32; 2], [f32; 2]) = (
        [rect.x, rect.y],
        [rect.x + rect.width, rect.y],
        [rect.x + rect.width, rect.y + rect.height],
        [rect.x, rect.y + rect.height],
    );

    let (so_val, sz, rs_val): ([f32; 2], [f32; 2], [f32; 4]) = if is_shadow > 0.0 {
        let center = [rect.x + rect.width * 0.5, rect.y + rect.height * 0.5];
        (center, [rect.width, rect.height], rs)
    } else {
        (shadow_offset, [rect.width, rect.height], rs)
    };

    [
        RectVertex {
            position: [tp00.x * sx - 1.0, tp00.y * sy + 1.0],
            color: c,
            size: sz,
            radii: rs_val,
            local: [0.0, 0.0],
            stroke_width,
            world_pos: wp00,
            shadow_offset: so_val,
            shadow_color,
            shadow_blur_radius: shadow_blur,
            is_shadow,
            clip_rect: cr,
            clip_radius: cra,
        },
        RectVertex {
            position: [tp10.x * sx - 1.0, tp10.y * sy + 1.0],
            color: c,
            size: sz,
            radii: rs_val,
            local: [rect.width, 0.0],
            stroke_width,
            world_pos: wp10,
            shadow_offset: so_val,
            shadow_color,
            shadow_blur_radius: shadow_blur,
            is_shadow,
            clip_rect: cr,
            clip_radius: cra,
        },
        RectVertex {
            position: [tp11.x * sx - 1.0, tp11.y * sy + 1.0],
            color: c,
            size: sz,
            radii: rs_val,
            local: [rect.width, rect.height],
            stroke_width,
            world_pos: wp11,
            shadow_offset: so_val,
            shadow_color,
            shadow_blur_radius: shadow_blur,
            is_shadow,
            clip_rect: cr,
            clip_radius: cra,
        },
        RectVertex {
            position: [tp01.x * sx - 1.0, tp01.y * sy + 1.0],
            color: c,
            size: sz,
            radii: rs_val,
            local: [0.0, rect.height],
            stroke_width,
            world_pos: wp01,
            shadow_offset: so_val,
            shadow_color,
            shadow_blur_radius: shadow_blur,
            is_shadow,
            clip_rect: cr,
            clip_radius: cra,
        },
    ]
}

fn gradient_to_vertices(
    rect: Rect,
    screen: Size,
    gradient: crate::style::LinearGradient,
    radii: crate::style::CornerRadii,
    stroke_width: f32,
    xform: Affine2,
    clip: ClipInfo,
) -> [GradientVertex; 4] {
    let sx = 2.0 / screen.width;
    let sy = -2.0 / screen.height;
    let p00 = xform.transform_point2(glam::Vec2::new(rect.x, rect.y));
    let p10 = xform.transform_point2(glam::Vec2::new(rect.x + rect.width, rect.y));
    let p11 = xform.transform_point2(glam::Vec2::new(rect.x + rect.width, rect.y + rect.height));
    let p01 = xform.transform_point2(glam::Vec2::new(rect.x, rect.y + rect.height));
    let s = [rect.width, rect.height];
    let so = clip.scroll_offset;
    let cr = [
        clip.rect.x + so[0],
        clip.rect.y + so[1],
        clip.rect.x + clip.rect.width + so[0],
        clip.rect.y + clip.rect.height + so[1],
    ];
    let cra = [
        clip.radius.top_left,
        clip.radius.top_right,
        clip.radius.bottom_right,
        clip.radius.bottom_left,
    ];
    let n = gradient.stop_count.min(4) as usize;
    let c0 = gradient
        .stops
        .first()
        .map_or([0.0; 4], |s| s.color.to_linear_array());
    let c1 = gradient
        .stops
        .get(1)
        .map_or(c0, |s| s.color.to_linear_array());
    let c2 = gradient
        .stops
        .get(2)
        .map_or(c1, |s| s.color.to_linear_array());
    let c3 = gradient
        .stops
        .get(3)
        .map_or(c2, |s| s.color.to_linear_array());
    let off1 = gradient.stops.get(1).map_or(0.0, |s| s.offset);
    let off2 = gradient.stops.get(2).map_or(1.0, |s| s.offset);
    let off3 = gradient.stops.get(3).map_or(1.0, |s| s.offset);
    let count_offsets: [f32; 4] = [n as f32, off1, off2, off3];
    // Endpoint semantics depend on kind (all mapped to document space here);
    // the shader's gradient_t interprets them per-kind. Kind is baked into the
    // (otherwise unused) shadow_offset.x of each vertex.
    let dir_start: [f32; 2] = [
        rect.x + gradient.start.0 * rect.width,
        rect.y + gradient.start.1 * rect.height,
    ];
    let dir_end: [f32; 2] = [
        rect.x + gradient.end.0 * rect.width,
        rect.y + gradient.end.1 * rect.height,
    ];
    let kind_f = match gradient.kind {
        crate::style::GradientKind::Linear => 0.0,
        crate::style::GradientKind::Radial => 1.0,
        crate::style::GradientKind::Conic => 2.0,
    };
    GradientVertex::gen_quad(
        [p00, p10, p11, p01],
        sx,
        sy,
        s,
        radii,
        stroke_width,
        cr,
        cra,
        c0,
        c1,
        c2,
        c3,
        count_offsets,
        dir_start,
        dir_end,
        rect,
        kind_f,
    )
}

impl GradientVertex {
    #[allow(clippy::too_many_arguments)]
    fn gen_quad(
        corners: [glam::Vec2; 4],
        sx: f32,
        sy: f32,
        size: [f32; 2],
        radii: crate::style::CornerRadii,
        stroke_width: f32,
        clip_rect: [f32; 4],
        clip_radius: [f32; 4],
        c0: [f32; 4],
        c1: [f32; 4],
        c2: [f32; 4],
        c3: [f32; 4],
        count_offsets: [f32; 4],
        dir_start: [f32; 2],
        dir_end: [f32; 2],
        rect: Rect,
        kind: f32,
    ) -> [GradientVertex; 4] {
        let rs = [
            radii.top_left,
            radii.top_right,
            radii.bottom_right,
            radii.bottom_left,
        ];
        let locals: [[f32; 2]; 4] = [
            [0.0, 0.0],
            [rect.width, 0.0],
            [rect.width, rect.height],
            [0.0, rect.height],
        ];
        let world_ps: [[f32; 2]; 4] = [
            [rect.x, rect.y],
            [rect.x + rect.width, rect.y],
            [rect.x + rect.width, rect.y + rect.height],
            [rect.x, rect.y + rect.height],
        ];
        // shadow_offset is unused by the gradient shader for shadows; we bake the
        // gradient kind into .x (0=Linear, 1=Radial, 2=Conic).
        let kv: [f32; 2] = [kind, 0.0];
        [
            GradientVertex {
                position: [corners[0].x * sx - 1.0, corners[0].y * sy + 1.0],
                size,
                radii: rs,
                local: locals[0],
                stroke_width,
                world_pos: world_ps[0],
                shadow_offset: kv,
                clip_rect,
                clip_radius,
                color_0: c0,
                color_1: c1,
                color_2: c2,
                color_3: c3,
                count_offsets,
                dir_start,
                dir_end,
            },
            GradientVertex {
                position: [corners[1].x * sx - 1.0, corners[1].y * sy + 1.0],
                size,
                radii: rs,
                local: locals[1],
                stroke_width,
                world_pos: world_ps[1],
                shadow_offset: kv,
                clip_rect,
                clip_radius,
                color_0: c0,
                color_1: c1,
                color_2: c2,
                color_3: c3,
                count_offsets,
                dir_start,
                dir_end,
            },
            GradientVertex {
                position: [corners[2].x * sx - 1.0, corners[2].y * sy + 1.0],
                size,
                radii: rs,
                local: locals[2],
                stroke_width,
                world_pos: world_ps[2],
                shadow_offset: kv,
                clip_rect,
                clip_radius,
                color_0: c0,
                color_1: c1,
                color_2: c2,
                color_3: c3,
                count_offsets,
                dir_start,
                dir_end,
            },
            GradientVertex {
                position: [corners[3].x * sx - 1.0, corners[3].y * sy + 1.0],
                size,
                radii: rs,
                local: locals[3],
                stroke_width,
                world_pos: world_ps[3],
                shadow_offset: kv,
                clip_rect,
                clip_radius,
                color_0: c0,
                color_1: c1,
                color_2: c2,
                color_3: c3,
                count_offsets,
                dir_start,
                dir_end,
            },
        ]
    }
}

/// Image quad with rounded-rect clip support.
fn image_quad_vertices(
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    screen: Size,
    clip_rect: [f32; 4],
    clip_radius: [f32; 4],
    opacity: f32,
) -> [TextVertex; 4] {
    let sx = 2.0 / screen.width;
    let sy = -2.0 / screen.height;
    [
        TextVertex {
            position: [x * sx - 1.0, y * sy + 1.0],
            uv: [0.0, 0.0],
            clip_rect,
            clip_radius,
            world_pos: [x, y],
            opacity,
        },
        TextVertex {
            position: [(x + w) * sx - 1.0, y * sy + 1.0],
            uv: [1.0, 0.0],
            clip_rect,
            clip_radius,
            world_pos: [x + w, y],
            opacity,
        },
        TextVertex {
            position: [(x + w) * sx - 1.0, (y + h) * sy + 1.0],
            uv: [1.0, 1.0],
            clip_rect,
            clip_radius,
            world_pos: [x + w, y + h],
            opacity,
        },
        TextVertex {
            position: [x * sx - 1.0, (y + h) * sy + 1.0],
            uv: [0.0, 1.0],
            clip_rect,
            clip_radius,
            world_pos: [x, y + h],
            opacity,
        },
    ]
}

fn clip_to_scissor(clip: ClipInfo, screen: Size, scale_factor: f32) -> (u32, u32, u32, u32) {
    let sf = scale_factor;
    let c = clip.rect;
    let x = (c.x * sf).round().max(0.0) as u32;
    let y = (c.y * sf).round().max(0.0) as u32;
    let r = ((c.x + c.width) * sf).round().min(screen.width * sf) as u32;
    let b = ((c.y + c.height) * sf).round().min(screen.height * sf) as u32;
    (x.min(r), y.min(b), r.saturating_sub(x), b.saturating_sub(y))
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
