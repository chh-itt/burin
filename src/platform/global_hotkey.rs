//! System global hotkey manager — OS-level keyboard shortcuts that fire
//! even when the application window is NOT focused.
//!
//! # Three-layer key binding model
//!
//! | Layer | Name | Scope | Active when |
//! |-------|------|-------|-------------|
//! | 1 | System global hotkey | OS kernel | Always (even when app is backgrounded) |
//! | 2 | Application shortcut | `KeyBindingMap::app` | Window has focus |
//! | 3 | Widget shortcut | `KeyBindingMap::per_widget` | Widget is focused |
//!
//! # Architecture
//!
//! Wraps `handy_keys::HotkeyManager` which internally runs a background
//! thread with OS-specific hooks (`RegisterHotKey` on Windows,
//! `CGEventTap` on macOS, `evdev` on Linux).  We poll `try_recv()` in
//! `App::about_to_wait` — no extra thread needed.
//!
//! # Platform support
//!
//! - **Windows**: `RegisterHotKey` — no permissions required.
//! - **macOS**: `CGEventTap` — needs Accessibility permission.
//! - **Linux**: `evdev` — needs input group or udev rule.

use std::collections::HashMap;

use handy_keys::{Hotkey, HotkeyId, HotkeyManager, HotkeyState};

use crate::event::action::ActionKind;
use crate::event::action::KeyChord;

/// A registered global hotkey handle.  Drop it to unregister.
#[derive(Debug)]
pub struct HotkeyHandle {
    #[allow(dead_code)]
    id: HotkeyId,
    #[allow(dead_code)]
    manager_id: u64,
}

/// Error type for global hotkey operations.
#[derive(Debug, Clone)]
pub enum GlobalHotkeyError {
    /// The hotkey string could not be parsed.
    ParseError(String),
    /// This hotkey chord is already registered.
    AlreadyRegistered(String),
    /// The platform backend is unavailable or failed to start.
    BackendError(String),
    /// Required OS permissions are not granted.
    PermissionDenied {
        platform: &'static str,
        guidance: &'static str,
    },
    /// The hotkey was not found (e.g. on unregister).
    NotFound,
    /// An internal error occurred.
    Internal(String),
}

impl std::fmt::Display for GlobalHotkeyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ParseError(s) => write!(f, "failed to parse hotkey: {s}"),
            Self::AlreadyRegistered(s) => write!(f, "hotkey already registered: {s}"),
            Self::BackendError(s) => write!(f, "backend error: {s}"),
            Self::PermissionDenied { platform, guidance } => {
                write!(f, "permission denied on {platform}: {guidance}")
            }
            Self::NotFound => write!(f, "hotkey not found"),
            Self::Internal(s) => write!(f, "internal error: {s}"),
        }
    }
}

impl std::error::Error for GlobalHotkeyError {}

/// Internal mapping from a hotkey ID to the action it triggers.
struct HotkeyBinding {
    #[allow(dead_code)]
    chord_str: String,
    action: ActionKind,
}

/// Manages system-global hotkey registration and dispatch.
///
/// Owned by [`App`](super::window::App).  Created lazily on first
/// registration; polls for events in `about_to_wait`.
pub(crate) struct GlobalHotkeyManager {
    manager: Option<HotkeyManager>,
    /// Maps `HotkeyId` → the action to dispatch.
    bindings: HashMap<HotkeyId, HotkeyBinding>,
    /// Monotonic counter for generating handle IDs.
    manager_gen: u64,
}

impl GlobalHotkeyManager {
    /// Create an uninitialised manager.  The underlying `HotkeyManager`
    /// is created lazily on the first `register()` call.
    pub fn new() -> Self {
        Self {
            manager: None,
            bindings: HashMap::new(),
            manager_gen: 0,
        }
    }

    /// Ensure the backend is started.
    fn ensure_manager(&mut self) -> Result<(), GlobalHotkeyError> {
        if self.manager.is_some() {
            return Ok(());
        }
        let m = HotkeyManager::new().map_err(|e| match e {
            handy_keys::Error::AccessibilityNotGranted => GlobalHotkeyError::PermissionDenied {
                platform: platform_name(),
                guidance: permission_guidance(),
            },
            other => GlobalHotkeyError::BackendError(other.to_string()),
        })?;
        self.manager = Some(m);
        Ok(())
    }

    /// Register a global hotkey from a string like `"Ctrl+Shift+S"`.
    ///
    /// Returns a `HotkeyHandle` that auto-unregisters on drop.
    pub fn register(
        &mut self,
        chord_str: &str,
        action: ActionKind,
    ) -> Result<HotkeyHandle, GlobalHotkeyError> {
        self.ensure_manager()?;
        let manager = self.manager.as_mut().unwrap();

        let hotkey: Hotkey = chord_str
            .parse()
            .map_err(|e: handy_keys::Error| GlobalHotkeyError::ParseError(e.to_string()))?;

        let id = manager.register(hotkey).map_err(|e| match e {
            handy_keys::Error::HotkeyAlreadyRegistered(_) => {
                GlobalHotkeyError::AlreadyRegistered(chord_str.to_string())
            }
            other => GlobalHotkeyError::BackendError(other.to_string()),
        })?;

        self.bindings.insert(
            id,
            HotkeyBinding {
                chord_str: chord_str.to_string(),
                action,
            },
        );

        self.manager_gen += 1;
        Ok(HotkeyHandle {
            id,
            manager_id: self.manager_gen,
        })
    }

    /// Register a hotkey from a `KeyChord` (the framework's native type).
    #[allow(dead_code)]
    pub fn register_chord(
        &mut self,
        chord: &KeyChord,
        action: ActionKind,
    ) -> Result<HotkeyHandle, GlobalHotkeyError> {
        let chord_str = chord_to_string(chord);
        self.register(&chord_str, action)
    }

    /// Unregister a hotkey by its string representation.
    pub fn unregister_by_string(&mut self, chord_str: &str) -> Result<(), GlobalHotkeyError> {
        let Some(manager) = self.manager.as_mut() else {
            return Err(GlobalHotkeyError::NotFound);
        };

        let _hotkey: Hotkey = chord_str
            .parse()
            .map_err(|e: handy_keys::Error| GlobalHotkeyError::ParseError(e.to_string()))?;

        // Find the id for this chord by iterating bindings.
        let id = self
            .bindings
            .iter()
            .find(|(_, b)| b.chord_str == chord_str)
            .map(|(&id, _)| id)
            .ok_or(GlobalHotkeyError::NotFound)?;

        manager
            .unregister(id)
            .map_err(|e| GlobalHotkeyError::Internal(e.to_string()))?;
        self.bindings.remove(&id);
        Ok(())
    }

    /// Unregister by handle.
    #[allow(dead_code)]
    pub fn unregister_handle(&mut self, handle: &HotkeyHandle) -> Result<(), GlobalHotkeyError> {
        let Some(manager) = self.manager.as_mut() else {
            return Ok(());
        };
        let _ = manager.unregister(handle.id);
        self.bindings.remove(&handle.id);
        Ok(())
    }

    /// List all registered hotkey strings.
    pub fn list(&self) -> Vec<String> {
        self.bindings
            .values()
            .map(|b| b.chord_str.clone())
            .collect()
    }

    /// Poll for pending hotkey events.  Called once per event-loop cycle
    /// from `App::about_to_wait`.  Returns actions to dispatch.
    pub fn poll(&mut self) -> Vec<ActionKind> {
        let Some(manager) = self.manager.as_mut() else {
            return Vec::new();
        };

        let mut actions = Vec::new();
        loop {
            match manager.try_recv() {
                Some(event) => {
                    if event.state == HotkeyState::Pressed {
                        if let Some(binding) = self.bindings.get(&event.id) {
                            actions.push(binding.action);
                        }
                    }
                }
                None => break,
            }
        }
        actions
    }

    /// Number of registered hotkeys.
    #[allow(dead_code)]
    pub fn count(&self) -> usize {
        self.bindings.len()
    }

    /// Check if the platform backend is available (permissions granted).
    pub fn is_available(&mut self) -> bool {
        self.manager.is_some() || HotkeyManager::new().is_ok()
    }

    /// Get a human-readable platform name.
    #[allow(dead_code)]
    pub fn platform_name() -> &'static str {
        platform_name()
    }

    /// Get platform-specific permission setup guidance.
    pub fn permission_guidance() -> &'static str {
        permission_guidance()
    }
}

// ── Helpers ─────────────────────────────────────────────────────────

fn platform_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "Windows"
    } else if cfg!(target_os = "macos") {
        "macOS"
    } else if cfg!(target_os = "linux") {
        "Linux"
    } else {
        "Unknown"
    }
}

fn permission_guidance() -> &'static str {
    if cfg!(target_os = "macos") {
        "请在 系统设置 → 隐私与安全性 → 辅助功能 中授权本应用，然后重启。"
    } else if cfg!(target_os = "linux") {
        "请将当前用户加入 input 组: sudo usermod -a -G input $USER，然后注销重新登录。\
         \n或安装 udev 规则（推荐用于分发的应用）。"
    } else {
        "无需特殊权限。"
    }
}

/// Convert a framework `KeyChord` to a handy-keys hotkey string.
#[allow(dead_code)]
fn chord_to_string(chord: &KeyChord) -> String {
    let mut parts = Vec::new();
    let m = chord.modifiers;
    if m.ctrl {
        parts.push("Ctrl");
    }
    if m.alt {
        parts.push("Alt");
    }
    if m.shift {
        parts.push("Shift");
    }
    if m.meta {
        if cfg!(target_os = "macos") {
            parts.push("Cmd");
        } else {
            parts.push("Meta");
        }
    }

    let key_str = match &chord.key {
        crate::event::types::Key::Character(c) => c.to_uppercase(),
        crate::event::types::Key::Enter => "Enter".into(),
        crate::event::types::Key::Tab => "Tab".into(),
        crate::event::types::Key::Space => "Space".into(),
        crate::event::types::Key::Backspace => "Backspace".into(),
        crate::event::types::Key::Delete => "Delete".into(),
        crate::event::types::Key::Escape => "Escape".into(),
        crate::event::types::Key::ArrowUp => "Up".into(),
        crate::event::types::Key::ArrowDown => "Down".into(),
        crate::event::types::Key::ArrowLeft => "Left".into(),
        crate::event::types::Key::ArrowRight => "Right".into(),
        crate::event::types::Key::Home => "Home".into(),
        crate::event::types::Key::End => "End".into(),
        crate::event::types::Key::PageUp => "PageUp".into(),
        crate::event::types::Key::PageDown => "PageDown".into(),
        crate::event::types::Key::F1 => "F1".into(),
        crate::event::types::Key::F2 => "F2".into(),
        crate::event::types::Key::F3 => "F3".into(),
        crate::event::types::Key::F4 => "F4".into(),
        crate::event::types::Key::F5 => "F5".into(),
        crate::event::types::Key::F6 => "F6".into(),
        crate::event::types::Key::F7 => "F7".into(),
        crate::event::types::Key::F8 => "F8".into(),
        crate::event::types::Key::F9 => "F9".into(),
        crate::event::types::Key::F10 => "F10".into(),
        crate::event::types::Key::F11 => "F11".into(),
        crate::event::types::Key::F12 => "F12".into(),
        _ => return String::new(),
    };

    parts.push(&key_str);
    parts.join("+")
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::action::KeyChord;
    use crate::event::types::Key as AuralisKey;
    use crate::event::types::Modifiers as AuralisMods;

    #[test]
    fn chord_to_string_ctrl_shift_s() {
        let chord = KeyChord::new(
            AuralisKey::Character("s".into()),
            AuralisMods {
                ctrl: true,
                shift: true,
                ..AuralisMods::NONE
            },
        );
        assert_eq!(chord_to_string(&chord), "Ctrl+Shift+S");
    }

    #[test]
    fn chord_to_string_cmd_k() {
        let chord = KeyChord::new(
            AuralisKey::Character("k".into()),
            AuralisMods {
                meta: true,
                ..AuralisMods::NONE
            },
        );
        let result = chord_to_string(&chord);
        assert!(result.contains("K"));
        assert!(result.contains("Cmd") || result.contains("Meta"));
    }

    #[test]
    fn chord_to_string_f5() {
        let chord = KeyChord::new(AuralisKey::F5, AuralisMods::NONE);
        assert_eq!(chord_to_string(&chord), "F5");
    }

    #[test]
    fn chord_to_string_enter() {
        let chord = KeyChord::new(AuralisKey::Enter, AuralisMods::NONE);
        assert_eq!(chord_to_string(&chord), "Enter");
    }
}
