use std::cell::{Cell, RefCell};
use std::ops::Range;
use std::rc::Rc;

use auralis_signal::Signal;
use ropey::Rope;
use web_time::Instant;

use crate::core::clock;
use crate::style::{Rect, Vec2};

// ── Config ─────────────────────────────────────────

#[derive(Clone)]
pub struct TextInputConfig {
    pub input_type: TextInputType,
    pub max_length: Option<usize>,
    pub read_only: bool,
    pub disabled: bool,
    pub placeholder: String,
    pub font_size: f32,
    pub font_weight: u16,
    pub line_height: f32,
    pub font_family: Option<String>,
    pub blink_period_ms: u64,
    pub pause_duration_ms: u64,
    pub on_change: Rc<RefCell<Option<Box<dyn Fn(String)>>>>,
    pub on_submit: Rc<RefCell<Option<Box<dyn Fn(String)>>>>,
    pub validator: Rc<RefCell<Option<Box<dyn Fn(&str) -> bool>>>>,
    pub error_text: Rc<RefCell<Option<String>>>,
    pub is_valid: Rc<Cell<bool>>,
}

impl Default for TextInputConfig {
    fn default() -> Self {
        Self {
            input_type: TextInputType::Text,
            max_length: None,
            read_only: false,
            disabled: false,
            placeholder: String::new(),
            font_size: 14.0,
            font_weight: 400,
            line_height: 1.5,
            font_family: None,
            blink_period_ms: 500,
            pause_duration_ms: 300,
            on_change: Rc::new(RefCell::new(None)),
            on_submit: Rc::new(RefCell::new(None)),
            validator: Rc::new(RefCell::new(None)),
            error_text: Rc::new(RefCell::new(None)),
            is_valid: Rc::new(Cell::new(true)),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TextInputType {
    Text,
    Password,
    Number,
    Email,
    Url,
    Multiline,
}

impl TextInputConfig {
    pub fn is_password(&self) -> bool {
        self.input_type == TextInputType::Password
    }
    pub fn is_multiline(&self) -> bool {
        self.input_type == TextInputType::Multiline
    }
    pub fn is_number(&self) -> bool {
        self.input_type == TextInputType::Number
    }
    pub fn is_email(&self) -> bool {
        self.input_type == TextInputType::Email
    }
    pub fn is_url(&self) -> bool {
        self.input_type == TextInputType::Url
    }

    pub fn fire_change(&self, text: &str) {
        if let Some(ref cb) = *self.on_change.borrow() {
            cb(text.to_string());
        }
    }

    pub fn fire_submit(&self) {
        // Submit is now handled by the action handler.
    }

    pub fn validate(&self, text: &str) {
        let valid = if let Some(ref v) = *self.validator.borrow() {
            v(text)
        } else {
            match self.input_type {
                TextInputType::Email => text.is_empty() || text.contains('@'),
                TextInputType::Url => {
                    text.is_empty() || text.starts_with("http://") || text.starts_with("https://")
                }
                _ => true,
            }
        };
        self.is_valid.set(valid);
    }
}

// ── CompositionState ───────────────────────────────

#[derive(Clone, Debug)]
pub struct CompositionState {
    pub text: String,
    pub range: Range<usize>,
    pub cursor_range: Option<(usize, usize)>,
}

// ── EditorSnapshot (for Undo) ──────────────────────
// Rope::clone() is O(1) reference-count bump — the undo stack no
// longer pays per-step O(N) string copies (audit 2026-07-16 L1L2).

#[derive(Clone)]
pub struct EditorSnapshot {
    pub text: Rope,
    pub cursor: usize,
    pub selection_anchor: usize,
    pub has_selection: bool,
}

// ── UndoStack ──────────────────────────────────────

pub struct UndoStack<T> {
    entries: Vec<T>,
    index: usize,
    capacity: usize,
    last_push: Option<Instant>,
    boundary_pending: bool,
    merge_window_ms: u64,
}

impl<T: Clone> UndoStack<T> {
    pub fn new(capacity: usize, merge_window_ms: u64) -> Self {
        Self {
            entries: Vec::new(),
            index: 0,
            capacity,
            last_push: None,
            boundary_pending: false,
            merge_window_ms,
        }
    }

    pub fn seed(&mut self, value: T) {
        self.entries.push(value);
        self.index = 0;
    }

    pub fn push(&mut self, value: T) {
        let should_merge = !self.boundary_pending
            && self
                .last_push
                .is_some_and(|t| (clock::now() - t).as_millis() < self.merge_window_ms as u128);

        if should_merge && !self.entries.is_empty() {
            if let Some(last) = self.entries.last_mut() {
                *last = value;
            }
        } else {
            self.entries.truncate(self.index + 1);
            self.entries.push(value);
            self.index += 1;
            while self.entries.len() > self.capacity + 1 {
                self.entries.remove(0);
                self.index = self.index.saturating_sub(1);
            }
        }

        self.last_push = Some(clock::now());
        self.boundary_pending = false;
    }

    pub fn undo(&mut self) -> Option<T> {
        if self.index > 0 {
            self.index -= 1;
            self.last_push = None;
            Some(self.entries[self.index].clone())
        } else {
            None
        }
    }

    pub fn redo(&mut self) -> Option<T> {
        if self.index + 1 < self.entries.len() {
            self.index += 1;
            self.last_push = None;
            Some(self.entries[self.index].clone())
        } else {
            None
        }
    }

    pub fn push_boundary(&mut self) {
        self.boundary_pending = true;
    }
}

// ── EditorState ────────────────────────────────────

pub struct EditorState {
    // SSOT — ropey::Rope replaces String for internal edits.
    // Rope::clone() is O(1); insert/remove are O(log N).
    text_signal: Signal<String>,
    pub(crate) text_rope: Rope,

    // Cursor & Selection (char indices)
    pub(crate) cursor: usize,
    pub(crate) selection_anchor: usize,
    pub(crate) has_selection: bool,
    pub(crate) preferred_x: f32,

    // IME
    pub(crate) composition: Option<CompositionState>,

    // Undo — EditorSnapshot.text is Rope, O(1) push.
    pub(crate) undo_stack: UndoStack<EditorSnapshot>,

    // Generation counter bumped on every text mutation.  Pairs with
    // `cached_display` to avoid O(N) to_string() on frames where the
    // text hasn't changed since the last query.
    text_version: Rc<Cell<u64>>,
    cached_display: RefCell<String>,
    cached_display_version: Cell<u64>,

    // Render cache (shared with Element via Rc<Cell>)
    pub cursor_pixel_x: Rc<Cell<f32>>,
    pub cursor_pixel_row: Rc<Cell<usize>>,
    pub cursor_visible: Rc<Cell<bool>>,
    pub cursor_focused: Rc<Cell<bool>>,
    pub cursor_blink_last_input: Rc<Cell<web_time::Instant>>,
    pub selection_rects: Rc<Cell<Vec<Rect>>>,
    pub composition_underline_rect: Rc<Cell<Option<Rect>>>,
    pub ime_cursor_rect: Rc<Cell<Option<Rect>>>,
    pub scroll_offset: Rc<Cell<Vec2>>,
    pub scroll_max: Rc<Cell<Vec2>>,
    pub content_bounds: Rc<Cell<Rect>>,
    pub max_scroll_y: Rc<Cell<f32>>,
    pub prev_cursor: Rc<Cell<usize>>,
    pub text_scroll_x: Rc<Cell<f32>>,
    pub text_scroll_y: Rc<Cell<f32>>,
    pub text_buffer: Rc<RefCell<Option<cosmic_text::Buffer>>>,

    // Config
    pub config: TextInputConfig,
}

impl EditorState {
    pub fn new(signal: Signal<String>, config: TextInputConfig) -> Rc<RefCell<Self>> {
        let text = signal.read();
        let text_rope = Rope::from_str(&text);
        let display_cache = text_rope.to_string();
        let len = text_rope.len_chars();

        let mut undo_stack = UndoStack::new(100, 400);
        undo_stack.seed(EditorSnapshot {
            text: text_rope.clone(),
            cursor: len,
            selection_anchor: len,
            has_selection: false,
        });

        let state = Self {
            text_signal: signal,
            text_rope,
            cursor: len,
            selection_anchor: len,
            has_selection: false,
            preferred_x: -1.0,
            composition: None,
            undo_stack,
            cursor_pixel_x: Rc::new(Cell::new(0.0)),
            cursor_pixel_row: Rc::new(Cell::new(0)),
            cursor_visible: Rc::new(Cell::new(false)),
            cursor_focused: Rc::new(Cell::new(false)),
            cursor_blink_last_input: Rc::new(Cell::new(clock::now())),
            selection_rects: Rc::new(Cell::new(Vec::new())),
            composition_underline_rect: Rc::new(Cell::new(None)),
            ime_cursor_rect: Rc::new(Cell::new(None)),
            scroll_offset: Rc::new(Cell::new(Vec2::ZERO)),
            scroll_max: Rc::new(Cell::new(Vec2::ZERO)),
            content_bounds: Rc::new(Cell::new(Rect::ZERO)),
            max_scroll_y: Rc::new(Cell::new(0.0)),
            prev_cursor: Rc::new(Cell::new(len)),
            text_scroll_x: Rc::new(Cell::new(0.0)),
            text_scroll_y: Rc::new(Cell::new(0.0)),
            text_buffer: Rc::new(RefCell::new(None)),
            text_version: Rc::new(Cell::new(1)),
            cached_display: RefCell::new(display_cache),
            cached_display_version: Cell::new(1),
            config,
        };

        Rc::new(RefCell::new(state))
    }

    // ── String accessors ──
    /// The full document text as a `String` — O(N).  Callers that only
    /// need a `&str` view on a portion of the document should prefer
    /// `text_slice()` or `text_for_display()` (render module).
    #[allow(dead_code)]
    pub(crate) fn text(&self) -> String {
        self.text_rope.to_string()
    }

    /// Cached `&str` equivalent: returns the rope as a `String`.
    /// Uses version-guarded caching — O(N) only when the text changed
    /// since the last call.
    pub(crate) fn text_for_display(&self) -> String {
        let v = self.text_version.get();
        if self.cached_display_version.get() == v {
            return self.cached_display.borrow().clone();
        }
        let s = self.text_rope.to_string();
        self.cached_display.replace(s.clone());
        self.cached_display_version.set(v);
        s
    }

    /// Bump the text version — call on every text mutation.
    pub(crate) fn bump_version(&self) {
        self.text_version
            .set(self.text_version.get().wrapping_add(1));
    }

    /// Number of chars in the document.
    #[allow(dead_code)]
    pub(crate) fn len(&self) -> usize {
        self.text_rope.len_chars()
    }

    // ── Signal access ──
    pub fn text_signal(&self) -> &Signal<String> {
        &self.text_signal
    }

    // ── External sync ──
    pub fn sync_from_signal(&mut self) {
        let external = self.text_signal.read();
        let ext_len = external.chars().count();
        if self.text_rope.len_chars() != ext_len || self.text_rope != external {
            self.text_rope = Rope::from_str(&external);
            self.cursor = ext_len;
            self.selection_anchor = self.cursor;
            self.has_selection = false;
            self.bump_version();
            self.push_snapshot();
            self.notify_change();
        }
    }

    // ── Snapshot management ──
    fn current_snapshot(&self) -> EditorSnapshot {
        EditorSnapshot {
            text: self.text_rope.clone(),
            cursor: self.cursor,
            selection_anchor: self.selection_anchor,
            has_selection: self.has_selection,
        }
    }

    pub(crate) fn push_snapshot(&mut self) {
        self.undo_stack.push(self.current_snapshot());
    }

    /// Fire on_change callback and run validator after a text mutation.
    fn notify_change(&self) {
        let s = self.text_rope.to_string();
        self.config.fire_change(&s);
        self.config.validate(&s);
    }

    fn restore_snapshot(&mut self, snapshot: &EditorSnapshot) {
        self.text_rope = snapshot.text.clone();
        self.bump_version();
        self.text_signal.set(snapshot.text.to_string());
        self.cursor = snapshot.cursor;
        self.selection_anchor = snapshot.selection_anchor;
        self.has_selection = snapshot.has_selection;
        let s = snapshot.text.to_string();
        self.config.fire_change(&s);
        self.config.validate(&s);
    }

    pub fn undo(&mut self) -> bool {
        if let Some(snapshot) = self.undo_stack.undo() {
            self.restore_snapshot(&snapshot);
            true
        } else {
            false
        }
    }

    pub fn redo(&mut self) -> bool {
        if let Some(snapshot) = self.undo_stack.redo() {
            self.restore_snapshot(&snapshot);
            true
        } else {
            false
        }
    }

    pub fn push_boundary(&mut self) {
        self.undo_stack.push_boundary();
    }

    // ── Selection helpers ──
    pub fn selection_range(&self) -> Option<(usize, usize)> {
        if self.has_selection && self.cursor != self.selection_anchor {
            let lo = self.cursor.min(self.selection_anchor);
            let hi = self.cursor.max(self.selection_anchor);
            Some((lo, hi))
        } else {
            None
        }
    }

    fn selected_len(&self) -> usize {
        self.selection_range().map_or(0, |(lo, hi)| hi - lo)
    }

    fn delete_selection(&mut self) {
        if let Some((lo, hi)) = self.selection_range() {
            self.text_rope.remove(lo..hi);
            self.cursor = lo;
            self.selection_anchor = lo;
            self.has_selection = false;
        }
    }

    // ── Insert / Delete ──
    fn can_insert(&self, len: usize) -> bool {
        match self.config.max_length {
            Some(max) => {
                let current_len = self.text_rope.len_chars();
                let selected = self.selected_len();
                current_len - selected + len <= max
            }
            None => true,
        }
    }

    /// Insert a single char at the cursor. O(log N).
    pub fn insert_char(&mut self, c: char) -> bool {
        if !self.can_insert(1) {
            return false;
        }
        if self.config.is_number() && !c.is_ascii_digit() && c != '.' && c != '-' {
            return false;
        }
        self.delete_selection();
        self.text_rope.insert(self.cursor, &c.to_string());
        self.cursor += 1;
        self.selection_anchor = self.cursor;
        self.has_selection = false;
        self.bump_version();
        self.text_signal.set(self.text_rope.to_string());
        self.push_snapshot();
        self.notify_change();
        true
    }

    /// Insert text at the cursor. O(log N + |text|).
    pub fn insert_text(&mut self, text: &str) -> bool {
        if !self.can_insert(text.chars().count()) {
            return false;
        }
        let sanitized = if !self.config.is_multiline() {
            text.replace('\n', " ").replace('\r', "")
        } else {
            text.to_string()
        };
        self.delete_selection();
        self.text_rope.insert(self.cursor, &sanitized);
        self.cursor += sanitized.chars().count();
        self.selection_anchor = self.cursor;
        self.has_selection = false;
        self.bump_version();
        self.text_signal.set(self.text_rope.to_string());
        self.push_snapshot();
        self.notify_change();
        true
    }

    /// Delete one char before the cursor (or current selection).
    pub fn delete_backward(&mut self) {
        if self.has_selection {
            self.delete_selection();
        } else if self.cursor > 0 {
            self.text_rope.remove(self.cursor - 1..self.cursor);
            self.cursor -= 1;
            self.selection_anchor = self.cursor;
        }
        self.bump_version();
        self.text_signal.set(self.text_rope.to_string());
        self.push_snapshot();
        self.notify_change();
    }

    /// Delete one char after the cursor (or current selection).
    pub fn delete_forward(&mut self) {
        if self.has_selection {
            self.delete_selection();
        } else if self.cursor < self.text_rope.len_chars() {
            self.text_rope.remove(self.cursor..self.cursor + 1);
        }
        self.bump_version();
        self.text_signal.set(self.text_rope.to_string());
        self.push_snapshot();
        self.notify_change();
    }

    /// IME `DeleteSurrounding` (audit 2026-07-17 round 5, C5):
    /// Deletions snap outward to char boundaries.  Works directly on
    /// the rope in O(log N + k) where k is the deleted span.
    pub fn delete_surrounding_bytes(&mut self, before_bytes: usize, after_bytes: usize) {
        if before_bytes == 0 && after_bytes == 0 {
            return;
        }

        // Count back chars until we've covered >= before_bytes.
        let mut back_chars = 0usize;
        let mut consumed = 0usize;
        if before_bytes > 0 && self.cursor > 0 {
            // ropey's Chars is not DoubleEndedIterator — collect to String for
            // the backward scan (IME operations are rare, O(N) acceptable here).
            let prefix: String = self.text_rope.slice(..self.cursor).chars().collect();
            for ch in prefix.chars().rev() {
                if consumed >= before_bytes {
                    break;
                }
                consumed += ch.len_utf8();
                back_chars += 1;
            }
        }

        // Count forward chars until we've covered >= after_bytes.
        let mut fwd_chars = 0usize;
        consumed = 0usize;
        if after_bytes > 0 && self.cursor < self.text_rope.len_chars() {
            for ch in self.text_rope.slice(self.cursor..).chars() {
                if consumed >= after_bytes {
                    break;
                }
                consumed += ch.len_utf8();
                fwd_chars += 1;
            }
        }

        let start = self.cursor.saturating_sub(back_chars);
        let end = (self.cursor + fwd_chars).min(self.text_rope.len_chars());

        if start < end {
            self.text_rope.remove(start..end);
            self.cursor = start;
            self.selection_anchor = self.cursor;
            self.has_selection = false;
            self.bump_version();
            self.text_signal.set(self.text_rope.to_string());
            self.push_snapshot();
            self.notify_change();
        }
    }

    // ── IME Composition ──
    pub fn set_composition(&mut self, text: String, cursor_range: Option<(usize, usize)>) {
        let range = self.cursor..self.cursor;
        self.composition = Some(CompositionState {
            text,
            range,
            cursor_range,
        });
    }

    /// Clear composition state. The actual committed text has already been
    /// inserted via fire_text_input → insert_char by the IME commit handler.
    pub fn commit_composition(&mut self) {
        self.composition = None;
    }

    /// Commit any pending IME composition text into the document.
    /// Called on focus loss so the user's in-progress input is not lost.
    pub fn finalize_composition(&mut self) {
        if let Some(comp) = self.composition.take() {
            if !comp.text.is_empty() {
                self.insert_text(&comp.text);
            }
        }
    }

    pub fn clear_composition(&mut self) {
        self.composition = None;
    }

    // ── Newline / Tab ──
    pub fn insert_newline(&mut self) {
        if self.config.is_multiline() {
            self.insert_char('\n');
        }
    }

    pub fn insert_tab(&mut self) {
        if self.config.is_multiline() {
            self.insert_char('\t');
        }
    }

    // ── Clipboard helpers ──
    pub fn selected_text(&self) -> String {
        if self.config.is_password() {
            return String::new();
        }
        self.selection_range()
            .map(|(lo, hi)| self.text_rope.slice(lo..hi).to_string())
            .unwrap_or_default()
    }

    // ── Word boundary helpers (used by movement and action modules) ──
    pub fn word_left(s: &str, pos: usize) -> usize {
        let mut p = pos;
        let mut rev = s[..p.min(s.len())].chars().rev().peekable();
        while rev.peek().is_some_and(|c| c.is_whitespace()) {
            rev.next();
            p -= 1;
        }
        while rev.peek().is_some_and(|c| !c.is_whitespace()) {
            rev.next();
            p -= 1;
        }
        p
    }

    pub fn word_right(s: &str, pos: usize) -> usize {
        let mut p = pos;
        let mut it = s[p.min(s.len())..].chars().peekable();
        while it.peek().is_some_and(|c| !c.is_whitespace()) {
            it.next();
            p += 1;
        }
        while it.peek().is_some_and(|c| c.is_whitespace()) {
            it.next();
            p += 1;
        }
        p
    }
}

// ── Tests ──────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use auralis_signal::Signal;

    fn setup() -> (Signal<String>, Rc<RefCell<EditorState>>) {
        let sig = Signal::new(String::new());
        let config = TextInputConfig::default();
        let state = EditorState::new(sig.clone(), config);
        (sig, state)
    }

    #[test]
    fn insert_char_basic() {
        let (sig, state) = setup();
        {
            let mut s = state.borrow_mut();
            s.insert_char('h');
            s.insert_char('i');
        }
        assert_eq!(sig.read(), "hi");
        assert_eq!(state.borrow().cursor, 2);
    }

    #[test]
    fn insert_replaces_selection() {
        let (sig, state) = setup();
        {
            let mut s = state.borrow_mut();
            s.insert_text("hello world");
        }
        {
            let mut s = state.borrow_mut();
            s.cursor = 5;
            s.selection_anchor = 0;
            s.has_selection = true;
            s.insert_char('H');
        }
        assert_eq!(sig.read(), "H world");
    }

    #[test]
    fn delete_backward_char() {
        let (sig, state) = setup();
        {
            let mut s = state.borrow_mut();
            s.insert_text("abc");
            s.delete_backward();
        }
        assert_eq!(sig.read(), "ab");
        assert_eq!(state.borrow().cursor, 2);
    }

    #[test]
    fn delete_backward_selection() {
        let (sig, state) = setup();
        {
            let mut s = state.borrow_mut();
            s.insert_text("hello world");
            s.cursor = 5;
            s.selection_anchor = 0;
            s.has_selection = true;
            s.delete_backward();
        }
        assert_eq!(sig.read(), " world");
        assert_eq!(state.borrow().cursor, 0);
    }

    #[test]
    fn undo_restores_full_snapshot() {
        crate::core::clock::install_virtual();
        let (sig, state) = setup();
        {
            let mut s = state.borrow_mut();
            s.insert_text("first");
        }
        crate::core::clock::advance(std::time::Duration::from_millis(500));
        {
            let mut s = state.borrow_mut();
            s.insert_text("second");
            s.cursor = 3;
            s.selection_anchor = 0;
            s.has_selection = true;
        }
        {
            let mut s = state.borrow_mut();
            assert!(s.undo());
        }
        assert_eq!(sig.read(), "first");
        assert_eq!(state.borrow().cursor, 5);
        assert!(!state.borrow().has_selection);
        crate::core::clock::reset_to_wall();
    }

    #[test]
    fn redo_after_undo() {
        crate::core::clock::install_virtual();
        let (sig, state) = setup();
        {
            let mut s = state.borrow_mut();
            s.insert_text("first");
        }
        crate::core::clock::advance(std::time::Duration::from_millis(500));
        {
            let mut s = state.borrow_mut();
            s.insert_text("second");
        }
        {
            let mut s = state.borrow_mut();
            s.undo();
        }
        assert_eq!(sig.read(), "first");
        {
            let mut s = state.borrow_mut();
            s.redo();
        }
        assert_eq!(sig.read(), "firstsecond");
        crate::core::clock::reset_to_wall();
    }

    #[test]
    fn max_length_blocks_insert() {
        let sig = Signal::new(String::new());
        let config = TextInputConfig {
            max_length: Some(3),
            ..TextInputConfig::default()
        };
        let state = EditorState::new(sig.clone(), config);
        {
            let mut s = state.borrow_mut();
            assert!(s.insert_text("abc"));
            assert!(!s.insert_char('d'));
        }
        assert_eq!(sig.read(), "abc");
    }

    #[test]
    fn composition_does_not_push_snapshot() {
        let (_sig, state) = setup();
        {
            let mut s = state.borrow_mut();
            s.insert_text("hello");
        }
        let snapshot_count_before = state.borrow().undo_stack.entries.len();
        {
            let mut s = state.borrow_mut();
            s.set_composition("ABC".into(), None);
        }
        let snapshot_count_after = state.borrow().undo_stack.entries.len();
        assert_eq!(
            snapshot_count_before, snapshot_count_after,
            "composition should not affect undo stack"
        );
    }

    #[test]
    fn password_clipboard_returns_empty() {
        let sig = Signal::new(String::new());
        let config = TextInputConfig {
            input_type: TextInputType::Password,
            ..TextInputConfig::default()
        };
        let state = EditorState::new(sig.clone(), config);
        {
            let mut s = state.borrow_mut();
            s.insert_text("secret");
            s.cursor = 0;
            s.selection_anchor = 6;
            s.has_selection = true;
        }
        assert_eq!(state.borrow().selected_text(), "");
    }

    #[test]
    fn external_signal_sync_detected() {
        let (sig, state) = setup();
        sig.set("external change".to_string());
        {
            let mut s = state.borrow_mut();
            s.sync_from_signal();
        }
        assert_eq!(state.borrow().text_rope.to_string(), "external change");
        assert_eq!(state.borrow().cursor, "external change".chars().count());
    }

    /// IME DeleteSurrounding (C5): byte counts snap outward to char
    /// boundaries; ASCII and multi-byte (CJK) both behave.
    #[test]
    fn delete_surrounding_ascii_and_cjk() {
        let (sig, state) = setup();
        {
            let mut s = state.borrow_mut();
            s.insert_text("hello world");
            s.cursor = 5; // between "hello" and " world"
            s.selection_anchor = 5;
            s.delete_surrounding_bytes(2, 3); // remove "lo" and " wo"
        }
        assert_eq!(sig.read(), "helrld");
        assert_eq!(state.borrow().cursor, 3);

        let (sig2, state2) = setup();
        {
            let mut s = state2.borrow_mut();
            s.insert_text("简体字abc");
            s.cursor = 3; // after 字
            s.selection_anchor = 3;
            // 字 is 3 bytes; asking for 1 byte snaps to the full char.
            s.delete_surrounding_bytes(1, 1); // removes 字 and a
        }
        assert_eq!(sig2.read(), "简体bc");
        assert_eq!(state2.borrow().cursor, 2);
    }

    #[test]
    fn delete_surrounding_zero_is_noop() {
        let (sig, state) = setup();
        {
            let mut s = state.borrow_mut();
            s.insert_text("abc");
            s.delete_surrounding_bytes(0, 0);
        }
        assert_eq!(sig.read(), "abc");
    }
}
