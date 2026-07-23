//! Full Component Gallery — CPU Backend (tiny-skia + softbuffer)
//! Delegates to examples/gallery/ for shared content.

#[path = "gallery/mod.rs"]
mod gallery;

use burin::platform::{App, WindowConfig};
use burin::render::RendererChoice;
use burin::theme::M3Theme;
use std::env;

fn main() {
    if env::var_os("AURALIS_CPU_PERF").is_none() {
        env::set_var("AURALIS_CPU_PERF", "1");
    }

    App::new()
        .window(
            WindowConfig {
                title: "Auralis UI — Full Component Gallery (CPU)".into(),
                width: 960.0,
                height: 900.0,
                theme: M3Theme::from_seed(burin::style::Color::rgba8(0x67, 0x79, 0xE8, 0xFF))
                    .preset(burin::theme::PresetTheme::neo_minimal_slate()),
                backend: RendererChoice::Cpu,
                ..Default::default()
            },
            gallery::app(),
        )
        .run()
        .expect("run");
}
