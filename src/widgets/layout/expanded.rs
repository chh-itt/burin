use crate::core::context::MountContext;
use crate::core::widget::Widget;
use crate::core::ElementId;
use crate::ecs::components;

pub struct Expanded {
    child: Option<Box<dyn Widget>>,
}

impl Expanded {
    pub fn new(child: impl Widget + 'static) -> Self {
        Self {
            child: Some(Box::new(child)),
        }
    }
}

impl Widget for Expanded {
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
            element.set_flex_grow(1.0);
            element.set_flex_shrink(1.0);
        }
        if let Some(child) = self.child.take() {
            let mut child_ctx = ctx.child_with_events(id);
            let child_id = child.mount_box(&mut child_ctx);
            ctx.arena.add_child(id, child_id);
        }
        id
    }
}

impl std::fmt::Debug for Expanded {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Expanded").finish_non_exhaustive()
    }
}
