use std::cell::Cell;
use std::rc::Rc;

use crate::core::config::{
    ElementBuilder, EventHandler, InteractionConfig, LayoutConfig, StateFlags,
};
use crate::core::context::MountContext;
use crate::core::element::DirtyFlags;
use crate::core::widget::Widget;
use crate::core::ElementId;
use crate::ecs::components;
use crate::resource::icons::Icon as IconKind;
use crate::style::styled::{StyleRefinement, Styled};
use crate::style::{Brush, Color, Dimension};
use crate::theme::m3::roles::{ComponentRole, InteractiveRole, ResolvedComponentStyle};

/// Dynamic state for the checkmark/dash icon — stored on the icon child element
/// alongside `IconPathData`.  `visible` controls whether the icon renders at all;
/// `brush_color` lets signal callbacks change the stroke colour without the arena.
pub struct CheckboxIconState {
    pub visible: Cell<bool>,
    pub brush_color: Cell<Color>,
}

// ── Widget ──

pub struct Checkbox {
    checked: auralis_signal::Signal<bool>,
    indeterminate: auralis_signal::Signal<bool>,
    on_value_changed: Option<Box<dyn Fn(bool)>>,
    disabled: bool,
    tab_index: Option<usize>,
    autofocus: bool,
    style: StyleRefinement,
}

impl Checkbox {
    pub fn new(checked: auralis_signal::Signal<bool>) -> Self {
        Self {
            checked,
            indeterminate: auralis_signal::Signal::new(false),
            on_value_changed: None,
            disabled: false,
            tab_index: None,
            autofocus: false,
            style: StyleRefinement::default(),
        }
    }

    pub fn indeterminate(mut self, indet: auralis_signal::Signal<bool>) -> Self {
        self.indeterminate = indet;
        self
    }
    pub fn disabled(mut self) -> Self {
        self.disabled = true;
        self
    }
    pub fn on_value_changed(mut self, f: impl Fn(bool) + 'static) -> Self {
        self.on_value_changed = Some(Box::new(f));
        self
    }
    pub fn tab_index(mut self, idx: usize) -> Self {
        self.tab_index = Some(idx);
        self
    }
    pub fn autofocus(mut self) -> Self {
        self.autofocus = true;
        self
    }
}

impl Styled for Checkbox {
    fn style_refinement(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl Widget for Checkbox {
    fn component_mask(&self) -> u64 {
        components::STYLE | components::LAYOUT | components::INTERACTION | components::LIFECYCLE
    }

    fn mount_box(mut self: Box<Self>, ctx: &mut MountContext<'_>) -> ElementId {
        let role = ComponentRole::Interactive(InteractiveRole::Checkbox {
            size: crate::theme::ControlSize::Medium,
        });
        let cb_style = match ctx.theme.resolve_component(&role) {
            ResolvedComponentStyle::Checkbox(s) => s,
            _ => unreachable!(),
        };

        let is_checked = self.checked.read();
        let is_indet = self.indeterminate.read();
        let disabled = self.disabled;

        let c_checked_bg = cb_style.checked_bg;
        let c_checked_bdr = cb_style.checked_bg;
        let c_unchecked_bdr = cb_style.unchecked_border;
        let c_disabled_bg = cb_style.disabled_bg;
        let c_disabled_bdr = cb_style.disabled_border;
        let c_disabled_fg = ctx.theme.scheme.disabled.foreground;
        let c_check_white = cb_style.checked_icon;
        let c_hover_unchecked = cb_style.hover_bg;
        let c_press_unchecked = cb_style.pressed_bg;

        let bg = self.style.background.unwrap_or(if disabled {
            c_disabled_bg
        } else {
            cb_style.unchecked_bg
        });
        let bdr = if disabled {
            c_disabled_bdr
        } else {
            c_unchecked_bdr
        };
        let ico = if disabled {
            c_disabled_fg
        } else {
            c_check_white
        };

        // ── Box — the rounded square (hosts interaction, focus, keyboard) ──
        let mut events = EventHandler::new();
        if !disabled {
            let ts = self.checked.clone();
            let is2 = self.indeterminate.clone();
            let oc = Rc::new(self.on_value_changed.take());
            events = events.on_click(move || {
                let v = toggle(&ts, &is2);
                if let Some(ref cb) = *oc {
                    cb(v);
                }
            });
        }

        let box_id = ElementBuilder::new()
            .with_components(
                components::STYLE
                    | components::LAYOUT
                    | components::INTERACTION
                    | components::LIFECYCLE,
            )
            .layout(LayoutConfig {
                width: Dimension::Pixels(cb_style.size),
                height: Dimension::Pixels(cb_style.size),
                flex_grow: 0.0,
                flex_shrink: 0.0,
                tab_index: self.tab_index,
                ..LayoutConfig::default()
            })
            .interaction(InteractionConfig {
                events: Some(events),
                enabled: !disabled,
                focusable: !disabled,
                cursor: crate::platform::CursorIcon::POINTER,
                ..InteractionConfig::default()
            })
            .accessibility(accesskit::Role::CheckBox, String::new())
            .build(ctx);
        {
            let Some(el) = ctx.arena.get_mut(box_id) else {
                return box_id;
            };
            el.set_affected_by_child_size(false);
            if disabled {
                el.state
                    .set(el.state.get() | crate::core::config::StateFlags::DISABLED);
            }
            el.set_background(bg);
            el.set_border_width(self.style.border_width.unwrap_or(2.0));
            el.set_border_color(bdr);
            el.set_corner_radii(self.style.corner_radius.unwrap_or(cb_style.corner_radius));
            el.set_content_align(crate::style::Alignment::Center);
            if let Some(zi) = self.style.z_index {
                el.set_z_index(zi);
            }
            if let Some(o) = self.style.opacity {
                el.set_opacity(o);
            }

            el.with_state_style(|ss| {
                ss.checked.background = if !disabled {
                    Some(c_checked_bg)
                } else {
                    Some(c_disabled_bg)
                };
                ss.checked.border_color = if !disabled { Some(c_checked_bdr) } else { None };
                ss.hovered.background = Some(c_hover_unchecked);
                ss.pressed.background = Some(c_press_unchecked);
                ss.disabled.background = Some(c_disabled_bg);
                ss.disabled.border_color = Some(c_disabled_bdr);
                ss.disabled.foreground = Some(c_disabled_fg);
            });
        }

        // Set initial checked/indeterminate flags
        if is_checked || is_indet {
            crate::core::dirty_registry::set_state(box_id, StateFlags::CHECKED, true);
        }
        if is_indet {
            crate::core::dirty_registry::set_state(box_id, StateFlags::INDETERMINATE, true);
        }

        // ── Icon child — checkmark / dash stroke path ──
        let icon_id = ctx.arena.allocate();
        ctx.preallocate(
            icon_id,
            components::STYLE | components::LAYOUT | components::TRANSFORM | components::LIFECYCLE,
        );
        let icon_state = Rc::new(CheckboxIconState {
            visible: Cell::new(is_checked || is_indet),
            brush_color: Cell::new(ico),
        });
        {
            let Some(el) = ctx.arena.get_mut(icon_id) else {
                return box_id;
            };
            el.set_preferred_width(Some(16.0));
            el.set_preferred_height(16.0);
            el.set_accessible_role(accesskit::Role::Image);
            el.set_accessible_label(String::from("checkbox icon"));

            let kind = if is_indet {
                IconKind::Minus
            } else {
                IconKind::Check
            };
            if let Some(path) = kind.build_path() {
                el.insert_user_data(crate::widgets::display::IconPathData {
                    path: Rc::new(path),
                    brush: Brush::Solid(ico),
                    stroke: kurbo::Stroke {
                        width: 2.0,
                        start_cap: kurbo::Cap::Round,
                        end_cap: kurbo::Cap::Round,
                        join: kurbo::Join::Round,
                        ..Default::default()
                    },
                });
            }
            el.insert_user_data(icon_state.clone());
        }
        ctx.arena.add_child(box_id, icon_id);

        // ── Signal subscriptions ──
        {
            let cs = self.checked.clone();
            let isig = self.indeterminate.clone();
            let ic_state = icon_state.clone();
            let d_bid = box_id;
            let d_iid = icon_id;
            crate::core::signal_bridge::subscribe_owned(box_id, &self.checked, move || {
                let c = cs.read();
                let i = isig.read();
                let active = c || i;
                let ico2 = if disabled {
                    c_disabled_fg
                } else {
                    c_check_white
                };

                crate::core::dirty_registry::set_state(d_bid, StateFlags::CHECKED, active);

                ic_state.visible.set(active);
                ic_state.brush_color.set(ico2);

                crate::core::dirty_registry::mark_dirty(d_bid, DirtyFlags::REPAINT);
                crate::core::dirty_registry::register_dirty(d_bid, DirtyFlags::REPAINT);
                crate::core::dirty_registry::mark_dirty(d_iid, DirtyFlags::REPAINT);
                crate::core::dirty_registry::register_dirty(d_iid, DirtyFlags::REPAINT);
                crate::core::dirty_registry::bump_subtree_gen(d_iid);
                crate::core::dirty_registry::bump_subtree_gen(d_bid);
            });
        }
        {
            let cs = self.checked.clone();
            let isig = self.indeterminate.clone();
            let ic_state = icon_state.clone();
            let d_bid = box_id;
            let d_iid = icon_id;
            crate::core::signal_bridge::subscribe_owned(box_id, &self.indeterminate, move || {
                let c = cs.read();
                let i = isig.read();
                let active = c || i;
                let ico2 = if disabled {
                    c_disabled_fg
                } else {
                    c_check_white
                };

                crate::core::dirty_registry::set_state(d_bid, StateFlags::CHECKED, active);
                crate::core::dirty_registry::set_state(d_bid, StateFlags::INDETERMINATE, i);

                ic_state.visible.set(active);
                ic_state.brush_color.set(ico2);

                crate::core::dirty_registry::mark_dirty(d_bid, DirtyFlags::REPAINT);
                crate::core::dirty_registry::register_dirty(d_bid, DirtyFlags::REPAINT);
                crate::core::dirty_registry::mark_dirty(d_iid, DirtyFlags::REPAINT);
                crate::core::dirty_registry::register_dirty(d_iid, DirtyFlags::REPAINT);
                crate::core::dirty_registry::bump_subtree_gen(d_iid);
                crate::core::dirty_registry::bump_subtree_gen(d_bid);
            });
        }

        if !disabled && self.autofocus {
            if let Some(reg) = ctx.event_registry.as_mut() {
                reg.request_autofocus(box_id);
            }
        }

        ctx.register_theme_component(
            box_id,
            &ResolvedComponentStyle::Checkbox(cb_style.clone()),
            &role,
            &self.style,
        );
        ctx.register_theme_component(
            icon_id,
            &ResolvedComponentStyle::Checkbox(cb_style.clone()),
            &role,
            &self.style,
        );

        box_id
    }
}

fn toggle(checked: &auralis_signal::Signal<bool>, indet: &auralis_signal::Signal<bool>) -> bool {
    if indet.read() {
        indet.set(false);
        checked.set(false);
        false
    } else {
        let v = !checked.read();
        checked.set(v);
        v
    }
}

impl std::fmt::Debug for Checkbox {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Checkbox")
            .field("disabled", &self.disabled)
            .finish_non_exhaustive()
    }
}
