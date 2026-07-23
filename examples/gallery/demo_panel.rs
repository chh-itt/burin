use auralis_signal::Signal;
use burin::core::{ElementId, MountContext, Widget};
use burin::style::{self, styled::Styled};
use burin::widgets::display::Text;
use burin::widgets::input::Checkbox;
use burin::widgets::layout::*;

pub struct DemoPanel {
    width: f32,
    items: Vec<DemoItem>,
}

pub enum DemoItem {
    Field {
        label: &'static str,
        value: Signal<String>,
    },
    Toggle {
        label: &'static str,
        value: Signal<bool>,
    },
    Info {
        label: &'static str,
        value: String,
    },
}

impl DemoPanel {
    pub fn new() -> Self {
        Self {
            width: 180.0,
            items: Vec::new(),
        }
    }
    #[allow(dead_code)]
    pub fn width(mut self, w: f32) -> Self {
        self.width = w;
        self
    }
    pub fn field(mut self, label: &'static str, value: Signal<String>) -> Self {
        self.items.push(DemoItem::Field { label, value });
        self
    }
    pub fn toggle(mut self, label: &'static str, value: Signal<bool>) -> Self {
        self.items.push(DemoItem::Toggle { label, value });
        self
    }
    pub fn info(mut self, label: &'static str, value: impl Into<String>) -> Self {
        self.items.push(DemoItem::Info {
            label,
            value: value.into(),
        });
        self
    }
}

impl Widget for DemoPanel {
    fn mount_box(self: Box<Self>, ctx: &mut MountContext<'_>) -> ElementId {
        let theme = ctx.theme;
        let id = ctx.arena.allocate();
        {
            let el = ctx.arena.get_mut(id).unwrap();
            el.set_background(theme.scheme.surface);
            el.set_corner_radius(6.0);
            el.set_border_width(1.0);
            el.set_border_color(theme.scheme.outline);
            el.set_preferred_width(Some(self.width));
            el.set_flex_grow(0.0);
            el.set_flex_shrink(0.0);
            el.set_padding(style::Padding::all(8.0));
            el.set_gap(6.0);
            el.set_font_size(12.0);
        }
        for item in self.items {
            let child_id = match item {
                DemoItem::Field { label, value } => {
                    let mut child_ctx = ctx.child_with_events(id);
                    let row = HStack::new()
                        .gap(4.0)
                        .push(Text::new(label).font_size(12.0))
                        .push(Expanded::new(
                            Text::new(value.read()).font_size(12.0).bind(value),
                        ));
                    Box::new(row).mount_box(&mut child_ctx)
                }
                DemoItem::Toggle { label, value } => {
                    let mut child_ctx = ctx.child_with_events(id);
                    let row = HStack::new()
                        .gap(4.0)
                        .push(Text::new(label).font_size(12.0))
                        .push(Checkbox::new(value));
                    Box::new(row).mount_box(&mut child_ctx)
                }
                DemoItem::Info { label, value } => {
                    let mut child_ctx = ctx.child_with_events(id);
                    let text = Text::new(format!("{}: {}", label, value)).font_size(12.0);
                    Box::new(text).mount_box(&mut child_ctx)
                }
            };
            ctx.arena.add_child(id, child_id);
        }
        id
    }
}
