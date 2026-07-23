//! Top-level `App` — application entry point, window registry, and event-loop runner.

use crate::core::widget::Widget;
use auralis_task::scheduler::DeferredScheduler;
use rustc_hash::FxHashMap;
use std::rc::Rc;

use super::config::WindowConfig;
use super::WindowState;

/// Create with [`App::new`], register windows with [`App::add_window`],
/// then call [`App::run`] to start the event loop.
pub struct App {
    pub(crate) windows: FxHashMap<winit::window::WindowId, WindowState>,
    pub(crate) pending: Vec<(WindowConfig, Box<dyn Widget>)>,
    pub(crate) flush_scheduler: Rc<DeferredScheduler>,
    pub(crate) max_windows: Option<usize>,
    #[cfg(feature = "global-hotkey")]
    pub(crate) hotkey_manager: crate::platform::global_hotkey::GlobalHotkeyManager,
    #[cfg(any(feature = "devtools", feature = "file-logging"))]
    #[allow(dead_code)]
    pub(crate) logging_guard: Option<crate::logging::LoggingGuard>,
    #[cfg(feature = "devtools")]
    pub(crate) devtools_buf: crate::debug::devtools::DevtoolsRingBuffer,
}

thread_local! {
    /// Queue of windows requested during widget callbacks.  Drained at the
    /// end of [`App::window_event`] so that `ActiveEventLoop` is still on
    /// the call stack and can create new winit windows.
    pub(crate) static PENDING_WINDOWS: std::cell::RefCell<Vec<(WindowConfig, Box<dyn Widget>)>>
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
    pub fn builder() -> super::app_builder::AppBuilder {
        super::app_builder::AppBuilder {
            config: super::app_builder::AppBuilderConfig::default(),
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
    #[cfg(feature = "global-hotkey")]
    pub fn register_global_hotkey(
        &mut self,
        chord_str: &str,
        action: crate::event::action::ActionKind,
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
