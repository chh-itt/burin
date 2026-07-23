# Theme System

## Material 3

Burin implements Material 3 (Material You) with a custom HCT color engine.

```rust
use burin::theme::M3Theme;
use burin::style::Color;

// From a seed color
let theme = M3Theme::from_seed(Color::rgba8(103, 121, 232, 255));

// From a preset
let theme = M3Theme::from_preset(PresetTheme::neo_minimal_slate());

// Dynamic color scheme
let scheme = theme.scheme;
scheme.primary;       // primary color
scheme.surface;       // background color
scheme.on_surface;    // text color on background
scheme.error;         // error color
```

## Theme Trait

Pluggable `Theme` trait for custom design systems:

```rust
pub trait Theme: Clone + 'static {
    fn scheme(&self) -> &ColorScheme;
    fn style_for(&self, role: &ComponentRole, state: StateFlags) -> ResolvedStyle;
    fn font_family(&self) -> &str;
    fn font_size(&self) -> f32;
}
```

Swap themes at runtime:

```rust
App::new()
    .window(config, my_ui())
    .theme(my_custom_theme)
    .run()
    .unwrap();
```

## HCT Color Engine

HCT (Hue, Chroma, Tone) is a perceptually uniform color space. A single seed
color generates a full palette of ~30 tones per hue, with guaranteed contrast ratios
for accessibility.

## State Styles

Widgets respond to interaction states automatically:

```rust
// The style resolver picks the correct variant:
StateFlags::NONE       // default
StateFlags::HOVERED    // hover
StateFlags::PRESSED    // pressed
StateFlags::FOCUSED    // focused
StateFlags::DISABLED   // disabled
StateFlags::SELECTED   // selected/active
```
