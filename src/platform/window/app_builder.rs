//! `AppBuilder` — builder-pattern API for configuring [`App`] before running.

use crate::core::error::UiError;
use crate::core::widget::Widget;
use std::rc::Rc;

use super::app::App;
use super::config::WindowConfig;

/// Builder-pattern API for configuring [`App`] before running.
pub struct AppBuilder {
    pub(crate) config: AppBuilderConfig,
    pub(crate) pending: Vec<(WindowConfig, Box<dyn Widget>)>,
    pub(crate) max_windows: Option<usize>,
}

#[derive(Default)]
pub(crate) struct AppBuilderConfig {
    pub(crate) error_handler: Option<Rc<dyn Fn(&UiError)>>,
    pub(crate) fatal_handler: Option<Rc<dyn Fn(&UiError)>>,
    pub(crate) error_buffer_limit: usize,
    pub(crate) logging: Option<crate::logging::LoggingConfig>,
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
            crate::core::error::set_error_handler(handler.clone());
        }
        if let Some(ref handler) = self.config.fatal_handler {
            crate::core::error::set_fatal_handler(handler.clone());
        }
        crate::core::error::set_error_buffer_limit(self.config.error_buffer_limit);

        auralis_task::set_panic_hook(Rc::new(|info: auralis_task::PanicInfo| {
            crate::core::error::push_error(UiError::CallbackPanic {
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
            windows: rustc_hash::FxHashMap::default(),
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
