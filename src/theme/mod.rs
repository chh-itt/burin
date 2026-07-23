//! Theme system: design tokens, semantic colors, state colors, typography, elevation.
//!
//! The theme module provides a layered architecture:
//!
//! - **Layer 1 (Ambient)**: Zero-config environment colors. Every widget
//!   defaults to these — no code needed for a neutral look.
//! - **Layer 2 (Semantic)**: Intent-driven colors via `.intent(Primary)`.
//!   Maps directly to the 7-intent [`SemanticPalette`].
//! - **Layer 3 (Direct)**: Full token-level override via [`Styled`] methods
//!   or by mutating the [`Theme`] struct fields.
//!
//! Widgets never hardcode visual properties — all defaults flow from Theme
//! through [`MountContext`] to widget [`mount_box`].

pub mod apply;
pub mod auto_switch;
pub mod m3;
pub mod tokens;

pub use auto_switch::auto_theme;

use crate::style::Color;

use crate::theme::m3::engine::{HctColor, SchemeGen};
use crate::theme::m3::typescale::Typescale;
use crate::theme::m3::DynamicColorScheme;

use crate::theme::m3::DesignParams;

// ── Theme trait ──────────────────────────────────────────────────────

/// Theme abstraction for pluggable design systems.
///
/// Third-party crates can implement this trait to provide custom theme
/// engines (e.g. Fluent, Cupertino, custom brand themes) without forking
/// the M3 engine. The built-in [`M3Theme`] implements this trait.
pub trait Theme: 'static {
    fn name(&self) -> &str;
    fn is_dark(&self) -> bool;
    fn resolve_component(
        &self,
        role: &crate::theme::m3::roles::ComponentRole,
    ) -> crate::theme::m3::roles::ResolvedComponentStyle;
    fn resolve_color(&self, role: crate::theme::m3::roles::ColorRole) -> Color;
}

// ── M3 Theme (new) ──────────────────────────────────────────────────

/// Material 3 dynamic theme — seed-based color generation.
/// Default personality: Refined (warm neutrals, restrained accent, gentle interaction).
/// Use builder methods like `.warmth()`, `.accent()` to customize.
/// Call `.m3_classic()` to restore the original M3 look.
#[derive(Clone, Debug)]
pub struct M3Theme {
    pub name: &'static str,
    pub is_dark: bool,
    pub scheme: DynamicColorScheme,
    pub typescale: Typescale,
    pub seed_color: Color,
    pub background_color: Color,
    pub z_index: ZIndexScale,
    pub breakpoints: Breakpoints,
}

impl M3Theme {
    /// Standard M3: seed color → all roles are computed. Default: Refined personality.
    pub fn from_seed(seed: Color) -> Self {
        build_theme("Refined", seed, seed, false, DesignParams::refined())
    }

    /// Background-first: user picks background → seed is derived.
    pub fn from_background(bg: Color) -> Self {
        let hct = HctColor::from_color(bg);
        let hue = if hct.chroma < 5.0 { 260.0 } else { hct.hue };
        let seed_tone = if hct.tone > 50.0 { 40.0 } else { 80.0 };
        let seed = HctColor::new(hue, 48.0, seed_tone).to_color();
        build_theme("Refined", seed, bg, false, DesignParams::refined())
    }

    /// Fine-grained control.
    pub fn from_colors(background: Color, primary: Color) -> Self {
        build_theme(
            "Refined",
            primary,
            background,
            false,
            DesignParams::refined(),
        )
    }

    /// Switch to dark/light variant of the same seed.
    pub fn with_is_dark(&self, is_dark: bool) -> Self {
        if self.is_dark == is_dark {
            return self.clone();
        }
        let dp = DesignParams {
            warmth: self.scheme.design_warmth,
            radius: self.scheme.design_radius,
            depth: self.scheme.design_depth,
            accent: self.scheme.design_accent,
            interaction: self.scheme.design_interaction,
            contrast: self.scheme.design_contrast,
            density: self.scheme.design_density,
            border_presence: self.scheme.design_border_presence,
            surface_variance: self.scheme.design_surface_variance,
            typescale_contrast: self.scheme.design_typescale_contrast,
            font_weight_contrast: self.scheme.design_font_weight_contrast,
        };
        build_theme(
            self.name,
            self.seed_color,
            self.background_color,
            is_dark,
            dp,
        )
    }

    /// Resolve a component role to its visual style.
    pub fn resolve_component(
        &self,
        role: &crate::theme::m3::roles::ComponentRole,
    ) -> crate::theme::m3::roles::ResolvedComponentStyle {
        self.scheme.resolve_component(role)
    }

    // ── Presets ──────────────────────────────────────────────

    /// Apply a complete [`PresetTheme`] — seed color + DesignParams + SchemeVariant in one call.
    ///
    /// ```ignore
    /// use burin::theme::m3::presets::PresetTheme;
    /// let theme = M3Theme::from_seed(SEED_SLATE)
    ///     .preset(PresetTheme::neo_minimal_slate());
    /// ```
    pub fn preset(mut self, preset: PresetTheme) -> Self {
        let mc_scheme = SchemeGen::generate(preset.seed, self.is_dark, preset.variant);
        self.scheme =
            DynamicColorScheme::from_mc_scheme_with_design(&mc_scheme, self.is_dark, preset.design);
        self.seed_color = preset.seed;
        self.name = preset.name;
        self
    }

    /// Restore M3 Classic (Google Material 3 original) look.
    pub fn m3_classic(mut self) -> Self {
        let dp = DesignParams::m3_classic();
        self.scheme = DynamicColorScheme::from_mc_scheme_with_design(
            &if self.is_dark {
                SchemeGen::dark(self.seed_color)
            } else {
                SchemeGen::light(self.seed_color)
            },
            self.is_dark,
            dp,
        );
        self.name = "M3 Classic";
        self
    }

    // ── Design parameter builders ────────────────────────────

    pub fn warmth(mut self, v: f32) -> Self {
        self.scheme.design_warmth = v.clamp(0.0, 1.0);
        self
    }
    pub fn radius(mut self, v: f32) -> Self {
        self.scheme.design_radius = v.clamp(0.0, 1.0);
        self
    }
    pub fn depth(mut self, v: f32) -> Self {
        self.scheme.design_depth = v.clamp(0.0, 1.0);
        self
    }
    pub fn accent(mut self, v: f32) -> Self {
        self.scheme.design_accent = v.clamp(0.0, 1.0);
        self
    }
    pub fn interaction(mut self, v: f32) -> Self {
        self.scheme.design_interaction = v.clamp(0.0, 1.0);
        self
    }
    pub fn contrast(mut self, v: f32) -> Self {
        self.scheme.design_contrast = v.clamp(0.0, 1.0);
        self
    }
    pub fn density(mut self, v: f32) -> Self {
        self.scheme.design_density = v.clamp(0.0, 1.0);
        self
    }
    pub fn border_presence(mut self, v: f32) -> Self {
        self.scheme.design_border_presence = v.clamp(0.0, 1.0);
        self
    }
    pub fn surface_variance(mut self, v: f32) -> Self {
        self.scheme.design_surface_variance = v.clamp(0.0, 1.0);
        self
    }
    pub fn typescale_contrast(mut self, v: f32) -> Self {
        self.scheme.design_typescale_contrast = v.clamp(0.0, 1.0);
        self
    }
    pub fn font_weight_contrast(mut self, v: f32) -> Self {
        self.scheme.design_font_weight_contrast = v.clamp(0.0, 1.0);
        self
    }
}

impl Theme for M3Theme {
    fn name(&self) -> &str {
        self.name
    }
    fn is_dark(&self) -> bool {
        self.is_dark
    }
    fn resolve_component(&self, role: &ComponentRole) -> ResolvedComponentStyle {
        self.scheme.resolve_component(role)
    }
    fn resolve_color(&self, role: ColorRole) -> Color {
        match role {
            ColorRole::OnSurface => self.scheme.on_surface,
            ColorRole::OnSurfaceVariant => self.scheme.on_surface_variant,
            ColorRole::Primary => self.scheme.primary,
            ColorRole::Error => self.scheme.error,
        }
    }
}

fn build_theme(
    name: &'static str,
    seed: Color,
    bg: Color,
    is_dark: bool,
    dp: DesignParams,
) -> M3Theme {
    let mc_scheme = if is_dark {
        SchemeGen::dark(seed)
    } else {
        SchemeGen::light(seed)
    };
    let scheme = DynamicColorScheme::from_mc_scheme_with_design(&mc_scheme, is_dark, dp);
    M3Theme {
        name,
        is_dark,
        scheme,
        typescale: Typescale::default(),
        seed_color: seed,
        background_color: bg,
        z_index: default_z_index(),
        breakpoints: default_breakpoints(),
    }
}

// Re-export M3 types for widget use
pub use crate::theme::m3::engine::SchemeVariant;
pub use crate::theme::m3::presets::PresetTheme;
pub use crate::theme::m3::roles::ColorRole;
pub use crate::theme::m3::roles::ComponentRole;
pub use crate::theme::m3::roles::DisplayRole;
pub use crate::theme::m3::roles::InputVariant;
pub use crate::theme::m3::roles::InteractiveRole;
pub use crate::theme::m3::roles::ResolvedComponentStyle;

// ── Z-index scale ──────────────────────────────────────────────────

/// Canonical z-index values for consistent layering.
#[derive(Clone, Debug)]
pub struct ZIndexScale {
    pub base: i32,
    pub dropdown: i32,
    pub sticky: i32,
    pub overlay: i32,
    pub modal: i32,
    pub toast: i32,
    pub tooltip: i32,
}

fn default_z_index() -> ZIndexScale {
    ZIndexScale {
        base: 0,
        dropdown: 50,
        sticky: 100,
        overlay: 200,
        modal: 300,
        toast: 400,
        tooltip: 500,
    }
}

// ── Breakpoints — responsive design ────────────────────────────────

#[derive(Clone, Debug)]
pub struct Breakpoints {
    pub sm: f32,
    pub md: f32,
    pub lg: f32,
    pub xl: f32,
}

fn default_breakpoints() -> Breakpoints {
    Breakpoints {
        sm: 640.0,
        md: 768.0,
        lg: 1024.0,
        xl: 1280.0,
    }
}

// ── Component variant types ─────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Intent {
    Default,
    Primary,
    Secondary,
    Danger,
    Warning,
    Success,
    Info,
    Accent,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Appearance {
    Filled,
    Outlined,
    Text,
    Elevated,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ControlSize {
    Small,
    Medium,
    Large,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ControlShape {
    Rounded,
    Pill,
    Square,
    Circle,
}
