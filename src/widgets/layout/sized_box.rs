use crate::core::context::MountContext;
use crate::core::widget::Widget;
use crate::core::ElementId;
use crate::ecs::components;
use crate::style::styled::{StyleRefinement, Styled};
use crate::style::Dimension;
use crate::widgets::layout::apply_style;
use accesskit::Role;

/// Constrain a child to a fixed width and height.
pub struct SizedBox {
    width: Dimension,
    height: Dimension,
    child: Option<Box<dyn Widget>>,
    style: StyleRefinement,
}

impl SizedBox {
    pub fn new() -> Self {
        Self {
            width: Dimension::Auto,
            height: Dimension::Auto,
            child: None,
            style: StyleRefinement::default(),
        }
    }

    pub fn width(mut self, w: impl Into<Dimension>) -> Self {
        self.width = w.into();
        self.style.width = Some(self.width);
        self
    }

    pub fn height(mut self, h: impl Into<Dimension>) -> Self {
        self.height = h.into();
        self.style.height = Some(self.height);
        self
    }

    pub fn child(mut self, child: impl Widget + 'static) -> Self {
        self.child = Some(Box::new(child));
        self
    }
}

impl Styled for SizedBox {
    fn style_refinement(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl Widget for SizedBox {
    fn component_mask(&self) -> u64 {
        components::STYLE | components::LAYOUT
    }

    fn mount_box(self: Box<Self>, ctx: &mut MountContext<'_>) -> ElementId {
        let id = ctx.arena.allocate();
        ctx.preallocate(id, self.component_mask());
        {
            let Some(element) = ctx.arena.get_mut(id) else {
                return id;
            };
            element.set_accessible_role(Role::GenericContainer);
            element.set_affected_by_child_size(false);
            apply_style(&self.style, element);
        }
        if let Some(child) = self.child {
            let mut child_ctx = ctx.child_with_events(id);
            let child_id = child.mount_box(&mut child_ctx);
            ctx.arena.add_child(id, child_id);
        }
        id
    }
}

impl Default for SizedBox {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for SizedBox {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SizedBox")
            .field("width", &self.width)
            .field("height", &self.height)
            .finish_non_exhaustive()
    }
}
