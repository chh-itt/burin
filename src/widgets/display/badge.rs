use std::cell::RefCell;
use std::rc::Rc;

use crate::core::config::EventHandler;
use crate::core::context::MountContext;
use crate::core::widget::Widget;
use crate::core::ElementId;
use crate::ecs::components;
use crate::render::wgpu::glyphon_bridge::create_buffer;
use crate::resource::icons::Icon as IconKind;
use crate::style::styled::{StyleRefinement, Styled};
use crate::style::Color;
use crate::style::TextAlign;
use crate::theme::m3::roles::{ComponentRole, DisplayRole, InteractiveRole};
use crate::theme::Intent;

/// A small label for status or counts.
pub struct Badge {
    label: String,
    background: Option<Color>,
    style: StyleRefinement,
}

impl Badge {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            background: None,
            style: StyleRefinement::default(),
        }
    }
    pub fn color(mut self, c: Color) -> Self {
        self.background = Some(c);
        self
    }
}

impl Styled for Badge {
    fn style_refinement(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl Widget for Badge {
    fn component_mask(&self) -> u64 {
        components::STYLE
            | components::LAYOUT
            | components::TRANSFORM
            | components::ACCESSIBLE
            | components::LIFECYCLE
    }

    fn mount_box(mut self: Box<Self>, ctx: &mut MountContext<'_>) -> ElementId {
        let role = ComponentRole::Display(DisplayRole::Badge {
            intent: Intent::Primary,
        });
        let b_resolved = match ctx.theme.resolve_component(&role) {
            crate::theme::m3::roles::ResolvedComponentStyle::Badge(s) => s,
            _ => unreachable!(),
        };
        let id = ctx.arena.allocate();
        ctx.preallocate(id, self.component_mask());
        {
            let Some(element) = ctx.arena.get_mut(id) else {
                return id;
            };
            let bg = self
                .background
                .or(self.style.background)
                .unwrap_or(b_resolved.background);
            let fg = self.style.text_color.unwrap_or(b_resolved.foreground);
            element.set_background(bg);
            element.set_foreground(fg);
            element.set_corner_radii(b_resolved.corner_radius);
            element.set_font_size(self.style.font_size.unwrap_or(b_resolved.font_size));
            let eff_fs = element.font_size();
            let eff_lh = element.line_height();
            element.set_preferred_height(b_resolved.height.max(eff_fs * eff_lh));
            element.set_affected_by_child_size(false);
            element.set_accessible_role(accesskit::Role::Status);
            element.set_accessible_label(self.label.clone());
            element.set_text_align(TextAlign::Center);
            let pref_w = crate::render::text::measure_text_width(
                &self.label,
                element.font_size(),
                element.font_weight(),
                element.font_family().map(|s| s.to_string()),
            )
            .max(element.font_size() * 2.0);
            element.set_preferred_width(Some(pref_w));
            let buf = Rc::new(RefCell::new(create_buffer(
                &self.label,
                element.font_size(),
                element.line_height(),
                element.font_weight(),
                element.font_family().as_deref(),
                Some(pref_w),
                element.text_align(),
            )));
            element.set_text_buffer(buf);
            if let Some(zi) = self.style.z_index {
                element.set_z_index(zi);
            }
            if let Some(o) = self.style.opacity {
                element.set_opacity(o);
            }
            if let Some(tx) = self.style.transform {
                element.set_transform(Some(tx));
            }
            self.style.height = Some(crate::style::Dimension::Pixels(
                b_resolved.height.max(eff_fs * eff_lh),
            ));
        }
        ctx.register_theme_component(
            id,
            &crate::theme::m3::roles::ResolvedComponentStyle::Badge(b_resolved.clone()),
            &role,
            &self.style,
        );
        id
    }
}

impl std::fmt::Debug for Badge {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Badge")
            .field("label", &self.label)
            .finish_non_exhaustive()
    }
}

// ── Chip ────

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ChipVariant {
    #[default]
    Assist,
    Filter,
    Input,
    Suggestion,
}

/// Interactive chip with optional leading icon.
pub struct Chip {
    label: String,
    variant: ChipVariant,
    icon: Option<IconKind>,
    selected: bool,
    on_click: Option<Box<dyn Fn()>>,
    style: StyleRefinement,
}

impl Chip {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            variant: ChipVariant::Assist,
            icon: None,
            selected: false,
            on_click: None,
            style: StyleRefinement::default(),
        }
    }
    pub fn variant(mut self, v: ChipVariant) -> Self {
        self.variant = v;
        self
    }
    pub fn icon(mut self, icon: IconKind) -> Self {
        self.icon = Some(icon);
        self
    }
    pub fn selected(mut self, s: bool) -> Self {
        self.selected = s;
        self
    }
    pub fn on_click(mut self, f: impl Fn() + 'static) -> Self {
        self.on_click = Some(Box::new(f));
        self
    }
}

impl Styled for Chip {
    fn style_refinement(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl Widget for Chip {
    fn component_mask(&self) -> u64 {
        components::STYLE
            | components::LAYOUT
            | components::TRANSFORM
            | components::ACCESSIBLE
            | components::LIFECYCLE
            | components::INTERACTION
    }

    fn mount_box(self: Box<Self>, ctx: &mut MountContext<'_>) -> ElementId {
        use crate::core::LayoutDirection;
        use crate::style::Padding;

        let theme = ctx.theme;
        let role = ComponentRole::Interactive(InteractiveRole::Chip {
            selected: self.selected,
        });
        let c_resolved = match theme.resolve_component(&role) {
            crate::theme::m3::roles::ResolvedComponentStyle::Chip(s) => s,
            _ => unreachable!(),
        };
        let id = ctx.arena.allocate();
        ctx.preallocate(id, self.component_mask());
        {
            let Some(element) = ctx.arena.get_mut(id) else {
                return id;
            };
            element.set_background(c_resolved.background);
            element.set_corner_radii(c_resolved.corner_radius);
            element.set_border_color(c_resolved.border_color);
            element.set_preferred_height(c_resolved.height);
            element.set_alignment(crate::style::Alignment::Center);
            element.set_affected_by_child_size(true);
            element.set_layout_direction(LayoutDirection::Horizontal);
            element.set_gap(self.style.gap.unwrap_or(8.0));
            element.set_padding(Padding::symmetric(c_resolved.padding_h, 0.0));
            element.set_accessible_role(accesskit::Role::Button);
            element.set_accessible_label(self.label.clone());
            element.set_focusable(true);
            element.set_cursor_icon(Some(crate::platform::CursorIcon::POINTER));

            if let Some(zi) = self.style.z_index {
                element.set_z_index(zi);
            }
            if let Some(o) = self.style.opacity {
                element.set_opacity(o);
            }
            if let Some(tx) = self.style.transform {
                element.set_transform(Some(tx));
            }
        }

        // Icon child
        if let Some(icon_kind) = self.icon {
            let mut child_ctx = ctx.child_with_events(id);
            let icon_size = self
                .style
                .font_size
                .unwrap_or(theme.typescale.label.small.size);
            let icon_widget = crate::widgets::display::Icon::new(icon_kind)
                .size(icon_size)
                .color(self.style.text_color.unwrap_or(c_resolved.foreground));
            let child_id = Box::new(icon_widget).mount_box(&mut child_ctx);
            ctx.arena.add_child(id, child_id);
        }

        // Text child
        {
            let mut child_ctx = ctx.child_with_events(id);
            let ts = self
                .style
                .font_size
                .unwrap_or(theme.typescale.label.small.size);
            let fg = self.style.text_color.unwrap_or(c_resolved.foreground);
            let text = crate::widgets::display::Text::new(&self.label)
                .font_size(ts)
                .text_color(fg);
            let child_id = Box::new(text).mount_box(&mut child_ctx);
            if let Some(el) = ctx.arena.get_mut(child_id) {
                let df = c_resolved.disabled.foreground;
                el.with_state_style(move |s| {
                    s.disabled.foreground = Some(df);
                });
            }
            ctx.arena.add_child(id, child_id);
        }

        // Click handler
        if let Some(handler) = self.on_click {
            let events = EventHandler::new().on_click(handler);
            if let Some(reg) = ctx.event_registry.as_mut() {
                events.register_all(reg, id);
            }
        }

        ctx.register_theme_component(
            id,
            &crate::theme::m3::roles::ResolvedComponentStyle::Chip(c_resolved.clone()),
            &role,
            &self.style,
        );

        id
    }
}

impl std::fmt::Debug for Chip {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Chip")
            .field("label", &self.label)
            .field("variant", &self.variant)
            .field("selected", &self.selected)
            .finish_non_exhaustive()
    }
}
