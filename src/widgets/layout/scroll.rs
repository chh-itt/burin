use std::cell::Cell;
use std::rc::Rc;

use crate::core::context::MountContext;
use crate::core::widget::Widget;
use crate::core::ElementId;
use crate::ecs::components;
use crate::physics::ScrollPhysics;
use crate::style::styled::{StyleRefinement, Styled};
use crate::style::Dimension;
use crate::style::Vec2;
use crate::widgets::bundle::ScrollBundle;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ScrollDirection {
    Vertical,
    Horizontal,
    Both,
}

pub struct ScrollView {
    child: Option<Box<dyn Widget>>,
    direction: ScrollDirection,
    scrollbar_width: f32,
    initial_scroll: Vec2,
    bind_offset_signal: Option<auralis_signal::Signal<Vec2>>,
    scroll_to_target: Option<ElementId>,
    fixed_width: Option<Dimension>,
    fixed_height: Option<Dimension>,
    physics: Option<Box<dyn ScrollPhysics>>,
    style: StyleRefinement,
}

impl ScrollView {
    pub fn new() -> Self {
        Self {
            child: None,
            direction: ScrollDirection::Vertical,
            scrollbar_width: 10.0,
            initial_scroll: Vec2::ZERO,
            bind_offset_signal: None,
            scroll_to_target: None,
            fixed_width: None,
            fixed_height: None,
            physics: None,
            style: StyleRefinement::default(),
        }
    }

    pub fn child(mut self, widget: impl Widget + 'static) -> Self {
        self.child = Some(Box::new(widget));
        self
    }

    pub fn scroll_direction(mut self, dir: ScrollDirection) -> Self {
        self.direction = dir;
        self
    }

    pub fn scrollbar_width(mut self, w: f32) -> Self {
        self.scrollbar_width = w;
        self
    }

    pub fn scroll_to(mut self, x: f32, y: f32) -> Self {
        self.initial_scroll = Vec2::new(x.max(0.0), y.max(0.0));
        self
    }

    pub fn scroll_to_element(mut self, target: ElementId) -> Self {
        self.scroll_to_target = Some(target);
        self
    }

    pub fn bind_offset(mut self, offset: auralis_signal::Signal<Vec2>) -> Self {
        self.bind_offset_signal = Some(offset);
        self
    }

    pub fn width(mut self, w: impl Into<Dimension>) -> Self {
        self.fixed_width = Some(w.into());
        self.style.width = Some(self.fixed_width.unwrap());
        self
    }

    pub fn height(mut self, h: impl Into<Dimension>) -> Self {
        self.fixed_height = Some(h.into());
        self.style.height = Some(self.fixed_height.unwrap());
        self
    }

    /// Set custom scroll physics (e.g. ClampPhysics, BouncePhysics).
    /// Defaults to PlatformPhysics (BouncePhysics on macOS, ClampPhysics elsewhere).
    pub fn physics(mut self, physics: impl ScrollPhysics + 'static) -> Self {
        self.physics = Some(Box::new(physics));
        self
    }
}

impl Styled for ScrollView {
    fn style_refinement(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl Widget for ScrollView {
    fn component_mask(&self) -> u64 {
        components::STYLE
            | components::LAYOUT
            | components::SCROLL
            | components::TEXT
            | components::TRANSFORM
    }

    fn mount_box(self: Box<Self>, ctx: &mut MountContext<'_>) -> ElementId {
        // Delegate container/clip/scrollbar creation to ScrollBundle
        let extra_mask = components::STYLE | components::TEXT | components::TRANSFORM;
        let bundle = if let Some(physics) = self.physics {
            ScrollBundle::new_rc_with_physics(
                ctx,
                extra_mask,
                self.direction,
                self.scrollbar_width,
                physics,
            )
        } else {
            ScrollBundle::new_rc(ctx, extra_mask, self.direction, self.scrollbar_width)
        };
        let id = bundle.container_id;

        {
            let Some(el) = ctx.arena.get_mut(id) else {
                return id;
            };
            el.set_accessible_role(accesskit::Role::Group);

            // Initial scroll offset
            bundle.scroll_offset.set(self.initial_scroll);
            if self.initial_scroll.x != 0.0 || self.initial_scroll.y != 0.0 {
                crate::core::dirty_registry::spatial_update_scroll(
                    id,
                    self.initial_scroll.x,
                    self.initial_scroll.y,
                );
            }

            // Fixed size overrides
            if let Some(w) = self.fixed_width {
                if let Dimension::Pixels(px) = w {
                    el.set_preferred_width(Some(px));
                }
                if matches!(w, Dimension::Percent(_)) {
                    el.set_width_dim(Some(w));
                }
                el.set_flex_grow(0.0);
            }
            if let Some(h) = self.fixed_height {
                if let Dimension::Pixels(px) = h {
                    el.set_preferred_height(px);
                }
                if matches!(h, Dimension::Percent(_)) {
                    el.set_height_dim(h);
                }
            }

            // Deferred scroll-to-element
            if let Some(target) = self.scroll_to_target {
                el.set_pending_scroll_to(Rc::new(Cell::new(Some(target))));
            }
        }

        // Mount content inside bundle's clip element
        if let Some(child) = self.child {
            let mut child_ctx = ctx.child_with_events(bundle.clip_id);
            let child_id = child.mount_box(&mut child_ctx);
            ctx.arena.add_child(bundle.clip_id, child_id);
        }

        // Signal binding — bundle::bind_offset handles spatial_update_scroll etc.
        if let Some(sig) = self.bind_offset_signal {
            bundle.bind_offset(sig);
        }

        id
    }
}

impl Default for ScrollView {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for ScrollView {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ScrollView")
            .field("direction", &self.direction)
            .finish_non_exhaustive()
    }
}
