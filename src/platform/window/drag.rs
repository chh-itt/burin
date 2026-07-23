use crate::core::config::StateFlags;
use crate::core::dirty_registry;
use crate::core::element::DirtyFlags;
use crate::core::ElementId;
use crate::event::DragData;
use crate::render::wgpu::glyphon_bridge::create_buffer;
use crate::style;
use super::WindowState;

/// State machine for in-app widget-to-widget drag-and-drop.
pub(crate) struct DragState {
    pub(crate) source: ElementId,
    pub(crate) payload: Option<DragData>,
    pub(crate) cursor: style::Point,
    pub(crate) ghost: Option<ElementId>,
    pub(crate) hovered_target: Option<ElementId>,
}

pub(crate) fn start_drag(state: &mut WindowState, source: ElementId, cursor: style::Point) {
    let payload = state
        .arena
        .get(source)
        .and_then(|el| el.on_drag_start().map(|ref f| f()))
        .or_else(|| state.arena.get(source).and_then(|el| el.drag_data()));
    let ghost = create_drag_ghost(state, source, &payload);
    state.drag_state = Some(DragState {
        source,
        payload,
        cursor,
        ghost,
        hovered_target: None,
    });
}

fn create_drag_ghost(
    state: &mut WindowState,
    source: ElementId,
    payload: &Option<DragData>,
) -> Option<ElementId> {
    let label = payload
        .as_ref()
        .and_then(|p| p.label.clone())
        .or_else(|| payload.as_ref().and_then(|p| p.text.clone()))
        .or_else(|| state.arena.get(source).and_then(|el| el.accessible_label()))
        .unwrap_or_else(|| "↕".into());
    let theme = state.config.theme.clone();
    let ghost_id = state.arena.allocate();
    {
        let el = state.arena.get_mut(ghost_id).unwrap();
        el.set_background(theme.scheme.surface.with_alpha(0.85));
        el.set_foreground(theme.scheme.on_surface);
        el.set_border_width(1.0);
        el.set_border_color(theme.scheme.outline_variant);
        el.set_corner_radius(6.0);
        el.set_opacity(0.9);
        el.set_z_index(500);
        el.set_preferred_width(Some(140.0));
        el.set_preferred_height(28.0);
        el.set_padding(style::Padding::all(6.0));
        el.set_input_pass_through(true);
        el.set_affected_by_child_size(false);
        el.set_preferred_width(Some(0.0));
        el.set_preferred_height(0.0);
        el.set_flex_grow(0.0);
        el.set_flex_shrink(0.0);
        el.set_visible(true);
    }
    let buf = create_buffer(
        &label,
        12.0,
        1.3,
        400,
        None,
        None,
        style::TextAlign::Center,
    );
    {
        let el = state.arena.get_mut(ghost_id).unwrap();
        el.set_text_buffer(std::rc::Rc::new(std::cell::RefCell::new(buf)));
        el.set_text_generation(std::rc::Rc::new(std::cell::Cell::new(1u64)));
        el.set_accessible_label(label);
        el.mark_repaint();
        dirty_registry::register_dirty(ghost_id, DirtyFlags::REPAINT);
    }
    if let Some(root) = state.arena.root_id {
        state.arena.add_child(root, ghost_id);
    }
    Some(ghost_id)
}

pub(crate) fn update_drag_cursor(state: &mut WindowState, cursor: style::Point) {
    if state.drag_state.is_none() {
        return;
    }
    state.drag_state.as_mut().unwrap().cursor = cursor;
    update_drag_hover(state);
    let ghost_id = state.drag_state.as_ref().and_then(|ds| ds.ghost);
    if let Some(ghost) = ghost_id {
        let x = state.drag_state.as_ref().map_or(0.0, |ds| ds.cursor.x);
        let y = state.drag_state.as_ref().map_or(0.0, |ds| ds.cursor.y);
        if let Some(el) = state.arena.get_mut(ghost) {
            let rect = style::Rect::new(x + 12.0, y + 12.0, 140.0, 28.0);
            el.screen_bounds = rect;
            el.set_bounds(rect);
            dirty_registry::update_bounds(ghost, rect);
            el.mark_repaint();
        }
    }
}

fn update_drag_hover(state: &mut WindowState) {
    let ds = match state.drag_state.as_mut() {
        Some(s) => s,
        None => return,
    };
    let old_target = ds.hovered_target;
    let ghost_id = ds.ghost;
    let hit = dirty_registry::hit_test_with_fallback(&state.arena, ds.cursor);

    // Walk ancestors to find the nearest drop_target container.
    let mut new_target = None;
    let mut cur = hit;
    while let Some(id) = cur {
        if Some(id) == ghost_id {
            break;
        }
        if id == ds.source {
            break;
        }
        if state.arena.get(id).is_some_and(|el| el.drop_target()) {
            // Validate accept_drop_types against payload kind
            let accepted =
                if let (Some(el), Some(ref payload)) = (state.arena.get(id), &ds.payload) {
                    let types = el.accept_drop_types();
                    types.is_empty() || types.iter().any(|dt| dt.matches(&payload.kind))
                } else {
                    true
                };
            if accepted {
                new_target = Some(id);
                break;
            }
        }
        cur = dirty_registry::parent_of(id);
    }

    if old_target != new_target {
        if let Some(old) = old_target {
            if let Some(el) = state.arena.get_mut(old) {
                el.set_state_dirty(StateFlags::DRAG_OVER, false);
            }
        }
        if let Some(new) = new_target {
            if let Some(el) = state.arena.get_mut(new) {
                el.set_state_dirty(StateFlags::DRAG_OVER, true);
            }
        }
    }
    ds.hovered_target = new_target;
}

pub(crate) fn end_drag(state: &mut WindowState) {
    if let Some(ds) = state.drag_state.take() {
        if let Some(old) = ds.hovered_target {
            if let Some(el) = state.arena.get_mut(old) {
                el.set_state_dirty(StateFlags::DRAG_OVER, false);
            }
        }
        if let Some(ghost) = ds.ghost {
            state.arena.remove(ghost);
        }
        if let Some(target) = ds.hovered_target {
            if let Some(mut payload) = ds.payload {
                payload.position = Some(ds.cursor);
                if let Some(el) = state.arena.get(target) {
                    if let Some(ref handler) = el.on_drop() {
                        handler(payload);
                    }
                }
            }
        }
    }
}
