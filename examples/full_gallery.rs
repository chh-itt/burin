//! Full Component Gallery — GPU Backend
//! Delegates to examples/gallery/ for shared content.

#[path = "gallery/mod.rs"]
mod gallery;

use burin::platform::{App, WindowConfig};
use burin::theme::M3Theme;

fn main() {
    App::new()
        .window(
            WindowConfig {
                title: "Auralis UI — Full Component Gallery (GPU)".into(),
                width: 960.0,
                height: 900.0,
                theme: M3Theme::from_seed(burin::style::Color::rgba8(0x67, 0x79, 0xE8, 0xFF))
                    .preset(burin::theme::PresetTheme::neo_minimal_slate()),
                ..Default::default()
            },
            gallery::app(),
        )
        .run()
        .expect("run");
}
