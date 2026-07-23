//! DevTools Demo — shows the `.devtools()` API.
//! Requires: `cargo run --example devtools_demo --features devtools`

use auralis_signal::Signal;
use burin::core::Compositor;
use burin::platform::{App, WindowConfig};
use burin::style::{Color, Styled};
use burin::theme::M3Theme;
use burin::widgets::display::Text;
use burin::widgets::input::Button;
use burin::widgets::layout::*;

fn main() {
    println!("[DEVTOOLS_DEMO] Starting...");

    let app = Compositor::new(|_scope| {
        let counter = Signal::new(0i32);
        let counter_label = Signal::new("Count: 0".to_string());
        let c2 = counter.clone();
        let cl2 = counter_label.clone();

        ScrollView::new().child(
            VStack::new()
                .gap(16.0)
                .padding(burin::style::Padding::all(24.0))
                .push(
                    Text::new("=== DevTools Demo ===")
                        .font_size(22.0)
                        .font_weight(700),
                )
                .push(
                    Text::new("DevTools window opens automatically via .devtools() API.")
                        .font_size(13.0)
                        .text_color(Color::rgba8(140, 140, 160, 255)),
                )
                .push(
                    HStack::new()
                        .gap(12.0)
                        .push(Button::new("Increment").on_click(move || {
                            let v = c2.read() + 1;
                            c2.set(v);
                            cl2.set(format!("Count: {}", v));
                        }))
                        .push(
                            Text::new(counter_label.read())
                                .font_size(16.0)
                                .bind(counter_label.clone()),
                        ),
                )
                .push(
                    Text::new("Click 'Increment' a few times — watch DevTools update live.")
                        .font_size(12.0)
                        .text_color(Color::rgba8(120, 120, 140, 255)),
                ),
        )
    });

    App::new()
        .window(
            WindowConfig {
                title: "DevTools Demo — Main Window".into(),
                width: 500.0,
                height: 360.0,
                theme: M3Theme::from_seed(Color::rgba8(0x67, 0x79, 0xE8, 0xFF))
                    .preset(burin::theme::PresetTheme::neo_minimal_slate()),
                ..Default::default()
            },
            app,
        )
        .devtools()
        .run()
        .expect("run");
}
