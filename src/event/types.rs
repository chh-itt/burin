//! Core event types for the event system.
//!
//! Events flow from winit raw events → [`Event`] translation → hit testing
//! → gesture recognition → Capture/Bubble propagation → user callbacks.

use crate::style::Point;

/// The complete set of high-level events in burin.
///
/// These are synthesized from raw winit events by the event translator.
#[derive(Clone, Debug)]
pub enum Event {
    // ── Pointer ──
    PointerMove {
        position: Point,
        finger_id: Option<u64>,
    },
    PointerDown {
        position: Point,
        button: MouseButton,
        finger_id: Option<u64>,
    },
    PointerUp {
        position: Point,
        button: MouseButton,
        finger_id: Option<u64>,
    },

    // ── Click ──
    Click {
        position: Point,
        button: MouseButton,
        finger_id: Option<u64>,
        modifiers: Modifiers,
    },

    // ── Drag ──
    DragStart {
        position: Point,
        button: MouseButton,
        finger_id: Option<u64>,
    },
    DragMove {
        position: Point,
        delta_x: f32,
        delta_y: f32,
        button: MouseButton,
        finger_id: Option<u64>,
    },
    DragEnd {
        position: Point,
        button: MouseButton,
        finger_id: Option<u64>,
    },

    // ── Drag cancellation (system interrupt: ACTION_CANCEL, focus loss) ──
    DragCancel {
        finger_id: Option<u64>,
    },

    // ── Scroll ──
    Scroll {
        delta_x: f32,
        delta_y: f32,
    },

    // ── Keyboard ──
    KeyDown {
        key: Key,
        modifiers: Modifiers,
    },
    KeyUp {
        key: Key,
        modifiers: Modifiers,
    },

    // ── Touch gestures (macOS / Wayland / iOS) ──
    Pinch {
        delta: f64,
        position: Point,
        phase: GesturePhase,
    },
    Rotate {
        delta: f32,
        position: Point,
        phase: GesturePhase,
    },

    // ── Extension ──
    Custom {
        type_id: std::any::TypeId,
        payload: std::rc::Rc<dyn std::any::Any>,
    },
}

/// Phase of a multi-touch gesture.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum GesturePhase {
    Started,
    Moved,
    Ended,
    Cancelled,
}

impl Event {
    pub fn finger_id(&self) -> Option<u64> {
        match self {
            Event::PointerMove { finger_id, .. }
            | Event::PointerDown { finger_id, .. }
            | Event::PointerUp { finger_id, .. }
            | Event::Click { finger_id, .. }
            | Event::DragStart { finger_id, .. }
            | Event::DragMove { finger_id, .. }
            | Event::DragEnd { finger_id, .. }
            | Event::DragCancel { finger_id, .. } => *finger_id,
            _ => None,
        }
    }
}

/// Mouse button identifiers.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
    Back,
    Forward,
    Other(u16),
}

/// Keyboard key identifiers.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum Key {
    Character(String),
    Enter,
    Tab,
    Space,
    Backspace,
    Delete,
    Escape,
    ArrowUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,
    Home,
    End,
    PageUp,
    PageDown,
    Shift,
    Control,
    Alt,
    Meta,
    F1,
    F2,
    F3,
    F4,
    F5,
    F6,
    F7,
    F8,
    F9,
    F10,
    F11,
    F12,
    Other(String),
}

/// Keyboard modifier flags.
#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub struct Modifiers {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub meta: bool,
}

impl Modifiers {
    pub const NONE: Self = Self {
        shift: false,
        ctrl: false,
        alt: false,
        meta: false,
    };
}

/// The phase of event propagation.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EventPhase {
    /// Root → target (top-down).
    Capture,
    /// Target → root (bottom-up).
    Bubble,
}

/// Result of event handling by a widget.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EventStatus {
    /// Event was not handled, continue propagation.
    Ignored,
    /// Event was handled, continue to parent if in Bubble phase.
    Consumed,
}

// ── Key held state ─────────────────────────────────────────────────

/// Held-key information for implementing press-and-hold acceleration.
///
/// ```ignore
/// fn on_key_down(key: Key, mods: Modifiers, held: Option<KeyHeldInfo>) {
///     if key == Key::Backspace {
///         let n = held.map_or(1, |h| h.repeat_multiplier().ceil() as usize);
///         delete_chars(n);
///     }
/// }
/// ```
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct KeyHeldInfo {
    pub held_duration: std::time::Duration,
    pub repeat_count: u32,
}

impl KeyHeldInfo {
    pub fn repeat_multiplier(&self) -> f32 {
        let ms = self.held_duration.as_millis() as f32;
        if ms < 400.0 {
            return 1.0;
        }
        if ms > 2000.0 {
            return 4.0;
        }
        1.0 + (ms - 400.0) / 1600.0 * 3.0
    }
}

// ── Drag & Drop ──────────────────────────────────────────────────

/// Drag axis constraint.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DragAxis {
    Free,
    Horizontal,
    Vertical,
}

/// Accepted drop data type.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum DropType {
    Text,
    Files,
    Custom(String),
}

impl DropType {
    /// Check whether this `DropType` accepts the given `DragKind`.
    pub fn matches(&self, kind: &DragKind) -> bool {
        match (self, kind) {
            (DropType::Text, DragKind::Text) => true,
            (DropType::Files, DragKind::Files) => true,
            (DropType::Custom(a), DragKind::Custom(b)) => a == b,
            _ => false,
        }
    }
}

/// Drag payload data.
#[derive(Clone, Debug)]
pub struct DragData {
    pub kind: DragKind,
    pub text: Option<String>,
    pub paths: Vec<std::path::PathBuf>,
    /// Screen position where the drop occurred. Used by drop handlers
    /// to compute insertion indices for drag-reorder (List/Tree).
    pub position: Option<Point>,
    /// Preferred label for the drag ghost chip. Falls back to `text` then
    /// the element's accessible label if not set.
    pub label: Option<String>,
}

impl DragData {
    /// Create a text drag payload.
    pub fn text(text: impl Into<String>) -> Self {
        let t = text.into();
        Self {
            kind: DragKind::Text,
            label: Some(t.clone()),
            text: Some(t),
            paths: Vec::new(),
            position: None,
        }
    }

    /// Create a file drag payload.
    pub fn files(paths: Vec<std::path::PathBuf>) -> Self {
        Self {
            kind: DragKind::Files,
            text: None,
            label: None,
            paths,
            position: None,
        }
    }

    /// Create a custom drag payload with a format identifier and data string.
    pub fn custom(kind: impl Into<String>, data: impl Into<String>) -> Self {
        Self {
            kind: DragKind::Custom(kind.into()),
            text: Some(data.into()),
            label: None,
            paths: Vec::new(),
            position: None,
        }
    }

    /// Convenience: return the text payload, if this is a text/custom drag.
    pub fn as_text(&self) -> Option<&str> {
        self.text.as_deref()
    }
}

impl Default for DragData {
    fn default() -> Self {
        Self {
            kind: DragKind::Files,
            text: None,
            paths: Vec::new(),
            position: None,
            label: None,
        }
    }
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum DragKind {
    Text,
    Files,
    Custom(String),
}

impl DragKind {
    /// Return the custom kind identifier, if this is a Custom variant.
    pub fn as_custom_str(&self) -> Option<&str> {
        match self {
            DragKind::Custom(s) => Some(s),
            _ => None,
        }
    }

    /// Convert this `DragKind` into a `DropType` for acceptance checking.
    pub fn to_drop_type(&self) -> DropType {
        match self {
            DragKind::Text => DropType::Text,
            DragKind::Files => DropType::Files,
            DragKind::Custom(s) => DropType::Custom(s.clone()),
        }
    }
}
