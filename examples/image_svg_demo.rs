//! GPU-backed demo: renders an SVG and a PNG image.
//!
//! Run with: cargo run --example image_svg_demo

use burin::asset;
use burin::core::{Compositor, Widget};
use burin::platform::{App, WindowConfig};
use burin::style::{Color, Styled};
use burin::widgets::display::{Image, SvgImage, Text};
use burin::widgets::layout::VStack;

fn main() {
    let config = WindowConfig {
        title: "Image + SVG Demo (GPU)".into(),
        width: 800.0,
        height: 650.0,
        ..WindowConfig::auto_theme()
    };

    App::new()
        .window(config, Compositor::new(move |_scope| gallery()))
        .run()
        .unwrap();
}

fn gallery() -> impl Widget {
    let svg_bytes = include_bytes!("柴犬.svg");
    let png_bytes = include_bytes!("a.png");

    let svg_id = asset::load_svg(svg_bytes).expect("failed to load SVG");
    let png_id = asset::load_image(png_bytes).expect("failed to load PNG");

    VStack::new()
        .gap(16.0)
        .push(
            Text::new("SVG -- 柴犬.svg")
                .font_size(24.0)
                .color(Color::WHITE),
        )
        .push(SvgImage::new(svg_id, 300, 300))
        .push(
            Text::new("PNG -- a.png")
                .font_size(24.0)
                .color(Color::WHITE),
        )
        .push(png_image(png_id))
}

fn png_image(id: asset::AssetId) -> Image {
    let img = asset::image_asset(id).expect("image not found");
    Image::from_rgba(img.data().to_vec(), img.width, img.height)
}
