//! System tray icon integration (requires `tray` feature).
//!
//! Uses the `tray-icon` crate for cross-platform tray icon support.
//! Events are dispatched via `set_event_handler` — callbacks fire
//! immediately when the tray icon or menu is interacted with, without
//! waiting for the next frame.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// A tray icon with optional menu, click handlers, and tooltip.
///
/// Events fire immediately (via `set_event_handler`), not on frame poll.
pub struct TrayIcon {
    #[allow(dead_code)]
    inner: tray_icon::TrayIcon,
    #[allow(dead_code)]
    click_ref: Arc<Mutex<Option<Box<dyn Fn() + Send>>>>,
    #[allow(dead_code)]
    menu_ref: Arc<Mutex<HashMap<String, Box<dyn Fn() + Send>>>>,
}

/// Builder for [`TrayIcon`].
pub struct TrayIconBuilder {
    icon_data: Option<(Vec<u8>, u32, u32)>,
    tooltip: Option<String>,
    menu: Option<TrayMenu>,
    on_click: Option<Box<dyn Fn() + Send>>,
    on_double_click: Option<Box<dyn Fn() + Send>>,
}

/// A tray context menu.
pub struct TrayMenu {
    items: Vec<TrayMenuItem>,
}

/// A single item in a [`TrayMenu`].
pub enum TrayMenuItem {
    Item {
        label: String,
        enabled: bool,
        action: Option<Box<dyn Fn() + Send>>,
    },
    Separator,
}

#[derive(Debug, thiserror::Error)]
#[cfg_attr(feature = "devtools", derive(serde::Serialize, serde::Deserialize))]
pub enum TrayError {
    #[error("icon data required")]
    NoIcon,
    #[error("tray icon creation failed: {0}")]
    Create(String),
    #[error("image decode failed: {0}")]
    Decode(String),
}

impl Default for TrayIconBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl TrayIconBuilder {
    pub fn new() -> Self {
        Self {
            icon_data: None,
            tooltip: None,
            menu: None,
            on_click: None,
            on_double_click: None,
        }
    }

    /// Set the tray icon from raw RGBA pixel data.
    pub fn icon(&mut self, rgba: &[u8], width: u32, height: u32) -> &mut Self {
        self.icon_data = Some((rgba.to_vec(), width, height));
        self
    }

    /// Set the tray icon by decoding a PNG byte buffer.
    /// Requires the `ext-image` feature for PNG decoding.
    #[cfg(feature = "ext-image")]
    pub fn icon_from_png(&mut self, png_bytes: &[u8]) -> Result<(), TrayError> {
        let img =
            image::load_from_memory(png_bytes).map_err(|e| TrayError::Decode(e.to_string()))?;
        let rgba = img.to_rgba8();
        let (w, h) = rgba.dimensions();
        self.icon_data = Some((rgba.into_raw(), w, h));
        Ok(())
    }

    /// Set the tooltip text shown on hover.
    pub fn tooltip(&mut self, text: impl Into<String>) -> &mut Self {
        self.tooltip = Some(text.into());
        self
    }

    /// Set the right-click context menu.
    pub fn menu(&mut self, menu: TrayMenu) -> &mut Self {
        self.menu = Some(menu);
        self
    }

    /// Set a callback for left-click on the tray icon.
    pub fn on_click(&mut self, f: impl Fn() + Send + 'static) -> &mut Self {
        self.on_click = Some(Box::new(f));
        self
    }

    /// Set a callback for double-click on the tray icon (Windows only).
    pub fn on_double_click(&mut self, f: impl Fn() + Send + 'static) -> &mut Self {
        self.on_double_click = Some(Box::new(f));
        self
    }

    /// Build and activate the tray icon.
    /// Registers immediate event handlers — no frame polling needed.
    pub fn build(self) -> Result<TrayIcon, TrayError> {
        let (rgba, w, h) = self.icon_data.ok_or(TrayError::NoIcon)?;
        let icon =
            tray_icon::Icon::from_rgba(rgba, w, h).map_err(|e| TrayError::Create(e.to_string()))?;

        let mut tb = tray_icon::TrayIconBuilder::new().with_icon(icon);

        if let Some(ref tooltip) = self.tooltip {
            tb = tb.with_tooltip(tooltip.as_str());
        }

        let mut menu_actions: HashMap<String, Box<dyn Fn() + Send>> = HashMap::new();

        if let Some(mut tray_menu) = self.menu {
            let muda_menu = build_muda_menu(&mut tray_menu, &mut menu_actions);
            tb = tb.with_menu(Box::new(muda_menu));
        }

        let inner = tb.build().map_err(|e| TrayError::Create(e.to_string()))?;

        // Wrap callbacks in Rc<RefCell<>> so handler closures have 'static lifetime.
        let click_cb: Arc<Mutex<Option<Box<dyn Fn() + Send>>>> =
            Arc::new(Mutex::new(self.on_click));
        let dbl_cb: Arc<Mutex<Option<Box<dyn Fn() + Send>>>> =
            Arc::new(Mutex::new(self.on_double_click));
        let menu_cb: Arc<Mutex<HashMap<String, Box<dyn Fn() + Send>>>> =
            Arc::new(Mutex::new(menu_actions));

        // Register immediate event handlers (fire synchronously, no frame polling).
        {
            let click = click_cb.clone();
            let dbl = dbl_cb.clone();
            tray_icon::TrayIconEvent::set_event_handler(Some(Box::new(
                move |event: tray_icon::TrayIconEvent| match event {
                    tray_icon::TrayIconEvent::Click { button, .. }
                        if button == tray_icon::MouseButton::Left =>
                    {
                        if let Some(ref f) = *click.lock().unwrap() {
                            f();
                        }
                    }
                    tray_icon::TrayIconEvent::DoubleClick { .. } => {
                        if let Some(ref f) = *dbl.lock().unwrap() {
                            f();
                        }
                    }
                    _ => {}
                },
            )));
        }
        {
            let actions = menu_cb.clone();
            tray_icon::menu::MenuEvent::set_event_handler(Some(Box::new(
                move |event: tray_icon::menu::MenuEvent| {
                    if let Some(action) = actions.lock().unwrap().remove(&event.id.0) {
                        action();
                    }
                },
            )));
        }

        Ok(TrayIcon {
            inner,
            click_ref: click_cb,
            menu_ref: menu_cb,
        })
    }
}

impl TrayIcon {
    /// No-op: events are dispatched immediately via `set_event_handler`.
    /// Kept for API compatibility.
    pub fn poll(&mut self) {}
}

// ── TrayMenu / TrayMenuItem builder API ──────────────────────────

impl TrayMenu {
    pub fn new() -> Self {
        Self { items: Vec::new() }
    }

    pub fn item(mut self, label: impl Into<String>, action: impl Fn() + Send + 'static) -> Self {
        self.items.push(TrayMenuItem::Item {
            label: label.into(),
            enabled: true,
            action: Some(Box::new(action)),
        });
        self
    }

    pub fn disabled_item(mut self, label: impl Into<String>) -> Self {
        self.items.push(TrayMenuItem::Item {
            label: label.into(),
            enabled: false,
            action: None,
        });
        self
    }

    pub fn separator(mut self) -> Self {
        self.items.push(TrayMenuItem::Separator);
        self
    }
}

impl Default for TrayMenu {
    fn default() -> Self {
        Self::new()
    }
}

// ── Internal: convert TrayMenu → tray_icon::menu::Menu (muda) ────

fn build_muda_menu(
    menu: &mut TrayMenu,
    actions: &mut HashMap<String, Box<dyn Fn() + Send>>,
) -> tray_icon::menu::Menu {
    use tray_icon::menu::{IsMenuItem, Menu, MenuId, MenuItemBuilder, PredefinedMenuItem};

    let mut items: Vec<Box<dyn IsMenuItem>> = Vec::new();

    for (i, item) in menu.items.iter_mut().enumerate() {
        match item {
            TrayMenuItem::Separator => {
                items.push(Box::new(PredefinedMenuItem::separator()));
            }
            TrayMenuItem::Item {
                label,
                enabled,
                action,
            } => {
                let id = format!("tray_{i}");
                let mi = MenuItemBuilder::new()
                    .text(label.clone())
                    .enabled(*enabled)
                    .id(MenuId(id.clone()))
                    .build();
                if let Some(a) = action.take() {
                    actions.insert(id, a);
                }
                items.push(Box::new(mi));
            }
        }
    }

    let menu = Menu::new();
    for item in &items {
        menu.append(item.as_ref()).ok();
    }

    menu
}
