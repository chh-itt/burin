//! Key-binding map: physical key chords → [`ActionKind`].
//!
//! Three-layer priority model (lowest → highest):
//! 1. **App shortcuts** — always active when the window has focus.
//! 2. **Widget shortcuts** — active only when a specific widget is focused.
//!    Widget bindings override app bindings for the same chord.
//! 3. **System global hotkeys** — handled by `GlobalHotkeyManager`,
//!    active even when the window is NOT focused.
//!
//! Default app-level bindings for standard editing and navigation are
//! registered in [`KeyBindingMap::new`].  Widget-level bindings can be
//! added via `register`.

use std::collections::HashMap;

use crate::core::ElementId;
use crate::event::action::{ActionKind, KeyChord};
use crate::event::types::{Key, Modifiers};

/// Maps key chords to semantic actions, scoped at the application level
/// or per-element.
pub struct KeyBindingMap {
    /// Application-level shortcuts — active when any window has focus.
    app: Vec<(KeyChord, ActionKind)>,
    /// Widget-level shortcuts — active only when the specific widget is focused.
    per_widget: HashMap<ElementId, Vec<(KeyChord, ActionKind)>>,
}

impl KeyBindingMap {
    /// Create a map with all built-in default bindings.
    pub fn new() -> Self {
        let mut map = Self {
            app: Vec::new(),
            per_widget: HashMap::new(),
        };
        map.register_defaults();
        map
    }

    /// Register an application-level key chord → action mapping.
    pub fn register(&mut self, chord: KeyChord, action: ActionKind) {
        self.app.push((chord, action));
    }

    /// Register a per-widget key chord → action mapping.
    pub fn register_for(&mut self, id: ElementId, chord: KeyChord, action: ActionKind) {
        self.per_widget.entry(id).or_default().push((chord, action));
    }

    /// Remove all bindings for a given chord (both app-level and per-widget).
    pub fn remove(&mut self, chord: &KeyChord) {
        self.app.retain(|(c, _)| c != chord);
        for bindings in self.per_widget.values_mut() {
            bindings.retain(|(c, _)| c != chord);
        }
    }

    /// Look up the action for a given key combination.
    /// Checks per-widget bindings first, then app-level.
    pub fn find(
        &self,
        id: Option<ElementId>,
        key: &Key,
        modifiers: &Modifiers,
    ) -> Option<ActionKind> {
        if let Some(id) = id {
            if let Some(bindings) = self.per_widget.get(&id) {
                for (chord, action) in bindings {
                    if chord.key == *key && chord.modifiers == *modifiers {
                        return Some(*action);
                    }
                }
            }
        }
        for (chord, action) in &self.app {
            if chord.key == *key && chord.modifiers == *modifiers {
                return Some(*action);
            }
        }
        None
    }

    fn register_defaults(&mut self) {
        let ctrl = Modifiers {
            ctrl: true,
            ..Modifiers::NONE
        };
        let shift = Modifiers {
            shift: true,
            ..Modifiers::NONE
        };
        let ctrl_shift = Modifiers {
            ctrl: true,
            shift: true,
            ..Modifiers::NONE
        };
        let none = Modifiers::NONE;

        // ── Clipboard ──
        self.register(
            KeyChord::new(Key::Character("c".into()), ctrl),
            ActionKind::Copy,
        );
        self.register(
            KeyChord::new(Key::Character("v".into()), ctrl),
            ActionKind::Paste,
        );
        self.register(
            KeyChord::new(Key::Character("x".into()), ctrl),
            ActionKind::Cut,
        );
        self.register(
            KeyChord::new(Key::Character("a".into()), ctrl),
            ActionKind::SelectAll,
        );

        // ── Undo / Redo ──
        self.register(
            KeyChord::new(Key::Character("z".into()), ctrl),
            ActionKind::Undo,
        );
        self.register(
            KeyChord::new(Key::Character("y".into()), ctrl),
            ActionKind::Redo,
        );
        self.register(
            KeyChord::new(Key::Character("z".into()), ctrl_shift),
            ActionKind::Redo,
        );

        // ── Focus ──
        self.register(KeyChord::new(Key::Tab, none), ActionKind::FocusNext);
        self.register(KeyChord::new(Key::Tab, shift), ActionKind::FocusPrev);
        self.register(KeyChord::new(Key::Tab, ctrl), ActionKind::InsertTab);

        // ── Activation ──
        self.register(KeyChord::new(Key::Enter, none), ActionKind::NewLine);
        self.register(KeyChord::new(Key::Space, none), ActionKind::Activate);
        self.register(KeyChord::new(Key::Escape, none), ActionKind::Cancel);

        // ── Navigation ──
        self.register(KeyChord::new(Key::ArrowLeft, none), ActionKind::MoveLeft);
        self.register(KeyChord::new(Key::ArrowRight, none), ActionKind::MoveRight);
        self.register(KeyChord::new(Key::ArrowUp, none), ActionKind::MoveUp);
        self.register(KeyChord::new(Key::ArrowDown, none), ActionKind::MoveDown);
        self.register(KeyChord::new(Key::ArrowLeft, shift), ActionKind::MoveLeft);
        self.register(KeyChord::new(Key::ArrowRight, shift), ActionKind::MoveRight);
        self.register(KeyChord::new(Key::ArrowUp, shift), ActionKind::MoveUp);
        self.register(KeyChord::new(Key::ArrowDown, shift), ActionKind::MoveDown);
        self.register(KeyChord::new(Key::Home, none), ActionKind::MoveHome);
        self.register(KeyChord::new(Key::End, none), ActionKind::MoveEnd);
        self.register(KeyChord::new(Key::Home, shift), ActionKind::MoveHome);
        self.register(KeyChord::new(Key::End, shift), ActionKind::MoveEnd);

        // ── Page navigation ──
        self.register(KeyChord::new(Key::PageDown, none), ActionKind::MovePageDown);
        self.register(KeyChord::new(Key::PageUp, none), ActionKind::MovePageUp);
        self.register(
            KeyChord::new(Key::PageDown, shift),
            ActionKind::MovePageDown,
        );
        self.register(KeyChord::new(Key::PageUp, shift), ActionKind::MovePageUp);

        // ── Word navigation ──
        self.register(
            KeyChord::new(Key::ArrowLeft, ctrl),
            ActionKind::MoveWordLeft,
        );
        self.register(
            KeyChord::new(Key::ArrowRight, ctrl),
            ActionKind::MoveWordRight,
        );
        self.register(
            KeyChord::new(Key::ArrowLeft, ctrl_shift),
            ActionKind::MoveWordLeft,
        );
        self.register(
            KeyChord::new(Key::ArrowRight, ctrl_shift),
            ActionKind::MoveWordRight,
        );

        // ── Editing ──
        self.register(
            KeyChord::new(Key::Backspace, none),
            ActionKind::DeleteBackward,
        );
        self.register(KeyChord::new(Key::Delete, none), ActionKind::DeleteForward);
        self.register(
            KeyChord::new(Key::Backspace, ctrl),
            ActionKind::DeleteWordBackward,
        );
        self.register(
            KeyChord::new(Key::Delete, ctrl),
            ActionKind::DeleteWordForward,
        );
        self.register(KeyChord::new(Key::Enter, ctrl), ActionKind::Submit);
    }
}

impl Default for KeyBindingMap {
    fn default() -> Self {
        Self::new()
    }
}
