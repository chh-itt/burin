use crate::core::context::MountContext;
use crate::core::widget::Widget;
use crate::core::ElementId;
use crate::ecs::components;
use crate::style::Alignment;

/// Center a single child within the available space.
pub struct Center {
    child: Option<Box<dyn Widget>>,
}

impl Center {
    pub fn new(widget: impl Widget + 'static) -> Self {
        Self {
            child: Some(Box::new(widget)),
        }
    }
}

impl Widget for Center {
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
            element.set_layout_direction(crate::core::LayoutDirection::Vertical);
            element.set_alignment(Alignment::Center);
            element.set_content_align(Alignment::Center);
            element.set_flex_grow(1.0);
        }
        if let Some(child) = self.child.take() {
            let mut child_ctx = ctx.child_with_events(id);
            let child_id = child.mount_box(&mut child_ctx);
            ctx.arena.add_child(id, child_id);
        }
        id
    }
}

impl std::fmt::Debug for Center {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Center").finish_non_exhaustive()
    }
}
