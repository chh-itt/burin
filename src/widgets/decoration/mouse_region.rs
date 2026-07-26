use crate::core::config::EventHandler;
use crate::core::context::MountContext;
use crate::core::widget::Widget;
use crate::core::ElementId;
use crate::event::action::{Action, ActionOutcome};
use crate::event::types::{GesturePhase, Key, Modifiers};
use crate::event::FocusReason;
use crate::platform::CursorIcon;
use crate::style::Point;

/// A widget that captures pointer, keyboard, and gesture events.
pub struct MouseRegion {
    child: Option<Box<dyn Widget>>,
    // ── Click ──
    on_click: Option<Box<dyn Fn()>>,
    on_click_at: Option<Box<dyn FnMut(Point)>>,
    on_click_with_mods: Option<Box<dyn FnMut(Modifiers)>>,
    on_click_at_with_mods: Option<Box<dyn FnMut(Point, Modifiers)>>,
    on_double_click: Option<Box<dyn Fn()>>,
    on_triple_click: Option<Box<dyn Fn()>>,
    // ── Hover ──
    on_hover_enter: Option<Box<dyn Fn()>>,
    on_hover_leave: Option<Box<dyn Fn()>>,
    // ── Focus ──
    on_focus_in: Option<Box<dyn Fn(FocusReason)>>,
    on_focus_out: Option<Box<dyn Fn(FocusReason)>>,
    // ── Long press ──
    on_long_press: Option<Box<dyn Fn()>>,
    // ── Drag ──
    on_drag_start: Option<Box<dyn FnMut(Point, Point)>>,
    on_drag_update: Option<Box<dyn FnMut(Point, Point)>>,
    on_drag_end: Option<Box<dyn FnMut(Point, Point)>>,
    drag_arbitration: Option<crate::event::DragArbitration>,
    // ── Keyboard ──
    on_key_down: Option<Box<dyn FnMut(Key, Modifiers) -> bool>>,
    on_key_up: Option<Box<dyn FnMut(Key, Modifiers) -> bool>>,
    // ── Scroll ──
    on_scroll: Option<Box<dyn FnMut(f32, f32) -> bool>>,
    // ── Pinch / Rotate ──
    on_pinch: Option<Box<dyn FnMut(f64, GesturePhase) -> bool>>,
    on_rotate: Option<Box<dyn FnMut(f32, GesturePhase) -> bool>>,
    // ── Resize ──
    on_resize: Option<Box<dyn FnMut(f32, f32)>>,
    // ── IME ──
    on_text_input: Option<Box<dyn FnMut(char)>>,
    on_preedit: Option<Box<dyn FnMut(String, Option<(usize, usize)>)>>,
    // ── Action / Clipboard ──
    on_action: Option<Box<dyn FnMut(&Action) -> ActionOutcome>>,
    on_clipboard_copy: Option<Box<dyn Fn() -> String>>,
    on_clipboard_paste: Option<Box<dyn FnMut(String)>>,
    // ── Configuration ──
    cursor: Option<CursorIcon>,
    opaque: bool,
    input_pass_through: bool,
    enabled: bool,
    focusable: bool,
}

impl MouseRegion {
    pub fn new(widget: impl Widget + 'static) -> Self {
        Self {
            child: Some(Box::new(widget)),
            on_click: None,
            on_click_at: None,
            on_click_with_mods: None,
            on_click_at_with_mods: None,
            on_double_click: None,
            on_triple_click: None,
            on_hover_enter: None,
            on_hover_leave: None,
            on_focus_in: None,
            on_focus_out: None,
            on_long_press: None,
            on_drag_start: None,
            on_drag_update: None,
            on_drag_end: None,
            drag_arbitration: None,
            on_key_down: None,
            on_key_up: None,
            on_scroll: None,
            on_pinch: None,
            on_rotate: None,
            on_resize: None,
            on_text_input: None,
            on_preedit: None,
            on_action: None,
            on_clipboard_copy: None,
            on_clipboard_paste: None,
            cursor: None,
            opaque: false,
            input_pass_through: false,
            enabled: true,
            focusable: false,
        }
    }

    // ── Click variants ──

    pub fn on_click(mut self, f: impl Fn() + 'static) -> Self {
        self.on_click = Some(Box::new(f));
        self
    }
    pub fn on_click_at(mut self, f: impl FnMut(Point) + 'static) -> Self {
        self.on_click_at = Some(Box::new(f));
        self
    }
    pub fn on_click_with_mods(mut self, f: impl FnMut(Modifiers) + 'static) -> Self {
        self.on_click_with_mods = Some(Box::new(f));
        self
    }
    pub fn on_click_at_with_mods(mut self, f: impl FnMut(Point, Modifiers) + 'static) -> Self {
        self.on_click_at_with_mods = Some(Box::new(f));
        self
    }
    pub fn on_double_click(mut self, f: impl Fn() + 'static) -> Self {
        self.on_double_click = Some(Box::new(f));
        self
    }
    pub fn on_triple_click(mut self, f: impl Fn() + 'static) -> Self {
        self.on_triple_click = Some(Box::new(f));
        self
    }

    // ── Hover ──

    pub fn on_hover_enter(mut self, f: impl Fn() + 'static) -> Self {
        self.on_hover_enter = Some(Box::new(f));
        self
    }
    pub fn on_hover_leave(mut self, f: impl Fn() + 'static) -> Self {
        self.on_hover_leave = Some(Box::new(f));
        self
    }

    // ── Focus ──

    pub fn on_focus_in(mut self, f: impl Fn(FocusReason) + 'static) -> Self {
        self.on_focus_in = Some(Box::new(f));
        self
    }
    pub fn on_focus_out(mut self, f: impl Fn(FocusReason) + 'static) -> Self {
        self.on_focus_out = Some(Box::new(f));
        self
    }

    // ── Long press ──

    pub fn on_long_press(mut self, f: impl Fn() + 'static) -> Self {
        self.on_long_press = Some(Box::new(f));
        self
    }

    // ── Drag ──

    pub fn on_drag_start(mut self, f: impl FnMut(Point, Point) + 'static) -> Self {
        self.on_drag_start = Some(Box::new(f));
        self
    }
    pub fn on_drag_update(mut self, f: impl FnMut(Point, Point) + 'static) -> Self {
        self.on_drag_update = Some(Box::new(f));
        self
    }
    pub fn on_drag_end(mut self, f: impl FnMut(Point, Point) + 'static) -> Self {
        self.on_drag_end = Some(Box::new(f));
        self
    }

    pub fn on_drag(
        mut self,
        start: impl FnMut(Point, Point) + 'static,
        update: impl FnMut(Point, Point) + 'static,
        end: impl FnMut(Point, Point) + 'static,
    ) -> Self {
        self.on_drag_start = Some(Box::new(start));
        self.on_drag_update = Some(Box::new(update));
        self.on_drag_end = Some(Box::new(end));
        self
    }

    /// Threshold-gated drag (tap-vs-drag disambiguation) or the eager
    /// zero-threshold default. See [`crate::event::DragArbitration`].
    pub fn drag_arbitration(mut self, mode: crate::event::DragArbitration) -> Self {
        self.drag_arbitration = Some(mode);
        self
    }

    // ── Keyboard ──

    pub fn on_key_down(mut self, f: impl FnMut(Key, Modifiers) -> bool + 'static) -> Self {
        self.on_key_down = Some(Box::new(f));
        self
    }
    pub fn on_key_up(mut self, f: impl FnMut(Key, Modifiers) -> bool + 'static) -> Self {
        self.on_key_up = Some(Box::new(f));
        self
    }

    // ── Scroll ──

    pub fn on_scroll(mut self, f: impl FnMut(f32, f32) -> bool + 'static) -> Self {
        self.on_scroll = Some(Box::new(f));
        self
    }

    // ── Pinch / Rotate ──

    pub fn on_pinch(mut self, f: impl FnMut(f64, GesturePhase) -> bool + 'static) -> Self {
        self.on_pinch = Some(Box::new(f));
        self
    }
    pub fn on_rotate(mut self, f: impl FnMut(f32, GesturePhase) -> bool + 'static) -> Self {
        self.on_rotate = Some(Box::new(f));
        self
    }

    // ── Resize ──

    pub fn on_resize(mut self, f: impl FnMut(f32, f32) + 'static) -> Self {
        self.on_resize = Some(Box::new(f));
        self
    }

    // ── IME ──

    pub fn on_text_input(mut self, f: impl FnMut(char) + 'static) -> Self {
        self.on_text_input = Some(Box::new(f));
        self
    }
    pub fn on_preedit(mut self, f: impl FnMut(String, Option<(usize, usize)>) + 'static) -> Self {
        self.on_preedit = Some(Box::new(f));
        self
    }

    // ── Action / Clipboard ──

    pub fn on_action(mut self, f: impl FnMut(&Action) -> ActionOutcome + 'static) -> Self {
        self.on_action = Some(Box::new(f));
        self
    }
    pub fn on_clipboard_copy(mut self, f: impl Fn() -> String + 'static) -> Self {
        self.on_clipboard_copy = Some(Box::new(f));
        self
    }
    pub fn on_clipboard_paste(mut self, f: impl FnMut(String) + 'static) -> Self {
        self.on_clipboard_paste = Some(Box::new(f));
        self
    }

    // ── Configuration ──

    pub fn cursor(mut self, cursor: CursorIcon) -> Self {
        self.cursor = Some(cursor);
        self
    }

    /// Make this region consume hit tests (prevent events from passing through to widgets behind).
    /// Equivalent to Flutter's `HitTestBehavior.opaque`.
    pub fn opaque(mut self) -> Self {
        self.opaque = true;
        self
    }

    /// Allow events to pass through to widgets behind this region.
    /// Equivalent to Flutter's `HitTestBehavior.translucent`.
    pub fn pass_through(mut self) -> Self {
        self.input_pass_through = true;
        self
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    pub fn focusable(mut self, focusable: bool) -> Self {
        self.focusable = focusable;
        self
    }
}

impl Widget for MouseRegion {
    fn mount_box(mut self: Box<Self>, ctx: &mut MountContext<'_>) -> ElementId {
        let id = ctx.arena.allocate();
        let needs_interaction = self.enabled
            && (self.on_click.is_some()
                || self.on_hover_enter.is_some()
                || self.on_hover_leave.is_some()
                || self.on_drag_start.is_some()
                || self.on_scroll.is_some()
                || self.is_interactive());

        if needs_interaction {
            ctx.preallocate(id, crate::ecs::components::INTERACTION);
        }

        {
            let Some(element) = ctx.arena.get_mut(id) else {
                return id;
            };
            element.set_focusable(self.focusable);
            if let Some(cursor) = self.cursor {
                element.set_cursor_icon(Some(cursor));
            }
            if self.input_pass_through {
                element.set_input_pass_through(true);
            }
        }

        if let Some(child) = self.child.take() {
            let child_id = child.mount_box(&mut ctx.child_with_events(id));
            ctx.arena.add_child(id, child_id);
        }

        if self.enabled {
            let mut events = EventHandler::new();
            if let Some(f) = self.on_click {
                events = events.on_click(f);
            }
            if let Some(f) = self.on_click_at {
                events = events.on_click_at(f);
            }
            if let Some(f) = self.on_click_with_mods {
                events = events.on_click_with_mods(f);
            }
            if let Some(f) = self.on_click_at_with_mods {
                events = events.on_click_at_with_mods(f);
            }
            if let Some(f) = self.on_double_click {
                events = events.on_double_click(f);
            }
            if let Some(f) = self.on_triple_click {
                events = events.on_triple_click(f);
            }
            if let Some(f) = self.on_hover_enter {
                events = events.on_hover_enter(f);
            }
            if let Some(f) = self.on_hover_leave {
                events = events.on_hover_leave(f);
            }
            if let Some(f) = self.on_focus_in {
                events = events.on_focus_in(f);
            }
            if let Some(f) = self.on_focus_out {
                events = events.on_focus_out(f);
            }
            if let Some(f) = self.on_long_press {
                events = events.on_long_press(f);
            }
            if let Some(f) = self.on_drag_start {
                events = events.on_drag_start(f);
            }
            if let Some(f) = self.on_drag_update {
                events = events.on_drag_update(f);
            }
            if let Some(f) = self.on_drag_end {
                events = events.on_drag_end(f);
            }
            if let Some(mode) = self.drag_arbitration {
                events = events.drag_arbitration(mode);
            }
            if let Some(f) = self.on_key_down {
                events = events.on_key_down(f);
            }
            if let Some(f) = self.on_key_up {
                events = events.on_key_up(f);
            }
            if let Some(f) = self.on_scroll {
                events = events.on_scroll(f);
            }
            if let Some(f) = self.on_pinch {
                events = events.on_pinch(f);
            }
            if let Some(f) = self.on_rotate {
                events = events.on_rotate(f);
            }
            if let Some(f) = self.on_resize {
                events = events.on_resize(f);
            }
            if let Some(f) = self.on_text_input {
                events = events.on_text_input(f);
            }
            if let Some(f) = self.on_preedit {
                events = events.on_preedit(f);
            }
            if let Some(f) = self.on_action {
                events = events.on_action(f);
            }
            if let Some(f) = self.on_clipboard_copy {
                events = events.on_clipboard_copy(f);
            }
            if let Some(f) = self.on_clipboard_paste {
                events = events.on_clipboard_paste(f);
            }

            if let Some(reg) = ctx.event_registry.as_mut() {
                events.register_all(reg, id);
            }
        }

        id
    }
}

impl MouseRegion {
    fn is_interactive(&self) -> bool {
        self.on_click_at.is_some()
            || self.on_click_with_mods.is_some()
            || self.on_click_at_with_mods.is_some()
            || self.on_double_click.is_some()
            || self.on_triple_click.is_some()
            || self.on_focus_in.is_some()
            || self.on_focus_out.is_some()
            || self.on_long_press.is_some()
            || self.on_drag_update.is_some()
            || self.on_drag_end.is_some()
            || self.on_key_down.is_some()
            || self.on_key_up.is_some()
            || self.on_pinch.is_some()
            || self.on_rotate.is_some()
            || self.on_resize.is_some()
            || self.on_text_input.is_some()
            || self.on_preedit.is_some()
            || self.on_action.is_some()
            || self.on_clipboard_copy.is_some()
            || self.on_clipboard_paste.is_some()
    }
}

impl std::fmt::Debug for MouseRegion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MouseRegion")
            .field("has_child", &self.child.is_some())
            .field("cursor", &self.cursor)
            .field("opaque", &self.opaque)
            .field("enabled", &self.enabled)
            .field("focusable", &self.focusable)
            .finish_non_exhaustive()
    }
}
