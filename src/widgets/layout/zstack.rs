use crate::core::context::MountContext;
use crate::core::widget::Widget;
use crate::core::ElementId;
use crate::ecs::components;
use crate::style::styled::{StyleRefinement, Styled};
use crate::widgets::layout::apply_style;

/// Stack children on top of each other (z-order by insertion).
///
/// Later children paint on top of earlier ones.  Use `.alignment()`
/// to control how children are positioned within the stack.
pub struct ZStack {
    children: Vec<Box<dyn Widget>>,
    style: StyleRefinement,
}

impl ZStack {
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

impl Styled for ZStack {
    fn style_refinement(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl Widget for ZStack {
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
            element.set_flex_grow(1.0);
            apply_style(&self.style, element);
        }
        // (audit 2026-07-18): ZStack previously laid children out in flow
        // (like a VStack). Each child is now absolutely-positioned via
        // z_index ≥ 1 (the same taffy bridge path used by portals), which
        // pulls them out of flex flow and places all children at the
        // container's origin — true z-index stacking. The container must
        // carry an explicit size (Sized-box, Center, SplitPane) or flex
        // into its parent; absolutely-positioned children do not contribute
        // to auto-sizing.
        for child in self.children {
            let mut child_ctx = ctx.child_with_events(id);
            let child_id = child.mount_box(&mut child_ctx);
            {
                let Some(el) = ctx.arena.get_mut(child_id) else {
                    return id;
                };
                // z_index ≥ 1 triggers taffy::Position::Absolute in
                // the bridge — the canonical mechanism for stacking
                // children in Z space.
                let existing_z = el.z_index.max(1);
                el.set_z_index(existing_z);
                el.set_preferred_width(Some(0.0));
                el.set_preferred_height(0.0);
                el.set_width_dim(Some(crate::style::Dimension::Percent(100.0)));
                el.set_height_dim(crate::style::Dimension::Percent(100.0));
            }
            ctx.arena.add_child(id, child_id);
        }
        id
    }
}

impl Default for ZStack {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for ZStack {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ZStack")
            .field("children", &self.children.len())
            .finish_non_exhaustive()
    }
}
