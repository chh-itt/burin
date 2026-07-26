//! Shared dropdown overlay helpers extracted from Select and ComboBox.
//!
//! These eliminate ~60 lines of duplicated portal-lifecycle boilerplate
//! across the two dropdown widgets.

use std::cell::Cell;
use std::rc::Rc;

use auralis_signal::Signal;

use crate::core::element::DirtyFlags;
use crate::core::id::ElementId;
use crate::event::TraversalEdgeBehavior;
use crate::style::Rect;
use crate::widgets::bundle::ScrollBundle;
use crate::widgets::shared::SelectionBg;

/// Register a dropdown overlay as a portal with a dismiss handler
/// that closes `open` when the user clicks outside.
pub fn register_dropdown_portal(owner_id: ElementId, dropdown_id: ElementId, open: Signal<bool>) {
    crate::platform::portal::register_portal(dropdown_id);
    crate::platform::portal::register_portal_owner(owner_id, dropdown_id);
    crate::platform::portal::register_dismiss(dropdown_id, {
        move || {
            if open.read() {
                open.set(false);
            }
        }
    });
}

/// Register an on-unmount callback that removes the portal and pops
/// any remaining modal scope (belt-and-suspenders cleanup).
pub fn register_dropdown_unmount(dropdown_id: ElementId) {
    let did = dropdown_id;
    let on_unmount = Rc::new(std::cell::RefCell::new(Some(Box::new(move || {
        crate::platform::portal::remove_portal(did);
        crate::event::remove_modal_scopes_of(did);
    }) as Box<dyn FnOnce()>)));
    crate::core::element::with_ct_mut(|ct| {
        ct.lc.entry(did).or_default().on_unmount = Some(on_unmount);
    });
}

/// Subscribe to an `open` signal and handle the common overlay lifecycle:
/// reactive_visibility toggle, portal_height update, modal scope push/pop,
/// OverlayStack registration, and dirty marking.  Optional `on_open` /
/// `on_close` callbacks carry widget-specific extra logic (suppress_nav,
/// autofocus, text reset…).
///
/// ## Dismiss contract (audit 2026-07-18, AnchoredPopup pass)
///
/// While open, the popup holds an [`OverlayEntry`](crate::event::overlay)
/// on the overlay stack (`layer: Popover`):
///
/// - **Escape** is handled by the propagation layer's stack-top pop —
///   strictly LIFO. A Select inside a Modal closes on the first Escape;
///   the Modal closes on the second.
/// - **Outside clicks** stay owned by the portal dismiss system
///   (`fire_dismiss` is anchor-aware; the OverlayStack check is not), so
///   the entry sets `dismiss_on_click_outside: false`. Because the
///   click-outside check only consults the stack TOP, an open popup also
///   shields the Modal underneath it — the first click closes the popup,
///   the second closes the Modal (standard two-step dismiss).
pub fn register_overlay_lifecycle(
    open: Signal<bool>,
    scope_id: ElementId,
    rv: Rc<Cell<bool>>,
    portal_h: Rc<Cell<f32>>,
    visible_height: f32,
    on_open: Option<Rc<dyn Fn()>>,
    on_close: Option<Rc<dyn Fn()>>,
) {
    let open_sub = open.clone();
    let open_entry = open.clone();
    crate::core::signal_bridge::subscribe_owned(scope_id, &open, move || {
        let is_open = open_sub.read();
        let was = rv.get();
        if was != is_open {
            rv.set(is_open);
            portal_h.set(if is_open { visible_height } else { 0.0 });
            if is_open {
                crate::event::push_modal_scope(scope_id, TraversalEdgeBehavior::Wrap);
                push_popup_overlay_entry(scope_id, open_entry.clone());
                if let Some(ref f) = on_open {
                    f();
                }
                crate::core::dirty_registry::register_dirty(scope_id, DirtyFlags::MEASURE);
                crate::core::dirty_registry::register_dirty(scope_id, DirtyFlags::REPAINT);
            } else {
                crate::event::pop_modal_scope();
                crate::event::overlay::remove(scope_id);
                if let Some(ref f) = on_close {
                    f();
                }
                crate::core::dirty_registry::register_dirty(scope_id, DirtyFlags::MEASURE);
            }
            crate::core::dirty_registry::bump_subtree_gen(scope_id);
        }
    });
    if open.read() {
        crate::event::push_modal_scope(scope_id, TraversalEdgeBehavior::Wrap);
        push_popup_overlay_entry(scope_id, open);
    }
}

/// Register the popup on the OverlayStack so Escape participates in the
/// stack-top LIFO contract. `on_dismiss` closes the driving signal (guarded
/// — the orderly close path re-enters `overlay::remove`, which is a no-op
/// once the entry is gone).
pub(crate) fn push_popup_overlay_entry(scope_id: ElementId, open: Signal<bool>) {
    crate::event::overlay::push(crate::event::overlay::OverlayEntry {
        element_id: scope_id,
        layer: crate::event::overlay::OverlayLayer::Popover,
        barrier_color: None,
        dismiss_on_click_outside: false,
        dismiss_on_escape: true,
        trap_focus: false,
        autofocus_first: false,
        previous_focus: None,
        on_dismiss: Some(Box::new(move || {
            if open.read() {
                open.set(false);
            }
        })),
    });
}

/// Register an on-unmount callback that pops the modal scope.
/// Use for overlays that push_modal_scope but **do not** own a portal
/// that needs removal (that is what `register_dropdown_unmount` is for).
pub fn register_unmount_pop_modal(scope_id: ElementId) {
    let on_unmount = Rc::new(std::cell::RefCell::new(Some(Box::new(move || {
        crate::event::remove_modal_scopes_of(scope_id);
    }) as Box<dyn FnOnce()>)));
    crate::core::element::with_ct_mut(|ct| {
        ct.lc.entry(scope_id).or_default().on_unmount = Some(on_unmount);
    });
}

/// Scroll the dropdown to keep the selected item visible when the
/// dropdown opens.  Duplicated ~12 lines in Select + ComboBox.
pub(crate) fn scroll_to_selected_on_open(
    selected_idx: Rc<Cell<Option<usize>>>,
    item_height: f32,
    visible_count: usize,
    num_items: usize,
    scroll: &ScrollBundle,
) {
    let idx = selected_idx.get().unwrap_or(0);
    let target_y = idx as f32 * item_height;
    let mut o = scroll.scroll_offset.get();
    let vph = (visible_count as f32 * item_height).max(1.0);
    if target_y + item_height > vph {
        o.y = (target_y + item_height - vph).max(0.0);
    }
    o.y = o.y.max(0.0);
    scroll
        .content_bounds
        .set(Rect::new(0.0, 0.0, 0.0, num_items as f32 * item_height));
    scroll.scroll_offset.set(o);
    crate::core::dirty_registry::spatial_update_scroll(scroll.container_id, o.x, o.y);
    crate::core::dirty_registry::bump_subtree_gen(scroll.container_id);
    scroll.generation.set(scroll.generation.get() + 1);
}

/// Re-sync the selection background + mark items dirty when the
/// dropdown reopens — needed because repaints may have been cleared
/// while the portal was invisible.
pub fn subscribe_dropdown_reopen(
    owner_eid: ElementId,
    open: Signal<bool>,
    selected_idx: Rc<Cell<Option<usize>>>,
    sel_bg: Rc<SelectionBg>,
    option_ids: Vec<ElementId>,
) {
    crate::core::signal_bridge::subscribe_owned(owner_eid, &open.clone(), move || {
        if open.read() {
            if let Some(idx) = selected_idx.get() {
                sel_bg.set_selected(idx);
            }
            for &oid in &option_ids {
                crate::core::dirty_registry::register_dirty(oid, DirtyFlags::MEASURE);
                crate::core::dirty_registry::register_dirty(oid, DirtyFlags::REPAINT);
                crate::core::dirty_registry::bump_subtree_gen(oid);
                crate::core::dirty_registry::bump_surface_gen_remote(oid);
            }
        }
    });
}
