use std::rc::Rc;

use auralis_signal::Signal;

use crate::animation::Animation;
use crate::core::config::EventHandler;
use crate::core::context::MountContext;
use crate::core::element::ElementId;
use crate::core::widget::Widget;
use crate::ecs::components;
use crate::event::action::{ActionKind, ActionOutcome};
use crate::style::styled::{StyleRefinement, Styled};
use crate::style::Rect;
use crate::theme::m3::roles::{ComponentRole, DisplayRole, ResolvedComponentStyle};
use crate::widgets::shared::{mount_portal_popup, PortalPopupConfig};

// ── Positioning types ─────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum PopoverPosition {
    #[default]
    Bottom,
    Top,
    Left,
    Right,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CrossAxisAlignment {
    Start,
    Center,
    End,
}

impl CrossAxisAlignment {
    fn offset(&self, anchor_start: f32, anchor_size: f32, popover_size: f32) -> f32 {
        match self {
            Self::Start => anchor_start,
            Self::Center => anchor_start + (anchor_size - popover_size) * 0.5,
            Self::End => anchor_start + anchor_size - popover_size,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FlipAxes {
    Both,
    VerticalOnly,
    HorizontalOnly,
}

#[derive(Clone, Copy, Debug)]
pub struct PopoverPlacement {
    pub preferred_position: PopoverPosition,
    pub alignment: CrossAxisAlignment,
    pub gap: f32,
    pub viewport_margin: f32,
    pub auto_flip: bool,
    pub flip_axes: FlipAxes,
    pub min_width: Option<f32>,
    pub max_width: Option<f32>,
}

impl Default for PopoverPlacement {
    fn default() -> Self {
        Self {
            preferred_position: PopoverPosition::Bottom,
            alignment: CrossAxisAlignment::Start,
            gap: 4.0,
            viewport_margin: 8.0,
            auto_flip: true,
            flip_axes: FlipAxes::Both,
            min_width: None,
            max_width: None,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PopoverGeometry {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub actual_position: PopoverPosition,
}

// ── Positioning engine ────────────────────────────────────────────

fn fits_viewport(bounds: Rect, viewport: Rect, margin: f32) -> bool {
    bounds.x >= viewport.x + margin
        && bounds.y >= viewport.y + margin
        && bounds.x + bounds.width <= viewport.x + viewport.width - margin
        && bounds.y + bounds.height <= viewport.y + viewport.height - margin
}

fn candidate_geometry(
    pos: PopoverPosition,
    anchor: Rect,
    content_w: f32,
    content_h: f32,
    placement: &PopoverPlacement,
) -> PopoverGeometry {
    let w = content_w.max(placement.min_width.unwrap_or(0.0));
    let w = if let Some(max_w) = placement.max_width {
        w.min(max_w)
    } else {
        w
    };
    let h = content_h;

    match pos {
        PopoverPosition::Bottom => {
            let x = placement.alignment.offset(anchor.x, anchor.width, w);
            let y = anchor.y + anchor.height + placement.gap;
            PopoverGeometry {
                x,
                y,
                width: w,
                height: h,
                actual_position: PopoverPosition::Bottom,
            }
        }
        PopoverPosition::Top => {
            let x = placement.alignment.offset(anchor.x, anchor.width, w);
            let y = anchor.y - h - placement.gap;
            PopoverGeometry {
                x,
                y,
                width: w,
                height: h,
                actual_position: PopoverPosition::Top,
            }
        }
        PopoverPosition::Right => {
            let x = anchor.x + anchor.width + placement.gap;
            let y = placement.alignment.offset(anchor.y, anchor.height, h);
            PopoverGeometry {
                x,
                y,
                width: w,
                height: h,
                actual_position: PopoverPosition::Right,
            }
        }
        PopoverPosition::Left => {
            let x = anchor.x - w - placement.gap;
            let y = placement.alignment.offset(anchor.y, anchor.height, h);
            PopoverGeometry {
                x,
                y,
                width: w,
                height: h,
                actual_position: PopoverPosition::Left,
            }
        }
    }
}

fn priority_order(preferred: PopoverPosition, flip_axes: FlipAxes) -> Vec<PopoverPosition> {
    match flip_axes {
        FlipAxes::Both => match preferred {
            PopoverPosition::Bottom => vec![
                PopoverPosition::Bottom,
                PopoverPosition::Top,
                PopoverPosition::Right,
                PopoverPosition::Left,
            ],
            PopoverPosition::Top => vec![
                PopoverPosition::Top,
                PopoverPosition::Bottom,
                PopoverPosition::Right,
                PopoverPosition::Left,
            ],
            PopoverPosition::Right => vec![
                PopoverPosition::Right,
                PopoverPosition::Left,
                PopoverPosition::Bottom,
                PopoverPosition::Top,
            ],
            PopoverPosition::Left => vec![
                PopoverPosition::Left,
                PopoverPosition::Right,
                PopoverPosition::Bottom,
                PopoverPosition::Top,
            ],
        },
        FlipAxes::VerticalOnly => match preferred {
            PopoverPosition::Bottom => vec![PopoverPosition::Bottom, PopoverPosition::Top],
            PopoverPosition::Top => vec![PopoverPosition::Top, PopoverPosition::Bottom],
            _ => vec![preferred],
        },
        FlipAxes::HorizontalOnly => match preferred {
            PopoverPosition::Right => vec![PopoverPosition::Right, PopoverPosition::Left],
            PopoverPosition::Left => vec![PopoverPosition::Left, PopoverPosition::Right],
            _ => vec![preferred],
        },
    }
}

fn clamp_to_viewport(geo: PopoverGeometry, viewport: Rect, margin: f32) -> PopoverGeometry {
    let mut g = geo;
    let min_x = viewport.x + margin;
    let max_x = viewport.x + viewport.width - margin;
    let min_y = viewport.y + margin;
    let max_y = viewport.y + viewport.height - margin;
    if g.x < min_x {
        g.x = min_x;
    }
    if g.y < min_y {
        g.y = min_y;
    }
    if g.x + g.width > max_x {
        g.x = (max_x - g.width).max(min_x);
    }
    if g.y + g.height > max_y {
        g.y = (max_y - g.height).max(min_y);
    }
    g
}

pub fn compute_popover_geometry(
    anchor: Rect,
    viewport: Rect,
    content_w: f32,
    content_h: f32,
    placement: PopoverPlacement,
) -> PopoverGeometry {
    let order = if placement.auto_flip {
        priority_order(placement.preferred_position, placement.flip_axes)
    } else {
        vec![placement.preferred_position]
    };

    for &pos in &order {
        let geo = candidate_geometry(pos, anchor, content_w, content_h, &placement);
        let bounds = Rect::new(geo.x, geo.y, geo.width, geo.height);
        if fits_viewport(bounds, viewport, placement.viewport_margin) {
            return geo;
        }
    }

    let fallback = candidate_geometry(
        placement.preferred_position,
        anchor,
        content_w,
        content_h,
        &placement,
    );
    clamp_to_viewport(fallback, viewport, placement.viewport_margin)
}

// ── Popover widget ────────────────────────────────────────────────

const DEFAULT_POPOVER_WIDTH: f32 = 220.0;
const DEFAULT_MAX_WIDTH: f32 = 400.0;
const DEFAULT_CONTENT_HEIGHT: f32 = 0.0;

pub struct Popover {
    child: Option<Box<dyn Widget>>,
    content: Option<Box<dyn Widget>>,
    is_open: Signal<bool>,
    placement: PopoverPlacement,
    close_on_outside: bool,
    animate: Option<Animation>,
    on_dismiss: Option<Rc<dyn Fn()>>,
    style: StyleRefinement,
}

impl Popover {
    pub fn new(
        is_open: Signal<bool>,
        child: impl Widget + 'static,
        content: impl Widget + 'static,
    ) -> Self {
        Self {
            child: Some(Box::new(child)),
            content: Some(Box::new(content)),
            is_open,
            placement: PopoverPlacement {
                min_width: Some(DEFAULT_POPOVER_WIDTH),
                max_width: Some(DEFAULT_MAX_WIDTH),
                ..PopoverPlacement::default()
            },
            close_on_outside: true,
            animate: None,
            on_dismiss: None,
            style: StyleRefinement::default(),
        }
    }

    pub fn position(mut self, pos: PopoverPosition) -> Self {
        self.placement.preferred_position = pos;
        self
    }

    pub fn placement(mut self, placement: PopoverPlacement) -> Self {
        self.placement = placement;
        self
    }

    pub fn alignment(mut self, align: CrossAxisAlignment) -> Self {
        self.placement.alignment = align;
        self
    }

    pub fn gap(mut self, px: f32) -> Self {
        self.placement.gap = px;
        self
    }

    pub fn min_width(mut self, px: f32) -> Self {
        self.placement.min_width = Some(px);
        self
    }

    pub fn max_width(mut self, px: f32) -> Self {
        self.placement.max_width = Some(px);
        self
    }

    pub fn close_on_outside_click(mut self, v: bool) -> Self {
        self.close_on_outside = v;
        self
    }

    pub fn animation(mut self, anim: Animation) -> Self {
        self.animate = Some(anim);
        self
    }

    pub fn on_dismiss(mut self, f: impl Fn() + 'static) -> Self {
        self.on_dismiss = Some(Rc::new(f));
        self
    }
}

impl Styled for Popover {
    fn style_refinement(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl Widget for Popover {
    fn mount_box(mut self: Box<Self>, ctx: &mut MountContext<'_>) -> ElementId {
        let theme = ctx.theme;
        let _placement = self.placement;

        let popover_role = ComponentRole::Display(DisplayRole::Popover);
        let popover_resolved = theme.resolve_component(&popover_role);
        let ps = match &popover_resolved {
            ResolvedComponentStyle::Popover(s) => s.clone(),
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
        {
            let Some(el) = ctx.arena.get_mut(anchor_id) else {
                return id;
            };
            el.set_flex_shrink(0.0);
        }

        // ── Portals overlay ──
        let content_id = mount_portal_popup(
            &mut ctx.child_with_events(id),
            PortalPopupConfig {
                open: self.is_open.clone(),
                anchor_id,
                placement: self.placement,
                z_index: theme.z_index.dropdown,
                modal: true,
                dismiss_on_outside: self.close_on_outside,
                visible_height: DEFAULT_CONTENT_HEIGHT,
                animate: self.animate,
                on_open: None,
                on_close: self.on_dismiss.clone(),
                background: None,
                border_color: None,
                border_width: None,
                corner_radius: None,
                shadow: None,
                padding: None,
            },
            StyledPortalContent {
                inner: self.content.take(),
                bg: ps.background,
                border_color: ps.border_color,
                corner_radii: ps.corner_radius,
                shadow: ps.shadow,
                padding: crate::style::Padding::all(8.0),
            },
        );

        // ── Autofocus first focusable child ──
        if let Some(content_child) = ctx
            .arena
            .get(content_id)
            .and_then(|el| el.children.first().copied())
        {
            let children = ctx
                .arena
                .get(content_child)
                .map(|el| el.children.clone())
                .unwrap_or_default();
            for &child_id in &children {
                if ctx.arena.get(child_id).is_some_and(|el| el.is_focusable()) {
                    if let Some(reg) = ctx.event_registry.as_mut() {
                        reg.request_autofocus(child_id);
                    }
                    break;
                }
            }
        }

        // ── Keyboard: Escape dismiss ──
        {
            let vis_cancel = self.is_open.clone();
            let events = EventHandler::new().on_action(move |action| {
                if action.kind == ActionKind::Cancel {
                    vis_cancel.set(false);
                    ActionOutcome::Consumed
                } else {
                    ActionOutcome::Unhandled
                }
            });
            if let Some(reg) = ctx.event_registry.as_mut() {
                events.register_all(reg, content_id);
            }
        }

        id
    }
}

struct StyledPortalContent {
    inner: Option<Box<dyn Widget>>,
    bg: crate::style::Color,
    border_color: crate::style::Color,
    corner_radii: crate::style::CornerRadii,
    shadow: Option<crate::style::styled::Shadow>,
    padding: crate::style::Padding,
}

impl Widget for StyledPortalContent {
    fn mount_box(mut self: Box<Self>, ctx: &mut MountContext<'_>) -> ElementId {
        let id = ctx.arena.allocate();
        ctx.preallocate(id, components::STYLE | components::LAYOUT);
        {
            let Some(el) = ctx.arena.get_mut(id) else {
                return id;
            };
            el.set_background(self.bg);
            el.set_border_width(1.0);
            el.set_border_color(self.border_color);
            el.set_corner_radii(self.corner_radii);
            if let Some(ref s) = self.shadow {
                el.set_shadow(Some(*s));
            }
            el.set_padding(self.padding);
            el.set_flex_shrink(0.0);
            el.set_layout_direction(crate::core::LayoutDirection::Vertical);
        }
        let inner_id = self
            .inner
            .take()
            .unwrap()
            .mount_box(&mut ctx.child_with_events(id));
        ctx.arena.add_child(id, inner_id);
        id
    }
}

impl std::fmt::Debug for Popover {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Popover")
            .field("placement", &self.placement)
            .finish_non_exhaustive()
    }
}
