//! Shared patterns extracted from List / Table for reuse across widgets.
//!
//! These primitives reflect the common "slot pool + lazy text + selection +
//! scroll + keyboard nav" architecture shared by List, Table, Tree, and
//! similar single-axis scrolling item widgets.

pub mod dropdown;
pub mod keyboard;
pub mod portal;
pub mod reorder;
pub mod selection;
pub mod slot_pool;
pub mod text_cell;

pub use dropdown::{
    register_dropdown_portal, register_dropdown_unmount, register_overlay_lifecycle,
    register_unmount_pop_modal, scroll_to_selected_on_open, subscribe_dropdown_reopen,
};
pub use keyboard::{row_nav, RowNavOutcome};
pub use portal::{mount_portal_popup, PortalPopupConfig};
pub use selection::{
    is_item_disabled, set_item_disabled, set_item_highlight, sync_list_selection_focus, SelectionBg,
};
pub use slot_pool::SlotPool;
pub use text_cell::TextCellState;
