use auralis_signal::Signal;

use crate::core::context::MountContext;
use crate::core::widget::Widget;
use crate::core::ElementId;
use crate::ecs::components;
use crate::style::styled::{StyleRefinement, Styled};
use crate::style::{Color, Dimension};
use crate::widgets::display::Text;
use crate::widgets::layout::{HStack, VStack};

/// A read-only property grid with labeled key-value rows.
pub struct PropertyGrid {
    sections: Vec<PropertySection>,
    label_width: f32,
    style: StyleRefinement,
}

/// A titled section within a property grid.
pub struct PropertySection {
    pub title: String,
    pub rows: Vec<PropertyRow>,
}

/// A single label-value pair in a property grid.
pub struct PropertyRow {
    pub label: String,
    pub value: Signal<String>,
    pub value_color: Option<Color>,
}

impl PropertyGrid {
    pub fn new() -> Self {
        Self {
            sections: Vec::new(),
            label_width: 120.0,
            style: StyleRefinement::default(),
        }
    }

    pub fn section(mut self, title: impl Into<String>, rows: Vec<PropertyRow>) -> Self {
        self.sections.push(PropertySection {
            title: title.into(),
            rows,
        });
        self
    }

    pub fn label_width(mut self, w: f32) -> Self {
        self.label_width = w;
        self
    }
}

impl Styled for PropertyGrid {
    fn style_refinement(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl Widget for PropertyGrid {
    fn component_mask(&self) -> u64 {
        components::STYLE | components::LAYOUT | components::LIFECYCLE
    }

    fn mount_box(mut self: Box<Self>, ctx: &mut MountContext<'_>) -> ElementId {
        let id = ctx.arena.allocate();
        ctx.preallocate(id, self.component_mask());
        {
            let Some(el) = ctx.arena.get_mut(id) else {
                return id;
            };
            el.set_accessible_role(accesskit::Role::Group);
            el.set_layout_direction(crate::core::LayoutDirection::Vertical);
            el.set_gap(12.0);
            el.set_padding(crate::style::Padding::all(8.0));
            if let Some(g) = self.style.gap {
                el.set_gap(g);
            }
            if let Some(p) = self.style.padding {
                el.set_padding(p);
            }
            if let Some(w) = self.style.width {
                if let Dimension::Pixels(px) = w {
                    el.set_preferred_width(Some(px));
                }
            }
        }

        let sections = std::mem::take(&mut self.sections);
        let lw = self.label_width;

        for section in sections {
            let mut section_vstack = VStack::new().gap(4.0);

            section_vstack = section_vstack.push(
                Text::new(&section.title)
                    .font_size(12.0)
                    .font_weight(700)
                    .text_color(Color::rgba8(140, 140, 160, 255)),
            );

            for row in &section.rows {
                let mut row_hstack = HStack::new().gap(8.0);
                row_hstack = row_hstack.push(
                    Text::new(&row.label)
                        .font_size(12.0)
                        .text_color(Color::rgba8(160, 160, 180, 255))
                        .width(lw),
                );

                let value_sig = row.value.clone();
                let mut value_text = Text::new(value_sig.read())
                    .font_size(12.0)
                    .font_weight(500)
                    .bind(value_sig);
                if let Some(c) = row.value_color {
                    value_text = value_text.text_color(c);
                }
                row_hstack = row_hstack.push(value_text);

                section_vstack = section_vstack.push(row_hstack);
            }

            let child_id = Box::new(section_vstack).mount_box(&mut ctx.child_with_events(id));
            ctx.arena.add_child(id, child_id);
        }

        id
    }
}
