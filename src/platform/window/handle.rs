use std::sync::Arc;

use super::buttons::WindowButtons;
use super::icon::WindowIcon;
use crate::platform::CursorIcon;

// ═══════════════════════ WindowHandle ═══════════════════════

/// A cloneable handle for runtime window control.
///
/// Obtained from `MountContext::window_handle` during widget mount,
/// or stored directly for imperative window operations.
#[derive(Clone)]
pub struct WindowHandle {
    pub(crate) window: Arc<dyn winit::window::Window>,
}

impl WindowHandle {
    pub fn id(&self) -> winit::window::WindowId {
        self.window.id()
    }

    pub fn set_visible(&self, v: bool) {
        self.window.set_visible(v);
    }
    pub fn is_visible(&self) -> Option<bool> {
        self.window.is_visible()
    }

    pub fn set_title(&self, title: &str) {
        self.window.set_title(title);
    }
    pub fn title(&self) -> String {
        self.window.title()
    }

    pub fn set_minimized(&self, v: bool) {
        self.window.set_minimized(v);
    }
    pub fn is_minimized(&self) -> Option<bool> {
        self.window.is_minimized()
    }

    pub fn set_maximized(&self, v: bool) {
        self.window.set_maximized(v);
    }
    pub fn is_maximized(&self) -> bool {
        self.window.is_maximized()
    }
    pub fn toggle_maximized(&self) {
        self.window.set_maximized(!self.window.is_maximized());
    }

    pub fn set_fullscreen(&self, v: bool) {
        if v {
            self.window
                .set_fullscreen(Some(winit::monitor::Fullscreen::Borderless(None)));
        } else {
            self.window.set_fullscreen(None);
        }
    }
    pub fn is_fullscreen(&self) -> bool {
        self.window.fullscreen().is_some()
    }
    pub fn toggle_fullscreen(&self) {
        self.set_fullscreen(!self.is_fullscreen());
    }

    pub fn set_resizable(&self, v: bool) {
        self.window.set_resizable(v);
    }
    pub fn is_resizable(&self) -> bool {
        self.window.is_resizable()
    }

    pub fn set_decorations(&self, v: bool) {
        self.window.set_decorations(v);
    }
    pub fn is_decorated(&self) -> bool {
        self.window.is_decorated()
    }

    pub fn set_transparent(&self, v: bool) {
        self.window.set_transparent(v);
    }

    pub fn set_min_inner_size(&self, size: Option<(f32, f32)>) {
        self.window.set_min_surface_size(
            size.map(|(w, h)| winit::dpi::LogicalSize::new(w as f64, h as f64).into()),
        );
    }
    pub fn set_max_inner_size(&self, size: Option<(f32, f32)>) {
        self.window.set_max_surface_size(
            size.map(|(w, h)| winit::dpi::LogicalSize::new(w as f64, h as f64).into()),
        );
    }

    pub fn set_window_icon(&self, icon: Option<&WindowIcon>) {
        self.window
            .set_window_icon(icon.map(WindowIcon::to_winit_icon));
    }

    pub fn set_always_on_top(&self, v: bool) {
        self.window.set_window_level(if v {
            winit::window::WindowLevel::AlwaysOnTop
        } else {
            winit::window::WindowLevel::Normal
        });
    }

    pub fn set_enabled_buttons(&self, buttons: WindowButtons) {
        self.window.set_enabled_buttons(buttons.inner());
    }

    pub fn set_theme(&self, theme: Option<winit::window::Theme>) {
        self.window.set_theme(theme);
    }
    pub fn theme(&self) -> Option<winit::window::Theme> {
        self.window.theme()
    }

    pub fn focus(&self) {
        self.window.focus_window();
    }

    pub fn close(&self) {
        self.window.set_visible(false);
    }

    pub fn drag_window(&self) {
        let _ = self.window.drag_window();
    }

    pub fn request_user_attention(&self, ty: Option<winit::window::UserAttentionType>) {
        self.window.request_user_attention(ty);
    }

    pub fn scale_factor(&self) -> f64 {
        self.window.scale_factor()
    }

    pub fn set_cursor(&self, icon: CursorIcon) {
        self.window
            .set_cursor(winit::cursor::Cursor::Icon(icon.inner()));
    }

    pub fn set_cursor_visible(&self, visible: bool) {
        self.window.set_cursor_visible(visible);
    }

    /// Monitor this window currently resides on.
    #[cfg(feature = "display")]
    pub fn current_monitor(&self) -> Option<super::super::display::MonitorHandle> {
        self.window
            .current_monitor()
            .map(super::super::display::MonitorHandle)
    }

    /// Enumerate all connected monitors.
    #[cfg(feature = "display")]
    pub fn available_monitors(&self) -> Vec<super::super::display::MonitorHandle> {
        self.window
            .available_monitors()
            .map(super::super::display::MonitorHandle)
            .collect()
    }

    /// Primary monitor.
    #[cfg(feature = "display")]
    pub fn primary_monitor(&self) -> Option<super::super::display::MonitorHandle> {
        self.window
            .primary_monitor()
            .map(super::super::display::MonitorHandle)
    }
}

impl std::fmt::Debug for WindowHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WindowHandle")
            .field("id", &self.window.id())
            .finish_non_exhaustive()
    }
}
