use std::cell::Cell;
use std::rc::Rc;

use auralis_signal::Signal;

use crate::core::context::MountContext;
use crate::core::element::DirtyFlags;
use crate::core::widget::Widget;
use crate::core::ElementId;
use crate::ecs::components;
use crate::style::styled::{StyleRefinement, Styled};

pub struct Conditional {
    condition: Signal<bool>,
    true_widget: Option<Box<dyn Widget>>,
    false_widget: Option<Box<dyn Widget>>,
    style: StyleRefinement,
}

impl Conditional {
    pub fn new(
        condition: Signal<bool>,
        true_widget: impl Widget + 'static,
        false_widget: impl Widget + 'static,
    ) -> Self {
        Self {
            condition,
            true_widget: Some(Box::new(true_widget)),
            false_widget: Some(Box::new(false_widget)),
            style: StyleRefinement::default(),
        }
    }

    pub fn when(condition: Signal<bool>, widget: impl Widget + 'static) -> Self {
        Self {
            condition,
            true_widget: Some(Box::new(widget)),
            false_widget: None,
            style: StyleRefinement::default(),
        }
    }
}

impl Styled for Conditional {
    fn style_refinement(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl Widget for Conditional {
    fn component_mask(&self) -> u64 {
        components::LAYOUT
    }

    fn mount_box(self: Box<Self>, ctx: &mut MountContext<'_>) -> ElementId {
        let initial = self.condition.read();

        let id = ctx.arena.allocate();
        ctx.preallocate(id, self.component_mask());
        {
            let Some(container) = ctx.arena.get_mut(id) else {
                return id;
            };
            container.set_accessible_role(accesskit::Role::Group);
            container.set_layout_direction(crate::core::LayoutDirection::Vertical);
        }

        let mut slots: Vec<Rc<Cell<bool>>> = Vec::new();

        if let Some(true_w) = self.true_widget {
            let mut child_ctx = ctx.child_with_events(id);
            let child_id = true_w.mount_box(&mut child_ctx);
            {
                let Some(child) = ctx.arena.get_mut(child_id) else {
                    return id;
                };
                child.slot_inactive.set(!initial);
                slots.push(child.slot_inactive.clone());
            }
            ctx.arena.add_child(id, child_id);
        }

        if let Some(false_w) = self.false_widget {
            let mut child_ctx = ctx.child_with_events(id);
            let child_id = false_w.mount_box(&mut child_ctx);
            {
                let Some(child) = ctx.arena.get_mut(child_id) else {
                    return id;
                };
                child.slot_inactive.set(initial);
                slots.push(child.slot_inactive.clone());
            }
            ctx.arena.add_child(id, child_id);
        }

        let sig = self.condition.clone();
        let eid = id;
        let active: Rc<Cell<bool>> = Rc::new(Cell::new(initial));

        crate::core::signal_bridge::subscribe_owned(eid, &self.condition, move || {
            let v = sig.read();
            if active.get() == v {
                return;
            }
            active.set(v);

            for (i, slot) in slots.iter().enumerate() {
                let is_true_slot = i == 0;
                slot.set(is_true_slot != v);
            }

            crate::core::dirty_registry::mark_dirty(eid, DirtyFlags::MEASURE);
            crate::core::dirty_registry::register_dirty(eid, DirtyFlags::MEASURE);
            crate::core::dirty_registry::bump_subtree_gen(eid);
            crate::core::dirty_registry::mark_structurally_changed(eid);
        });

        id
    }
}

impl std::fmt::Debug for Conditional {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Conditional").finish_non_exhaustive()
    }
}
