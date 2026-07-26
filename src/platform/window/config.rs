use auralis_signal::Signal;

use super::buttons::WindowButtons;
use super::icon::WindowIcon;
use crate::style::Color;
use crate::theme::M3Theme;

// ═══════════════════════ WindowConfig ═══════════════════════

/// Window creation parameters.
///
/// Set the title, size, theme, rendering backend, and optionally a
/// reactive theme signal.  Pass to [`App::window`](crate::platform::window::app::App::window).
pub struct WindowConfig {
    pub title: String,
    pub width: f32,
    pub height: f32,
    pub theme: M3Theme,
    pub theme_signal: Option<Signal<M3Theme>>,
    pub backend: crate::render::RendererChoice,
    #[cfg(feature = "tray")]
    pub tray: Option<crate::platform::tray::TrayIconBuilder>,
    #[cfg(feature = "i18n")]
    pub i18n: Option<std::rc::Rc<crate::i18n::I18n>>,
    // ── Desktop-only window behavior ──
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    pub resizable: bool,
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    pub min_width: Option<f32>,
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    pub min_height: Option<f32>,
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    pub max_width: Option<f32>,
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    pub max_height: Option<f32>,
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    pub maximized: bool,
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    pub fullscreen: bool,
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    pub decorations: bool,
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    pub transparent: bool,
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    pub always_on_top: bool,
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    pub window_icon: Option<WindowIcon>,
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    pub enabled_buttons: WindowButtons,
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    pub visible: bool,
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    pub position: Option<(f32, f32)>,
    #[cfg(all(
        feature = "display",
        not(any(target_os = "android", target_os = "ios"))
    ))]
    pub monitor: Option<super::super::display::MonitorHandle>,
    // ── Mobile-only window behavior (reserved for future Android/iOS support) ──
    #[cfg(any(target_os = "android", target_os = "ios"))]
    pub immersive_mode: bool,
    // ── Scroll physics ──
    pub scroll_friction: f32,
    pub scroll_stop_speed: f32,
}

impl Default for WindowConfig {
    fn default() -> Self {
        Self {
            title: "Auralis UI".into(),
            width: 1024.0,
            height: 768.0,
            theme: M3Theme::from_seed(Color::rgba8(0x67, 0x79, 0xE8, 0xFF))
                .preset(crate::theme::PresetTheme::neo_minimal_slate()),
            theme_signal: None,
            backend: crate::render::RendererChoice::Auto,
            #[cfg(feature = "tray")]
            tray: None,
            #[cfg(feature = "i18n")]
            i18n: None,
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            resizable: true,
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            min_width: None,
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            min_height: None,
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            max_width: None,
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            max_height: None,
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            maximized: false,
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            fullscreen: false,
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            decorations: true,
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            transparent: false,
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            always_on_top: false,
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            window_icon: None,
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            enabled_buttons: WindowButtons::ALL,
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            visible: true,
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            position: None,
            #[cfg(all(
                feature = "display",
                not(any(target_os = "android", target_os = "ios"))
            ))]
            monitor: None,
            #[cfg(any(target_os = "android", target_os = "ios"))]
            immersive_mode: true,
            scroll_friction: 800.0,
            scroll_stop_speed: 30.0,
        }
    }
}

impl WindowConfig {
    pub fn auto_theme() -> Self {
        #[cfg(feature = "system-theme")]
        {
            let is_dark = dark_light::detect()
                .map(|m| matches!(m, dark_light::Mode::Dark))
                .unwrap_or(false);
            let seed = Color::rgba8(0x67, 0x79, 0xE8, 0xFF);
            let initial = if is_dark {
                M3Theme::from_seed(seed).with_is_dark(true)
            } else {
                M3Theme::from_seed(seed)
            };
            let sig = Signal::new(initial);
            Self {
                theme: sig.read(),
                theme_signal: Some(sig),
                ..Default::default()
            }
        }
        #[cfg(not(feature = "system-theme"))]
        {
            Self {
                theme: M3Theme::from_seed(Color::rgba8(0x67, 0x79, 0xE8, 0xFF)),
                ..Default::default()
            }
        }
    }
}
