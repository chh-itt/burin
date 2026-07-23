//! Platform integration: winit windowing, clipboard, accessibility.

mod a11y_adapter;
#[doc(hidden)]
pub mod a11y_bridge;
mod accessibility;
pub mod clipboard;
mod cursor;
pub mod display;
mod drag_drop;
mod file_dialog;
#[cfg(feature = "global-hotkey")]
pub mod global_hotkey;
mod ime;
pub mod insets;
pub mod portal;
#[cfg(feature = "tray")]
pub mod tray;
pub mod wake;
pub(crate) mod window;

pub use a11y_adapter::A11yAdapter;
pub use accessibility::build_accessibility_tree;
#[doc(hidden)]
pub use accessibility::debug_node_cache_len;
pub use clipboard::{Clipboard, ClipboardError};

#[cfg(all(feature = "clipboard", feature = "ext-image"))]
pub use clipboard::ClipboardImage;
pub use cursor::CursorIcon;
pub use drag_drop::DropData;
pub use file_dialog::{
    pick_file, pick_files, pick_folder, save_file, FileDialogBuilder, SelectedFile,
};
pub use ime::{compose_ime_surface_rect, ImeState};
pub use window::{
    create_window, App, AppBuilder, WindowButtons, WindowConfig, WindowHandle, WindowIcon,
};
