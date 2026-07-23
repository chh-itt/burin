use crate::core::context::MountContext;
use crate::core::widget::Widget;
use crate::core::ElementId;
use crate::ecs::components;
use crate::style::Padding as PaddingStyle;

pub struct Padding {
    padding: PaddingStyle,
    child: Option<Box<dyn Widget>>,
}

impl Padding {
    pub fn new(padding: PaddingStyle, widget: impl Widget + 'static) -> Self {
        Self {
            padding,
            child: Some(Box::new(widget)),
        }
    }
}

impl Widget for Padding {
    fn component_mask(&self) -> u64 {
        components::LAYOUT
    }

    fn mount_box(mut self: Box<Self>, ctx: &mut MountContext<'_>) -> ElementId {
        let id = ctx.arena.allocate();
        ctx.preallocate(id, self.component_mask());
        {
            let Some(element) = ctx.arena.get_mut(id) else {
                return id;
            };
            element.set_accessible_role(accesskit::Role::GenericContainer);
            element.set_affected_by_child_size(false);
            element.set_flex_grow(1.0);
            element.set_flex_shrink(1.0);
            element.set_padding(self.padding);
        }
        if let Some(child) = self.child.take() {
            let mut child_ctx = ctx.child_with_events(id);
            let child_id = child.mount_box(&mut child_ctx);
            // Force child to fill the content area so padding is visible on all sides.
            if let Some(child_el) = ctx.arena.get_mut(child_id) {
                child_el.set_flex_grow(1.0);
                child_el.set_flex_shrink(1.0);
            }
            ctx.arena.add_child(id, child_id);
        }
        id
    }
}

impl std::fmt::Debug for Padding {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Padding")
            .field("padding", &self.padding)
            .finish_non_exhaustive()
    }
}
