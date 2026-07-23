use std::rc::Rc;

use kurbo::BezPath;

use crate::core::context::MountContext;
use crate::core::widget::Widget;
use crate::core::ElementId;
use crate::ecs::components;
use crate::resource::icons::Icon as IconKind;
use crate::style::styled::{StyleRefinement, Styled};
use crate::style::{Brush, Color};
use crate::theme::m3::roles::{ComponentRole, DisplayRole};

/// Data stored in element user_data for path-based icon rendering.
pub struct IconPathData {
    pub path: Rc<BezPath>,
    pub brush: Brush,
    pub stroke: kurbo::Stroke,
}

pub struct Icon {
    icon: IconKind,
    style: StyleRefinement,
}

impl Icon {
    pub fn new(icon: IconKind) -> Self {
        Self {
            icon,
            style: StyleRefinement::default(),
        }
    }

    pub fn color(mut self, c: Color) -> Self {
        self.style.text_color = Some(c);
        self
    }
    pub fn size(mut self, s: f32) -> Self {
        self.style.font_size = Some(s);
        self
    }
    pub fn glyph(&self) -> &'static str {
        match self.icon {
            IconKind::Check => "\u{2713}",
            IconKind::X => "\u{2717}",
            IconKind::Plus => "+",
            IconKind::Minus => "\u{2212}",
            IconKind::Search => "\u{2315}",
            IconKind::ArrowRight => "\u{2192}",
            IconKind::ArrowLeft => "\u{2190}",
            IconKind::ArrowUp => "\u{2191}",
            IconKind::ArrowDown => "\u{2193}",
            IconKind::Home => "\u{2302}",
            IconKind::User => "\u{263A}",
            IconKind::Settings => "\u{2699}",
            IconKind::Folder => "\u{1F4C1}",
            IconKind::File => "\u{1F4C4}",
            IconKind::Image => "\u{1F5BC}",
            IconKind::Menu => "\u{2261}",
            IconKind::Refresh => "\u{21BB}",
            IconKind::Mail => "\u{2709}",
            IconKind::MessageCircle => "\u{25CB}",
            IconKind::Phone => "\u{260E}",
            IconKind::Link => "\u{1F517}",
            IconKind::Play => "\u{25B6}",
            IconKind::Pause => "\u{23F8}",
            IconKind::Volume => "\u{1F50A}",
            IconKind::Save => "\u{1F4BE}",
            IconKind::Delete => "\u{1F5D1}",
            IconKind::Edit => "\u{270E}",
            IconKind::Copy => "\u{1F4CB}",
            IconKind::Paste => "\u{1F4CB}",
            IconKind::Cut => "\u{2702}",
            IconKind::Undo => "\u{21B6}",
            IconKind::Redo => "\u{21B7}",
            IconKind::Filter => "\u{25BC}",
            IconKind::AlertCircle => "\u{26A0}",
            IconKind::Info => "\u{2139}",
            IconKind::Calendar => "\u{1F4C5}",
        }
    }
}

impl Styled for Icon {
    fn style_refinement(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl Widget for Icon {
    fn component_mask(&self) -> u64 {
        components::STYLE
            | components::LAYOUT
            | components::TRANSFORM
            | components::ACCESSIBLE
            | components::LIFECYCLE
    }

    fn mount_box(self: Box<Self>, ctx: &mut MountContext<'_>) -> ElementId {
        let role = ComponentRole::Display(DisplayRole::Icon);
        let i_resolved = match ctx.theme.resolve_component(&role) {
            crate::theme::m3::roles::ResolvedComponentStyle::Icon(s) => s,
            _ => unreachable!(),
        };
        let icon_size = self.style.font_size.unwrap_or(i_resolved.size);

        let id = ctx.arena.allocate();
        ctx.preallocate(id, self.component_mask());
        {
            let Some(element) = ctx.arena.get_mut(id) else {
                return id;
            };
            element.set_accessible_role(accesskit::Role::Image);
            element.set_accessible_label(format!("Icon({:?})", self.icon));
            element.set_affected_by_child_size(false);
            element.set_preferred_width(Some(icon_size));
            element.set_preferred_height(icon_size);

            let icon_color = self.style.text_color.unwrap_or(i_resolved.color);
            if let Some(path) = self.icon.build_path() {
                let brush = Brush::Solid(icon_color);
                let stroke = kurbo::Stroke {
                    width: 2.0,
                    start_cap: kurbo::Cap::Round,
                    end_cap: kurbo::Cap::Round,
                    join: kurbo::Join::Round,
                    ..Default::default()
                };
                element.insert_user_data(IconPathData {
                    path: Rc::new(path),
                    brush,
                    stroke,
                });
            }

            if let Some(bg) = self.style.background {
                element.set_background(bg);
            }
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
        ctx.register_theme_component(
            id,
            &crate::theme::m3::roles::ResolvedComponentStyle::Icon(i_resolved.clone()),
            &role,
            &self.style,
        );
        id
    }
}

impl std::fmt::Debug for Icon {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Icon")
            .field("icon", &self.icon)
            .finish_non_exhaustive()
    }
}
