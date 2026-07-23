//! Server-Side Rendering — render widgets to PNG without a window.
//!
//! ## Quick start
//!
//! ```ignore
//! use burin::render::ssr;
//! use burin::widgets::display::Text;
//! use burin::widgets::layout::Center;
//!
//! let png_bytes = ssr::render_to_png(
//!     Center::new(Text::new("Hello from Burin SSR!")),
//!     400.0, 300.0,
//! ).expect("ssr render");
//!
//! std::fs::write("output.png", &png_bytes).unwrap();
//! ```

use std::rc::Rc;

use crate::animation::AnimationDriver;
use crate::core::app_context::{self, AppContext};
use crate::core::clock;
use crate::core::context::MountContext;
use crate::core::dirty_registry;
use crate::core::element::ElementArena;
use crate::core::frame_driver::{self, FrameState};
use crate::core::scheduler;
use crate::core::widget::Widget;
use crate::core::ElementId;
use crate::event::{EventRegistry, FocusManager};
use crate::layout::taffy_bridge::TaffyBridge;
use crate::render::TinySkiaRenderer;
use crate::style::{Color, Rect, Size};
use crate::theme::M3Theme;

#[derive(Debug, Clone)]
pub enum SsrError {
    Mount(String),
    Render(String),
    Encode(String),
}

impl std::fmt::Display for SsrError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Mount(e) => write!(f, "SSR mount error: {e}"),
            Self::Render(e) => write!(f, "SSR render error: {e}"),
            Self::Encode(e) => write!(f, "SSR PNG encode error: {e}"),
        }
    }
}

impl std::error::Error for SsrError {}

pub struct SsrOutput {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u32>,
    pub png: Vec<u8>,
}

/// Render a widget tree to RGBA pixels + PNG bytes.
pub fn render(
    widget: impl Widget + 'static,
    width: f32,
    height: f32,
) -> Result<SsrOutput, SsrError> {
    let theme = M3Theme::from_seed(Color::rgba8(0x67, 0x79, 0xE8, 0xFF))
        .preset(crate::theme::PresetTheme::neo_minimal_slate());

    init_ssr_context();

    let app = Rc::new(AppContext::new());
    app_context::set_current_app(&app);
    clock::install_virtual();
    scheduler::reset();

    let mut arena = ElementArena::new();
    let root_id = arena.allocate();
    arena.set_root(root_id);

    let mut events = EventRegistry::new();
    let mut taffy = TaffyBridge::new();
    let mut focus = FocusManager::new();

    // Ensure ComponentTables — lazily created by AppContext if not already set.
    let ct = app.ensure_component_tables();
    crate::core::element::set_component_tables(ct);

    let mut ctx = MountContext::new(
        &mut arena,
        Some(root_id),
        Some(&mut events),
        &theme,
        None,
        Rc::downgrade(&app),
    );

    dirty_registry::begin_mount_batch();
    let child_id = Box::new(widget).mount_box(&mut ctx);
    dirty_registry::end_mount_batch();
    arena.add_child(root_id, child_id);

    for portal in crate::platform::portal::drain_portals() {
        arena.add_child(root_id, portal);
    }

    let (cmds, tas, painted) = drive_pipeline(
        &app,
        &mut arena,
        &mut taffy,
        &mut events,
        &mut focus,
        Size::new(width, height),
        &theme,
    )
    .map_err(|e| SsrError::Render(e))?;

    let rgba =
        rasterize(cmds, tas, painted, width, height, &theme).map_err(|e| SsrError::Render(e))?;

    let png = encode_png(&rgba, width as u32, height as u32).map_err(|e| SsrError::Encode(e))?;

    Ok(SsrOutput {
        width: width as u32,
        height: height as u32,
        rgba,
        png,
    })
}

/// Render a widget tree directly to PNG bytes.
pub fn render_to_png(
    widget: impl Widget + 'static,
    width: f32,
    height: f32,
) -> Result<Vec<u8>, SsrError> {
    render(widget, width, height).map(|o| o.png)
}

// ── internals ──

fn init_ssr_context() {
    static INIT: std::sync::Once = std::sync::Once::new();
    INIT.call_once(|| {
        auralis_task::init_time_source(Rc::new(clock::ClockTimeSource::new()));
    });
}

fn drive_pipeline(
    app: &Rc<AppContext>,
    arena: &mut ElementArena,
    taffy: &mut TaffyBridge,
    events: &mut EventRegistry,
    focus: &mut FocusManager,
    size: Size,
    theme: &M3Theme,
) -> Result<
    (
        Vec<crate::render::DrawCommand>,
        Vec<crate::render::wgpu::glyphon_bridge::TextAreaDesc>,
        bool,
    ),
    String,
> {
    let scene_cache = std::cell::RefCell::new(rustc_hash::FxHashMap::default());
    let subtree_cache = std::cell::RefCell::new(rustc_hash::FxHashMap::default());
    let fcx = crate::core::frame_context::FrameContext::new(app, &scene_cache, &subtree_cache);

    let input = frame_driver::FrameInput {
        size,
        frame_id: 1,
        is_first_frame: true,
        force_layout: false,
        scale_factor: 1.0,
        bg: theme.scheme.surface,
        fg: theme.scheme.on_surface,
        highlight_mode: focus.highlight_mode(),
        now: clock::now(),
        scroll_friction: 0.0135,
        scroll_stop_speed: 5.0,
        skip_paint: false,
    };

    let mut animations = AnimationDriver::new();
    let mut scroll_kinetic: Option<frame_driver::ScrollKinetic> = None;
    let mut scroll_k_target: Option<ElementId> = None;

    let stage = frame_driver::drive_frame_layout(
        FrameState {
            arena,
            taffy,
            events,
            animations: &mut animations,
            focus,
            scroll_kinetic: &mut scroll_kinetic,
            scroll_kinetic_target: &mut scroll_k_target,
        },
        &input,
        &mut frame_driver::NoHook,
    );

    let outcome = frame_driver::drive_frame_paint(
        FrameState {
            arena,
            taffy,
            events,
            animations: &mut animations,
            focus,
            scroll_kinetic: &mut scroll_kinetic,
            scroll_kinetic_target: &mut scroll_k_target,
        },
        &fcx,
        &input,
        stage,
    );

    Ok((outcome.commands, outcome.text_areas, outcome.painted))
}

fn rasterize(
    cmds: Vec<crate::render::DrawCommand>,
    tas: Vec<crate::render::wgpu::glyphon_bridge::TextAreaDesc>,
    _painted: bool,
    width: f32,
    height: f32,
    theme: &M3Theme,
) -> Result<Vec<u32>, String> {
    let mut renderer = TinySkiaRenderer::new_headless(width, height, 1.0);
    renderer.set_clear_color(theme.scheme.surface);

    let mut cmds = cmds;
    let mut tas = tas;
    let damage = vec![Rect::new(0.0, 0.0, width, height)];
    renderer.render_damage(&damage, &mut cmds, &mut tas, &[]);

    Ok(renderer.pixels().to_vec())
}

fn encode_png(rgba: &[u32], width: u32, height: u32) -> Result<Vec<u8>, String> {
    let mut buf = Vec::with_capacity((width * height * 4) as usize);
    for px in rgba {
        let r = (*px & 0xFF) as u8;
        let g = ((*px >> 8) & 0xFF) as u8;
        let b = ((*px >> 16) & 0xFF) as u8;
        let a = ((*px >> 24) & 0xFF) as u8;
        buf.extend_from_slice(&[r, g, b, a]);
    }

    let mut png_buf = std::io::Cursor::new(Vec::new());
    {
        let encoder = image::codecs::png::PngEncoder::new(&mut png_buf);
        use image::ImageEncoder;
        encoder
            .write_image(&buf, width, height, image::ExtendedColorType::Rgba8)
            .map_err(|e| e.to_string())?;
    }

    Ok(png_buf.into_inner())
}
