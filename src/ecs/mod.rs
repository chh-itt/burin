//! Thread-local tracking sets for incremental O(k) operations.
//!
//! Each set avoids full-tree walks: theme reapply, drag layout, scroll
//! targeting, and scrollable-element queries all iterate only the elements
//! that have registered for the relevant callback.

mod registry;

pub mod active;
pub mod components;
pub mod tables;

pub use registry::{
    drain_a11y_changed, drain_drag_elements, drain_mount_callbacks, drain_theme_elements,
    has_a11y_changed, mark_a11y_changed, pending_scroll_elements, register_drag_element,
    register_on_mount, register_pending_scroll, register_scrollable, register_theme_element,
    scrollable_elements, unregister_element, unregister_pending_scroll, unregister_scrollable,
};
