use std::cell::{Cell, RefCell};
use std::rc::Rc;

use crate::core::clock;
use crate::core::config::{
    ElementBuilder, EventHandler, InteractionConfig, LayoutConfig, Overflow, PaintConfig,
};
use crate::core::context::MountContext;
use crate::core::dirty_registry;
use crate::core::element::DirtyFlags;
use crate::core::widget::Widget;
use crate::core::ElementId;
use crate::event::action::{Action, ActionKind, ActionOutcome};
use crate::event::FocusReason;
use crate::style::styled::{StyleRefinement, Styled};
use crate::style::{Dimension, Point, Rect, Vec2};
use crate::theme::m3::roles::{ComponentRole, InteractiveRole, ResolvedComponentStyle};
use crate::theme::InputVariant;
use auralis_signal::Signal;

use super::text_editor::action::handle_action;
use super::text_editor::composition::{composition_rects, Composition};
use super::text_editor::render::{
    auto_scroll, build_buffer, cursor_pixel_pos, display_text, selection_rects,
};
use super::text_editor::state::{
    EditorState, TextInputConfig, TextInputType as EditorTextInputType,
};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TextInputType {
    Text,
    Password,
    Number,
    Email,
    Url,
    Multiline,
}

/// A single-line or multi-line text field.
///
/// Binds to a `Signal<String>` for two-way text editing.  Supports
/// placeholder text, max length, password masking, IME composition,
/// and undo/redo.
pub struct TextInput {
    value: Signal<String>,
    placeholder: String,
    input_type: TextInputType,
    max_length: Option<usize>,
    read_only: bool,
    disabled: bool,
    validation: Option<Box<dyn Fn(&str) -> bool>>,
    error_message: Option<String>,
    on_value_changed: Option<Box<dyn Fn(String)>>,
    on_submit: Option<Box<dyn Fn(String)>>,
    tab_index: Option<usize>,
    autofocus: bool,
    /// When set, navigation keys (arrows, Home/End, etc.) return
    /// `ActionOutcome::Unhandled` so a parent ComboBox can intercept them.
    suppress_nav: Option<Rc<Cell<bool>>>,
    style: StyleRefinement,
}

impl TextInput {
    pub fn new(value: Signal<String>) -> Self {
        Self {
            value,
            placeholder: String::new(),
            input_type: TextInputType::Text,
            max_length: None,
            read_only: false,
            disabled: false,
            validation: None,
            error_message: None,
            on_value_changed: None,
            on_submit: None,
            tab_index: None,
            autofocus: false,
            suppress_nav: None,
            style: StyleRefinement::default(),
        }
    }
    pub fn placeholder(mut self, p: impl Into<String>) -> Self {
        self.placeholder = p.into();
        self
    }
    pub fn input_type(mut self, t: TextInputType) -> Self {
        self.input_type = t;
        self
    }
    pub fn max_length(mut self, n: usize) -> Self {
        self.max_length = Some(n);
        self
    }
    pub fn read_only(mut self) -> Self {
        self.read_only = true;
        self
    }
    pub fn disabled(mut self) -> Self {
        self.disabled = true;
        self
    }
    pub fn validation(mut self, f: impl Fn(&str) -> bool + 'static) -> Self {
        self.validation = Some(Box::new(f));
        self
    }
    pub fn error_message(mut self, msg: impl Into<String>) -> Self {
        self.error_message = Some(msg.into());
        self
    }
    pub fn on_value_changed(mut self, f: impl Fn(String) + 'static) -> Self {
        self.on_value_changed = Some(Box::new(f));
        self
    }
    pub fn on_submit(mut self, f: impl Fn(String) + 'static) -> Self {
        self.on_submit = Some(Box::new(f));
        self
    }
    pub fn tab_index(mut self, idx: usize) -> Self {
        self.tab_index = Some(idx);
        self
    }
    pub fn autofocus(mut self) -> Self {
        self.autofocus = true;
        self
    }
    /// Suppress navigation key handling (arrows, Home/End, etc.).
    /// When the cell is `true`, those keys return `Unhandled` so a
    /// parent ComboBox can intercept them for dropdown navigation.
    pub fn suppress_nav_keys(mut self, flag: Rc<Cell<bool>>) -> Self {
        self.suppress_nav = Some(flag);
        self
    }
}

impl Styled for TextInput {
    fn style_refinement(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

fn to_editor_type(t: TextInputType) -> EditorTextInputType {
    match t {
        TextInputType::Text => EditorTextInputType::Text,
        TextInputType::Password => EditorTextInputType::Password,
        TextInputType::Number => EditorTextInputType::Number,
        TextInputType::Email => EditorTextInputType::Email,
        TextInputType::Url => EditorTextInputType::Url,
        TextInputType::Multiline => EditorTextInputType::Multiline,
    }
}

impl Widget for TextInput {
    fn component_mask(&self) -> u64 {
        crate::ecs::components::STYLE
            | crate::ecs::components::LAYOUT
            | crate::ecs::components::INTERACTION
            | crate::ecs::components::TEXT
            | crate::ecs::components::CURSOR
            | crate::ecs::components::SCROLL
            | crate::ecs::components::LIFECYCLE
    }

    fn mount_box(mut self: Box<Self>, ctx: &mut MountContext<'_>) -> ElementId {
        let disabled = self.disabled || self.read_only;
        let initial_valid = true; // validated after editor creation
        let role = ComponentRole::Interactive(InteractiveRole::TextInput {
            variant: InputVariant::Filled,
            size: crate::theme::ControlSize::Medium,
            disabled: self.disabled,
            readonly: self.read_only,
            is_valid: initial_valid,
        });
        let resolved = ctx.theme.resolve_component(&role);
        let input_style = match &resolved {
            ResolvedComponentStyle::TextInput(s) => s,
            _ => unreachable!(),
        };
        let fs = input_style.font_size;
        let fw = input_style.font_weight;
        let lh = self.style.line_height.unwrap_or(1.5);
        let is_multi = self.input_type == TextInputType::Multiline;
        let max_len = self.max_length;
        let pad = self.style.padding.unwrap_or(input_style.padding);
        let pad_left = pad.left;
        let pad_top = pad.top;
        let default_h = if is_multi {
            120.0
        } else {
            (fs * lh * 1.8).max(36.0)
        };
        let el_w = 200.0;

        // Shared state for callbacks, validation, and error display
        let cb_on_change: Rc<RefCell<Option<Box<dyn Fn(String)>>>> = Rc::new(RefCell::new(
            self.on_value_changed
                .take()
                .map(|f| f as Box<dyn Fn(String)>),
        ));
        let cb_on_submit: Rc<RefCell<Option<Box<dyn Fn(String)>>>> = Rc::new(RefCell::new(
            self.on_submit.take().map(|f| f as Box<dyn Fn(String)>),
        ));
        let cb_validator: Rc<RefCell<Option<Box<dyn Fn(&str) -> bool>>>> = Rc::new(RefCell::new(
            self.validation
                .take()
                .map(|f| f as Box<dyn Fn(&str) -> bool>),
        ));
        let error_text: Rc<RefCell<Option<String>>> =
            Rc::new(RefCell::new(self.error_message.clone()));

        // Build editor config
        let editor_config = TextInputConfig {
            input_type: to_editor_type(self.input_type),
            max_length: max_len,
            read_only: self.read_only,
            disabled,
            placeholder: self.placeholder.clone(),
            font_size: fs,
            font_weight: fw,
            line_height: lh,
            font_family: None,
            blink_period_ms: 500,
            pause_duration_ms: 300,
            on_change: cb_on_change,
            on_submit: cb_on_submit,
            validator: cb_validator,
            error_text: error_text.clone(),
            is_valid: Rc::new(Cell::new(true)),
        };

        // Create EditorState
        let editor = EditorState::new(self.value.clone(), editor_config);

        // Run initial validation
        {
            let state = editor.borrow();
            let t = state.text_rope.to_string();
            state.config.validate(&t);
        }

        // Initial display text for buffer creation
        let display_init = self.value.read();

        // Create shared text buffer (Element renderer + frame_tick + event handlers)
        let text_buf = Rc::new(RefCell::new(
            crate::render::wgpu::glyphon_bridge::create_buffer(
                &display_init,
                fs,
                lh,
                fw,
                None,
                Some((el_w - pad_left * 2.0).max(fs * 2.0)),
                crate::style::TextAlign::Center,
            ),
        ));

        // Layout & Paint
        let layout = LayoutConfig {
            width: self.style.width.unwrap_or(Dimension::Pixels(el_w)),
            height: self.style.height.unwrap_or(Dimension::Pixels(default_h)),
            padding: pad,
            ..LayoutConfig::default()
        };
        let paint = PaintConfig {
            background: Some(input_style.background),
            foreground: Some(input_style.foreground),
            border_width: self.style.border_width.unwrap_or(1.0),
            border_color: Some(input_style.border_color),
            corner_radius: input_style.corner_radius,
            font_size: fs,
            font_weight: fw,
            line_height: lh,
            text_align: if is_multi {
                crate::style::TextAlign::Start
            } else {
                crate::style::TextAlign::Center
            },
            placeholder_color: self
                .style
                .placeholder_color
                .or(Some(input_style.placeholder_color)),
            ..PaintConfig::default()
        };
        let deferred_id: Rc<Cell<Option<ElementId>>> = Rc::new(Cell::new(None));

        let mut events = EventHandler::new();

        if !disabled {
            let eid = deferred_id.clone();
            let sn = self.suppress_nav.clone();
            let ml = is_multi;
            let is_password = self.input_type == TextInputType::Password;

            // Action handler
            events = events.on_action({
                let ed = editor.clone();
                let buf = text_buf.clone();
                let sn_flag = sn;
                let eid_clone = eid.clone();
                let ml = ml;
                move |action: &Action| {
                    let kind = action.kind;
                    if !ml && (kind == ActionKind::MoveDown || kind == ActionKind::MoveUp) {
                        return ActionOutcome::Unhandled;
                    }
                    if let Some(ref flag) = sn_flag {
                        if flag.get()
                            && (kind == ActionKind::MoveDown
                                || kind == ActionKind::MoveUp
                                || kind == ActionKind::MoveLeft
                                || kind == ActionKind::MoveRight
                                || kind == ActionKind::MoveHome
                                || kind == ActionKind::MoveEnd
                                || kind == ActionKind::MovePageDown
                                || kind == ActionKind::MovePageUp)
                        {
                            return ActionOutcome::Unhandled;
                        }
                    }
                    let mut state = ed.borrow_mut();
                    let mut b = buf.borrow_mut();
                    let result = handle_action(&mut state, action, &mut b);
                    if let ActionOutcome::Consumed = result {
                        state.cursor_blink_last_input.set(clock::now());
                        if let Some(id) = eid_clone.get() {
                            dirty_registry::mark_widget_repaint(id);
                        }
                    }
                    result
                }
            });

            // Text input (characters)
            events = events.on_text_input({
                let ed = editor.clone();
                move |c: char| {
                    let mut state = ed.borrow_mut();
                    if state.config.read_only {
                        return;
                    }
                    state.insert_char(c);
                    state.cursor_blink_last_input.set(clock::now());
                }
            });

            // IME Preedit — disabled for password inputs
            if !is_password {
                events = events.on_preedit({
                    let ed = editor.clone();
                    move |text: String, cursor_range: Option<(usize, usize)>| {
                        let mut state = ed.borrow_mut();
                        if text.is_empty() {
                            state.commit_composition();
                        } else {
                            state.set_composition(text, cursor_range);
                        }
                    }
                });
                // IME DeleteSurrounding (audit 2026-07-17 round 5, C5):
                // byte-count deletion around the cursor, pre-edit untouched.
                events = events.on_ime_delete_surrounding({
                    let ed = editor.clone();
                    move |before_bytes: usize, after_bytes: usize| {
                        let mut state = ed.borrow_mut();
                        if state.config.read_only {
                            return;
                        }
                        state.delete_surrounding_bytes(before_bytes, after_bytes);
                    }
                });
                // IME Commit — the whole committed string lands as ONE edit:
                // one undo entry, one signal set, one dirty pass (splice P0).
                events = events.on_ime_commit({
                    let ed = editor.clone();
                    move |text: String| {
                        let mut state = ed.borrow_mut();
                        if state.config.read_only {
                            return;
                        }
                        state.clear_composition();
                        state.push_boundary();
                        state.insert_text(&text);
                        state.cursor_blink_last_input.set(clock::now());
                    }
                });
            }

            // Clipboard copy
            events = events.on_clipboard_copy({
                let ed = editor.clone();
                move || -> String { ed.borrow().selected_text() }
            });

            // Clipboard paste
            events = events.on_clipboard_paste({
                let ed = editor.clone();
                move |text: String| {
                    let mut state = ed.borrow_mut();
                    if state.config.read_only {
                        return;
                    }
                    state.insert_text(&text);
                }
            });

            // Click (position cursor)
            events = events.on_click_at({
                let ed = editor.clone();
                let buf = text_buf.clone();
                move |pos: Point| {
                    let mut state = ed.borrow_mut();
                    let b = buf.borrow();
                    let scroll_x = state.scroll_offset.get().x;
                    state.click_at(pos, &b, (pad_left, pad_top), is_multi, scroll_x);
                    state.cursor_blink_last_input.set(clock::now());
                }
            });

            // Drag start
            events = events.on_drag_start({
                let ed = editor.clone();
                let buf = text_buf.clone();
                let eid_clone = eid.clone();
                move |pos: Point, _abs: Point| {
                    let mut state = ed.borrow_mut();
                    let b = buf.borrow();
                    let scroll_x = state.scroll_offset.get().x;
                    state.click_at(pos, &b, (pad_left, pad_top), is_multi, scroll_x);
                    state.cursor_blink_last_input.set(clock::now());
                    if let Some(id) = eid_clone.get() {
                        dirty_registry::mark_widget_repaint(id);
                    }
                }
            });

            // Drag (extend selection)
            events = events.on_drag_update({
                let ed = editor.clone();
                let buf = text_buf.clone();
                let eid_clone = eid.clone();
                move |pos: Point, _abs: Point| {
                    let mut state = ed.borrow_mut();
                    let b = buf.borrow();
                    let scroll_x = state.scroll_offset.get().x;
                    state.extend_selection_to(pos, &b, (pad_left, pad_top), is_multi, scroll_x);
                    state.cursor_blink_last_input.set(clock::now());
                    if let Some(id) = eid_clone.get() {
                        dirty_registry::mark_widget_repaint(id);
                    }
                }
            });

            // Scroll — for multiline, consume scroll events
            if is_multi {
                events = events.on_scroll({
                    let ed = editor.clone();
                    let seid = eid.clone();
                    move |dx: f32, dy: f32| -> bool {
                        let state = ed.borrow_mut();
                        let mut o = state.scroll_offset.get();
                        let max = state.scroll_max.get();
                        let old_x = o.x;
                        let old_y = o.y;
                        o.x = (o.x - dx).clamp(0.0, max.x);
                        o.y = (o.y - dy).clamp(0.0, max.y);
                        if o.x != old_x || o.y != old_y {
                            state.scroll_offset.set(o);
                            state.text_scroll_x.set(o.x);
                            state.text_scroll_y.set(o.y);
                            if let Some(sid) = seid.get() {
                                dirty_registry::spatial_update_scroll(sid, o.x, o.y);
                                dirty_registry::bump_subtree_gen(sid);
                                dirty_registry::register_dirty(sid, DirtyFlags::REPAINT);
                            }
                        }
                        true
                    }
                });
            }

            // Double click (select word)
            events = events.on_double_click({
                let ed = editor.clone();
                let buf = text_buf.clone();
                move || {
                    let mut state = ed.borrow_mut();
                    let b = buf.borrow();
                    state.select_word_at(Point::ZERO, &b, (pad_left, pad_top), 0.0);
                    state.cursor_blink_last_input.set(clock::now());
                }
            });

            // Triple click (select line)
            events = events.on_triple_click({
                let ed = editor.clone();
                let buf = text_buf.clone();
                move || {
                    let mut state = ed.borrow_mut();
                    let b = buf.borrow();
                    state.select_line_at(Point::ZERO, &b, (pad_left, pad_top), 0.0);
                    state.cursor_blink_last_input.set(clock::now());
                }
            });

            // Focus in
            events = events.on_focus_in({
                let ed = editor.clone();
                let eid_clone = eid.clone();
                move |_reason: FocusReason| {
                    let mut state = ed.borrow_mut();
                    state.cursor_focused.set(true);
                    state.cursor_visible.set(true);
                    state.cursor_blink_last_input.set(clock::now());
                    state.clear_composition();
                    state.push_boundary();
                    if let Some(id) = eid_clone.get() {
                        dirty_registry::mark_widget_repaint(id);
                        crate::ecs::active::register_active(
                            id,
                            crate::ecs::active::ActiveTag::CursorBlink,
                        );
                    }
                }
            });

            // Focus out
            events = events.on_focus_out({
                let ed = editor.clone();
                move |_reason: FocusReason| {
                    let mut state = ed.borrow_mut();
                    state.cursor_focused.set(false);
                    state.cursor_visible.set(false);
                    state.finalize_composition();
                    state.push_boundary();
                }
            });
        }

        let interaction = InteractionConfig {
            events: Some(events),
            enabled: !disabled,
            focusable: !disabled,
            cursor: crate::platform::CursorIcon::TEXT,
            autofocus: self.autofocus,
            ..InteractionConfig::default()
        };

        let id = ElementBuilder::new()
            .with_components(self.component_mask())
            .layout(layout)
            .interaction(interaction)
            .paint(paint)
            .accessibility(
                accesskit::Role::TextInput,
                if display_init.is_empty() {
                    self.placeholder.clone()
                } else {
                    display_init.clone()
                },
            )
            .build(ctx);

        // Set deferred id so handlers can use it for dirty_registry calls
        deferred_id.set(Some(id));

        // Multiline: share ECS ScrollComponent cells for unified scrollbar support
        if is_multi {
            {
                let ct = ctx.arena.component_tables.borrow();
                if let Some(sc) = ct.scroll.get(&id) {
                    let mut state = editor.borrow_mut();
                    state.scroll_offset = sc.scroll_offset.clone();
                    state.content_bounds = sc.content_bounds.clone();
                    state.max_scroll_y = sc.max_scroll_y.clone();
                }
            }
            let Some(element) = ctx.arena.get_mut(id) else {
                return id;
            };
            element.set_overflow(Overflow::Scroll);
            // Seed spatial index so hit-testing the scrollbar thumb works
            // before the first scroll event.
            dirty_registry::spatial_update_scroll(id, 0.0, 0.0);
        }

        // Fix up font_family from the mounted element
        {
            if let Some(el) = ctx.arena.get(id) {
                if let Some(ff) = el.font_family() {
                    editor.borrow_mut().config.font_family = Some(ff.to_string());
                }
            }
        }

        // Bind Element fields to EditorState render cache
        {
            let Some(element) = ctx.arena.get_mut(id) else {
                return id;
            };
            element.set_text_buffer(text_buf.clone());

            let state_ref = editor.borrow();
            element.set_cursor_x(state_ref.cursor_pixel_x.clone());
            element.set_cursor_visible(state_ref.cursor_visible.clone());
            element.set_cursor_line(state_ref.cursor_pixel_row.clone());
            element.set_cursor_focused(state_ref.cursor_focused.clone());
            element.set_cursor_blink_last_input(state_ref.cursor_blink_last_input.clone());
            element.set_selection_rect(state_ref.selection_rects.clone());
            element.set_ime_cursor_rect(state_ref.ime_cursor_rect.clone());
            element.set_composition_underline_rect(state_ref.composition_underline_rect.clone());
            element.set_text_scroll_x(state_ref.text_scroll_x.clone());
            element.set_text_scroll_y(state_ref.text_scroll_y.clone());

            if disabled {
                element
                    .state
                    .set(element.state.get() | crate::core::config::StateFlags::DISABLED);
                element.mark_repaint();
            }

            element.set_preferred_height(default_h);
            let min_w = (fs * 3.0).max(60.0);
            element.set_min_main(min_w);
            if is_multi {
                element.set_text_vertical_center(false);
            }
        }

        let text_align = ctx.arena.get(id).unwrap().text_align();

        // Frame tick
        {
            let editor_tick = editor.clone();
            let text_buf_tick = text_buf.clone();
            let text_gen_tick = ctx.arena.get(id).unwrap().text_generation().clone();
            // Render-key change guard (audit 2026-07-15, C1a): the tick used
            // to re-shape the buffer and bump text_generation EVERY frame,
            // costing a full cosmic-text shaping per input per frame and
            // permanently defeating the subtree cache. Recompute only when
            // any render input actually changed.
            let prev_render_key: std::cell::Cell<u64> = std::cell::Cell::new(0);

            let Some(element) = ctx.arena.get_mut(id) else {
                return id;
            };
            element.set_frame_tick(Box::new(move || {
                let mut state = editor_tick.borrow_mut();

                // External signal sync
                state.sync_from_signal();

                let render_key = {
                    use std::hash::{Hash, Hasher};
                    let mut h = rustc_hash::FxHasher::default();
                    let text = state.text_rope.to_string();
                    text.hash(&mut h);
                    state.cursor.hash(&mut h);
                    state.selection_anchor.hash(&mut h);
                    state.has_selection.hash(&mut h);
                    if let Some(ref comp) = state.composition {
                        comp.range.start.hash(&mut h);
                        comp.text.hash(&mut h);
                        // Caret can move INSIDE the preedit without the text
                        // changing — include the IME cursor range.
                        comp.cursor_range.hash(&mut h);
                    }
                    let sb = crate::core::dirty_registry::bounds_of(id)
                        .unwrap_or(crate::style::Rect::ZERO);
                    sb.width.to_bits().hash(&mut h);
                    sb.height.to_bits().hash(&mut h);
                    let so = state.scroll_offset.get();
                    so.x.to_bits().hash(&mut h);
                    so.y.to_bits().hash(&mut h);
                    h.finish().max(1) // 0 is the "never computed" sentinel
                };
                if prev_render_key.get() == render_key {
                    return;
                }
                prev_render_key.set(render_key);

                let display = display_text(&state);
                let buffer = build_buffer(&display, &state.config, el_w, pad_left, text_align);

                // Update shared buffer for renderer
                *text_buf_tick.borrow_mut() = buffer;
                // Bump text generation so scene cache knows to re-record.
                // Deferred: generation bumps must not happen during prepass.
                if let Some(ref tg) = text_gen_tick {
                    let tg_rc = tg.clone();
                    crate::core::dirty_registry::defer_action(move |_arena, _, _| {
                        tg_rc.set(tg_rc.get().wrapping_add(1));
                    });
                }

                // Re-borrow for pixel calculations
                let buf = text_buf_tick.borrow();

                // Cursor pixel — measured against the DISPLAY text (the text
                // actually shaped into `buf`), not the raw text_cache. In
                // password mode the mask bullet '•' is 3 UTF-8 bytes while
                // raw chars are typically 1 byte; mapping raw byte offsets
                // into the masked buffer put the caret at ~1/3 of the row.
                //
                // During IME composition the preedit is spliced into display
                // at `state.cursor`; the caret sits INSIDE the preedit at the
                // IME-reported cursor (byte offset into the preedit), not at
                // the splice point (audit follow-up #4).
                let caret_ci = if let Some(ref comp) = state.composition {
                    let inner = match comp.cursor_range {
                        Some((s, _)) => comp
                            .text
                            .get(..s.min(comp.text.len()))
                            .map_or(0, |prefix| prefix.chars().count()),
                        None => comp.text.chars().count(),
                    };
                    comp.range.start + inner
                } else {
                    state.cursor
                };
                let (row, x_pos) = cursor_pixel_pos(&buf, &display, caret_ci);
                state.cursor_pixel_row.set(row);
                state.cursor_pixel_x.set(x_pos);

                // Selection rects — same display-text mapping as the caret.
                if let Some((lo, hi)) = state.selection_range() {
                    let rects = selection_rects(&buf, &display, lo, hi, fs * lh, pad_left, pad_top);
                    state.selection_rects.set(rects);
                } else {
                    state.selection_rects.set(Vec::new());
                }

                // Composition underline — multi-rect for soft-wrap (P2).
                if let Some(ref comp) = state.composition {
                    let comp_range = (
                        comp.range.start,
                        comp.range.start + comp.text.chars().count(),
                    );
                    let clause_display = Composition {
                        text: comp.text.clone(),
                        anchor: comp.range.start,
                        caret_bytes: comp.cursor_range,
                    }
                    .clause_in_preedit()
                    .map(|r| (r.start, r.end));
                    let rects = composition_rects(
                        &buf,
                        &display,
                        comp_range,
                        clause_display,
                        fs * lh,
                        pad_left,
                        pad_top,
                    );
                    state.composition_underline_rect.set(rects.first().copied());
                } else {
                    state.composition_underline_rect.set(None);
                }

                // IME cursor rect
                state.ime_cursor_rect.set(Some(Rect::new(
                    pad_left + x_pos,
                    pad_top + row as f32 * fs * lh,
                    2.0,
                    fs * lh,
                )));

                // Auto scroll — use actual element bounds, not the nominal default_h
                let sb =
                    crate::core::dirty_registry::bounds_of(id).unwrap_or(crate::style::Rect::ZERO);
                let vis_w = (sb.width - pad_left * 2.0).max(10.0);
                let vis_h = sb.height.max(fs * lh);
                let total_rows = crate::render::text::visual_row_count(&buf) as f32;
                let text = state.text_rope.to_string();
                let text_end_x = crate::render::text::glyph_pos_at_ci(
                    &buf,
                    crate::render::text::raw_to_expanded(&text, text.chars().count()),
                    false,
                )
                .1;
                let max_scroll_x = (text_end_x - vis_w).max(0.0);
                let max_scroll_y = (total_rows * fs * lh - vis_h).max(0.0);
                state.scroll_max.set(Vec2::new(max_scroll_x, max_scroll_y));
                state.max_scroll_y.set(max_scroll_y);

                // Write content_bounds for scrollbar rendering (leaf widget, no children)
                if is_multi {
                    let content_h = pad_top + total_rows * fs * lh + pad_top;
                    state.content_bounds.set(Rect::new(
                        0.0,
                        0.0,
                        sb.width,
                        content_h.max(sb.height),
                    ));
                }

                // Only auto-scroll when the cursor position changed (typing,
                // arrow keys, clicks). Manual scrolling (mouse wheel, scrollbar
                // drag) should not be overridden.
                let cursor_moved = state.cursor != state.prev_cursor.get();
                state.prev_cursor.set(state.cursor);
                if cursor_moved {
                    auto_scroll(
                        x_pos,
                        row,
                        fs * lh,
                        vis_w,
                        vis_h,
                        &state.scroll_offset,
                        &state.scroll_max,
                    );
                }
                // Sync scroll offset to Element fields for renderer
                let v = state.scroll_offset.get();
                state.text_scroll_x.set(v.x);
                state.text_scroll_y.set(v.y);
                // Keep spatial index in sync so hit-testing works after
                // auto-scroll (multiline). Without this, clicks on a
                // multiline TextInput may miss after focus-out/in.
                if is_multi {
                    dirty_registry::spatial_update_scroll(id, v.x, v.y);
                }

                drop(buf);

                // Cursor blink is driven by process_cursor_blink via discrete
                // scheduler deadlines (every 500ms) — no need to perpetually
                // re-mark dirty or force continuous mode.
            }));
        }

        ctx.register_theme_component(id, &resolved, &role, &self.style);

        // Signal subscription
        {
            let sub_dir = ctx.arena.get(id).unwrap().dirty.clone();
            let sub_eid = id;
            crate::core::signal_bridge::subscribe_owned(id, &self.value, move || {
                sub_dir.set(sub_dir.get() | DirtyFlags::REPAINT);
                dirty_registry::register_dirty(sub_eid, DirtyFlags::REPAINT);
                dirty_registry::bump_subtree_gen(sub_eid);
            });
        }

        // IME suppressed for password inputs
        if !disabled {
            let is_password = self.input_type == TextInputType::Password;
            if is_password {
                if let Some(reg) = ctx.event_registry.as_mut() {
                    reg.set_ime_suppressed(id, true);
                }
            }
        }

        id
    }
}

impl std::fmt::Debug for TextInput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TextInput")
            .field("input_type", &self.input_type)
            .field("disabled", &self.disabled)
            .finish_non_exhaustive()
    }
}
