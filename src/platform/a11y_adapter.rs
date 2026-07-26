//! Accessibility adapter (compatibility shim over [`A11yBridge`]).
//!
//! Historical module retained so existing call-sites don't break.
//! All real work is delegated to [`crate::platform::a11y_bridge::A11yBridge`].

use crate::platform::a11y_bridge::A11yBridge;
use accesskit::TreeUpdate;

/// Compatibility shim that delegates to the platform accessibility bridge.
pub struct A11yAdapter {
    bridge: A11yBridge,
}

impl A11yAdapter {
    pub fn new() -> Self {
        Self {
            bridge: A11yBridge::new(),
        }
    }

    pub fn init(&mut self, raw_handle: raw_window_handle::RawWindowHandle) {
        self.bridge.init(raw_handle);
    }

    pub fn update_if_active(&mut self, updater: impl FnOnce() -> TreeUpdate) {
        self.bridge.update(updater);
    }

    pub fn process_event(&mut self, _event: &winit::event::WindowEvent) {}

    pub fn latest_tree_update(&self) -> Option<TreeUpdate> {
        self.bridge.latest_tree_update()
    }
}

impl Default for A11yAdapter {
    fn default() -> Self {
        Self::new()
    }
}
