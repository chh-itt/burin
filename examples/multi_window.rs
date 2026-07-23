//! Multi-Window Demo
//!
//!   - Primary window: counter + "Open Helper" button
//!   - Inspector window: colour swatches (launched at startup)
//!   - Helper window: launched dynamically by clicking "Open Helper"
//!
//!   cargo run --example multi_window

use auralis_signal::Signal;
use burin::core::{Compositor, Widget};
use burin::platform::{self, App, WindowConfig};
use burin::style::{Color, CornerRadii, Padding, Styled};
use burin::theme::M3Theme;
use burin::widgets::display::*;
use burin::widgets::input::*;
use burin::widgets::layout::*;

fn main() {
    let counter = Signal::new(0i32);

    App::new()
        .window(
            WindowConfig {
                title: "Multi-Window Demo".into(),
                width: 380.0,
                height: 240.0,
                theme: M3Theme::from_seed(Color::rgba8(0x67, 0x79, 0xE8, 0xFF)),
                ..Default::default()
            },
            main_window(counter.clone()),
        )
        .window(
            WindowConfig {
                title: "Inspector".into(),
                width: 320.0,
                height: 200.0,
                theme: M3Theme::from_seed(Color::rgba8(0x67, 0x79, 0xE8, 0xFF)),
                ..Default::default()
            },
            inspector_window(counter.clone()),
        )
        .run()
        .expect("run");
}

fn main_window(counter: Signal<i32>) -> impl Widget {
    Compositor::new(move |_scope| {
        let c = counter.clone();

        VStack::new()
            .padding(Padding::all(20.0))
            .gap(12.0)
            .push(Text::new("Primary Window").font_size(18.0).font_weight(700))
            .push(Text::new("Interact with both windows independently").font_size(12.0))
            .push(
                HStack::new()
                    .gap(8.0)
                    .push(Button::new("Increment").primary().on_click({
                        let c = c.clone();
                        move || c.set(c.read() + 1)
                    }))
                    .push(Button::new("Decrement").secondary().on_click({
                        let c = c.clone();
                        move || c.set((c.read() - 1).max(0))
                    })),
            )
            .push(Button::new("Open Helper").on_click(|| {
                platform::create_window(
                    WindowConfig {
                        title: "Helper".into(),
                        width: 300.0,
                        height: 160.0,
                        theme: M3Theme::from_seed(Color::rgba8(0x67, 0x79, 0xE8, 0xFF))
                            .preset(burin::theme::PresetTheme::neo_minimal_slate()),
                        ..Default::default()
                    },
                    helper_window(),
                );
            }))
    })
}

fn inspector_window(counter: Signal<i32>) -> impl Widget {
    Compositor::new(move |_scope| {
        // Cross-window shared state: this Text lives in the Inspector
        // window but is driven by the counter mutated in the Primary
        // window (audit 2026-07-18 multi-window routing pass — the
        // Weak-routed dirty now wakes THIS window's event loop).
        let label = Signal::new(format!("Counter: {}", counter.read()));
        {
            let label = label.clone();
            let counter = counter.clone();
            auralis_signal::subscription::subscribe_derived(
                &counter.clone(),
                &label.clone(),
                move |l| l.set(format!("Counter: {}", counter.read())),
            );
        }
        VStack::new()
            .padding(Padding::all(20.0))
            .gap(10.0)
            .push(Text::new("Inspector").font_size(18.0).font_weight(700))
            .push(Text::new("Live view of the primary window's counter:").font_size(12.0))
            .push(
                Text::new(label.read())
                    .bind(label)
                    .font_size(24.0)
                    .font_weight(700),
            )
            .push(
                HStack::new()
                    .gap(8.0)
                    .push(color_swatch(59, 130, 246))
                    .push(color_swatch(34, 197, 94))
                    .push(color_swatch(239, 68, 68))
                    .push(color_swatch(234, 179, 8)),
            )
    })
}

fn color_swatch(r: u8, g: u8, b: u8) -> impl Widget {
    VStack::new()
        .width(40.0)
        .height(40.0)
        .background(Color::rgba8(r, g, b, 255))
        .corner_radius(CornerRadii::all(4.0))
}

fn helper_window() -> impl Widget {
    Compositor::new(move |_scope| {
        VStack::new()
            .padding(Padding::all(20.0))
            .gap(10.0)
            .push(Text::new("Helper").font_size(18.0).font_weight(700))
            .push(Text::new("Opened dynamically via").font_size(12.0))
            .push(Text::new("platform::create_window()").font_size(12.0))
            .push(Text::new("from a button callback.").font_size(12.0))
    })
}
