use crate::core::element::{DirtyFlags, ElementArena};
use crate::core::id::ElementId;
use crate::style::{Point, Rect};
use std::cell::RefCell;

/// Per-window portal registries (audit 2026-07-18 multi-window pass).
///
/// Previously process-global `thread_local!`s: window A's outside-click
/// swept window B's dismiss handlers, and B's dropdown closed from events
/// it never saw. Now an [`AppContext::extension`] domain.
#[derive(Default)]
pub(crate) struct PortalDomain {
    /// Drain-once queue for tree registration (mount attaches to root).
    portals: RefCell<Vec<ElementId>>,
    dismiss_handlers: RefCell<Vec<(ElementId, Box<dyn Fn()>)>>,
    removals: RefCell<Vec<ElementId>>,
    /// Portals that need position updates every frame.  Never drained —
    /// distinct from `portals` which is drain-once for tree registration.
    persistent: RefCell<Vec<ElementId>>,
    /// Owner links: main-tree element → portals it owns (audit 2026-07-16,
    /// round 3, item ①). Portals mount as root children, so removing the
    /// owner's subtree does not touch them — element teardown consults this
    /// map and queues the owned portals for removal, which recursively
    /// tears them down (firing their own on_unmount cleanup).
    owners: RefCell<Vec<(ElementId, ElementId)>>,
}

fn domain() -> std::rc::Rc<PortalDomain> {
    crate::core::app_context::current_app().extension::<PortalDomain>()
}

pub fn register_portal(el_id: ElementId) {
    let dom = domain();
    dom.portals.borrow_mut().push(el_id);
    dom.persistent.borrow_mut().push(el_id);
}

/// Link `portal_id` to the lifetime of `owner_id` (an element in the main
/// tree — typically the widget's root or trigger). When the owner is torn
/// down, the portal is queued for removal automatically.
pub fn register_portal_owner(owner_id: ElementId, portal_id: ElementId) {
    domain().owners.borrow_mut().push((owner_id, portal_id));
}

/// Take (and unlink) all portals owned by `owner_id`. Called by element
/// teardown; returns an empty Vec in the common case without allocating
/// side effects.
pub fn take_portals_of(owner_id: ElementId) -> Vec<ElementId> {
    let dom = domain();
    let mut owners = dom.owners.borrow_mut();
    if owners.iter().all(|(own, _)| *own != owner_id) {
        return Vec::new();
    }
    let mut taken = Vec::new();
    owners.retain(|(own, portal)| {
        if *own == owner_id {
            taken.push(*portal);
            false
        } else {
            true
        }
    });
    taken
}

pub fn portal_ids() -> Vec<ElementId> {
    domain().portals.borrow().clone()
}

pub fn drain_portals() -> Vec<ElementId> {
    domain().portals.borrow_mut().drain(..).collect()
}

pub fn push_portal(el_id: ElementId) {
    register_portal(el_id);
}

pub fn remove_portal(el_id: ElementId) {
    let dom = domain();
    dom.removals.borrow_mut().push(el_id);
    dom.persistent.borrow_mut().retain(|&id| id != el_id);
    dom.dismiss_handlers
        .borrow_mut()
        .retain(|(id, _)| *id != el_id);
    dom.owners
        .borrow_mut()
        .retain(|(_, portal)| *portal != el_id);
}

pub fn drain_portal_removals() -> Vec<ElementId> {
    domain().removals.borrow_mut().drain(..).collect()
}

/// Returns all portal elements that need per-frame position updates.
/// Unlike [`portal_ids`], this list is never drained.
pub fn portal_position_ids() -> Vec<ElementId> {
    domain().persistent.borrow().clone()
}

/// Portal overlay height override.  Inserted as user_data on an absolute
/// (z_index>0) element to give it a fixed pixel height and enable
/// `auto` right/bottom insets — preventing taffy from stretching the
/// element to the parent's full height.  Distinct from the generic
/// `Rc<Cell<f32>>` user_data used for dynamic width.
pub struct PortalHeight(pub std::rc::Rc<std::cell::Cell<f32>>);

/// Portal overlay width override.  Inserted as user_data on an absolute
/// (z_index>0) element to give it a fixed pixel width independent of the
/// anchor element.  When present, `update_portal_positions` uses this value
/// instead of the anchor content width.  Useful for popups that should
/// have their own intrinsic width (e.g. DatePicker, ColorPicker) rather
/// than matching the trigger width (Select, ComboBox).
pub struct PortalWidth(pub std::rc::Rc<std::cell::Cell<f32>>);

/// Positioning strategy for anchor-tracked portals (Select, ComboBox,
/// DatePicker). Inserted as user_data on the portal element; read by
/// `update_portal_positions` to compute the (x, y, w) written into the
/// `portal_pos` cell.
#[derive(Clone, Copy, Debug)]
pub struct PortalAnchorStrategy {
    /// Vertical gap between anchor and portal when placed below.
    pub gap: f32,
    /// When true, flip above the anchor if placing below overflows the
    /// viewport and there is room above. On flip the gap collapses to 0.
    pub auto_flip: bool,
    /// When true, clamp the portal height to remaining viewport space
    /// (writes back into the PortalHeight cell). For scrollable dropdowns.
    pub clamp_to_viewport: bool,
}

impl Default for PortalAnchorStrategy {
    fn default() -> Self {
        Self {
            gap: 4.0,
            auto_flip: true,
            clamp_to_viewport: true,
        }
    }
}

/// Register a persistent dismiss callback bound to a portal element.
///
/// The callback fires on a [`fire_dismiss`] tick when the click lies
/// **outside** both the portal's screen bounds AND the portal's anchor
/// element (the `ElementId` stored as user_data — the trigger that toggles
/// the portal open). Clicks inside the portal (option buttons) or on the
/// anchor (its own handler owns the toggle) never dismiss.
///
/// Handlers are **persistent**: they are NOT removed after firing, so a
/// dropdown can be reopened any number of times. The callback must guard
/// itself (e.g. `if open.read() { open.set(false) }`). Handlers are dropped
/// when their portal is removed via [`remove_portal`].
///
/// `skip` is an opt-out counter for special cases (e.g. context menus that
/// manage their own dismissal use a large value). For normal anchored
/// dropdowns use `skip = 0`; the anchor-awareness handles the opening click.
/// Register a persistent dismiss callback bound to a portal element.
///
/// The callback fires on a [`fire_dismiss`] tick when the click lies
/// **outside** both the portal's screen bounds AND the portal's anchor
/// element (the `ElementId` stored as user_data — the trigger that toggles
/// the portal open). Clicks inside the portal (option buttons) or on the
/// anchor (its own handler owns the toggle) never dismiss.
///
/// Handlers are **persistent**: they are NOT removed after firing, so a
/// dropdown can be reopened any number of times. The callback must guard
/// itself (e.g. `if open.read() { open.set(false) }`). Handlers are dropped
/// when their portal is removed via [`remove_portal`].
pub fn register_dismiss(portal_id: ElementId, on_dismiss: impl Fn() + 'static) {
    domain()
        .dismiss_handlers
        .borrow_mut()
        .push((portal_id, Box::new(on_dismiss)));
}

/// Tick all pending dismiss handlers.
///
/// Called by the window after dispatching every PointerDown / Click event.
/// `arena` is the element arena; `click_pos` is the current click
/// position in window coordinates.
///
/// A handler fires only when the click resolves (via the scroll-aware
/// hit-test) to an element that is NEITHER inside the portal subtree NOR
/// inside the portal's anchor subtree.
/// Using the hit-test (rather than raw `screen_bounds.contains`) makes this
/// correct under scrolling: `screen_bounds` are layout-space while `click_pos`
/// is window-space, so a direct contains-check would mis-fire on scrolled
/// content. Handlers are persistent (not removed on fire) so portals reopen.
pub fn fire_dismiss(arena: &ElementArena, click_pos: Point) {
    // Resolve the actually-clicked element with the scroll-aware hit-test.
    let hit = crate::core::dirty_registry::hit_test_with_fallback(arena, click_pos);

    let dom = domain();
    // Decide inside/outside for each handler.
    let decisions: Vec<(bool, usize)> = {
        let handlers = dom.dismiss_handlers.borrow();
        handlers
            .iter()
            .enumerate()
            .map(|(i, (portal_id, _))| {
                let anchor_id = arena
                    .get(*portal_id)
                    .and_then(|el| el.get_user_data::<ElementId>().copied());
                let inside = match hit {
                    Some(h) => {
                        h == *portal_id
                            || crate::core::dirty_registry::is_descendant_of(h, *portal_id)
                            || anchor_id.is_some_and(|aid| {
                                h == aid || crate::core::dirty_registry::is_descendant_of(h, aid)
                            })
                    }
                    None => false,
                };
                (inside, i)
            })
            .collect()
    };

    // Fire pass: outside clicks.
    let handlers = dom.dismiss_handlers.borrow();
    for (inside, i) in &decisions {
        if *inside {
            continue;
        }
        let (_, handler) = &handlers[*i];
        handler();
    }
}

// ── Per-frame portal positioning (moved from platform/window.rs — audit
// round 3, ② phase 1: portal geometry belongs to the portal module) ──

pub(crate) fn update_portal_positions(arena: &ElementArena, root_id: ElementId) {
    let viewport_h = arena.get(root_id).map_or(768.0, |r| r.bounds().height);
    let portals = portal_position_ids();
    for &portal_id in &portals {
        if let Some(child) = arena.get(portal_id) {
            // Skip hidden portals — their anchors still move with scroll,
            // but the portal itself is not rendered, so recalculating its
            // position and registering MEASURE dirty is pure waste (and
            // forces a full taffy pass every scroll frame). The position
            // will be recalculated when the portal becomes visible again.
            if !child.is_visible() {
                continue;
            }
            if let Some(anchor_id) = child.get_user_data::<crate::core::ElementId>().copied() {
                // 3-element variant (Select, ComboBox, DatePicker, old dropdowns)
                if let Some(pos) =
                    child.get_user_data::<std::rc::Rc<std::cell::Cell<(f32, f32, f32)>>>()
                {
                    let sb = arena.get(anchor_id).map(|a| a.screen_bounds);
                    let padding = arena.comp_layout(anchor_id).map(|l| l.padding);
                    if let (Some(sb), Some(padding)) = (sb, padding) {
                        let strat = child
                            .get_user_data::<PortalAnchorStrategy>()
                            .copied()
                            .unwrap_or(PortalAnchorStrategy {
                                gap: 4.0,
                                auto_flip: true,
                                clamp_to_viewport: false,
                            });
                        // Width: PortalWidth (intrinsic) wins; else match the
                        // anchor's full width (System-B `anchor_w` semantics).
                        let intrinsic_w = child
                            .get_user_data::<PortalWidth>()
                            .map(|pw| pw.0.get())
                            .filter(|w| *w > 0.0);
                        let _ = padding;
                        let pw = intrinsic_w.unwrap_or_else(|| sb.width.max(1.0));
                        let (scroll_x, scroll_y) = arena.accumulated_scroll(anchor_id);
                        let anchor_screen_y = sb.y - scroll_y;
                        let anchor_screen_x = sb.x - scroll_x;
                        // Prefer the intended PortalHeight (set synchronously on
                        // open) over child.bounds().height, which lags taffy by a
                        // frame on first open — otherwise the flip-above decision
                        // uses height 0 and the portal is placed below, overflowing
                        // the viewport until a later re-layout corrects it.
                        let portal_h = child
                            .get_user_data::<PortalHeight>()
                            .map(|ph| ph.0.get())
                            .filter(|h| *h > 0.0)
                            .unwrap_or_else(|| child.bounds().height);
                        let below_y = anchor_screen_y + sb.height + strat.gap;
                        let window_y = if strat.auto_flip
                            && below_y + portal_h > viewport_h
                            && anchor_screen_y - portal_h > 0.0
                        {
                            anchor_screen_y - portal_h // flip above, gap collapses to 0
                        } else {
                            below_y
                        };
                        if strat.clamp_to_viewport {
                            if let Some(ph_cell) = child.get_user_data::<PortalHeight>() {
                                let max_h = (viewport_h - window_y).max(0.0);
                                let cur = ph_cell.0.get();
                                if cur > max_h && max_h > 0.0 {
                                    ph_cell.0.set(max_h);
                                }
                            }
                        }
                        let new_pos = (anchor_screen_x, window_y, pw);
                        if pos.get() != new_pos {
                            pos.set(new_pos);
                            crate::core::dirty_registry::bump_subtree_gen(portal_id);
                            crate::core::dirty_registry::register_dirty(
                                portal_id,
                                DirtyFlags::MEASURE,
                            );
                        }
                    }
                }
                // Popover (PopoverGeometry variant)
                if let Some(geo_cell) = child.get_user_data::<std::rc::Rc<std::cell::Cell<crate::widgets::overlay::PopoverGeometry>>>() {
                    if let Some(placement) = child.get_user_data::<crate::widgets::overlay::PopoverPlacement>().copied() {
                        let sb = arena.get(anchor_id).map(|a| a.screen_bounds);
                        if let Some(sb) = sb {
                            let (scroll_x, scroll_y) = arena.accumulated_scroll(anchor_id);
                            let anchor = crate::style::Rect::new(
                                sb.x - scroll_x,
                                sb.y - scroll_y,
                                sb.width,
                                sb.height,
                            );
                            let viewport_rect = crate::style::Rect::new(0.0, 0.0, arena.get(root_id).map_or(1024.0, |r| r.bounds().width), viewport_h);
                            let content_w = placement.min_width
                                .unwrap_or_else(|| (sb.width).max(1.0));
                            // Use PortalHeight for flip decision when available —
                            // child.bounds() may be 0 on first frame, causing the
                            // engine to pick Bottom when the real content overflows.
                            let content_h = child
                                .get_user_data::<PortalHeight>()
                                .map(|ph| ph.0.get())
                                .filter(|h| *h > 0.0)
                                .unwrap_or_else(|| child.bounds().height.max(1.0));
                            let new_geo = crate::widgets::overlay::compute_popover_geometry(
                                anchor, viewport_rect, content_w, content_h, placement,
                            );

                            // Clamp PortalHeight to viewport (for scrollable dropdowns)
                            if let Some(ph_cell) = child
                                .get_user_data::<PortalHeight>()
                            {
                                let max_h = (viewport_h - new_geo.y - placement.viewport_margin).max(0.0);
                                let cur = ph_cell.0.get();
                                if cur > max_h && max_h > 0.0 {
                                    ph_cell.0.set(max_h);
                                }
                            }

                            let old = geo_cell.get();
                            if old != new_geo {
                                geo_cell.set(new_geo);
                                crate::core::dirty_registry::bump_subtree_gen(portal_id);
                                crate::core::dirty_registry::register_dirty(portal_id, DirtyFlags::MEASURE);
                            }
                        }
                    }
                }
            }
        }
    }
}

pub(crate) fn stretch_visible_anchored_portals(arena: &mut ElementArena) {
    for portal_id in portal_position_ids() {
        let (has_strategy, has_intrinsic_w, visible, dims) = arena
            .get(portal_id)
            .map(|el| {
                (
                    el.get_user_data::<PortalAnchorStrategy>().is_some(),
                    el.get_user_data::<PortalWidth>().is_some(),
                    el.is_visible(),
                    (el.screen_bounds.width, el.screen_bounds.height),
                )
            })
            .unwrap_or((false, false, false, (0.0, 0.0)));
        // Only stretch anchor-width dropdowns (Select/ComboBox). Portals with
        // an intrinsic PortalWidth (DatePicker) have their own fixed internal
        // layout (calendar grid, fixed day cells) and must not be stretched —
        // doing so would blow up e.g. a selected day cell to the full width.
        if has_strategy && visible && !has_intrinsic_w {
            stretch_children_to_width(arena, portal_id, dims.0, dims.1);
        }
    }
}

/// Stretch overlay children to the portal's new width.  Called after
/// `layout_all` updates the portal bounds, before `paint_all`, to fix
/// child sizes that were computed by the main taffy pass with old bounds.
fn stretch_children_to_width(
    arena: &mut ElementArena,
    parent_id: ElementId,
    parent_w: f32,
    _parent_h: f32,
) {
    let children: Vec<ElementId> = arena
        .get(parent_id)
        .map(|el| el.children.clone())
        .unwrap_or_default();
    for cid in children {
        // Read element data, then drop the immutable ref before mutable access
        let (old_w, fg, fs, pad_left, pad_right, x, y, h) = {
            if let Some(el) = arena.get(cid) {
                let p = el.padding();
                (
                    el.screen_bounds.width,
                    el.flex_grow(),
                    el.flex_shrink(),
                    p.left,
                    p.right,
                    el.screen_bounds.x,
                    el.screen_bounds.y,
                    el.screen_bounds.height,
                )
            } else {
                continue;
            }
        };
        let avail_w = (parent_w - pad_left - pad_right).max(0.0);

        if (fg > 0.0 || fs > 0.0) && (old_w - avail_w).abs() > 0.5 {
            let new_b = Rect::new(x, y, avail_w, h);
            if let Some(el_mut) = arena.get_mut(cid) {
                el_mut.set_bounds(new_b);
                el_mut.screen_bounds = new_b;
            }
            crate::core::dirty_registry::update_bounds(cid, new_b);
            crate::core::dirty_registry::mark_dirty(cid, crate::core::element::DirtyFlags::REPAINT);
            crate::core::dirty_registry::register_dirty(
                cid,
                crate::core::element::DirtyFlags::REPAINT,
            );
        }
        stretch_children_to_width(arena, cid, avail_w, 0.0);
    }
}
