//! Top-level Window with winit event loop, rendering pipeline, layout, and input.

use crate::animation::AnimationDriver;
use crate::core::config::StateFlags;
use crate::core::context::MountContext;
use crate::core::dirty_registry;
use crate::core::element::{
    apply_drag_layouts, fire_on_mount, reapply_element_theme, DirtyFlags, ElementArena,
};
use crate::core::error::{
    panic_to_string, push_error, set_error_buffer_limit, set_error_handler, GpuErrorKind, UiError,
};
use crate::core::widget::Widget;
use crate::core::ElementId;
use crate::debug::{FrameMetrics, MetricsHistory};
use crate::event::action::{Action, ActionKind};
use crate::event::bindings::KeyBindingMap;
use crate::event::focus_traversal::Direction;
use crate::event::DragData;
use crate::event::{ClickCounter, EventRegistry, EventTranslator, FocusManager, FocusReason};
use crate::layout::taffy_bridge::TaffyBridge;
#[cfg(feature = "tray")]
use crate::platform::tray;
use crate::platform::A11yAdapter;
use crate::platform::CursorIcon;
use crate::render::wgpu::glyphon_bridge::create_buffer;
use crate::render::Painter;
use crate::style::{Color, Rect, Size, Vec2};
use crate::theme::M3Theme;
use auralis_signal::Signal;
use auralis_task::scheduler::DeferredScheduler;
use rustc_hash::FxHashMap;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant as StdInstant;
use winit::application::ApplicationHandler;
use winit::event_loop::ActiveEventLoop;
use winit::event_loop::ControlFlow;
use winit::raw_window_handle::HasWindowHandle;

/// Lightweight CPU render-phase instrumentation.
///
/// Enable at runtime with `AURALIS_CPU_PERF=1`. Works in release builds (the
/// only meaningful place to measure tight pixel loops). Every 60 painted frames
/// it prints averaged per-phase timings for the CPU (tiny-skia) backend:
/// `gen` (build the DrawCommand list, cache-replayed), `raster`
/// (`render_damage` — clear + rasterise all commands/text), `present`
/// (buffer copy + softbuffer present). Zero overhead when disabled.
#[cfg(feature = "backend-tiny-skia")]
mod cpu_perf {
    use std::cell::RefCell;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::OnceLock;
    use std::time::Instant;

    struct Acc {
        frames: u32,
        gen_us: u64,
        raster_us: u64,
        present_us: u64,
        cmds: u64,
        text: u64,
        max_raster_us: u64,
        dmg_rects: u64,
        dmg_frac: f64,
        last_pr: usize,
        last_rp: usize,
        last_flush: Option<Instant>,
    }

    impl Default for Acc {
        fn default() -> Self {
            Self {
                frames: 0,
                gen_us: 0,
                raster_us: 0,
                present_us: 0,
                cmds: 0,
                text: 0,
                max_raster_us: 0,
                dmg_rects: 0,
                dmg_frac: 0.0,
                last_pr: 0,
                last_rp: 0,
                last_flush: None,
            }
        }
    }

    thread_local! {
        static ACC: RefCell<Acc> = RefCell::new(Acc::default());
    }

    pub fn enabled() -> bool {
        static ON: OnceLock<bool> = OnceLock::new();
        *ON.get_or_init(|| {
            std::env::var("AURALIS_CPU_PERF")
                .map(|v| !v.is_empty() && v != "0")
                .unwrap_or(false)
        })
    }

    /// Print a one-time confirmation that instrumentation is active (so the
    /// absence of later output can be attributed to "no painted frames" rather
    /// than "env var not set"). Prints unconditionally once for diagnosis, on
    /// stdout (matching the existing `[dirty-bench]` output).
    pub fn announce_once() {
        static SHOWN: AtomicBool = AtomicBool::new(false);
        if SHOWN.swap(true, Ordering::Relaxed) {
            return;
        }
        match std::env::var("AURALIS_CPU_PERF") {
            Ok(ref v) if !v.is_empty() && v != "0" => {
                println!(
                    "[cpu-perf] ENABLED (AURALIS_CPU_PERF={v:?}). Per-phase timings print every ~30 painted frames (or ~1s). Idle with no animation produces no painted frames — scroll / hover / type to generate them."
                );
            }
            _ => {} // silent when disabled or not set
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn record(
        gen_us: u64,
        raster_us: u64,
        present_us: u64,
        cmds: usize,
        text: usize,
        dmg_n: usize,
        dmg_frac: f32,
        pr_n: usize,
        rp_n: usize,
        w: f32,
        h: f32,
    ) {
        if !enabled() {
            return;
        }
        ACC.with(|a| {
            let mut a = a.borrow_mut();
            a.frames += 1;
            a.gen_us += gen_us;
            a.raster_us += raster_us;
            a.present_us += present_us;
            a.cmds += cmds as u64;
            a.text += text as u64;
            a.max_raster_us = a.max_raster_us.max(raster_us);
            a.dmg_rects += dmg_n as u64;
            a.dmg_frac += dmg_frac as f64;
            a.last_pr = pr_n;
            a.last_rp = rp_n;

            let now = Instant::now();
            let due_time = match a.last_flush {
                Some(t) => now.duration_since(t).as_millis() >= 1000,
                None => {
                    a.last_flush = Some(now);
                    false
                }
            };
            if a.frames >= 30 || due_time {
                let f = a.frames as u64;
                let total = (a.gen_us + a.raster_us + a.present_us) / f;
                let fps = if total > 0 { 1_000_000 / total } else { 0 };
                let dmg_pct = (a.dmg_frac / a.frames as f64) * 100.0;
                println!(
                    "[cpu-perf] {f}f avg | gen {}us  raster {}us (max {})  present {}us  | paint-total {total}us (~{fps} fps) | damage {:.1}% ({} rects, pr={} rp={}) | cmds {}  text {}  win {w:.0}x{h:.0}",
                    a.gen_us / f,
                    a.raster_us / f,
                    a.max_raster_us,
                    a.present_us / f,
                    dmg_pct,
                    a.dmg_rects / f,
                    a.last_pr,
                    a.last_rp,
                    a.cmds / f,
                    a.text / f,
                );
                a.frames = 0;
                a.gen_us = 0;
                a.raster_us = 0;
                a.present_us = 0;
                a.cmds = 0;
                a.text = 0;
                a.max_raster_us = 0;
                a.dmg_rects = 0;
                a.dmg_frac = 0.0;
                a.last_flush = Some(now);
            }
        });
    }
}

// Scrollbar axis moved to widgets/bundle/scroll.rs with the scrollbar
// geometry (audit round 5, phase 2).
use crate::widgets::bundle::scroll::ScrollAxis;

// ═══════════════════════ Scroll physics ═══════════════════════

const VELOCITY_HISTORY_MAX_MS: u128 = 120;
/// Pixels per mouse wheel notch (LineDelta). One notch ≈ 3 lines at ~13px/line on Windows.
pub(crate) const WHEEL_PIXELS_PER_LINE: f32 = 13.0;
const WHEEL_LINES_PER_NOTCH: f32 = 3.0;

pub(crate) use crate::core::frame_driver::ScrollKinetic;

/// Window's `FrameHook` impl: SEAM 1 (after-dirty) platform work — the hovered
/// submenu delayed-open, plus signaling whether a full relayout is forced.
const SUBMENU_DELAY_MS: u128 = 200;
const SUBMENU_TIMER_KEY: u64 = 0x5B6D;
pub(crate) struct WindowFrameHook<'a> {
    pub config: &'a WindowConfig,
    pub scale_factor: f64,
    pub winit_window: Option<&'a Arc<dyn winit::window::Window>>,
}

impl crate::core::frame_driver::FrameHook for WindowFrameHook<'_> {
    fn on_focus_transferred(&mut self, events: &EventRegistry, new_id: ElementId) {
        if let Some(w) = self.winit_window {
            request_ime_enable(w, events, new_id);
        }
    }
    fn on_after_dirty(&mut self, arena: &mut ElementArena, events: &mut EventRegistry) -> bool {
        // ── Hovered submenu: open after delay ──
        let mut open_sub = None;
        crate::core::app_context::current_app().hovered_submenu_with(|s| {
            if let Some((eid, since)) = s.as_ref() {
                if since.elapsed().as_millis() >= SUBMENU_DELAY_MS {
                    if let Some(el) = arena.get(*eid) {
                        let sb = el.screen_bounds;
                        let items = el
                            .get_user_data::<crate::widgets::overlay::ContextMenuItems>()
                            .cloned();
                        let screen_w = self.config.width / self.scale_factor as f32;
                        let screen_h = self.config.height / self.scale_factor as f32;
                        let parent = crate::core::dirty_registry::parent_of(*eid);
                        let prefer_left = parent
                            .and_then(|p| arena.get(p))
                            .and_then(|pel| {
                                pel.get_user_data::<crate::widgets::overlay::MenuOpenDir>()
                            })
                            .is_some_and(|d| d.0);
                        open_sub = items.map(|cmi| {
                            let sub_h =
                                cmi.0.iter().filter(|i| !i.separator).count().max(1) as f32 * 32.0;
                            let (sub_x, opened_left) =
                                submenu_x(sb.x, sb.width, screen_w, prefer_left);
                            let sub_y = submenu_y(sb.y, sb.height, sub_h, screen_h);
                            (
                                cmi.0,
                                crate::style::Point::new(sub_x, sub_y),
                                parent,
                                opened_left,
                            )
                        });
                    }
                    *s = None;
                    crate::core::scheduler::cancel(SUBMENU_TIMER_KEY);
                }
            }
        });
        if let Some((items, pos, parent, opened_left)) = open_sub {
            if let Some(rid) = arena.root_id {
                crate::widgets::overlay::open_context_menu(
                    items,
                    pos,
                    arena,
                    rid,
                    Some(events),
                    parent,
                    opened_left,
                    self.config.height / self.scale_factor as f32,
                );
                crate::widgets::overlay::mark_submenu_opened();
            }
        }

        // Force a full relayout if deferred actions / portal adds / submenu
        // open registered structural or dirty changes (window's needs_taffy).
        crate::core::dirty_registry::has_structurally_changed()
            || crate::core::dirty_registry::has_pending_dirty()
    }
}

// ═══════════════════════ WindowIcon ═══════════════════════

/// RGBA pixel data for a window icon.
#[derive(Clone)]
pub struct WindowIcon {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl WindowIcon {
    pub fn from_rgba(rgba: Vec<u8>, width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            rgba,
        }
    }

    #[cfg(feature = "ext-image")]
    pub fn from_image_bytes(bytes: &[u8]) -> Result<Self, String> {
        let img = image::load_from_memory(bytes).map_err(|e| e.to_string())?;
        let rgba = img.into_rgba8();
        Ok(Self {
            width: rgba.width(),
            height: rgba.height(),
            rgba: rgba.into_raw(),
        })
    }

    pub(crate) fn to_winit_icon(&self) -> winit::icon::Icon {
        let ri = winit::icon::RgbaIcon::new(self.rgba.clone(), self.width, self.height)
            .expect("WindowIcon: invalid RGBA data");
        winit::icon::Icon::from(ri)
    }
}

impl std::fmt::Debug for WindowIcon {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WindowIcon")
            .field("dimensions", &(self.width, self.height))
            .finish_non_exhaustive()
    }
}

// ═══════════════════════ WindowButtons ═══════════════════════

/// Which titlebar buttons are enabled (Close, Minimize, Maximize).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct WindowButtons(winit::window::WindowButtons);

impl WindowButtons {
    pub const CLOSE: Self = Self(winit::window::WindowButtons::CLOSE);
    pub const MINIMIZE: Self = Self(winit::window::WindowButtons::MINIMIZE);
    pub const MAXIMIZE: Self = Self(winit::window::WindowButtons::MAXIMIZE);
    pub const ALL: Self = Self(winit::window::WindowButtons::all());

    pub fn contains(self, other: Self) -> bool {
        self.0.contains(other.0)
    }
    pub fn with(mut self, other: Self) -> Self {
        self.0 |= other.0;
        self
    }
    pub fn without(mut self, other: Self) -> Self {
        self.0 -= other.0;
        self
    }

    pub(crate) fn inner(self) -> winit::window::WindowButtons {
        self.0
    }
}

impl Default for WindowButtons {
    fn default() -> Self {
        Self::ALL
    }
}

// ═══════════════════════ WindowHandle ═══════════════════════

/// A cloneable handle for runtime window control.
///
/// Obtained from [`MountContext::window_handle`] during widget mount,
/// or stored directly for imperative window operations.
#[derive(Clone)]
pub struct WindowHandle {
    pub(crate) window: Arc<dyn winit::window::Window>,
}

impl WindowHandle {
    pub fn id(&self) -> winit::window::WindowId {
        self.window.id()
    }

    pub fn set_visible(&self, v: bool) {
        self.window.set_visible(v);
    }
    pub fn is_visible(&self) -> Option<bool> {
        self.window.is_visible()
    }

    pub fn set_title(&self, title: &str) {
        self.window.set_title(title);
    }
    pub fn title(&self) -> String {
        self.window.title()
    }

    pub fn set_minimized(&self, v: bool) {
        self.window.set_minimized(v);
    }
    pub fn is_minimized(&self) -> Option<bool> {
        self.window.is_minimized()
    }

    pub fn set_maximized(&self, v: bool) {
        self.window.set_maximized(v);
    }
    pub fn is_maximized(&self) -> bool {
        self.window.is_maximized()
    }
    pub fn toggle_maximized(&self) {
        self.window.set_maximized(!self.window.is_maximized());
    }

    pub fn set_fullscreen(&self, v: bool) {
        if v {
            self.window
                .set_fullscreen(Some(winit::monitor::Fullscreen::Borderless(None)));
        } else {
            self.window.set_fullscreen(None);
        }
    }
    pub fn is_fullscreen(&self) -> bool {
        self.window.fullscreen().is_some()
    }
    pub fn toggle_fullscreen(&self) {
        self.set_fullscreen(!self.is_fullscreen());
    }

    pub fn set_resizable(&self, v: bool) {
        self.window.set_resizable(v);
    }
    pub fn is_resizable(&self) -> bool {
        self.window.is_resizable()
    }

    pub fn set_decorations(&self, v: bool) {
        self.window.set_decorations(v);
    }
    pub fn is_decorated(&self) -> bool {
        self.window.is_decorated()
    }

    pub fn set_transparent(&self, v: bool) {
        self.window.set_transparent(v);
    }

    pub fn set_min_inner_size(&self, size: Option<(f32, f32)>) {
        self.window.set_min_surface_size(
            size.map(|(w, h)| winit::dpi::LogicalSize::new(w as f64, h as f64).into()),
        );
    }
    pub fn set_max_inner_size(&self, size: Option<(f32, f32)>) {
        self.window.set_max_surface_size(
            size.map(|(w, h)| winit::dpi::LogicalSize::new(w as f64, h as f64).into()),
        );
    }

    pub fn set_window_icon(&self, icon: Option<&WindowIcon>) {
        self.window
            .set_window_icon(icon.map(WindowIcon::to_winit_icon));
    }

    pub fn set_always_on_top(&self, v: bool) {
        self.window.set_window_level(if v {
            winit::window::WindowLevel::AlwaysOnTop
        } else {
            winit::window::WindowLevel::Normal
        });
    }

    pub fn set_enabled_buttons(&self, buttons: WindowButtons) {
        self.window.set_enabled_buttons(buttons.inner());
    }

    pub fn set_theme(&self, theme: Option<winit::window::Theme>) {
        self.window.set_theme(theme);
    }
    pub fn theme(&self) -> Option<winit::window::Theme> {
        self.window.theme()
    }

    pub fn focus(&self) {
        self.window.focus_window();
    }

    pub fn close(&self) {
        self.window.set_visible(false);
    }

    pub fn drag_window(&self) {
        let _ = self.window.drag_window();
    }

    pub fn request_user_attention(&self, ty: Option<winit::window::UserAttentionType>) {
        self.window.request_user_attention(ty);
    }

    pub fn scale_factor(&self) -> f64 {
        self.window.scale_factor()
    }

    pub fn set_cursor(&self, icon: CursorIcon) {
        self.window
            .set_cursor(winit::cursor::Cursor::Icon(icon.inner()));
    }

    pub fn set_cursor_visible(&self, visible: bool) {
        self.window.set_cursor_visible(visible);
    }

    /// Monitor this window currently resides on.
    #[cfg(feature = "display")]
    pub fn current_monitor(&self) -> Option<super::display::MonitorHandle> {
        self.window
            .current_monitor()
            .map(super::display::MonitorHandle)
    }

    /// Enumerate all connected monitors.
    #[cfg(feature = "display")]
    pub fn available_monitors(&self) -> Vec<super::display::MonitorHandle> {
        self.window
            .available_monitors()
            .map(super::display::MonitorHandle)
            .collect()
    }

    /// Primary monitor.
    #[cfg(feature = "display")]
    pub fn primary_monitor(&self) -> Option<super::display::MonitorHandle> {
        self.window
            .primary_monitor()
            .map(super::display::MonitorHandle)
    }
}

impl std::fmt::Debug for WindowHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WindowHandle")
            .field("id", &self.window.id())
            .finish_non_exhaustive()
    }
}

// ═══════════════════════ WindowConfig ═══════════════════════

pub struct WindowConfig {
    pub title: String,
    pub width: f32,
    pub height: f32,
    pub theme: M3Theme,
    pub theme_signal: Option<Signal<M3Theme>>,
    pub backend: crate::render::RendererChoice,
    #[cfg(feature = "tray")]
    pub tray: Option<tray::TrayIconBuilder>,
    #[cfg(feature = "i18n")]
    pub i18n: Option<std::rc::Rc<crate::i18n::I18n>>,
    // ── Desktop-only window behavior ──
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    pub resizable: bool,
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    pub min_width: Option<f32>,
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    pub min_height: Option<f32>,
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    pub max_width: Option<f32>,
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    pub max_height: Option<f32>,
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    pub maximized: bool,
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    pub fullscreen: bool,
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    pub decorations: bool,
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    pub transparent: bool,
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    pub always_on_top: bool,
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    pub window_icon: Option<WindowIcon>,
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    pub enabled_buttons: WindowButtons,
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    pub visible: bool,
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    pub position: Option<(f32, f32)>,
    #[cfg(all(
        feature = "display",
        not(any(target_os = "android", target_os = "ios"))
    ))]
    pub monitor: Option<super::display::MonitorHandle>,
    // ── Mobile-only window behavior (reserved for future Android/iOS support) ──
    #[cfg(any(target_os = "android", target_os = "ios"))]
    pub immersive_mode: bool,
    // ── Scroll physics ──
    pub scroll_friction: f32,
    pub scroll_stop_speed: f32,
}

impl Default for WindowConfig {
    fn default() -> Self {
        Self {
            title: "Auralis UI".into(),
            width: 1024.0,
            height: 768.0,
            theme: M3Theme::from_seed(Color::rgba8(0x67, 0x79, 0xE8, 0xFF))
                .preset(crate::theme::PresetTheme::neo_minimal_slate()),
            theme_signal: None,
            backend: crate::render::RendererChoice::Auto,
            #[cfg(feature = "tray")]
            tray: None,
            #[cfg(feature = "i18n")]
            i18n: None,
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            resizable: true,
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            min_width: None,
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            min_height: None,
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            max_width: None,
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            max_height: None,
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            maximized: false,
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            fullscreen: false,
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            decorations: true,
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            transparent: false,
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            always_on_top: false,
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            window_icon: None,
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            enabled_buttons: WindowButtons::ALL,
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            visible: true,
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            position: None,
            #[cfg(all(
                feature = "display",
                not(any(target_os = "android", target_os = "ios"))
            ))]
            monitor: None,
            #[cfg(any(target_os = "android", target_os = "ios"))]
            immersive_mode: true,
            scroll_friction: 800.0,
            scroll_stop_speed: 30.0,
        }
    }
}

impl WindowConfig {
    pub fn auto_theme() -> Self {
        #[cfg(feature = "system-theme")]
        {
            let is_dark = dark_light::detect()
                .map(|m| matches!(m, dark_light::Mode::Dark))
                .unwrap_or(false);
            let seed = Color::rgba8(0x67, 0x79, 0xE8, 0xFF);
            let initial = if is_dark {
                M3Theme::from_seed(seed).with_is_dark(true)
            } else {
                M3Theme::from_seed(seed)
            };
            let sig = Signal::new(initial);
            Self {
                theme: sig.read(),
                theme_signal: Some(sig),
                ..Default::default()
            }
        }
        #[cfg(not(feature = "system-theme"))]
        {
            Self {
                theme: M3Theme::from_seed(Color::rgba8(0x67, 0x79, 0xE8, 0xFF)),
                ..Default::default()
            }
        }
    }
}

/// Application entry point.  Manages a collection of windows and routes
/// winit events to the correct [`WindowState`].
///
/// Create with [`App::new`], register windows with [`App::add_window`],
/// then call [`App::run`] to start the event loop.
pub struct App {
    windows: FxHashMap<winit::window::WindowId, WindowState>,
    pending: Vec<(WindowConfig, Box<dyn Widget>)>,
    flush_scheduler: Rc<DeferredScheduler>,
    max_windows: Option<usize>,
    #[cfg(feature = "global-hotkey")]
    hotkey_manager: crate::platform::global_hotkey::GlobalHotkeyManager,
    #[cfg(any(feature = "devtools", feature = "file-logging"))]
    #[allow(dead_code)]
    logging_guard: Option<crate::logging::LoggingGuard>,
    #[cfg(feature = "devtools")]
    devtools_buf: crate::debug::devtools::DevtoolsRingBuffer,
}

thread_local! {
    /// Queue of windows requested during widget callbacks.  Drained at the
    /// end of [`App::window_event`] so that `ActiveEventLoop` is still on
    /// the call stack and can create new winit windows.
    static PENDING_WINDOWS: std::cell::RefCell<Vec<(WindowConfig, Box<dyn Widget>)>>
        = std::cell::RefCell::new(Vec::new());
}

impl App {
    pub fn new() -> Self {
        let flush_scheduler = DeferredScheduler::new();
        auralis_task::init_flush_scheduler(
            flush_scheduler.clone() as Rc<dyn auralis_task::ScheduleFlush>
        );
        // Async timer axis (audit 2026-07-17 round 5, A1): without a
        // TimeSource the executor expires EVERY timer on the next flush —
        // `timer::sleep(n)` silently became `yield_now()` in production.
        auralis_task::init_time_source(Rc::new(crate::core::clock::ClockTimeSource::new()));
        #[cfg(feature = "devtools")]
        let devtools_buf = {
            let buf = crate::debug::devtools::new_ring_buffer();
            crate::debug::devtools::install_ring_buffer(buf.clone());
            crate::core::perf::perf_enable();
            crate::core::dirty_registry::set_dirty_trace_enabled(true);
            crate::debug::devtools::install_signal_observer();
            buf
        };
        Self {
            windows: FxHashMap::default(),
            pending: Vec::new(),
            flush_scheduler,
            max_windows: None,
            #[cfg(feature = "global-hotkey")]
            hotkey_manager: crate::platform::global_hotkey::GlobalHotkeyManager::new(),
            #[cfg(any(feature = "devtools", feature = "file-logging"))]
            logging_guard: None,
            #[cfg(feature = "devtools")]
            devtools_buf,
        }
    }

    /// Create an AppBuilder for configuring the application before running.
    pub fn builder() -> AppBuilder {
        AppBuilder {
            config: AppBuilderConfig::default(),
            pending: Vec::new(),
            max_windows: None,
        }
    }

    /// Set a maximum number of concurrent windows.  Calls to
    /// [`create_window`] beyond this limit are silently ignored.
    pub fn with_max_windows(mut self, limit: usize) -> Self {
        self.max_windows = Some(limit);
        self
    }

    /// Register a window to be created when the event loop starts.
    pub fn add_window(mut self, config: WindowConfig, widget: impl Widget + 'static) -> Self {
        self.pending.push((config, Box::new(widget)));
        self
    }

    /// Convenience alias for `add_window`.
    pub fn window(self, config: WindowConfig, widget: impl Widget + 'static) -> Self {
        self.add_window(config, widget)
    }

    /// Enable DevTools data collection (ring buffer, perf, dirty trace).
    /// No UI window is opened; data is accessible via the public `devtools` module.
    #[cfg(feature = "devtools")]
    pub fn devtools(self) -> Self {
        self
    }

    /// Toggle the DevTools window visibility (no-op — UI not yet shipped).
    #[cfg(feature = "devtools")]
    #[allow(dead_code)]
    fn toggle_devtools_window(&mut self) {}

    /// Start the event loop.  Blocks until all windows are closed.
    pub fn run(self) -> Result<(), winit::error::EventLoopError> {
        let event_loop = winit::event_loop::EventLoop::new()?;
        crate::platform::wake::set_ui_proxy(event_loop.create_proxy());
        #[cfg(feature = "async-tokio")]
        crate::task::init_tokio();
        event_loop.run_app(Box::new(self))
    }

    // ── Global hotkey API ────────────────────────────────────────────

    /// Register a system-global hotkey that fires even when the
    /// application window is not focused.
    ///
    /// # Example
    ///
    /// ```ignore
    /// app.register_global_hotkey("Ctrl+Shift+S", ActionKind::Custom { id: "screenshot" })?;
    /// ```
    #[cfg(feature = "global-hotkey")]
    pub fn register_global_hotkey(
        &mut self,
        chord_str: &str,
        action: ActionKind,
    ) -> Result<
        crate::platform::global_hotkey::HotkeyHandle,
        crate::platform::global_hotkey::GlobalHotkeyError,
    > {
        self.hotkey_manager.register(chord_str, action)
    }

    /// Unregister a global hotkey by its chord string.
    #[cfg(feature = "global-hotkey")]
    pub fn unregister_global_hotkey(
        &mut self,
        chord_str: &str,
    ) -> Result<(), crate::platform::global_hotkey::GlobalHotkeyError> {
        self.hotkey_manager.unregister_by_string(chord_str)
    }

    /// List all currently registered global hotkeys.
    #[cfg(feature = "global-hotkey")]
    pub fn list_global_hotkeys(&self) -> Vec<String> {
        self.hotkey_manager.list()
    }

    /// Check whether the global hotkey backend is available on this
    /// platform (e.g. macOS Accessibility permission granted).
    #[cfg(feature = "global-hotkey")]
    pub fn is_global_hotkey_available(&mut self) -> bool {
        self.hotkey_manager.is_available()
    }

    /// Get a human-readable description of required permissions for
    /// this platform when global hotkeys fail.
    #[cfg(feature = "global-hotkey")]
    pub fn global_hotkey_permission_guidance() -> &'static str {
        crate::platform::global_hotkey::GlobalHotkeyManager::permission_guidance()
    }
}

// ═══════════════════════ AppBuilder ═══════════════════════

/// Builder-pattern API for configuring [`App`] before running.
pub struct AppBuilder {
    config: AppBuilderConfig,
    pending: Vec<(WindowConfig, Box<dyn Widget>)>,
    max_windows: Option<usize>,
}

struct AppBuilderConfig {
    error_handler: Option<Rc<dyn Fn(&UiError)>>,
    fatal_handler: Option<Rc<dyn Fn(&UiError)>>,
    error_buffer_limit: usize,
    logging: Option<crate::logging::LoggingConfig>,
}

impl Default for AppBuilderConfig {
    fn default() -> Self {
        Self {
            error_handler: None,
            fatal_handler: None,
            error_buffer_limit: 128,
            logging: None,
        }
    }
}

impl AppBuilder {
    /// Register a global callback invoked for every [`UiError`].
    pub fn on_error(mut self, handler: impl Fn(&UiError) + 'static) -> Self {
        self.config.error_handler = Some(Rc::new(handler));
        self
    }

    /// Register a global callback invoked for fatal [`UiError`]s.
    ///
    /// **IMPORTANT:** The handler runs synchronously and blocks `push_error`.
    /// Do not perform UI operations (e.g., opening a dialog) because the
    /// window system may already be in an invalid state. The handler should
    /// limit itself to logging, flushing state to disk, and signalling an
    /// external watchdog to restart the process.
    pub fn on_fatal(mut self, handler: impl Fn(&UiError) + 'static) -> Self {
        self.config.fatal_handler = Some(Rc::new(handler));
        self
    }

    /// Set the maximum number of buffered errors (default 128).
    pub fn error_buffer(mut self, limit: usize) -> Self {
        self.config.error_buffer_limit = limit;
        self
    }

    /// Configure the tracing subscriber with the given config.
    /// Falls back to `RUST_LOG` env var when level is `None`.
    pub fn logging(mut self, config: crate::logging::LoggingConfig) -> Self {
        self.config.logging = Some(config);
        self
    }

    /// Set the log level filter (e.g. "info", "debug", "warn").
    /// Shorthand for `.logging(LoggingConfig { level: Some(level), .. })`.
    pub fn log_level(mut self, level: impl Into<String>) -> Self {
        self.config
            .logging
            .get_or_insert_with(Default::default)
            .level = Some(level.into());
        self
    }

    /// Set a maximum number of concurrent windows.
    pub fn with_max_windows(mut self, limit: usize) -> Self {
        self.max_windows = Some(limit);
        self
    }

    /// Register a window to be created when the event loop starts.
    pub fn window(mut self, config: WindowConfig, widget: impl Widget + 'static) -> Self {
        self.pending.push((config, Box::new(widget)));
        self
    }

    /// Consume the builder and produce an [`App`] ready to run.
    pub fn build(self) -> Result<App, UiError> {
        if let Some(ref handler) = self.config.error_handler {
            set_error_handler(handler.clone());
        }
        if let Some(ref handler) = self.config.fatal_handler {
            crate::core::error::set_fatal_handler(handler.clone());
        }
        set_error_buffer_limit(self.config.error_buffer_limit);

        auralis_task::set_panic_hook(Rc::new(|info: auralis_task::PanicInfo| {
            push_error(UiError::CallbackPanic {
                context: format!("auralis-task:task={},scope={}", info.task_id, info.scope_id),
                window_id: None,
                element_id: None,
                message: crate::core::error::panic_to_string(&info.payload),
            });

            #[cfg(any(feature = "devtools", feature = "file-logging"))]
            tracing::error!(
                target: "auralis-task",
                task_id = info.task_id,
                scope_id = info.scope_id,
                "task panicked"
            );
        }));

        let flush_scheduler = auralis_task::DeferredScheduler::new();
        auralis_task::init_flush_scheduler(
            flush_scheduler.clone() as Rc<dyn auralis_task::ScheduleFlush>
        );
        // Async timer axis — see App::new (A1).
        auralis_task::init_time_source(Rc::new(crate::core::clock::ClockTimeSource::new()));

        #[cfg(feature = "devtools")]
        let devtools_buf = {
            let buf = crate::debug::devtools::new_ring_buffer();
            crate::debug::devtools::install_ring_buffer(buf.clone());
            crate::core::perf::perf_enable();
            crate::core::dirty_registry::set_dirty_trace_enabled(true);
            crate::debug::devtools::install_signal_observer();
            buf
        };

        Ok(App {
            windows: FxHashMap::default(),
            pending: self.pending,
            flush_scheduler,
            max_windows: self.max_windows,
            #[cfg(feature = "global-hotkey")]
            hotkey_manager: crate::platform::global_hotkey::GlobalHotkeyManager::new(),
            #[cfg(any(feature = "devtools", feature = "file-logging"))]
            logging_guard: self.config.logging.map(crate::logging::init),
            #[cfg(feature = "devtools")]
            devtools_buf,
        })
    }
}

/// Request a new window from within a widget callback (button click, etc.).
///
/// The window is created on the next event-loop tick, not synchronously,
/// so the caller can continue without blocking.
///
/// Silently ignored if [`App::with_max_windows`] has been set and the limit
/// is already reached.
pub fn create_window(config: WindowConfig, widget: impl Widget + 'static) {
    PENDING_WINDOWS.with(|q| q.borrow_mut().push((config, Box::new(widget))));
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Default)]
struct FingerState {
    position: crate::style::Point,
    hovered_chain: Vec<ElementId>,
    pressed: Option<ElementId>,
}

/// State machine for in-app widget-to-widget drag-and-drop.
struct DragState {
    source: ElementId,
    payload: Option<DragData>,
    cursor: crate::style::Point,
    ghost: Option<ElementId>,
    hovered_target: Option<ElementId>,
}

/// Per-window state: arena, renderer, focus, event handling.
pub(crate) struct WindowState {
    config: WindowConfig,
    winit_window: Option<Arc<dyn winit::window::Window>>,
    renderer: Option<crate::render::BackendRenderer>,
    pub arena: ElementArena,
    pub app: std::rc::Rc<crate::core::app_context::AppContext>,
    scene_cache: std::cell::RefCell<
        rustc_hash::FxHashMap<ElementId, std::rc::Rc<crate::render::CachedScene>>,
    >,
    subtree_cache: std::cell::RefCell<
        rustc_hash::FxHashMap<ElementId, std::rc::Rc<crate::render::CachedSubtree>>,
    >,
    focus_manager: FocusManager,
    /// (element, original z_index_floor) for the currently dragged row so its
    /// raised render-layer can be restored on drag-end.
    drag_elevated: Option<(ElementId, Option<i32>)>,
    click_counter: ClickCounter,
    animations: AnimationDriver,
    translator: EventTranslator,
    event_registry: EventRegistry,
    taffy: TaffyBridge,
    last_cursor: Option<crate::style::Point>,
    touches: FxHashMap<u64, FingerState>,
    frame_id: u64,
    drag_text_input: Option<ElementId>,
    scrollbar_drag: Option<(ElementId, ScrollAxis, f32)>,
    scrollbar_hover: Option<(ElementId, ScrollAxis)>,
    scale_factor: f64,
    frame_stats_painted: u64,
    paint_occurred_this_frame: bool,
    needs_taffy: bool,
    needs_theme_reapply: bool,
    theme_signal: Option<Signal<M3Theme>>,
    a11y: A11yAdapter,
    key_bindings: KeyBindingMap,
    flush_scheduler: Rc<DeferredScheduler>,
    coalesce_skip_paint: bool,
    skip_frame: bool,
    close_requested: bool,
    needs_rebuild: bool,
    needs_cleanup: bool,
    drag_state: Option<DragState>,
    metrics: MetricsHistory,
    window_handle: Option<WindowHandle>,
    scroll_kinetic: Option<ScrollKinetic>,
    velocity_history: Vec<(f32, f32, StdInstant)>,
    scroll_kinetic_target: Option<ElementId>,
    last_frame_instant: Option<StdInstant>,
    #[cfg(feature = "tray")]
    tray_icon: Option<tray::TrayIcon>,
    /// Last IME cursor area sent to the platform (surface coords, logical px),
    /// maintained so we skip redundant Update requests (IME area dedup P1).
    last_sent_ime_area: Option<crate::style::Rect>,

    /// DevTools ring buffer handle (shared across all windows).
    #[cfg(feature = "devtools")]
    pub(crate) devtools_buf: Option<crate::debug::devtools::DevtoolsRingBuffer>,
    #[cfg(feature = "devtools")]
    pub(crate) last_frame_us: u64,
    #[cfg(feature = "devtools")]
    pub(crate) last_paint_us: u64,
}

fn finger_id_from_source(source: &winit::event::PointerSource) -> (u64, Option<u64>) {
    match source {
        winit::event::PointerSource::Touch { finger_id, .. } => {
            let id = finger_id.into_raw() as u64;
            (id, Some(id))
        }
        _ => (0, None),
    }
}

fn finger_id_from_button(button: &winit::event::ButtonSource) -> (u64, Option<u64>) {
    match button {
        winit::event::ButtonSource::Touch { finger_id, .. } => {
            let id = finger_id.into_raw() as u64;
            (id, Some(id))
        }
        _ => (0, None),
    }
}

fn finger_id_from_kind(kind: &winit::event::PointerKind) -> (u64, Option<u64>) {
    match kind {
        winit::event::PointerKind::Touch(fid) => {
            let id = fid.into_raw() as u64;
            (id, Some(id))
        }
        _ => (0, None),
    }
}

// ── A11y action data (stored in element user_data for widget polling) ──

// A11y marker user_data types moved to platform/a11y_bridge.rs
// (audit round 4 — dispatched from the shared SEAM-2 path).

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
            animations: AnimationDriver::new(),
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
            key_bindings: KeyBindingMap::new(),
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

    fn primary_state(&self) -> Option<&FingerState> {
        self.touches.get(&0)
    }
    #[allow(dead_code)]
    fn primary_hovered(&self) -> Option<ElementId> {
        self.primary_state()
            .and_then(|s| s.hovered_chain.first().copied())
    }
    fn primary_pressed(&self) -> Option<ElementId> {
        self.primary_state().and_then(|s| s.pressed)
    }
    fn finger_state(&mut self, fid: u64) -> &mut FingerState {
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

    fn mount_root(&mut self, widget: Box<dyn Widget>) {
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
                self.transfer_focus(focus_id, FocusReason::Programmatic);
            }
        }
        for focus_id in self.focus_manager.drain_autofocus() {
            if self.focus_manager.is_in_current_scope(focus_id) {
                self.transfer_focus(focus_id, FocusReason::Programmatic);
            }
        }
    }

    fn ensure_renderer(&mut self) {
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

    fn on_frame(&mut self) {
        crate::core::app_context::set_current_app(&self.app);
        crate::core::error::set_current_frame_id(Some(self.frame_id));
        // ── Freeze check: skip expensive work (layout, paint, animations) ──
        if self.app.is_frozen() {
            self.frame_id += 1;
            self.paint_occurred_this_frame = false;
            #[cfg(feature = "devtools")]
            if let Some(ref buf) = self.devtools_buf {
                if let Some(ref win) = self.winit_window {
                    let fps = self.metrics.latest().map(|m| m.fps).unwrap_or(0.0);
                    let snapshot = crate::debug::devtools::collect_frame_snapshot(
                        &self.arena,
                        self.frame_id,
                        0,
                        0,
                        fps,
                        0,
                        0,
                    );
                    crate::debug::devtools::push_snapshot(buf, win.id(), snapshot);
                }
            }
            crate::core::error::set_current_frame_id(None);
            return;
        }
        if self.needs_rebuild {
            self.rebuild_state();
        }
        if self.skip_frame {
            crate::core::error::set_current_frame_id(None);
            return;
        }
        #[cfg(feature = "backend-tiny-skia")]
        cpu_perf::announce_once();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let frame_start = StdInstant::now();
            self.frame_id += 1;

            // ── Perf activation (devtools auto-enables, otherwise opt-in via AURALIS_PERF=1) ──
            {
                use std::sync::OnceLock;
                static PERF_CHECKED: OnceLock<bool> = OnceLock::new();
                PERF_CHECKED.get_or_init(|| {
                    let enable = cfg!(feature = "devtools")
                        || std::env::var("AURALIS_PERF").is_ok_and(|v| v == "1");
                    if enable {
                        crate::core::perf::perf_enable();
                    }
                    enable
                });
            }
            crate::core::perf::perf_reset_frame();

            self.last_frame_instant = Some(frame_start);

            #[cfg(feature = "tray")]
            if let Some(ref mut tray) = self.tray_icon {
                tray.poll();
            }

            self.flush_scheduler.drain();
            auralis_task::drain_deferred_signal_callbacks();
            #[cfg(debug_assertions)]
            crate::core::dirty_registry::reset_stats();
            crate::core::dirty_registry::devtools_reset_dirty();

            let is_first_frame = self.frame_id == 1;
            let root_id = match self.arena.root_id {
                Some(rid) => rid,
                None => return,
            };

            // ── Theme reapply (pre-frame; forces a full relayout) ──
            if self.needs_theme_reapply {
                reapply_element_theme(&mut self.arena, root_id, &self.config.theme);
                self.needs_theme_reapply = false;
                self.needs_taffy = true;
            }
            let force_layout = self.needs_taffy;
            self.needs_taffy = false;

            // ── Frame input (divergence points explicit) ──
            let input = crate::core::frame_driver::FrameInput {
                size: Size::new(self.config.width, self.config.height),
                frame_id: self.frame_id,
                is_first_frame,
                force_layout,
                scale_factor: self.scale_factor as f32,
                bg: self.config.theme.scheme.surface,
                fg: self.config.theme.scheme.on_surface,
                highlight_mode: self.focus_manager.highlight_mode(),
                now: crate::core::clock::now(),
                scroll_friction: self.config.scroll_friction,
                scroll_stop_speed: self.config.scroll_stop_speed,
                skip_paint: self.coalesce_skip_paint,
            };

            // ── Phase 1: drive_frame_layout (kinetic → dirty → SEAM1 → layout) ──
            let mut hook = WindowFrameHook {
                config: &self.config,
                scale_factor: self.scale_factor,
                winit_window: self.winit_window.as_ref(),
            };
            let stage = {
                let st = crate::core::frame_driver::FrameState {
                    arena: &mut self.arena,
                    taffy: &mut self.taffy,
                    events: &mut self.event_registry,
                    animations: &mut self.animations,
                    focus: &mut self.focus_manager,
                    scroll_kinetic: &mut self.scroll_kinetic,
                    scroll_kinetic_target: &mut self.scroll_kinetic_target,
                };
                crate::core::frame_driver::drive_frame_layout(st, &input, &mut hook)
            };
            let root_id = stage.root_id;

            // ── SEAM 2 (shared platform-frame work: long-press wins, drag
            //   ghost, drag-z, autofocus, a11y dispatch — same code path as
            //   TestHarness::run_frame; audit round 4) ──
            {
                let st = crate::core::frame_driver::FrameState {
                    arena: &mut self.arena,
                    taffy: &mut self.taffy,
                    events: &mut self.event_registry,
                    animations: &mut self.animations,
                    focus: &mut self.focus_manager,
                    scroll_kinetic: &mut self.scroll_kinetic,
                    scroll_kinetic_target: &mut self.scroll_kinetic_target,
                };
                let args = crate::core::frame_driver::PlatformArgs {
                    drag_ghost: self
                        .drag_state
                        .as_ref()
                        .and_then(|ds| ds.ghost.map(|g| (g, ds.cursor))),
                    drag_elevated: &mut self.drag_elevated,
                };
                crate::core::frame_driver::drive_frame_platform(
                    st, &stage, &input, args, &mut hook,
                );
            }
            drop(hook);

            // ── Phase 2: drive_frame_paint (animation → paint → exits) ──
            let paint_start = StdInstant::now();
            let out = {
                let fcx = crate::core::frame_context::FrameContext::new(
                    &self.app,
                    &self.scene_cache,
                    &self.subtree_cache,
                );
                let st = crate::core::frame_driver::FrameState {
                    arena: &mut self.arena,
                    taffy: &mut self.taffy,
                    events: &mut self.event_registry,
                    animations: &mut self.animations,
                    focus: &mut self.focus_manager,
                    scroll_kinetic: &mut self.scroll_kinetic,
                    scroll_kinetic_target: &mut self.scroll_kinetic_target,
                };
                crate::core::frame_driver::drive_frame_paint(st, &fcx, &input, stage)
            };

            self.paint_occurred_this_frame = out.painted;

            // Startup probe — fires exactly once, on the first frame that
            // actually painted. env AURALIS_STARTUP_PROBE=1 enables it.
            #[cfg(feature = "backend-tiny-skia")]
            if self.paint_occurred_this_frame {
                static FIRED: std::sync::atomic::AtomicBool =
                    std::sync::atomic::AtomicBool::new(false);
                if !FIRED.swap(true, std::sync::atomic::Ordering::Relaxed)
                    && std::env::var("AURALIS_STARTUP_PROBE").is_ok()
                {
                    use std::sync::OnceLock;
                    static T0: OnceLock<std::time::Instant> = OnceLock::new();
                    let t0 = *T0.get_or_init(std::time::Instant::now);
                    eprintln!(
                        "[STARTUP] first painted frame (id={}) at {:.0} ms",
                        self.frame_id,
                        t0.elapsed().as_millis()
                    );
                    std::process::exit(0);
                }
            }

            if out.painted && self.renderer.is_some() {
                self.frame_stats_painted += 1;
                let repaint_ids = out.repaint_ids;
                let dmg_pr_n = out.paint_roots.len();
                let dmg_rp_n = repaint_ids.len();
                let mut commands = out.commands;
                let mut text_areas = out.text_areas;
                let backdrop_regions = out.backdrop_regions;

                if commands.is_empty() && text_areas.is_empty() {
                    // Skip present — nothing to render, avoid overwriting
                    // previously rendered content (e.g. during enter-animation frames).
                } else {
                    match &mut self.renderer {
                        #[cfg(feature = "backend-tiny-skia")]
                        Some(crate::render::BackendRenderer::Cpu(ref mut cpu)) => {
                            // gen phase = everything since paint_start (paint_element_tree + take_commands)
                            let gen_us = paint_start.elapsed().as_micros() as u64;
                            let cmd_count = commands.len();
                            let ta_count = text_areas.len();

                            // Only elements that actually have REPAINT set (filter
                            // out "repaint-boundary" solid-bg containers that are in
                            // paint_roots but whose own surface did not change).
                            // `repaint_ids` was captured BEFORE paint_element_tree
                            // cleared the dirty flags.
                            let pr_n = dmg_pr_n;
                            let rp_n = dmg_rp_n;
                            let viewport =
                                Rect::new(0.0, 0.0, self.config.width, self.config.height);
                            let damage_rects: Vec<Rect> = {
                                // Inflated (shadow/outline/transform-aware), disjoint
                                // damage rects with previous-position tracking —
                                // see render::cpu::damage (audit 2026-07-16 C5/C6).
                                let raw = if !repaint_ids.is_empty() {
                                    cpu.damage_tracker.compute(
                                        &repaint_ids,
                                        &self.arena,
                                        8,
                                        viewport,
                                    )
                                } else {
                                    vec![]
                                };
                                if raw.is_empty() {
                                    vec![viewport]
                                } else {
                                    raw
                                }
                            };

                            let dmg_n = damage_rects.len();
                            let dmg_frac = {
                                let win_area = (self.config.width * self.config.height).max(1.0);
                                let dmg_area: f32 =
                                    damage_rects.iter().map(|r| r.width * r.height).sum();
                                dmg_area / win_area
                            };

                            let t_raster = StdInstant::now();
                            cpu.render_damage(
                                &damage_rects,
                                &mut commands,
                                &mut text_areas,
                                &backdrop_regions,
                            );
                            let raster_us = t_raster.elapsed().as_micros() as u64;
                            let t_present = StdInstant::now();
                            cpu.present(&damage_rects);
                            let present_us = t_present.elapsed().as_micros() as u64;
                            cpu.end_frame();
                            cpu_perf::record(
                                gen_us,
                                raster_us,
                                present_us,
                                cmd_count,
                                ta_count,
                                dmg_n,
                                dmg_frac,
                                pr_n,
                                rp_n,
                                self.config.width,
                                self.config.height,
                            );
                        }
                        #[cfg(feature = "backend-wgpu")]
                        Some(crate::render::BackendRenderer::Gpu(ref mut gpu)) => {
                            match gpu.begin_frame() {
                                Ok(mut frame) => {
                                    gpu.draw_commands(
                                        &mut frame,
                                        &commands,
                                        &text_areas,
                                        &backdrop_regions,
                                        Size::new(self.config.width, self.config.height),
                                    );
                                    gpu.end_frame(frame);
                                }
                                Err(e) => {
                                    push_error(UiError::GpuRender(e.to_string()));
                                }
                            }
                        }
                        _ => {}
                    }
                } // else block for commands.is_empty() guard

                #[cfg(debug_assertions)]
                if self.frame_id > 2 {
                    let (dc, _ps) = crate::core::dirty_registry::stats();
                    let ec = crate::core::dirty_registry::element_count();
                    let now = StdInstant::now();
                    let layout_us = if crate::core::perf::perf_is_enabled() {
                        crate::core::perf::perf_take_frame().phases
                            [crate::core::perf::PerfPhase::Layout as usize]
                    } else {
                        0
                    };
                    self.metrics.push(FrameMetrics {
                        frame_id: self.frame_id,
                        element_count: ec,
                        dirty_measure_count: 0,
                        dirty_reposition_count: 0,
                        dirty_repaint_count: dc,
                        layout_time_us: layout_us,
                        paint_time_us: (now - paint_start).as_micros() as u64,
                        total_time_us: (now - frame_start).as_micros() as u64,
                        fps: {
                            let us = (now - frame_start).as_micros().max(1) as f32;
                            1_000_000.0 / us
                        },
                    });
                    let paint_us = (now - paint_start).as_micros() as u64;
                    let total_us = (now - frame_start).as_micros() as u64;
                    println!(
                        "[dirty-bench] frame#{} | tree={} | dirty={} | paint={}µs | total={}µs",
                        self.frame_id, ec, dc, paint_us, total_us
                    );
                    let hl_count = crate::core::dirty_registry::hittest_leaf_fallback_count();
                    if hl_count > 0 {
                        eprintln!(
                            "[hittest] hit_test_leaf O(N) fallback triggered {} time(s) this frame",
                            hl_count
                        );
                    }
                }
            }

            // ── Accessibility (caller-side; window uses its platform adapter) ──
            let focus_id = self.focus_manager.focused();
            if crate::core::dirty_registry::is_a11y_dirty() {
                self.a11y.update_if_active(|| {
                    crate::platform::build_accessibility_tree(&self.arena, root_id, focus_id)
                });
                crate::core::dirty_registry::clear_a11y_dirty();
            }

            self.focus_manager.check_alive(|_id| true);

            #[cfg(debug_assertions)]
            for w in crate::debug::drain_over_render_warnings() {
                eprintln!("{w}");
            }
            #[cfg(feature = "devtools")]
            {
                self.last_frame_us = frame_start.elapsed().as_micros() as u64;
                self.last_paint_us = paint_start.elapsed().as_micros() as u64;
            }
        }));
        if let Err(panic) = result {
            push_error(UiError::CallbackPanic {
                context: "on_frame".into(),
                window_id: self
                    .window_handle
                    .as_ref()
                    .map(|h| h.id().into_raw() as u64),
                element_id: None,
                message: panic_to_string(&panic),
            });
            self.needs_rebuild = true;
        }
        #[cfg(feature = "devtools")]
        if let Some(ref buf) = self.devtools_buf {
            if let Some(ref win) = self.winit_window {
                let fps = self.metrics.latest().map(|m| m.fps).unwrap_or_else(|| {
                    if let Some(prev) = self.last_frame_instant {
                        let us = prev.elapsed().as_micros().max(1) as f32;
                        1_000_000.0 / us
                    } else {
                        0.0
                    }
                });
                let snapshot = crate::debug::devtools::collect_frame_snapshot(
                    &self.arena,
                    self.frame_id,
                    0,
                    self.frame_stats_painted as usize,
                    fps,
                    self.last_frame_us,
                    self.last_paint_us,
                );
                let display_text =
                    format!(
                    "Frame #{} | {:.1} fps | {} elements | {} dirty | paint: {}µs | cache: {:.0}%",
                    self.frame_id, fps,
                    snapshot.element_count, snapshot.dirty_count,
                    snapshot.frame_timing.paint_total_us, snapshot.cache_stats.hit_rate(),
                );
                crate::debug::devtools::push_snapshot(buf, win.id(), snapshot);
                crate::debug::devtools::notify_display(display_text);
            }
        }
        crate::core::error::set_current_frame_id(None);
    }

    fn dispatch_events(&mut self, events: &[crate::event::Event]) {
        for evt in events {
            match evt {
                crate::event::Event::PointerDown { position, .. } => {
                    // Reuse the hit-test's ancestor path (B4) instead of
                    // rebuilding it via path_to_root.
                    let (hit, hit_path) = match self.primary_pressed() {
                        Some(t) => (Some(t), None),
                        None => match crate::event::hit_test::hit_test(&self.arena, *position) {
                            Some(r) => (Some(r.target), Some(r.path)),
                            None => (None, None),
                        },
                    };
                    if let Some(target) = hit {
                        // If element is draggable, start drag state — skip text_input capture
                        if self.arena.get(target).is_some_and(|el| el.draggable()) {
                            self.start_drag(target, *position);
                        } else {
                            let path = hit_path.unwrap_or_else(|| self.arena.path_to_root(target));
                            let outcome = crate::event::propagation::dispatch_event(
                                &mut self.arena,
                                evt,
                                &path,
                                &mut self.focus_manager,
                                &mut self.event_registry,
                                self.translator.modifiers(),
                            );
                            // If gesture arena determined this is a drag, capture the element
                            if let Some(drag_elem) = outcome.drag_winner {
                                self.drag_text_input = Some(drag_elem);
                            } else {
                                self.drag_text_input = Some(target);
                            }
                        }
                    }
                    if self.arena.root_id.is_some() {
                        // Overlay dismiss (portal system — Select, ComboBox, etc.)
                        crate::platform::portal::fire_dismiss(&self.arena, *position);
                    }
                }
                crate::event::Event::PointerMove { position, .. } => {
                    let has_drag = self.drag_state.is_some();
                    if has_drag {
                        self.update_drag_cursor(*position);
                    } else if let Some(id) = self.drag_text_input {
                        let path = self.arena.path_to_root(id);
                        let modifiers = self.translator.modifiers();
                        let _outcome = crate::event::propagation::dispatch_event(
                            &mut self.arena,
                            evt,
                            &path,
                            &mut self.focus_manager,
                            &mut self.event_registry,
                            modifiers,
                        );
                    }
                }
                crate::event::Event::PointerUp { position: _, .. } => {
                    if let Some(id) = self.drag_text_input {
                        let path = self.arena.path_to_root(id);
                        let modifiers = self.translator.modifiers();
                        let _outcome = crate::event::propagation::dispatch_event(
                            &mut self.arena,
                            evt,
                            &path,
                            &mut self.focus_manager,
                            &mut self.event_registry,
                            modifiers,
                        );
                        if let Some(el) = self.arena.get_mut(id) {
                            el.set_state_dirty(StateFlags::PRESSED, false);
                        }
                    }
                    self.end_drag();
                    self.drag_text_input = None;
                }
                crate::event::Event::Click {
                    position,
                    finger_id,
                    ..
                } => {
                    // Gesture-arena click suppression (mobile-groundwork
                    // W2): a non-Tap win (scroll/drag/long-press) on this
                    // pointer sequence eats the synthesized Click —
                    // scrolling over a button must not press it.
                    let pid = finger_id.unwrap_or(0);
                    if crate::event::recognizer::take_click_suppressed(pid) {
                        continue;
                    }
                    let modifiers = self.translator.modifiers();
                    if let Some(result) = crate::event::hit_test::hit_test(&self.arena, *position) {
                        let _ = crate::event::propagation::dispatch_event(
                            &mut self.arena,
                            evt,
                            &result.path,
                            &mut self.focus_manager,
                            &mut self.event_registry,
                            modifiers,
                        );
                    }
                    crate::platform::portal::fire_dismiss(&self.arena, *position);
                }
                crate::event::Event::KeyDown { .. } | crate::event::Event::KeyUp { .. } => {
                    if let Some(fid) = self.focus_manager.focused() {
                        let path = self.arena.path_to_root(fid);
                        let modifiers = self.translator.modifiers();
                        let _ = crate::event::propagation::dispatch_event(
                            &mut self.arena,
                            evt,
                            &path,
                            &mut self.focus_manager,
                            &mut self.event_registry,
                            modifiers,
                        );
                    }
                }
                crate::event::Event::DragEnd { .. } => {
                    if self.drag_state.is_some() {
                        self.end_drag();
                    } else if let Some(pid) = self.primary_pressed() {
                        let container_id = dirty_registry::parent_of(pid);
                        if let Some(cid) = container_id {
                            apply_drag_layouts(&mut self.arena, cid);
                            if let Some(el) = self.arena.get_mut(cid) {
                                el.dirty.set(el.dirty.get() | DirtyFlags::MEASURE);
                            }
                            dirty_registry::register_dirty(cid, DirtyFlags::MEASURE);
                        }
                        if let Some(state) = self.touches.get_mut(&0) {
                            state.pressed = None;
                        }
                    }
                }
                crate::event::Event::DragCancel { .. } => {
                    self.end_drag();
                    self.drag_text_input = None;
                }
                crate::event::Event::DragStart { .. } | crate::event::Event::DragMove { .. } => {
                    // Handled via PointerDown/Move capture + drag_text_input routing;
                    // DragEnd is handled above (applies drag layouts).
                }
                _ => {}
            }
        }
    }

    fn start_drag(&mut self, source: ElementId, cursor: crate::style::Point) {
        let payload = self
            .arena
            .get(source)
            .and_then(|el| el.on_drag_start().map(|ref f| f()))
            .or_else(|| self.arena.get(source).and_then(|el| el.drag_data()));
        let ghost = self.create_drag_ghost(source, &payload);
        self.drag_state = Some(DragState {
            source,
            payload,
            cursor,
            ghost,
            hovered_target: None,
        });
    }

    fn create_drag_ghost(
        &mut self,
        source: ElementId,
        payload: &Option<DragData>,
    ) -> Option<ElementId> {
        let label = payload
            .as_ref()
            .and_then(|p| p.label.clone())
            .or_else(|| payload.as_ref().and_then(|p| p.text.clone()))
            .or_else(|| self.arena.get(source).and_then(|el| el.accessible_label()))
            .unwrap_or_else(|| "↕".into());
        let theme = self.config.theme.clone();
        let ghost_id = self.arena.allocate();
        {
            let el = self.arena.get_mut(ghost_id).unwrap();
            el.set_background(theme.scheme.surface.with_alpha(0.85));
            el.set_foreground(theme.scheme.on_surface);
            el.set_border_width(1.0);
            el.set_border_color(theme.scheme.outline_variant);
            el.set_corner_radius(6.0);
            el.set_opacity(0.9);
            el.set_z_index(500);
            el.set_preferred_width(Some(140.0));
            el.set_preferred_height(28.0);
            el.set_padding(crate::style::Padding::all(6.0));
            el.set_input_pass_through(true);
            el.set_affected_by_child_size(false);
            el.set_preferred_width(Some(0.0));
            el.set_preferred_height(0.0);
            el.set_flex_grow(0.0);
            el.set_flex_shrink(0.0);
            el.set_visible(true);
        }
        let buf = create_buffer(
            &label,
            12.0,
            1.3,
            400,
            None,
            None,
            crate::style::TextAlign::Center,
        );
        {
            let el = self.arena.get_mut(ghost_id).unwrap();
            el.set_text_buffer(std::rc::Rc::new(std::cell::RefCell::new(buf)));
            el.set_text_generation(std::rc::Rc::new(std::cell::Cell::new(1u64)));
            el.set_accessible_label(label);
            el.mark_repaint();
            dirty_registry::register_dirty(ghost_id, DirtyFlags::REPAINT);
        }
        if let Some(root) = self.arena.root_id {
            self.arena.add_child(root, ghost_id);
        }
        Some(ghost_id)
    }

    fn update_drag_cursor(&mut self, cursor: crate::style::Point) {
        if self.drag_state.is_none() {
            return;
        }
        self.drag_state.as_mut().unwrap().cursor = cursor;
        self.update_drag_hover();
        let ghost_id = self.drag_state.as_ref().and_then(|ds| ds.ghost);
        if let Some(ghost) = ghost_id {
            let x = self.drag_state.as_ref().map_or(0.0, |ds| ds.cursor.x);
            let y = self.drag_state.as_ref().map_or(0.0, |ds| ds.cursor.y);
            if let Some(el) = self.arena.get_mut(ghost) {
                let rect = crate::style::Rect::new(x + 12.0, y + 12.0, 140.0, 28.0);
                el.screen_bounds = rect;
                el.set_bounds(rect);
                crate::core::dirty_registry::update_bounds(ghost, rect);
                el.mark_repaint();
            }
        }
    }

    fn update_drag_hover(&mut self) {
        let ds = match self.drag_state.as_mut() {
            Some(s) => s,
            None => return,
        };
        let old_target = ds.hovered_target;
        let ghost_id = ds.ghost;
        let hit = dirty_registry::hit_test_with_fallback(&self.arena, ds.cursor);

        // Walk ancestors to find the nearest drop_target container.
        let mut new_target = None;
        let mut cur = hit;
        while let Some(id) = cur {
            if Some(id) == ghost_id {
                break;
            }
            if id == ds.source {
                break;
            }
            if self.arena.get(id).is_some_and(|el| el.drop_target()) {
                // Validate accept_drop_types against payload kind
                let accepted =
                    if let (Some(el), Some(ref payload)) = (self.arena.get(id), &ds.payload) {
                        let types = el.accept_drop_types();
                        types.is_empty() || types.iter().any(|dt| dt.matches(&payload.kind))
                    } else {
                        true
                    };
                if accepted {
                    new_target = Some(id);
                    break;
                }
            }
            cur = dirty_registry::parent_of(id);
        }

        if old_target != new_target {
            if let Some(old) = old_target {
                if let Some(el) = self.arena.get_mut(old) {
                    el.set_state_dirty(StateFlags::DRAG_OVER, false);
                }
            }
            if let Some(new) = new_target {
                if let Some(el) = self.arena.get_mut(new) {
                    el.set_state_dirty(StateFlags::DRAG_OVER, true);
                }
            }
        }
        ds.hovered_target = new_target;
    }

    fn end_drag(&mut self) {
        if let Some(ds) = self.drag_state.take() {
            if let Some(old) = ds.hovered_target {
                if let Some(el) = self.arena.get_mut(old) {
                    el.set_state_dirty(StateFlags::DRAG_OVER, false);
                }
            }
            if let Some(ghost) = ds.ghost {
                self.arena.remove(ghost);
            }
            if let Some(target) = ds.hovered_target {
                if let Some(mut payload) = ds.payload {
                    payload.position = Some(ds.cursor);
                    if let Some(el) = self.arena.get(target) {
                        if let Some(ref handler) = el.on_drop() {
                            handler(payload);
                        }
                    }
                }
            }
        }
    }

    fn dispatch_action(&mut self, action: &Action, path: &[ElementId]) {
        let outcome = crate::event::propagation::dispatch_action(
            &mut self.arena,
            action,
            path,
            &mut self.event_registry,
            &[],
        );
        if !outcome.is_handled() {
            match action.kind {
                ActionKind::Activate | ActionKind::NewLine => {
                    if let Some(fid) = self.focus_manager.focused() {
                        self.event_registry.fire_click(fid);
                    }
                }
                ActionKind::FocusNext | ActionKind::InsertTab => {
                    // Tab closes any open context menu, then performs normal
                    // focus traversal. (RovingTabindex menus have a single Tab
                    // stop, so "wrapping inside" would be a no-op; standard
                    // desktop menus dismiss on Tab.)
                    crate::widgets::overlay::dismiss_context_menu_immediate(&mut self.arena);
                    if let Some(next_id) = self.focus_manager.focus_next(&self.arena) {
                        self.transfer_focus(next_id, FocusReason::TabNavigation);
                    }
                }
                ActionKind::FocusPrev => {
                    crate::widgets::overlay::dismiss_context_menu_immediate(&mut self.arena);
                    if let Some(prev_id) = self.focus_manager.focus_prev(&self.arena) {
                        self.transfer_focus(prev_id, FocusReason::TabNavigation);
                    }
                }
                ActionKind::MoveDown => {
                    if let Some(next_id) = self
                        .focus_manager
                        .focus_in_direction(&self.arena, Direction::Down)
                    {
                        self.transfer_focus(next_id, FocusReason::TabNavigation);
                    }
                }
                ActionKind::MoveUp => {
                    if let Some(next_id) = self
                        .focus_manager
                        .focus_in_direction(&self.arena, Direction::Up)
                    {
                        self.transfer_focus(next_id, FocusReason::TabNavigation);
                    }
                }
                ActionKind::MoveLeft => {
                    if let Some(next_id) = self
                        .focus_manager
                        .focus_in_direction(&self.arena, Direction::Left)
                    {
                        self.transfer_focus(next_id, FocusReason::TabNavigation);
                    }
                }
                ActionKind::MoveRight => {
                    if let Some(next_id) = self
                        .focus_manager
                        .focus_in_direction(&self.arena, Direction::Right)
                    {
                        self.transfer_focus(next_id, FocusReason::TabNavigation);
                    }
                }
                ActionKind::Copy => {
                    if let Some(fid) = self.focus_manager.focused() {
                        #[cfg(feature = "clipboard")]
                        if let Some(text) = self.event_registry.fire_clipboard_copy(fid) {
                            if !text.is_empty() {
                                if let Err(e) = crate::platform::Clipboard.write_text(&text) {
                                    push_error(UiError::Clipboard(e));
                                }
                            }
                        }
                    }
                }
                ActionKind::Cut => {
                    if let Some(fid) = self.focus_manager.focused() {
                        #[cfg(feature = "clipboard")]
                        if let Some(text) = self.event_registry.fire_clipboard_copy(fid) {
                            if !text.is_empty() {
                                if let Err(e) = crate::platform::Clipboard.write_text(&text) {
                                    push_error(UiError::Clipboard(e));
                                }
                            }
                        }
                        let del_action = Action::new(ActionKind::DeleteForward);
                        let path = self.arena.path_to_root(fid);
                        let _ = crate::event::propagation::dispatch_action(
                            &mut self.arena,
                            &del_action,
                            &path,
                            &mut self.event_registry,
                            &[],
                        );
                    }
                }
                ActionKind::Paste => {
                    if let Some(fid) = self.focus_manager.focused() {
                        match crate::platform::Clipboard.read_text() {
                            Ok(Some(text)) => self.event_registry.fire_clipboard_paste(fid, text),
                            Ok(None) => {}
                            #[cfg(feature = "clipboard")]
                            Err(e) => push_error(UiError::Clipboard(e)),
                            #[cfg(not(feature = "clipboard"))]
                            Err(_) => {} // NotAvailable is expected with the feature off
                        }
                    }
                }
                ActionKind::Undo | ActionKind::Redo => {
                    if let Some(fid) = self.focus_manager.focused() {
                        if let Some(state) = self.arena.get(fid).and_then(|el| {
                            el.get_user_data::<crate::core::undo::ElementUndoState>()
                        }) {
                            match action.kind {
                                ActionKind::Undo => {
                                    state.undo_all();
                                }
                                ActionKind::Redo => {
                                    state.redo_all();
                                }
                                _ => {}
                            }
                        }
                    }
                }
                ActionKind::Cancel => {
                    if let Some(root_id) = self.arena.root_id {
                        let path = cancel_path_for_visible(&self.arena, root_id);
                        if !path.is_empty() {
                            let _ = crate::event::propagation::dispatch_action(
                                &mut self.arena,
                                action,
                                &path,
                                &mut self.event_registry,
                                &[],
                            );
                        }
                    }
                }
                _ => {}
            }
        }
    }

    fn transfer_focus(&mut self, new_id: ElementId, reason: FocusReason) {
        crate::event::focus_manager::transfer_focus(
            &mut self.arena,
            &mut self.event_registry,
            &mut self.focus_manager,
            new_id,
            reason,
        );
        if let Some(ref w) = self.winit_window {
            request_ime_enable(w, &self.event_registry, new_id);
        }
    }

    /// Force relayout + repaint of the whole UI after a context-menu portal is
    /// removed during the event phase. `arena.remove` marks the parent for
    /// relayout but NOT repaint, so the overlay's pixels would otherwise linger
    /// until some later event happens to trigger a repaint. This mirrors the
    /// repaint side-effect an action callback would normally cause via signals.
    fn invalidate_after_menu_change(&mut self) {
        self.needs_taffy = true;
        if let Some(rid) = self.arena.root_id {
            crate::core::dirty_registry::mark_dirty(rid, crate::core::element::DirtyFlags::REPAINT);
            crate::core::dirty_registry::register_dirty(
                rid,
                crate::core::element::DirtyFlags::REPAINT,
            );
            crate::core::dirty_registry::bump_subtree_gen(rid);
        }
        if let Some(ref w) = self.winit_window {
            w.request_redraw();
        }
    }

    // a11y_scroll / scroll_focused_into_view moved to event/focus_manager.rs
    // (audit round 4 - shared by the SEAM-2 path and the harness).

    #[allow(dead_code)]
    pub fn animations(&mut self) -> &mut crate::animation::AnimationDriver {
        &mut self.animations
    }
    fn rebuild_state(&mut self) {
        self.needs_rebuild = false;
        self.needs_taffy = true;
        self.frame_id = 0;
        self.scroll_kinetic = None;
        self.drag_state = None;
        self.touches.clear();
        self.focus_manager.clear();
        crate::core::dirty_registry::mark_all_dirty();
        self.skip_frame = false;
    }
    #[allow(dead_code)]
    pub fn painter(&self) -> Painter {
        Painter::new(Rect::new(0.0, 0.0, self.config.width, self.config.height))
    }
}

impl App {
    fn create_window_inner(
        &mut self,
        event_loop: &dyn ActiveEventLoop,
        config: WindowConfig,
        widget: Box<dyn Widget>,
    ) {
        if let Some(max) = self.max_windows {
            if self.windows.len() >= max {
                #[cfg(any(feature = "devtools", feature = "file-logging"))]
                tracing::warn!("Window limit ({max}) reached; ignoring create_window call");
                return;
            }
        }
        let mut win_attrs = winit::window::WindowAttributes::default()
            .with_title(&config.title)
            .with_surface_size(winit::dpi::LogicalSize::new(
                config.width as f64,
                config.height as f64,
            ))
            .with_visible(false);

        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        {
            win_attrs = win_attrs
                .with_resizable(config.resizable)
                .with_decorations(config.decorations)
                .with_transparent(config.transparent)
                .with_maximized(config.maximized)
                .with_enabled_buttons(config.enabled_buttons.inner());
            if config.always_on_top {
                win_attrs = win_attrs.with_window_level(winit::window::WindowLevel::AlwaysOnTop);
            }
            if config.fullscreen {
                #[allow(unused_mut)]
                let mut fs_monitor: Option<winit::monitor::MonitorHandle> = None;
                #[cfg(feature = "display")]
                if let Some(ref m) = config.monitor {
                    fs_monitor = Some(m.inner().clone());
                }
                win_attrs = win_attrs
                    .with_fullscreen(Some(winit::monitor::Fullscreen::Borderless(fs_monitor)));
            }
            if let Some(ref icon) = config.window_icon {
                win_attrs = win_attrs.with_window_icon(Some(icon.to_winit_icon()));
            }
            if let Some((x, y)) = config.position {
                win_attrs =
                    win_attrs.with_position(winit::dpi::LogicalPosition::new(x as f64, y as f64));
            }
            if let Some(min_w) = config.min_width.or(config.min_height.map(|_| 0.0)) {
                let min_h = config.min_height.unwrap_or(0.0);
                win_attrs = win_attrs.with_min_surface_size(winit::dpi::LogicalSize::new(
                    min_w as f64,
                    min_h as f64,
                ));
            }
            if let Some(max_w) = config.max_width.or(config.max_height.map(|_| f32::MAX)) {
                let max_h = config.max_height.unwrap_or(f32::MAX);
                win_attrs = win_attrs.with_max_surface_size(winit::dpi::LogicalSize::new(
                    max_w as f64,
                    max_h as f64,
                ));
            }
        }

        #[cfg(all(
            feature = "display",
            not(any(target_os = "android", target_os = "ios"))
        ))]
        if config.position.is_none() {
            if let Some(ref monitor) = config.monitor {
                if let Some(pos) = monitor.position() {
                    let sf = monitor.scale_factor();
                    let logical_size =
                        winit::dpi::LogicalSize::new(config.width as f64, config.height as f64);
                    let physical_size: winit::dpi::PhysicalSize<f64> = logical_size.to_physical(sf);
                    let msize = monitor.size().unwrap_or((0, 0));
                    let cx = (pos.0 as f64 + (msize.0 as f64 - physical_size.width) / 2.0).max(0.0);
                    let cy =
                        (pos.1 as f64 + (msize.1 as f64 - physical_size.height) / 2.0).max(0.0);
                    win_attrs = win_attrs
                        .with_position(winit::dpi::PhysicalPosition::new(cx as i32, cy as i32));
                }
            }
        }

        let window = match event_loop.create_window(win_attrs) {
            Ok(w) => w,
            Err(e) => {
                push_error(UiError::WindowCreate(e.to_string()));
                return;
            }
        };
        let id = window.id();
        let sf = window.scale_factor();
        let win_arc: Arc<dyn winit::window::Window> = Arc::from(window);
        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        let visible = config.visible;
        #[cfg(any(target_os = "android", target_os = "ios"))]
        let visible = true;

        let mut state = WindowState::new(config);
        #[cfg(feature = "devtools")]
        {
            state.devtools_buf = Some(self.devtools_buf.clone());
        }
        state.scale_factor = sf;
        state.winit_window = Some(win_arc.clone());
        let raw_handle = match win_arc.window_handle() {
            Ok(h) => h.as_raw(),
            Err(e) => {
                push_error(UiError::SurfaceCreate(e.to_string()));
                return;
            }
        };
        state.a11y.init(raw_handle);
        state.window_handle = Some(WindowHandle {
            window: win_arc.clone(),
        });
        win_arc.set_visible(visible);
        {
            // Per-window wake: every register_dirty on THIS window's
            // AppContext requests a redraw of THIS window (audit 2026-07-18
            // multi-window pass — replaces the process-global ON_DIRTY slot).
            let w = win_arc.clone();
            state.app.set_on_dirty(Rc::new(move || {
                w.request_redraw();
            }));
        }
        state.ensure_renderer();
        state.mount_root(widget);

        #[cfg(feature = "tray")]
        if let Some(builder) = state.config.tray.take() {
            match builder.build() {
                Ok(tray) => state.tray_icon = Some(tray),
                Err(_e) => {
                    #[cfg(any(feature = "devtools", feature = "file-logging"))]
                    tracing::warn!(target: "burin", "[tray] failed to create tray icon: {_e}");
                }
            }
        }

        if let Some(ref w) = state.winit_window {
            w.request_redraw();
        }
        self.windows.insert(id, state);
    }
}

impl ApplicationHandler for App {
    fn can_create_surfaces(&mut self, event_loop: &dyn ActiveEventLoop) {
        let pending: Vec<_> = self.pending.drain(..).collect();
        for (config, widget) in pending {
            self.create_window_inner(event_loop, config, widget);
        }
    }

    fn window_event(
        &mut self,
        event_loop: &dyn ActiveEventLoop,
        window_id: winit::window::WindowId,
        event: winit::event::WindowEvent,
    ) {
        if let Some(state) = self.windows.get_mut(&window_id) {
            // Install this window's AppContext as the active one for the whole
            // event-handling scope. Event callbacks (pointer/keyboard/scroll)
            // call dirty_registry::* free functions which route through the
            // CURRENT_APP bridge; without this, callbacks fired between frames
            // would enqueue into whichever window ran on_frame last — wrong for
            // multi-window. This is the per-window routing anchor for the
            // "widgets stay bridge-routed by default" design (widgets that need
            // to escape the current-app scope capture their own Weak instead).
            // The wake callback (`on_dirty`) is per-AppContext, installed once
            // at window creation — no per-event global-slot juggling.
            crate::core::app_context::set_current_app(&state.app);
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                state.handle_event(event_loop, event);
            }));
            if let Err(panic) = result {
                let wid = state
                    .window_handle
                    .as_ref()
                    .map(|h| h.id().into_raw() as u64);
                push_error(UiError::CallbackPanic {
                    context: "window_event".into(),
                    window_id: wid,
                    element_id: None,
                    message: panic_to_string(&panic),
                });
                state.needs_rebuild = true;
            }
            if state.close_requested {
                if let Some(ref win) = state.winit_window {
                    win.set_visible(false);
                }
                self.windows.remove(&window_id);
                if self.windows.is_empty() {
                    event_loop.exit();
                }
            }
        }

        // Drain pending windows queued by widget callbacks during this event.
        PENDING_WINDOWS.with(|q| {
            for (config, widget) in q.borrow_mut().drain(..) {
                self.create_window_inner(event_loop, config, widget);
            }
        });
    }

    fn about_to_wait(&mut self, event_loop: &dyn ActiveEventLoop) {
        crate::platform::wake::drain_ui_queue();

        // ── Global hotkey poll (system-global, works when app is backgrounded) ──
        #[cfg(feature = "global-hotkey")]
        {
            let actions = self.hotkey_manager.poll();
            if !actions.is_empty() {
                for state in self.windows.values_mut() {
                    if state.close_requested {
                        continue;
                    }
                    crate::core::app_context::set_current_app(&state.app);
                    if let Some(root_id) = state.arena.root_id {
                        let path = state.arena.path_to_root(root_id);
                        for action_kind in &actions {
                            let action = Action::new(*action_kind);
                            state.dispatch_action(&action, &path);
                        }
                    }
                }
            }
        }

        self.flush_scheduler.drain();
        auralis_task::drain_deferred_signal_callbacks();

        // ── Async timer bridge (audit 2026-07-17 round 5, A1) ──
        // The executor only checks its timers inside a flush; nothing else
        // re-schedules one when a timer expires while the loop sleeps. Fire
        // expired timers now, then fold the earliest remaining deadline into
        // the WaitUntil below so the loop actually wakes up for it.
        if auralis_task::next_timer_delay_ms() == Some(0) {
            auralis_task::flush_all();
            // The woken tasks may have queued follow-up work.
            self.flush_scheduler.drain();
            auralis_task::drain_deferred_signal_callbacks();
        }

        // ── Multi-window dirty redistribution (audit 2026-07-18) ──
        // Entries registered under a stale current_app (top-of-loop drains
        // above, cross-window bridge-routed callbacks) were parked in the
        // processing window's foreign bucket. Route each to the window whose
        // arena owns the element and wake it. Empty in single-window apps.
        let window_ids: Vec<winit::window::WindowId> = self.windows.keys().copied().collect();
        for wid in &window_ids {
            let Some(state) = self.windows.get(wid) else {
                continue;
            };
            if !state.app.has_foreign_dirty() {
                continue;
            }
            let orphans = state.app.take_foreign_dirty();
            for (eid, flags) in orphans {
                for target in self.windows.values() {
                    if target.arena.get(eid).is_some() {
                        target.app.register_dirty(eid, flags);
                        break;
                    }
                }
                // Element in no window's arena: dropped (torn down between
                // registration and redistribution — same as single-window
                // behaviour for dead ids).
            }
        }

        for state in self.windows.values_mut() {
            if state.close_requested {
                continue;
            }
            // Scope every per-window drain/frame to ITS OWN AppContext —
            // reset_dirty_redraw, has_pending_dirty and deferred signal
            // callbacks all route through current_app (audit 2026-07-18:
            // these previously ran under whichever window handled the last
            // event, resetting the wrong wake gate and reading the wrong
            // dirty queue).
            crate::core::app_context::set_current_app(&state.app);
            state.app.reset_dirty_redraw();

            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                state.about_to_wait(event_loop);
            }));
            if let Err(panic) = result {
                push_error(UiError::CallbackPanic {
                    context: "about_to_wait".into(),
                    window_id: state
                        .window_handle
                        .as_ref()
                        .map(|h| h.id().into_raw() as u64),
                    element_id: None,
                    message: panic_to_string(&panic),
                });
                state.needs_rebuild = true;
            }
        }

        // ── Scheduler: when a discrete deadline is pending, set
        //    WaitUntil so the loop sleeps until the deadline.
        //    Continuous work and the legacy busy-pump
        //    (request_redraw) still keep the loop alive as before.
        //    Async task timers (timer::sleep) are folded in on the same
        //    clock axis (ClockTimeSource reads clock::now()).
        //    Deadlines are per-window (scheduler lives on AppContext):
        //    fold the earliest across every window into one WaitUntil.
        let mut deadline: Option<web_time::Instant> = None;
        for state in self.windows.values() {
            crate::core::app_context::set_current_app(&state.app);
            if let Some(d) = crate::core::scheduler::next_deadline() {
                deadline = Some(deadline.map_or(d, |cur: web_time::Instant| cur.min(d)));
            }
        }
        if let Some(delay_ms) = auralis_task::next_timer_delay_ms() {
            let task_at = crate::core::clock::now() + std::time::Duration::from_millis(delay_ms);
            deadline = Some(deadline.map_or(task_at, |d| d.min(task_at)));
        }
        if let Some(deadline) = deadline {
            event_loop.set_control_flow(ControlFlow::WaitUntil(deadline));
        }
    }

    fn suspended(&mut self, _event_loop: &dyn ActiveEventLoop) {
        for state in self.windows.values_mut() {
            state.skip_frame = true;
            if let Some(ref mut renderer) = state.renderer {
                renderer.destroy_surface();
            }
        }
    }

    fn resumed(&mut self, _event_loop: &dyn ActiveEventLoop) {
        for state in self.windows.values_mut() {
            state.skip_frame = false;
            state.ensure_renderer();
            if let Some(ref w) = state.winit_window {
                w.request_redraw();
            }
        }
    }

    fn destroy_surfaces(&mut self, _event_loop: &dyn ActiveEventLoop) {
        for state in self.windows.values_mut() {
            if let Some(ref mut renderer) = state.renderer {
                renderer.destroy_surface();
            }
        }
    }
}

impl WindowState {
    pub(crate) fn handle_event(
        &mut self,
        _event_loop: &dyn ActiveEventLoop,
        event: winit::event::WindowEvent,
    ) {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let win = self.winit_window.clone();
            let rid = self.arena.root_id;

            match event {
                winit::event::WindowEvent::CloseRequested => {
                    self.renderer.take();
                    self.close_requested = true;
                }
                winit::event::WindowEvent::RedrawRequested => {
                    #[cfg(debug_assertions)]
                    if self.frame_id == 0 {
                        eprintln!("[event] first RedrawRequested received");
                    }
                    self.on_frame();
                }
                winit::event::WindowEvent::SurfaceResized(size) => {
                    let sf = self.scale_factor as f32;
                    self.config.width = size.width as f32 / sf;
                    self.config.height = size.height as f32 / sf;
                    self.needs_taffy = true;
                    #[cfg(feature = "devtools")]
                    crate::debug::devtools::record_interaction(
                        self.frame_id,
                        crate::debug::devtools::InteractionKind::Resize {
                            width: self.config.width,
                            height: self.config.height,
                        },
                        auralis_signal::now_us(),
                    );
                    if let Some(ref mut r) = self.renderer {
                        r.resize_gpu(size.width, size.height);
                        r.resize_cpu(self.config.width, self.config.height, sf);
                    }
                    if let Some(ref w) = win {
                        w.request_redraw();
                    }
                }
                winit::event::WindowEvent::PointerMoved {
                    position, source, ..
                } => {
                    let sf = self.scale_factor as f32;
                    let lx = position.x as f32 / sf;
                    let ly = position.y as f32 / sf;
                    #[cfg(feature = "devtools")]
                    crate::debug::devtools::record_interaction(
                        self.frame_id,
                        crate::debug::devtools::InteractionKind::PointerMove { x: lx, y: ly },
                        auralis_signal::now_us(),
                    );
                    let pos = crate::style::Point::new(lx, ly);
                    let (state_key, event_fid) = finger_id_from_source(&source);
                    self.last_cursor = Some(pos);

                    let old_chain: Vec<ElementId> = self
                        .touches
                        .get_mut(&state_key)
                        .map(|s| std::mem::take(&mut s.hovered_chain))
                        .unwrap_or_default();
                    let new_chain =
                        if let Some(result) = crate::event::hit_test::hit_test(&self.arena, pos) {
                            result.path
                        } else {
                            Vec::new()
                        };

                    // Chains are leaf-first ancestor paths to the same root, so
                    // their set intersection is exactly the common SUFFIX —
                    // diffing is O(depth), not the old O(depth²) contains() scan
                    // (which also iterated .rev() while claiming "deepest first").
                    let common = {
                        let mut n = 0;
                        while n < old_chain.len()
                            && n < new_chain.len()
                            && old_chain[old_chain.len() - 1 - n]
                                == new_chain[new_chain.len() - 1 - n]
                        {
                            n += 1;
                        }
                        n
                    };

                    // Fire leave on elements in old but not new, deepest first.
                    for &eid in &old_chain[..old_chain.len() - common] {
                        {
                            if let Some(el) = self.arena.get_mut(eid) {
                                el.set_state_dirty(StateFlags::HOVERED, false);
                            }
                            self.event_registry.fire_hover_leave(eid);
                            // Clear pending submenu timer if leaving the item
                            crate::core::app_context::current_app().hovered_submenu_with(|s| {
                                if s.as_ref().is_some_and(|(id, _)| *id == eid)
                                    && !crate::widgets::overlay::is_submenu_recently_opened()
                                {
                                    *s = None;
                                    crate::core::scheduler::cancel(SUBMENU_TIMER_KEY);
                                }
                            });
                        }
                    }
                    // Fire enter on elements in new but not old, deepest first.
                    for &eid in &new_chain[..new_chain.len() - common] {
                        {
                            if dirty_registry::is_element_or_ancestor_disabled(eid) {
                                continue;
                            }
                            if let Some(el) = self.arena.get_mut(eid) {
                                el.set_state_dirty(StateFlags::HOVERED, true);
                            }
                            self.event_registry.fire_hover_enter(eid);
                            // Arm the hover-to-open-submenu timer ONLY for rows that
                            // belong to an already-open menu — not for ordinary host
                            // widgets (e.g. a Table) that merely carry ContextMenuItems.
                            if self
                                .arena
                                .get(eid)
                                .and_then(|el| {
                                    el.get_user_data::<crate::widgets::overlay::ContextMenuItems>()
                                })
                                .is_some()
                                && crate::widgets::overlay::row_belongs_to_open_menu(eid)
                            {
                                let keep = crate::widgets::overlay::is_submenu_recently_opened()
                                    && crate::core::app_context::current_app()
                                        .hovered_submenu_with(|s| s.is_some());
                                if !keep {
                                    crate::core::app_context::current_app().set_hovered_submenu(
                                        Some((eid, crate::core::clock::now())),
                                    );
                                    // Schedule a frame at the deadline so
                                    // on_after_dirty can check elapsed() even
                                    // when the app is otherwise idle.
                                    crate::core::scheduler::schedule_at(
                                        crate::core::clock::now()
                                            + std::time::Duration::from_millis(
                                                SUBMENU_DELAY_MS as u64,
                                            ),
                                        SUBMENU_TIMER_KEY,
                                    );
                                }
                            }
                        }
                    }

                    // Apply cursor from deepest hovered element
                    let new_cursor = new_chain
                        .iter()
                        .filter_map(|&eid| self.arena.get(eid).and_then(|el| el.cursor_icon()))
                        .next()
                        .unwrap_or(CursorIcon::DEFAULT);
                    if let Some(ref wh) = self.window_handle {
                        wh.set_cursor(new_cursor);
                    }

                    // Screen-position check covers the parent–submenu gap.
                    crate::widgets::overlay::update_submenu_autoclose(
                        &mut self.arena,
                        pos,
                        &new_chain,
                    );

                    let state = self.finger_state(state_key);
                    state.position = pos;
                    state.hovered_chain = new_chain;

                    // Scrollbar drag update.
                    if let Some(ref drag) = self.scrollbar_drag {
                        crate::widgets::overlay::dismiss_context_menu_immediate(&mut self.arena);
                        let eid = drag.0;
                        let axis = drag.1;
                        let gf = drag.2;
                        let sb = self
                            .arena
                            .get(eid)
                            .map(|el| el.screen_bounds)
                            .unwrap_or(crate::style::Rect::ZERO);
                        let sc = self.arena.comp_scroll(eid);
                        let lc = self.arena.comp_layout(eid);
                        if let Some(sc) = sc {
                            let own_so = sc.scroll_offset.get();
                            let cb = sc.content_bounds.get();
                            let sbw = lc.map_or(10.0, |l| l.scrollbar_width);
                            let cw = cb.width.max(1.0);
                            let ch = cb.height.max(1.0);
                            let (asx, asy) = crate::core::dirty_registry::accumulated_scroll_cached(
                                &self.arena,
                                eid,
                            );
                            let ancestor_x = asx - own_so.x;
                            let ancestor_y = asy - own_so.y;

                            // Try to use ScrollBundle for physics-aware scrollbar
                            let via_bundle = crate::widgets::bundle::scroll::try_set_offset(
                                &self.arena,
                                eid,
                                |bundle, vp| match axis {
                                    ScrollAxis::Vertical if ch > sb.height => {
                                        let thumb_h = (sb.height / ch * sb.height).max(20.0);
                                        let trk = sb.height - thumb_h;
                                        let adj = pos.y + ancestor_y - sb.y - gf * thumb_h;
                                        let frac = (adj / trk.max(1.0)).clamp(0.0, 1.0);
                                        let off = frac * (ch - sb.height);
                                        let mut v = own_so;
                                        v.y = off;
                                        bundle.set_offset_with_physics(v, vp);
                                    }
                                    ScrollAxis::Horizontal if cw > sb.width => {
                                        let h_gutter = if ch > sb.height { sbw + 2.0 } else { 0.0 };
                                        let thumb_w = (sb.width / cw * sb.width).max(20.0);
                                        let trk = sb.width - thumb_w - h_gutter;
                                        let adj = pos.x + ancestor_x - sb.x - gf * thumb_w;
                                        let frac = (adj / trk.max(1.0)).clamp(0.0, 1.0);
                                        let off = frac * (cw - sb.width);
                                        let mut v = own_so;
                                        v.x = off;
                                        bundle.set_offset_with_physics(v, vp);
                                    }
                                    _ => {}
                                },
                            );

                            // Fallback: direct ECS write if no ScrollBundle available
                            if !via_bundle {
                                match axis {
                                    ScrollAxis::Vertical if ch > sb.height => {
                                        let thumb_h = (sb.height / ch * sb.height).max(20.0);
                                        let trk = sb.height - thumb_h;
                                        let adj = pos.y + ancestor_y - sb.y - gf * thumb_h;
                                        let frac = (adj / trk.max(1.0)).clamp(0.0, 1.0);
                                        let off = frac * (ch - sb.height);
                                        let mut v = sc.scroll_offset.get();
                                        v.y = off;
                                        sc.scroll_offset.set(v);
                                        crate::core::dirty_registry::spatial_update_scroll(
                                            eid, v.x, v.y,
                                        );
                                    }
                                    ScrollAxis::Horizontal if cw > sb.width => {
                                        let h_gutter = if ch > sb.height { sbw + 2.0 } else { 0.0 };
                                        let thumb_w = (sb.width / cw * sb.width).max(20.0);
                                        let trk = sb.width - thumb_w - h_gutter;
                                        let adj = pos.x + ancestor_x - sb.x - gf * thumb_w;
                                        let frac = (adj / trk.max(1.0)).clamp(0.0, 1.0);
                                        let off = frac * (cw - sb.width);
                                        let mut v = sc.scroll_offset.get();
                                        v.x = off;
                                        sc.scroll_offset.set(v);
                                        crate::core::dirty_registry::spatial_update_scroll(
                                            eid, v.x, v.y,
                                        );
                                    }
                                    _ => {}
                                }
                                if let Some(el) = self.arena.get(eid) {
                                    el.mark_repaint();
                                }
                            }
                        }
                    }

                    // Scrollbar hover tracking (single combined scan — audit 2026-07-17).
                    self.scrollbar_hover = None;
                    if rid.is_some() && self.scrollbar_drag.is_none() {
                        if let Some(hit) =
                            crate::widgets::bundle::scroll::hit_scrollbar(&self.arena, pos)
                        {
                            self.scrollbar_hover = Some((hit.eid, hit.axis));
                        }
                    }
                    // Override cursor icon when hovering over a scrollbar (thumb or track)
                    if self.scrollbar_hover.is_some() {
                        if let Some(ref wh) = self.window_handle {
                            wh.set_cursor(CursorIcon::DEFAULT);
                        }
                    }

                    let events = self.translator.cursor_moved(lx, ly, event_fid);
                    let _prev = dirty_registry::current_trigger();
                    dirty_registry::set_current_trigger(
                        dirty_registry::DirtyTriggerTag::PointerEvent,
                    );
                    self.dispatch_events(&events);
                    dirty_registry::set_current_trigger(_prev);
                }
                winit::event::WindowEvent::PointerButton { state, button, .. } => {
                    let pressed = state == winit::event::ElementState::Pressed;
                    let (state_key, event_fid) = finger_id_from_button(&button);
                    let btn = button
                        .mouse_button()
                        .map(map_mouse_button)
                        .unwrap_or(crate::event::MouseButton::Left);
                    #[cfg(feature = "devtools")]
                    if let Some(ref pos) = self.last_cursor {
                        let raw_button: u32 = match btn {
                            crate::event::MouseButton::Left => 1,
                            crate::event::MouseButton::Right => 2,
                            crate::event::MouseButton::Middle => 3,
                            crate::event::MouseButton::Back => 4,
                            crate::event::MouseButton::Forward => 5,
                            crate::event::MouseButton::Other(v) => v as u32,
                        };
                        let kind = if pressed {
                            crate::debug::devtools::InteractionKind::PointerDown {
                                x: pos.x,
                                y: pos.y,
                                button: raw_button,
                                modifiers: crate::debug::devtools::modifiers_to_u32(
                                    self.translator.modifiers(),
                                ),
                            }
                        } else {
                            crate::debug::devtools::InteractionKind::PointerUp {
                                x: pos.x,
                                y: pos.y,
                                button: raw_button,
                                modifiers: crate::debug::devtools::modifiers_to_u32(
                                    self.translator.modifiers(),
                                ),
                            }
                        };
                        crate::debug::devtools::record_interaction(
                            self.frame_id,
                            kind,
                            auralis_signal::now_us(),
                        );
                    }

                    let hover_from_move = self
                        .touches
                        .get(&state_key)
                        .and_then(|s| s.hovered_chain.first().copied());
                    let old_pressed = self.touches.get(&state_key).and_then(|s| s.pressed);
                    let gesture_target = old_pressed;

                    // On mouse DOWN, perform a fresh hit test so PRESSED state is
                    // set on the correct element even without a prior PointerMove.
                    let old_hovered = if pressed {
                        if let Some(ref pos) = self.last_cursor {
                            crate::event::hit_test::hit_test(&self.arena, *pos)
                                .map(|r| r.target)
                                .or(hover_from_move)
                        } else {
                            hover_from_move
                        }
                    } else {
                        hover_from_move
                    };

                    let mut new_pressed: Option<ElementId> = None;

                    if pressed {
                        let mut scrollbar_return = false;
                        if let Some(ref pos) = self.last_cursor {
                            if rid.is_some() {
                                if let Some(hit) =
                                    crate::widgets::bundle::scroll::hit_scrollbar(&self.arena, *pos)
                                {
                                    crate::widgets::overlay::dismiss_context_menu_immediate(
                                        &mut self.arena,
                                    );
                                    self.scroll_kinetic = None;
                                    if hit.on_thumb {
                                        self.scrollbar_drag =
                                            Some((hit.eid, hit.axis, hit.fraction));
                                    } else {
                                        crate::widgets::bundle::scroll::scrollbar_jump_to(
                                            &self.arena,
                                            hit.eid,
                                            hit.axis,
                                            hit.fraction,
                                        );
                                        self.scrollbar_drag = Some((hit.eid, hit.axis, 0.5));
                                    }
                                    self.scrollbar_hover = Some((hit.eid, hit.axis));
                                    scrollbar_return = true;
                                }
                            }
                        }
                        if scrollbar_return {
                            // Clear pressed state to prevent stale primary_pressed() values
                            // from blocking subsequent PointerDown dispatch.
                            self.touches.entry(state_key).or_default().pressed = None;
                            return;
                        }
                        new_pressed = old_hovered;

                        let old_focus = self.focus_manager.focused();
                        let focus_target = old_hovered.and_then(|hit| {
                            let path = self.arena.path_to_root(hit);
                            path.iter().find_map(|&id| {
                                self.arena.get(id).and_then(|el| {
                                    if el.is_focusable() {
                                        Some(id)
                                    } else {
                                        None
                                    }
                                })
                            })
                        });

                        const REASON: FocusReason = FocusReason::PointerClick;
                        if let Some(hit_id) = focus_target {
                            if old_focus != Some(hit_id) {
                                if let Some(old_id) = old_focus {
                                    // Flush IME composition into the old TextInput
                                    // before focus transfers to the new element.
                                    self.event_registry.commit_preedit(old_id);
                                    if let Some(el) = self.arena.get_mut(old_id) {
                                        el.set_state_dirty(StateFlags::FOCUSED, false);
                                        el.last_focus_reason.set(Some(REASON));
                                    }
                                    self.event_registry.fire_focus_out(old_id, REASON);
                                }
                                if let Some(el) = self.arena.get_mut(hit_id) {
                                    el.set_state_dirty(StateFlags::FOCUSED, true);
                                    el.last_focus_reason.set(Some(REASON));
                                }
                                self.event_registry.fire_focus_in(hit_id, REASON);
                                self.focus_manager.set_focused(Some(hit_id));
                                if let Some(ref w) = win {
                                    request_ime_enable(w, &self.event_registry, hit_id);
                                }
                            }
                        } else if let Some(old_id) = old_focus {
                            // Only defer blur if the element has an active IME
                            // composition. Otherwise, blur immediately to avoid
                            // frame-by-frame focus oscillation.
                            if self.event_registry.has_text_input(old_id) {
                                // On focus loss (click outside), do NOT commit
                                // the IME preedit — the user hasn't confirmed the
                                // composition. Instead, just clear the local
                                // composition state so the TextInput's visual is
                                // clean. The OS IME keeps its state; when the user
                                // re-focuses and commits, the composed text appears
                                // as expected (matching WeChat, VS Code, etc.).
                                //
                                // For focus TRANSFER (clicking another TextInput),
                                // commit_preedit is called above to flush text into
                                // the old element before focus moves.
                                self.event_registry
                                    .fire_ime_preedit(old_id, String::new(), None);
                                self.focus_manager.defer_blur(old_id);
                            } else {
                                if let Some(el) = self.arena.get_mut(old_id) {
                                    el.set_state_dirty(StateFlags::FOCUSED, false);
                                    el.last_focus_reason.set(Some(FocusReason::PointerClick));
                                }
                                self.event_registry
                                    .fire_focus_out(old_id, FocusReason::PointerClick);
                                self.focus_manager.set_focused(None);
                            }
                        }
                    }

                    let press_target = if pressed { old_hovered } else { old_pressed };
                    if let Some(target_id) = press_target {
                        if !dirty_registry::is_element_or_ancestor_disabled(target_id) {
                            if let Some(el) = self.arena.get_mut(target_id) {
                                el.set_state_dirty(StateFlags::PRESSED, pressed);
                            }
                        }
                    }
                    if !pressed {
                        self.scrollbar_drag = None;
                        self.scrollbar_hover = None;
                        // Force-clear pressed after scrollbar drag to prevent
                        // stale primary_pressed() from blocking PointerDown dispatch.
                        if let Some(state) = self.touches.get_mut(&state_key) {
                            state.pressed = None;
                        }
                        // Fire hover-leave and re-hover on next PointerMoved
                        let old_chain: Vec<ElementId> = self
                            .touches
                            .get(&state_key)
                            .map_or(Vec::new(), |s| s.hovered_chain.clone());
                        for &eid in old_chain.iter().rev() {
                            if let Some(el) = self.arena.get_mut(eid) {
                                el.set_state_dirty(StateFlags::HOVERED, false);
                            }
                            self.event_registry.fire_hover_leave(eid);
                        }
                        if let Some(state) = self.touches.get_mut(&state_key) {
                            // Keep the leaf-most entry so old_hovered is available
                            // for the next pointer_down without PointerMoved.
                            let last = state.hovered_chain.last().copied();
                            state.hovered_chain.clear();
                            if let Some(eid) = last {
                                state.hovered_chain.push(eid);
                            }
                        }
                    }

                    let events = self.translator.mouse_input(pressed, btn, event_fid);
                    if pressed {
                        if let Some(ref pos) = self.last_cursor {
                            self.click_counter
                                .pointer_down(*pos, crate::core::clock::now());
                        }
                    } else {
                        if let Some(ref pos) = self.last_cursor {
                            match self
                                .click_counter
                                .pointer_up(*pos, crate::core::clock::now())
                            {
                                crate::event::ClickResult::Double { .. } => {
                                    if let Some(id) = gesture_target {
                                        self.event_registry.fire_double_click(id);
                                    }
                                }
                                crate::event::ClickResult::Triple { .. } => {
                                    if let Some(id) = gesture_target {
                                        self.event_registry.fire_triple_click(id);
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                    // Publish the freshly-hit press target BEFORE dispatching, so the
                    // PointerDown handler's drag routing (which reads primary_pressed()
                    // to fire drag_start / set drag_text_input) sees THIS press, not the
                    // previous one — otherwise a drag widget's first drag is lost (the
                    // drag is routed to the previously-pressed element). Releases keep
                    // the old pressed until after dispatch so the DragEnd handler can
                    // still resolve the element being released.
                    if pressed {
                        self.touches.entry(state_key).or_default().pressed = new_pressed;
                    }
                    let _prev = dirty_registry::current_trigger();
                    dirty_registry::set_current_trigger(
                        dirty_registry::DirtyTriggerTag::PointerEvent,
                    );
                    self.dispatch_events(&events);
                    dirty_registry::set_current_trigger(_prev);
                    // Right-click context menu: walk the hit-test path and open
                    // the first element with a context_menu attached.
                    if !pressed && btn == crate::event::MouseButton::Right {
                        if let Some(ref pos) = self.last_cursor {
                            let rid = self.arena.root_id;
                            if let Some(result) =
                                crate::event::hit_test::hit_test(&self.arena, *pos)
                            {
                                for &eid in &result.path {
                                    let items = self.arena.get(eid)
                                    .and_then(|el| el.get_user_data::<crate::widgets::overlay::ContextMenuItems>().cloned());
                                    if let Some(cmi) = items {
                                        if let Some(root_id) = rid {
                                            let sw = self.config.width / self.scale_factor as f32;
                                            let sh = self.config.height / self.scale_factor as f32;
                                            let x = pos.x.min(sw - 220.0).max(0.0);
                                            let y = if pos.y + 100.0 > sh {
                                                (pos.y - 100.0).max(0.0)
                                            } else {
                                                pos.y
                                            };
                                            let adjusted = crate::style::Point::new(x, y);
                                            crate::widgets::overlay::open_context_menu(
                                                cmi.0,
                                                adjusted,
                                                &mut self.arena,
                                                root_id,
                                                Some(&mut self.event_registry),
                                                None,
                                                false,
                                                sh,
                                            ); // root menu (no parent, opens right)
                                        }
                                        break;
                                    }
                                }
                            }
                        }
                    }
                    // Left-click on a context menu item that has submenu children:
                    // open the nested menu at the right edge of the clicked item.
                    // Dismiss only if the click is outside ALL open menus.
                    if !pressed && btn == crate::event::MouseButton::Left {
                        let mut hit_menu = false;
                        if let Some(ref pos) = self.last_cursor {
                            let rid = self.arena.root_id;
                            if let Some(result) =
                                crate::event::hit_test::hit_test(&self.arena, *pos)
                            {
                                for &eid in &result.path {
                                    // Only treat a ContextMenuItems-carrying element as a
                                    // submenu trigger when it actually lives inside an OPEN
                                    // menu. An ordinary widget that merely set .context_menu()
                                    // (e.g. a Table with a right-click menu) also carries
                                    // ContextMenuItems; without this guard a LEFT-click on it
                                    // spuriously opens its menu and steals focus. Mirrors the
                                    // hover-path guard (`row_belongs_to_open_menu`).
                                    if !crate::widgets::overlay::row_belongs_to_open_menu(eid) {
                                        continue;
                                    }
                                    let has_submenu = self.arena.get(eid)
                                    .and_then(|el| el.get_user_data::<crate::widgets::overlay::ContextMenuItems>().cloned());
                                    if let Some(sub) = has_submenu {
                                        hit_menu = true;
                                        let sb = self.arena.get(eid).map(|el| el.screen_bounds);
                                        if let (Some(sb), Some(root_id)) = (sb, rid) {
                                            let parent =
                                                crate::core::dirty_registry::parent_of(eid);
                                            let prefer_left = parent
                                            .and_then(|p| self.arena.get(p))
                                            .and_then(|pel| pel.get_user_data::<crate::widgets::overlay::MenuOpenDir>())
                                            .is_some_and(|d| d.0);
                                            let sub_h = sub
                                                .0
                                                .iter()
                                                .filter(|i| !i.separator)
                                                .count()
                                                .max(1)
                                                as f32
                                                * 32.0;
                                            let screen_w =
                                                self.config.width / self.scale_factor as f32;
                                            let screen_h =
                                                self.config.height / self.scale_factor as f32;
                                            let (sub_x, opened_left) =
                                                submenu_x(sb.x, sb.width, screen_w, prefer_left);
                                            let sub_y = submenu_y(sb.y, sb.height, sub_h, screen_h);
                                            let sub_pos = crate::style::Point::new(sub_x, sub_y);
                                            crate::widgets::overlay::open_context_menu(
                                                sub.0,
                                                sub_pos,
                                                &mut self.arena,
                                                root_id,
                                                Some(&mut self.event_registry),
                                                parent,
                                                opened_left,
                                                screen_h,
                                            );
                                        }
                                        break;
                                    }
                                }
                            }
                            // Also consider click "on menu" if the cursor is inside any
                            // open menu container (covers gap between parent & submenu).
                            if !hit_menu {
                                hit_menu =
                                    crate::widgets::overlay::menu_chain_contains(&self.arena, *pos);
                            }
                        }
                        if !hit_menu {
                            crate::widgets::overlay::dismiss_context_menu_immediate(
                                &mut self.arena,
                            );
                        }
                    }
                    if !pressed {
                        self.touches.entry(state_key).or_default().pressed = new_pressed;
                    }
                }
                winit::event::WindowEvent::PointerEntered { position, kind, .. } => {
                    let sf = self.scale_factor as f32;
                    let lx = position.x as f32 / sf;
                    let ly = position.y as f32 / sf;
                    let pos = crate::style::Point::new(lx, ly);
                    let (state_key, _event_fid) = finger_id_from_kind(&kind);

                    let new_chain =
                        if let Some(result) = crate::event::hit_test::hit_test(&self.arena, pos) {
                            result.path
                        } else {
                            Vec::new()
                        };
                    let old_chain: Vec<ElementId> = self
                        .touches
                        .get(&state_key)
                        .map_or(Vec::new(), |s| s.hovered_chain.clone());

                    for &eid in old_chain.iter().rev() {
                        if !new_chain.contains(&eid) {
                            if let Some(el) = self.arena.get_mut(eid) {
                                el.set_state_dirty(StateFlags::HOVERED, false);
                            }
                            self.event_registry.fire_hover_leave(eid);
                        }
                    }
                    for &eid in &new_chain {
                        if !old_chain.contains(&eid) {
                            if let Some(el) = self.arena.get_mut(eid) {
                                el.set_state_dirty(StateFlags::HOVERED, true);
                            }
                            self.event_registry.fire_hover_enter(eid);
                        }
                    }

                    let state = self.touches.entry(state_key).or_default();
                    state.position = pos;
                    state.hovered_chain = new_chain;
                }
                winit::event::WindowEvent::PointerLeft { kind, .. } => {
                    let (state_key, _) = finger_id_from_kind(&kind);
                    if state_key == 0 {
                        self.last_cursor = None;
                    }
                    if let Some(state) = self.touches.get_mut(&state_key) {
                        for &eid in state.hovered_chain.iter().rev() {
                            if let Some(el) = self.arena.get_mut(eid) {
                                el.set_state_dirty(StateFlags::HOVERED, false);
                            }
                            self.event_registry.fire_hover_leave(eid);
                        }
                        state.hovered_chain.clear();
                        state.pressed = None;
                    }
                    if let Some(ref wh) = self.window_handle {
                        wh.set_cursor(CursorIcon::DEFAULT);
                    }
                }
                winit::event::WindowEvent::MouseWheel { delta, phase, .. } => {
                    // Scrolling over an open menu scrolls the menu itself; only a
                    // wheel event *outside* any open menu dismisses it.
                    let over_menu = self.last_cursor.is_some_and(|pos| {
                        crate::widgets::overlay::menu_chain_contains(&self.arena, pos)
                    });
                    if !over_menu {
                        crate::widgets::overlay::dismiss_context_menu_immediate(&mut self.arena);
                    }
                    let (dx, dy) = match delta {
                        winit::event::MouseScrollDelta::LineDelta(x, y) => (
                            x * WHEEL_PIXELS_PER_LINE * WHEEL_LINES_PER_NOTCH,
                            -y * WHEEL_PIXELS_PER_LINE * WHEEL_LINES_PER_NOTCH,
                        ),
                        winit::event::MouseScrollDelta::PixelDelta(pos) => {
                            (pos.x as f32, pos.y as f32)
                        }
                    };
                    #[cfg(feature = "devtools")]
                    if let Some(ref pos) = self.last_cursor {
                        crate::debug::devtools::record_interaction(
                            self.frame_id,
                            crate::debug::devtools::InteractionKind::Scroll {
                                x: pos.x,
                                y: pos.y,
                                delta_x: dx,
                                delta_y: dy,
                            },
                            auralis_signal::now_us(),
                        );
                    }
                    let now = StdInstant::now();

                    // Velocity tracking for kinetic scroll on trackpad gesture end
                    match phase {
                        winit::event::TouchPhase::Started => {
                            self.scroll_kinetic = None;
                            self.velocity_history.clear();
                        }
                        winit::event::TouchPhase::Moved => {
                            self.velocity_history.push((dx, dy, now));
                            self.velocity_history.retain(|(_, _, t)| {
                                now.duration_since(*t).as_millis() < VELOCITY_HISTORY_MAX_MS
                            });
                        }
                        winit::event::TouchPhase::Ended => {
                            if let Some(target) = self.scroll_kinetic_target {
                                let vel = crate::widgets::bundle::scroll::compute_scroll_velocity(
                                    &self.velocity_history,
                                );
                                crate::widgets::bundle::scroll::try_fling(
                                    &self.arena,
                                    target,
                                    crate::style::Vec2::new(vel.x, vel.y),
                                );
                            }
                            self.velocity_history.clear();
                        }
                        _ => {}
                    }

                    // Propagate scroll through capture → bubble first.
                    // Reuse the hit-test result for the fallback below (B3):
                    // previously every wheel event paid TWO full point queries.
                    let mut propagated = false;
                    let mut wheel_hit: Option<crate::event::hit_test::HitTestResult> = None;
                    if let Some(ref pos) = self.last_cursor {
                        wheel_hit = crate::event::hit_test::hit_test(&self.arena, *pos);
                        if let Some(ref result) = wheel_hit {
                            let evt = crate::event::Event::Scroll {
                                delta_x: dx,
                                delta_y: dy,
                            };
                            let modifiers = self.translator.modifiers();
                            propagated = crate::event::propagation::dispatch_event(
                                &mut self.arena,
                                &evt,
                                &result.path,
                                &mut self.focus_manager,
                                &mut self.event_registry,
                                modifiers,
                            )
                            .handled;
                        }
                    }

                    // Default scroll behavior — only if no widget consumed the event.
                    if !propagated {
                        if let Some(ref pos) = self.last_cursor {
                            let mut handled = false;
                            let hit_target = wheel_hit.as_ref().map(|r| r.target).or_else(|| {
                                dirty_registry::hit_test_with_fallback(&self.arena, *pos)
                            });

                            // ── Text scroll on hit target ──
                            if let Some(hit) = hit_target {
                                if let Some(el) = self.arena.get(hit) {
                                    if let Some(ref sy) = el.text_scroll_y() {
                                        let old_v = sy.get();
                                        let mut v = old_v - dy;
                                        let max_v = el
                                            .max_scroll_y()
                                            .as_ref()
                                            .map_or(f32::MAX, |c| c.get());
                                        v = v.clamp(0.0, max_v);
                                        if v != old_v {
                                            sy.set(v);
                                            el.mark_repaint();
                                            handled = true;
                                        }
                                    }
                                }
                            }

                            // ── Nested scroll chain: innermost-first, pass unconsumed ──
                            if !handled {
                                let chain = dirty_registry::spatial_scroll_chain(&self.arena, *pos);
                                if !chain.is_empty() {
                                    self.scroll_kinetic_target = Some(chain[0]);
                                    let mut rem_x = dx;
                                    let mut rem_y = dy;
                                    for &scrollable in &chain {
                                        if rem_x == 0.0 && rem_y == 0.0 {
                                            break;
                                        }
                                        let vp = self
                                            .arena
                                            .get(scrollable)
                                            .map_or(crate::style::Rect::ZERO, |el| {
                                                el.screen_bounds
                                            });
                                        if let Some((ux, uy)) =
                                            crate::widgets::bundle::scroll::try_scroll_by(
                                                &self.arena,
                                                scrollable,
                                                rem_x,
                                                rem_y,
                                                vp.height,
                                                vp.width,
                                            )
                                        {
                                            rem_x = ux;
                                            rem_y = uy;
                                            handled = true;
                                        }
                                    }
                                }
                            }

                            // ── Raw SCROLL component WITHOUT ScrollBundle (portal fallback) ──
                            if !handled {
                                if let Some(hit) = hit_target {
                                    let mut cur = Some(hit);
                                    while let Some(eid) = cur {
                                        if self.arena.get(eid).and_then(|el| el.get_user_data::<crate::widgets::bundle::scroll::ScrollBundleRef>()).is_none()
                                        && self.arena.comp_scroll(eid).is_some()
                                    {
                                        let sc = self.arena.comp_scroll(eid).unwrap();
                                        let mut off = sc.scroll_offset.get();
                                        off.y = (off.y + dy).max(0.0).min(sc.max_scroll_y.get());
                                        sc.scroll_offset.set(off);
                                        crate::core::dirty_registry::spatial_update_scroll(eid, 0.0, off.y);
                                        crate::core::dirty_registry::bump_subtree_gen(eid);
                                        crate::core::dirty_registry::register_dirty(eid, DirtyFlags::REPAINT);
                                        if let Some(el) = self.arena.get(eid) { el.mark_repaint(); }
                                        handled = true;
                                        break;
                                    }
                                        cur = dirty_registry::parent_of(eid);
                                    }
                                }
                            }

                            // ── Last-resort: tree-walk fallback ──
                            if !handled {
                                if let Some(root_id) = rid {
                                    let target =
                                        crate::widgets::bundle::scroll::find_scrollable_at_position(
                                            &self.arena,
                                            root_id,
                                            *pos,
                                        );
                                    self.scroll_kinetic_target = target;
                                    if let Some(eid) = target {
                                        let vp = self
                                            .arena
                                            .get(eid)
                                            .map_or(crate::style::Rect::ZERO, |el| {
                                                el.screen_bounds
                                            });
                                        crate::widgets::bundle::scroll::try_scroll_by(
                                            &self.arena,
                                            eid,
                                            dx,
                                            dy,
                                            vp.height,
                                            vp.width,
                                        );
                                    }
                                }
                            }
                        }
                    }
                    if let Some(ref w) = self.winit_window {
                        w.request_redraw();
                    }
                }
                winit::event::WindowEvent::ModifiersChanged(mods) => {
                    self.translator.set_modifiers(crate::event::Modifiers {
                        shift: mods.state().shift_key(),
                        ctrl: mods.state().control_key(),
                        alt: mods.state().alt_key(),
                        meta: mods.state().meta_key(),
                    });
                }
                winit::event::WindowEvent::KeyboardInput {
                    event: key_event, ..
                } => {
                    #[cfg(feature = "devtools")]
                    {
                        let key_name = format!("{:?}", key_event.logical_key);
                        let mods = self.translator.modifiers();
                        let kind = if key_event.state == winit::event::ElementState::Pressed {
                            crate::debug::devtools::InteractionKind::KeyPress {
                                key_name,
                                modifiers: crate::debug::devtools::modifiers_to_u32(mods),
                            }
                        } else {
                            crate::debug::devtools::InteractionKind::KeyRelease {
                                key_name,
                                modifiers: crate::debug::devtools::modifiers_to_u32(mods),
                            }
                        };
                        crate::debug::devtools::record_interaction(
                            self.frame_id,
                            kind,
                            auralis_signal::now_us(),
                        );
                    }
                    if key_event.state == winit::event::ElementState::Pressed {
                        self.focus_manager
                            .set_highlight_mode(crate::event::FocusHighlightMode::Traditional);
                        let mods = self.translator.modifiers();
                        let focused = self.focus_manager.focused();
                        let mut handled = false;

                        let action_path: Vec<ElementId> = focused
                            .map(|fid| self.arena.path_to_root(fid))
                            .unwrap_or_default();

                        // 1. Try action dispatch
                        if let winit::keyboard::Key::Character(c) = &key_event.logical_key {
                            if !c.is_empty() {
                                let chr = if mods.ctrl || mods.shift {
                                    c.to_lowercase()
                                } else {
                                    c.to_string()
                                };
                                if let Some(action_kind) = self.key_bindings.find(
                                    focused,
                                    &crate::event::Key::Character(chr.clone()),
                                    &mods,
                                ) {
                                    let action = if mods.shift {
                                        Action::new(action_kind).with_selection()
                                    } else {
                                        Action::new(action_kind)
                                    };
                                    self.dispatch_action(&action, &action_path);
                                    handled = true;
                                }
                            }
                        }

                        // 2. Named keys / special characters.
                        if !handled {
                            let is_in_textinput =
                                focused.is_some_and(|fid| self.event_registry.has_text_input(fid));
                            let lookup_key = match &key_event.logical_key {
                                winit::keyboard::Key::Character(c)
                                    if is_in_textinput && c == " " =>
                                {
                                    crate::event::Key::Character("?".into())
                                }
                                _ => map_winit_action_key(&key_event.logical_key),
                            };
                            if let Some(action_kind) =
                                self.key_bindings.find(focused, &lookup_key, &mods)
                            {
                                let action = if mods.shift {
                                    Action::new(action_kind).with_selection()
                                } else {
                                    Action::new(action_kind)
                                };
                                self.dispatch_action(&action, &action_path);
                                handled = true;
                            }
                        }

                        // Context-menu keyboard actions (Esc / activate / submenu leaf)
                        // mark portals for removal via dismiss_context_menu(). Drain
                        // them now, then invalidate the vacated region: arena.remove
                        // marks the parent for *relayout* but NOT repaint, so without
                        // this the overlay's pixels linger until the next event frame.
                        if handled {
                            let mut removed_any = false;
                            for removed in crate::platform::portal::drain_portal_removals() {
                                self.arena.remove(removed);
                                removed_any = true;
                            }
                            if removed_any {
                                self.invalidate_after_menu_change();
                            }
                        }

                        // Keyboard submenu open/close requests need arena + root_id,
                        // so they are fulfilled here (same pattern as HOVERED_SUBMENU).
                        if let Some(req) = crate::widgets::overlay::take_kb_menu_request() {
                            match req {
                                crate::widgets::overlay::KbMenuRequest::OpenSubmenu(row_eid) => {
                                    let info = self.arena.get(row_eid).map(|el| (
                                    el.screen_bounds,
                                    el.get_user_data::<crate::widgets::overlay::ContextMenuItems>().cloned(),
                                ));
                                    if let Some((sb, Some(cmi))) = info {
                                        let win_w = self.config.width / self.scale_factor as f32;
                                        let win_h = self.config.height / self.scale_factor as f32;
                                        let sub_h =
                                            cmi.0.iter().filter(|i| !i.separator).count().max(1)
                                                as f32
                                                * 32.0;
                                        let parent =
                                            crate::core::dirty_registry::parent_of(row_eid);
                                        let prefer_left = parent
                                        .and_then(|p| self.arena.get(p))
                                        .and_then(|pel| pel.get_user_data::<crate::widgets::overlay::MenuOpenDir>())
                                        .is_some_and(|d| d.0);
                                        let (sub_x, opened_left) =
                                            submenu_x(sb.x, sb.width, win_w, prefer_left);
                                        let sub_y = submenu_y(sb.y, sb.height, sub_h, win_h);
                                        if let Some(rid) = self.arena.root_id {
                                            crate::widgets::overlay::open_context_menu(
                                                cmi.0,
                                                crate::style::Point::new(sub_x, sub_y),
                                                &mut self.arena,
                                                rid,
                                                Some(&mut self.event_registry),
                                                parent,
                                                opened_left,
                                                win_h,
                                            );
                                            crate::widgets::overlay::mark_submenu_opened();
                                        }
                                    }
                                }
                                crate::widgets::overlay::KbMenuRequest::CloseSubmenu => {
                                    if let Some(parent) =
                                        crate::widgets::overlay::close_deepest_submenu(
                                            &mut self.arena,
                                        )
                                    {
                                        self.transfer_focus(parent, FocusReason::Programmatic);
                                    }
                                    self.invalidate_after_menu_change();
                                }
                            }
                        }

                        // 3. Fallback: printable character -> text_input.
                        if !handled {
                            if let winit::keyboard::Key::Character(c) = &key_event.logical_key {
                                if !c.is_empty() {
                                    if let Some(focused_id) = focused {
                                        if self.event_registry.has_text_input(focused_id) {
                                            for ch in c.chars() {
                                                self.event_registry.fire_text_input(focused_id, ch);
                                            }
                                            let _ = true;
                                        }
                                    }
                                }
                            }
                        }

                        // 4. Always emit KeyDown/KeyUp for widget-level handlers.
                        if let Some(k) = map_winit_key(&key_event.logical_key) {
                            let key_events = self.translator.keyboard_input(true, k);
                            self.dispatch_events(&key_events);
                        }
                    }
                    if let Some(ref w) = self.winit_window {
                        w.request_redraw();
                    }
                }
                winit::event::WindowEvent::Ime(ime) => {
                    use winit::event::Ime;
                    if let Some(focused_id) = self.focus_manager.focused() {
                        if self.event_registry.has_text_input(focused_id) {
                            match ime {
                                Ime::Commit(text) => {
                                    #[cfg(feature = "devtools")]
                                    crate::debug::devtools::record_interaction(
                                        self.frame_id,
                                        crate::debug::devtools::InteractionKind::ImeCommit {
                                            text: text.clone(),
                                        },
                                        auralis_signal::now_us(),
                                    );
                                    // Atomic path: the whole commit lands as one
                                    // edit. Fallback: per-char for widgets that
                                    // only register text_input.
                                    if self.event_registry.has_ime_commit(focused_id) {
                                        self.event_registry.fire_ime_commit(focused_id, text);
                                    } else {
                                        for ch in text.chars() {
                                            self.event_registry.fire_text_input(focused_id, ch);
                                        }
                                    }
                                    self.event_registry.fire_ime_preedit(
                                        focused_id,
                                        String::new(),
                                        None,
                                    );
                                }
                                Ime::Preedit(text, cursor) => {
                                    #[cfg(feature = "devtools")]
                                    crate::debug::devtools::record_interaction(
                                        self.frame_id,
                                        crate::debug::devtools::InteractionKind::ImePreedit {
                                            text: text.clone(),
                                            cursor_begin: cursor.map(|(s, _)| s),
                                            cursor_end: cursor.map(|(_, e)| e),
                                        },
                                        auralis_signal::now_us(),
                                    );
                                    self.event_registry.fire_ime_preedit(
                                        focused_id,
                                        text,
                                        cursor.map(|(s, e)| (s, e)),
                                    );
                                }
                                Ime::Enabled => {
                                    // Force the next sync to send a fresh area —
                                    // the OS IME just came up and needs coordinates
                                    // now, not after the next caret move (IME area
                                    // dedup P1).
                                    self.last_sent_ime_area = None;
                                }
                                Ime::Disabled => {
                                    self.last_sent_ime_area = None;
                                    // Do NOT set ime_suppressed here — that flag is
                                    // for widgets that permanently opt out of IME
                                    // (password fields). The OS will send Ime::Enabled
                                    // when the user reactivates the IME; we must
                                    // honour that (Ctrl+Space fix).
                                }
                                Ime::DeleteSurrounding {
                                    before_bytes,
                                    after_bytes,
                                } => {
                                    self.event_registry.fire_ime_delete_surrounding(
                                        focused_id,
                                        before_bytes,
                                        after_bytes,
                                    );
                                }
                            }
                        }
                    }
                }
                winit::event::WindowEvent::ThemeChanged(theme) => {
                    if let Some(ref sig) = self.theme_signal {
                        let is_dark = matches!(theme, winit::window::Theme::Dark);
                        let seed = Color::rgba8(0x67, 0x79, 0xE8, 0xFF);
                        if is_dark {
                            sig.set(M3Theme::from_seed(seed).with_is_dark(true));
                        } else {
                            sig.set(M3Theme::from_seed(seed));
                        }
                        let new_theme = sig.read();
                        self.config.theme = new_theme;
                        self.needs_theme_reapply = true;
                        if let Some(ref mut r) = self.renderer {
                            r.set_clear_color(self.config.theme.scheme.surface);
                        }
                    }
                }
                // ── Touch gestures (macOS / Wayland) ──
                winit::event::WindowEvent::PinchGesture { delta, phase, .. } => {
                    let pos = self.last_cursor.unwrap_or(crate::style::Point::ZERO);
                    if let Some(result) = crate::event::hit_test::hit_test(&self.arena, pos) {
                        let gphase = map_touch_phase(phase);
                        let modifiers = self.translator.modifiers();
                        let _ = crate::event::propagation::dispatch_event(
                            &mut self.arena,
                            &crate::event::Event::Pinch {
                                delta,
                                position: pos,
                                phase: gphase,
                            },
                            &result.path,
                            &mut self.focus_manager,
                            &mut self.event_registry,
                            modifiers,
                        );
                    }
                }
                winit::event::WindowEvent::RotationGesture { delta, phase, .. } => {
                    let pos = self.last_cursor.unwrap_or(crate::style::Point::ZERO);
                    if let Some(result) = crate::event::hit_test::hit_test(&self.arena, pos) {
                        let gphase = map_touch_phase(phase);
                        let modifiers = self.translator.modifiers();
                        let _ = crate::event::propagation::dispatch_event(
                            &mut self.arena,
                            &crate::event::Event::Rotate {
                                delta,
                                position: pos,
                                phase: gphase,
                            },
                            &result.path,
                            &mut self.focus_manager,
                            &mut self.event_registry,
                            modifiers,
                        );
                    }
                }
                // ── Drag & Drop ──
                winit::event::WindowEvent::DragEntered { paths, position } => {
                    let sf = self.scale_factor;
                    let pos = crate::style::Point::new(
                        position.x as f32 / sf as f32,
                        position.y as f32 / sf as f32,
                    );
                    if let Some(hit) = dirty_registry::hit_test_with_fallback(&self.arena, pos) {
                        if let Some(el) = self.arena.get_mut(hit) {
                            if el.drop_target() {
                                el.state
                                    .set(el.state.get() | crate::core::config::StateFlags::HOVERED);
                                el.mark_repaint();
                            }
                        }
                    }
                    let _ = paths;
                }
                winit::event::WindowEvent::DragMoved { position } => {
                    let sf = self.scale_factor;
                    let pos = crate::style::Point::new(
                        position.x as f32 / sf as f32,
                        position.y as f32 / sf as f32,
                    );
                    if let Some(hit) = dirty_registry::hit_test_with_fallback(&self.arena, pos) {
                        if let Some(el) = self.arena.get(hit) {
                            if el.drop_target() {
                                el.mark_repaint();
                            }
                        }
                    }
                }
                winit::event::WindowEvent::DragDropped { paths, position } => {
                    let sf = self.scale_factor;
                    let pos = crate::style::Point::new(
                        position.x as f32 / sf as f32,
                        position.y as f32 / sf as f32,
                    );
                    if let Some(hit) = dirty_registry::hit_test_with_fallback(&self.arena, pos) {
                        if let Some(el) = self.arena.get_mut(hit) {
                            if el.drop_target() {
                                let drag_data = crate::event::DragData {
                                    kind: crate::event::DragKind::Files,
                                    text: None,
                                    paths,
                                    position: Some(pos),
                                    label: None,
                                };
                                if let Some(ref handler) = el.on_drop() {
                                    handler(drag_data);
                                }
                            }
                        }
                    }
                    let _ = paths;
                }
                winit::event::WindowEvent::DragLeft { .. } => {}
                winit::event::WindowEvent::Focused(gained) => {
                    if !gained {
                        if let Some(old_id) = self.focus_manager.focused() {
                            if let Some(el) = self.arena.get_mut(old_id) {
                                el.set_state_dirty(StateFlags::FOCUSED, false);
                                el.last_focus_reason
                                    .set(Some(FocusReason::WindowActivation));
                            }
                            self.event_registry
                                .fire_focus_out(old_id, FocusReason::WindowActivation);
                            self.focus_manager.set_focused(None);
                        }
                    } else {
                        self.skip_frame = false;
                        if let Some(ref w) = win {
                            w.request_redraw();
                        }
                    }
                }
                winit::event::WindowEvent::Occluded(occluded) => {
                    self.skip_frame = occluded;
                    if !occluded {
                        if let Some(ref w) = win {
                            w.request_redraw();
                        }
                    }
                }
                winit::event::WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                    self.scale_factor = scale_factor;
                    if let Some(ref mut r) = self.renderer {
                        r.set_scale_factor_gpu(scale_factor);
                        r.resize_cpu(self.config.width, self.config.height, scale_factor as f32);
                    }
                    if let Some(ref w) = win {
                        w.request_redraw();
                    }
                }
                _ => {}
            }
            self.flush_scheduler.drain();
            auralis_task::drain_deferred_signal_callbacks();

            if let Some(_root_id) = rid {
                if crate::core::dirty_registry::has_pending_dirty() {
                    self.on_frame();
                }
            }
        }));
        if let Err(panic) = result {
            push_error(UiError::CallbackPanic {
                context: "handle_event".into(),
                window_id: self
                    .window_handle
                    .as_ref()
                    .map(|h| h.id().into_raw() as u64),
                element_id: None,
                message: panic_to_string(&panic),
            });
            self.needs_rebuild = true;
        }
    }

    fn about_to_wait(&mut self, _event_loop: &dyn ActiveEventLoop) {
        if self.needs_cleanup {
            self.touches.clear();
            self.drag_state = None;
            self.scrollbar_drag = None;
            self.focus_manager.clear();
            self.needs_cleanup = false;
        }

        self.flush_scheduler.drain();
        auralis_task::drain_deferred_signal_callbacks();

        let deferred = self.focus_manager.drain_deferred_blurs();
        for fid in deferred {
            if let Some(el) = self.arena.get_mut(fid) {
                el.set_state_dirty(StateFlags::FOCUSED, false);
                el.last_focus_reason.set(Some(FocusReason::PointerClick));
            }
            self.event_registry
                .fire_focus_out(fid, FocusReason::PointerClick);
            if self.focus_manager.focused() == Some(fid) {
                self.focus_manager.set_focused(None);
            }
        }

        let Some(root_id) = self.arena.root_id else {
            if let Some(ref w) = self.winit_window {
                w.request_redraw();
            }
            return;
        };
        if crate::core::dirty_registry::has_pending_dirty() {
            self.on_frame();
            while crate::core::dirty_registry::has_pending_dirty() {
                self.coalesce_skip_paint = true;
                self.on_frame();
            }
            self.coalesce_skip_paint = false;
            if self
                .arena
                .get(root_id)
                .is_some_and(|r| r.dirty.get().has_repaint())
            {
                self.on_frame();
            }
        }
        // ── Scheduler: reconcile this window's feature-level continuous
        //    subscriptions. Keyed acquire/release (audit 2026-07-18
        //    animation pass) — widget-held keys (toast transitions,
        //    spinners) are never touched here, so no owner can evict
        //    another owner's wake. Discrete deadlines (tooltip, blink)
        //    are handled by WaitUntil in WinState::about_to_wait and
        //    expired_discrete here — no per-frame busy-pump.
        {
            use crate::core::scheduler::{self, keys};
            // Renewal sweep: element wakes (spinners) decay unless their
            // frame_tick renewed them this turn (hidden ticks are skipped
            // by the tick pass, so hiding stops the renewal).
            scheduler::sweep_stale_element_wakes();
            // Visibility-gated: fully-hidden animations let the loop sleep;
            // the reactive_visible flip that reveals them registers dirty,
            // which wakes the loop and resumes ticking at the right phase.
            if self.animations.has_active_visible(&self.arena) {
                scheduler::acquire_continuous(keys::ANIM_DRIVER);
            } else {
                scheduler::release_continuous(keys::ANIM_DRIVER);
            }
            if crate::core::dirty_registry::is_exit_pending_active() {
                scheduler::acquire_continuous(keys::EXIT_ANIMS);
            } else {
                scheduler::release_continuous(keys::EXIT_ANIMS);
            }
        }

        // ── IME cursor area sync (P1): after the frame settles, tell the
        //    OS where the composition caret sits. Runs once per frame,
        //    O(1), with dedup so we don't spam the platform.
        self.sync_ime_cursor_area();

        let needs_redraw = self.paint_occurred_this_frame
            || crate::core::scheduler::has_continuous()
            || crate::core::scheduler::expired_discrete()
            || crate::core::dirty_registry::has_pending_dirty()
            // Liveness: a defer_action queued outside the frame (event
            // callback) without an accompanying register_dirty would
            // otherwise sit until the next incidental frame. TestHarness
            // already checks this (test_harness.rs quiescence gate) — the
            // real window must match, or harness-green code can still
            // freeze UI updates in production. (audit 2026-07-15, C4)
            || crate::core::dirty_registry::has_pending_actions();
        self.paint_occurred_this_frame = false;
        if needs_redraw {
            if let Some(ref w) = self.winit_window {
                w.request_redraw();
            }
        }
    }

    /// Per-frame IME caret-area sync (IME area dedup P1).
    /// O(1) — reads the focused element's cached cursor rect from ECS.
    fn sync_ime_cursor_area(&mut self) {
        let Some(ref w) = self.winit_window else {
            return;
        };
        let Some(fid) = self.focus_manager.focused() else {
            return;
        };
        if !self.event_registry.has_text_input(fid) || self.event_registry.is_ime_suppressed(fid) {
            return;
        }
        let bounds =
            crate::core::dirty_registry::bounds_of(fid).unwrap_or(crate::style::Rect::ZERO);
        let sf = self.scale_factor as f32;
        let bounds_logical = crate::style::Rect::new(
            bounds.x / sf,
            bounds.y / sf,
            bounds.width / sf,
            bounds.height / sf,
        );
        let (local, _) = crate::core::element::with_ct(|ct| {
            let local = ct.cursor.get(&fid).and_then(|c| c.ime_cursor_rect.get());
            (local, ())
        });
        // Ancestor-accumulated scroll (not the element's own — TextInput may
        // sit inside a ScrollView).  O(1) via generation cache.
        let asc_scroll = crate::core::dirty_registry::accumulated_scroll_cached(&self.arena, fid);
        let area =
            crate::platform::ime::compose_ime_surface_rect(bounds_logical, local, asc_scroll);

        // Dedup: skip when the caret hasn't moved beyond 0.5 logical px.
        if let Some(last) = self.last_sent_ime_area {
            if (area.x - last.x).abs() < 0.5 && (area.y - last.y).abs() < 0.5 {
                return;
            }
        }

        let _ = w.request_ime_update(winit::window::ImeRequest::Update(
            winit::window::ImeRequestData::default().with_cursor_area(
                winit::dpi::Position::Logical(winit::dpi::LogicalPosition::new(
                    f64::from(area.x),
                    f64::from(area.y),
                )),
                winit::dpi::Size::Logical(winit::dpi::LogicalSize::new(
                    f64::from(area.width.max(1.0)),
                    f64::from(area.height.max(1.0)),
                )),
            ),
        ));
        self.last_sent_ime_area = Some(area);
    }
}

// ── Tree walking helpers ──────────────────────────────────────────

fn cancel_path_for_visible(arena: &ElementArena, root_id: ElementId) -> Vec<ElementId> {
    let mut path = Vec::new();
    if find_cancel_handler(arena, root_id, &mut path) {
        path
    } else {
        Vec::new()
    }
}

fn find_cancel_handler(arena: &ElementArena, eid: ElementId, path: &mut Vec<ElementId>) -> bool {
    let el = match arena.get(eid) {
        Some(e) => e,
        None => return false,
    };
    if !el.is_visible() {
        return false;
    }
    path.push(eid);
    let child_ids = &el.children;
    for cid in child_ids.iter().rev() {
        if find_cancel_handler(arena, *cid, path) {
            return true;
        }
    }
    if el.reactive_visible().is_some() || (el.z_index > 0 && el.is_focusable()) {
        return true;
    }
    path.pop();
    false
}

// (update_portal_positions moved to platform/portal.rs — audit round 3, ② phase 1)

// ── Progress paint ─────────────────────────────────────────────
// (moved to widgets/display/progress.rs — audit round 3, ② phase 1)
// ── Slider paint ───────────────────────────────────────────────
// (moved to widgets/input/slider.rs — audit round 3, ② phase 1)

// ── Color plane / hue bar / alpha bar paint ───────────────────
// (moved to widgets/input/color_picker.rs — audit round 3, ② phase 1)

/// O(k) cursor blink pass: iterate only cursor-component elements,
/// avoiding full tree walk during paint. Toggles cursor_visible every 500ms.
pub(crate) fn process_cursor_blink(arena: &ElementArena) {
    use crate::ecs::active::{drain_active, register_active, ActiveTag};
    for eid in drain_active(ActiveTag::CursorBlink) {
        let cursor = match arena.component_tables.borrow().cursor.get(&eid).cloned() {
            Some(c) => c,
            None => continue,
        };
        let is_focused = cursor.cursor_focused.get();
        let last_input = cursor.cursor_blink_last_input.get();
        let elapsed = crate::core::clock::now()
            .duration_since(last_input)
            .as_millis() as u64;
        let should_be_visible = if !is_focused {
            false
        } else {
            (elapsed / 500).is_multiple_of(2)
        };
        if cursor.cursor_visible.get() != should_be_visible {
            cursor.cursor_visible.set(should_be_visible);
            if let Some(el) = arena.get(eid) {
                el.mark_repaint();
            }
        }
        if is_focused {
            // Scheduler: next blink toggle at the next 500ms boundary.
            let phase = elapsed / 500;
            let next_ms = (phase + 1) * 500;
            let next_deadline = last_input + std::time::Duration::from_millis(next_ms);
            crate::core::scheduler::schedule_at(
                next_deadline,
                crate::core::scheduler::keys::CURSOR_BLINK,
            );
            register_active(eid, ActiveTag::CursorBlink);
        }
    }
}

/// O(active) frame_tick pass: iterate only elements registered via the
/// active-set (not the full LifecycleComponent table).
pub(crate) fn process_frame_ticks(arena: &ElementArena) {
    use crate::ecs::active::{drain_active, register_active, ActiveTag};
    let active_eids: Vec<ElementId> = drain_active(ActiveTag::FrameTick).into_iter().collect();
    let (to_run, others): (Vec<_>, Vec<_>) = active_eids.into_iter().partition(|eid| {
        let Some(lc) = arena.component_tables.borrow().lc.get(eid).cloned() else {
            return false;
        };
        let Some(_tick) = lc.frame_tick.as_ref() else {
            return false;
        };
        if crate::core::dirty_registry::is_slot_inactive_in_ancestry(*eid, arena) {
            return false;
        }
        if crate::core::dirty_registry::is_reactive_hidden_in_ancestry(*eid) {
            return false;
        }
        true
    });
    let ticks: Vec<_> = to_run
        .iter()
        .filter_map(|&eid| {
            let lc = arena.component_tables.borrow().lc.get(&eid).cloned()?;
            let tick = lc.frame_tick.as_ref()?;
            Some((eid, tick.clone()))
        })
        .collect();
    for (eid, tick) in ticks {
        if let Some(f) = tick.borrow_mut().as_mut() {
            f();
        }
        // Re-register if the tick callback is still installed (it may have been
        // cleared during the callback, e.g. one-shot end-of-animation cleanup).
        let still_installed = arena
            .component_tables
            .borrow()
            .lc
            .get(&eid)
            .and_then(|lc| lc.frame_tick.as_ref())
            .is_some();
        if still_installed {
            register_active(eid, ActiveTag::FrameTick);
        }
    }
    // Re-register filtered-out elements so they can fire when they become
    // visible again (e.g. toast container hidden between toasts).
    for eid in others {
        let still_installed = arena
            .component_tables
            .borrow()
            .lc
            .get(&eid)
            .and_then(|lc| lc.frame_tick.as_ref())
            .is_some();
        if still_installed {
            register_active(eid, ActiveTag::FrameTick);
        }
    }
}
// ── Scroll helpers ────────────────────────────────────────────────

// Scrollbar geometry / do_scroll / velocity / pending scrolls moved to
// widgets/bundle/scroll.rs (audit round 5 - phase 2: ScrollBundle domain).

// ── winit helpers ─────────────────────────────────────────────────

fn map_mouse_button(btn: winit::event::MouseButton) -> crate::event::MouseButton {
    match btn {
        winit::event::MouseButton::Left => crate::event::MouseButton::Left,
        winit::event::MouseButton::Right => crate::event::MouseButton::Right,
        winit::event::MouseButton::Middle => crate::event::MouseButton::Middle,
        winit::event::MouseButton::Back => crate::event::MouseButton::Back,
        winit::event::MouseButton::Forward => crate::event::MouseButton::Forward,
        _ => crate::event::MouseButton::Other(0),
    }
}

fn map_touch_phase(phase: winit::event::TouchPhase) -> crate::event::GesturePhase {
    match phase {
        winit::event::TouchPhase::Started => crate::event::GesturePhase::Started,
        winit::event::TouchPhase::Moved => crate::event::GesturePhase::Moved,
        winit::event::TouchPhase::Ended => crate::event::GesturePhase::Ended,
        winit::event::TouchPhase::Cancelled => crate::event::GesturePhase::Cancelled,
    }
}

fn map_winit_key(logical: &winit::keyboard::Key) -> Option<crate::event::Key> {
    use winit::keyboard::{Key, NamedKey};
    Some(match logical {
        Key::Character(c) if c == "\r" => crate::event::Key::Enter,
        Key::Character(c) if c == "\t" => crate::event::Key::Tab,
        Key::Character(c) if c == "\u{8}" => crate::event::Key::Backspace,
        Key::Character(c) if c == "\u{1b}" => crate::event::Key::Escape,
        Key::Character(c) if c == " " => crate::event::Key::Space,
        Key::Character(c) => crate::event::Key::Character(c.to_string()),
        Key::Named(NamedKey::Enter) => crate::event::Key::Enter,
        Key::Named(NamedKey::Tab) => crate::event::Key::Tab,
        Key::Named(NamedKey::Backspace) => crate::event::Key::Backspace,
        Key::Named(NamedKey::Escape) => crate::event::Key::Escape,
        Key::Named(NamedKey::ArrowUp) => crate::event::Key::ArrowUp,
        Key::Named(NamedKey::ArrowDown) => crate::event::Key::ArrowDown,
        Key::Named(NamedKey::ArrowLeft) => crate::event::Key::ArrowLeft,
        Key::Named(NamedKey::ArrowRight) => crate::event::Key::ArrowRight,
        _ => return None,
    })
}

fn map_winit_action_key(logical: &winit::keyboard::Key) -> crate::event::Key {
    use winit::keyboard::{Key, NamedKey};
    match logical {
        Key::Character(c) if c == "\t" => crate::event::Key::Tab,
        Key::Character(c) if c == " " => crate::event::Key::Space,
        Key::Character(c) if c == "\r" => crate::event::Key::Enter,
        Key::Character(c) if c == "\u{1b}" => crate::event::Key::Escape,
        Key::Named(NamedKey::Enter) => crate::event::Key::Enter,
        Key::Named(NamedKey::Tab) => crate::event::Key::Tab,
        Key::Named(NamedKey::Backspace) => crate::event::Key::Backspace,
        Key::Named(NamedKey::Delete) => crate::event::Key::Delete,
        Key::Named(NamedKey::Escape) => crate::event::Key::Escape,
        Key::Named(NamedKey::ArrowUp) => crate::event::Key::ArrowUp,
        Key::Named(NamedKey::ArrowDown) => crate::event::Key::ArrowDown,
        Key::Named(NamedKey::ArrowLeft) => crate::event::Key::ArrowLeft,
        Key::Named(NamedKey::ArrowRight) => crate::event::Key::ArrowRight,
        Key::Named(NamedKey::Home) => crate::event::Key::Home,
        Key::Named(NamedKey::End) => crate::event::Key::End,
        _ => crate::event::Key::Character("?".into()),
    }
}

/// Compute a submenu's x and the direction it actually opened.
/// `prefer_left` is the parent menu's open direction, inherited so a deep
/// cascade keeps going the same way instead of zig-zagging back onto an
/// ancestor menu. When neither side fits, clamp into the screen — the submenu
/// stays fully visible; it may overlap the parent (an unavoidable physical
/// limit, as on Windows).
fn submenu_x(parent_x: f32, parent_w: f32, screen_w: f32, prefer_left: bool) -> (f32, bool) {
    let w = 220.0_f32;
    let gap = 4.0_f32;
    let right_x = parent_x + parent_w + gap;
    let left_x = parent_x - w - gap;
    let fits_right = right_x + w <= screen_w;
    let fits_left = left_x >= 0.0;
    let max_x = (screen_w - w).max(0.0);
    if prefer_left {
        if fits_left {
            (left_x, true)
        } else if fits_right {
            (right_x, false)
        } else {
            (left_x.clamp(0.0, max_x), true)
        }
    } else {
        if fits_right {
            (right_x, false)
        } else if fits_left {
            (left_x, true)
        } else {
            (right_x.clamp(0.0, max_x), false)
        }
    }
}

fn submenu_y(parent_y: f32, parent_h: f32, sub_h: f32, screen_h: f32) -> f32 {
    if parent_y + sub_h > screen_h {
        // Not enough room below: open upward, aligning the submenu's bottom with
        // the parent row's bottom so the two menus stay visually connected.
        // (Uses the real submenu height, not a fixed estimate — otherwise a
        // shorter submenu leaves a gap when flipped.)
        (parent_y + parent_h - sub_h).max(0.0)
    } else {
        parent_y
    }
}

fn request_ime_enable(
    w: &Arc<dyn winit::window::Window>,
    registry: &crate::event::EventRegistry,
    id: ElementId,
) -> bool {
    if registry.has_text_input(id) && !registry.is_ime_suppressed(id) {
        let sf = w.scale_factor() as f32;
        let bounds = crate::core::dirty_registry::bounds_of(id).unwrap_or(crate::style::Rect::ZERO);
        let bounds_logical = crate::style::Rect::new(
            bounds.x / sf,
            bounds.y / sf,
            bounds.width / sf,
            bounds.height / sf,
        );
        let (local, _) = crate::core::element::with_ct(|ct| {
            let local = ct.cursor.get(&id).and_then(|c| c.ime_cursor_rect.get());
            (local, ())
        });
        // Note: enable fires during focus-transfer, before the first frame
        // layout — bounds may be stale and ancestor scroll unavailable here.
        // sync_ime_cursor_area corrects the position within one frame.
        let area =
            crate::platform::ime::compose_ime_surface_rect(bounds_logical, local, (0.0, 0.0));
        let _ = w.request_ime_update(winit::window::ImeRequest::Enable(
            winit::window::ImeEnableRequest::new(
                winit::window::ImeCapabilities::new()
                    .with_cursor_area()
                    .with_hint_and_purpose(),
                winit::window::ImeRequestData::default()
                    .with_hint_and_purpose(
                        winit::window::ImeHint::NONE,
                        winit::window::ImePurpose::Normal,
                    )
                    .with_cursor_area(
                        winit::dpi::Position::Logical(winit::dpi::LogicalPosition::new(
                            f64::from(area.x),
                            f64::from(area.y),
                        )),
                        winit::dpi::Size::Logical(winit::dpi::LogicalSize::new(
                            f64::from(area.width.max(1.0)),
                            f64::from(area.height.max(1.0)),
                        )),
                    ),
            )
            .unwrap(),
        ));
        true
    } else {
        let _ = w.request_ime_update(winit::window::ImeRequest::Disable);
        false
    }
}
