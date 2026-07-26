use crate::core::context::MountContext;
use crate::core::widget::Widget;
use crate::core::ElementId;
use crate::ecs::components;
use crate::style::styled::{StyleRefinement, Styled};
use crate::widgets::layout::apply_style;

/// A horizontal stack of widgets with configurable gap and alignment.
///
/// Children are laid out left to right.  Use `.push()` to add children
/// and `.gap()` to set spacing between them.
pub struct HStack {
    children: Vec<Box<dyn Widget>>,
    style: StyleRefinement,
}

impl HStack {
    pub fn new() -> Self {
        Self {
            children: Vec::new(),
            style: StyleRefinement::default(),
        }
    }

    pub fn push(mut self, child: impl Widget + 'static) -> Self {
        self.children.push(Box::new(child));
        self
    }
}

impl Styled for HStack {
    fn style_refinement(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl Widget for HStack {
    fn component_mask(&self) -> u64 {
        components::STYLE | components::LAYOUT | components::TEXT | components::TRANSFORM
    }

    fn mount_box(self: Box<Self>, ctx: &mut MountContext<'_>) -> ElementId {
        let id = ctx.arena.allocate();
        ctx.preallocate(id, self.component_mask());
        {
            let Some(element) = ctx.arena.get_mut(id) else {
                return id;
            };
            element.set_accessible_role(accesskit::Role::Group);
            element.set_layout_direction(crate::core::LayoutDirection::Horizontal);
            apply_style(&self.style, element);
        }
        for child in self.children {
            let mut child_ctx = ctx.child_with_events(id);
            let child_id = child.mount_box(&mut child_ctx);
            ctx.arena.add_child(id, child_id);
        }
        id
    }
}

impl Default for HStack {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for HStack {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HStack")
            .field("children", &self.children.len())
            .finish_non_exhaustive()
    }
}
