//! SSR Demo — renders a widget tree to PNG without a window.
//!
//!   cargo run --example ssr_demo
//!
//! Output: ssr_output.png in the current directory.

use burin::core::Compositor;
use burin::render::ssr;
use burin::style::{Color, Padding, Styled};
use burin::widgets::display::Text;
use burin::widgets::layout::{Center, VStack};

fn main() {
    let png = ssr::render_to_png(app(), 600.0, 400.0).expect("ssr render");
    std::fs::write("ssr_output.png", &png).expect("write png");
    eprintln!("Wrote ssr_output.png ({} bytes)", png.len());
}

fn app() -> impl burin::core::Widget {
    Compositor::new(|_scope| {
        Center::new(burin::widgets::layout::Padding::new(
            Padding::all(24.0),
            VStack::new()
                .gap(16.0)
                .push(Text::new("Burin SSR Demo").font_size(22.0).font_weight(700))
                .push(
                    Text::new("Rendered without a window — CPU + tiny-skia")
                        .font_size(13.0)
                        .text_color(Color::rgba8(150, 150, 160, 255)),
                ),
        ))
    })
}
