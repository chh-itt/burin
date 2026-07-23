//! Drag & Drop Demo — demonstrates widget-to-widget drag-and-drop.
//!
//! Drag any colored chip from the left onto the drop zone on the right.
//!   cargo run --example drag_drop_demo

use auralis_signal::Signal;
use burin::core::config::{ElementBuilder, InteractionConfig, LayoutConfig, PaintConfig};
use burin::core::context::MountContext;
use burin::core::element::ElementId;
use burin::core::widget::StaticWidget;
use burin::core::{Compositor, Widget};
use burin::event::{DragData, DragKind, DropType};
use burin::platform::{App, WindowConfig};
use burin::render::wgpu::glyphon_bridge::create_buffer;
use burin::style::{Color, CornerRadii, Dimension, Padding, Styled};
use burin::theme::M3Theme;
use burin::widgets::display::Text;
use burin::widgets::layout::Padding as PadW;
use burin::widgets::layout::ScrollView;
use burin::widgets::layout::{HStack, VStack};

fn main() {
    App::new()
        .window(
            WindowConfig {
                title: "Auralis UI — Drag & Drop Demo".into(),
                width: 700.0,
                height: 460.0,
                theme: M3Theme::from_seed(Color::rgba8(0x67, 0x79, 0xE8, 0xFF))
                    .preset(burin::theme::PresetTheme::neo_minimal_slate()),
                ..Default::default()
            },
            app(),
        )
        .run()
        .expect("run");
}

/// A small colored chip that can be dragged.
struct Chip {
    label: String,
    color: Color,
}

impl Chip {
    fn new(label: impl Into<String>, color: Color) -> Self {
        Self {
            label: label.into(),
            color,
        }
    }
}

impl StaticWidget for Chip {
    fn mount_static(self: Box<Self>, ctx: &mut MountContext<'_>) -> ElementId {
        let bg = self.color;
        let fg = Color::auto_fg(&bg);
        let id = ElementBuilder::new()
            .layout(LayoutConfig {
                width: Dimension::Pixels(140.0),
                height: Dimension::Pixels(36.0),
                padding: Padding::all(10.0),
                ..LayoutConfig::default()
            })
            .interaction(InteractionConfig {
                enabled: true,
                focusable: false,
                cursor: burin::platform::CursorIcon::POINTER,
                draggable: true,
                drag_data: Some(DragData {
                    kind: DragKind::Text,
                    text: Some(self.label.clone()),
                    ..Default::default()
                }),
                ..InteractionConfig::default()
            })
            .paint(PaintConfig {
                background: Some(bg),
                foreground: Some(fg),
                corner_radius: CornerRadii::all(8.0),
                font_size: 14.0,
                ..PaintConfig::default()
            })
            .accessibility(accesskit::Role::Button, self.label.clone())
            .build(ctx);
        let el = ctx.arena.get_mut(id).unwrap();
        let buf = create_buffer(
            &self.label,
            14.0,
            1.3,
            500,
            None,
            None,
            burin::style::TextAlign::Center,
        );
        el.set_text_buffer(std::rc::Rc::new(std::cell::RefCell::new(buf)));
        el.set_text_generation(std::rc::Rc::new(std::cell::Cell::new(1u64)));
        id
    }
}

/// Wrapper: mounts inner widget, then attaches a drop handler to the container.
struct DropTarget<W: Widget + 'static> {
    inner: W,
    on_drop: Box<dyn Fn(DragData)>,
}

impl<W: Widget + 'static> Widget for DropTarget<W> {
    fn mount_box(self: Box<Self>, ctx: &mut MountContext<'_>) -> ElementId {
        let theme = ctx.theme;
        let container = ElementBuilder::new()
            .layout(LayoutConfig {
                padding: Padding::all(20.0),
                ..LayoutConfig::default()
            })
            .interaction(InteractionConfig {
                drop_target: true,
                accept_drop_types: vec![
                    DropType::Text,
                    DropType::Files,
                    DropType::Custom(String::new()),
                ],
                ..InteractionConfig::default()
            })
            .paint(PaintConfig {
                background: Some(theme.scheme.surface),
                border_width: 2.0,
                border_color: Some(theme.scheme.outline),
                corner_radius: CornerRadii::all(12.0),
                ..PaintConfig::default()
            })
            .accessibility(accesskit::Role::Group, "drop-target")
            .build(ctx);
        ctx.arena
            .get_mut(container)
            .unwrap()
            .set_on_drop(self.on_drop);
        let inner_id = Box::new(self.inner).mount_box(ctx);
        ctx.arena.add_child(container, inner_id);
        container
    }
}

fn app() -> impl Widget {
    Compositor::new(|_scope| {
        let dropped = Signal::new(String::from("Drop items here"));
        let count = Signal::new(0u32);
        let count_text = Signal::new(String::from("Dropped: 0 times"));

        let d = dropped.clone();
        let c = count.clone();
        let ct = count_text.clone();
        let target = DropTarget {
            inner: VStack::new()
                .gap(8.0)
                .push(Text::new("Drop Zone").font_size(15.0).font_weight(700))
                .push(Text::new("Drop items here").bind(d.clone()).font_size(14.0))
                .push(
                    Text::new("Dropped: 0 times")
                        .bind(ct.clone())
                        .font_size(12.0),
                ),
            on_drop: Box::new(move |data: DragData| {
                let label = data.text.unwrap_or_else(|| "?".into());
                d.set(format!("Last dropped: {}", label));
                let n = c.read() + 1;
                c.set(n);
                ct.set(format!("Dropped: {} times", n));
            }),
        };

        HStack::new()
            .gap(0.0)
            .push(PadW::new(
                Padding::all(16.0),
                ScrollView::new().child(
                    VStack::new()
                        .gap(8.0)
                        .push(Text::new("Drag items").font_size(15.0).font_weight(700))
                        .push(Chip::new("Apple", Color::rgba8(248, 113, 113, 255)))
                        .push(Chip::new("Lemon", Color::rgba8(250, 204, 21, 255)))
                        .push(Chip::new("Grape", Color::rgba8(192, 132, 252, 255)))
                        .push(Chip::new("Kiwi", Color::rgba8(134, 239, 172, 255)))
                        .push(Chip::new("Berry", Color::rgba8(96, 165, 250, 255))),
                ),
            ))
            .push(PadW::new(Padding::all(16.0), target))
    })
}
