//! Unified style application — single entry point for applying
//! ResolvedComponentStyle to an Element, with user override merging.

use crate::core::element::Element;
use crate::style::state_style::StateStyle;
use crate::style::styled::{Shadow, StyleRefinement};
use crate::style::{Color, CornerRadii, Padding};
use crate::theme::m3::roles::*;
use crate::theme::m3::states::VariantStates;

/// Apply a resolved component style to an element, merging user overrides.
///
/// Priority: user StyleRefinement > theme ResolvedComponentStyle.
/// User state_style overrides are fully respected (no recalculation).
///
/// When the user overrides the base background, hover/pressed/focused state
/// colors are re-derived from the effective background instead of using the
/// theme-precomputed values (which were calculated from theme colors).
pub(crate) fn apply_style_to_element(
    el: &mut Element,
    resolved: &ResolvedComponentStyle,
    user_overrides: &StyleRefinement,
    is_dark: bool,
    interaction: f32,
) {
    let common = extract_common_fields(resolved);

    if let Some(bg) = user_overrides.background.or(common.background) {
        el.set_background(bg);
    }
    if let Some(fg) = user_overrides.text_color.or(common.foreground) {
        el.set_foreground(fg);
    }
    if let Some(bd) = user_overrides.border_color.or(common.border) {
        el.set_border_color(bd);
    }
    if let Some(bw) = user_overrides.border_width {
        el.set_border_width(bw);
    } else if common.border.is_some() {
        el.set_border_width(1.0);
    }
    if let Some(fs) = user_overrides.font_size.or(common.font_size) {
        el.set_font_size(fs);
    }
    if let Some(fw) = user_overrides.font_weight.or(common.font_weight) {
        el.set_font_weight(fw);
    }
    if let Some(cr) = user_overrides.corner_radius.or(common.corner_radius) {
        el.set_corner_radii(cr);
    }
    if let Some(h) = user_overrides.height {
        if let crate::style::Dimension::Pixels(px) = h {
            el.set_preferred_height(px);
        }
    } else if let Some(h) = common.height {
        el.set_preferred_height(h);
    }
    if let Some(p) = user_overrides.padding.or(common.padding) {
        el.set_padding(p);
    }

    if common.shadow.is_some() {
        el.set_shadow(common.shadow);
    }

    if let Some(tsl) = common.theme_state_layer {
        let tsl = if user_overrides.background.is_some() {
            let effective_bg = el.background().unwrap_or(Color::TRANSPARENT);
            let effective_fg = el.foreground().unwrap_or(Color::TRANSPARENT);
            let layers = VariantStates::filled(effective_bg, effective_fg, is_dark, interaction);
            ThemeStateLayer {
                hover_bg: Some(layers.hover.background),
                hover_fg: Some(layers.hover.foreground),
                pressed_bg: Some(layers.pressed.background),
                pressed_fg: Some(layers.pressed.foreground),
                focused_bg: Some(layers.focused.background),
                focused_fg: Some(layers.focused.foreground),
                disabled_bg: tsl.disabled_bg,
                disabled_fg: tsl.disabled_fg,
            }
        } else {
            tsl
        };
        merge_state_style(el, &tsl, user_overrides.state_style.as_ref());
    }

    if let Some(sh) = user_overrides.shadow.or(common.shadow) {
        el.set_shadow(Some(sh));
    }
}

struct CommonFields {
    background: Option<Color>,
    foreground: Option<Color>,
    border: Option<Color>,
    font_size: Option<f32>,
    font_weight: Option<u16>,
    corner_radius: Option<CornerRadii>,
    height: Option<f32>,
    padding: Option<Padding>,
    shadow: Option<Shadow>,
    theme_state_layer: Option<ThemeStateLayer>,
}

struct ThemeStateLayer {
    hover_bg: Option<Color>,
    hover_fg: Option<Color>,
    pressed_bg: Option<Color>,
    pressed_fg: Option<Color>,
    focused_bg: Option<Color>,
    focused_fg: Option<Color>,
    disabled_bg: Option<Color>,
    disabled_fg: Option<Color>,
}

fn extract_common_fields(resolved: &ResolvedComponentStyle) -> CommonFields {
    match resolved {
        ResolvedComponentStyle::Button(s) => CommonFields {
            background: Some(s.background),
            foreground: Some(s.foreground),
            border: s.border,
            font_size: Some(s.font_size),
            font_weight: Some(s.font_weight),
            corner_radius: Some(s.corner_radius),
            height: Some(s.height),
            padding: Some(s.padding),
            shadow: None,
            theme_state_layer: Some(ThemeStateLayer {
                hover_bg: Some(s.hover.background),
                hover_fg: Some(s.hover.foreground),
                pressed_bg: Some(s.pressed.background),
                pressed_fg: Some(s.pressed.foreground),
                focused_bg: Some(s.focused.background),
                focused_fg: Some(s.focused.foreground),
                disabled_bg: Some(s.disabled.background),
                disabled_fg: Some(s.disabled.foreground),
            }),
        },
        ResolvedComponentStyle::TextInput(s) => CommonFields {
            background: Some(s.background),
            foreground: Some(s.foreground),
            border: Some(s.border_color),
            font_size: Some(s.font_size),
            font_weight: Some(s.font_weight),
            corner_radius: Some(s.corner_radius),
            height: Some(s.height),
            padding: Some(s.padding),
            shadow: None,
            theme_state_layer: Some(ThemeStateLayer {
                hover_bg: Some(s.hover.background),
                hover_fg: Some(s.hover.foreground),
                pressed_bg: Some(s.pressed.background),
                pressed_fg: Some(s.pressed.foreground),
                focused_bg: Some(s.focused.background),
                focused_fg: Some(s.focused.foreground),
                disabled_bg: Some(s.disabled.background),
                disabled_fg: Some(s.disabled.foreground),
            }),
        },
        ResolvedComponentStyle::Select(s) => CommonFields {
            background: Some(s.trigger_bg),
            foreground: Some(s.trigger_fg),
            border: Some(s.trigger_border),
            font_size: Some(s.font_size),
            font_weight: None,
            corner_radius: Some(s.corner_radius),
            height: Some(s.height),
            padding: None,
            shadow: s.shadow,
            theme_state_layer: None,
        },
        ResolvedComponentStyle::Text(s) => CommonFields {
            background: None,
            foreground: Some(s.foreground),
            border: None,
            font_size: Some(s.font_size),
            font_weight: Some(s.font_weight),
            corner_radius: None,
            height: None,
            padding: None,
            shadow: None,
            theme_state_layer: None,
        },
        ResolvedComponentStyle::Badge(s) => CommonFields {
            background: Some(s.background),
            foreground: Some(s.foreground),
            border: None,
            font_size: Some(s.font_size),
            font_weight: Some(s.font_weight),
            corner_radius: Some(s.corner_radius),
            height: Some(s.height),
            padding: Some(Padding {
                left: s.padding_h,
                right: s.padding_h,
                top: 0.0,
                bottom: 0.0,
            }),
            shadow: None,
            theme_state_layer: None,
        },
        ResolvedComponentStyle::Divider(s) => CommonFields {
            background: Some(s.color),
            foreground: None,
            border: None,
            font_size: None,
            font_weight: None,
            corner_radius: None,
            height: Some(s.thickness),
            padding: None,
            shadow: None,
            theme_state_layer: None,
        },
        ResolvedComponentStyle::Skeleton(s) => CommonFields {
            background: Some(s.background),
            foreground: None,
            border: None,
            font_size: None,
            font_weight: None,
            corner_radius: None,
            height: None,
            padding: None,
            shadow: None,
            theme_state_layer: None,
        },
        ResolvedComponentStyle::Avatar(s) => CommonFields {
            background: None,
            foreground: None,
            border: None,
            font_size: Some(s.font_size),
            font_weight: Some(s.font_weight),
            corner_radius: Some(s.corner_radius),
            height: Some(s.size),
            padding: None,
            shadow: None,
            theme_state_layer: None,
        },
        ResolvedComponentStyle::Icon(s) => CommonFields {
            background: None,
            foreground: Some(s.color),
            border: None,
            font_size: Some(s.size),
            font_weight: None,
            corner_radius: None,
            height: None,
            padding: None,
            shadow: None,
            theme_state_layer: None,
        },
        ResolvedComponentStyle::Tooltip(s) => CommonFields {
            background: Some(s.background),
            foreground: Some(s.foreground),
            border: None,
            font_size: Some(s.font_size),
            font_weight: None,
            corner_radius: Some(s.corner_radius),
            height: None,
            padding: Some(s.padding),
            shadow: None,
            theme_state_layer: None,
        },
        ResolvedComponentStyle::Popover(s) => CommonFields {
            background: Some(s.background),
            foreground: None,
            border: Some(s.border_color),
            font_size: None,
            font_weight: None,
            corner_radius: Some(s.corner_radius),
            height: None,
            padding: None,
            shadow: s.shadow,
            theme_state_layer: None,
        },
        ResolvedComponentStyle::Modal(s) => CommonFields {
            background: Some(s.backdrop_color),
            foreground: None,
            border: None,
            font_size: None,
            font_weight: None,
            corner_radius: None,
            height: None,
            padding: None,
            shadow: None,
            theme_state_layer: None,
        },
        ResolvedComponentStyle::Dialog(s) => CommonFields {
            background: Some(s.background),
            foreground: Some(s.foreground),
            border: Some(s.border_color),
            font_size: Some(s.body_font_size),
            font_weight: None,
            corner_radius: Some(s.corner_radius),
            height: None,
            padding: Some(s.padding),
            shadow: s.shadow,
            theme_state_layer: None,
        },
        ResolvedComponentStyle::Toast(s) => CommonFields {
            background: Some(s.background),
            foreground: Some(s.foreground),
            border: None,
            font_size: Some(s.font_size),
            font_weight: None,
            corner_radius: Some(s.corner_radius),
            height: None,
            padding: None,
            shadow: None,
            theme_state_layer: None,
        },
        ResolvedComponentStyle::Checkbox(s) => CommonFields {
            background: Some(s.unchecked_bg),
            foreground: None,
            border: Some(s.unchecked_border),
            font_size: None,
            font_weight: None,
            corner_radius: Some(s.corner_radius),
            height: Some(s.size),
            padding: None,
            shadow: None,
            theme_state_layer: None,
        },
        ResolvedComponentStyle::Switch(s) => CommonFields {
            background: Some(s.unchecked_bg),
            foreground: None,
            border: None,
            font_size: None,
            font_weight: None,
            corner_radius: None,
            height: Some(s.height),
            padding: None,
            shadow: None,
            theme_state_layer: Some(ThemeStateLayer {
                hover_bg: Some(s.hover_bg),
                hover_fg: Some(s.hover_fg),
                pressed_bg: Some(s.pressed_bg),
                pressed_fg: Some(s.pressed_fg),
                focused_bg: Some(s.focused_bg),
                focused_fg: Some(s.focused_fg),
                disabled_bg: Some(s.disabled_bg),
                disabled_fg: Some(s.disabled_thumb),
            }),
        },
        ResolvedComponentStyle::Radio(s) => CommonFields {
            background: Some(s.unchecked_bg),
            foreground: None,
            border: Some(s.unchecked_border),
            font_size: None,
            font_weight: None,
            corner_radius: None,
            height: Some(s.size),
            padding: None,
            shadow: None,
            theme_state_layer: None,
        },
        ResolvedComponentStyle::Slider(_s) => CommonFields {
            background: None,
            foreground: None,
            border: None,
            font_size: None,
            font_weight: None,
            corner_radius: None,
            height: None,
            padding: None,
            shadow: None,
            theme_state_layer: None,
        },
        ResolvedComponentStyle::ColorPicker(s) => CommonFields {
            background: Some(s.panel_bg),
            foreground: None,
            border: Some(s.panel_border),
            font_size: Some(s.font_size),
            font_weight: None,
            corner_radius: Some(s.corner_radius),
            height: None,
            padding: None,
            shadow: s.shadow,
            theme_state_layer: None,
        },
        ResolvedComponentStyle::Chip(s) => CommonFields {
            background: Some(s.background),
            foreground: Some(s.foreground),
            border: Some(s.border_color),
            font_size: Some(s.font_size),
            font_weight: None,
            corner_radius: Some(s.corner_radius),
            height: Some(s.height),
            padding: Some(Padding {
                left: s.padding_h,
                right: s.padding_h,
                top: 0.0,
                bottom: 0.0,
            }),
            shadow: None,
            theme_state_layer: Some(ThemeStateLayer {
                hover_bg: Some(s.hover_bg),
                hover_fg: Some(s.hover_fg),
                pressed_bg: Some(s.pressed_bg),
                pressed_fg: Some(s.pressed_fg),
                focused_bg: Some(s.focused_bg),
                focused_fg: Some(s.focused_fg),
                disabled_bg: Some(s.disabled.background),
                disabled_fg: Some(s.disabled.foreground),
            }),
        },
        ResolvedComponentStyle::Tab(s) => CommonFields {
            background: Some(s.background),
            foreground: Some(s.foreground),
            border: None,
            font_size: Some(s.font_size),
            font_weight: None,
            corner_radius: Some(s.pill_radius),
            height: Some(s.height),
            padding: None,
            shadow: None,
            theme_state_layer: Some(ThemeStateLayer {
                hover_bg: Some(s.hover_bg),
                hover_fg: Some(s.hover_fg),
                pressed_bg: Some(s.pressed_bg),
                pressed_fg: Some(s.pressed_fg),
                focused_bg: Some(s.focused_bg),
                focused_fg: Some(s.focused_fg),
                disabled_bg: Some(s.disabled.background),
                disabled_fg: Some(s.disabled.foreground),
            }),
        },
        ResolvedComponentStyle::MenuItem(s) => CommonFields {
            background: Some(s.background),
            foreground: Some(s.foreground),
            border: None,
            font_size: Some(s.font_size),
            font_weight: None,
            corner_radius: None,
            height: Some(s.height),
            padding: Some(s.padding),
            shadow: None,
            theme_state_layer: None,
        },
        ResolvedComponentStyle::ListItem(s) => CommonFields {
            background: Some(s.background),
            foreground: Some(s.foreground),
            border: None,
            font_size: Some(s.font_size),
            font_weight: None,
            corner_radius: None,
            height: Some(s.height),
            padding: Some(s.padding),
            shadow: None,
            theme_state_layer: None,
        },
        ResolvedComponentStyle::TreeItem(s) => CommonFields {
            background: Some(s.background),
            foreground: Some(s.foreground),
            border: None,
            font_size: Some(s.font_size),
            font_weight: None,
            corner_radius: None,
            height: Some(s.height),
            padding: None,
            shadow: None,
            theme_state_layer: None,
        },
        ResolvedComponentStyle::Progress(s) => CommonFields {
            background: Some(s.track_color),
            foreground: None,
            border: None,
            font_size: None,
            font_weight: None,
            corner_radius: Some(s.corner_radius),
            height: Some(s.height),
            padding: None,
            shadow: None,
            theme_state_layer: None,
        },
    }
}

/// Merge theme state layer defaults with user state_style overrides.
///
/// Priority chain:
/// 1. User explicit state_style.{} — fully respected, not overwritten
/// 2. Theme default state layer — applied only where user didn't set
fn merge_state_style(
    el: &mut Element,
    theme: &ThemeStateLayer,
    user_state_style: Option<&StateStyle>,
) {
    el.with_state_style(|ss| {
        let has_user_hover_bg = user_state_style
            .and_then(|u| u.hovered.background)
            .is_some();
        let has_user_pressed_bg = user_state_style
            .and_then(|u| u.pressed.background)
            .is_some();
        let has_user_disabled_bg = user_state_style
            .and_then(|u| u.disabled.background)
            .is_some();

        if !has_user_hover_bg {
            if let Some(bg) = theme.hover_bg {
                ss.hovered.background = Some(bg);
            }
            if let Some(fg) = theme.hover_fg {
                ss.hovered.foreground = Some(fg);
            }
        }
        if !has_user_pressed_bg {
            if let Some(bg) = theme.pressed_bg {
                ss.pressed.background = Some(bg);
            }
            if let Some(fg) = theme.pressed_fg {
                ss.pressed.foreground = Some(fg);
            }
        }
        if !has_user_disabled_bg {
            if let Some(bg) = theme.disabled_bg {
                ss.disabled.background = Some(bg);
            }
            if let Some(fg) = theme.disabled_fg {
                ss.disabled.foreground = Some(fg);
            }
        }
        if let Some(bg) = theme.focused_bg {
            if user_state_style
                .and_then(|u| u.focused.background)
                .is_none()
            {
                ss.focused.background = Some(bg);
            }
        }
        if let Some(fg) = theme.focused_fg {
            if user_state_style
                .and_then(|u| u.focused.foreground)
                .is_none()
            {
                ss.focused.foreground = Some(fg);
            }
        }
    });
}
