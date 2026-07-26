use std::cell::Cell;
use std::rc::Rc;

use auralis_signal::Signal;

use crate::core::config::EventHandler;
use crate::core::context::MountContext;
use crate::core::dirty_registry;
use crate::core::element::{with_ct_mut, DirtyFlags};
use crate::core::widget::Widget;
use crate::core::ElementId;
use crate::ecs::components;
use crate::event::action::{Action, ActionKind, ActionOutcome};
use crate::event::{Key, Modifiers};
use crate::style::styled::{StyleRefinement, Styled};
use crate::style::Point;

/// Which axis the splitter moves along.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SplitDirection {
    Horizontal,
    Vertical,
}

fn set_first_pane_size(eid: ElementId, is_h: bool, v: f32) {
    with_ct_mut(|ct| {
        if let Some(layout) = ct.layout.get_mut(&eid) {
            layout.flex_grow = 0.0; // switch to fixed-size mode
            if is_h {
                layout.preferred_width = Some(v);
            } else {
                layout.preferred_height = v;
            }
        }
    });
}

fn mark_dirty_tree(ids: &[ElementId]) {
    for &id in ids {
        dirty_registry::mark_dirty(id, DirtyFlags::MEASURE);
        dirty_registry::register_dirty(id, DirtyFlags::MEASURE);
        dirty_registry::bump_subtree_gen(id);
    }
    // Invalidate all caches to force children of resized panes to re-record
    crate::core::app_context::with_current_app(|app| app.queue_clear_all_caches());
}

/// A resizable split pane with a draggable divider.
///
/// Contains two children and a handle between them.  Drag the handle
/// to resize the panes.  Use `.direction()` to pick horizontal or vertical.
pub struct SplitPane {
    first: Option<Box<dyn Widget>>,
    second: Option<Box<dyn Widget>>,
    direction: SplitDirection,
    split_ratio: f32,
    min_first: f32,
    min_second: f32,
    divider_width: f32,
    bind_split_signal: Option<Signal<f32>>,
    style: StyleRefinement,
}

impl SplitPane {
    pub fn new(first: impl Widget + 'static, second: impl Widget + 'static) -> Self {
        Self {
            first: Some(Box::new(first)),
            second: Some(Box::new(second)),
            direction: SplitDirection::Horizontal,
            split_ratio: 0.5,
            min_first: 0.0,
            min_second: 0.0,
            divider_width: 4.0,
            bind_split_signal: None,
            style: StyleRefinement::default(),
        }
    }

    pub fn direction(mut self, d: SplitDirection) -> Self {
        self.direction = d;
        self
    }
    pub fn split_ratio(mut self, r: f32) -> Self {
        self.split_ratio = r.clamp(0.0, 1.0);
        self
    }
    pub fn min_sizes(mut self, first: f32, second: f32) -> Self {
        self.min_first = first;
        self.min_second = second;
        self
    }
    pub fn divider_width(mut self, w: f32) -> Self {
        self.divider_width = w;
        self
    }
    pub fn bind_split(mut self, s: Signal<f32>) -> Self {
        self.bind_split_signal = Some(s);
        self
    }
}

impl Styled for SplitPane {
    fn style_refinement(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl Widget for SplitPane {
    fn component_mask(&self) -> u64 {
        components::STYLE | components::LAYOUT | components::INTERACTION | components::LIFECYCLE
    }

    fn mount_box(mut self: Box<Self>, ctx: &mut MountContext<'_>) -> ElementId {
        let theme = ctx.theme;
        let is_h = matches!(self.direction, SplitDirection::Horizontal);
        let min1 = self.min_first;
        let min2 = self.min_second;
        let dw = self.divider_width;

        let id = ctx.arena.allocate();
        ctx.preallocate(id, self.component_mask());
        {
            let Some(element) = ctx.arena.get_mut(id) else {
                return id;
            };
            element.set_accessible_role(accesskit::Role::Group);
            element.set_layout_direction(match self.direction {
                SplitDirection::Horizontal => crate::core::LayoutDirection::Horizontal,
                SplitDirection::Vertical => crate::core::LayoutDirection::Vertical,
            });
            element.set_flex_grow(1.0);
            element.set_affected_by_child_size(false);
        }

        // ── First pane ──
        let first_id = {
            let mut cc = ctx.child_with_events(id);
            self.first.take().unwrap().mount_box(&mut cc)
        };
        {
            let Some(el) = ctx.arena.get_mut(first_id) else {
                return id;
            };
            el.set_affected_by_child_size(false);
            el.set_flex_grow(1.0);
            el.set_flex_basis(0.0);
            el.set_flex_shrink(1.0);
        }
        ctx.arena.add_child(id, first_id);

        // ── Divider ──
        let divider_id = ctx.arena.allocate();
        {
            let Some(div) = ctx.arena.get_mut(divider_id) else {
                return id;
            };
            div.set_accessible_role(accesskit::Role::Splitter);
            div.set_accessible_label(String::from("Resize split pane"));
            div.set_affected_by_child_size(false);
            div.set_accessible_value(50.0);
            div.set_accessible_min(0.0);
            div.set_accessible_max(100.0);
            div.set_focusable(true);
            div.set_flex_grow(0.0);
            div.set_flex_shrink(0.0);
            if is_h {
                div.set_preferred_width(Some(dw));
            } else {
                div.set_preferred_height(dw);
            }
            div.set_background(theme.scheme.outline);
            div.set_corner_radius(dw * 0.5);
            div.with_state_style(|ss| {
                ss.hovered.background = Some(theme.scheme.outline_variant);
                ss.pressed.background = Some(theme.scheme.primary);
            });
            div.set_cursor_icon(Some(if is_h {
                crate::platform::CursorIcon::COL_RESIZE
            } else {
                crate::platform::CursorIcon::ROW_RESIZE
            }));
        }
        ctx.arena.add_child(id, divider_id);
        crate::ecs::register_theme_element(divider_id);
        crate::ecs::register_theme_element(id);

        // ── Second pane ──
        let second_id = {
            let mut cc = ctx.child_with_events(id);
            self.second.take().unwrap().mount_box(&mut cc)
        };
        {
            let Some(el) = ctx.arena.get_mut(second_id) else {
                return id;
            };
            el.set_flex_grow(1.0);
            el.set_flex_basis(0.0);
            el.set_flex_shrink(1.0);
            el.set_min_main(0.0);
        }
        ctx.arena.add_child(id, second_id);

        // ── Drag state ──
        // drag_anchor: (start_absolute_cursor, start_pane_size)
        let drag_anchor: Rc<Cell<Option<(f32, f32)>>> = Rc::new(Cell::new(None));
        let pane_size = Rc::new(Cell::new(200.0f32));

        let da_start = drag_anchor.clone();
        let ps_start = pane_size.clone();
        let _first_start = first_id;
        let _is_h_start = is_h;

        let divider_events = {
            let ps = pane_size.clone();
            let da = drag_anchor.clone();
            let cont_id = id;
            let first_id2 = first_id;
            let second_id2 = second_id;
            let is_h2 = is_h;
            let dw2 = dw;
            let m1 = min1;
            let m2 = min2;

            let drag_update_da = da.clone();
            let drag_update_ps = ps.clone();
            let on_drag_update = move |_local: Point, abs: Point| {
                let cursor = if is_h2 { abs.x } else { abs.y };
                let (anchor_cursor, start_size) = match drag_update_da.get() {
                    Some(v) => v,
                    None => return,
                };
                let delta = cursor - anchor_cursor;
                let raw = start_size + delta;
                let max_pane = crate::core::dirty_registry::bounds_of(cont_id)
                    .map(|r| (if is_h2 { r.width } else { r.height }) - dw2 - m2)
                    .unwrap_or(f32::MAX);
                let new_size = raw.clamp(m1, max_pane);
                drag_update_ps.set(new_size);
                set_first_pane_size(first_id2, is_h2, new_size);
                mark_dirty_tree(&[first_id2, second_id2, cont_id]);
            };

            let da_end = drag_anchor.clone();
            let _div_id_end = divider_id;
            let div_id3 = divider_id;

            EventHandler::new()
                .on_drag_start({
                    let is_h_start = is_h;
                    let first_start = first_id;
                    let ps_start = ps_start.clone();
                    let da_start = da_start.clone();
                    move |_local: Point, abs: Point| {
                        let cursor = if is_h_start { abs.x } else { abs.y };
                        let actual = crate::core::dirty_registry::bounds_of(first_start)
                            .map(|r| if is_h_start { r.width } else { r.height })
                            .unwrap_or(ps_start.get());
                        ps_start.set(actual);
                        da_start.set(Some((cursor, actual)));
                    }
                })
                .on_drag_update(on_drag_update)
                .on_drag_end(move |_local: Point, _abs: Point| {
                    da_end.set(None);
                })
                .on_hover_leave(move || {
                    crate::core::dirty_registry::set_state(
                        div_id3,
                        crate::core::config::StateFlags::PRESSED,
                        false,
                    );
                })
                .on_action({
                    let is_h4 = is_h;
                    move |action: &Action| -> ActionOutcome {
                        match (is_h4, action.kind) {
                            (true, ActionKind::MoveLeft)
                            | (true, ActionKind::MoveRight)
                            | (false, ActionKind::MoveUp)
                            | (false, ActionKind::MoveDown) => ActionOutcome::Consumed,
                            _ => ActionOutcome::Unhandled,
                        }
                    }
                })
                .on_key_down({
                    let ps2 = pane_size.clone();
                    let cont_id2 = id;
                    let first_id3 = first_id;
                    let second_id3 = second_id;
                    let is_h3 = is_h;
                    move |key: Key, _mod: Modifiers| -> bool {
                        let step = 10.0f32;
                        let delta = match (is_h3, key) {
                            (true, Key::ArrowLeft) | (false, Key::ArrowUp) => -step,
                            (true, Key::ArrowRight) | (false, Key::ArrowDown) => step,
                            _ => return false,
                        };
                        let actual = crate::core::dirty_registry::bounds_of(first_id3)
                            .map(|r| if is_h3 { r.width } else { r.height })
                            .unwrap_or(ps2.get());
                        ps2.set(actual);
                        let raw = ps2.get() + delta;
                        let max_pane = crate::core::dirty_registry::bounds_of(cont_id2)
                            .map(|r| (if is_h3 { r.width } else { r.height }) - dw - m1)
                            .unwrap_or(f32::MAX);
                        let new_size = raw.clamp(min1, max_pane);
                        ps2.set(new_size);
                        set_first_pane_size(first_id3, is_h3, new_size);
                        mark_dirty_tree(&[first_id3, second_id3, cont_id2]);
                        true
                    }
                })
        };
        if let Some(reg) = ctx.event_registry.as_mut() {
            divider_events.register_all(reg, divider_id);
        }

        // ── External signal binding ──
        if let Some(sig) = self.bind_split_signal {
            let ps = pane_size.clone();
            let first_id2 = first_id;
            let second_id2 = second_id;
            let cont_id = id;
            let is_h2 = is_h;
            let sig2 = sig.clone();
            crate::core::signal_bridge::subscribe_owned(id, &sig, move || {
                let v = sig2.read().max(min1);
                ps.set(v);
                set_first_pane_size(first_id2, is_h2, v);
                mark_dirty_tree(&[first_id2, second_id2, cont_id]);
            });
        }

        id
    }
}

impl std::fmt::Debug for SplitPane {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SplitPane")
            .field("direction", &self.direction)
            .finish_non_exhaustive()
    }
}
