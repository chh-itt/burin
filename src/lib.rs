//! # burin
//!
//! A retained-mode Rust GUI framework built on the
//! [auralis](https://github.com/chh-itt/auralis) reactive signal kernel.
//!
//! ## Quick start
//!
//! ```no_run
//! use burin::core::{Compositor, Widget};
//! use burin::widgets::display::Text;
//! use auralis_signal::Signal;
//!
//! fn hello() -> impl Widget {
//!     Compositor::new(|_scope| {
//!         let _greeting = Signal::new("Hello, world!".to_string());
//!         Text::new("Hello, world!")
//!     })
//! }
//! ```
//!
//! ## Architecture at a glance
//!
//! ```text
//! Signal change → dirty flag (O(1) register)
//!   → process_dirty_set (O(k) ancestor walk)
//!     → 4-path incremental taffy layout
//!     → scene/subtree cache check
//!       → paint only dirty subtrees
//!         → GPU command submission
//! ```
//!
//! ## Modules
//!
//! | Module | Content |
//! |--------|---------|
//! | [`core`] | Widget trait, Element (12 on-demand sub-structs), ElementArena, Compositor, Prop, Context types, signal bridge |
//! | [`style`] | Color, Rect, Size, Dimension, Point, Padding, Margin, Styled trait, geometry |
//! | [`widgets`] | Built-in widget library (~50 widgets: layout, display, input, overlay, composite, decoration) |
//! | [`event`] | Event system (hit testing, gesture, focus, keyboard, action bindings, propagation) |
//! | [`layout`] | Layout system (incremental taffy bridge, dirty propagation, grid layout) |
//! | [`render`] | Renderer abstraction, Painter API, wgpu GPU backend, tiny-skia CPU backend, text shaping |
//! | [`platform`] | Window (winit event loop), accessibility (accesskit), portal system, clipboard, drag-drop, IME |
//! | [`theme`] | Theme system, Material 3 design tokens, light/dark auto-detection |
//! | [`animation`] | Animation driver, easing curves, property transitions, enter/exit animations |
//! | [`ecs`] | Thread-local tracking sets for O(k) operations (theme/drag/scroll/a11y) |
//! | [`testing`] | TestHarness — headless full-frame simulation without window/GPU |
//! | [`debug`] | Debug utilities |
//! | `i18n` | Internationalization (Fluent) |
//! | [`resource`] | Embedded assets (fonts, icons) |
//!
//! ## Features
//!
//! - **`backend-wgpu`** (default) — GPU rendering via wgpu + glyphon
//! - **`backend-tiny-skia`** — CPU rasterization via tiny-skia + softbuffer + swash
//! - **`system-theme`** (default) — Auto-detect system light/dark preference
//! - **`clipboard`** (default) — Clipboard access via arboard
//! - **`i18n`** — Internationalization via Fluent
//! - **`devtools`** — Devtools via auralis-devtools + profiling
//! - **`hot-reload`** — Hot reload via notify
//! - **`rayon`** — Parallel processing
//! - **`ext-image`** — Image loading (png, jpeg)
//! - **`ext-jiff`** — Date/time support
//! - See `Cargo.toml` for the full list.
//!
//! ## Design principles
//!
//! 1. **Signal is state.** No extra state management abstraction. Reading a `Signal`
//!    subscribes; writing notifies. There is no virtual DOM, no diff, no reconciliation.
//! 2. **Composition over macros.** No template language, DSL, or proc macros. UI is pure
//!    Rust function calls. IDE completions, refactoring, and go-to-definition work
//!    without configuration.
//! 3. **O(k) rendering.** Only elements that changed are processed. Dirty flags propagate
//!    only up the ancestor chain, short-circuited at containment boundaries. Paint
//!    re-records only dirty subtrees; clean subtrees replay from cache.
//! 4. **`forbid(unsafe_code)`** everywhere except platform boundaries.
//! 5. **Retained mode.** Each widget mounts into a persistent `Element`. Signal changes
//!    update in-place; the tree is never rebuilt.
//! 6. **Accessibility is mandatory.** AccessKit integration built-in, not bolted on.

#![forbid(unsafe_code)]

/// Debug-only version-generation trace macro.
/// In debug builds, emits `eprintln!` only when `AURALIS_VGEN=1` is set in
/// the environment (checked once, cached). Always a no-op in release builds.
///
/// Rationale (audit 2026-07-16, F2): unconditional `eprintln!` in the signal
/// and paint hot paths polluted benchmark numbers (`bench` profile inherits
/// `dev`, so `debug_assertions` is on) and spammed stderr in dev runs.
#[cfg(debug_assertions)]
#[doc(hidden)]
pub fn vgen_enabled() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var_os("AURALIS_VGEN").is_some_and(|v| v == "1"))
}

#[cfg(debug_assertions)]
#[macro_export]
macro_rules! vgen {
    ($($arg:tt)*) => {
        if $crate::vgen_enabled() {
            eprintln!($($arg)*);
        }
    };
}

#[cfg(not(debug_assertions))]
#[macro_export]
macro_rules! vgen {
    ($($arg:tt)*) => {};
}

pub mod animation;
pub mod asset;
#[cfg(feature = "ext-audio")]
pub mod audio;
pub mod core;
pub mod debug;
pub mod ecs;
pub mod event;
#[cfg(feature = "i18n")]
pub mod i18n;
pub mod physics;

#[cfg(not(feature = "i18n"))]
#[macro_export]
macro_rules! t {
    ($ctx:expr, $msg_id:literal $(, $key:ident = $val:expr)*) => {{
        let _ = ($($key = &$val),*);
        let _ = $ctx;
        $crate::auralis_signal::Signal::new($msg_id.to_string())
    }};
}
pub mod layout;
pub mod logging;
pub mod platform;
pub mod render;
pub mod resource;
pub mod style;
pub mod task;
pub mod testing;
pub mod theme;
pub mod widgets;

/// Golden snapshot assertion. Renders the harness's most recent frame and
/// compares it against `<CARGO_MANIFEST_DIR>/tests/snapshots/<name>.png`.
/// Bless with `AURALIS_UPDATE_SNAPSHOTS=1`.
///
/// ```ignore
/// h.run_frame();
/// assert_snapshot!(h, "button_primary");
/// assert_snapshot!(h, "button_primary", SnapshotOptions::default().ignore_antialiasing(true));
/// ```
#[cfg(feature = "backend-tiny-skia")]
#[macro_export]
macro_rules! assert_snapshot {
    ($h:expr, $name:expr $(,)?) => {
        $h.assert_snapshot_at(env!("CARGO_MANIFEST_DIR"), $name)
    };
    ($h:expr, $name:expr, $opts:expr $(,)?) => {
        $h.assert_snapshot_at_with(env!("CARGO_MANIFEST_DIR"), $name, &$opts)
    };
}

pub use core::error::UiError;

/// Re-exports for convenient `use burin::prelude::*;`
pub mod prelude {
    pub use crate::animation::{Animation, EasingCurve};
    #[cfg(feature = "ext-audio")]
    pub use crate::audio::{play_sound, play_sound_bytes, AudioError, AudioPlayer};
    pub use crate::core::{
        Compositor, DirtyFlags, Element, ElementArena, ElementId, Prop, StaticWidget, Widget,
    };
    pub use crate::event::{Key, KeyHeldInfo};
    pub use crate::platform::clipboard::{Clipboard, ClipboardError};
    pub use crate::platform::window::{WindowButtons, WindowConfig, WindowHandle, WindowIcon};
    pub use crate::platform::CursorIcon;
    pub use crate::style::{
        auto, pct, px, Color, CornerRadii, Dimension, Margin, Padding, Point, Rect, Size, Styled,
        Vec2,
    };
    #[cfg(feature = "ext-audio")]
    pub use crate::widgets::composite::AudioPlayerWidget;
    pub use crate::widgets::display::Text;
    pub use crate::widgets::input::{Checkbox, NumberInput, TextInput, TextInputType};
    pub use crate::widgets::layout::*;
}

// ── Cross-thread convenience re-exports ──
pub mod ui {
    pub use crate::platform::wake::{run_on_ui, wake_ui};
    #[cfg(feature = "async-tokio")]
    pub use crate::task::spawn_background;
}
