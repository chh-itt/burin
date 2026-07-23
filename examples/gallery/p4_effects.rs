use burin::core::Widget;
use burin::style::{
    self,
    styled::{BlendMode, Styled},
    CornerRadii, LinearGradient,
};
use burin::widgets::display::Text;
use burin::widgets::layout::*;

use super::section_title;

pub fn gradient_section() -> impl Widget {
    let stops = [
        (style::Color::rgba8(59, 130, 246, 255), 0.0), // blue
        (style::Color::rgba8(168, 85, 247, 255), 0.5), // purple
        (style::Color::rgba8(236, 72, 153, 255), 1.0), // pink
    ];
    VStack::new()
        .gap(8.0)
        .push(section_title(
            "GPU Effects — Gradients (Linear / Radial / Conic)",
        ))
        .push(Text::new("Blue → Purple → Pink, three gradient kinds.").font_size(12.0))
        .push(
            HStack::new()
                .gap(16.0)
                .push(gradient_card(
                    "Linear",
                    LinearGradient::new((0.0, 0.0), (1.0, 1.0), &stops),
                ))
                .push(gradient_card(
                    "Radial",
                    LinearGradient::radial((0.5, 0.5), (1.0, 0.5), &stops),
                ))
                .push(gradient_card(
                    "Conic",
                    LinearGradient::conic((0.5, 0.5), (1.0, 0.5), &stops),
                )),
        )
}

fn gradient_card(label: &'static str, grad: LinearGradient) -> impl Widget {
    VStack::new()
        .gap(6.0)
        .push(Text::new(label).font_size(12.0).font_weight(600))
        .push(
            SizedBox::new()
                .width(150.0)
                .height(100.0)
                .corner_radius(CornerRadii::all(12.0))
                .gradient(grad),
        )
}

pub fn shadow_section() -> impl Widget {
    VStack::new()
        .gap(8.0)
        .push(section_title("P4: GPU Effects — Shadows (SDF)"))
        .push(Text::new("SDF-based box shadows with varying blur radius.").font_size(12.0))
        .push(
            VStack::new()
                .gap(12.0)
                .push(
                    HStack::new()
                        .gap(16.0)
                        .push(shadow_card("Sm (r=4)", 0.0, 3.0, 4.0))
                        .push(shadow_card("Md (r=8)", 0.0, 5.0, 8.0)),
                )
                .push(
                    HStack::new()
                        .gap(16.0)
                        .push(shadow_card("Lg (r=16)", 0.0, 8.0, 16.0))
                        .push(shadow_card("XL (r=24)", 0.0, 12.0, 24.0)),
                ),
        )
        .push(
            VStack::new()
                .gap(12.0)
                .push(
                    HStack::new()
                        .gap(16.0)
                        .push(shadow_card_colored(
                            "Red glow",
                            style::Color::rgba8(255, 60, 60, 200),
                            0.0,
                            0.0,
                            12.0,
                        ))
                        .push(shadow_card_colored(
                            "Blue glow",
                            style::Color::rgba8(60, 120, 255, 200),
                            0.0,
                            0.0,
                            12.0,
                        )),
                )
                .push(
                    HStack::new()
                        .gap(16.0)
                        .push(shadow_card_colored(
                            "Green",
                            style::Color::rgba8(50, 200, 80, 200),
                            0.0,
                            0.0,
                            12.0,
                        ))
                        .push(shadow_card_colored(
                            "Purple",
                            style::Color::rgba8(160, 60, 220, 200),
                            0.0,
                            0.0,
                            12.0,
                        )),
                ),
        )
}

fn shadow_card(label: &'static str, ox: f32, oy: f32, blur: f32) -> impl Widget {
    SizedBox::new()
        .width(140.0)
        .height(80.0)
        .child(Center::new(
            Text::new(label).font_size(13.0).font_weight(600),
        ))
        .background(style::Color::WHITE)
        .corner_radius(CornerRadii::all(8.0))
        .shadow(style::Color::BLACK.with_alpha(0.35), ox, oy, blur)
}

fn shadow_card_colored(
    label: &'static str,
    glow: style::Color,
    ox: f32,
    oy: f32,
    blur: f32,
) -> impl Widget {
    SizedBox::new()
        .width(140.0)
        .height(80.0)
        .child(Center::new(
            Text::new(label)
                .font_size(12.0)
                .font_weight(600)
                .text_color(style::Color::WHITE),
        ))
        .background(style::Color::rgba8(30, 30, 36, 255))
        .corner_radius(CornerRadii::all(8.0))
        .shadow(glow, ox, oy, blur)
}

pub fn blend_mode_section() -> impl Widget {
    VStack::new()
        .gap(8.0)
        .push(section_title("GPU Effects — Blend Modes"))
        .push(Text::new("Each card: colored bg + overlay rect with a blend mode.").font_size(12.0))
        .push(
            Text::new(
                "Corners must be smooth (no black edges). Blend samples a backdrop snapshot.",
            )
            .font_size(11.0),
        )
        .push(
            HStack::new()
                .gap(16.0)
                .push(blend_card("Multiply", BlendMode::Multiply))
                .push(blend_card("Screen", BlendMode::Screen))
                .push(blend_card("Overlay", BlendMode::Overlay)),
        )
}

fn blend_card(label: &'static str, mode: BlendMode) -> impl Widget {
    // Parent SizedBox paints its red-orange background first; the child blend
    // rect renders on top of it (child-after-parent paint order), so the blend
    // samples the red backdrop. (Avoids ZStack, which currently lays children
    // out in flow rather than overlapping.)
    VStack::new()
        .gap(6.0)
        .push(Text::new(label).font_size(12.0).font_weight(600))
        .push(
            SizedBox::new()
                .width(160.0)
                .height(100.0)
                .background(style::Color::rgba8(230, 90, 70, 255))
                .corner_radius(CornerRadii::all(12.0))
                .child(Center::new(
                    SizedBox::new()
                        .width(120.0)
                        .height(64.0)
                        .background(style::Color::rgba8(90, 130, 235, 255))
                        .corner_radius(CornerRadii::all(10.0))
                        .blend_mode(mode),
                )),
        )
}

pub fn gaussian_blur_section() -> impl Widget {
    // Frosted-glass panel over a colored backdrop. The panel is a CHILD of the
    // colored card, so it paints on top of the card's content and blurs it.
    // (Overlap via parent-background/child-on-top, since ZStack does not yet
    // overlap children — tracked as tech debt.)
    VStack::new()
        .gap(8.0)
        .push(section_title("GPU Effects — Backdrop Blur (frosted glass)"))
        .push(Text::new("The centered panel blurs the colored card behind it.").font_size(12.0))
        .push(
            SizedBox::new()
                .width(320.0)
                .height(160.0)
                .background(style::Color::rgba8(235, 90, 70, 255))
                .corner_radius(CornerRadii::all(12.0))
                .child(Center::new(
                    SizedBox::new()
                        .width(220.0)
                        .height(90.0)
                        .corner_radius(CornerRadii::all(14.0))
                        .backdrop_filter(12.0)
                        .child(Center::new(
                            Text::new("Frosted")
                                .font_size(16.0)
                                .font_weight(700)
                                .text_color(style::Color::WHITE),
                        )),
                )),
        )
}

pub fn overview_section() -> impl Widget {
    VStack::new()
        .gap(8.0)
        .push(section_title("P4: GPU Visual Effects Pipeline"))
        .push(Text::new("Fully wired GPU effects:").font_size(12.0))
        .push(Text::new("  SDF Shadows:    working (alpha bug fixed)").font_size(11.0))
        .push(
            Text::new("  Blend Modes:    Multiply/Screen/Overlay, edge-compositing fixed")
                .font_size(11.0),
        )
        .push(
            Text::new("  Backdrop Blur:  infra ready, per-region compositing WIP").font_size(11.0),
        )
        .push(
            Text::new("  Scroll fix:     persistent_dirty now tracks blend + backdrop")
                .font_size(11.0),
        )
}
