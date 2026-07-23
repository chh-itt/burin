//! Minimal CPU startup probe: a single Text widget, no gallery.
use burin::platform::{App, WindowConfig};
use burin::render::RendererChoice;
use burin::style::Color;
use burin::theme::M3Theme;
use burin::widgets::display::Text;
use burin::widgets::layout::Center;
use std::env;

fn main() {
    if env::var_os("AURALIS_CPU_PERF").is_none() {
        env::set_var("AURALIS_CPU_PERF", "1");
    }

    App::new()
        .window(
            WindowConfig {
                title: "minimal startup probe".into(),
                width: 400.0,
                height: 300.0,
                theme: M3Theme::from_seed(Color::rgba8(0x67, 0x79, 0xE8, 0xFF))
                    .preset(burin::theme::PresetTheme::neo_minimal_slate()),
                backend: RendererChoice::Cpu,
                ..Default::default()
            },
            Center::new(Text::new("Hello")),
        )
        .run()
        .expect("run");
}
