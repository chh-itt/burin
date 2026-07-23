use std::cell::Cell;
use std::rc::Rc;

use crate::animation::{self, AnimatedProperty, AnimatedValue};
use crate::core::config::{ElementBuilder, EventHandler, InteractionConfig, LayoutConfig};
use crate::core::context::MountContext;
use crate::core::element::{DirtyFlags, Element};
use crate::core::widget::Widget;
use crate::core::ElementId;
use crate::ecs::components;
use crate::style::styled::{StyleRefinement, Styled};
use crate::style::{Dimension, StateStyle, Vec2};
use crate::theme::m3::roles::{ComponentRole, InteractiveRole, ResolvedComponentStyle};

pub struct Switch {
    checked: auralis_signal::Signal<bool>,
    on_value_changed: Option<Box<dyn Fn(bool)>>,
    disabled: bool,
    tab_index: Option<usize>,
    autofocus: bool,
    style: StyleRefinement,
}

impl Switch {
    pub fn new(checked: auralis_signal::Signal<bool>) -> Self {
        Self {
            checked,
            on_value_changed: None,
            disabled: false,
            tab_index: None,
            autofocus: false,
            style: StyleRefinement::default(),
        }
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

impl Styled for Switch {
    fn style_refinement(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl Widget for Switch {
    fn component_mask(&self) -> u64 {
        components::STYLE | components::LAYOUT | components::INTERACTION | components::LIFECYCLE
    }

    fn mount_box(mut self: Box<Self>, ctx: &mut MountContext<'_>) -> ElementId {
        let role = ComponentRole::Interactive(InteractiveRole::Switch {
            size: crate::theme::ControlSize::Medium,
        });
        let sw_style = match ctx.theme.resolve_component(&role) {
            ResolvedComponentStyle::Switch(s) => s,
            _ => unreachable!(),
        };
        let is_checked = self.checked.read();
        let disabled = self.disabled;

        let c_track_bg = sw_style.checked_bg;
        let c_unchecked_bg = sw_style.unchecked_bg;
        let c_disabled_bg = sw_style.disabled_bg;
        let c_disabled_bdr = sw_style.disabled_bg;
        let c_disabled_fg = sw_style.disabled_thumb;
        let c_thumb_white = sw_style.unchecked_thumb;
        let taffy_centre = (sw_style.width - sw_style.thumb_size) * 0.5;
        let thumb_on_x = (sw_style.width - sw_style.thumb_size - 2.0) - taffy_centre;
        let thumb_off_x = 2.0 - taffy_centre;

        let track_bg0 = self.style.background.unwrap_or(if disabled {
            sw_style.disabled_bg
        } else {
            sw_style.unchecked_bg
        });
        let track_bdr0 = if disabled { c_disabled_bdr } else { c_track_bg };
        let thumb0 = if disabled {
            c_disabled_fg
        } else {
            c_thumb_white
        };
        let chk_bg0 = sw_style.checked_bg;

        // ── Track — pill-shaped bar (hosts interaction, focus, keyboard) ──
        let track_radius = sw_style.height * 0.5;

        let mut events = EventHandler::new();
        if !disabled {
            let ts = self.checked.clone();
            let oc = self.on_value_changed.take();
            events = events.on_click(move || {
                let v = !ts.read();
                ts.set(v);
                if let Some(ref cb) = oc {
                    cb(v);
                }
            });
        }

        let track_id = ElementBuilder::new()
            .with_components(
                components::STYLE
                    | components::LAYOUT
                    | components::INTERACTION
                    | components::LIFECYCLE,
            )
            .layout(LayoutConfig {
                width: Dimension::Pixels(sw_style.width),
                height: Dimension::Pixels(sw_style.height),
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
            .accessibility(accesskit::Role::Switch, String::new())
            .build(ctx);

        {
            let Some(el) = ctx.arena.get_mut(track_id) else {
                return track_id;
            };
            el.set_background(track_bg0);
            el.set_corner_radius(track_radius);
            el.set_affected_by_child_size(false);
            el.set_alignment(crate::style::Alignment::Center);
            el.set_content_align(crate::style::Alignment::Center);
            if let Some(bw) = self.style.border_width {
                el.set_border_width(bw);
            }
            el.set_border_color(track_bdr0);
            if let Some(zi) = self.style.z_index {
                el.set_z_index(zi);
            }
            if let Some(o) = self.style.opacity {
                el.set_opacity(o);
            }
            el.with_state_style(|ss| {
                ss.checked.background = Some(chk_bg0);
            });
            if disabled {
                el.state
                    .set(el.state.get() | crate::core::config::StateFlags::DISABLED);
            }
            if is_checked {
                crate::core::dirty_registry::set_state(
                    track_id,
                    crate::core::config::StateFlags::CHECKED,
                    true,
                );
            }
        }

        // ── Thumb child — circle sliding inside the track ──
        let thumb_id = ctx.arena.allocate();
        let thumb_radius = sw_style.thumb_size * 0.5;
        let thumb_offset = Rc::new(Cell::new(Vec2::new(
            if is_checked { thumb_on_x } else { thumb_off_x },
            0.0,
        )));
        ctx.preallocate(
            thumb_id,
            components::STYLE | components::LAYOUT | components::TRANSFORM | components::LIFECYCLE,
        );
        {
            let Some(el) = ctx.arena.get_mut(thumb_id) else {
                return track_id;
            };
            el.set_background(thumb0);
            el.set_corner_radius(thumb_radius);
            el.set_preferred_width(Some(sw_style.thumb_size));
            el.set_preferred_height(sw_style.thumb_size);
            el.set_flex_grow(0.0);
            el.set_flex_shrink(0.0);
            el.set_affected_by_child_size(false);
            el.set_position_offset(thumb_offset.clone());
        }
        ctx.arena.add_child(track_id, thumb_id);

        // ── Signal subscriptions ──
        let track_dirty = ctx.arena.get(track_id).unwrap().dirty.clone();
        let thumb_dirty = ctx.arena.get(thumb_id).unwrap().dirty.clone();
        {
            let cs = self.checked.clone();
            let to2 = thumb_offset.clone();
            let t_bid = track_id;
            let t_tid = thumb_id;
            let t_dirty = track_dirty.clone();
            let t_tdirty = thumb_dirty.clone();
            let on_x = thumb_on_x;
            let off_x = thumb_off_x;
            let anim_target = thumb_id;
            crate::core::signal_bridge::subscribe_owned(track_id, &self.checked, move || {
                let c = cs.read();
                let bg2 = if disabled {
                    c_disabled_bg
                } else {
                    c_unchecked_bg
                };
                let bdr2 = if disabled {
                    c_disabled_bdr
                } else {
                    c_unchecked_bg
                };
                let chk2 = c_track_bg;
                let th2 = if disabled {
                    c_disabled_fg
                } else {
                    c_thumb_white
                };

                crate::core::dirty_registry::set_state(
                    t_bid,
                    crate::core::config::StateFlags::CHECKED,
                    c,
                );

                crate::core::element::with_ct_mut(|ct| {
                    let s = ct.style.entry(t_bid).or_default();
                    s.background = Some(bg2);
                    s.border_color = Some(bdr2);
                    let ss = s.state_style.get_or_insert_with(StateStyle::default);
                    ss.checked.background = Some(chk2);
                    let ts = ct.style.entry(t_tid).or_default();
                    ts.background = Some(th2);
                });

                let current_x = to2.get().x;
                let target_x = if c { on_x } else { off_x };
                if (current_x - target_x).abs() > 0.01 {
                    animation::request_anim(
                        anim_target,
                        AnimatedProperty::Position,
                        AnimatedValue::Float(current_x),
                        AnimatedValue::Float(target_x),
                        animation::Animation::toggle(),
                    );
                }

                Element::mark_surface_dirty_remote(&t_dirty, t_bid);
                Element::mark_repaint_remote(&t_tdirty, t_tid);
                crate::core::dirty_registry::register_dirty(t_bid, DirtyFlags::REPAINT);
                crate::core::dirty_registry::bump_subtree_gen(t_bid);
            });
        }

        if !disabled && self.autofocus {
            if let Some(reg) = ctx.event_registry.as_mut() {
                reg.request_autofocus(track_id);
            }
        }

        ctx.register_theme_component(
            track_id,
            &ResolvedComponentStyle::Switch(sw_style.clone()),
            &role,
            &self.style,
        );

        // Thumb is a plain circle — no theme registration (would overwrite its size).

        track_id
    }
}

impl std::fmt::Debug for Switch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Switch")
            .field("disabled", &self.disabled)
            .finish_non_exhaustive()
    }
}
