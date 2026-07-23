//! Tray Demo — demonstrates system tray icon with menu and click handlers.
//!
//!   cargo run --example tray_demo --features "tray,ext-image"

use auralis_signal::Signal;
use burin::core::{Compositor, Widget};
use burin::platform::tray::{TrayIconBuilder, TrayMenu};
use burin::platform::{App, WindowConfig};
use burin::style::Styled;
use burin::theme::M3Theme;
use burin::widgets::display::Text;
use burin::widgets::layout::{Padding as PadW, VStack};

fn main() {
    let icon_bytes = include_bytes!("a.png");

    let mut tray_builder = TrayIconBuilder::new();
    tray_builder
        .icon_from_png(icon_bytes)
        .expect("decode a.png");
    tray_builder
        .tooltip("Auralis Tray Demo")
        .menu(
            TrayMenu::new()
                .item("Show Window", || println!("[tray] Show Window"))
                .item("Settings", || println!("[tray] Settings"))
                .separator()
                .item("Quit", || std::process::exit(0)),
        )
        .on_click(|| println!("[tray] left click"))
        .on_double_click(|| println!("[tray] double click"));

    App::new()
        .window(
            WindowConfig {
                title: "Tray Demo — see tray icon".into(),
                width: 400.0,
                height: 300.0,
                theme: M3Theme::from_seed(burin::style::Color::rgba8(0x67, 0x79, 0xE8, 0xFF))
                    .preset(burin::theme::PresetTheme::neo_minimal_slate()),
                tray: Some(tray_builder),
                ..Default::default()
            },
            app(),
        )
        .run()
        .expect("run");
}

fn app() -> impl Widget {
    Compositor::new(|_scope| {
        let text = Signal::new(String::from(
            "Check the system tray icon!\nRight-click it for a menu.",
        ));

        PadW::new(
            burin::style::Padding::all(24.0),
            VStack::new()
                .gap(12.0)
                .push(Text::new("Tray Demo").font_size(20.0).font_weight(700))
                .push(
                    Text::new("Check the system tray icon!")
                        .bind(text)
                        .font_size(14.0),
                ),
        )
    })
}
