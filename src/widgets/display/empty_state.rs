//! EmptyState — placeholder for empty lists, search results, etc.
use crate::core::context::MountContext;
use crate::core::widget::Widget;
use crate::core::ElementId;
use crate::style::styled::Styled;
use crate::widgets::display::{Icon, Text};
use crate::widgets::layout::{Center, VStack};

pub struct EmptyState {
    icon: Option<Icon>,
    title: String,
    description: String,
    action: Option<Box<dyn Widget>>,
}

impl EmptyState {
    pub fn new() -> Self {
        Self {
            icon: None,
            title: String::new(),
            description: String::new(),
            action: None,
        }
    }

    pub fn icon(mut self, icon: Icon) -> Self {
        self.icon = Some(icon);
        self
    }
    pub fn title(mut self, t: impl Into<String>) -> Self {
        self.title = t.into();
        self
    }
    pub fn description(mut self, d: impl Into<String>) -> Self {
        self.description = d.into();
        self
    }
    pub fn action(mut self, widget: impl Widget + 'static) -> Self {
        self.action = Some(Box::new(widget));
        self
    }
}

impl Widget for EmptyState {
    fn mount_box(self: Box<Self>, ctx: &mut MountContext<'_>) -> ElementId {
        let theme = ctx.theme;

        let mut body = VStack::new().gap(8.0);

        if let Some(icon) = self.icon {
            body = body.push(icon.color(theme.scheme.on_surface_variant).size(48.0));
        }

        if !self.title.is_empty() {
            body = body.push(
                Text::new(self.title)
                    .font_size(theme.typescale.title.medium.size)
                    .font_weight(600),
            );
        }

        if !self.description.is_empty() {
            body =
                body.push(Text::new(self.description).font_size(theme.typescale.body.medium.size));
        }

        if let Some(widget) = self.action {
            body = body.push(BoxedWidget(widget));
        }

        Box::new(Center::new(body)).mount_box(ctx)
    }
}

impl Default for EmptyState {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for EmptyState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EmptyState").finish_non_exhaustive()
    }
}

struct BoxedWidget(Box<dyn Widget>);

impl Widget for BoxedWidget {
    fn mount_box(self: Box<Self>, ctx: &mut MountContext<'_>) -> ElementId {
        self.0.mount_box(ctx)
    }
}
