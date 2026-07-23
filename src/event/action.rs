//! Action system: semantic actions dispatched through the widget tree
//! via Bubble propagation, replacing ad-hoc keyboard shortcuts.
//!
//! Each action has a [`KeyBindingMap`](super::bindings::KeyBindingMap) entry
//! mapping physical keys to semantic actions (app-level or per-widget), and a
//! handler chain in [`EventRegistry`](super::registry::EventRegistry) that
//! widgets register on. System-global hotkeys are managed separately by
//! [`GlobalHotkeyManager`](super::super::platform::global_hotkey::GlobalHotkeyManager).

use crate::event::types::{Key, Modifiers};

/// A typed action that can be dispatched through the widget tree.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Action {
    pub kind: ActionKind,
    /// Whether the Shift modifier is active (navigation → selection extension).
    pub selection: bool,
}

impl Action {
    pub fn new(kind: ActionKind) -> Self {
        Self {
            kind,
            selection: false,
        }
    }

    pub fn with_selection(mut self) -> Self {
        self.selection = true;
        self
    }
}

/// Semantic operation identifiers.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ActionKind {
    // ── Clipboard ──
    Copy,
    Cut,
    Paste,
    SelectAll,

    // ── Undo / Redo ──
    Undo,
    Redo,

    // ── Navigation ──
    MoveLeft,
    MoveRight,
    MoveUp,
    MoveDown,
    MoveWordLeft,
    MoveWordRight,
    MoveHome,
    MoveEnd,
    MovePageUp,
    MovePageDown,

    // ── Editing ──
    DeleteForward,
    DeleteBackward,
    DeleteWordForward,
    DeleteWordBackward,
    NewLine,
    InsertTab,

    // ── Focus ──
    FocusNext,
    FocusPrev,

    // ── Activation ──
    Activate,
    Cancel,
    Submit,

    // ── Accessibility (screen reader actions) ──
    A11yIncrement,
    A11yDecrement,
    A11yExpand,
    A11yCollapse,
    A11yShowTooltip,
    A11yHideTooltip,

    /// Widget-specific action (de-duplicated by string identity).
    Custom {
        id: &'static str,
    },
}

/// A physical key chord (key + modifiers).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct KeyChord {
    pub key: Key,
    pub modifiers: Modifiers,
}

impl KeyChord {
    pub fn new(key: Key, modifiers: Modifiers) -> Self {
        Self { key, modifiers }
    }
}

/// Outcome of dispatching an action through the Bubble chain.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ActionOutcome {
    /// No handler processed this action → default behaviour (if any) runs.
    Unhandled,
    /// A handler consumed the action → default behaviour is suppressed,
    /// but propagation continues upward.
    Consumed,
    /// A handler consumed the action and stopped further propagation.
    Blocked,
}

impl ActionOutcome {
    pub fn is_handled(self) -> bool {
        !matches!(self, Self::Unhandled)
    }

    pub fn should_stop(self) -> bool {
        matches!(self, Self::Blocked)
    }
}
