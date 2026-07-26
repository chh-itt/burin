use std::cell::Cell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use crate::animation::{AnimatedProperty, AnimatedValue, Animation, AnimationConfig, EasingCurve};
use crate::core::config::EventHandler;
use crate::core::context::MountContext;
use crate::core::element::{DirtyFlags, ElementId};
use crate::core::widget::Widget;
use crate::ecs::components;
use crate::event::types::Key;
use crate::platform::portal::PortalHeight;
use crate::style::TooltipPlacement;
use crate::theme::m3::roles::{ComponentRole, DisplayRole, ResolvedComponentStyle};
use crate::widgets::overlay::{FlipAxes, PopoverGeometry, PopoverPlacement, PopoverPosition};

thread_local! {
    static LAST_TOOLTIP_HIDE: Cell<Option<Instant>> = const { Cell::new(None) };
}

const GRACE_MS: u64 = 300;
const SCHEDULER_KEY: u64 = crate::core::scheduler::keys::TOOLTIP;

/// A tooltip that appears on hover with a configurable delay.
pub struct Tooltip {
    child: Option<Box<dyn Widget>>,
    content: Option<Box<dyn Widget>>,
    placement: TooltipPlacement,
    delay_ms: u64,
    gap: f32,
}

impl Tooltip {
    pub fn new(child: impl Widget + 'static, content: impl Widget + 'static) -> Self {
        Self {
            child: Some(Box::new(child)),
            content: Some(Box::new(content)),
            placement: TooltipPlacement::Top,
            delay_ms: 300,
            gap: 4.0,
        }
    }

    pub fn placement(mut self, p: TooltipPlacement) -> Self {
        self.placement = p;
        self
    }
    pub fn delay(mut self, ms: u64) -> Self {
        self.delay_ms = ms;
        self
    }
    pub fn gap(mut self, px: f32) -> Self {
        self.gap = px;
        self
    }
}

impl Widget for Tooltip {
    fn mount_box(mut self: Box<Self>, ctx: &mut MountContext<'_>) -> ElementId {
        let theme = ctx.theme;

        let tooltip_role = ComponentRole::Display(DisplayRole::Tooltip);
        let resolved = theme.resolve_component(&tooltip_role);
        let ts = match &resolved {
            ResolvedComponentStyle::Tooltip(s) => s.clone(),
            _ => unreachable!(),
        };

        // ── Root container ──
        let id = ctx.arena.allocate();
        ctx.preallocate(
            id,
            components::STYLE | components::LAYOUT | components::LIFECYCLE,
        );
        {
            let Some(el) = ctx.arena.get_mut(id) else {
                return id;
            };
            el.set_layout_direction(crate::core::LayoutDirection::Vertical);
        }

        // ── Mount anchor (child widget) ──
        let anchor_id = self
            .child
            .take()
            .unwrap()
            .mount_box(&mut ctx.child_with_events(id));
        ctx.arena.add_child(id, anchor_id);

        // ── Tooltip state ──
        let hover_start: Rc<Cell<Option<Instant>>> = Rc::new(Cell::new(None));
        let hovered: Rc<Cell<bool>> = Rc::new(Cell::new(false));
        let tip_visible: Rc<Cell<bool>> = Rc::new(Cell::new(false));

        // ── Preferred popover position ──
        let popover_pos = match self.placement {
            TooltipPlacement::Top | TooltipPlacement::Auto => PopoverPosition::Top,
            TooltipPlacement::Bottom => PopoverPosition::Bottom,
            TooltipPlacement::Left => PopoverPosition::Left,
            TooltipPlacement::Right => PopoverPosition::Right,
        };

        // ── Portal overlay container ──
        let tooltip_id = ctx.arena.allocate();
        ctx.preallocate(tooltip_id, components::LAYOUT | components::LIFECYCLE);

        let portal_h: Rc<Cell<f32>> = Rc::new(Cell::new(0.0));
        let rv: Rc<Cell<bool>> = Rc::new(Cell::new(false));
        let geo_cell: Rc<Cell<PopoverGeometry>> = Rc::new(Cell::new(PopoverGeometry {
            x: 0.0,
            y: 0.0,
            width: 150.0,
            height: 0.0,
            actual_position: popover_pos,
        }));

        let placement_config = PopoverPlacement {
            preferred_position: popover_pos,
            gap: self.gap,
            viewport_margin: 8.0,
            auto_flip: true,
            flip_axes: FlipAxes::Both,
            min_width: Some(150.0),
            max_width: Some(300.0),
            ..Default::default()
        };

        {
            let Some(el) = ctx.arena.get_mut(tooltip_id) else {
                return id;
            };
            el.set_z_index(theme.z_index.tooltip);
            el.z_index_floor = Some(theme.z_index.tooltip);
            el.set_background(ts.background);
            el.set_foreground(ts.foreground);
            el.set_font_size(ts.font_size);
            el.set_corner_radii(ts.corner_radius);
            el.set_padding(ts.padding);
            el.set_flex_shrink(0.0);
            el.set_layout_direction(crate::core::LayoutDirection::Vertical);
            el.set_reactive_visible(rv.clone());
            el.insert_user_data(PortalHeight(portal_h.clone()));
            el.insert_user_data(geo_cell.clone());
            el.insert_user_data(placement_config);
            el.insert_user_data(anchor_id);

            el.set_animation_config(Some(AnimationConfig {
                property: AnimatedProperty::Opacity,
                from: AnimatedValue::Float(0.0),
                to: AnimatedValue::Float(1.0),
                animation: Animation {
                    curve: EasingCurve::EaseOut,
                    duration_secs: 0.15,
                },
            }));
        }

        // ── Mount content inside portal ──
        let content_id = self
            .content
            .take()
            .unwrap()
            .mount_box(&mut ctx.child_with_events(tooltip_id));
        ctx.arena.add_child(tooltip_id, content_id);

        // ── Register portal (without dismiss — tooltip is not click-dismissed) ──
        crate::platform::portal::register_portal(tooltip_id);
        crate::platform::portal::register_portal_owner(id, tooltip_id);

        // ── Frame tick: check hover timer + sync visibility ──
        {
            let ft_hs = hover_start.clone();
            let ft_hv = hovered.clone();
            let ft_tv = tip_visible.clone();
            let ft_delay = self.delay_ms;
            let ft_rv = rv.clone();
            let ft_ph = portal_h.clone();
            let ft_scope_id = tooltip_id;
            let Some(el_ft) = ctx.arena.get_mut(tooltip_id) else {
                return id;
            };
            let ft_dirty = el_ft.dirty.clone();

            let Some(el_ft_id) = ctx.arena.get_mut(id) else {
                return id;
            };
            el_ft_id.set_frame_tick(Box::new(move || {
                let should_show = if ft_hv.get() {
                    if let Some(start) = ft_hs.get() {
                        let effective_delay = {
                            let last_hide = LAST_TOOLTIP_HIDE.with(|l| l.get());
                            if let Some(lh) = last_hide {
                                if crate::core::clock::now().duration_since(lh).as_millis()
                                    < GRACE_MS as u128
                                {
                                    0
                                } else {
                                    ft_delay
                                }
                            } else {
                                ft_delay
                            }
                        };
                        if start.elapsed().as_millis() >= effective_delay as u128 {
                            true
                        } else {
                            crate::core::scheduler::schedule_at(
                                start + Duration::from_millis(effective_delay),
                                SCHEDULER_KEY,
                            );
                            false
                        }
                    } else {
                        false
                    }
                } else {
                    false
                };

                let was = ft_tv.get();
                if was != should_show {
                    ft_tv.set(should_show);
                    ft_rv.set(should_show);
                    ft_ph.set(if should_show { 100.0 } else { 0.0 });
                    if !should_show {
                        LAST_TOOLTIP_HIDE.with(|l| l.set(Some(crate::core::clock::now())));
                        crate::core::scheduler::cancel(SCHEDULER_KEY);
                    }
                    ft_dirty.set(ft_dirty.get() | DirtyFlags::MEASURE | DirtyFlags::REPAINT);
                    crate::core::dirty_registry::register_dirty(ft_scope_id, DirtyFlags::MEASURE);
                    crate::core::dirty_registry::register_dirty(ft_scope_id, DirtyFlags::REPAINT);
                    crate::core::dirty_registry::bump_subtree_gen(ft_scope_id);
                }
            }));
        }

        // ── Unmount guard ──
        {
            let did = tooltip_id;
            let on_unmount = Rc::new(std::cell::RefCell::new(Some(Box::new(move || {
                crate::platform::portal::remove_portal(did);
                crate::core::scheduler::cancel(SCHEDULER_KEY);
            })
                as Box<dyn FnOnce()>)));
            crate::core::element::with_ct_mut(|ct| {
                ct.lc.entry(did).or_default().on_unmount = Some(on_unmount);
            });
        }

        // ── Event handlers on anchor ──
        {
            let events = EventHandler::new()
                .on_hover_enter({
                    let hs = hover_start.clone();
                    let hv = hovered.clone();
                    move || {
                        hv.set(true);
                        hs.set(Some(crate::core::clock::now()));
                    }
                })
                .on_hover_leave({
                    let hs = hover_start.clone();
                    let hv = hovered.clone();
                    let tv = tip_visible.clone();
                    let rv_l = rv.clone();
                    let ph_l = portal_h.clone();
                    let did = tooltip_id;
                    let Some(el) = ctx.arena.get_mut(tooltip_id) else {
                        return id;
                    };
                    let dirty = el.dirty.clone();
                    move || {
                        hs.set(None);
                        hv.set(false);
                        if tv.get() {
                            tv.set(false);
                            rv_l.set(false);
                            ph_l.set(0.0);
                            LAST_TOOLTIP_HIDE.with(|l| l.set(Some(crate::core::clock::now())));
                            crate::core::scheduler::cancel(SCHEDULER_KEY);
                            dirty.set(dirty.get() | DirtyFlags::MEASURE | DirtyFlags::REPAINT);
                            crate::core::dirty_registry::register_dirty(did, DirtyFlags::MEASURE);
                            crate::core::dirty_registry::register_dirty(did, DirtyFlags::REPAINT);
                            crate::core::dirty_registry::bump_subtree_gen(did);
                        }
                    }
                })
                .on_scroll({
                    let tv = tip_visible.clone();
                    move |_dx, _dy| {
                        if tv.get() {
                            tv.set(false);
                        }
                        false
                    }
                })
                .on_key_down({
                    let tv = tip_visible.clone();
                    move |key, _mods| {
                        if key == Key::Escape && tv.get() {
                            tv.set(false);
                            true
                        } else {
                            false
                        }
                    }
                });

            if let Some(reg) = ctx.event_registry.as_mut() {
                events.register_all(reg, anchor_id);
            }
        }

        // ── Register theme ──
        ctx.register_theme_component(
            tooltip_id,
            &ResolvedComponentStyle::Tooltip(ts),
            &tooltip_role,
            &crate::style::styled::StyleRefinement::default(),
        );

        id
    }
}

impl std::fmt::Debug for Tooltip {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Tooltip")
            .field("placement", &self.placement)
            .field("delay_ms", &self.delay_ms)
            .finish_non_exhaustive()
    }
}
