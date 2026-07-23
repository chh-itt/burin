use std::cell::Cell;
use std::rc::Rc;

use crate::animation::{self, AnimatedProperty, AnimatedValue};
use crate::core::config::{
    ElementBuilder, EventHandler, InteractionConfig, LayoutConfig, PaintConfig,
};
use crate::core::context::MountContext;
use crate::core::element::{DirtyFlags, Element};
use crate::core::widget::Widget;
use crate::core::ElementId;
use crate::ecs::components;
use crate::style::styled::{StyleRefinement, Styled};
use crate::style::{Color, Dimension, Padding, StateStyle, Vec2};
use crate::theme::m3::roles::{ComponentRole, InteractiveRole, ResolvedComponentStyle};
use crate::theme::tokens;
use auralis_signal::Signal;

// Re-exported for backward compatibility.
pub struct RadioButton<T: Clone + PartialEq + 'static> {
    pub value: T,
    pub label: String,
    pub disabled: bool,
}

impl<T: Clone + PartialEq + 'static> RadioButton<T> {
    pub fn new(value: T, label: impl Into<String>) -> Self {
        Self {
            value,
            label: label.into(),
            disabled: false,
        }
    }
    pub fn disabled(mut self) -> Self {
        self.disabled = true;
        self
    }
}

pub struct RadioGroup<T: Clone + PartialEq + 'static> {
    selected: Signal<T>,
    on_value_changed: Option<Rc<dyn Fn(T)>>,
    options: Vec<RadioOption<T>>,
    disabled: bool,
    style: StyleRefinement,
}

struct RadioOption<T: Clone + PartialEq + 'static> {
    label: String,
    value: T,
    disabled: bool,
}

impl<T: Clone + PartialEq + 'static> RadioGroup<T> {
    pub fn new(selected: Signal<T>) -> Self {
        Self {
            selected,
            on_value_changed: None,
            options: Vec::new(),
            disabled: false,
            style: StyleRefinement::default(),
        }
    }

    pub fn disabled(mut self) -> Self {
        self.disabled = true;
        self
    }
    pub fn on_value_changed(mut self, f: impl Fn(T) + 'static) -> Self {
        self.on_value_changed = Some(Rc::new(f));
        self
    }
    pub fn option(mut self, label: impl Into<String>, value: T) -> Self {
        self.options.push(RadioOption {
            label: label.into(),
            value,
            disabled: false,
        });
        self
    }
    pub fn disabled_option(mut self, label: impl Into<String>, value: T) -> Self {
        self.options.push(RadioOption {
            label: label.into(),
            value,
            disabled: true,
        });
        self
    }
}

impl<T: Clone + PartialEq + 'static> Styled for RadioGroup<T> {
    fn style_refinement(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl<T: Clone + PartialEq + 'static> Widget for RadioGroup<T> {
    fn component_mask(&self) -> u64 {
        components::STYLE
            | components::LAYOUT
            | components::INTERACTION
            | components::TEXT
            | components::LIFECYCLE
    }

    fn mount_box(mut self: Box<Self>, ctx: &mut MountContext<'_>) -> ElementId {
        let theme = ctx.theme;
        let role = ComponentRole::Interactive(InteractiveRole::Radio {
            size: crate::theme::ControlSize::Medium,
        });
        let r_style = match theme.resolve_component(&role) {
            ResolvedComponentStyle::Radio(s) => s,
            _ => unreachable!(),
        };
        let disabled_all = self.disabled;
        let _n = self.options.len();
        let fs = self
            .style
            .font_size
            .unwrap_or(theme.typescale.body.small.size);

        // Capture colours for signal closures
        let c_checked_bg = r_style.checked_bg;
        let c_checked_fg = r_style.checked_dot;
        let c_unchecked_bg = r_style.unchecked_bg;
        let c_unchecked_bdr = r_style.unchecked_border;
        let c_disabled_bg = r_style.disabled_bg;
        let c_disabled_bdr = r_style.disabled_border;
        let c_disabled_fg = theme.scheme.disabled.foreground;
        let c_hover_checked = r_style.hover_bg;
        let c_hover_uncheck = r_style.hover_bg;
        let c_press_checked = r_style.hover_bg;
        let c_press_uncheck = r_style.hover_bg;

        // ── Container (RadioGroup) ──
        let container_id = ElementBuilder::new()
            .with_components(self.component_mask())
            .layout(LayoutConfig {
                width: self.style.width.unwrap_or(Dimension::Auto),
                height: self.style.height.unwrap_or(Dimension::Auto),
                padding: self.style.padding.unwrap_or(Padding::ZERO),
                margin: self.style.margin.unwrap_or_default(),
                gap: self.style.gap.unwrap_or(tokens::S1),
                ..LayoutConfig::default()
            })
            .interaction(InteractionConfig::default())
            .paint(PaintConfig::default())
            .accessibility(accesskit::Role::RadioGroup, String::new())
            .build(ctx);

        {
            let Some(el) = ctx.arena.get_mut(container_id) else {
                return container_id;
            };
            el.set_layout_direction(crate::core::LayoutDirection::Vertical);
        }

        let group_sig = self.selected.clone();
        let on_change = self.on_value_changed.take();

        // Extract style values before drain() borrows self.options
        let cm = self.component_mask();
        let row_width = self.style.width.unwrap_or(Dimension::Auto);
        let row_margin = self.style.margin.unwrap_or_default();
        let bdr_width = self.style.border_width.unwrap_or(2.0);

        for opt in self.options.drain(..) {
            let is_selected = group_sig.read() == opt.value;
            let item_disabled = disabled_all || opt.disabled;

            // ── Row: label (leading) + circle ──
            let row_pad = Padding::all(tokens::S1);
            let row_gap = 8.0;

            let mut events = EventHandler::new();
            if !item_disabled {
                let sig = group_sig.clone();
                let val = opt.value.clone();
                let oc = on_change.clone();
                events = events.on_click(move || {
                    sig.set(val.clone());
                    if let Some(ref cb) = oc {
                        cb(val.clone());
                    }
                });
            }

            let row_id = ElementBuilder::new()
                .with_components(cm)
                .layout(LayoutConfig {
                    width: row_width,
                    height: Dimension::Auto,
                    padding: row_pad,
                    margin: row_margin,
                    gap: row_gap,
                    ..LayoutConfig::default()
                })
                .interaction(InteractionConfig {
                    events: Some(events),
                    enabled: !item_disabled,
                    focusable: !item_disabled,
                    cursor: crate::platform::CursorIcon::POINTER,
                    ..InteractionConfig::default()
                })
                .paint(PaintConfig::default())
                .accessibility(accesskit::Role::RadioButton, opt.label.clone())
                .build(ctx);

            {
                let Some(el) = ctx.arena.get_mut(row_id) else {
                    return container_id;
                };
                el.set_layout_direction(crate::core::LayoutDirection::Horizontal);
                el.set_alignment(crate::style::Alignment::Center);
                el.set_preferred_height(fs.max(r_style.size) + row_pad.top + row_pad.bottom);
                if item_disabled {
                    el.state
                        .set(el.state.get() | crate::core::config::StateFlags::DISABLED);
                }
            }

            // ── Circle (leading) — fixed position, all circles align ──
            let circle_id = ctx.arena.allocate();
            ctx.preallocate(circle_id, components::STYLE | components::LAYOUT);
            let circle_radius = r_style.size * 0.5;
            let style_bg = self.style.background;
            {
                let Some(el) = ctx.arena.get_mut(circle_id) else {
                    return container_id;
                };
                let circle_bg0 = if item_disabled {
                    r_style.disabled_bg
                } else if is_selected {
                    r_style.checked_bg
                } else {
                    r_style.unchecked_bg
                };
                el.set_background(style_bg.unwrap_or(circle_bg0));
                el.with_state_style(|ss| {
                    ss.hovered.background = Some(if item_disabled {
                        r_style.disabled_bg
                    } else if is_selected {
                        r_style.hover_bg
                    } else {
                        r_style.hover_bg
                    });
                    ss.pressed.background = Some(if item_disabled {
                        r_style.disabled_bg
                    } else if is_selected {
                        r_style.hover_bg
                    } else {
                        r_style.hover_bg
                    });
                });
                el.set_border_width(bdr_width);
                el.set_border_color(if item_disabled {
                    r_style.disabled_border
                } else if is_selected {
                    r_style.checked_bg
                } else {
                    r_style.unchecked_border
                });
                el.set_corner_radius(circle_radius);
                el.set_preferred_width(Some(r_style.size));
                el.set_preferred_height(r_style.size);
                el.set_flex_grow(0.0);
                el.set_flex_shrink(0.0);
                el.set_alignment(crate::style::Alignment::Center);
                el.set_content_align(crate::style::Alignment::Center);
                el.set_affected_by_child_size(false);
            }
            ctx.arena.add_child(row_id, circle_id);

            // ── Inner dot ──
            let dot_id = ctx.arena.allocate();
            ctx.preallocate(
                dot_id,
                components::STYLE
                    | components::LAYOUT
                    | components::TRANSFORM
                    | components::LIFECYCLE,
            );
            let dot_radius = 5.0;
            let dot_scale = Rc::new(Cell::new(Vec2::new(
                if is_selected { 1.0 } else { 0.0 },
                if is_selected { 1.0 } else { 0.0 },
            )));
            {
                let Some(el) = ctx.arena.get_mut(dot_id) else {
                    return container_id;
                };
                el.set_background(if item_disabled {
                    c_disabled_fg
                } else if is_selected {
                    r_style.checked_dot
                } else {
                    Color::TRANSPARENT
                });
                el.set_corner_radius(dot_radius);
                el.set_preferred_width(Some(10.0));
                el.set_preferred_height(10.0);
                el.set_size_scale(dot_scale.clone());
            }
            ctx.arena.add_child(circle_id, dot_id);

            // ── Label (after circle) ──
            let label_fg = if item_disabled {
                theme.scheme.disabled.foreground
            } else {
                theme.scheme.on_surface
            };
            let tw = crate::widgets::display::Text::new(opt.label.clone())
                .font_size(fs)
                .color(label_fg);
            let label_id = {
                let mut cc = ctx.child_with_events(row_id);
                Box::new(tw).mount_box(&mut cc)
            };
            ctx.arena.add_child(row_id, label_id);

            // ── Signal subscription ──
            let circle_dirty = ctx.arena.get(circle_id).unwrap().dirty.clone();
            let dot_dirty = ctx.arena.get(dot_id).unwrap().dirty.clone();
            {
                let cs2 = group_sig.clone();
                let v2 = opt.value.clone();
                let c_id = circle_id;
                let d_id = dot_id;
                let cd = circle_dirty.clone();
                let dd = dot_dirty.clone();
                let ds = dot_scale.clone();
                crate::core::signal_bridge::subscribe_owned(row_id, &group_sig, move || {
                    let checked = cs2.read() == v2;

                    crate::core::dirty_registry::set_state(
                        c_id,
                        crate::core::config::StateFlags::CHECKED,
                        checked,
                    );

                    crate::core::element::with_ct_mut(|ct| {
                        let s = ct.style.entry(c_id).or_default();
                        s.background = Some(if item_disabled {
                            c_disabled_bg
                        } else if checked {
                            c_checked_bg
                        } else {
                            c_unchecked_bg
                        });
                        s.border_color = Some(if item_disabled {
                            c_disabled_bdr
                        } else if checked {
                            c_checked_bg
                        } else {
                            c_unchecked_bdr
                        });
                        let ss = s.state_style.get_or_insert_with(StateStyle::default);
                        ss.hovered.background = if item_disabled {
                            Some(c_disabled_bg)
                        } else if checked {
                            Some(c_hover_checked)
                        } else {
                            Some(c_hover_uncheck)
                        };
                        ss.pressed.background = if item_disabled {
                            Some(c_disabled_bg)
                        } else if checked {
                            Some(c_press_checked)
                        } else {
                            Some(c_press_uncheck)
                        };

                        let ts = ct.style.entry(d_id).or_default();
                        ts.background = Some(if item_disabled {
                            c_disabled_fg
                        } else if checked {
                            c_checked_fg
                        } else {
                            Color::TRANSPARENT
                        });
                    });

                    Element::mark_repaint_remote(&cd, c_id);
                    Element::mark_repaint_remote(&dd, d_id);
                    crate::core::dirty_registry::register_dirty(c_id, DirtyFlags::REPAINT);
                    crate::core::dirty_registry::bump_subtree_gen(c_id);

                    // Animate dot scale (0→1 or 1→0)
                    let target_s = if checked { 1.0 } else { 0.0 };
                    let current_s = ds.get().x;
                    if (current_s - target_s).abs() > 0.01 {
                        animation::request_anim(
                            d_id,
                            AnimatedProperty::Size,
                            AnimatedValue::Float(current_s),
                            AnimatedValue::Float(target_s),
                            animation::Animation::toggle(),
                        );
                    }
                });
            }

            ctx.arena.add_child(container_id, row_id);
        }

        ctx.register_theme_component(
            container_id,
            &ResolvedComponentStyle::Radio(r_style.clone()),
            &role,
            &self.style,
        );

        container_id
    }
}

impl<T: Clone + PartialEq + 'static> std::fmt::Debug for RadioGroup<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RadioGroup")
            .field("disabled", &self.disabled)
            .field("options", &self.options.len())
            .finish_non_exhaustive()
    }
}
