use crate::core::context::MountContext;
use crate::core::element::{fire_on_mount, ElementArena};
use crate::core::error::{push_error, GpuErrorKind, UiError};
use crate::core::widget::Widget;
use crate::core::ElementId;
use crate::debug::MetricsHistory;
use crate::event::{ClickCounter, EventRegistry, EventTranslator, FocusManager, FocusReason};
use crate::layout::taffy_bridge::TaffyBridge;
use crate::platform::A11yAdapter;
use crate::style::{Rect, Vec2};
use crate::theme::M3Theme;
use auralis_signal::Signal;
use auralis_task::scheduler::DeferredScheduler;
use rustc_hash::FxHashMap;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant as StdInstant;
use super::action;
use super::config::WindowConfig;
use super::drag;
use super::finger;
use super::scroll_physics::ScrollKinetic;
use super::WindowHandle;

/// Per-window state: arena, renderer, focus, event handling.
pub(crate) struct WindowState {
    pub(crate) config: WindowConfig,
    pub(crate) winit_window: Option<Arc<dyn winit::window::Window>>,
    pub(crate) renderer: Option<crate::render::BackendRenderer>,
    pub arena: ElementArena,
    pub app: std::rc::Rc<crate::core::app_context::AppContext>,
    pub(crate) scene_cache: std::cell::RefCell<
        rustc_hash::FxHashMap<ElementId, std::rc::Rc<crate::render::CachedScene>>,
    >,
    pub(crate) subtree_cache: std::cell::RefCell<
        rustc_hash::FxHashMap<ElementId, std::rc::Rc<crate::render::CachedSubtree>>,
    >,
    pub(crate) focus_manager: FocusManager,
    /// (element, original z_index_floor) for the currently dragged row so its
    /// raised render-layer can be restored on drag-end.
    pub(crate) drag_elevated: Option<(ElementId, Option<i32>)>,
    pub(crate) click_counter: ClickCounter,
    pub(crate) animations: crate::animation::AnimationDriver,
    pub(crate) translator: EventTranslator,
    pub(crate) event_registry: EventRegistry,
    pub(crate) taffy: TaffyBridge,
    pub(crate) last_cursor: Option<crate::style::Point>,
    pub(crate) touches: FxHashMap<u64, finger::FingerState>,
    pub(crate) frame_id: u64,
    pub(crate) drag_text_input: Option<ElementId>,
    pub(crate) scrollbar_drag: Option<(ElementId, crate::widgets::bundle::scroll::ScrollAxis, f32)>,
    pub(crate) scrollbar_hover: Option<(ElementId, crate::widgets::bundle::scroll::ScrollAxis)>,
    pub(crate) scale_factor: f64,
    pub(crate) frame_stats_painted: u64,
    pub(crate) paint_occurred_this_frame: bool,
    pub(crate) needs_taffy: bool,
    pub(crate) needs_theme_reapply: bool,
    pub(crate) theme_signal: Option<Signal<M3Theme>>,
    pub(crate) a11y: A11yAdapter,
    pub(crate) key_bindings: crate::event::bindings::KeyBindingMap,
    pub(crate) flush_scheduler: Rc<DeferredScheduler>,
    pub(crate) coalesce_skip_paint: bool,
    pub(crate) skip_frame: bool,
    pub(crate) close_requested: bool,
    pub(crate) needs_rebuild: bool,
    pub(crate) needs_cleanup: bool,
    pub(crate) drag_state: Option<drag::DragState>,
    pub(crate) metrics: MetricsHistory,
    pub(crate) window_handle: Option<WindowHandle>,
    pub(crate) scroll_kinetic: Option<ScrollKinetic>,
    pub(crate) velocity_history: Vec<(f32, f32, StdInstant)>,
    pub(crate) scroll_kinetic_target: Option<ElementId>,
    pub(crate) last_frame_instant: Option<StdInstant>,
    #[cfg(feature = "tray")]
    pub(crate) tray_icon: Option<crate::platform::tray::TrayIcon>,
    /// Last IME cursor area sent to the platform (surface coords, logical px),
    /// maintained so we skip redundant Update requests (IME area dedup P1).
    pub(crate) last_sent_ime_area: Option<Rect>,

    /// DevTools ring buffer handle (shared across all windows).
    #[cfg(feature = "devtools")]
    pub(crate) devtools_buf: Option<crate::debug::devtools::DevtoolsRingBuffer>,
    #[cfg(feature = "devtools")]
    pub(crate) last_frame_us: u64,
    #[cfg(feature = "devtools")]
    pub(crate) last_paint_us: u64,
}

impl WindowState {
    pub fn new(config: WindowConfig) -> Self {
        let theme_signal = config.theme_signal.clone();
        let flush_scheduler = DeferredScheduler::new();
        Self {
            config,
            winit_window: None,
            renderer: None,
            arena: ElementArena::new(),
            app: std::rc::Rc::new(crate::core::app_context::AppContext::new()),
            scene_cache: std::cell::RefCell::new(rustc_hash::FxHashMap::default()),
            subtree_cache: std::cell::RefCell::new(rustc_hash::FxHashMap::default()),
            focus_manager: FocusManager::new(),
            drag_elevated: None,
            click_counter: ClickCounter::new(),
            animations: crate::animation::AnimationDriver::new(),
            translator: EventTranslator::new(),
            event_registry: EventRegistry::new(),
            taffy: TaffyBridge::new(),
            frame_id: 0,
            last_cursor: None,
            touches: FxHashMap::default(),
            drag_text_input: None,
            scrollbar_drag: None,
            scrollbar_hover: None,
            scale_factor: 1.0,
            frame_stats_painted: 0,
            paint_occurred_this_frame: false,
            needs_taffy: true,
            needs_theme_reapply: false,
            theme_signal,
            a11y: A11yAdapter::new(),
            key_bindings: crate::event::bindings::KeyBindingMap::new(),
            flush_scheduler,
            coalesce_skip_paint: false,
            skip_frame: false,
            close_requested: false,
            needs_rebuild: false,
            needs_cleanup: false,
            drag_state: None,
            metrics: MetricsHistory::default(),
            window_handle: None,
            scroll_kinetic: None,
            velocity_history: Vec::new(),
            scroll_kinetic_target: None,
            last_frame_instant: None,
            last_sent_ime_area: None,
            #[cfg(feature = "devtools")]
            devtools_buf: None,
            #[cfg(feature = "devtools")]
            last_frame_us: 0,
            #[cfg(feature = "devtools")]
            last_paint_us: 0,
            #[cfg(feature = "tray")]
            tray_icon: None,
        }
    }

    /// Access the rolling frame metrics history (up to 600 frames).
    #[allow(dead_code)]
    pub fn metrics(&self) -> &MetricsHistory {
        &self.metrics
    }

    fn primary_state(&self) -> Option<&finger::FingerState> {
        self.touches.get(&0)
    }
    #[allow(dead_code)]
    fn primary_hovered(&self) -> Option<ElementId> {
        self.primary_state()
            .and_then(|s| s.hovered_chain.first().copied())
    }
    pub(crate) fn primary_pressed(&self) -> Option<ElementId> {
        self.primary_state().and_then(|s| s.pressed)
    }
    pub(crate) fn finger_state(&mut self, fid: u64) -> &mut finger::FingerState {
        self.touches.entry(fid).or_default()
    }

    /// Start an animated scroll to the given offset on the given scroll container.
    #[allow(dead_code)]
    pub fn scroll_to_animated(&mut self, target_eid: ElementId, to: Vec2, duration_secs: f32) {
        let start = self
            .arena
            .comp_scroll(target_eid)
            .map(|s| s.scroll_offset.get())
            .unwrap_or(Vec2::ZERO);
        self.scroll_kinetic_target = Some(target_eid);
        self.scroll_kinetic = Some(ScrollKinetic::AnimatedTo {
            target: to,
            start,
            anchor: None,
            duration_secs,
        });
    }

    pub(crate) fn mount_root(&mut self, widget: Box<dyn Widget>) {
        // Install the active AppContext BEFORE allocating the root element.
        // `arena.allocate()` calls `register_element_full`, which forwards to
        // the active AppContext when installed (else the thread_local fallback).
        // If we allocate the root first, its registry entry lands in the
        // thread_local instead of app.el_registry, leaving the root absent from
        // app's registry — which breaks `is_visible_chain_fast` (the chain walk
        // hits the unregistered root, returns false, and every hit-test fails).
        crate::core::app_context::set_current_app(&self.app);
        crate::core::element::set_component_tables(self.arena.component_tables.clone());
        let root_id = self.arena.allocate();
        self.arena.set_root(root_id);
        debug_assert!(
            self.app.has_element(root_id),
            "root element {:?} must be registered in the active AppContext after \
             allocate(); if this fails, set_current_app ran too late and the \
             root landed in the thread_local fallback, breaking hit-testing",
            root_id,
        );
        let wh = self
            .window_handle
            .as_ref()
            .expect("window_handle not set before mount_root");
        #[cfg(feature = "i18n")]
        let ctx_i18n = self.config.i18n.as_deref();
        let app_weak = std::rc::Rc::downgrade(&self.app);
        let mut ctx = MountContext::new(
            &mut self.arena,
            None,
            Some(&mut self.event_registry),
            &self.config.theme,
            Some(wh),
            app_weak,
        );
        #[cfg(feature = "i18n")]
        {
            ctx.i18n = ctx_i18n;
        }
        crate::core::dirty_registry::begin_mount_batch();
        let widget_id = Box::new(widget).mount_box(&mut ctx);
        crate::core::dirty_registry::end_mount_batch();
        self.arena.add_child(root_id, widget_id);
        let portals: Vec<ElementId> = crate::platform::portal::drain_portals();
        for portal_id in portals {
            self.arena.add_child(root_id, portal_id);
        }
        for removed in crate::platform::portal::drain_portal_removals() {
            self.arena.remove(removed);
        }
        fire_on_mount(&mut self.arena);
        for focus_id in self.event_registry.drain_autofocus() {
            if self.focus_manager.is_in_current_scope(focus_id) {
                action::transfer_focus(self, focus_id, FocusReason::Programmatic);
            }
        }
        for focus_id in self.focus_manager.drain_autofocus() {
            if self.focus_manager.is_in_current_scope(focus_id) {
                action::transfer_focus(self, focus_id, FocusReason::Programmatic);
            }
        }
    }

    pub(crate) fn ensure_renderer(&mut self) {
        if self.renderer.is_some() {
            return;
        }
        let window = match self.winit_window.clone() {
            Some(w) => w,
            None => return,
        };

        #[cfg(feature = "text-cosmic")]
        crate::render::wgpu::glyphon_bridge::ensure_font_system();

        match self.config.backend {
            crate::render::RendererChoice::Auto => {
                self.try_init_gpu(&window);
                if self.renderer.is_none() {
                    self.try_init_cpu(&window);
                }
                if self.renderer.is_none() {
                    self.try_init_gpu(&window);
                }
            }
            crate::render::RendererChoice::Gpu => {
                self.try_init_gpu(&window);
            }
            crate::render::RendererChoice::Cpu => {
                self.try_init_cpu(&window);
                if self.renderer.is_none() {
                    self.try_init_gpu(&window);
                }
            }
        }
    }

    fn try_init_gpu(&mut self, window: &Arc<dyn winit::window::Window>) {
        #[cfg(feature = "backend-wgpu")]
        {
            #[cfg(not(target_arch = "wasm32"))]
            let rt = pollster::block_on(crate::render::WgpuRenderer::new(Arc::clone(window)));
            #[cfg(target_arch = "wasm32")]
            let rt = Err(crate::render::wgpu::RenderError::NoAdapter);

            match rt {
                Ok(mut r) => {
                    r.set_clear_color(self.config.theme.scheme.surface);
                    self.renderer = Some(crate::render::BackendRenderer::Gpu(r));
                }
                Err(e) => {
                    push_error(UiError::GpuInit(match e {
                        crate::render::wgpu::RenderError::NoAdapter => GpuErrorKind::NoAdapter,
                        crate::render::wgpu::RenderError::Surface => GpuErrorKind::Surface,
                        crate::render::wgpu::RenderError::Device => GpuErrorKind::Device,
                    }));
                }
            }
        }
    }

    fn try_init_cpu(&mut self, window: &Arc<dyn winit::window::Window>) {
        #[cfg(feature = "backend-tiny-skia")]
        {
            match crate::render::TinySkiaRenderer::new(
                Arc::clone(window),
                self.config.width,
                self.config.height,
                self.scale_factor as f32,
            ) {
                Ok(mut r) => {
                    r.set_clear_color(self.config.theme.scheme.surface);
                    self.renderer = Some(crate::render::BackendRenderer::Cpu(r));
                }
                Err(_e) => {
                    push_error(UiError::GpuInit(GpuErrorKind::Other(format!(
                        "CPU renderer init failed: {_e}"
                    ))));
                    #[cfg(any(feature = "devtools", feature = "file-logging"))]
                    tracing::warn!("CPU renderer init failed: {_e}");
                }
            }
        }
    }
}
