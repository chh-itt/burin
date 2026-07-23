use std::cell::Cell;
use std::rc::Rc;

use crate::core::config::EventHandler;
use crate::core::ElementId;
use crate::event::action::{ActionKind, ActionOutcome};
use crate::event::registry::EventRegistry;
use crate::event::Key;
use crate::widgets::shared::{row_nav, RowNavOutcome};
use auralis_signal::Signal;

/// Unified keyboard navigation for dropdown-style widgets.
///
/// Registers `on_action` + `on_key_down` handlers that together provide:
///
/// | Key             | Channel      | Behaviour                          |
/// |-----------------|--------------|------------------------------------|
/// | Enter / Space   | `on_action`  | Open (if closed) or select (if open) |
/// | Escape           | `on_action`  | Close                              |
/// | Arrow keys       | `on_action`  | Navigate highlight                 |
/// | Home/End/PgUp/Dn | `on_key_down`| Navigate highlight                 |
/// | Tab              | `on_key_down`| Select + close                     |
/// | Character        | `on_key_down`| Type-ahead search                  |
///
/// **Why Enter/Space go through `on_action`:**
/// The key-binding pipeline maps Space→`Activate` and Enter→`NewLine`.
/// By consuming them in `on_action` we prevent the default `fire_click`
/// behaviour and avoid the need for fragile `space_just_opened` flags.
///
/// # Example (Select)
///
/// ```ignore
/// DropdownKeyboard::new(container_id, open.clone())
///     .with_highlighted(highlighted.clone())
///     .with_item_count(num_items)
///     .with_is_disabled(move |i| is_header(i))
///     .with_on_select(move |i| { sel.set(Some(values[i].clone())); })
///     .with_on_navigate(move |i| { sel_bg.sync(...); scroll_to(i); dirty(); })
///     .with_typeahead_labels(Rc::new(option_labels))
///     .register(reg);
/// ```
pub struct DropdownKeyboard {
    container_id: ElementId,
    open: Signal<bool>,
    highlighted: Rc<Cell<usize>>,
    item_count: usize,
    close_on_select: bool,
    /// When `true` (ComboBox), ArrowDown/Up open the dropdown when closed.
    /// When `false` (default, Select), directional keys are swallowed when
    /// closed so the trigger receives all keyboard events.
    open_on_down: bool,
    is_disabled: Box<dyn Fn(usize) -> bool>,
    on_select: Box<dyn Fn(usize)>,
    on_navigate: Box<dyn Fn(usize)>,
    typeahead_labels: Option<Rc<Vec<String>>>,
}

impl DropdownKeyboard {
    pub fn new(container_id: ElementId, open: Signal<bool>) -> Self {
        Self {
            container_id,
            open,
            highlighted: Rc::new(Cell::new(0)),
            item_count: 0,
            close_on_select: true,
            open_on_down: false,
            is_disabled: Box::new(|_| false),
            on_select: Box::new(|_| {}),
            on_navigate: Box::new(|_| {}),
            typeahead_labels: None,
        }
    }

    pub fn with_highlighted(mut self, h: Rc<Cell<usize>>) -> Self {
        self.highlighted = h;
        self
    }

    pub fn with_item_count(mut self, n: usize) -> Self {
        self.item_count = n;
        self
    }

    pub fn with_close_on_select(mut self, v: bool) -> Self {
        self.close_on_select = v;
        self
    }

    /// When `true` (ComboBox), ArrowDown/Up open the dropdown when closed.
    /// When `false` (default, Select), directional keys are swallowed when
    /// closed — the trigger handles Enter/Space to open instead.
    pub fn with_open_on_down(mut self, v: bool) -> Self {
        self.open_on_down = v;
        self
    }

    /// Predicate: returns `true` for indices that should be skipped
    /// during navigation (headers, disabled items, separators, etc.).
    pub fn with_is_disabled(mut self, f: impl Fn(usize) -> bool + 'static) -> Self {
        self.is_disabled = Box::new(f);
        self
    }

    /// Called when Enter / Space commits the highlighted index.
    pub fn with_on_select(mut self, f: impl Fn(usize) + 'static) -> Self {
        self.on_select = Box::new(f);
        self
    }

    /// Called after keyboard navigation moves the highlight to `idx`.
    /// The widget should sync visual highlight (SelectionBg), scroll the
    /// item into view, and mark the container dirty.
    pub fn with_on_navigate(mut self, f: impl Fn(usize) + 'static) -> Self {
        self.on_navigate = Box::new(f);
        self
    }

    /// Optional labels for type-ahead character search.
    pub fn with_typeahead_labels(mut self, labels: Rc<Vec<String>>) -> Self {
        self.typeahead_labels = Some(labels);
        self
    }

    /// Register `on_action` and `on_key_down` handlers on `container_id`.
    pub fn register(self, reg: &mut EventRegistry) {
        let Self {
            container_id,
            open,
            highlighted,
            item_count,
            close_on_select,
            open_on_down,
            is_disabled,
            on_select,
            on_navigate,
            typeahead_labels,
        } = self;

        // ── Shared callbacks (wrapped for both handlers) ──────────
        let is_disabled: Rc<Box<dyn Fn(usize) -> bool>> = Rc::new(is_disabled);
        let on_select: Rc<Box<dyn Fn(usize)>> = Rc::new(on_select);
        let on_navigate: Rc<Box<dyn Fn(usize)>> = Rc::new(on_navigate);

        // ── Build EventHandler with all keyboard/action handlers ──
        let mut dropdown_events = EventHandler::new().on_action({
            let open_a = open.clone();
            let hl_a = highlighted.clone();
            let is_dis_a = is_disabled.clone();
            let on_sel_a = on_select.clone();
            let on_nav_a = on_navigate.clone();
            let total_a = item_count;
            let cls_a = close_on_select;
            let ood = open_on_down;

            let first_enabled = {
                let is_dis_fe = is_dis_a.clone();
                move |start: usize| -> usize {
                    for off in 0..total_a {
                        let idx = (start + off) % total_a;
                        if !is_dis_fe(idx) {
                            return idx;
                        }
                    }
                    start
                }
            };

            move |action| {
                let kind = action.kind;

                if kind == ActionKind::Cancel {
                    // Only consume when the dropdown is actually open —
                    // otherwise let Escape bubble to the enclosing
                    // overlay (Modal) per the LIFO dismiss contract.
                    if open_a.read() {
                        open_a.set(false);
                        return ActionOutcome::Consumed;
                    }
                    return ActionOutcome::Unhandled;
                }

                if kind == ActionKind::Activate || kind == ActionKind::NewLine {
                    if open_a.read() {
                        let cur = hl_a.get();
                        if !is_dis_a(cur) {
                            on_sel_a(cur);
                            if cls_a {
                                open_a.set(false);
                            } else {
                                on_nav_a(cur);
                            }
                        }
                    } else {
                        let init = first_enabled(hl_a.get());
                        hl_a.set(init);
                        on_nav_a(init);
                        open_a.set(true);
                    }
                    return ActionOutcome::Consumed;
                }

                if kind == ActionKind::MoveDown || kind == ActionKind::MoveUp {
                    if !open_a.read() {
                        if ood && (kind == ActionKind::MoveDown || kind == ActionKind::MoveUp) {
                            let init = if kind == ActionKind::MoveDown {
                                first_enabled(hl_a.get())
                            } else {
                                let mut last = total_a.saturating_sub(1);
                                loop {
                                    if !is_dis_a(last) {
                                        break;
                                    }
                                    if last == 0 {
                                        break;
                                    }
                                    last -= 1;
                                }
                                last
                            };
                            hl_a.set(init);
                            on_nav_a(init);
                            open_a.set(true);
                        }
                        return ActionOutcome::Consumed;
                    }
                    let outcome = row_nav(kind, total_a, hl_a.get(), |i| is_dis_a(i));
                    if let RowNavOutcome::Navigate(ni) = outcome {
                        hl_a.set(ni);
                        on_nav_a(ni);
                    }
                    return ActionOutcome::Consumed;
                }

                ActionOutcome::Unhandled
            }
        });

        // ── on_key_down ───────────────────────────────────────────
        {
            let open_kd = open;
            let hl_kd = highlighted;
            let is_dis_kd = is_disabled;
            let on_sel_kd = on_select;
            let on_nav_kd = on_navigate;
            let total_kd = item_count;
            let ttl_kd = typeahead_labels;

            let ta_buf: Rc<Cell<String>> = Rc::new(Cell::new(String::new()));
            let ta_time: Rc<Cell<u64>> = Rc::new(Cell::new(0));

            dropdown_events = dropdown_events.on_key_down(move |key, _mods| -> bool {
                let is_open = open_kd.read();
                let cur_hl = hl_kd.get();

                match &key {
                    Key::Home | Key::End | Key::PageUp | Key::PageDown => {
                        if !is_open {
                            return false;
                        }
                        let kind = match &key {
                            Key::Home => ActionKind::MoveHome,
                            Key::End => ActionKind::MoveEnd,
                            Key::PageDown => ActionKind::MovePageDown,
                            Key::PageUp => ActionKind::MovePageUp,
                            _ => unreachable!(),
                        };
                        let outcome = row_nav(kind, total_kd, cur_hl, |i| is_dis_kd(i));
                        if let RowNavOutcome::Navigate(ni) = outcome {
                            hl_kd.set(ni);
                            on_nav_kd(ni);
                        }
                        true
                    }

                    Key::Escape => {
                        if is_open {
                            open_kd.set(false);
                            true
                        } else {
                            false
                        }
                    }

                    Key::Tab => {
                        if is_open {
                            if !is_dis_kd(cur_hl) {
                                on_sel_kd(cur_hl);
                            }
                            open_kd.set(false);
                        }
                        false
                    }

                    Key::Character(c) if c.len() == 1 && is_open && total_kd > 0 => {
                        if let Some(ref labels) = ttl_kd {
                            let ch = c.clone();
                            let now = std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap_or_default()
                                .as_millis() as u64;
                            if now - ta_time.get() > 800 {
                                ta_buf.set(String::new());
                            }
                            ta_time.set(now);
                            let mut buf = ta_buf.take();
                            buf.push_str(&ch);
                            let query = buf.to_lowercase();
                            let start = (cur_hl + 1) % total_kd;
                            let mut found = None;
                            for off in 0..total_kd {
                                let idx = (start + off) % total_kd;
                                if is_dis_kd(idx) {
                                    continue;
                                }
                                if let Some(label) = labels.get(idx) {
                                    if label.to_lowercase().starts_with(&query) {
                                        found = Some(idx);
                                        break;
                                    }
                                }
                            }
                            ta_buf.set(buf);
                            if let Some(idx) = found {
                                hl_kd.set(idx);
                                on_nav_kd(idx);
                            }
                        }
                        true
                    }

                    _ => false,
                }
            });
        }

        dropdown_events.register_all(reg, container_id);
    }
}
