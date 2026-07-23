//! Event system: event types, hit testing, gesture recognition, focus, keyboard.

pub mod action;
pub mod bindings;
pub mod click;
pub mod dropdown;
pub mod focus;
pub mod focus_manager;
pub mod focus_traversal;
pub mod hit_test;
pub mod overlay;
pub mod propagation;
pub mod recognizer;
pub mod registry;
pub mod translator;
pub mod types;

pub use click::{ClickCounter, ClickResult};
pub use dropdown::DropdownKeyboard;

pub use focus::{
    current_modal_scope_root, is_in_modal_scope, pop_modal_scope, push_modal_scope,
    remove_modal_scopes_of, FocusHighlightMode, FocusReason, TraversalEdgeBehavior,
};
pub use focus_manager::FocusManager;
pub use focus_traversal::{Direction, TabOrderPolicy, TraversalPolicy, WidgetOrderPolicy};
pub use recognizer::{
    drain_long_press_wins, process_pointer_event, register_recognizer, reset_all, scroll_capture,
    take_click_suppressed, unregister_recognizer, DoubleTapRecognizer, DragRecognizer,
    EagerDragRecognizer, GestureDomain, GestureWin, LongPressRecognizer, Recognizer,
    RecognizerKind, RecognizerRegistration, RecognizerResult, ScrollRecognizer, TapRecognizer,
    TOUCH_SLOP,
};
pub use registry::{DragArbitration, EventRegistry};
pub use translator::EventTranslator;
pub use types::{
    DragAxis, DragData, DragKind, DropType, Event, EventPhase, EventStatus, GesturePhase, Key,
    KeyHeldInfo, Modifiers, MouseButton,
};
