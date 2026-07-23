use crate::gallery::demo_panel::DemoPanel;
use crate::gallery::{section_sub, section_title};
use auralis_signal::Signal;
use burin::core::{Compositor, Widget};
use burin::style::{Color, CornerRadii, Padding as PaddingStyle, Styled};
use burin::widgets::display::Text;
use burin::widgets::input::Button;
use burin::widgets::layout::*;

pub fn sized_box_section() -> impl Widget {
    Compositor::new(|_scope| {
        let width_sig = Signal::new("100".to_string());
        let height_sig = Signal::new("60".to_string());

        VStack::new()
            .gap(8.0)
            .push(section_title("SizedBox  G1"))
            .push(section_sub(
                "Fixed-size box. Sets exact width/height for its child.",
            ))
            .push(
                HStack::new()
                    .gap(12.0)
                    .push(
                        VStack::new()
                            .gap(8.0)
                            .push(Text::new("100×60").font_size(13.0))
                            .push(
                                SizedBox::new()
                                    .width(100.0)
                                    .height(60.0)
                                    .background(Color::rgba8(59, 130, 246, 255))
                                    .corner_radius(CornerRadii::all(4.0))
                                    .child(Center::new(Text::new("100×60").font_size(12.0))),
                            ),
                    )
                    .push(
                        VStack::new()
                            .gap(8.0)
                            .push(Text::new("200×40").font_size(13.0))
                            .push(
                                SizedBox::new()
                                    .width(200.0)
                                    .height(40.0)
                                    .background(Color::rgba8(34, 197, 94, 255))
                                    .corner_radius(CornerRadii::all(4.0))
                                    .child(Center::new(Text::new("200×40").font_size(12.0))),
                            ),
                    )
                    .push(
                        VStack::new()
                            .gap(8.0)
                            .push(Text::new("Auto height, 120px width").font_size(13.0))
                            .push(
                                SizedBox::new()
                                    .width(120.0)
                                    .background(Color::rgba8(249, 115, 22, 255))
                                    .corner_radius(CornerRadii::all(4.0))
                                    .child(Center::new(Text::new("120px").font_size(12.0))),
                            ),
                    )
                    .push(
                        DemoPanel::new()
                            .field("Width", width_sig.clone())
                            .field("Height", height_sig.clone())
                            .info("Style", "bg + radius"),
                    ),
            )
    })
}

pub fn spacer_section() -> impl Widget {
    Compositor::new(|_scope| {
        VStack::new()
            .gap(8.0)
            .push(section_title("Spacer  G1"))
            .push(section_sub(
                "Fills remaining space in a flex layout. Pushes siblings apart.",
            ))
            .push(
                VStack::new()
                    .gap(4.0)
                    .push(Text::new("Horizontal — [Left] ← Spacer → [Right]").font_size(13.0))
                    .push(
                        HStack::new()
                            .height(40.0)
                            .push(
                                SizedBox::new()
                                    .width(80.0)
                                    .height(40.0)
                                    .background(Color::rgba8(59, 130, 246, 255))
                                    .corner_radius(CornerRadii::all(4.0))
                                    .child(Center::new(Text::new("Left").font_size(12.0))),
                            )
                            .push(Spacer::new())
                            .push(
                                SizedBox::new()
                                    .width(80.0)
                                    .height(40.0)
                                    .background(Color::rgba8(34, 197, 94, 255))
                                    .corner_radius(CornerRadii::all(4.0))
                                    .child(Center::new(Text::new("Right").font_size(12.0))),
                            ),
                    ),
            )
            .push(
                VStack::new()
                    .gap(4.0)
                    .push(Text::new("Vertical — [Top] ← Spacer → [Bottom]").font_size(13.0))
                    .push(
                        VStack::new()
                            .height(120.0)
                            .push(
                                SizedBox::new()
                                    .width(80.0)
                                    .height(40.0)
                                    .background(Color::rgba8(249, 115, 22, 255))
                                    .corner_radius(CornerRadii::all(4.0))
                                    .child(Center::new(Text::new("Top").font_size(12.0))),
                            )
                            .push(Spacer::new())
                            .push(
                                SizedBox::new()
                                    .width(80.0)
                                    .height(40.0)
                                    .background(Color::rgba8(139, 92, 246, 255))
                                    .corner_radius(CornerRadii::all(4.0))
                                    .child(Center::new(Text::new("Bottom").font_size(12.0))),
                            ),
                    ),
            )
    })
}

pub fn padding_section() -> impl Widget {
    Compositor::new(|_scope| {
        VStack::new()
            .gap(8.0)
            .push(section_title("Padding  G1"))
            .push(section_sub(
                "Adds space around its child. Padding values: all, symmetric, or per-side.",
            ))
            .push(
                HStack::new()
                    .gap(12.0)
                    .push(
                        VStack::new()
                            .gap(4.0)
                            .push(
                                Text::new("Padding::all(12) — blue outer, green inner")
                                    .font_size(13.0),
                            )
                            .push(
                                SizedBox::new()
                                    .width(250.0)
                                    .height(80.0)
                                    .background(Color::rgba8(59, 130, 246, 255))
                                    .corner_radius(CornerRadii::all(4.0))
                                    .child(Padding::new(
                                        PaddingStyle::all(12.0),
                                        SizedBox::new()
                                            .background(Color::rgba8(34, 197, 94, 255))
                                            .corner_radius(CornerRadii::all(4.0))
                                            .child(Center::new(
                                                Text::new("all(12)").font_size(12.0),
                                            )),
                                    )),
                            ),
                    )
                    .push(
                        VStack::new()
                            .gap(4.0)
                            .push(Text::new("Padding::symmetric(h=20,v=8)").font_size(13.0))
                            .push(
                                SizedBox::new()
                                    .width(250.0)
                                    .height(80.0)
                                    .background(Color::rgba8(249, 115, 22, 255))
                                    .corner_radius(CornerRadii::all(4.0))
                                    .child(Padding::new(
                                        PaddingStyle::symmetric(20.0, 8.0),
                                        SizedBox::new()
                                            .background(Color::rgba8(139, 92, 246, 255))
                                            .corner_radius(CornerRadii::all(4.0))
                                            .child(Center::new(
                                                Text::new("h=20 v=8").font_size(12.0),
                                            )),
                                    )),
                            ),
                    ),
            )
    })
}

pub fn center_section() -> impl Widget {
    Compositor::new(|_scope| {
        VStack::new()
            .gap(8.0)
            .push(section_title("Center  G1"))
            .push(section_sub(
                "Centers its child both horizontally and vertically.",
            ))
            .push(
                HStack::new().gap(12.0).push(
                    VStack::new()
                        .gap(4.0)
                        .push(
                            Text::new("Center fixes child centered in a 200×80 box")
                                .font_size(13.0),
                        )
                        .push(
                            SizedBox::new()
                                .width(200.0)
                                .height(80.0)
                                .background(Color::rgba8(59, 130, 246, 255))
                                .corner_radius(CornerRadii::all(4.0))
                                .child(Center::new(
                                    SizedBox::new()
                                        .width(120.0)
                                        .height(40.0)
                                        .background(Color::rgba8(34, 197, 94, 255))
                                        .corner_radius(CornerRadii::all(4.0))
                                        .child(Center::new(Text::new("centered").font_size(14.0))),
                                )),
                        ),
                ),
            )
    })
}

pub fn opacity_section() -> impl Widget {
    Compositor::new(|_scope| {
        VStack::new()
            .gap(8.0)
            .push(section_title("Opacity  G1"))
            .push(section_sub(
                "Adjusts the opacity (transparency) of its child subtree.",
            ))
            .push(
                HStack::new()
                    .gap(12.0)
                    .push(
                        VStack::new()
                            .gap(4.0)
                            .push(Text::new("opacity: 1.0").font_size(13.0))
                            .push(Opacity::new(
                                1.0,
                                SizedBox::new()
                                    .width(80.0)
                                    .height(60.0)
                                    .background(Color::rgba8(59, 130, 246, 255))
                                    .corner_radius(CornerRadii::all(4.0))
                                    .child(Center::new(Text::new("1.0").font_size(12.0))),
                            )),
                    )
                    .push(
                        VStack::new()
                            .gap(4.0)
                            .push(Text::new("opacity: 0.6").font_size(13.0))
                            .push(Opacity::new(
                                0.6,
                                SizedBox::new()
                                    .width(80.0)
                                    .height(60.0)
                                    .background(Color::rgba8(59, 130, 246, 255))
                                    .corner_radius(CornerRadii::all(4.0))
                                    .child(Center::new(Text::new("0.6").font_size(12.0))),
                            )),
                    )
                    .push(
                        VStack::new()
                            .gap(4.0)
                            .push(Text::new("opacity: 0.3").font_size(13.0))
                            .push(Opacity::new(
                                0.3,
                                SizedBox::new()
                                    .width(80.0)
                                    .height(60.0)
                                    .background(Color::rgba8(59, 130, 246, 255))
                                    .corner_radius(CornerRadii::all(4.0))
                                    .child(Center::new(Text::new("0.3").font_size(12.0))),
                            )),
                    ),
            )
    })
}

pub fn expanded_section() -> impl Widget {
    Compositor::new(|_scope| {
        VStack::new()
            .gap(8.0)
            .push(section_title("Expanded  G1"))
            .push(section_sub(
                "Fills available space in a flex layout. flex_grow=1, flex_shrink=1.",
            ))
            .push(
                HStack::new()
                    .gap(4.0)
                    .height(60.0)
                    .push(
                        SizedBox::new()
                            .width(60.0)
                            .background(Color::rgba8(59, 130, 246, 255))
                            .corner_radius(CornerRadii::all(4.0))
                            .child(Center::new(Text::new("60px").font_size(12.0))),
                    )
                    .push(Expanded::new(
                        SizedBox::new()
                            .background(Color::rgba8(34, 197, 94, 255))
                            .corner_radius(CornerRadii::all(4.0))
                            .child(Center::new(Text::new("Expanded").font_size(12.0))),
                    ))
                    .push(
                        SizedBox::new()
                            .width(80.0)
                            .background(Color::rgba8(249, 115, 22, 255))
                            .corner_radius(CornerRadii::all(4.0))
                            .child(Center::new(Text::new("80px").font_size(12.0))),
                    ),
            )
    })
}

pub fn flexible_section() -> impl Widget {
    Compositor::new(|_scope| {
        VStack::new()
            .gap(8.0)
            .push(section_title("Flexible  G1"))
            .push(section_sub(
                "Proportional flex-grow. flex=2 gets twice the space of flex=1.",
            ))
            .push(
                HStack::new()
                    .gap(4.0)
                    .height(60.0)
                    .push(Flexible::new(
                        1.0,
                        SizedBox::new()
                            .background(Color::rgba8(59, 130, 246, 255))
                            .corner_radius(CornerRadii::all(4.0))
                            .child(Center::new(Text::new("flex=1").font_size(12.0))),
                    ))
                    .push(Flexible::new(
                        2.0,
                        SizedBox::new()
                            .background(Color::rgba8(34, 197, 94, 255))
                            .corner_radius(CornerRadii::all(4.0))
                            .child(Center::new(Text::new("flex=2").font_size(12.0))),
                    ))
                    .push(Flexible::new(
                        3.0,
                        SizedBox::new()
                            .background(Color::rgba8(249, 115, 22, 255))
                            .corner_radius(CornerRadii::all(4.0))
                            .child(Center::new(Text::new("flex=3").font_size(12.0))),
                    )),
            )
    })
}

pub fn grid_section() -> impl Widget {
    Compositor::new(|_scope| {
        VStack::new()
            .gap(8.0)
            .push(section_title("Grid  G1"))
            .push(section_sub(
                "CSS Grid layout with fixed column count. Items span and offset.",
            ))
            .push(
                GridRow::new()
                    .columns(4)
                    .gap(4.0)
                    .push(GridItem::new(
                        SizedBox::new()
                            .height(40.0)
                            .background(Color::rgba8(59, 130, 246, 255))
                            .corner_radius(CornerRadii::all(4.0))
                            .child(Center::new(Text::new("1").font_size(14.0))),
                    ))
                    .push(
                        GridItem::new(
                            SizedBox::new()
                                .height(40.0)
                                .background(Color::rgba8(34, 197, 94, 255))
                                .corner_radius(CornerRadii::all(4.0))
                                .child(Center::new(Text::new("2").font_size(14.0))),
                        )
                        .cols(2),
                    )
                    .push(GridItem::new(
                        SizedBox::new()
                            .height(40.0)
                            .background(Color::rgba8(249, 115, 22, 255))
                            .corner_radius(CornerRadii::all(4.0))
                            .child(Center::new(Text::new("3").font_size(14.0))),
                    ))
                    .push(GridItem::new(
                        SizedBox::new()
                            .height(40.0)
                            .background(Color::rgba8(34, 197, 94, 255))
                            .corner_radius(CornerRadii::all(4.0))
                            .child(Center::new(Text::new("4").font_size(14.0))),
                    ))
                    .push(
                        GridItem::new(
                            SizedBox::new()
                                .height(40.0)
                                .background(Color::rgba8(139, 92, 246, 255))
                                .corner_radius(CornerRadii::all(4.0))
                                .child(Center::new(Text::new("5→7 cols=3").font_size(12.0))),
                        )
                        .cols(3),
                    )
                    .push(GridItem::new(
                        SizedBox::new()
                            .height(40.0)
                            .background(Color::rgba8(236, 72, 153, 255))
                            .corner_radius(CornerRadii::all(4.0))
                            .child(Center::new(Text::new("6").font_size(14.0))),
                    )),
            )
    })
}

pub fn conditional_section() -> impl Widget {
    Compositor::new(|_scope| {
        let show = Signal::new(true);
        VStack::new()
            .gap(8.0)
            .push(section_title("Conditional  G1"))
            .push(section_sub(
                "Shows one child or the other based on a Signal<bool>. Uses slot_inactive.",
            ))
            .push(
                HStack::new().gap(12.0).push(
                    VStack::new()
                        .gap(4.0)
                        .push(Text::new("Toggle with button below").font_size(13.0))
                        .push(Conditional::new(
                            show.clone(),
                            SizedBox::new()
                                .width(200.0)
                                .height(60.0)
                                .background(Color::rgba8(34, 197, 94, 255))
                                .corner_radius(CornerRadii::all(4.0))
                                .child(Center::new(
                                    Text::new("TRUE BRANCH").font_size(14.0).font_weight(700),
                                )),
                            SizedBox::new()
                                .width(200.0)
                                .height(60.0)
                                .background(Color::rgba8(239, 68, 68, 255))
                                .corner_radius(CornerRadii::all(4.0))
                                .child(Center::new(
                                    Text::new("FALSE BRANCH").font_size(14.0).font_weight(700),
                                )),
                        ))
                        .push({
                            let s = show.clone();
                            Button::new("Toggle")
                                .small()
                                .on_click(move || s.update(|v| *v = !*v))
                        }),
                ),
            )
    })
}

pub fn split_pane_section() -> impl Widget {
    Compositor::new(|_scope| {
        VStack::new()
            .gap(8.0)
            .push(section_title("SplitPane  G3"))
            .push(section_sub(
                "Resizable split pane. Drag the divider or use Arrow keys when focused.",
            ))
            .push(
                SizedBox::new().width(500.0).height(120.0).child(
                    SplitPane::new(
                        SizedBox::new()
                            .background(Color::rgba8(59, 130, 246, 255))
                            .child(Center::new(Text::new("Left pane").font_size(14.0))),
                        SizedBox::new()
                            .background(Color::rgba8(34, 197, 94, 255))
                            .child(Center::new(Text::new("Right pane").font_size(14.0))),
                    )
                    .split_ratio(0.4)
                    .min_sizes(60.0, 60.0),
                ),
            )
            .push(
                SizedBox::new().width(500.0).height(120.0).child(
                    SplitPane::new(
                        SizedBox::new()
                            .background(Color::rgba8(249, 115, 22, 255))
                            .child(Center::new(Text::new("Top pane").font_size(14.0))),
                        SizedBox::new()
                            .background(Color::rgba8(139, 92, 246, 255))
                            .child(Center::new(Text::new("Bottom pane").font_size(14.0))),
                    )
                    .direction(SplitDirection::Vertical)
                    .split_ratio(0.3)
                    .min_sizes(40.0, 40.0),
                ),
            )
    })
}
