use crate::core::context::MountContext;
use crate::core::widget::Widget;
use crate::core::ElementId;
use crate::ecs::components;
use crate::style::styled::{StyleRefinement, Styled};
use crate::widgets::layout::apply_style;

pub struct GridRow {
    columns: u32,
    gap: f32,
    auto_wrap: bool,
    auto_center: bool,
    children: Vec<GridItem>,
    style: StyleRefinement,
}

pub struct GridItem {
    cols: u32,
    offset: u32,
    child: Box<dyn Widget>,
}

impl GridItem {
    pub fn new(child: impl Widget + 'static) -> Self {
        Self {
            cols: 1,
            offset: 0,
            child: Box::new(child),
        }
    }
    pub fn cols(mut self, n: u32) -> Self {
        self.cols = n;
        self
    }
    pub fn offset(mut self, n: u32) -> Self {
        self.offset = n;
        self
    }
}

impl GridRow {
    pub fn new() -> Self {
        Self {
            columns: 24,
            gap: 0.0,
            auto_wrap: false,
            auto_center: false,
            children: Vec::new(),
            style: StyleRefinement::default(),
        }
    }
    pub fn columns(mut self, n: u32) -> Self {
        self.columns = n;
        self
    }
    pub fn gap(mut self, g: f32) -> Self {
        self.gap = g;
        self.style.gap = Some(g);
        self
    }
    pub fn auto_wrap(mut self) -> Self {
        self.auto_wrap = true;
        self
    }
    pub fn auto_center(mut self) -> Self {
        self.auto_center = true;
        self
    }
    pub fn push(mut self, item: GridItem) -> Self {
        self.children.push(item);
        self
    }
}

impl Styled for GridRow {
    fn style_refinement(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl Widget for GridRow {
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
            element.set_accessible_role(accesskit::Role::Grid);
            apply_style(&self.style, element);
            element.set_grid_columns(self.columns);
            element.set_affected_by_child_size(false);
            if self.auto_center {
                element.set_content_align(crate::style::Alignment::Center);
            }
        }

        for item in self.children {
            let mut child_ctx = ctx.child_with_events(id);
            let child_id = item.child.mount_box(&mut child_ctx);
            if item.cols > 1 || item.offset > 0 {
                let Some(child_el) = ctx.arena.get_mut(child_id) else {
                    return id;
                };
                child_el.set_grid_column_span(item.cols);
                child_el.set_grid_column_offset(item.offset);
            }
            ctx.arena.add_child(id, child_id);
        }

        id
    }
}

impl Default for GridRow {
    fn default() -> Self {
        Self::new()
    }
}
impl std::fmt::Debug for GridRow {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GridRow")
            .field("columns", &self.columns)
            .field("children", &self.children.len())
            .finish_non_exhaustive()
    }
}
