//! Demo: Internationalization with Project Fluent.
//!
//! Run with: cargo run --example i18n_demo --features i18n

use std::rc::Rc;

use burin::core::{Compositor, Widget};
use burin::i18n::{tr_signal, I18n};
use burin::platform::{App, WindowConfig};
use burin::style::{Color, Styled};
use burin::widgets::display::Text;
use burin::widgets::input::Button;
use burin::widgets::layout::{HStack, VStack};

fn main() {
    let i18n = I18n::builder()
        .initial_locale("en-US".parse().unwrap())
        .fallback("en-US".parse().unwrap())
        .add_resource(
            "en-US".parse().unwrap(),
            include_str!("locales/en-US/main.ftl"),
        )
        .add_resource(
            "zh-CN".parse().unwrap(),
            include_str!("locales/zh-CN/main.ftl"),
        )
        .build()
        .expect("i18n init");

    let config = WindowConfig {
        title: "i18n Demo".into(),
        width: 480.0,
        height: 320.0,
        i18n: Some(i18n.clone()),
        ..WindowConfig::auto_theme()
    };

    App::new()
        .window(config, Compositor::new(move |_scope| demo(i18n)))
        .run()
        .unwrap();
}

fn demo(i18n: Rc<I18n>) -> impl Widget {
    let title = tr_signal(&i18n, "app-title", &[]);
    let greeting = tr_signal(&i18n, "greeting", &[]);
    let instruction = tr_signal(&i18n, "select-locale", &[]);

    VStack::new()
        .gap(16.0)
        .push(
            Text::new(title.read())
                .bind(title)
                .font_size(28.0)
                .color(Color::WHITE),
        )
        .push(Text::new(greeting.read()).bind(greeting).font_size(18.0))
        .push(
            Text::new(instruction.read())
                .bind(instruction)
                .font_size(14.0),
        )
        .push(
            HStack::new()
                .gap(12.0)
                .push(Button::new("English").primary().on_click({
                    let i = i18n.clone();
                    move || i.set_locale("en-US".parse().unwrap())
                }))
                .push(Button::new("中文").primary().on_click({
                    let i = i18n.clone();
                    move || i.set_locale("zh-CN".parse().unwrap())
                })),
        )
}
