/// Spawn a future on the global executor and log the spawn.
/// Panics inside the future are automatically caught by the executor
/// and reported to the global error handler via the panic hook.
///
/// Not available on wasm32 — use a platform-specific spawn mechanism instead.
#[cfg(not(target_arch = "wasm32"))]
pub fn spawn_logged<F>(future: F)
where
    F: std::future::Future<Output = ()> + 'static,
{
    #[cfg(any(feature = "devtools", feature = "file-logging"))]
    tracing::debug!(target: "burin", "spawned task");
    auralis_task::spawn_global(future);
}

/// Spawn a future on the global executor with a given priority.
///
/// Not available on wasm32 — use a platform-specific spawn mechanism instead.
#[cfg(not(target_arch = "wasm32"))]
pub fn spawn_logged_with_priority<F>(priority: auralis_task::Priority, future: F)
where
    F: std::future::Future<Output = ()> + 'static,
{
    #[cfg(any(feature = "devtools", feature = "file-logging"))]
    tracing::debug!(target: "burin", "spawned task (priority={priority:?})");
    auralis_task::spawn_global_with_priority(priority, future);
}

// ── Layer 2: tokio background executor (async-tokio feature) ────────
#[cfg(feature = "async-tokio")]
static TOKIO_RT: std::sync::OnceLock<tokio::runtime::Runtime> = std::sync::OnceLock::new();

#[cfg(feature = "async-tokio")]
pub(crate) fn init_tokio() {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(4)
        .enable_all()
        .build()
        .expect("failed to create tokio runtime");
    let _ = TOKIO_RT.set(rt);
}

/// Spawn a `Send + 'static` future on the tokio background runtime.
///
/// The future runs on a tokio worker thread and may use any tokio
/// ecosystem crate (reqwest, sqlx, tonic, …).  To update UI state from
/// inside the future, use [`crate::platform::wake::run_on_ui`]:
///
/// ```ignore
/// crate::task::spawn_background(async move {
///     let data = reqwest::get(url).await.unwrap().text().await.unwrap();
///     crate::platform::wake::run_on_ui(move || {
///         my_signal.set(Some(data));
///     });
/// });
/// ```
///
/// # Panics
/// Panics if called before [`App::run`] or if the `async-tokio` feature
/// is disabled.
#[cfg(feature = "async-tokio")]
pub fn spawn_background<F>(future: F)
where
    F: std::future::Future<Output = ()> + Send + 'static,
{
    TOKIO_RT
        .get()
        .expect("tokio runtime not initialised — call App::run() first")
        .spawn(future);
}
