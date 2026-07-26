//! One-call portal popup setup — eliminates ~40 lines of duplicated
//! portal lifecycle boilerplate across Select, ComboBox, DatePicker,
//! ColorPicker, Popover, and all future signal-driven anchored popups.
//!
//! ## Usage
//! ```ignore
//! let portal_id = mount_portal_popup(
//!     &mut ctx.child_with_events(parent_id),
//!     PortalPopupConfig {
//!         open: open.clone(),
//!         anchor_id: trigger_id,
//!         placement: PopoverPlacement { flip_axes: FlipAxes::VerticalOnly, ..Default::default() },
//!         z_index: theme.z_index.dropdown,
//!         modal: true,
//!         dismiss_on_outside: true,
//!         visible_height: vh,
//!         animate: None,
//!         on_open: Some(Rc::new(|| { ... })),
//!         on_close: Some(Rc::new(|| { ... })),
//!     },
//!     content_widget,
//! );
//! ctx.register_theme_component(portal_id, &resolved, &role, &style);
//! ```

use std::cell::Cell;
use std::rc::Rc;

use auralis_signal::Signal;

use crate::animation::Animation;
use crate::core::context::MountContext;
use crate::core::element::{DirtyFlags, ElementId};
use crate::core::widget::Widget;
use crate::ecs::components;
use crate::event::TraversalEdgeBehavior;
use crate::platform::portal::PortalHeight;
use crate::style::Color;
use crate::widgets::overlay::{PopoverGeometry, PopoverPlacement, PopoverPosition};

/// Configuration for mounting a signal-driven anchored portal popup.
pub struct PortalPopupConfig {
    pub open: Signal<bool>,
    pub anchor_id: ElementId,
    pub placement: PopoverPlacement,
    pub z_index: i32,
    pub modal: bool,
    pub dismiss_on_outside: bool,
    pub visible_height: f32,
    pub animate: Option<Animation>,
    pub on_open: Option<Rc<dyn Fn()>>,
    pub on_close: Option<Rc<dyn Fn()>>,
    pub background: Option<Color>,
    pub border_color: Option<Color>,
    pub border_width: Option<f32>,
    pub corner_radius: Option<f32>,
    pub shadow: Option<crate::style::styled::Shadow>,
    pub padding: Option<crate::style::Padding>,
}

/// Allocate, register, and life-cycle a signal-driven anchored portal popup.
///
/// Returns the portal `ElementId` so the caller can apply theme styling and
/// append additional children.
///
/// This single call replaces:
/// - Element allocation + property setup
/// - `PortalHeight` / `PopoverGeometry` / `PopoverPlacement` / anchor user_data
/// - `register_portal` + optional `register_dismiss`
/// - Signal subscription → reactive_visible + portal_h + modal_scope + dirty
/// - Unmount cleanup (portal removal + modal scope pop)
pub fn mount_portal_popup(
    ctx: &mut MountContext<'_>,
    config: PortalPopupConfig,
    content: impl Widget + 'static,
) -> ElementId {
    let portal_id = ctx.arena.allocate();
    ctx.preallocate(portal_id, components::LAYOUT | components::LIFECYCLE);

    let rv: Rc<Cell<bool>> = Rc::new(Cell::new(config.open.read()));
    let portal_h: Rc<Cell<f32>> = Rc::new(Cell::new(0.0));
    let initial_w = config.placement.min_width.unwrap_or(200.0);
    let geo_cell: Rc<Cell<PopoverGeometry>> = Rc::new(Cell::new(PopoverGeometry {
        x: 0.0,
        y: 0.0,
        width: initial_w,
        height: 0.0,
        actual_position: PopoverPosition::Bottom,
    }));

    {
        let Some(el) = ctx.arena.get_mut(portal_id) else {
            return portal_id;
        };
        el.set_z_index(config.z_index);
        el.z_index_floor = Some(config.z_index);
        el.set_flex_shrink(0.0);
        el.set_layout_direction(crate::core::LayoutDirection::Vertical);
        el.set_reactive_visible(rv.clone());
        if let Some(bg) = config.background {
            el.set_background(bg);
        }
        if let Some(bc) = config.border_color {
            el.set_border_color(bc);
        }
        if let Some(bw) = config.border_width {
            el.set_border_width(bw);
        }
        if let Some(cr) = config.corner_radius {
            el.set_corner_radius(cr);
        }
        if let Some(ref s) = config.shadow {
            el.set_shadow(Some(*s));
        }
        if let Some(p) = config.padding {
            el.set_padding(p);
        }
        el.insert_user_data(PortalHeight(portal_h.clone()));
        el.insert_user_data(geo_cell.clone());
        el.insert_user_data(config.placement);
        el.insert_user_data(config.anchor_id);

        if let Some(anim) = config.animate {
            el.set_animation_config(Some(crate::animation::AnimationConfig {
                property: crate::animation::AnimatedProperty::Opacity,
                from: crate::animation::AnimatedValue::Float(0.0),
                to: crate::animation::AnimatedValue::Float(1.0),
                animation: anim,
            }));
        }
    }

    let content_id = Box::new(content).mount_box(&mut ctx.child_with_events(portal_id));
    ctx.arena.add_child(portal_id, content_id);

    crate::platform::portal::register_portal(portal_id);
    // Owner link: the anchor lives in the main tree — when its subtree is
    // torn down the portal is removed automatically (audit round 3, ①).
    crate::platform::portal::register_portal_owner(config.anchor_id, portal_id);

    if config.dismiss_on_outside {
        let dismiss_sig = config.open.clone();
        crate::platform::portal::register_dismiss(portal_id, move || {
            if dismiss_sig.read() {
                dismiss_sig.set(false);
            }
        });
    }

    {
        let open_sub = config.open.clone();
        let rv_sub = rv.clone();
        let ph = portal_h.clone();
        let scope_id = portal_id;
        let Some(el) = ctx.arena.get_mut(portal_id) else {
            return portal_id;
        };
        let dirty = el.dirty.clone();
        let vh = config.visible_height;
        let on_open = config.on_open.clone();
        let on_close = config.on_close.clone();
        let open_entry = config.open.clone();

        crate::core::signal_bridge::subscribe_owned(portal_id, &config.open, move || {
            let is_open = open_sub.read();
            let was = rv_sub.get();
            if was == is_open {
                return;
            }
            rv_sub.set(is_open);
            ph.set(if is_open { vh } else { 0.0 });
            if is_open {
                crate::event::push_modal_scope(scope_id, TraversalEdgeBehavior::Wrap);
                crate::widgets::shared::dropdown::push_popup_overlay_entry(
                    scope_id,
                    open_entry.clone(),
                );
                if let Some(ref f) = on_open {
                    f();
                }
                dirty.set(dirty.get() | DirtyFlags::MEASURE | DirtyFlags::REPAINT);
                crate::core::dirty_registry::register_dirty(scope_id, DirtyFlags::MEASURE);
                crate::core::dirty_registry::register_dirty(scope_id, DirtyFlags::REPAINT);
            } else {
                crate::event::pop_modal_scope();
                crate::event::overlay::remove(scope_id);
                if let Some(ref f) = on_close {
                    f();
                }
                dirty.set(dirty.get() | DirtyFlags::MEASURE);
                crate::core::dirty_registry::register_dirty(scope_id, DirtyFlags::MEASURE);
            }
            crate::core::dirty_registry::bump_subtree_gen(scope_id);
        });

        if config.open.read() {
            crate::event::push_modal_scope(portal_id, TraversalEdgeBehavior::Wrap);
            crate::widgets::shared::dropdown::push_popup_overlay_entry(
                portal_id,
                config.open.clone(),
            );
        }
    }

    {
        let did = portal_id;
        let on_unmount = Rc::new(std::cell::RefCell::new(Some(Box::new(move || {
            crate::platform::portal::remove_portal(did);
            crate::event::remove_modal_scopes_of(did);
        })
            as Box<dyn FnOnce()>)));
        crate::core::element::with_ct_mut(|ct| {
            ct.lc.entry(did).or_default().on_unmount = Some(on_unmount);
        });
    }

    portal_id
}
