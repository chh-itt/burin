use std::path::PathBuf;

/// Configuration for the structured logging subscriber.
pub struct LoggingConfig {
    /// Log level filter (e.g. "info", "debug", "warn").
    /// Falls back to `RUST_LOG` env var, then `"info"`.
    pub level: Option<String>,
    /// Optional path for file logging.
    /// Requires `file-logging` feature; otherwise ignored.
    pub file: Option<PathBuf>,
    /// Whether to use non-blocking writer (default true).
    pub non_blocking: bool,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: None,
            file: None,
            non_blocking: true,
        }
    }
}

/// Handle returned by [`init()`].
/// Keeps the tracing subscriber and any non-blocking writer alive.
pub struct LoggingGuard;

/// Initialize the tracing subscriber based on the provided config.
///
/// - Reads `config.level`, then `RUST_LOG` env, defaults to `"info"`.
/// - When `tracing` feature is off, this is a no-op.
#[cfg(any(feature = "devtools", feature = "file-logging"))]
pub fn init(config: LoggingConfig) -> LoggingGuard {
    use tracing_subscriber::prelude::*;
    use tracing_subscriber::{fmt, EnvFilter, Registry};

    let level = config
        .level
        .or_else(|| std::env::var("RUST_LOG").ok())
        .unwrap_or_else(|| "info".into());

    let filter = EnvFilter::try_new(&level).unwrap_or_else(|_| EnvFilter::new("info"));
    let fmt_layer = fmt::Layer::default();

    #[cfg(feature = "file-logging")]
    if let Some(path) = &config.file {
        let file_appender = tracing_appender::rolling::daily(path, "burin.log");
        let (non_blocking, _file_guard) = tracing_appender::non_blocking(file_appender);
        // Intentionally leak: keep worker thread alive for app lifetime.
        // The OS cleans up threads on process exit.
        std::mem::forget(_file_guard);
        let subscriber = Registry::default()
            .with(filter)
            .with(fmt_layer)
            .with(fmt::Layer::default().with_writer(non_blocking));
        tracing::subscriber::set_global_default(subscriber)
            .expect("tracing subscriber already set");
        return LoggingGuard;
    }

    let subscriber = Registry::default().with(filter).with(fmt_layer);
    tracing::subscriber::set_global_default(subscriber).expect("tracing subscriber already set");
    LoggingGuard
}

/// No-op fallback when `tracing` feature is disabled.
#[cfg(not(any(feature = "devtools", feature = "file-logging")))]
pub fn init(_config: LoggingConfig) -> LoggingGuard {
    LoggingGuard
}
