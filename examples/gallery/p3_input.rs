use std::collections::HashSet;
use std::rc::Rc;

use crate::gallery::demo_panel::DemoPanel;
use crate::gallery::{section_sub, section_title};
use auralis_signal::Signal;
use burin::animation::{Animation, EasingCurve};
use burin::core::{Compositor, Widget};
use burin::resource::icons::Icon as IconKind;
use burin::style::Styled;
use burin::style::{Color, Padding, TooltipPlacement};
use burin::widgets::display::{Icon, Text};
#[cfg(feature = "ext-jiff")]
use burin::widgets::input::DatePicker;
#[cfg(feature = "ext-jiff")]
use burin::widgets::input::DateRange;
use burin::widgets::input::{
    Button, Checkbox, ComboBox, IconButton, NumberInput, OptionGroup, RadioGroup, Select, Slider,
    Switch, TextInput, TextInputType,
};
use burin::widgets::layout::*;
use burin::widgets::overlay::{toast, ToastContainer, ToastKind};
use burin::widgets::overlay::{Dialog, DialogAction, Modal};
use burin::widgets::overlay::{Popover, PopoverPosition, Tooltip};

pub fn button_section() -> impl Widget {
    Compositor::new(|_scope| {
        let click_count = Signal::new(0u32);
        let click_str = Signal::new("0".to_string());
        let disabled = Signal::new(false);
        let loading = Signal::new(false);
        let label_str = Signal::new("Click me".to_string());

        {
            let cc = click_count.clone();
            let cs = click_str.clone();
            let d = disabled.clone();
            let ls = label_str.clone();
            let l = loading.clone();
            auralis_signal::subscribe(
                &click_count,
                std::rc::Rc::new(move || {
                    cs.set(cc.read().to_string());
                }),
            );
            auralis_signal::subscribe(
                &disabled,
                std::rc::Rc::new({
                    let d2 = d.clone();
                    let ls2 = ls.clone();
                    move || {
                        ls2.set(if d2.read() {
                            "Disabled".into()
                        } else {
                            "Click me".into()
                        });
                    }
                }),
            );
            auralis_signal::subscribe(
                &loading,
                std::rc::Rc::new(move || {
                    ls.set(if l.read() {
                        "Loading...".into()
                    } else if d.read() {
                        "Disabled".into()
                    } else {
                        "Click me".into()
                    });
                }),
            );
        }

        VStack::new().gap(8.0)
            .push(section_title("Button  G2"))
            .push(section_sub("Core interactive element. 7 intents x 4 appearances + sizes/shapes. ARIA button pattern."))
            // ── Filled ──
            .push(Text::new("Filled").font_size(13.0).font_weight(600))
            .push(
                HStack::new().gap(6.0)
                    .push(Button::new("Default"))
                    .push(Button::new("Primary").primary())
                    .push(Button::new("Secondary").secondary())
                    .push(Button::new("Danger").danger())
                    .push(Button::new("Warning").warning())
                    .push(Button::new("Success").success())
                    .push(Button::new("Info").info())
                    .push(Button::new("Accent").accent())
            )
            // ── Outlined ──
            .push(Text::new("Outlined").font_size(13.0).font_weight(600))
            .push(
                HStack::new().gap(6.0)
                    .push(Button::new("Default").outlined())
                    .push(Button::new("Primary").primary().outlined())
                    .push(Button::new("Secondary").secondary().outlined())
                    .push(Button::new("Danger").danger().outlined())
                    .push(Button::new("Warning").warning().outlined())
                    .push(Button::new("Success").success().outlined())
                    .push(Button::new("Info").info().outlined())
                    .push(Button::new("Accent").accent().outlined())
            )
            // ── Text ──
            .push(Text::new("Text").font_size(13.0).font_weight(600))
            .push(
                HStack::new().gap(6.0)
                    .push(Button::new("Default").text_only())
                    .push(Button::new("Primary").primary().text_only())
                    .push(Button::new("Secondary").secondary().text_only())
                    .push(Button::new("Danger").danger().text_only())
                    .push(Button::new("Warning").warning().text_only())
                    .push(Button::new("Success").success().text_only())
                    .push(Button::new("Info").info().text_only())
                    .push(Button::new("Accent").accent().text_only())
            )
            // ── Elevated ──
            .push(Text::new("Elevated").font_size(13.0).font_weight(600))
            .push(
                HStack::new().gap(6.0)
                    .push(Button::new("Default").elevated())
                    .push(Button::new("Primary").primary().elevated())
                    .push(Button::new("Secondary").secondary().elevated())
                    .push(Button::new("Danger").danger().elevated())
                    .push(Button::new("Warning").warning().elevated())
                    .push(Button::new("Success").success().elevated())
                    .push(Button::new("Info").info().elevated())
                    .push(Button::new("Accent").accent().elevated())
            )
            // ── Sizes ──
            .push(Text::new("Sizes").font_size(13.0).font_weight(600))
            .push(
                HStack::new().gap(6.0)
                    .push(Button::new("Small").small().primary())
                    .push(Button::new("Medium").medium().primary())
                    .push(Button::new("Large").large().primary())
            )
            // ── Shapes ──
            .push(Text::new("Shapes").font_size(13.0).font_weight(600))
            .push(
                HStack::new().gap(6.0)
                    .push(Button::new("Rounded").rounded().primary())
                    .push(Button::new("Pill").pill().primary())
                    .push(Button::new("Square").square().primary())
            )
            // ── Interactive Demo ──
            .push(Text::new("Interactive").font_size(13.0).font_weight(600))
            .push(
                HStack::new().gap(12.0)
                    .push(
                        VStack::new().gap(8.0)
                            .push(Text::new("Click count:").font_size(12.0))
                            .push(
                                HStack::new().gap(8.0)
                                    .push(Button::new(label_str.read())
                                        .bind(label_str.clone())
                                        .primary()
                                        .on_click({
                                            let c = click_count.clone();
                                            move || { c.set(c.read() + 1); }
                                        })
                                    )
                                    .push(Text::new(click_str.read()).bind(click_str.clone()).font_size(16.0))
                            )
                    )
                    .push(
                        DemoPanel::new()
                            .toggle("Disabled", disabled.clone())
                            .toggle("Loading", loading.clone())
                            .field("Clicks", click_str.clone())
                            .info("Label", "Signal-bound")
                            .info("Role", "button (AccessKit)")
                    )
            )
    })
}

pub fn icon_button_section() -> impl Widget {
    Compositor::new(|_scope| {
        let click_count = Signal::new(0u32);
        let click_str = Signal::new("0".to_string());
        let disabled = Signal::new(false);
        let loading = Signal::new(false);

        {
            let cc = click_count.clone();
            let cs = click_str.clone();
            auralis_signal::subscribe(
                &click_count,
                std::rc::Rc::new(move || {
                    cs.set(cc.read().to_string());
                }),
            );
        }

        VStack::new().gap(8.0)
            .push(section_title("IconButton  G2"))
            .push(section_sub("Circle icon button. 7 intents x 4 appearances. Complete intent mapping + Styled + component_mask."))
            // ── Filled ──
            .push(Text::new("Filled").font_size(13.0).font_weight(600))
            .push(
                HStack::new().gap(6.0)
                    .push(IconButton::new(Icon::new(IconKind::Check)))
                    .push(IconButton::new(Icon::new(IconKind::Check)).primary())
                    .push(IconButton::new(Icon::new(IconKind::Check)).secondary())
                    .push(IconButton::new(Icon::new(IconKind::X)).danger())
                    .push(IconButton::new(Icon::new(IconKind::AlertCircle)).warning())
                    .push(IconButton::new(Icon::new(IconKind::Check)).success())
                    .push(IconButton::new(Icon::new(IconKind::Info)).info())
                    .push(IconButton::new(Icon::new(IconKind::Mail)).accent())
            )
            // ── Outlined ──
            .push(Text::new("Outlined").font_size(13.0).font_weight(600))
            .push(
                HStack::new().gap(6.0)
                    .push(IconButton::new(Icon::new(IconKind::Check)).outlined())
                    .push(IconButton::new(Icon::new(IconKind::Check)).primary().outlined())
                    .push(IconButton::new(Icon::new(IconKind::Check)).secondary().outlined())
                    .push(IconButton::new(Icon::new(IconKind::X)).danger().outlined())
                    .push(IconButton::new(Icon::new(IconKind::AlertCircle)).warning().outlined())
                    .push(IconButton::new(Icon::new(IconKind::Check)).success().outlined())
                    .push(IconButton::new(Icon::new(IconKind::Info)).info().outlined())
                    .push(IconButton::new(Icon::new(IconKind::Mail)).accent().outlined())
            )
            // ── Text ──
            .push(Text::new("Text").font_size(13.0).font_weight(600))
            .push(
                HStack::new().gap(6.0)
                    .push(IconButton::new(Icon::new(IconKind::Check)).text_only())
                    .push(IconButton::new(Icon::new(IconKind::Check)).primary().text_only())
                    .push(IconButton::new(Icon::new(IconKind::Check)).secondary().text_only())
                    .push(IconButton::new(Icon::new(IconKind::X)).danger().text_only())
                    .push(IconButton::new(Icon::new(IconKind::AlertCircle)).warning().text_only())
                    .push(IconButton::new(Icon::new(IconKind::Check)).success().text_only())
                    .push(IconButton::new(Icon::new(IconKind::Info)).info().text_only())
                    .push(IconButton::new(Icon::new(IconKind::Mail)).accent().text_only())
            )
            // ── Elevated ──
            .push(Text::new("Elevated").font_size(13.0).font_weight(600))
            .push(
                HStack::new().gap(6.0)
                    .push(IconButton::new(Icon::new(IconKind::Check)).elevated())
                    .push(IconButton::new(Icon::new(IconKind::Check)).primary().elevated())
                    .push(IconButton::new(Icon::new(IconKind::Check)).secondary().elevated())
                    .push(IconButton::new(Icon::new(IconKind::X)).danger().elevated())
                    .push(IconButton::new(Icon::new(IconKind::AlertCircle)).warning().elevated())
                    .push(IconButton::new(Icon::new(IconKind::Check)).success().elevated())
                    .push(IconButton::new(Icon::new(IconKind::Info)).info().elevated())
                    .push(IconButton::new(Icon::new(IconKind::Mail)).accent().elevated())
            )
            // ── Sizes ──
            .push(Text::new("Sizes").font_size(13.0).font_weight(600))
            .push(
                HStack::new().gap(6.0)
                    .push(IconButton::new(Icon::new(IconKind::Search)).small().primary())
                    .push(IconButton::new(Icon::new(IconKind::Search)).medium().primary())
                    .push(IconButton::new(Icon::new(IconKind::Search)).large().primary())
            )
            // ── Interactive Demo ──
            .push(Text::new("Interactive").font_size(13.0).font_weight(600))
            .push(
                HStack::new().gap(12.0)
                    .push(
                        HStack::new().gap(8.0)
                            .push(IconButton::new(Icon::new(IconKind::Play))
                                .primary()
                                .on_click({
                                    let c = click_count.clone();
                                    move || { c.set(c.read() + 1); }
                                })
                            )
                            .push(Text::new(click_str.read()).bind(click_str.clone()).font_size(16.0))
                    )
                    .push(
                        DemoPanel::new()
                            .toggle("Disabled", disabled.clone())
                            .toggle("Loading", loading.clone())
                            .field("Clicks", click_str.clone())
                            .info("Shape", "Circle")
                            .info("Role", "button (AccessKit)")
                    )
            )
    })
}

pub fn checkbox_section() -> impl Widget {
    Compositor::new(|_scope| {
        let checked1 = Signal::new(true);
        let checked2 = Signal::new(false);
        let indet1 = Signal::new(true);
        let demo_checked = Signal::new(true);
        let demo_indet = Signal::new(true);
        let toggle_count = Signal::new(0u32);
        let toggle_str = Signal::new("0".to_string());

        {
            let tc = toggle_count.clone();
            let ts = toggle_str.clone();
            auralis_signal::subscribe(
                &toggle_count,
                std::rc::Rc::new(move || {
                    ts.set(tc.read().to_string());
                }),
            );
        }

        VStack::new()
            .gap(8.0)
            .push(section_title("Checkbox  G2"))
            .push(section_sub(
                "Proper drawn visual (Lucide checkmark/dash), Space key, ARIA checkbox pattern.",
            ))
            // ── States ──
            .push(Text::new("States").font_size(13.0).font_weight(600))
            .push(
                HStack::new()
                    .gap(16.0)
                    .push(
                        HStack::new()
                            .gap(4.0)
                            .push(Checkbox::new(checked2.clone()))
                            .push(Text::new("Unchecked").font_size(13.0)),
                    )
                    .push(
                        HStack::new()
                            .gap(4.0)
                            .push(Checkbox::new(checked1.clone()))
                            .push(Text::new("Checked").font_size(13.0)),
                    )
                    .push(
                        HStack::new()
                            .gap(4.0)
                            .push(Checkbox::new(Signal::new(true)).indeterminate(indet1.clone()))
                            .push(Text::new("Indeterminate").font_size(13.0)),
                    )
                    .push(
                        HStack::new()
                            .gap(4.0)
                            .push(Checkbox::new(Signal::new(false)).disabled())
                            .push(Text::new("Disabled").font_size(13.0)),
                    ),
            )
            // ── Interactive Demo ──
            .push(Text::new("Interactive").font_size(13.0).font_weight(600))
            .push(
                HStack::new()
                    .gap(12.0)
                    .push(
                        VStack::new()
                            .gap(8.0)
                            .push(
                                HStack::new()
                                    .gap(4.0)
                                    .push(Checkbox::new(demo_checked.clone()).on_value_changed({
                                        let tc = toggle_count.clone();
                                        move |v| {
                                            tc.set(tc.read() + 1);
                                            _ = v;
                                        }
                                    }))
                                    .push(Text::new("Toggle me").font_size(13.0)),
                            )
                            .push(
                                HStack::new()
                                    .gap(4.0)
                                    .push(
                                        Checkbox::new(Signal::new(true))
                                            .indeterminate(demo_indet.clone()),
                                    )
                                    .push(Text::new("Indeterminate demo").font_size(13.0)),
                            )
                            .push(
                                HStack::new()
                                    .gap(4.0)
                                    .push(Checkbox::new(Signal::new(false)).disabled())
                                    .push(Text::new("Disabled demo").font_size(13.0)),
                            ),
                    )
                    .push(
                        DemoPanel::new()
                            .toggle("Checked", demo_checked.clone())
                            .toggle("Indeterminate", demo_indet.clone())
                            .field("Toggles", toggle_str.clone())
                            .info("Role", "checkbox (AccessKit)")
                            .info("Keyboard", "Space to toggle"),
                    ),
            )
    })
}

pub fn switch_section() -> impl Widget {
    Compositor::new(|_scope| {
        let on_sig = Signal::new(true);
        let off_sig = Signal::new(false);
        let demo_sig = Signal::new(true);
        let toggle_count = Signal::new(0u32);
        let toggle_str = Signal::new("0".to_string());

        {
            let tc = toggle_count.clone();
            let ts = toggle_str.clone();
            auralis_signal::subscribe(
                &toggle_count,
                std::rc::Rc::new(move || {
                    ts.set(tc.read().to_string());
                }),
            );
        }

        VStack::new()
            .gap(8.0)
            .push(section_title("Switch  G2"))
            .push(section_sub(
                "Proper drawn track+thumb pill visual, Space key, ARIA switch pattern.",
            ))
            .push(Text::new("States").font_size(13.0).font_weight(600))
            .push(
                HStack::new()
                    .gap(16.0)
                    .push(Switch::new(off_sig.clone()))
                    .push(Switch::new(on_sig.clone()))
                    .push(Switch::new(Signal::new(false)).disabled())
                    .push(Switch::new(Signal::new(true)).disabled()),
            )
            .push(Text::new("Interactive").font_size(13.0).font_weight(600))
            .push(
                HStack::new()
                    .gap(12.0)
                    .push(
                        VStack::new()
                            .gap(8.0)
                            .push(Switch::new(demo_sig.clone()).on_value_changed({
                                let tc = toggle_count.clone();
                                move |v| {
                                    tc.set(tc.read() + 1);
                                    _ = v;
                                }
                            }))
                            .push(Switch::new(Signal::new(false)).disabled()),
                    )
                    .push(
                        DemoPanel::new()
                            .toggle("On", demo_sig.clone())
                            .field("Toggles", toggle_str.clone())
                            .info("Role", "switch (AccessKit)")
                            .info("Keyboard", "Space to toggle"),
                    ),
            )
    })
}

pub fn radio_section() -> impl Widget {
    Compositor::new(|_scope| {
        let selected = Signal::new("alpha".to_string());
        let change_count = Signal::new(0u32);
        let change_str = Signal::new("alpha".to_string());
        let count_str = Signal::new("0".to_string());

        {
            let sc = selected.clone();
            let cs = change_str.clone();
            auralis_signal::subscribe(
                &selected,
                std::rc::Rc::new(move || {
                    cs.set(sc.read());
                }),
            );
        }
        {
            let cc = change_count.clone();
            let cs = count_str.clone();
            auralis_signal::subscribe(
                &change_count,
                std::rc::Rc::new(move || {
                    cs.set(cc.read().to_string());
                }),
            );
        }

        VStack::new().gap(8.0)
            .push(section_title("Radio  G2"))
            .push(section_sub("Proper drawn outer circle + inner dot, Signal-driven group, ARIA radiogroup pattern."))
            .push(Text::new("States").font_size(13.0).font_weight(600))
            .push(
                HStack::new().gap(16.0)
                    .push(RadioGroup::new(Signal::new("a".to_string()))
                        .option("Alpha", "a".to_string())
                        .option("Beta", "b".to_string())
                        .option("Gamma", "c".to_string()))
                    .push(RadioGroup::new(Signal::new("b".to_string()))
                        .option("One", "a".to_string())
                        .option("Two", "b".to_string())
                        .option("Three", "c".to_string()))
            )
            .push(Text::new("Interactive").font_size(13.0).font_weight(600))
            .push(
                HStack::new().gap(12.0)
                    .push(
                        VStack::new().gap(8.0)
                            .push(
                                RadioGroup::new(selected.clone())
                                    .option("Alpha", "alpha".to_string())
                                    .option("Beta", "beta".to_string())
                                    .option("Gamma", "gamma".to_string())
                                    .on_value_changed({
                                        let cc = change_count.clone();
                                        move |_| { cc.set(cc.read() + 1); }
                                    })
                            )
                            .push(Text::new("Disabled options:").font_size(12.0).font_weight(600))
                            .push(
                                RadioGroup::new(Signal::new("d".to_string()))
                                    .option("Enabled", "d".to_string())
                                    .disabled_option("Disabled", "e".to_string())
                            )
                            .push(Text::new("All disabled:").font_size(12.0).font_weight(600))
                            .push(
                                RadioGroup::new(Signal::new("f".to_string()))
                                    .option("N/A 1", "f".to_string())
                                    .option("N/A 2", "g".to_string())
                                    .disabled()
                            )
                    )
                    .push(
                        DemoPanel::new()
                            .field("Value", change_str.clone())
                            .field("Changes", count_str.clone())
                            .info("Role", "radiogroup (AccessKit)")
                            .info("Keyboard", "Tab to focus, Space to select")
                    )
            )
    })
}

pub fn slider_section() -> impl Widget {
    Compositor::new(|_scope| {
        let value = Signal::new(50.0f32);
        let value_str = Signal::new("50".to_string());
        let change_count = Signal::new(0u32);
        let count_str = Signal::new("0".to_string());

        {
            let vs = value_str.clone();
            let v = value.clone();
            auralis_signal::subscribe(
                &value,
                std::rc::Rc::new(move || {
                    vs.set(format!("{:.0}", v.read()));
                }),
            );
        }
        {
            let cc = change_count.clone();
            let cs = count_str.clone();
            auralis_signal::subscribe(
                &change_count,
                std::rc::Rc::new(move || {
                    cs.set(cc.read().to_string());
                }),
            );
        }

        VStack::new()
            .gap(8.0)
            .push(section_title("Slider  G2"))
            .push(section_sub(
                "Custom-painted track + thumb, drag/click/keyboard (Arrow/Home/End), ARIA slider.",
            ))
            .push(Text::new("States").font_size(13.0).font_weight(600))
            .push(
                HStack::new()
                    .gap(16.0)
                    .push(Slider::new(Signal::new(30.0f32)).width(160.0))
                    .push(Slider::new(Signal::new(70.0f32)).width(160.0))
                    .push(Slider::new(Signal::new(0.0f32)).width(160.0).disabled()),
            )
            .push(Text::new("Interactive").font_size(13.0).font_weight(600))
            .push(
                HStack::new()
                    .gap(12.0)
                    .push(
                        VStack::new().gap(8.0).push(
                            Slider::new(value.clone())
                                .width(240.0)
                                .range(0.0, 100.0)
                                .on_changed({
                                    let cc = change_count.clone();
                                    move |_| {
                                        cc.set(cc.read() + 1);
                                    }
                                }),
                        ),
                    )
                    .push(
                        DemoPanel::new()
                            .field("Value", value_str.clone())
                            .field("Changes", count_str.clone())
                            .info("Role", "slider (AccessKit)")
                            .info("Keyboard", "Arrow ±1, Home/End bounds"),
                    ),
            )
    })
}

pub fn number_input_section() -> impl Widget {
    Compositor::new(|_scope| {
        let num_val = Signal::new(42.0f64);

        VStack::new()
            .gap(12.0)
            .push(section_title("NumberInput"))
            .push(Text::new("Basic").font_size(13.0).font_weight(600))
            .push(
                HStack::new()
                    .gap(12.0)
                    .push(
                        NumberInput::new(num_val.clone())
                            .step(1.0)
                            .decimals(0)
                            .placeholder("Enter a number..."),
                    )
                    .push(DemoPanel::new().info("Role", "spinbutton")),
            )
            .push(
                Text::new("Decimal + Range")
                    .font_size(13.0)
                    .font_weight(600),
            )
            .push(
                HStack::new()
                    .gap(12.0)
                    .push(
                        NumberInput::new(Signal::new(3.14f64))
                            .range(0.0, 10.0)
                            .step(0.01)
                            .decimals(2)
                            .placeholder("0.00–10.00"),
                    )
                    .push(
                        NumberInput::new(Signal::new(50.0f64))
                            .range(0.0, 100.0)
                            .step(5.0)
                            .decimals(0)
                            .placeholder("0–100"),
                    ),
            )
            .push(Text::new("Disabled").font_size(13.0).font_weight(600))
            .push(
                NumberInput::new(Signal::new(25.0f64))
                    .range(0.0, 100.0)
                    .step(1.0)
                    .decimals(0)
                    .disabled()
                    .placeholder("Disabled"),
            )
    })
}

pub fn text_input_section() -> impl Widget {
    Compositor::new(|_scope| {
        let text_val = Signal::new("Hello".to_string());
        let pw_val = Signal::new(String::new());
        let multi_val = Signal::new(String::new());
        let text_str = Signal::new("Hello".to_string());

        {
            let tv = text_val.clone();
            let ts = text_str.clone();
            auralis_signal::subscribe(
                &text_val,
                std::rc::Rc::new(move || {
                    ts.set(tv.read());
                }),
            );
        }

        VStack::new().gap(8.0)
            .push(section_title("TextInput  G4"))
            .push(section_sub("Core text editing. Single/Multi/Password modes. IME, undo, selection. ARIA textbox pattern."))
            .push(Text::new("Single-line").font_size(13.0).font_weight(600))
            .push(
                HStack::new().gap(12.0)
                    .push(TextInput::new(text_val.clone()).placeholder("Type something..."))
                    .push(DemoPanel::new().field("Value", text_str.clone()).info("Role", "textbox"))
            )
            .push(Text::new("Password").font_size(13.0).font_weight(600))
            .push(TextInput::new(pw_val.clone()).input_type(TextInputType::Password).placeholder("Enter password"))
            .push(Text::new("Multiline").font_size(13.0).font_weight(600))
            .push(TextInput::new(multi_val.clone()).input_type(TextInputType::Multiline).placeholder("Type multiple lines..."))
    })
}

pub fn select_section() -> impl Widget {
    Compositor::new(|_scope| {
        let options: Vec<String> = vec![
            "Apple",
            "Banana",
            "Cherry",
            "Date",
            "Elderberry",
            "Fig",
            "Grape",
            "Honeydew",
        ]
        .into_iter()
        .map(|s| s.to_string())
        .collect();
        let selected = Signal::new(None::<String>);
        let selected_str = Signal::new("None".to_string());
        let disabled = Signal::new(false);
        let loading = Signal::new(false);
        let change_count = Signal::new(0u32);
        let change_str = Signal::new("0".to_string());

        // ── Select 2: clearable + close_on_select=false ──
        let selected2 = Signal::new(None::<String>);
        let sel2_str = Signal::new("None".to_string());

        // ── Select 3: groups ──
        let selected3 = Signal::new(None::<String>);
        let sel3_str = Signal::new("None".to_string());

        // ── Select 4: disabled options ──
        let selected4 = Signal::new(None::<String>);
        let sel4_str = Signal::new("None".to_string());
        let dis4 = Signal::new(HashSet::from([0usize, 2usize, 4usize]));

        {
            let ss = selected_str.clone();
            let s = selected.clone();
            auralis_signal::subscribe(
                &selected,
                Rc::new(move || {
                    ss.set(s.read().map_or("None".into(), |v| v));
                }),
            );
        }
        {
            let cs = change_str.clone();
            let cc = change_count.clone();
            auralis_signal::subscribe(
                &change_count,
                Rc::new(move || {
                    cs.set(cc.read().to_string());
                }),
            );
        }
        {
            let s2 = sel2_str.clone();
            let sel = selected2.clone();
            auralis_signal::subscribe(
                &selected2,
                Rc::new(move || {
                    s2.set(sel.read().map_or("None".into(), |v| v));
                }),
            );
        }
        {
            let s3 = sel3_str.clone();
            let sel = selected3.clone();
            auralis_signal::subscribe(
                &selected3,
                Rc::new(move || {
                    s3.set(sel.read().map_or("None".into(), |v| v));
                }),
            );
        }
        {
            let s4 = sel4_str.clone();
            let sel = selected4.clone();
            auralis_signal::subscribe(
                &selected4,
                Rc::new(move || {
                    s4.set(sel.read().map_or("None".into(), |v| v));
                }),
            );
        }

        VStack::new()
            .gap(8.0)
            .push(section_title("Select  G3"))
            .push(section_sub(
                "Dropdown with keyboard nav, ARIA combobox. Groups, disabled options.",
            ))
            // ── Basic Select ──
            .push(Text::new("Basic").font_size(13.0).font_weight(600))
            .push(
                HStack::new()
                    .gap(12.0)
                    .push(
                        Select::new(selected.clone())
                            .options(options.clone())
                            .render(|s: &String| s.clone())
                            .placeholder("Choose a fruit...")
                            .max_visible(5)
                            .on_change({
                                let cc = change_count.clone();
                                move |_v| {
                                    cc.update(|c| *c += 1);
                                }
                            }),
                    )
                    .push(
                        DemoPanel::new()
                            .field("Value", selected_str.clone())
                            .field("Changes", change_str.clone())
                            .toggle("Disabled", disabled.clone())
                            .toggle("Loading", loading.clone())
                            .info("Role", "combobox"),
                    ),
            )
            // ── Stay-open ──
            .push(Text::new("Stay-open").font_size(13.0).font_weight(600))
            .push(
                HStack::new()
                    .gap(12.0)
                    .push(
                        Select::new(selected2.clone())
                            .options(options.clone())
                            .render(|s: &String| s.clone())
                            .placeholder("Choose a fruit...")
                            .close_on_select(false)
                            .max_visible(5),
                    )
                    .push(
                        DemoPanel::new()
                            .field("Value", sel2_str.clone())
                            .info("Close", "stays open on select"),
                    ),
            )
            // ── Option groups ──
            .push(Text::new("Option groups").font_size(13.0).font_weight(600))
            .push(
                HStack::new()
                    .gap(12.0)
                    .push(
                        Select::new(selected3.clone())
                            .groups(vec![
                                OptionGroup::new(
                                    "Citrus",
                                    vec!["Lemon".into(), "Orange".into(), "Lime".into()],
                                ),
                                OptionGroup::new(
                                    "Berries",
                                    vec![
                                        "Strawberry".into(),
                                        "Blueberry".into(),
                                        "Raspberry".into(),
                                    ],
                                ),
                            ])
                            .render(|s: &String| s.clone())
                            .placeholder("Pick a fruit...")
                            .max_visible(6),
                    )
                    .push(
                        DemoPanel::new()
                            .field("Value", sel3_str.clone())
                            .info("Groups", "2 sections, 6 items"),
                    ),
            )
            // ── Disabled options ──
            .push(
                Text::new("Disabled options")
                    .font_size(13.0)
                    .font_weight(600),
            )
            .push(
                HStack::new()
                    .gap(12.0)
                    .push(
                        Select::new(selected4.clone())
                            .options(options.clone())
                            .render(|s: &String| s.clone())
                            .placeholder("Some unavailable...")
                            .disabled_options(dis4.clone())
                            .max_visible(5),
                    )
                    .push(
                        DemoPanel::new()
                            .field("Value", sel4_str.clone())
                            .info("Disabled", "Apple, Cherry, Elderberry"),
                    ),
            )
    })
}

pub fn combobox_section() -> impl Widget {
    Compositor::new(|_scope| {
        let options: Vec<String> = vec![
            "Apple",
            "Banana",
            "Cherry",
            "Date",
            "Elderberry",
            "Fig",
            "Grape",
            "Honeydew",
            "Kiwi",
            "Lemon",
            "Mango",
            "Nectarine",
            "Orange",
            "Papaya",
            "Quince",
            "Raspberry",
            "Strawberry",
            "Tangerine",
            "Ugli",
            "Vanilla",
            "Watermelon",
        ]
        .into_iter()
        .map(|s| s.to_string())
        .collect();
        let selected = Signal::new(None::<String>);
        let selected_str = Signal::new("None".to_string());
        let change_count = Signal::new(0u32);
        let change_str = Signal::new("0".to_string());

        // Stay-open variant
        let selected2 = Signal::new(None::<String>);
        let sel2_str = Signal::new("None".to_string());

        {
            let ss = selected_str.clone();
            let s = selected.clone();
            auralis_signal::subscribe(
                &selected,
                Rc::new(move || {
                    ss.set(s.read().map_or("None".into(), |v| v));
                }),
            );
        }
        {
            let cs = change_str.clone();
            let cc = change_count.clone();
            auralis_signal::subscribe(
                &change_count,
                Rc::new(move || {
                    cs.set(cc.read().to_string());
                }),
            );
        }
        {
            let s2 = sel2_str.clone();
            let sel = selected2.clone();
            auralis_signal::subscribe(
                &selected2,
                Rc::new(move || {
                    s2.set(sel.read().map_or("None".into(), |v| v));
                }),
            );
        }

        VStack::new().gap(8.0)
            .push(section_title("ComboBox  G3"))
            .push(section_sub("Type to filter. ArrowDown opens, keyboard navigates, Enter selects. ARIA combobox pattern."))
            // ── Basic ComboBox ──
            .push(Text::new("Basic (type to filter)").font_size(13.0).font_weight(600))
            .push(
                HStack::new().gap(12.0)
                    .push(
                        ComboBox::new(selected.clone())
                            .options(options.clone())
                            .render(|s: &String| s.clone())
                            .placeholder("Search fruits...")
                            .max_visible(6)
                            .on_change({ let cc = change_count.clone(); move |_v| { cc.update(|c| *c += 1); } })
                    )
                    .push(DemoPanel::new()
                        .field("Value", selected_str.clone())
                        .field("Changes", change_str.clone())
                        .info("Role", "combobox")
                        .info("Hint", "type to filter"))
            )
            // ── Stay-open ──
            .push(Text::new("Stay-open").font_size(13.0).font_weight(600))
            .push(
                HStack::new().gap(12.0)
                    .push(
                        ComboBox::new(selected2.clone())
                            .options(options.clone())
                            .render(|s: &String| s.clone())
                            .placeholder("Multi-select fruits...")
                            .close_on_select(false)
                            .max_visible(6)
                    )
                    .push(DemoPanel::new()
                        .field("Value", sel2_str.clone())
                        .info("Close", "stays open on select"))
            )
            // ── Long list (scrolling) ──
            .push(Text::new("Long list (scroll)").font_size(13.0).font_weight(600))
            .push(
                HStack::new().gap(12.0)
                    .push(
                        ComboBox::new(Signal::new(None::<String>))
                            .options(options.clone())
                            .render(|s: &String| s.clone())
                            .placeholder("21 fruits...")
                            .max_visible(8)
                            .item_height(32.0)
                    )
                    .push(DemoPanel::new()
                        .info("Items", "21 total")
                        .info("Scroll", "max-visible=8"))
            )
    })
}

#[cfg(feature = "ext-jiff")]
pub fn datepicker_section() -> impl Widget {
    use jiff::civil::Date;
    Compositor::new(|_scope| {
        let selected_date = Signal::new(None::<Date>);
        let date_str = Signal::new("None".to_string());
        let change_count = Signal::new(0u32);
        let change_str = Signal::new("0".to_string());

        {
            let ds = date_str.clone();
            let sd = selected_date.clone();
            auralis_signal::subscribe(
                &selected_date,
                Rc::new(move || {
                    ds.set(
                        sd.read()
                            .map_or("None".into(), |d| d.strftime("%Y-%m-%d").to_string()),
                    );
                }),
            );
        }
        {
            let cs = change_str.clone();
            let cc = change_count.clone();
            auralis_signal::subscribe(
                &change_count,
                Rc::new(move || {
                    cs.set(cc.read().to_string());
                }),
            );
        }

        VStack::new().gap(8.0)
            .push(section_title("DatePicker  G3"))
            .push(section_sub("Calendar grid with keyboard nav, Year/Month picker, ARIA dialog/grid pattern (ext-jiff)."))
            .push(Text::new("Basic").font_size(13.0).font_weight(600))
            .push(
                HStack::new().gap(12.0)
                    .push(
                        DatePicker::new(selected_date.clone())
                            .placeholder("Pick a date..."),
                    )
                    .push(DemoPanel::new()
                        .field("Value", date_str.clone())
                        .field("Changes", change_str.clone())
                        .info("Role", "dialog/grid")),
            )
            .push(Text::new("Range selection").font_size(13.0).font_weight(600))
            .push(
                HStack::new().gap(12.0)
                    .push({
                        let rng = Signal::new(None::<DateRange>);
                        let rng_str = Signal::new("None".to_string());
                        {
                            let r = rng.clone();
                            let rs = rng_str.clone();
                            auralis_signal::subscribe(&rng, Rc::new(move || {
                                rs.set(r.read().map_or("None".into(), |dr| {
                                    format!("{} ~ {}", dr.start.strftime("%Y-%m-%d"), dr.end.strftime("%Y-%m-%d"))
                                }));
                            }));
                        }
                        HStack::new().gap(12.0)
                            .push(
                                DatePicker::new_range(rng)
                                    .min_date(Date::new(2024, 1, 1).unwrap())
                                    .max_date(Date::new(2026, 12, 31).unwrap())
                                    .placeholder("Select range..."),
                            )
                            .push(DemoPanel::new()
                                .field("Range", rng_str)
                                .info("Selection", "Tap start → tap end"))
                    }),
            )
    })
}

pub fn color_picker_section() -> impl Widget {
    Compositor::new(|_scope| {
        let color = Signal::new(burin::style::Color::rgba8(59, 130, 246, 255));
        let hex_str = Signal::new("#3B82F6".to_string());

        {
            let c = color.clone();
            let h = hex_str.clone();
            auralis_signal::subscribe(
                &color,
                std::rc::Rc::new(move || {
                    h.set(c.read().to_string());
                }),
            );
        }

        let change_count = Signal::new(0u32);
        let count_str = Signal::new("0".to_string());
        {
            let cc = change_count.clone();
            let cs = count_str.clone();
            auralis_signal::subscribe(
                &change_count,
                std::rc::Rc::new(move || {
                    cs.set(cc.read().to_string());
                }),
            );
        }

        VStack::new().gap(8.0)
            .push(section_title("ColorPicker  \u{1F3A8}"))
            .push(section_sub("HSLA model with 2D saturation-lightness plane, hue & alpha bars, hex input, 12 presets."))
            .push(
                HStack::new().gap(16.0)
                    .push(
                        burin::widgets::input::ColorPicker::new(color.clone())
                            .on_changed({
                                let cc = change_count.clone();
                                move |_| { cc.set(cc.read() + 1); }
                            }),
                    )
                    .push(
                        DemoPanel::new()
                            .field("Hex", hex_str.clone())
                            .field("Changes", count_str.clone()),
                    )
            )
    })
}

#[cfg(not(feature = "ext-jiff"))]
pub fn datepicker_section() -> impl Widget {
    use burin::widgets::display::Text;
    Text::new("DatePicker (needs ext-jiff feature)").font_size(12.0)
}

#[cfg(feature = "file-dialog")]
pub fn filepicker_section() -> impl Widget {
    use burin::widgets::input::FilePickerButton;

    Compositor::new(|_scope| {
        let path_str = Signal::new("—".to_string());
        let multi_str = Signal::new("—".to_string());
        let folder_str = Signal::new("—".to_string());
        let save_str = Signal::new("—".to_string());

        let change_count = Signal::new(0u32);
        let change_str = Signal::new("0".to_string());
        {
            let cc = change_count.clone();
            let cs = change_str.clone();
            auralis_signal::subscribe(
                &change_count,
                Rc::new(move || {
                    cs.set(cc.read().to_string());
                }),
            );
        }

        VStack::new().gap(8.0)
            .push(section_title("FilePicker  G4"))
            .push(section_sub("Native OS file dialog. Open / Save / Folder / Multi-select. ARIA button pattern (file-dialog)."))
            .push(Text::new("Open").font_size(13.0).font_weight(600))
            .push(
                HStack::new().gap(12.0)
                    .push({
                        let ps = path_str.clone();
                        let cc = change_count.clone();
                        FilePickerButton::new("Open File...")
                            .open_mode()
                            .on_file_selected(move |f| {
                                ps.set(f.path.display().to_string());
                                cc.set(cc.read() + 1);
                            })
                    })
                    .push(
                        FilePickerButton::new("Open (Text)")
                            .open_mode()
                            .appearance(burin::theme::Appearance::Outlined)
                            .filter("Text", &["txt", "md", "rs"])
                            .on_file_selected({
                                let ps = path_str.clone();
                                let cc = change_count.clone();
                                move |f| {
                                    ps.set(f.path.display().to_string());
                                    cc.set(cc.read() + 1);
                                }
                            }),
                    )
                    .push(DemoPanel::new()
                        .field("Path", path_str.clone())
                        .field("Changes", change_str.clone())
                        .info("Role", "button")),
            )
            .push(Text::new("Save").font_size(13.0).font_weight(600))
            .push(
                HStack::new().gap(12.0)
                    .push({
                        let ss = save_str.clone();
                        FilePickerButton::new("Save File...")
                            .save_mode()
                            .default_filename("untitled.txt")
                            .filter("All Files", &["*"])
                            .intent(burin::theme::Intent::Primary)
                            .on_file_selected(move |f| {
                                ss.set(f.path.display().to_string());
                            })
                    })
                    .push(DemoPanel::new()
                        .field("Save Path", save_str.clone())
                        .info("Role", "button")),
            )
            .push(Text::new("Folder & Multi").font_size(13.0).font_weight(600))
            .push(
                HStack::new().gap(12.0)
                    .push({
                        let fs = folder_str.clone();
                        FilePickerButton::new("Pick Folder...")
                            .folder_mode()
                            .appearance(burin::theme::Appearance::Outlined)
                            .intent(burin::theme::Intent::Accent)
                            .on_file_selected(move |f| {
                                fs.set(f.path.display().to_string());
                            })
                    })
                    .push({
                        let ms = multi_str.clone();
                        FilePickerButton::new("Open Files...")
                            .multi_mode()
                            .appearance(burin::theme::Appearance::Outlined)
                            .filter("Images", &["png", "jpg"])
                            .on_files_selected(move |files| {
                                ms.set(format!("{} files", files.len()));
                            })
                    })
                    .push(DemoPanel::new()
                        .field("Folder", folder_str.clone())
                        .field("Multi", multi_str.clone())),
            )
    })
}

#[cfg(not(feature = "file-dialog"))]
pub fn filepicker_section() -> impl Widget {
    use burin::widgets::display::Text;
    Text::new("FilePicker (needs file-dialog feature)").font_size(12.0)
}

// ── Tooltip ───────────────────────────────────────────────────────

pub fn tooltip_section() -> impl Widget {
    Compositor::new(|_scope| {
        let tc = burin::style::Color::rgba8(255, 255, 255, 255);

        VStack::new().gap(8.0)
            .push(section_title("Tooltip  \u{1F4AC}"))
            .push(section_sub("Hover-triggered floating label. Auto-flip positioning, grace period, fade animation."))
            .push(Text::new("Basic (Top placement, 300ms delay)").font_size(13.0).font_weight(600))
            .push(
                HStack::new().gap(12.0)
                    .push(
                        Tooltip::new(
                            Button::new("Hover me").primary(),
                            Text::new("This is a tooltip!").font_size(12.0).color(tc),
                        ),
                    )
                    .push(
                        Tooltip::new(
                            Button::new("Rich content").secondary(),
                            VStack::new().gap(4.0)
                                .push(Text::new("Custom tooltip").font_size(12.0).font_weight(600).color(tc))
                                .push(Text::new("With multiple lines").font_size(11.0).color(tc)),
                        ),
                    )
                    .push(DemoPanel::new()
                        .info("Placement", "Top")
                        .info("Delay", "300ms")
                        .info("Grace", "300ms")),
            )
            .push(Text::new("Placements").font_size(13.0).font_weight(600))
            .push(
                HStack::new().gap(8.0)
                    .push(
                        Tooltip::new(
                            Button::new("Top").success(),
                            Text::new("Tooltip on top").font_size(12.0).color(tc),
                        ).placement(TooltipPlacement::Top),
                    )
                    .push(
                        Tooltip::new(
                            Button::new("Bottom").warning(),
                            Text::new("Tooltip on bottom").font_size(12.0).color(tc),
                        ).placement(TooltipPlacement::Bottom),
                    )
                    .push(
                        Tooltip::new(
                            Button::new("Left").danger(),
                            Text::new("Tooltip on left").font_size(12.0).color(tc),
                        ).placement(TooltipPlacement::Left),
                    )
                    .push(
                        Tooltip::new(
                            Button::new("Right").info(),
                            Text::new("Tooltip on right").font_size(12.0).color(tc),
                        ).placement(TooltipPlacement::Right),
                    )
                    .push(DemoPanel::new()
                        .info("Auto-flip", "enabled")),
            )
            .push(Text::new("Icon trigger").font_size(13.0).font_weight(600))
            .push(
                HStack::new().gap(8.0)
                    .push(
                        Tooltip::new(
                            IconButton::new(Icon::new(IconKind::Info)),
                            Text::new("Help information").font_size(12.0).color(tc),
                        ),
                    )
                    .push(
                        Tooltip::new(
                            IconButton::new(Icon::new(IconKind::Settings)).primary().outlined(),
                            VStack::new().gap(2.0)
                                .push(Text::new("Auralis-UI v0.1").font_size(12.0).font_weight(600).color(tc))
                                .push(Text::new("Rust GUI framework").font_size(11.0).color(tc)),
                        ),
                    )
                    .push(DemoPanel::new()
                        .info("Content", "text + widget stack")),
            )
    })
}

// ── Popover ───────────────────────────────────────────────────────

pub fn popover_section() -> impl Widget {
    Compositor::new(|_scope| {
        let open = Signal::new(false);
        let open_str = Signal::new("false".to_string());
        let click_count = Signal::new(0u32);
        let count_str = Signal::new("0".to_string());

        {
            let o = open.clone();
            let os = open_str.clone();
            auralis_signal::subscribe(
                &open,
                Rc::new(move || {
                    os.set(o.read().to_string());
                }),
            );
        }
        {
            let cc = click_count.clone();
            let cs = count_str.clone();
            auralis_signal::subscribe(
                &click_count,
                Rc::new(move || {
                    cs.set(cc.read().to_string());
                }),
            );
        }

        VStack::new().gap(8.0)
            .push(section_title("Popover  \u{1F4E6}"))
            .push(section_sub("Click-triggered floating panel. Anchor-positioned, focus-trapped, outside-click-dismiss, keyboard Escape."))
            .push(Text::new("Basic (Bottom, dismiss on outside click)").font_size(13.0).font_weight(600))
            .push(
                HStack::new().gap(12.0)
                    .push(
                        Popover::new(
                            open.clone(),
                            Button::new("Open Popover").primary().on_click({
                                let o = open.clone();
                                move || o.set(!o.read())
                            }),
                            VStack::new().gap(8.0).padding(burin::style::Padding::all(4.0))
                                .push(Text::new("Popover Content").font_size(14.0).font_weight(600))
                                .push(Text::new("Click outside or press Escape.").font_size(12.0))
                                .push(
                                    HStack::new().gap(6.0)
                                        .push(Button::new("Action").success().on_click({
                                            let cc = click_count.clone();
                                            move || { cc.set(cc.read() + 1); }
                                        }))
                                        .push(Button::new("Close").danger().on_click({
                                            let o = open.clone();
                                            move || o.set(false)
                                        })),
                                ),
                        )
                    )
                    .push(DemoPanel::new()
                        .field("Open", open_str.clone())
                        .field("Clicks", count_str.clone())
                        .info("Dismiss", "outside click")),
            )
            .push(Text::new("Positions + Animation").font_size(13.0).font_weight(600))
            .push(
                HStack::new().gap(8.0)
                    .push({
                        let o = Signal::new(false);
                        Popover::new(
                            o.clone(),
                            Button::new("Bottom").warning().on_click({ let o = o.clone(); move || o.set(!o.read()) }),
                            Text::new("Placed below the trigger").font_size(12.0),
                        ).position(PopoverPosition::Bottom)
                            .animation(Animation { curve: EasingCurve::EaseOut, duration_secs: 0.15 })
                    })
                    .push({
                        let o = Signal::new(false);
                        Popover::new(
                            o.clone(),
                            Button::new("Top").info().on_click({ let o = o.clone(); move || o.set(!o.read()) }),
                            Text::new("Placed above the trigger").font_size(12.0),
                        ).position(PopoverPosition::Top)
                            .animation(Animation { curve: EasingCurve::EaseOut, duration_secs: 0.15 })
                    })
                    .push({
                        let o = Signal::new(false);
                        Popover::new(
                            o.clone(),
                            Button::new("Right").success().on_click({ let o = o.clone(); move || o.set(!o.read()) }),
                            Text::new("Placed right").font_size(12.0),
                        ).position(PopoverPosition::Right)
                    })
                    .push({
                        let o = Signal::new(false);
                        Popover::new(
                            o.clone(),
                            Button::new("Left").danger().on_click({ let o = o.clone(); move || o.set(!o.read()) }),
                            Text::new("Placed left").font_size(12.0),
                        ).position(PopoverPosition::Left)
                    })
                    .push(DemoPanel::new()
                        .info("Animation", "fade 150ms")
                        .info("Auto-flip", "enabled")),
            )
    })
}

// ── Modal ─────────────────────────────────────────────────────────

pub fn modal_section() -> impl Widget {
    Compositor::new(|_scope| {
        let modal_visible = Signal::new(false);
        let modal_str = Signal::new("false".to_string());

        {
            let m = modal_visible.clone();
            let ms = modal_str.clone();
            auralis_signal::subscribe(
                &modal_visible,
                Rc::new(move || {
                    ms.set(m.read().to_string());
                }),
            );
        }

        VStack::new().gap(8.0)
            .push(section_title("Modal & Dialog  \u{1F3DE}"))
            .push(section_sub("Full-screen overlay with backdrop. Focus-trapped, click-backdrop-dismiss, Escape."))
            .push(Text::new("Modal (custom content)").font_size(13.0).font_weight(600))
            .push(
                HStack::new().gap(12.0)
                    .push(
                        Modal::new(
                            modal_visible.clone(),
                            Center::new(
                                VStack::new().gap(12.0)
                                    .background(Color::rgba8(30, 30, 40, 255))
                                    .corner_radius(burin::style::CornerRadii::all(12.0))
                                    .padding(Padding::all(24.0))
                                    .push(Text::new("Custom Modal").font_size(18.0).font_weight(700).color(Color::rgba8(255, 255, 255, 255)))
                                    .push(Text::new("This is a custom modal with backdrop.").font_size(13.0).color(Color::rgba8(200, 200, 220, 255)))
                                    .push(Button::new("Close Modal").danger().on_click({
                                        let m = modal_visible.clone();
                                        move || m.set(false)
                                    })),
                            ),
                        ),
                    )
                    .push(
                        Button::new("Open Modal").primary().on_click({
                            let m = modal_visible.clone();
                            move || m.set(true)
                        }),
                    )
                    .push(DemoPanel::new().field("Visible", modal_str.clone()).info("Backdrop", "scrim")),
            )
            .push(Text::new("Dialog (title + text + actions)").font_size(13.0).font_weight(600))
            .push({
                let d = Signal::new(false);
                HStack::new().gap(12.0)
                    .push(
                        Dialog::new(d.clone())
                            .title("Confirm Delete")
                            .content_text("Are you sure you want to delete this item? This action cannot be undone.")
                            .actions(vec![
                                DialogAction::new("Cancel", burin::theme::Intent::Secondary),
                                DialogAction::new("Delete", burin::theme::Intent::Danger)
                                    .on_click(|| { /* delete logic */ }),
                            ])
                    )
                    .push(
                        Button::new("Open Dialog").danger().on_click({
                            let d = d.clone();
                            move || d.set(true)
                        }),
                    )
                    .push(DemoPanel::new()
                        .info("Title", "Confirm Delete")
                        .info("Actions", "Cancel + Delete")
                        .info("Backdrop", "click to close"))
            })
    })
}

// ── Accordion ─────────────────────────────────────────────────────

pub fn accordion_section() -> impl Widget {
    use burin::widgets::composite::Accordion;
    use std::collections::HashSet;

    Compositor::new(|_scope| {
        let open_set = Signal::new(HashSet::<usize>::new());
        let open_str = Signal::new("none".to_string());
        let toggle_count = Signal::new(0u32);
        let toggle_str = Signal::new("0".to_string());

        {
            let os = open_set.clone();
            let ostr = open_str.clone();
            auralis_signal::subscribe(
                &os.clone(),
                Rc::new(move || {
                    let set = os.read();
                    if set.is_empty() {
                        ostr.set("none".to_string());
                    } else {
                        ostr.set(
                            set.iter()
                                .map(|i| i.to_string())
                                .collect::<Vec<_>>()
                                .join(", "),
                        );
                    }
                }),
            );
        }
        {
            let tc = toggle_count.clone();
            let ts = toggle_str.clone();
            auralis_signal::subscribe(
                &tc.clone(),
                Rc::new(move || {
                    ts.set(tc.read().to_string());
                }),
            );
        }

        VStack::new().gap(8.0)
            .push(section_title("Accordion  G5"))
            .push(section_sub("Expandable sections. Single/multi mode, animation, callbacks, disabled. ARIA disclosure pattern."))
            .push(Text::new("Basic (single expand)").font_size(13.0).font_weight(600))
            .push(
                HStack::new().gap(12.0)
                    .push({
                        let os = open_set.clone();
                        let tc = toggle_count.clone();
                        Accordion::new(os)
                            .section("Getting Started",
                                Text::new("Welcome to Auralis-UI! This framework provides a comprehensive set of widgets for building modern desktop applications. Start by exploring the gallery.")
                                    .font_size(12.0).padding(burin::style::Padding::all(8.0)),
                            )
                            .section("Installation",
                                Text::new("Add burin to your Cargo.toml:\n\nburin = \"0.1\"\n\nThen import the prelude:\n\nuse burin::prelude::*;")
                                    .font_size(12.0).padding(burin::style::Padding::all(8.0)),
                            )
                            .section("Theming",
                                Text::new("Auralis-UI supports both Tailwind CSS v4 and Base design tokens. Themes can be switched at runtime and widgets automatically re-theme.")
                                    .font_size(12.0).padding(burin::style::Padding::all(8.0)),
                            )
                            .on_toggle(move |_idx, _is_open| {
                                tc.set(tc.read() + 1);
                            })
                    })
                    .push(DemoPanel::new()
                        .field("Open", open_str.clone())
                        .field("Toggles", toggle_str.clone())
                        .info("Mode", "single")),
            )
            .push(Text::new("Multi + Animated (click headers independently)").font_size(13.0).font_weight(600))
            .push(
                HStack::new().gap(12.0)
                    .push({
                        let os_multi = Signal::new(HashSet::new());
                        VStack::new().gap(8.0)
                            .push(
                                Accordion::new(os_multi)
                                    .allow_multiple()
                                    .section("Features",
                                        Text::new("✓ Virtual scrolling lists\n✓ Drag-to-reorder\n✓ Context menus with submenus\n✓ Keyboard navigation\n✓ Accessibility support")
                                            .font_size(12.0).padding(burin::style::Padding::all(8.0)),
                                    )
                                    .section("Components",
                                        VStack::new().gap(4.0)
                                            .push(Text::new("Built-in components:").font_size(12.0))
                                            .push(Text::new("  • Button, Checkbox, Switch, Radio").font_size(11.0))
                                            .push(Text::new("  • Slider, NumberInput, TextInput").font_size(11.0))
                                            .push(Text::new("  • Select, ComboBox, DatePicker").font_size(11.0))
                                            .push(Text::new("  • List, Table, Tree").font_size(11.0))
                                            .push(Text::new("  • Modal, Toast, Tooltip, Popover").font_size(11.0))
                                            .padding(burin::style::Padding::all(8.0)),
                                    )
                                    .section("Architecture",
                                        Text::new("Built on ECS + Taffy layout + Signal reactivity. Incremental layout with subtree caching for optimal performance.")
                                            .font_size(12.0)                                            .padding(burin::style::Padding::all(8.0)),
                                    )
                            )
                            .push(Text::new("Click each header to toggle — multiple sections stay open. Opacity animation: 400ms ease-out.").font_size(11.0))
                    }),
            )
    })
}

pub fn toast_section() -> impl Widget {
    Compositor::new(|_scope| {
        let counter = Signal::new(0u32);
        let count_str = Signal::new("0".to_string());

        {
            let cc = counter.clone();
            let cs = count_str.clone();
            auralis_signal::subscribe(
                &counter,
                std::rc::Rc::new(move || {
                    cs.set(cc.read().to_string());
                }),
            );
        }

        // Mount the ToastContainer — it registers as a portal, so it renders above
        // everything regardless of where it sits in the tree.
        let toast = ToastContainer::new();

        VStack::new()
            .gap(8.0)
            .push(toast)
            .push(section_title("Toast / Snackbar"))
            .push(section_sub(
                "FIFO queue, slide-up animation, auto-dismiss (4s)",
            ))
            .push(
                HStack::new()
                    .gap(6.0)
                    .push({
                        let c = counter.clone();
                        Button::new("Info").on_click(move || {
                            toast::show(format!("Info message #{}", c.read()), ToastKind::Info);
                            c.set(c.read() + 1);
                        })
                    })
                    .push({
                        let c = counter.clone();
                        Button::new("Success").success().on_click(move || {
                            toast::show(format!("Success #{}", c.read()), ToastKind::Success);
                            c.set(c.read() + 1);
                        })
                    })
                    .push({
                        let c = counter.clone();
                        Button::new("Warning").warning().on_click(move || {
                            toast::show(format!("Warning #{}", c.read()), ToastKind::Warning);
                            c.set(c.read() + 1);
                        })
                    })
                    .push({
                        let c = counter.clone();
                        Button::new("Error").danger().on_click(move || {
                            toast::show(format!("Error #{}", c.read()), ToastKind::Error);
                            c.set(c.read() + 1);
                        })
                    }),
            )
            .push(
                HStack::new()
                    .gap(6.0)
                    .push({
                        let c = counter.clone();
                        Button::new("With Undo").on_click(move || {
                            let count = c.read();
                            let c_undo = c.clone();
                            toast::show_action(
                                format!("Item {} deleted", count),
                                ToastKind::Info,
                                "Undo",
                                move || {
                                    c_undo.set(count);
                                },
                            );
                            c.set(count + 1);
                        })
                    })
                    .push({
                        let c = counter.clone();
                        Button::new("Long (8s)").on_click(move || {
                            toast::show_duration(
                                format!("Sticky #{}", c.read()),
                                ToastKind::Warning,
                                8000,
                            );
                            c.set(c.read() + 1);
                        })
                    })
                    .push(Button::new("Clear Queue").on_click(|| toast::clear_queue())),
            )
            .push(
                DemoPanel::new()
                    .field("Fired", count_str.clone())
                    .info("Queue", &format!("{} pending", toast::queue_len())),
            )
    })
}
