use crate::core::app_context::current_app;
use crate::core::config::EventHandler;
use crate::core::context::MountContext;
use crate::core::element::DirtyFlags;
use crate::core::element::ElementArena;
use crate::core::widget::Widget;
use crate::core::ElementId;
use crate::event::action::{ActionKind, ActionOutcome};
use crate::event::EventRegistry;
use crate::event::TraversalEdgeBehavior;
use crate::render::wgpu::glyphon_bridge::create_buffer;
use crate::style::styled::{StyleRefinement, Styled};
use crate::style::Color;
use crate::style::Point;
use crate::theme::m3::roles::{ComponentRole, DisplayRole, ResolvedComponentStyle};
use crate::widgets::shared::{row_nav, RowNavOutcome, SelectionBg};
use auralis_signal::Signal;
use kurbo::BezPath;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;

const MENU_WIDTH: f32 = 220.0;

/// A declarative right-click context menu widget.
pub struct ContextMenu {
    visible: Signal<bool>,
    position: Signal<Point>,
    items: Vec<ContextMenuItem>,
    style: StyleRefinement,
}

#[derive(Clone)]
/// A single item in a context menu.
pub struct ContextMenuItem {
    pub label: String,
    pub enabled: bool,
    pub separator: bool,
    pub children: Vec<ContextMenuItem>,
    pub icon: Option<crate::resource::icons::Icon>,
    pub shortcut: Option<String>,
    /// A check / radio mark drawn in the row's left (icon) slot.
    pub mark: Option<MenuMark>,
    action: Option<Rc<dyn Fn()>>,
}

/// A check or radio mark shown at a menu item's left slot. The state is a
/// snapshot taken when the menu opens; update your state source in the item's
/// action and it reflects on the next open (menus are transient).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MenuMark {
    /// Checkbox: `true` draws a check glyph, `false` reserves the slot.
    Check(bool),
    /// Radio: `true` draws a filled dot, `false` reserves the slot.
    Radio(bool),
}

impl ContextMenuItem {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            enabled: true,
            separator: false,
            children: Vec::new(),
            icon: None,
            shortcut: None,
            mark: None,
            action: None,
        }
    }
    pub fn action(mut self, f: impl Fn() + 'static) -> Self {
        self.action = Some(Rc::new(f));
        self
    }
    pub fn disabled(mut self) -> Self {
        self.enabled = false;
        self
    }
    pub fn submenu(mut self, items: Vec<ContextMenuItem>) -> Self {
        self.children = items;
        self
    }
    pub fn icon(mut self, icon: crate::resource::icons::Icon) -> Self {
        self.icon = Some(icon);
        self
    }
    pub fn shortcut(mut self, s: impl Into<String>) -> Self {
        self.shortcut = Some(s.into());
        self
    }
    /// Mark this item as a checkbox with the given checked state.
    pub fn checked(mut self, v: bool) -> Self {
        self.mark = Some(MenuMark::Check(v));
        self
    }
    /// Mark this item as a radio option with the given selected state.
    pub fn radio(mut self, v: bool) -> Self {
        self.mark = Some(MenuMark::Radio(v));
        self
    }
    pub fn separator() -> Self {
        Self {
            label: String::new(),
            enabled: false,
            separator: true,
            children: Vec::new(),
            icon: None,
            shortcut: None,
            mark: None,
            action: None,
        }
    }
}

impl ContextMenu {
    pub fn new(visible: Signal<bool>, position: Signal<Point>) -> Self {
        Self {
            visible,
            position,
            items: Vec::new(),
            style: StyleRefinement::default(),
        }
    }

    pub fn item(mut self, item: ContextMenuItem) -> Self {
        self.items.push(item);
        self
    }
}

impl Styled for ContextMenu {
    fn style_refinement(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl Widget for ContextMenu {
    fn mount_box(self: Box<Self>, ctx: &mut MountContext<'_>) -> ElementId {
        let theme = ctx.theme;
        let id = ctx.arena.allocate();
        let menu_role = ComponentRole::Display(DisplayRole::Popover);
        let menu_style = match theme.scheme.resolve_component(&menu_role) {
            ResolvedComponentStyle::Popover(s) => s,
            _ => unreachable!(),
        };
        {
            let Some(element) = ctx.arena.get_mut(id) else {
                return id;
            };
            element.set_layout_direction(crate::core::LayoutDirection::Vertical);

            element.set_border_width(1.0);
            element.set_preferred_width(Some(MENU_WIDTH));
            element.set_z_index(theme.z_index.dropdown);
            element.set_accessible_role(accesskit::Role::Menu);
            // RovingTabindex: the menu container is the single Tab stop.
            element.set_focusable(true);

            let rv = Rc::new(Cell::new(self.visible.read()));
            element.set_reactive_visible(rv.clone());
            let vis_sync = self.visible.clone();
            let dirty = element.dirty.clone();
            let eid = id;
            let rv_s = rv.clone();
            let scope_id = id;
            crate::core::signal_bridge::subscribe_owned(id, &self.visible, move || {
                let v = vis_sync.read();
                if rv_s.get() != v {
                    rv_s.set(v);
                    if v {
                        crate::event::push_modal_scope(scope_id, TraversalEdgeBehavior::Wrap);
                    } else {
                        crate::event::pop_modal_scope();
                    }
                    dirty.set(dirty.get() | DirtyFlags::REPAINT);
                    crate::core::dirty_registry::register_dirty(eid, DirtyFlags::REPAINT);
                }
            });
            if self.visible.read() {
                crate::event::push_modal_scope(id, TraversalEdgeBehavior::Wrap);
            }
            crate::widgets::shared::dropdown::register_unmount_pop_modal(id);

            let pos = self.position.read();
            element.set_bounds(crate::style::Rect::new(pos.x, pos.y, MENU_WIDTH, 0.0));
        }

        let parent_id = id;
        // RovingTabindex menu body — same model as `open_context_menu`, built
        // from raw rows (no Button), with keyboard nav wired via the shared
        // `register_menu_keyboard` helper. Dismiss = `visible.set(false)`.
        let focus_bg = theme.scheme.primary_container;
        let hover_bg = theme.scheme.primary_container;
        let pressed_bg = theme.scheme.primary;
        let fg = theme.scheme.on_surface;
        let focused_index: Rc<Cell<usize>> = Rc::new(Cell::new(0));
        let mut row_ids: Vec<ElementId> = Vec::new();
        let mut row_actions: Vec<Option<Rc<dyn Fn()>>> = Vec::new();
        let mut row_disabled: Vec<bool> = Vec::new();
        let mut row_labels: Vec<String> = Vec::new();
        for item in self.items {
            if item.separator {
                let sep_id = ctx.arena.allocate();
                if let Some(sep) = ctx.arena.get_mut(sep_id) {
                    sep.set_background(theme.scheme.outline);
                    sep.set_preferred_height(1.0);
                    sep.set_affected_by_child_size(false);
                    sep.set_flex_shrink(0.0);
                }
                ctx.arena.add_child(parent_id, sep_id);
                continue;
            }
            let row_id = ctx.arena.allocate();
            {
                let Some(el) = ctx.arena.get_mut(row_id) else {
                    return id;
                };
                el.set_layout_direction(crate::core::LayoutDirection::Horizontal);
                el.set_background(Color::TRANSPARENT);
                el.set_preferred_height(32.0);
                el.set_affected_by_child_size(false);
                el.set_flex_shrink(0.0); // don't compress rows inside a scrollable menu
                el.set_accepts_mouse(item.enabled);
                // ── Accessibility ──
                el.set_accessible_role(match item.mark {
                    Some(MenuMark::Check(_)) => accesskit::Role::MenuItemCheckBox,
                    Some(MenuMark::Radio(_)) => accesskit::Role::MenuItemRadio,
                    None => accesskit::Role::MenuItem,
                });
                el.set_accessible_label(item.label.clone());
                if let Some(MenuMark::Check(v) | MenuMark::Radio(v)) = item.mark {
                    el.set_accessible_checked(v);
                }
                if !item.enabled {
                    el.set_state_dirty(crate::core::config::StateFlags::DISABLED, true);
                }
                // Rows are not focusable; the container is the single Tab stop.
                el.set_focusable(false);
                el.with_state_style(|ss| {
                    ss.hovered.background = Some(hover_bg);
                    ss.pressed.background = Some(pressed_bg);
                    ss.checked.background = Some(focus_bg);
                    ss.focused.background = Some(focus_bg);
                });
            }
            ctx.arena.add_child(parent_id, row_id);
            if let Some(action) = &item.action {
                let a = action.clone();
                let vis = self.visible.clone();
                let row_click_events = EventHandler::new().on_click(move || {
                    a();
                    vis.set(false);
                });
                if let Some(reg) = ctx.event_registry.as_mut() {
                    row_click_events.register_all(reg, row_id);
                }
            }
            // ── Label ──
            {
                let lid = ctx.arena.allocate();
                if let Some(el) = ctx.arena.get_mut(lid) {
                    el.set_background(Color::TRANSPARENT);
                    el.set_flex_grow(1.0);
                    el.set_text_buffer(Rc::new(RefCell::new(create_buffer(
                        &item.label,
                        14.0,
                        1.3,
                        400,
                        None,
                        None,
                        crate::style::TextAlign::Start,
                    ))));
                    el.set_text_generation(Rc::new(Cell::new(1u64)));
                    el.set_text_vertical_center(true);
                    el.set_foreground(if item.enabled {
                        fg
                    } else {
                        fg.with_alpha(0.38)
                    });
                    el.set_padding(crate::style::Padding {
                        left: 12.0,
                        right: 12.0,
                        top: 0.0,
                        bottom: 0.0,
                    });
                }
                ctx.arena.add_child(row_id, lid);
            }
            row_ids.push(row_id);
            row_actions.push(item.action.clone());
            row_disabled.push(!item.enabled);
            row_labels.push(item.label.to_lowercase());
        }
        // Keyboard navigation — shared with `open_context_menu`.
        let sel_bg = Rc::new(SelectionBg::new(row_ids.clone()));
        {
            let vis = self.visible.clone();
            // The declarative widget does not support portal submenus.
            let no_submenu = vec![false; row_ids.len()];
            if let Some(reg) = ctx.event_registry.as_mut() {
                register_menu_keyboard(
                    reg,
                    id,
                    focused_index.clone(),
                    row_ids.clone(),
                    row_actions.clone(),
                    row_disabled.clone(),
                    no_submenu,
                    row_labels.clone(),
                    false,
                    sel_bg,
                    Rc::new(move || vis.set(false)),
                    Rc::new(|_: usize| {}),
                );
            }
        }

        ctx.register_theme_component(
            id,
            &ResolvedComponentStyle::Popover(menu_style.clone()),
            &menu_role,
            &self.style,
        );
        id
    }
}

impl std::fmt::Debug for ContextMenu {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ContextMenu")
            .field("items", &self.items.len())
            .finish_non_exhaustive()
    }
}

// ══════════════════════ System-level right-click hook ═══════════════════

/// Stored in element `user_data` so the framework can look up context-menu
/// items when the user right-clicks the element (or a descendant).
#[derive(Clone)]
pub struct ContextMenuItems(pub Vec<ContextMenuItem>);

impl std::fmt::Debug for ContextMenuItems {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("ContextMenuItems")
            .field(&self.0.len())
            .finish()
    }
}

/// A menu's horizontal open direction, stored on its container so child
/// submenus inherit it. `true` = this menu opened to the *left* of its parent.
#[derive(Clone, Copy)]
pub struct MenuOpenDir(pub bool);

// MENU_CHAIN, HOVERED_SUBMENU, SUBMENU_OPEN_TIME moved to AppContext.interaction.
// KB_MENU_REQUEST is per-window via the extension anymap (audit 2026-07-18):
// the widget-local KbMenuRequest type stays in this module, no core dependency.
#[derive(Default)]
struct KbMenuDomain {
    request: std::cell::Cell<Option<KbMenuRequest>>,
}

fn kb_menu_domain() -> std::rc::Rc<KbMenuDomain> {
    current_app().extension::<KbMenuDomain>()
}

/// Mark that a submenu just opened.  PointerMoved uses this to create a
/// ~300 ms "safe zone" where diagonal mouse movement doesn't change the
/// active HOVERED_SUBMENU.
pub fn mark_submenu_opened() {
    current_app().set_submenu_open_time(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_millis() as u64),
    );
}
/// True while the diagonal-movement safe zone is active.
pub fn is_submenu_recently_opened() -> bool {
    let last = current_app().submenu_open_time();
    if last == 0 {
        return false;
    }
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64);
    now.saturating_sub(last) < 300
}

/// Called on every PointerMoved.  If a submenu is open, the safe-triangle
/// period has expired, and the cursor is NOT inside the submenu, collapse it
/// immediately — no timer, no polling.
pub fn update_submenu_autoclose(
    arena: &mut ElementArena,
    pos: crate::style::Point,
    hit_path: &[ElementId],
) {
    if is_submenu_recently_opened() {
        return;
    }
    let depth = current_app().menu_chain_with(|chain| {
        if chain.len() <= 1 {
            return None;
        }
        let parent = chain[chain.len() - 2];
        let deepest = chain[chain.len() - 1];
        let arena_ref = &*arena;
        if let (Some(ps), Some(ss)) = (
            arena_ref.get(parent).map(|e| e.screen_bounds),
            arena_ref.get(deepest).map(|e| e.screen_bounds),
        ) {
            let min_x = ps.x.min(ss.x);
            let min_y = ps.y.min(ss.y);
            let max_x = (ps.x + ps.width).max(ss.x + ss.width);
            let max_y = (ps.y + ps.height).max(ss.y + ss.height);
            if crate::style::Rect::new(min_x, min_y, max_x - min_x, max_y - min_y).contains(pos) {
                return None;
            }
        }
        if hit_path.iter().any(|&eid| {
            crate::core::dirty_registry::is_descendant_of(eid, deepest)
                || crate::core::dirty_registry::is_descendant_of(eid, parent)
        }) {
            return None;
        }
        Some(chain.len().saturating_sub(2))
    });
    if let Some(depth) = depth {
        trim_chain_to(depth, arena);
    }
}

/// Dismiss all context menus (root + every submenu in the chain).
pub fn dismiss_context_menu() {
    let chain = current_app().menu_chain_with(std::mem::take);
    for portal_id in chain {
        crate::platform::portal::remove_portal(portal_id);
        crate::event::pop_modal_scope();
    }
}

/// Same as [`dismiss_context_menu`] but also immediately detaches
/// portal elements from the arena tree so they disappear in the
/// current frame (useful for scroll / focus-dismiss).
pub fn dismiss_context_menu_immediate(arena: &mut ElementArena) {
    let chain = current_app().menu_chain_with(std::mem::take);
    for portal_id in chain {
        arena.remove(portal_id);
        crate::platform::portal::remove_portal(portal_id);
        crate::event::pop_modal_scope();
    }
}

/// Check whether `pos` falls inside any open menu container in the
/// current chain (root or any submenu).  Used so that a click in the gap
/// between parent and child menu is still considered "on menu".
pub fn menu_chain_contains(arena: &ElementArena, pos: Point) -> bool {
    current_app().menu_chain_with(|m| {
        m.iter().any(|&portal_id| {
            arena
                .get(portal_id)
                .is_some_and(|el| el.screen_bounds.contains(pos))
        })
    })
}

/// True if any context menu (root or submenu) is currently open.
pub fn is_menu_open() -> bool {
    current_app().menu_chain_with(|m| !m.is_empty())
}

/// True if `eid` lies inside one of the currently-open menu containers.
///
/// Used to tell a hover over a real submenu row apart from a hover over an
/// ordinary widget that merely carries `ContextMenuItems` (e.g. a Table with a
/// right-click menu). Only the former should arm the hover-to-open-submenu
/// timer; the latter must NOT, or moving the mouse over the host widget would
/// spuriously re-open / dismiss the menu.
pub fn row_belongs_to_open_menu(eid: ElementId) -> bool {
    current_app().menu_chain_with(|m| {
        m.iter()
            .any(|&c| crate::core::dirty_registry::is_descendant_of(eid, c))
    })
}
/// Used when the mouse leaves a parent item that triggered a submenu.
pub fn trim_submenus(arena: &mut ElementArena) {
    trim_chain_to(0, arena);
}

/// Dismiss submenus deeper than `depth` (keeping `chain[0..=depth]`).
/// Each removed menu also pops its modal scope so the focus-scope stack stays
/// in sync with MENU_CHAIN — essential for nested cascades, where a leaked
/// mid-stack scope would never be pruned (pruning only happens at the top).
fn trim_chain_to(depth: usize, arena: &mut ElementArena) {
    current_app().menu_chain_with(|chain| {
        while chain.len() > depth + 1 {
            if let Some(portal_id) = chain.pop() {
                arena.remove(portal_id);
                crate::platform::portal::remove_portal(portal_id);
                crate::event::pop_modal_scope();
            }
        }
    });
}

// ══════════════════════ Submenu indicator ════════════════════

/// Right-pointing triangle glyph rendered on menu items that have
/// children.  Stored as `user_data` on the element so
/// `paint_element_surface` picks it up during the paint phase.
#[derive(Clone)]
pub struct SubmenuIndicator {
    pub path: Arc<BezPath>,
    pub size: f32,
}

fn submenu_arrow_path() -> BezPath {
    // Two connected strokes forming a right-pointing chevron (>).
    BezPath::from_vec(vec![
        kurbo::PathEl::MoveTo((2.0, 1.0).into()),
        kurbo::PathEl::LineTo((7.0, 5.0).into()),
        kurbo::PathEl::LineTo((2.0, 9.0).into()),
    ])
}

// ══════════════════════ Menu item icon ═════════════════════

/// Small icon rendered left of a menu item's label text.
/// Stored as `user_data` on the Button element so
/// `paint_element_surface` picks it up during the paint phase.
#[derive(Clone)]
pub struct MenuItemIcon {
    pub path: Arc<BezPath>,
    pub size: f32,
    pub color: Color,
    /// Fill the path (radio dot) instead of stroking it (icons / check glyph).
    pub filled: bool,
}

impl MenuItemIcon {
    pub fn from_kind(kind: crate::resource::icons::Icon, color: Color) -> Option<Self> {
        kind.build_path().map(|path| Self {
            path: Arc::new(path),
            size: 16.0,
            color,
            filled: false,
        })
    }

    /// A stroked check glyph for checkbox menu items.
    pub fn checkmark(color: Color) -> Option<Self> {
        crate::resource::icons::Icon::Check
            .build_path()
            .map(|path| Self {
                path: Arc::new(path),
                size: 16.0,
                color,
                filled: false,
            })
    }

    /// A small filled dot for selected radio menu items.
    pub fn radio_dot(color: Color) -> Self {
        // 4-segment cubic-Bézier circle (kappa ≈ 0.5523) in 24×24 icon space.
        let (cx, cy, r) = (12.0_f64, 12.0_f64, 4.0_f64);
        let k = r * 0.5523;
        let dot = BezPath::from_vec(vec![
            kurbo::PathEl::MoveTo((cx, cy - r).into()),
            kurbo::PathEl::CurveTo(
                (cx + k, cy - r).into(),
                (cx + r, cy - k).into(),
                (cx + r, cy).into(),
            ),
            kurbo::PathEl::CurveTo(
                (cx + r, cy + k).into(),
                (cx + k, cy + r).into(),
                (cx, cy + r).into(),
            ),
            kurbo::PathEl::CurveTo(
                (cx - k, cy + r).into(),
                (cx - r, cy + k).into(),
                (cx - r, cy).into(),
            ),
            kurbo::PathEl::CurveTo(
                (cx - r, cy - k).into(),
                (cx - k, cy - r).into(),
                (cx, cy - r).into(),
            ),
            kurbo::PathEl::ClosePath,
        ]);
        Self {
            path: Arc::new(dot),
            size: 16.0,
            color,
            filled: true,
        }
    }
}

/// A keyboard-driven menu mutation that must run where `arena` is available
/// (the window event loop). Mirrors the mouse-hover `HOVERED_SUBMENU` path.
#[derive(Clone, Copy, Debug)]
pub enum KbMenuRequest {
    /// Expand the submenu owned by this row element.
    OpenSubmenu(ElementId),
    /// Collapse the deepest submenu and return focus to its parent.
    CloseSubmenu,
}

pub fn set_kb_menu_request(req: KbMenuRequest) {
    kb_menu_domain().request.set(Some(req));
}

pub fn take_kb_menu_request() -> Option<KbMenuRequest> {
    kb_menu_domain().request.take()
}

/// Remove the deepest submenu layer (and pop its modal scope); returns the
/// container that becomes the new deepest menu (for refocus), or `None` if
/// only the root menu remains.
pub fn close_deepest_submenu(arena: &mut ElementArena) -> Option<ElementId> {
    current_app().menu_chain_with(|chain| {
        if chain.len() > 1 {
            if let Some(portal_id) = chain.pop() {
                arena.remove(portal_id);
                crate::platform::portal::remove_portal(portal_id);
                crate::event::pop_modal_scope();
            }
        }
        chain.last().copied()
    })
}

/// Wire RovingTabindex keyboard navigation onto a menu `container_id`.
///
/// Shared by the portal-based [`open_context_menu`] and the declarative
/// [`ContextMenu`] widget. The only behavioural difference between the two —
/// *how the menu is dismissed* — is injected via the `dismiss` callback.
///
/// Reuses the framework primitives `row_nav` (arrow / Home / End navigation
/// that natively skips disabled rows) and `SelectionBg` (single source of
/// truth for the keyboard-focus highlight), exactly like the `List` widget.
/// Replaces the old bespoke `KB_FOCUS` thread-local + paint hack.
///
/// Submenu control: →/Enter/Space on a `has_submenu` row requests expansion;
/// ← collapses the current submenu (when `is_submenu`). Both requests are
/// fulfilled by the window loop via [`KbMenuRequest`].
#[allow(clippy::too_many_arguments)]
fn register_menu_keyboard(
    reg: &mut EventRegistry,
    container_id: ElementId,
    focused_index: Rc<Cell<usize>>,
    row_ids: Vec<ElementId>,
    actions: Vec<Option<Rc<dyn Fn()>>>,
    disabled: Vec<bool>,
    has_submenu: Vec<bool>,
    // Lowercased row labels, for type-ahead first-letter jumping.
    labels: Vec<String>,
    is_submenu: bool,
    sel_bg: Rc<SelectionBg>,
    dismiss: Rc<dyn Fn()>,
    // Scrolls the given row index into view (no-op for non-scrolling menus).
    scroll_to: Rc<dyn Fn(usize)>,
) {
    // Highlight the first enabled row on open.
    if let Some(i0) = disabled.iter().position(|d| !d) {
        focused_index.set(i0);
        sel_bg.set_selected(i0);
    }

    // ── Type-ahead + keyboard navigation ──
    let ta_buf: Rc<Cell<String>> = Rc::new(Cell::new(String::new()));
    let ta_time: Rc<Cell<u64>> = Rc::new(Cell::new(0));
    let fi = focused_index.clone();
    let sel = sel_bg.clone();
    let st = scroll_to.clone();
    let menu_events = EventHandler::new()
        .on_key_down({
            let ta_buf = ta_buf.clone();
            let ta_time = ta_time.clone();
            let fi = fi.clone();
            let dis = disabled.clone();
            let sel = sel.clone();
            let st = st.clone();
            move |key, _mods| -> bool {
                let ch = match &key {
                    crate::event::Key::Character(c) if c.chars().count() == 1 => c.to_lowercase(),
                    _ => return false,
                };
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_millis() as u64)
                    .unwrap_or(0);
                if now.saturating_sub(ta_time.get()) > 800 {
                    ta_buf.set(String::new());
                }
                ta_time.set(now);
                let mut buf = ta_buf.take();
                buf.push_str(&ch);
                let cnt = labels.len();
                if cnt == 0 {
                    ta_buf.set(buf);
                    return false;
                }
                let cur = fi.get().min(cnt - 1);
                let start = if buf.chars().count() <= 1 {
                    (cur + 1) % cnt
                } else {
                    cur
                };
                let mut found = None;
                for off in 0..cnt {
                    let idx = (start + off) % cnt;
                    if dis[idx] {
                        continue;
                    }
                    if labels[idx].starts_with(&buf) {
                        found = Some(idx);
                        break;
                    }
                }
                ta_buf.set(buf);
                if let Some(new_idx) = found {
                    fi.set(new_idx);
                    sel.set_selected(new_idx);
                    st(new_idx);
                    true
                } else {
                    false
                }
            }
        })
        .on_action({
            let fi = focused_index;
            let disabled = disabled.clone();
            let has_submenu = has_submenu.clone();
            let actions = actions.clone();
            let row_ids = row_ids.clone();
            move |action| {
                let cnt = disabled.len();
                if cnt == 0 {
                    return ActionOutcome::Unhandled;
                }
                let old = fi.get().min(cnt - 1);
                match action.kind {
                    ActionKind::Cancel => {
                        dismiss();
                        ActionOutcome::Consumed
                    }
                    ActionKind::Activate | ActionKind::NewLine => {
                        if !disabled[old] {
                            if has_submenu[old] {
                                set_kb_menu_request(KbMenuRequest::OpenSubmenu(row_ids[old]));
                            } else if let Some(Some(cb)) = actions.get(old) {
                                cb();
                                dismiss();
                            }
                        }
                        ActionOutcome::Consumed
                    }
                    ActionKind::MoveRight if !disabled[old] && has_submenu[old] => {
                        set_kb_menu_request(KbMenuRequest::OpenSubmenu(row_ids[old]));
                        ActionOutcome::Consumed
                    }
                    ActionKind::MoveLeft if is_submenu => {
                        set_kb_menu_request(KbMenuRequest::CloseSubmenu);
                        ActionOutcome::Consumed
                    }
                    _ => match row_nav(action.kind, cnt, old, |i| disabled[i]) {
                        RowNavOutcome::Navigate(new_idx) => {
                            fi.set(new_idx);
                            sel_bg.set_selected(new_idx);
                            scroll_to(new_idx);
                            ActionOutcome::Consumed
                        }
                        _ => ActionOutcome::Unhandled,
                    },
                }
            }
        });
    menu_events.register_all(reg, container_id);
    // RovingTabindex: focus the container itself so arrow-key Actions route
    // to its on_action handler (path_to_root includes the container).
    reg.request_autofocus(container_id);
}

/// Open a context menu at `position`.
///
/// If `is_submenu` is true this menu is a child of a currently-open
/// parent — the parent stays visible.  Action clicks inside any menu
/// dismiss the **entire chain**.

pub fn open_context_menu(
    items: Vec<ContextMenuItem>,
    position: Point,
    arena: &mut ElementArena,
    root_id: ElementId,
    event_registry: Option<&mut EventRegistry>,
    parent_menu: Option<ElementId>,
    open_left: bool,
    screen_h: f32,
) {
    let is_submenu = parent_menu.is_some();
    match parent_menu {
        // Top-level menu: replace whatever is currently open.
        None => dismiss_context_menu(),
        // Submenu: `parent` is the parent menu's container; its index in
        // MENU_CHAIN is its depth. Trim everything *deeper* than the parent so
        // this menu stacks onto it — supporting arbitrary nesting depth
        // (data-driven) rather than a fixed single level.
        Some(parent) => {
            let parent_depth =
                current_app().menu_chain_with(|m| m.iter().position(|&c| c == parent));
            match parent_depth {
                Some(d) => trim_chain_to(d, arena),
                None => dismiss_context_menu(),
            }
        }
    }

    let theme = crate::theme::M3Theme::from_seed(Color::rgba8(0x67, 0x79, 0xE8, 0xFF));
    let n_items = items.iter().filter(|i| !i.separator).count().max(1);
    let menu_h = n_items as f32 * 32.0;
    // Cap the menu height to the screen; taller menus become scrollable.
    let visible_h = menu_h.min((screen_h - 16.0).max(64.0));
    let scroll_offset = Rc::new(Cell::new(crate::style::Vec2::ZERO));
    let content_bounds = Rc::new(Cell::new(crate::style::Rect::new(
        0.0, 0.0, MENU_WIDTH, menu_h,
    )));

    let container_id = arena.allocate();
    arena.component_tables.borrow_mut().preallocate(
        container_id,
        crate::ecs::components::SCROLL | crate::ecs::components::LAYOUT,
    );
    let depth = current_app().menu_chain_with(|m| m.len()) as i32;
    let menu_role = ComponentRole::Display(DisplayRole::Popover);
    let menu_style = match theme.scheme.resolve_component(&menu_role) {
        ResolvedComponentStyle::Popover(s) => s,
        _ => unreachable!(),
    };
    {
        let Some(el) = arena.get_mut(container_id) else {
            return;
        };
        el.set_layout_direction(crate::core::LayoutDirection::Vertical);

        el.set_border_width(1.0);
        el.set_preferred_width(Some(MENU_WIDTH));
        // Cascade depth: each level sits one z above its parent so a submenu
        // forced to overlap its parent occludes it via the (already-correct)
        // cross-z path. Each distinct z is its own render layer with its own
        // pooled TextRenderer (wgpu), so the text no longer corrupts. Depth =
        // this menu's index in MENU_CHAIN, after trimming and before push.
        let menu_z = theme.z_index.dropdown + depth;
        el.set_z_index(menu_z);
        el.z_index_floor = Some(menu_z);

        let pos = position;
        let portal_pos: Rc<Cell<(f32, f32, f32)>> = Rc::new(Cell::new((pos.x, pos.y, MENU_WIDTH)));
        el.insert_user_data(portal_pos);
        el.insert_user_data(crate::platform::portal::PortalHeight(Rc::new(Cell::new(
            visible_h,
        ))));
        // Scrollable when content exceeds the capped height (short menus clamp
        // to max_y=0 and never actually scroll). Mouse wheel is handled by the
        // window's generic scroll path; keyboard nav sets `pending_scroll`.
        // A scrollable container takes its height from `preferred_height`
        // (taffy scroll-container path), NOT from children — so cap it to the
        // visible height explicitly. content_bounds carries the true (taller)
        // content height used for clamping/scrollbar.
        let needs_scroll = menu_h > visible_h;
        if needs_scroll {
            el.set_preferred_height(visible_h);
            el.set_scroll_offset(scroll_offset.clone());
            el.set_content_bounds(content_bounds.clone());
            let max_scroll = Rc::new(std::cell::Cell::new((menu_h - visible_h).max(0.0)));
            el.set_max_scroll_y(max_scroll);
            el.set_overflow(crate::core::config::Overflow::Scroll);
            el.set_scrollbar_width(4.0);
        } else {
            el.set_preferred_height(menu_h);
        }

        el.set_focusable(true);
        el.set_accessible_role(accesskit::Role::Menu);
        // Remember this menu's horizontal open direction so child submenus
        // inherit it (avoids zig-zagging back onto an ancestor menu).
        el.insert_user_data(MenuOpenDir(open_left));
    }

    // Re-apply style + register theme component (no MountContext ctx available here)
    if let Some(el) = arena.get_mut(container_id) {
        crate::theme::apply::apply_style_to_element(
            el,
            &ResolvedComponentStyle::Popover(menu_style.clone()),
            &StyleRefinement::default(),
            theme.is_dark,
            theme.scheme.design_interaction,
        );
    }
    if let Some(lc) = arena
        .component_tables
        .borrow_mut()
        .lc
        .get_mut(&container_id)
    {
        lc.component_role = Some(menu_role);
        lc.style_refinement = Some(StyleRefinement::default());
    }
    crate::ecs::register_theme_element(container_id);

    // Mount menu items as horizontal rows: [icon] label spacer shortcut [arrow]
    // Keyboard-focus highlight: the theme's primary container colour, the
    // same as the mouse-hover highlight and the List widget's selection —
    // keyboard and pointer feedback now share one source of truth.
    let _focus_bg = theme.scheme.primary_container;
    // RovingTabindex model: `focused_index` tracks the highlighted row;
    // visual feedback flows through `SelectionBg` (bg_override) — the very
    // same primitive the List widget uses, instead of a bespoke thread-local.
    let focus_bg = theme.scheme.primary_container;
    let focused_index: Rc<Cell<usize>> = Rc::new(Cell::new(0));
    let mut row_ids: Vec<ElementId> = Vec::new();
    let mut row_actions: Vec<Option<Rc<dyn Fn()>>> = Vec::new();
    let mut row_disabled: Vec<bool> = Vec::new();
    let mut row_has_submenu: Vec<bool> = Vec::new();
    let mut row_labels: Vec<String> = Vec::new();
    {
        let app_weak = crate::core::app_context::try_with_current_app()
            .map(|rc| std::rc::Rc::downgrade(&rc))
            .unwrap_or_default();
        let mut ctx = MountContext::new(arena, None, event_registry, &theme, None, app_weak);

        for item in &items {
            if item.separator {
                let sid = ctx.arena.allocate();
                if let Some(s) = ctx.arena.get_mut(sid) {
                    s.set_background(theme.scheme.outline);
                    s.set_preferred_height(1.0);
                    s.set_affected_by_child_size(false);
                    s.set_flex_shrink(0.0);
                }
                ctx.arena.add_child(container_id, sid);
                continue;
            }
            let hover_bg = theme.scheme.primary_container;
            let pressed_bg = theme.scheme.primary;
            let row_height = 32.0;
            let fg = theme.scheme.on_surface;
            let sc_fg = theme.scheme.on_surface_variant;

            // ── Horizontal row container ──
            let row_id = ctx.arena.allocate();
            {
                let Some(el) = ctx.arena.get_mut(row_id) else {
                    return;
                };
                el.set_layout_direction(crate::core::LayoutDirection::Horizontal);
                el.set_background(Color::TRANSPARENT);
                el.set_preferred_height(row_height);
                el.set_affected_by_child_size(false);
                el.set_flex_shrink(0.0); // don't compress rows inside a scrollable menu
                el.set_accepts_mouse(item.enabled);
                // ── Accessibility ──
                el.set_accessible_role(match item.mark {
                    Some(MenuMark::Check(_)) => accesskit::Role::MenuItemCheckBox,
                    Some(MenuMark::Radio(_)) => accesskit::Role::MenuItemRadio,
                    None => accesskit::Role::MenuItem,
                });
                el.set_accessible_label(item.label.clone());
                if let Some(MenuMark::Check(v) | MenuMark::Radio(v)) = item.mark {
                    el.set_accessible_checked(v);
                }
                if !item.enabled {
                    el.set_state_dirty(crate::core::config::StateFlags::DISABLED, true);
                }
                // RovingTabindex: rows are NOT focusable — the container is the
                // single Tab stop. Keyboard highlight flows via CHECKED flag
                // (SelectionBg), mouse hover via HOVERED flag (StateStyle).
                el.set_focusable(false);
                el.with_state_style(|ss| {
                    ss.hovered.background = Some(hover_bg);
                    ss.pressed.background = Some(pressed_bg);
                    ss.checked.background = Some(focus_bg);
                    ss.focused.background = Some(focus_bg);
                });
                // Left slot: a check/radio mark takes priority over a custom icon.
                match item.mark {
                    Some(MenuMark::Check(true)) => {
                        if let Some(mi) = MenuItemIcon::checkmark(fg) {
                            el.insert_user_data(mi);
                        }
                    }
                    Some(MenuMark::Radio(true)) => {
                        el.insert_user_data(MenuItemIcon::radio_dot(fg));
                    }
                    // Checkable but unchecked: no glyph; the slot stays reserved
                    // via the label padding below so rows align.
                    Some(_) => {}
                    None => {
                        if let Some(ref icon_kind) = item.icon {
                            if let Some(mi) = MenuItemIcon::from_kind(*icon_kind, fg) {
                                el.insert_user_data(mi);
                            }
                        }
                    }
                }
                if !item.children.is_empty() {
                    el.insert_user_data(SubmenuIndicator {
                        path: Arc::new(submenu_arrow_path()),
                        size: 10.0,
                    });
                    el.insert_user_data(ContextMenuItems(item.children.clone()));
                }
                if let Some(action) = &item.action {
                    let a = action.clone();
                    let row_oc_events = EventHandler::new().on_click(move || {
                        a();
                        dismiss_context_menu();
                    });
                    if let Some(reg) = ctx.event_registry.as_mut() {
                        row_oc_events.register_all(reg, row_id);
                    }
                }
            }
            ctx.arena.add_child(container_id, row_id);
            // Track row for keyboard navigation
            row_ids.push(row_id);
            let action_clone: Option<Rc<dyn Fn()>> = item.action.as_ref().map(|a| a.clone());
            row_actions.push(action_clone);
            row_disabled.push(!item.enabled);
            row_has_submenu.push(!item.children.is_empty());
            row_labels.push(item.label.to_lowercase());

            // ── Label ──
            {
                let lid = ctx.arena.allocate();
                if let Some(el) = ctx.arena.get_mut(lid) {
                    el.set_background(Color::TRANSPARENT);
                    el.set_flex_grow(1.0);
                    el.set_text_buffer(Rc::new(RefCell::new(create_buffer(
                        &item.label,
                        14.0,
                        1.3,
                        400,
                        None,
                        None,
                        crate::style::TextAlign::Start,
                    ))));
                    el.set_text_generation(Rc::new(Cell::new(1u64)));
                    el.set_text_vertical_center(true);
                    el.set_foreground(if item.enabled {
                        fg
                    } else {
                        fg.with_alpha(0.38)
                    });
                    el.set_padding(crate::style::Padding {
                        left: if item.icon.is_some() || item.mark.is_some() {
                            28.0
                        } else {
                            12.0
                        },
                        right: 4.0,
                        top: 0.0,
                        bottom: 0.0,
                    });
                }
                ctx.arena.add_child(row_id, lid);
            }

            // ── Shortcut hint ──
            if let Some(ref sc) = item.shortcut {
                let sid = ctx.arena.allocate();
                let sc_right = if item.children.is_empty() { 12.0 } else { 28.0 };
                if let Some(el) = ctx.arena.get_mut(sid) {
                    el.set_background(Color::TRANSPARENT);
                    el.set_text_buffer(Rc::new(RefCell::new(create_buffer(
                        sc,
                        14.0 * 0.85,
                        1.3,
                        400,
                        None,
                        None,
                        crate::style::TextAlign::End,
                    ))));
                    el.set_text_generation(Rc::new(Cell::new(1u64)));
                    el.set_text_vertical_center(true);
                    el.set_foreground(sc_fg);
                    el.set_padding(crate::style::Padding {
                        left: 4.0,
                        right: sc_right,
                        top: 0.0,
                        bottom: 0.0,
                    });
                }
                ctx.arena.add_child(row_id, sid);
            }
        }

        // ── Keyboard navigation (RovingTabindex, shared with ContextMenu widget) ──
        let sel_bg = Rc::new(SelectionBg::new(row_ids.clone()));
        // Scroll the focused row into view directly (the pending-scroll path
        // only runs on relayout frames, which keyboard nav doesn't trigger).
        let scroll_to: Rc<dyn Fn(usize)> = {
            let so = scroll_offset.clone();
            let cid = container_id;
            Rc::new(move |idx: usize| {
                let row_h = 32.0_f32;
                let target_y = idx as f32 * row_h;
                let mut o = so.get();
                let old_y = o.y;
                if target_y < o.y {
                    o.y = target_y;
                } else if target_y + row_h > o.y + visible_h {
                    o.y = target_y + row_h - visible_h;
                }
                o.y = o.y.clamp(0.0, (menu_h - visible_h).max(0.0));
                if (o.y - old_y).abs() > 0.5 {
                    so.set(o);
                    crate::core::dirty_registry::spatial_update_scroll(cid, o.x, o.y);
                    crate::core::dirty_registry::bump_subtree_gen(cid);
                    crate::core::dirty_registry::mark_dirty(cid, DirtyFlags::REPAINT);
                    crate::core::dirty_registry::register_dirty(cid, DirtyFlags::REPAINT);
                }
            })
        };
        if let Some(reg) = ctx.event_registry.as_mut() {
            register_menu_keyboard(
                reg,
                container_id,
                focused_index.clone(),
                row_ids.clone(),
                row_actions.clone(),
                row_disabled.clone(),
                row_has_submenu.clone(),
                row_labels.clone(),
                is_submenu,
                sel_bg,
                Rc::new(dismiss_context_menu),
                scroll_to,
            );
        }

        // Trap keyboard within the menu (like a dialog)
        crate::event::push_modal_scope(container_id, TraversalEdgeBehavior::Wrap);
    }

    arena.add_child(root_id, container_id);
    crate::platform::portal::push_portal(container_id);

    // Outside-click dismissal is owned by the window's PointerDown handler
    // (`menu_chain_contains` check → `dismiss_context_menu_immediate`), NOT
    // the portal dismiss system. The old `register_dismiss(9999, …)` here
    // was dead code: its skip counter armed only after 9999 outside clicks,
    // so the callback never ran (audit 2026-07-18, AnchoredPopup pass).

    current_app().menu_chain_push(container_id);
}
