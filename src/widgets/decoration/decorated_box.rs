use crate::core::config::Overflow;
use crate::core::context::MountContext;
use crate::core::widget::Widget;
use crate::core::ElementId;
use crate::style::styled::{BlendMode, Shadow, StyleRefinement, Styled};
use crate::style::{Color, CornerRadii, LinearGradient};
use crate::widgets::layout::apply_style;

// ── BoxShape ──────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Debug, Default)]
/// The shape of a box decoration.
pub enum BoxShape {
    #[default]
    Rectangle,
    Circle,
}

// ── BoxDecoration ─────────────────────────────────────────────────

#[derive(Clone, Debug, Default)]
/// Visual properties for a box background, border, and shadow.
pub struct BoxDecoration {
    pub color: Option<Color>,
    pub gradient: Option<LinearGradient>,
    pub border: Option<(f32, Color)>,
    pub border_radius: Option<CornerRadii>,
    pub shadow: Option<Shadow>,
    pub shape: BoxShape,
    pub background_blend_mode: Option<BlendMode>,
}

impl BoxDecoration {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn color(mut self, c: impl Into<Color>) -> Self {
        self.color = Some(c.into());
        self
    }

    pub fn gradient(mut self, g: LinearGradient) -> Self {
        self.gradient = Some(g);
        self
    }

    pub fn border(mut self, width: f32, color: impl Into<Color>) -> Self {
        self.border = Some((width, color.into()));
        self
    }

    pub fn border_radius(mut self, r: impl Into<CornerRadii>) -> Self {
        self.border_radius = Some(r.into());
        self
    }

    pub fn shadow(
        mut self,
        color: impl Into<Color>,
        offset_x: f32,
        offset_y: f32,
        blur: f32,
    ) -> Self {
        self.shadow = Some(Shadow::new(color.into(), offset_x, offset_y, blur));
        self
    }

    pub fn shape(mut self, s: BoxShape) -> Self {
        self.shape = s;
        self
    }

    pub fn background_blend_mode(mut self, m: BlendMode) -> Self {
        self.background_blend_mode = Some(m);
        self
    }

    fn to_style(&self) -> StyleRefinement {
        let mut s = StyleRefinement::default();
        if let Some(c) = self.color {
            s.background = Some(c);
        }
        if let Some(g) = self.gradient {
            s.gradient = Some(g);
        }
        if let Some((w, c)) = self.border {
            s.border_width = Some(w);
            s.border_color = Some(c);
        }
        if let Some(r) = self.border_radius {
            s.corner_radius = Some(r);
        }
        if let Some(sh) = self.shadow {
            s.shadow = Some(sh);
        }
        if let Some(m) = self.background_blend_mode {
            s.blend_mode = Some(m);
        }
        s
    }
}

// ── DecoratedBox ──────────────────────────────────────────────────

/// A widget that paints a decoration behind its child.
pub struct DecoratedBox {
    decoration: Option<BoxDecoration>,
    clip_children: bool,
    child: Option<Box<dyn Widget>>,
    style: StyleRefinement,
}

impl DecoratedBox {
    pub fn new() -> Self {
        Self {
            decoration: None,
            clip_children: false,
            child: None,
            style: StyleRefinement::default(),
        }
    }

    pub fn decoration(mut self, d: BoxDecoration) -> Self {
        self.decoration = Some(d);
        self
    }

    pub fn clip_children(mut self, clip: bool) -> Self {
        self.clip_children = clip;
        self
    }

    pub fn child(mut self, widget: impl Widget + 'static) -> Self {
        self.child = Some(Box::new(widget));
        self
    }
}

impl Styled for DecoratedBox {
    fn style_refinement(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl Widget for DecoratedBox {
    fn mount_box(self: Box<Self>, ctx: &mut MountContext<'_>) -> ElementId {
        let id = ctx.arena.allocate();
        {
            let Some(element) = ctx.arena.get_mut(id) else {
                return id;
            };

            if let Some(ref dec) = self.decoration {
                apply_style(&dec.to_style(), element);

                if dec.shape == BoxShape::Circle {
                    element.set_corner_radii(CornerRadii::all(1e6));
                }
            }

            apply_style(&self.style, element);

            if self.clip_children {
                element.set_overflow(Overflow::Clip);
            }
        }

        if let Some(child) = self.child {
            let child_id = child.mount_box(&mut ctx.child_with_events(id));
            ctx.arena.add_child(id, child_id);
        }

        id
    }
}

impl Default for DecoratedBox {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for DecoratedBox {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DecoratedBox")
            .field("decoration", &self.decoration)
            .field("clip_children", &self.clip_children)
            .field("has_child", &self.child.is_some())
            .finish_non_exhaustive()
    }
}
