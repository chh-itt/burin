//! M3 Theme core — DynamicColorScheme and component resolution.

pub mod elevation;
pub mod engine;
pub mod presets;
pub mod roles;
pub mod shape;
pub mod states;
pub mod typescale;

use crate::style::{Color, Padding};
use crate::theme::m3::elevation::ElevationLevel;
use crate::theme::m3::engine::scheme_color_to_rgba;
use crate::theme::m3::roles::*;
use crate::theme::m3::shape::ShapeLevel;
use crate::theme::m3::states::{DisabledColors, IntentStates, VariantStates};
use crate::theme::m3::typescale::{M3TypeToken, Typescale};
use crate::theme::{Appearance, ControlShape, ControlSize, Intent};

/// The full M3 dynamic color scheme — 30+ color roles + precomputed state layers.
/// Design parameters (design_*) control the visual personality. Default = Refined.
#[derive(Clone, Debug)]
pub struct DynamicColorScheme {
    pub primary: Color,
    pub on_primary: Color,
    pub primary_container: Color,
    pub on_primary_container: Color,
    pub primary_fixed: Color,
    pub on_primary_fixed: Color,
    pub primary_fixed_dim: Color,
    pub on_primary_fixed_variant: Color,
    pub secondary: Color,
    pub on_secondary: Color,
    pub secondary_container: Color,
    pub on_secondary_container: Color,
    pub tertiary: Color,
    pub on_tertiary: Color,
    pub tertiary_container: Color,
    pub on_tertiary_container: Color,
    pub error: Color,
    pub on_error: Color,
    pub error_container: Color,
    pub on_error_container: Color,
    pub surface_dim: Color,
    pub surface: Color,
    pub surface_bright: Color,
    pub surface_container_lowest: Color,
    pub surface_container_low: Color,
    pub surface_container: Color,
    pub surface_container_high: Color,
    pub surface_container_highest: Color,
    pub on_surface: Color,
    pub on_surface_variant: Color,
    pub outline: Color,
    pub outline_variant: Color,
    pub inverse_surface: Color,
    pub inverse_on_surface: Color,
    pub inverse_primary: Color,
    pub shadow: Color,
    pub scrim: Color,
    pub surface_tint: Color,
    pub primary_states: IntentStates,
    pub secondary_states: IntentStates,
    pub tertiary_states: IntentStates,
    pub error_states: IntentStates,
    pub disabled: DisabledColors,
    pub typescale: Typescale,
    pub is_dark: bool,
    pub design_warmth: f32,
    pub design_radius: f32,
    pub design_depth: f32,
    pub design_accent: f32,
    pub design_interaction: f32,
    pub design_contrast: f32,
    pub design_density: f32,
    pub design_border_presence: f32,
    pub design_surface_variance: f32,
    pub design_typescale_contrast: f32,
    pub design_font_weight_contrast: f32,
}

/// Bundled design parameters. Pass to `from_mc_scheme_with_design` or use presets.
#[derive(Clone, Copy, Debug)]
pub struct DesignParams {
    pub warmth: f32,
    pub radius: f32,
    pub depth: f32,
    pub accent: f32,
    pub interaction: f32,
    pub contrast: f32,
    pub density: f32,
    pub border_presence: f32,
    pub surface_variance: f32,
    pub typescale_contrast: f32,
    pub font_weight_contrast: f32,
}

impl DesignParams {
    pub fn refined() -> Self {
        Self {
            warmth: 0.65,
            radius: 0.35,
            depth: 0.20,
            accent: 0.25,
            interaction: 0.30,
            contrast: 0.55,
            density: 0.60,
            border_presence: 0.25,
            surface_variance: 0.15,
            typescale_contrast: 0.50,
            font_weight_contrast: 0.45,
        }
    }

    pub fn m3_classic() -> Self {
        Self {
            warmth: 0.40,
            radius: 0.50,
            depth: 0.55,
            accent: 1.00,
            interaction: 0.65,
            contrast: 0.65,
            density: 0.45,
            border_presence: 0.45,
            surface_variance: 0.60,
            typescale_contrast: 0.60,
            font_weight_contrast: 0.55,
        }
    }

    /// Neo-Minimal: cool-toned, ultra-soft, near-monochrome.
    ///
    /// Designed for the "新极简主义 + Soft 柔和" aesthetic:
    /// - Near-zero accent (off-black/off-white fills)
    /// - Ultra-subtle interaction feedback
    /// - Low contrast across all surfaces
    /// - High surface variance to compensate for flat color palette
    /// - Generous corner radius
    pub fn neo_minimal() -> Self {
        Self {
            warmth: 0.25,
            radius: 0.55,
            depth: 0.12,
            accent: 0.10,
            interaction: 0.12,
            contrast: 0.30,
            density: 0.50,
            border_presence: 0.06,
            surface_variance: 0.28,
            typescale_contrast: 0.35,
            font_weight_contrast: 0.30,
        }
    }
}

pub use engine::SchemeVariant;
/// Re-export for convenience — preset themes use these types directly.
pub use presets::PresetTheme;

impl DynamicColorScheme {
    pub fn from_mc_scheme(
        scheme: &material_colors::dynamic_color::DynamicScheme,
        is_dark: bool,
    ) -> Self {
        Self::from_mc_scheme_with_design(scheme, is_dark, DesignParams::refined())
    }

    pub fn from_mc_scheme_with_design(
        scheme: &material_colors::dynamic_color::DynamicScheme,
        is_dark: bool,
        dp: DesignParams,
    ) -> Self {
        use scheme_color_to_rgba as c;
        let primary = c(scheme.primary());
        let on_primary = c(scheme.on_primary());
        let secondary = c(scheme.secondary());
        let on_secondary = c(scheme.on_secondary());
        let surface = c(scheme.surface());
        let on_surface = c(scheme.on_surface());
        let error = c(scheme.error());
        let on_error = c(scheme.on_error());

        let interaction = dp.interaction;
        let primary_states = IntentStates::new(primary, on_primary, is_dark, interaction);
        let secondary_states = IntentStates::new(secondary, on_secondary, is_dark, interaction);
        let tertiary_states = IntentStates::new(
            c(scheme.tertiary()),
            c(scheme.on_tertiary()),
            is_dark,
            interaction,
        );
        let error_states = IntentStates::new(error, on_error, is_dark, interaction);

        let disabled = DisabledColors::new(on_surface, surface, is_dark);

        Self {
            primary,
            on_primary,
            primary_container: c(scheme.primary_container()),
            on_primary_container: c(scheme.on_primary_container()),
            primary_fixed: c(scheme.primary_fixed()),
            on_primary_fixed: c(scheme.on_primary_fixed()),
            primary_fixed_dim: c(scheme.primary_fixed_dim()),
            on_primary_fixed_variant: c(scheme.on_primary_fixed_variant()),
            secondary,
            on_secondary,
            secondary_container: c(scheme.secondary_container()),
            on_secondary_container: c(scheme.on_secondary_container()),
            tertiary: c(scheme.tertiary()),
            on_tertiary: c(scheme.on_tertiary()),
            tertiary_container: c(scheme.tertiary_container()),
            on_tertiary_container: c(scheme.on_tertiary_container()),
            error,
            on_error,
            error_container: c(scheme.error_container()),
            on_error_container: c(scheme.on_error_container()),
            surface_dim: c(scheme.surface_dim()),
            surface,
            surface_bright: c(scheme.surface_bright()),
            surface_container_lowest: c(scheme.surface_container_lowest()),
            surface_container_low: c(scheme.surface_container_low()),
            surface_container: c(scheme.surface_container()),
            surface_container_high: c(scheme.surface_container_high()),
            surface_container_highest: c(scheme.surface_container_highest()),
            on_surface,
            on_surface_variant: c(scheme.on_surface_variant()),
            outline: c(scheme.outline()),
            outline_variant: c(scheme.outline_variant()),
            inverse_surface: c(scheme.inverse_surface()),
            inverse_on_surface: c(scheme.inverse_on_surface()),
            inverse_primary: c(scheme.inverse_primary()),
            shadow: c(scheme.shadow()),
            scrim: c(scheme.scrim()),
            surface_tint: c(scheme.surface_tint()),
            primary_states,
            secondary_states,
            tertiary_states,
            error_states,
            disabled,
            typescale: Typescale::default(),
            is_dark,
            design_warmth: dp.warmth,
            design_radius: dp.radius,
            design_depth: dp.depth,
            design_accent: dp.accent,
            design_interaction: dp.interaction,
            design_contrast: dp.contrast,
            design_density: dp.density,
            design_border_presence: dp.border_presence,
            design_surface_variance: dp.surface_variance,
            design_typescale_contrast: dp.typescale_contrast,
            design_font_weight_contrast: dp.font_weight_contrast,
        }
    }

    /// Resolve a ComponentRole to its ResolvedComponentStyle.
    pub fn resolve_component(&self, role: &ComponentRole) -> ResolvedComponentStyle {
        match role {
            ComponentRole::Interactive(ir) => self.resolve_interactive(ir),
            ComponentRole::Display(dr) => self.resolve_display(dr),
        }
    }

    fn resolve_interactive(&self, role: &InteractiveRole) -> ResolvedComponentStyle {
        match role {
            InteractiveRole::Button {
                intent,
                appearance,
                size,
                shape,
            } => {
                let states = self.intent_states_for(*intent);
                let variant = match appearance {
                    Appearance::Filled | Appearance::Elevated => &states.filled,
                    Appearance::Outlined => &states.outlined,
                    Appearance::Text => &states.text,
                };
                let token = self.size_token(*size);
                let radius = self.shape_radius(*shape).to_corner_radii();
                let height = match size {
                    ControlSize::Small => 32.0,
                    ControlSize::Medium => 40.0,
                    ControlSize::Large => 48.0,
                };
                ResolvedComponentStyle::Button(ButtonStyle {
                    background: variant.base.background,
                    foreground: variant.base.foreground,
                    border: variant.base.border,
                    hover: variant.hover,
                    pressed: variant.pressed,
                    focused: variant.focused,
                    disabled: self.disabled.clone(),
                    font_size: token.size,
                    font_weight: 500,
                    letter_spacing: token.letter_spacing,
                    corner_radius: radius,
                    min_width: 64.0,
                    height,
                    padding: Padding {
                        left: 24.0,
                        right: 24.0,
                        top: 0.0,
                        bottom: 0.0,
                    },
                })
            }
            InteractiveRole::TextInput { variant, size, .. } => {
                let token = self.size_token(*size);
                let bg = match variant {
                    InputVariant::Filled => self.surface_container_highest,
                    InputVariant::Outlined => Color::TRANSPARENT,
                };
                let height = match size {
                    ControlSize::Small => 32.0,
                    ControlSize::Medium => 40.0,
                    ControlSize::Large => 48.0,
                };
                let layers = self.surface_layers(bg, self.on_surface);
                ResolvedComponentStyle::TextInput(InputStyle {
                    background: bg,
                    foreground: self.on_surface,
                    border_color: self.outline,
                    placeholder_color: self.on_surface_variant,
                    hover: layers.hover,
                    pressed: layers.pressed,
                    focused: layers.focused,
                    disabled: self.disabled.clone(),
                    font_size: token.size,
                    font_weight: 400,
                    letter_spacing: token.letter_spacing,
                    corner_radius: ShapeLevel::from_design_radius(self.design_radius)
                        .to_corner_radii(),
                    height,
                    padding: Padding {
                        left: 12.0,
                        right: 12.0,
                        top: 0.0,
                        bottom: 0.0,
                    },
                })
            }
            InteractiveRole::Select { size } => {
                let token = self.size_token(*size);
                let trigger_layers = self.surface_layers(self.surface, self.on_surface);
                ResolvedComponentStyle::Select(SelectStyle {
                    trigger_bg: self.surface,
                    trigger_fg: self.on_surface,
                    trigger_border: self.outline,
                    trigger_hover_bg: trigger_layers.hover.background,
                    dropdown_bg: self.surface_container,
                    dropdown_fg: self.on_surface,
                    dropdown_border: self.outline_variant,
                    selected_bg: self.secondary_container,
                    selected_fg: self.on_secondary_container,
                    option_hover_bg: trigger_layers.hover.background,
                    option_disabled_fg: self.disabled.foreground,
                    corner_radius: ShapeLevel::from_design_radius(self.design_radius)
                        .to_corner_radii(),
                    height: 40.0,
                    font_size: token.size,
                    shadow: Some(ElevationLevel::Level3.shadow(self.is_dark, self.design_depth)),
                })
            }
            InteractiveRole::Checkbox { .. } => {
                let layers = self.surface_layers(self.surface_container_highest, self.on_surface);
                ResolvedComponentStyle::Checkbox(CheckboxStyle {
                    checked_bg: self.accent_fill(),
                    unchecked_bg: self.surface_container_highest,
                    unchecked_border: self.outline,
                    checked_icon: self.accent_on_fill(),
                    hover_bg: layers.hover.background,
                    pressed_bg: layers.pressed.background,
                    disabled_bg: self.disabled.background,
                    disabled_border: self.disabled.border,
                    corner_radius: ShapeLevel::from_design_radius(self.design_radius)
                        .to_corner_radii(),
                    size: 20.0,
                })
            }
            InteractiveRole::Switch { .. } => {
                let layers = self.surface_layers(self.surface_container_highest, self.on_surface);
                ResolvedComponentStyle::Switch(SwitchStyle {
                    checked_bg: self.accent_fill(),
                    unchecked_bg: self.surface_container_highest,
                    checked_thumb: self.accent_on_fill(),
                    unchecked_thumb: self.outline,
                    disabled_bg: self.disabled.background,
                    disabled_thumb: self.disabled.foreground,
                    hover_bg: layers.hover.background,
                    hover_fg: layers.hover.foreground,
                    pressed_bg: layers.pressed.background,
                    pressed_fg: layers.pressed.foreground,
                    focused_bg: layers.focused.background,
                    focused_fg: layers.focused.foreground,
                    width: 52.0,
                    height: 32.0,
                    thumb_size: 24.0,
                })
            }
            InteractiveRole::Radio { .. } => {
                let layers = self.surface_layers(self.surface_container_highest, self.on_surface);
                ResolvedComponentStyle::Radio(RadioStyle {
                    checked_bg: self.accent_fill(),
                    unchecked_bg: self.surface_container_highest,
                    unchecked_border: self.outline,
                    checked_dot: self.accent_on_fill(),
                    hover_bg: layers.hover.background,
                    disabled_bg: self.disabled.background,
                    disabled_border: self.disabled.border,
                    size: 20.0,
                })
            }
            InteractiveRole::Slider { .. } => {
                let fill = self.accent_fill();
                ResolvedComponentStyle::Slider(SliderStyle {
                    track_color: self.surface_container_highest,
                    fill_color: fill,
                    thumb_color: fill,
                    hover_thumb: self.primary_container,
                    pressed_thumb: fill,
                    disabled_track: self.disabled.background,
                    disabled_thumb: self.disabled.foreground,
                    track_height: 4.0,
                    thumb_radius: 10.0,
                })
            }
            InteractiveRole::ColorPicker { size } => {
                let token = self.size_token(*size);
                let radius = ShapeLevel::from_design_radius(self.design_radius).to_corner_radii();
                ResolvedComponentStyle::ColorPicker(ColorPickerStyle {
                    panel_bg: self.surface_container,
                    panel_border: self.outline_variant,
                    trigger_bg: self.surface,
                    trigger_border: self.outline,
                    plane_handle_color: Color::WHITE,
                    plane_handle_border: self.outline,
                    slider_handle_color: self.on_surface,
                    slider_track_color: self.surface_container_highest,
                    preview_border: self.outline_variant,
                    preset_border: self.outline_variant,
                    label_color: self.on_surface_variant,
                    font_size: token.size,
                    corner_radius: radius,
                    handle_radius: 6.0,
                    hue_track_height: 12.0,
                    alpha_track_height: 12.0,
                    plane_size: 150.0,
                    preset_size: 20.0,
                    shadow: Some(ElevationLevel::Level3.shadow(self.is_dark, self.design_depth)),
                })
            }
            InteractiveRole::Chip { selected } => {
                let (bg, fg) = if *selected {
                    (self.secondary_container, self.on_secondary_container)
                } else {
                    (self.surface, self.on_surface)
                };
                let layers = self.surface_layers(bg, fg);
                ResolvedComponentStyle::Chip(ChipStyle {
                    background: bg,
                    foreground: fg,
                    selected_bg: self.secondary_container,
                    selected_fg: self.on_secondary_container,
                    border_color: self.outline,
                    corner_radius: ShapeLevel::Small.to_corner_radii(),
                    height: 32.0,
                    font_size: self.typescale.label.large.size,
                    padding_h: 12.0,
                    hover_bg: layers.hover.background,
                    hover_fg: layers.hover.foreground,
                    pressed_bg: layers.pressed.background,
                    pressed_fg: layers.pressed.foreground,
                    focused_bg: layers.focused.background,
                    focused_fg: layers.focused.foreground,
                    disabled: self.disabled.clone(),
                })
            }
            InteractiveRole::Tab { selected } => {
                let accent = self.accent_fill();
                let fg = if *selected {
                    accent
                } else {
                    self.on_surface_variant
                };
                let layers = self.surface_layers(self.surface, fg);
                let shape = ShapeLevel::from_design_radius(self.design_radius);
                ResolvedComponentStyle::Tab(TabStyle {
                    background: self.surface,
                    foreground: fg,
                    selected_bg: self.secondary_container,
                    selected_fg: accent,
                    indicator_color: accent,
                    indicator_height: 3.0,
                    hover_bg: layers.hover.background,
                    hover_fg: layers.hover.foreground,
                    pressed_bg: layers.pressed.background,
                    pressed_fg: layers.pressed.foreground,
                    focused_bg: layers.focused.background,
                    focused_fg: layers.focused.foreground,
                    disabled: self.disabled.clone(),
                    font_size: self.typescale.title.small.size,
                    height: 48.0,
                    tab_gap: 0.0,
                    pill_radius: shape.to_corner_radii(),
                })
            }
            InteractiveRole::MenuItem => {
                let layers = self.surface_layers(self.surface, self.on_surface);
                ResolvedComponentStyle::MenuItem(MenuItemStyle {
                    background: self.surface,
                    foreground: self.on_surface,
                    hover_bg: layers.hover.background,
                    hover_fg: self.on_surface,
                    disabled_fg: self.disabled.foreground,
                    font_size: self.typescale.body.medium.size,
                    height: 40.0,
                    padding: Padding {
                        left: 12.0,
                        right: 12.0,
                        top: 0.0,
                        bottom: 0.0,
                    },
                })
            }
            InteractiveRole::ListItem { selected } => {
                let (bg, fg) = if *selected {
                    (self.secondary_container, self.on_secondary_container)
                } else {
                    (self.surface, self.on_surface)
                };
                let layers = self.surface_layers(bg, fg);
                ResolvedComponentStyle::ListItem(ListItemStyle {
                    background: bg,
                    foreground: fg,
                    selected_bg: self.secondary_container,
                    selected_fg: self.on_secondary_container,
                    hover_bg: layers.hover.background,
                    font_size: self.typescale.body.medium.size,
                    height: 40.0,
                    padding: Padding {
                        left: 16.0,
                        right: 16.0,
                        top: 0.0,
                        bottom: 0.0,
                    },
                })
            }
            InteractiveRole::TreeItem { selected, .. } => {
                let (bg, fg) = if *selected {
                    (self.secondary_container, self.on_secondary_container)
                } else {
                    (self.surface, self.on_surface)
                };
                let layers = self.surface_layers(bg, fg);
                ResolvedComponentStyle::TreeItem(TreeItemStyle {
                    background: bg,
                    foreground: fg,
                    selected_bg: self.secondary_container,
                    selected_fg: self.on_secondary_container,
                    hover_bg: layers.hover.background,
                    indent: 24.0,
                    font_size: self.typescale.body.medium.size,
                    height: 36.0,
                })
            }
            InteractiveRole::Progress { intent } => {
                let fill = match intent {
                    Intent::Danger => self.error,
                    Intent::Success => self.primary,
                    _ => self.primary,
                };
                ResolvedComponentStyle::Progress(ProgressStyle {
                    track_color: self.surface_container_highest,
                    fill_color: fill,
                    height: 4.0,
                    corner_radius: ShapeLevel::Full.to_corner_radii(),
                    circular_size: 48.0,
                })
            }
        }
    }

    fn resolve_display(&self, role: &DisplayRole) -> ResolvedComponentStyle {
        match role {
            DisplayRole::Text { foreground } => {
                let fg = match foreground {
                    ColorRole::OnSurface => self.on_surface,
                    ColorRole::OnSurfaceVariant => self.on_surface_variant,
                    ColorRole::Primary => self.primary,
                    ColorRole::Error => self.error,
                };
                let body = self.typescale.body.medium;
                ResolvedComponentStyle::Text(TextStyle {
                    foreground: fg,
                    font_size: body.size,
                    font_weight: 400,
                    letter_spacing: body.letter_spacing,
                })
            }
            DisplayRole::Badge { intent } => {
                let (bg, fg) = match intent {
                    Intent::Primary => (self.primary_container, self.on_primary_container),
                    Intent::Danger => (self.error_container, self.on_error_container),
                    _ => (self.surface_container, self.on_surface),
                };
                ResolvedComponentStyle::Badge(BadgeStyle {
                    background: bg,
                    foreground: fg,
                    corner_radius: ShapeLevel::Full.to_corner_radii(),
                    font_size: self.typescale.label.small.size,
                    font_weight: 500,
                    height: 20.0,
                    padding_h: 8.0,
                })
            }
            DisplayRole::Divider => ResolvedComponentStyle::Divider(DividerStyle {
                color: self.outline_variant,
                thickness: 1.0,
            }),
            DisplayRole::Skeleton => ResolvedComponentStyle::Skeleton(SkeletonStyle {
                background: self.surface_container_highest,
                shimmer: self.on_surface,
            }),
            DisplayRole::Avatar => ResolvedComponentStyle::Avatar(AvatarStyle {
                font_size: self.typescale.title.medium.size,
                font_weight: 500,
                size: 40.0,
                corner_radius: ShapeLevel::Full.to_corner_radii(),
            }),
            DisplayRole::Icon => ResolvedComponentStyle::Icon(IconStyle {
                color: self.on_surface_variant,
                size: 20.0,
            }),
            DisplayRole::Tooltip => ResolvedComponentStyle::Tooltip(TooltipStyle {
                background: self.inverse_surface,
                foreground: self.inverse_on_surface,
                font_size: self.typescale.body.small.size,
                corner_radius: ShapeLevel::ExtraSmall.to_corner_radii(),
                padding: Padding {
                    left: 8.0,
                    right: 8.0,
                    top: 4.0,
                    bottom: 4.0,
                },
            }),
            DisplayRole::Popover => ResolvedComponentStyle::Popover(PopoverStyle {
                background: self.surface_container,
                border_color: self.outline_variant,
                corner_radius: ShapeLevel::Small.to_corner_radii(),
                shadow: Some(ElevationLevel::Level3.shadow(self.is_dark, self.design_depth)),
            }),
            DisplayRole::Modal => {
                // M3 spec: scrim is black applied at 32% opacity (light) or 50% (dark).
                // material-colors `scheme.scrim()` returns opaque black (alpha=1.0).
                let mut scrim = self.scrim;
                scrim.a = if self.is_dark { 0.50 } else { 0.32 };
                ResolvedComponentStyle::Modal(ModalStyle {
                    backdrop_color: scrim,
                })
            }
            DisplayRole::Dialog => ResolvedComponentStyle::Dialog(DialogStyle {
                background: self.surface_container_high,
                foreground: self.on_surface,
                border_color: self.outline_variant,
                corner_radius: ShapeLevel::Large.to_corner_radii(),
                shadow: Some(ElevationLevel::Level3.shadow(self.is_dark, self.design_depth)),
                title_font_size: self.typescale.title.medium.size,
                body_font_size: self.typescale.body.medium.size,
                padding: Padding::all(24.0),
                gap: 16.0,
                max_width: 560.0,
            }),
            DisplayRole::Toast { kind } => {
                let (bg, fg) = match kind {
                    ToastKind::Info => (self.primary_container, self.on_primary_container),
                    ToastKind::Success => (self.primary_container, self.on_primary_container),
                    ToastKind::Warning => (self.tertiary_container, self.on_tertiary_container),
                    ToastKind::Error => (self.error_container, self.on_error_container),
                };
                ResolvedComponentStyle::Toast(ToastStyle {
                    background: bg,
                    foreground: fg,
                    corner_radius: ShapeLevel::Small.to_corner_radii(),
                    font_size: self.typescale.body.small.size,
                })
            }
        }
    }

    fn intent_states_for(&self, intent: Intent) -> &IntentStates {
        match intent {
            Intent::Primary => &self.primary_states,
            Intent::Secondary => &self.secondary_states,
            Intent::Danger => &self.error_states,
            Intent::Success => &self.primary_states,
            Intent::Warning => &self.tertiary_states,
            Intent::Info => &self.secondary_states,
            Intent::Accent => &self.tertiary_states,
            Intent::Default => &self.primary_states, // fallback
        }
    }

    /// Compute interaction state layers derived from a component's own background.
    fn surface_layers(&self, bg: Color, fg: Color) -> VariantStates {
        VariantStates::filled(bg, fg, self.is_dark, self.design_interaction)
    }

    fn size_token(&self, size: ControlSize) -> M3TypeToken {
        match size {
            ControlSize::Small => M3TypeToken {
                size: 12.0,
                line_height: 18.0,
                letter_spacing: 0.5,
            },
            ControlSize::Medium => M3TypeToken {
                size: 14.0,
                line_height: 20.0,
                letter_spacing: 0.1,
            },
            ControlSize::Large => M3TypeToken {
                size: 16.0,
                line_height: 24.0,
                letter_spacing: 0.15,
            },
        }
    }

    fn shape_radius(&self, shape: ControlShape) -> ShapeLevel {
        match shape {
            ControlShape::Square => ShapeLevel::None,
            ControlShape::Rounded => ShapeLevel::from_design_radius(self.design_radius),
            ControlShape::Pill => ShapeLevel::Full,
            ControlShape::Circle => ShapeLevel::Full,
        }
    }

    fn accent_fill(&self) -> Color {
        if self.design_accent < 0.5 {
            if self.is_dark {
                Color::rgba8(230, 230, 235, 255)
            } else {
                Color::rgba8(28, 25, 23, 255)
            }
        } else {
            self.primary
        }
    }

    fn accent_on_fill(&self) -> Color {
        if self.design_accent < 0.5 {
            if self.is_dark {
                Color::rgba8(28, 25, 23, 255)
            } else {
                Color::rgba8(250, 250, 249, 255)
            }
        } else {
            self.on_primary
        }
    }

    pub fn surface_at(&self, level: ElevationLevel) -> Color {
        match level {
            ElevationLevel::Level0 => self.surface,
            ElevationLevel::Level1 => self.surface_container_lowest,
            ElevationLevel::Level2 => self.surface_container_low,
            ElevationLevel::Level3 => self.surface_container,
            ElevationLevel::Level4 => self.surface_container_high,
            ElevationLevel::Level5 => self.surface_container_highest,
        }
    }
}
