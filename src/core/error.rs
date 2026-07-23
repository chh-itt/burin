use rustc_hash::FxHashMap;
use std::any::Any;
use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::rc::Rc;
use thiserror::Error;

// ── Severity ──

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[cfg_attr(feature = "devtools", derive(serde::Serialize, serde::Deserialize))]
pub enum ErrorSeverity {
    Fatal,
    Recoverable,
    Warning,
}

// ── Error Record ──

#[derive(Debug)]
pub struct ErrorRecord {
    pub severity: ErrorSeverity,
    pub error: UiError,
    pub timestamp: web_time::Instant,
    pub frame_id: Option<u64>,
    #[cfg(feature = "devtools")]
    pub backtrace: Option<std::backtrace::Backtrace>,
}

// ── UiError ──

#[derive(Error, Debug)]
#[cfg_attr(feature = "devtools", derive(serde::Serialize, serde::Deserialize))]
pub enum UiError {
    // ── A: Window / Platform ──
    #[error("failed to create window: {0}")]
    WindowCreate(String),

    #[error("surface creation failed: {0}")]
    SurfaceCreate(String),

    #[cfg(feature = "clipboard")]
    #[error("clipboard error: {0}")]
    Clipboard(#[from] crate::platform::clipboard::ClipboardError),

    #[cfg(feature = "tray")]
    #[error("tray icon error: {0}")]
    Tray(#[from] crate::platform::tray::TrayError),

    #[cfg(feature = "display-advanced")]
    #[error("display error: {0}")]
    Display(#[from] crate::platform::display::DisplayError),

    // ── B: GPU ──
    #[error("GPU initialization failed: {0}")]
    GpuInit(#[source] GpuErrorKind),

    /// GPU device lost (reserved for future device-loss recovery).
    /// Not yet constructed — the wgpu renderer does not currently detect
    /// device loss events.
    #[allow(dead_code)]
    #[error("GPU device lost")]
    GpuDeviceLost,

    #[error("GPU render error: {0}")]
    GpuRender(String),

    // ── D: Widget / Callback ──
    #[error("widget callback panic in {context}: {message}")]
    CallbackPanic {
        context: String,
        window_id: Option<u64>,
        element_id: Option<crate::core::ElementId>,
        message: String,
    },

    // ── E: Resource ──
    #[cfg(feature = "ext-image")]
    #[error("image error: {0}")]
    Image(#[from] crate::resource::ImageError),

    #[cfg(feature = "ext-svg")]
    #[error("SVG error: {0}")]
    Svg(String),

    #[cfg(feature = "ext-audio")]
    #[error("audio error: {0}")]
    Audio(#[from] crate::audio::AudioError),

    #[cfg(feature = "i18n")]
    #[error("i18n error: {0}")]
    I18n(#[from] crate::i18n::I18nError),

    #[error("font loading failed: {0}")]
    FontLoad(String),

    // ── F: Layout (config-time) ──
    #[error("grid layout error: {0}")]
    GridLayout(String),
}

impl UiError {
    pub fn severity(&self) -> ErrorSeverity {
        match self {
            Self::WindowCreate(_) | Self::SurfaceCreate(_) => ErrorSeverity::Fatal,
            Self::GpuInit(_) | Self::GpuDeviceLost | Self::GpuRender(_) => {
                ErrorSeverity::Recoverable
            }
            Self::CallbackPanic { .. } => ErrorSeverity::Recoverable,
            Self::Image(_) | Self::Svg(_) | Self::FontLoad(_) | Self::GridLayout(_) => {
                ErrorSeverity::Warning
            }
            #[cfg(feature = "ext-audio")]
            Self::Audio(_) => ErrorSeverity::Warning,
            #[cfg(feature = "clipboard")]
            Self::Clipboard(_) => ErrorSeverity::Warning,
            #[cfg(feature = "tray")]
            Self::Tray(_) => ErrorSeverity::Warning,
            #[cfg(feature = "display-advanced")]
            Self::Display(_) => ErrorSeverity::Warning,
            #[cfg(feature = "i18n")]
            Self::I18n(_) => ErrorSeverity::Warning,
        }
    }

    /// Returns the variant name as a static string for error counting.
    ///
    /// **Maintenance note:** When adding a new variant to `UiError`, you must:
    /// 1. Add a match arm here
    /// 2. Add a severity mapping in `UiError::severity()`
    /// 3. Both arms must use the same variant path
    pub(crate) fn variant_name(&self) -> &'static str {
        match self {
            Self::WindowCreate(_) => "WindowCreate",
            Self::SurfaceCreate(_) => "SurfaceCreate",
            #[cfg(feature = "clipboard")]
            Self::Clipboard(_) => "Clipboard",
            #[cfg(feature = "tray")]
            Self::Tray(_) => "Tray",
            #[cfg(feature = "display-advanced")]
            Self::Display(_) => "Display",
            Self::GpuInit(_) => "GpuInit",
            Self::GpuDeviceLost => "GpuDeviceLost",
            Self::GpuRender(_) => "GpuRender",
            Self::CallbackPanic { .. } => "CallbackPanic",
            #[cfg(feature = "ext-image")]
            Self::Image(_) => "Image",
            #[cfg(feature = "ext-svg")]
            Self::Svg(_) => "Svg",
            #[cfg(feature = "ext-audio")]
            Self::Audio(_) => "Audio",
            #[cfg(feature = "i18n")]
            Self::I18n(_) => "I18n",
            Self::FontLoad(_) => "FontLoad",
            Self::GridLayout(_) => "GridLayout",
        }
    }
}

// ── GpuErrorKind ──

#[derive(Error, Debug)]
#[cfg_attr(feature = "devtools", derive(serde::Serialize, serde::Deserialize))]
pub enum GpuErrorKind {
    #[error("no suitable GPU adapter found")]
    NoAdapter,
    #[error("surface configuration failed")]
    Surface,
    #[error("device creation failed")]
    Device,
    #[error("unknown: {0}")]
    Other(String),
}

// ── Thread-Local Storage ──

thread_local! {
    static ERROR_BUFFER: RefCell<VecDeque<ErrorRecord>> =
        const { RefCell::new(VecDeque::new()) };
    static ERROR_HANDLER: RefCell<Option<Rc<dyn Fn(&UiError)>>> =
        const { RefCell::new(None) };
    static FATAL_HANDLER: RefCell<Option<Rc<dyn Fn(&UiError)>>> =
        const { RefCell::new(None) };
    static ERROR_LIMIT: Cell<usize> = const { Cell::new(128) };
    static ERROR_COUNTS: RefCell<FxHashMap<&'static str, u64>> =
        RefCell::new(FxHashMap::default());
    static CURRENT_FRAME_ID: Cell<Option<u64>> = const { Cell::new(None) };
}

// ── Public API ──

pub fn push_error(err: UiError) {
    push_error_impl(err, None);
}

pub fn push_error_with_severity(err: UiError, severity: ErrorSeverity) {
    push_error_impl(err, Some(severity));
}

fn push_error_impl(err: UiError, severity_override: Option<ErrorSeverity>) {
    let severity = severity_override.unwrap_or_else(|| err.severity());
    let is_fatal = severity == ErrorSeverity::Fatal;
    let variant_name = err.variant_name();

    #[cfg(debug_assertions)]
    eprintln!("[burin] [{severity:?}] {}", err);

    ERROR_COUNTS.with(|c| {
        *c.borrow_mut().entry(variant_name).or_insert(0) += 1;
    });

    #[cfg(any(feature = "devtools", feature = "file-logging"))]
    tracing::error!(target: "burin", "{:?}", err);

    ERROR_HANDLER.with(|h| {
        if let Some(ref handler) = *h.borrow() {
            handler(&err);
        }
    });

    if is_fatal {
        FATAL_HANDLER.with(|h| {
            if let Some(ref handler) = *h.borrow() {
                handler(&err);
            } else {
                std::panic::panic_any(err);
            }
        });
        return;
    }

    ERROR_BUFFER.with(|buf| {
        let mut buf = buf.borrow_mut();
        let limit = ERROR_LIMIT.get();
        if limit > 0 && buf.len() >= limit {
            buf.pop_front();
        }
        buf.push_back(ErrorRecord {
            severity,
            timestamp: web_time::Instant::now(),
            frame_id: CURRENT_FRAME_ID.get(),
            error: err,
            #[cfg(feature = "devtools")]
            backtrace: Some(std::backtrace::Backtrace::force_capture()),
        });
    });
}

pub fn set_error_handler(handler: Rc<dyn Fn(&UiError)>) {
    ERROR_HANDLER.with(|h| *h.borrow_mut() = Some(handler));
}

/// Register a handler for fatal errors.
///
/// When a `Fatal`-severity error is pushed, this handler is invoked.
/// If no handler is registered, the default behaviour is to panic.
///
/// **IMPORTANT:** The handler runs synchronously and blocks `push_error`.
/// It must not perform UI operations (e.g., opening a dialog) because the
/// window system may already be in an invalid state. The handler should
/// limit itself to logging, flushing state to disk, and signalling an
/// external watchdog to restart the process.
pub fn set_fatal_handler(handler: Rc<dyn Fn(&UiError)>) {
    FATAL_HANDLER.with(|h| *h.borrow_mut() = Some(handler));
}

pub fn set_error_buffer_limit(limit: usize) {
    ERROR_LIMIT.set(limit);
}

pub fn set_current_frame_id(id: Option<u64>) {
    CURRENT_FRAME_ID.set(id);
}

pub fn error_counts() -> Vec<(&'static str, u64)> {
    ERROR_COUNTS.with(|c| c.borrow().iter().map(|(k, v)| (*k, *v)).collect())
}

#[cfg(feature = "devtools")]
#[derive(Debug, Clone)]
pub struct ErrorSummary {
    pub severity: ErrorSeverity,
    pub message: String,
    pub variant: &'static str,
    pub timestamp_millis: u128,
    pub frame_id: Option<u64>,
}

#[cfg(feature = "devtools")]
pub fn recent_errors() -> Vec<ErrorSummary> {
    ERROR_BUFFER.with(|buf| {
        buf.borrow()
            .iter()
            .map(|r| ErrorSummary {
                severity: r.severity,
                message: r.error.to_string(),
                variant: r.error.variant_name(),
                timestamp_millis: r.timestamp.elapsed().as_millis(),
                frame_id: r.frame_id,
            })
            .collect()
    })
}

pub(crate) fn panic_to_string(panic: &Box<dyn Any + Send>) -> String {
    panic
        .downcast_ref::<String>()
        .cloned()
        .or_else(|| panic.downcast_ref::<&str>().map(|s| s.to_string()))
        .unwrap_or_default()
}

// ── Tests ──

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn test_severity_mapping() {
        assert_eq!(
            UiError::WindowCreate("".into()).severity(),
            ErrorSeverity::Fatal
        );
        assert_eq!(
            UiError::SurfaceCreate("".into()).severity(),
            ErrorSeverity::Fatal
        );
        assert_eq!(
            UiError::GpuInit(GpuErrorKind::NoAdapter).severity(),
            ErrorSeverity::Recoverable
        );
        assert_eq!(
            UiError::GpuDeviceLost.severity(),
            ErrorSeverity::Recoverable
        );
        assert_eq!(
            UiError::GpuRender("".into()).severity(),
            ErrorSeverity::Recoverable
        );
        assert_eq!(
            UiError::CallbackPanic {
                context: "".into(),
                window_id: None,
                element_id: None,
                message: "".into(),
            }
            .severity(),
            ErrorSeverity::Recoverable
        );
        assert_eq!(
            UiError::FontLoad("".into()).severity(),
            ErrorSeverity::Warning
        );
        assert_eq!(
            UiError::GridLayout("".into()).severity(),
            ErrorSeverity::Warning
        );
    }

    #[test]
    fn test_variant_name() {
        assert_eq!(
            UiError::WindowCreate("".into()).variant_name(),
            "WindowCreate"
        );
        assert_eq!(
            UiError::GpuInit(GpuErrorKind::NoAdapter).variant_name(),
            "GpuInit"
        );
        assert_eq!(
            UiError::CallbackPanic {
                context: "".into(),
                window_id: None,
                element_id: None,
                message: "".into(),
            }
            .variant_name(),
            "CallbackPanic"
        );
        assert_eq!(UiError::FontLoad("".into()).variant_name(), "FontLoad");
    }

    #[test]
    fn test_push_error_calls_handler() {
        let called = Rc::new(Cell::new(false));
        let c = called.clone();
        set_error_handler(Rc::new(move |_: &UiError| {
            c.set(true);
        }));
        push_error(UiError::GridLayout("test".into()));
        assert!(called.get());
        ERROR_HANDLER.with(|h| *h.borrow_mut() = None);
    }

    #[test]
    fn test_error_buffer_bounded() {
        set_error_buffer_limit(3);
        for i in 0..10 {
            push_error(UiError::GridLayout(format!("err_{i}")));
        }
        ERROR_BUFFER.with(|b| {
            let buf = b.borrow();
            assert_eq!(buf.len(), 3);
            assert_eq!(buf[0].error.to_string(), "grid layout error: err_7");
            assert_eq!(buf[2].error.to_string(), "grid layout error: err_9");
        });
        ERROR_HANDLER.with(|h| *h.borrow_mut() = None);
        set_error_buffer_limit(128);
    }

    #[test]
    fn test_push_error_with_severity_override() {
        push_error_with_severity(
            UiError::GridLayout("override".into()),
            ErrorSeverity::Recoverable,
        );
        ERROR_BUFFER.with(|b| {
            let buf = b.borrow();
            assert_eq!(buf[buf.len() - 1].severity, ErrorSeverity::Recoverable);
        });
        ERROR_HANDLER.with(|h| *h.borrow_mut() = None);
    }

    #[test]
    fn test_error_counts() {
        push_error(UiError::FontLoad("a".into()));
        push_error(UiError::FontLoad("b".into()));
        push_error(UiError::GridLayout("c".into()));
        let counts = error_counts();
        let font_count = counts
            .iter()
            .find(|(k, _)| *k == "FontLoad")
            .map(|(_, v)| *v)
            .unwrap_or(0);
        let grid_count = counts
            .iter()
            .find(|(k, _)| *k == "GridLayout")
            .map(|(_, v)| *v)
            .unwrap_or(0);
        assert!(font_count >= 2);
        assert!(grid_count >= 1);
        ERROR_HANDLER.with(|h| *h.borrow_mut() = None);
    }

    #[test]
    fn test_error_record_has_timestamp() {
        push_error(UiError::FontLoad("ts".into()));
        ERROR_BUFFER.with(|b| {
            let buf = b.borrow();
            // timestamp should be recent (< 5 seconds ago)
            assert!(buf[buf.len() - 1].timestamp.elapsed().as_secs() < 5);
        });
        ERROR_HANDLER.with(|h| *h.borrow_mut() = None);
    }

    #[test]
    fn test_error_record_has_frame_id() {
        set_current_frame_id(Some(42));
        push_error(UiError::FontLoad("fid".into()));
        set_current_frame_id(None);
        ERROR_BUFFER.with(|b| {
            let buf = b.borrow();
            assert_eq!(buf[buf.len() - 1].frame_id, Some(42));
        });
        ERROR_HANDLER.with(|h| *h.borrow_mut() = None);
    }

    #[test]
    fn test_panic_to_string_string() {
        let msg = "hello".to_string();
        let boxed: Box<dyn std::any::Any + Send> = Box::new(msg);
        assert_eq!(panic_to_string(&boxed), "hello");
    }

    #[test]
    fn test_panic_to_string_str() {
        let boxed: Box<dyn std::any::Any + Send> = Box::new("world");
        assert_eq!(panic_to_string(&boxed), "world");
    }

    #[test]
    fn test_fatal_error_panics_without_handler() {
        let err = UiError::WindowCreate("fatal".into());
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            push_error(err);
        }));
        assert!(result.is_err());
        ERROR_HANDLER.with(|h| *h.borrow_mut() = None);
    }

    #[test]
    fn test_fatal_handler_suppresses_panic() {
        let handled = Rc::new(Cell::new(false));
        let h = handled.clone();
        set_fatal_handler(Rc::new(move |_: &UiError| {
            h.set(true);
        }));
        let err = UiError::WindowCreate("suppressed".into());
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            push_error(err);
        }));
        assert!(result.is_ok());
        assert!(handled.get());
        FATAL_HANDLER.with(|f| *f.borrow_mut() = None);
        ERROR_HANDLER.with(|h| *h.borrow_mut() = None);
    }

    #[test]
    fn test_panic_hook_forwards_to_push_error() {
        use std::sync::atomic::{AtomicBool, Ordering};
        let received = std::sync::Arc::new(AtomicBool::new(false));
        let r = received.clone();
        set_error_handler(Rc::new(move |_: &UiError| {
            r.store(true, Ordering::SeqCst);
        }));

        // Simulate what auralis-task's executor does on a panicked poll:
        push_error(UiError::CallbackPanic {
            context: "auralis-task:task=1,scope=0".into(),
            window_id: None,
            element_id: None,
            message: "task panicked".into(),
        });

        assert!(received.load(Ordering::SeqCst));
        ERROR_HANDLER.with(|h| *h.borrow_mut() = None);
    }
}
