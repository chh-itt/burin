//! M3 State Layer system — precomputed interaction color states.
//!
//! All hover/pressed/focused colors are computed once at Scheme construction
//! time. Widgets read precomputed LayerColor values with zero runtime cost.

use crate::style::Color;

/// All interaction states for a single intent color.
#[derive(Clone, Debug)]
pub struct IntentStates {
    pub filled: VariantStates,
    pub outlined: VariantStates,
    pub text: VariantStates,
}

/// Interaction states for one appearance variant.
#[derive(Clone, Debug)]
pub struct VariantStates {
    pub base: LayerColor,
    pub hover: LayerColor,
    pub pressed: LayerColor,
    pub focused: LayerColor,
}

/// Fully resolved color pair for a single interaction layer.
#[derive(Clone, Copy, Debug)]
pub struct LayerColor {
    pub background: Color,
    pub foreground: Color,
    pub border: Option<Color>,
}

/// Global disabled color set.
#[derive(Clone, Debug)]
pub struct DisabledColors {
    pub background: Color,
    pub foreground: Color,
    pub border: Color,
}

impl IntentStates {
    /// Precompute all interaction states from container color + on-container color.
    pub fn new(container: Color, on_container: Color, is_dark: bool, interaction: f32) -> Self {
        Self {
            filled: VariantStates::filled(container, on_container, is_dark, interaction),
            outlined: VariantStates::outlined(container, is_dark, interaction),
            text: VariantStates::text(container, is_dark, interaction),
        }
    }
}

impl VariantStates {
    pub fn filled(container: Color, on_container: Color, is_dark: bool, interaction: f32) -> Self {
        let overlay = overlay_color(is_dark);
        let hover_opacity = 0.02 + interaction * 0.12;
        let press_opacity = 0.04 + interaction * 0.14;
        let focus_opacity = 0.06 + interaction * 0.10;
        let hover_bg = blend_overlay(container, overlay, hover_opacity);
        let pressed_bg = blend_overlay(container, overlay, press_opacity);
        let focused_bg = blend_overlay(container, overlay, focus_opacity);
        Self {
            base: LayerColor {
                background: container,
                foreground: on_container,
                border: None,
            },
            hover: LayerColor {
                background: hover_bg,
                foreground: stable_foreground(hover_bg, on_container),
                border: None,
            },
            pressed: LayerColor {
                background: pressed_bg,
                foreground: stable_foreground(pressed_bg, on_container),
                border: None,
            },
            focused: LayerColor {
                background: focused_bg,
                foreground: on_container,
                border: Some(container),
            },
        }
    }

    fn outlined(container: Color, is_dark: bool, interaction: f32) -> Self {
        let hover_opacity = if is_dark {
            0.06 + interaction * 0.04
        } else {
            0.02 + interaction * 0.12
        };
        let press_opacity = if is_dark {
            0.08 + interaction * 0.06
        } else {
            0.04 + interaction * 0.14
        };
        let focus_opacity = 0.06 + interaction * 0.10;
        let hover_bg = blend_overlay(Color::TRANSPARENT, container, hover_opacity);
        let pressed_bg = blend_overlay(Color::TRANSPARENT, container, press_opacity);
        let focused_bg = blend_overlay(Color::TRANSPARENT, container, focus_opacity);
        Self {
            base: LayerColor {
                background: Color::TRANSPARENT,
                foreground: container,
                border: Some(container),
            },
            hover: LayerColor {
                background: hover_bg,
                foreground: container,
                border: Some(container),
            },
            pressed: LayerColor {
                background: pressed_bg,
                foreground: container,
                border: Some(container),
            },
            focused: LayerColor {
                background: focused_bg,
                foreground: container,
                border: Some(container),
            },
        }
    }

    fn text(container: Color, is_dark: bool, interaction: f32) -> Self {
        let hover_opacity = if is_dark {
            0.06 + interaction * 0.04
        } else {
            0.02 + interaction * 0.12
        };
        let press_opacity = if is_dark {
            0.08 + interaction * 0.06
        } else {
            0.04 + interaction * 0.14
        };
        let focus_opacity = 0.06 + interaction * 0.10;
        let hover_bg = blend_overlay(Color::TRANSPARENT, container, hover_opacity);
        let pressed_bg = blend_overlay(Color::TRANSPARENT, container, press_opacity);
        let focused_bg = blend_overlay(Color::TRANSPARENT, container, focus_opacity);
        Self {
            base: LayerColor {
                background: Color::TRANSPARENT,
                foreground: container,
                border: None,
            },
            hover: LayerColor {
                background: hover_bg,
                foreground: container,
                border: None,
            },
            pressed: LayerColor {
                background: pressed_bg,
                foreground: container,
                border: None,
            },
            focused: LayerColor {
                background: focused_bg,
                foreground: container,
                border: None,
            },
        }
    }
}

impl DisabledColors {
    /// M3 disabled spec: on_surface at 38% opacity for fg, 12% for border.
    pub fn new(on_surface: Color, surface: Color, is_dark: bool) -> Self {
        let overlay = overlay_color(is_dark);
        Self {
            foreground: on_surface.with_alpha(0.38),
            background: blend_overlay(surface, overlay, 0.12),
            border: on_surface.with_alpha(0.12),
        }
    }
}

/// Blends an overlay color at the given opacity onto a base color.
pub fn blend_overlay(base: Color, overlay: Color, opacity: f32) -> Color {
    let oa = overlay.a * opacity;
    let ba = base.a;
    let out_a = oa + ba * (1.0 - oa);
    if out_a <= 0.0 {
        return Color::TRANSPARENT;
    }
    Color {
        r: (overlay.r * oa + base.r * ba * (1.0 - oa)) / out_a,
        g: (overlay.g * oa + base.g * ba * (1.0 - oa)) / out_a,
        b: (overlay.b * oa + base.b * ba * (1.0 - oa)) / out_a,
        a: out_a,
    }
}

/// Stable foreground color selection with hysteresis to prevent flickering.
///
/// 1. If current foreground meets WCAG AA (4.5:1), keep it.
/// 2. Choose black or white — whichever has higher contrast.
/// 3. If contrast difference < 0.8, preserve current color's lightness to prevent oscillation.
pub fn stable_foreground(bg: Color, current_fg: Color) -> Color {
    if current_fg.contrast_ratio(&bg) >= 4.5 {
        return current_fg;
    }
    let white_ratio = Color::WHITE.contrast_ratio(&bg);
    let black_ratio = Color::BLACK.contrast_ratio(&bg);
    if (white_ratio - black_ratio).abs() < 0.8 {
        return if current_fg.relative_luminance() > 0.5 {
            Color::WHITE
        } else {
            Color::BLACK
        };
    }
    if white_ratio > black_ratio {
        Color::WHITE
    } else {
        Color::BLACK
    }
}

fn overlay_color(is_dark: bool) -> Color {
    if is_dark {
        Color::WHITE
    } else {
        Color::BLACK
    }
}
