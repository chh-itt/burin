use crate::core::context::MountContext;
use crate::core::widget::Widget;
use crate::core::ElementId;
use crate::ecs::components;

/// A widget that applies an opacity multiplier to its child.
pub struct Opacity {
    opacity: f32,
    child: Option<Box<dyn Widget>>,
}

impl Opacity {
    pub fn new(opacity: f32, widget: impl Widget + 'static) -> Self {
        Self {
            opacity: opacity.clamp(0.0, 1.0),
            child: Some(Box::new(widget)),
        }
    }
}

impl Widget for Opacity {
    fn component_mask(&self) -> u64 {
        components::STYLE
    }

    fn mount_box(mut self: Box<Self>, ctx: &mut MountContext<'_>) -> ElementId {
        let id = ctx.arena.allocate();
        ctx.preallocate(id, self.component_mask());
        {
            let Some(element) = ctx.arena.get_mut(id) else {
                return id;
            };
            element.set_accessible_role(accesskit::Role::GenericContainer);
            element.set_opacity(self.opacity);
        }
        if let Some(child) = self.child.take() {
            let mut child_ctx = ctx.child_with_events(id);
            let child_id = child.mount_box(&mut child_ctx);
            ctx.arena.add_child(id, child_id);
        }
        id
    }
}

impl std::fmt::Debug for Opacity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Opacity")
            .field("opacity", &self.opacity)
            .finish()
    }
}
