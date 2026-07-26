use crate::core::context::MountContext;
use crate::core::widget::Widget;
use crate::core::ElementId;
use crate::ecs::components;

/// An empty widget that takes available flex space.
pub struct Spacer;

impl Spacer {
    pub fn new() -> Self {
        Self
    }
}

impl Widget for Spacer {
    fn component_mask(&self) -> u64 {
        components::LAYOUT
    }

    fn mount_box(self: Box<Self>, ctx: &mut MountContext<'_>) -> ElementId {
        let id = ctx.arena.allocate();
        ctx.preallocate(id, self.component_mask());
        {
            let Some(el) = ctx.arena.get_mut(id) else {
                return id;
            };
            el.set_flex_grow(1.0);
            el.set_flex_shrink(1.0);
        }
        id
    }
}

impl Default for Spacer {
    fn default() -> Self {
        Self
    }
}

impl std::fmt::Debug for Spacer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Spacer").finish()
    }
}
