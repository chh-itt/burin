use crate::core::config::StateFlags;
use crate::core::element::{Element, ElementArena};
use crate::core::ElementId;
use crate::event::action::{Action, ActionKind, ActionOutcome};
use crate::event::types::{Event, GesturePhase, Key, Modifiers};
use crate::event::{EventRegistry, FocusManager, FocusReason};
use crate::style::Point;

/// Result of dispatching an event through the propagation system.
#[derive(Debug, Clone, Copy)]
pub struct DispatchOutcome {
    /// Whether a widget handler consumed the event (stopping further propagation).
    pub handled: bool,
    /// If a drag gesture was recognized (by the gesture arena), the element that won.
    pub drag_winner: Option<ElementId>,
}

pub fn dispatch_event(
    arena: &mut ElementArena,
    event: &Event,
    hit_path: &[ElementId],
    focus: &mut FocusManager,
    registry: &mut EventRegistry,
    modifiers: Modifiers,
) -> DispatchOutcome {
    match event {
        Event::Click {
            position,
            modifiers: click_mods,
            ..
        } => DispatchOutcome {
            handled: propagate_click(arena, hit_path, *position, *click_mods, registry),
            drag_winner: None,
        },
        Event::PointerDown {
            position,
            finger_id,
            ..
        } => {
            let pid = finger_id.unwrap_or(0);
            let is_touch = finger_id.is_some();
            let win = crate::event::recognizer::process_pointer_event(
                hit_path,
                crate::event::GesturePhase::Started,
                *position,
                pid,
                is_touch,
            );
            // Eager drag wins at PointerDown: synthesize drag_start now.
            if let Some(w) = win {
                if w.kind == crate::event::recognizer::RecognizerKind::Drag {
                    fire_drag_start_local(arena, registry, w.element_id, *position);
                }
            }

            // Overlay: if there's an active overlay and the click is outside it, dismiss
            if crate::event::overlay::is_active() {
                if let Some(overlay_id) = crate::event::overlay::should_dismiss_on_click_outside() {
                    if !hit_path.contains(&overlay_id)
                        && !hit_path.iter().any(|_eid| {
                            false // Simplified: widget checks its own bounds
                        })
                    {
                        crate::event::overlay::remove(overlay_id);
                        return DispatchOutcome {
                            handled: true,
                            drag_winner: None,
                        };
                    }
                }
            }

            DispatchOutcome {
                handled: propagate_pointer_down(
                    arena, hit_path, *position, focus, registry, modifiers,
                ),
                drag_winner: win.map(|w| w.element_id),
            }
        }
        Event::PointerMove {
            position,
            finger_id,
            ..
        } => {
            let pid = finger_id.unwrap_or(0);
            let is_touch = finger_id.is_some();
            let win = crate::event::recognizer::process_pointer_event(
                hit_path,
                crate::event::GesturePhase::Moved,
                *position,
                pid,
                is_touch,
            );
            // Threshold drag verdict arrives mid-move: late drag_start.
            if let Some(w) = win {
                if w.kind == crate::event::recognizer::RecognizerKind::Drag {
                    fire_drag_start_local(arena, registry, w.element_id, *position);
                }
            }
            // Scroll capture: apply the finger delta to the container
            // (wheel-space: do_scroll subtracts, so content follows the
            // finger) and record a velocity sample for the release fling.
            if let Some((sc_eid, dx, dy)) =
                crate::event::recognizer::scroll_capture_advance(pid, *position)
            {
                crate::widgets::bundle::scroll::do_scroll(arena, sc_eid, dx, dy);
            }
            DispatchOutcome {
                handled: propagate_pointer_move(arena, *position, pid, registry),
                drag_winner: win.map(|w| w.element_id),
            }
        }
        Event::PointerUp {
            position,
            finger_id,
            ..
        } => {
            let pid = finger_id.unwrap_or(0);
            let is_touch = finger_id.is_some();
            // Read the captures BEFORE the arena resolves/clears state.
            let capture = crate::event::recognizer::drag_capture(pid);
            let scroll_release = crate::event::recognizer::scroll_capture_release(pid, *position);
            let win = crate::event::recognizer::process_pointer_event(
                hit_path,
                crate::event::GesturePhase::Ended,
                *position,
                pid,
                is_touch,
            );
            // Fling: tracked velocity is in finger space; the ballistic
            // simulation runs in offset space (inverted).
            if let Some((sc_eid, v)) = scroll_release {
                crate::widgets::bundle::scroll::try_fling(
                    arena,
                    sc_eid,
                    crate::style::Vec2::new(-v.x, -v.y),
                );
            }
            let handled = propagate_pointer_up(arena, capture, *position, registry);
            crate::event::recognizer::clear_drag_capture(pid);
            DispatchOutcome {
                handled,
                drag_winner: win.map(|w| w.element_id),
            }
        }
        Event::Scroll { delta_x, delta_y } => DispatchOutcome {
            handled: propagate_scroll(hit_path, *delta_x, *delta_y, registry),
            drag_winner: None,
        },
        Event::KeyDown { key, modifiers } => {
            if *key == crate::event::Key::Escape
                && crate::event::overlay::should_dismiss_on_escape()
            {
                crate::event::overlay::pop();
                return DispatchOutcome {
                    handled: true,
                    drag_winner: None,
                };
            }
            DispatchOutcome {
                handled: propagate_key(hit_path, focus, key, modifiers, registry, true),
                drag_winner: None,
            }
        }
        Event::KeyUp { key, modifiers } => DispatchOutcome {
            handled: propagate_key(hit_path, focus, key, modifiers, registry, false),
            drag_winner: None,
        },
        Event::Pinch { delta, phase, .. } => DispatchOutcome {
            handled: propagate_pinch(hit_path, *delta, *phase, registry),
            drag_winner: None,
        },
        Event::Rotate { delta, phase, .. } => DispatchOutcome {
            handled: propagate_rotate(hit_path, *delta, *phase, registry),
            drag_winner: None,
        },
        Event::DragStart { .. }
        | Event::DragMove { .. }
        | Event::DragEnd { .. }
        | Event::DragCancel { .. } => {
            // Drag events are handled via pointer-event propagation
            // (propagate_pointer_down/move/up → registry.fire_drag_start/update/end).
            // DragCancel is a system interrupt that clears drag state; no widget dispatch needed.
            DispatchOutcome {
                handled: false,
                drag_winner: None,
            }
        }
        Event::Custom { .. } => {
            // Custom events are dispatched as no-ops by default.
            // Third-party code may handle them via EventRegistry.
            DispatchOutcome {
                handled: false,
                drag_winner: None,
            }
        }
    }
}

fn propagate_pinch(
    path: &[ElementId],
    delta: f64,
    phase: GesturePhase,
    registry: &mut EventRegistry,
) -> bool {
    for &id in path.iter().rev() {
        if registry.fire_pinch(id, delta, phase) {
            return true;
        }
    }
    for &id in path.iter() {
        if registry.fire_pinch(id, delta, phase) {
            return true;
        }
    }
    false
}

fn propagate_rotate(
    path: &[ElementId],
    delta: f32,
    phase: GesturePhase,
    registry: &mut EventRegistry,
) -> bool {
    for &id in path.iter().rev() {
        if registry.fire_rotate(id, delta, phase) {
            return true;
        }
    }
    for &id in path.iter() {
        if registry.fire_rotate(id, delta, phase) {
            return true;
        }
    }
    false
}

fn scroll_to(arena: &ElementArena, target: ElementId) -> crate::style::Vec2 {
    let (sx, sy) = arena.accumulated_scroll(target);
    crate::style::Vec2::new(sx, sy)
}

pub fn propagate_click(
    _arena: &ElementArena,
    path: &[ElementId],
    _position: Point,
    mods: Modifiers,
    registry: &mut EventRegistry,
) -> bool {
    // Target-first (leaf → root): the deepest element with a handler wins, so an
    // ancestor's click handler cannot shadow a descendant's. Previously a
    // root→leaf "capture" pass fired the OUTERMOST handler first, which meant a
    // checkbox cell inside a clickable row never received its own click — the
    // row's selection handler always shadowed it.
    for &id in path.iter() {
        if registry.has_handlers(id) {
            registry.fire_click(id);
            registry.fire_click_with_mods(id, mods);
            registry.fire_click_at_with_mods(id, _position, mods);
            return true;
        }
    }
    false
}

/// Synthesize drag_start on the arena's drag winner (eager: at
/// PointerDown; threshold: at the mid-move verdict).
fn fire_drag_start_local(
    arena: &ElementArena,
    registry: &mut EventRegistry,
    id: ElementId,
    position: Point,
) {
    if !registry.has_drag_start(id) {
        return;
    }
    if let Some(sb) = arena.get(id).map(|el| el.screen_bounds) {
        let (sx, sy) = arena.accumulated_scroll(id);
        let local = Point::new(position.x - sb.x + sx, position.y - sb.y + sy);
        registry.fire_drag_start(id, local, position);
    }
}

fn propagate_pointer_down(
    arena: &mut ElementArena,
    path: &[ElementId],
    position: Point,
    focus: &mut FocusManager,
    registry: &mut EventRegistry,
    modifiers: Modifiers,
) -> bool {
    const REASON: FocusReason = FocusReason::PointerClick;
    let old_focus = focus.focused();

    let focus_target = path
        .iter()
        .rev()
        .find(|&&id| arena.get(id).is_some_and(|el| el.is_focusable()))
        .copied();

    if let Some(hit_id) = focus_target {
        if old_focus != Some(hit_id) {
            if let Some(old_id) = old_focus {
                if let Some(el) = arena.get_mut(old_id) {
                    el.set_state_dirty(StateFlags::FOCUSED, false);
                    el.last_focus_reason.set(Some(REASON));
                }
                registry.fire_focus_out(old_id, REASON);
            }
            focus.set_focused(Some(hit_id));
            if let Some(el) = arena.get_mut(hit_id) {
                el.set_state_dirty(StateFlags::FOCUSED, true);
                el.last_focus_reason.set(Some(REASON));
            }
            registry.fire_focus_in(hit_id, REASON);
        }
    } else if let Some(old_id) = old_focus {
        if focus.has_deferred_blurs() {
        } else {
            if let Some(el) = arena.get_mut(old_id) {
                el.set_state_dirty(StateFlags::FOCUSED, false);
                el.last_focus_reason.set(Some(REASON));
            }
            registry.fire_focus_out(old_id, REASON);
            focus.set_focused(None);
        }
    }

    // click_at: find deepest element with a handler (leaf → root)
    for &id in path.iter() {
        if registry.has_click_at(id) {
            if let Some(el) = arena.get(id) {
                let sb = el.screen_bounds;
                let scroll = scroll_to(arena, id);
                let local = Point::new(position.x - sb.x + scroll.x, position.y - sb.y + scroll.y);
                registry.fire_click_at(id, local);
                registry.fire_click_at_with_mods(id, local, modifiers);
            }
            break;
        }
    }

    // Drag synthesis moved to the gesture arena (audit 2026-07-19 G2a):
    // drag_start fires when a Drag-kind registration WINS (eager: at
    // PointerDown via dispatch_event; threshold: at the 6px verdict) —
    // not unconditionally on any element that happens to have a handler.

    true
}

fn propagate_pointer_move(
    arena: &ElementArena,
    position: Point,
    pointer_id: u64,
    registry: &mut EventRegistry,
) -> bool {
    // Updates route to the pointer's drag CAPTURE (the arena's Drag-kind
    // winner) — hit paths are irrelevant mid-drag, so wandering outside
    // the element's bounds cannot lose the gesture.
    if let Some(cap) = crate::event::recognizer::drag_capture(pointer_id) {
        if registry.has_drag_update(cap) {
            if let Some(sb) = arena.get(cap).map(|el| el.screen_bounds) {
                let (sx, sy) = arena.accumulated_scroll(cap);
                let local = Point::new(position.x - sb.x + sx, position.y - sb.y + sy);
                registry.fire_drag_update(cap, local, position);
            }
            return true;
        }
    }
    false
}

fn propagate_pointer_up(
    arena: &ElementArena,
    capture: Option<ElementId>,
    position: Point,
    registry: &mut EventRegistry,
) -> bool {
    // drag_end goes to whoever held the capture for this pointer.
    if let Some(cap) = capture {
        if registry.has_drag_end(cap) {
            if let Some(sb) = arena.get(cap).map(|el| el.screen_bounds) {
                let (sx, sy) = arena.accumulated_scroll(cap);
                let local = Point::new(position.x - sb.x + sx, position.y - sb.y + sy);
                registry.fire_drag_end(cap, local, position);
            }
            return true;
        }
    }
    false
}

pub fn propagate_scroll(
    path: &[ElementId],
    dx: f32,
    dy: f32,
    registry: &mut EventRegistry,
) -> bool {
    // Capture: root → leaf
    for &id in path.iter().rev() {
        if registry.fire_scroll(id, dx, dy) {
            return true;
        }
    }
    // Bubble: leaf → root
    for &id in path.iter() {
        if registry.fire_scroll(id, dx, dy) {
            return true;
        }
    }
    false
}

fn propagate_key(
    path: &[ElementId],
    focus: &mut FocusManager,
    key: &Key,
    modifiers: &Modifiers,
    registry: &mut EventRegistry,
    is_down: bool,
) -> bool {
    let focused = focus.focused();

    // Focused element first (it's the most specific target)
    if let Some(fid) = focused {
        if is_down {
            if registry.fire_key_down(fid, key.clone(), *modifiers) {
                return true;
            }
        } else {
            if registry.fire_key_up(fid, key.clone(), *modifiers) {
                return true;
            }
        }
    }

    // Capture: root → leaf
    for &id in path.iter().rev() {
        if Some(id) == focused {
            continue;
        }
        if is_down {
            if registry.fire_key_down(id, key.clone(), *modifiers) {
                return true;
            }
        } else {
            if registry.fire_key_up(id, key.clone(), *modifiers) {
                return true;
            }
        }
    }
    // Bubble: leaf → root
    if !focused.is_some() {
        for &id in path.iter() {
            if is_down {
                if registry.fire_key_down(id, key.clone(), *modifiers) {
                    return true;
                }
            } else {
                if registry.fire_key_up(id, key.clone(), *modifiers) {
                    return true;
                }
            }
        }
    }
    false
}

pub fn dispatch_action(
    _arena: &mut ElementArena,
    action: &Action,
    hit_path: &[ElementId],
    registry: &mut EventRegistry,
    _window_defaults: &[(ActionKind, fn(&mut Element, &Action) -> ActionOutcome)],
) -> ActionOutcome {
    propagate_action(hit_path, action, registry)
}

fn propagate_action(
    path: &[ElementId],
    action: &Action,
    registry: &mut EventRegistry,
) -> ActionOutcome {
    // Cancel is innermost-first by nature (Escape closes the deepest open
    // popup, not every ancestor overlay at once) — bubble only, first
    // Consumed wins (audit 2026-07-18, popup dismiss contract).
    if action.kind == ActionKind::Cancel {
        for &id in path.iter() {
            match registry.fire_action(id, action) {
                ActionOutcome::Blocked => return ActionOutcome::Blocked,
                ActionOutcome::Consumed => return ActionOutcome::Consumed,
                ActionOutcome::Unhandled => {}
            }
        }
        return ActionOutcome::Unhandled;
    }
    // Capture: root → leaf
    let mut best = ActionOutcome::Unhandled;
    for &id in path.iter().rev() {
        match registry.fire_action(id, action) {
            ActionOutcome::Blocked => return ActionOutcome::Blocked,
            ActionOutcome::Consumed => best = ActionOutcome::Consumed,
            ActionOutcome::Unhandled => {}
        }
    }
    if best.is_handled() {
        return best;
    }
    // Bubble: leaf → root
    for &id in path.iter() {
        match registry.fire_action(id, action) {
            ActionOutcome::Blocked => return ActionOutcome::Blocked,
            ActionOutcome::Consumed => return ActionOutcome::Consumed,
            ActionOutcome::Unhandled => {}
        }
    }
    ActionOutcome::Unhandled
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::rc::Rc;

    /// A click must be handled by the DEEPEST element with a handler (the target),
    /// not by an ancestor. Otherwise a checkbox cell inside a clickable row never
    /// receives its own click (the row's selection handler shadows it).
    #[test]
    fn click_targets_deepest_handler_not_ancestor() {
        let arena = ElementArena::new();
        let mut reg = EventRegistry::new();
        let parent = ElementId::allocate();
        let child = ElementId::allocate();
        let fired: Rc<Cell<&'static str>> = Rc::new(Cell::new(""));
        {
            let f = fired.clone();
            reg.on_click(parent, move || f.set("parent"));
        }
        {
            let f = fired.clone();
            reg.on_click(child, move || f.set("child"));
        }
        // hit_path is ordered leaf → root.
        let path = [child, parent];
        let handled = propagate_click(
            &arena,
            &path,
            Point::new(0.0, 0.0),
            Modifiers::NONE,
            &mut reg,
        );
        assert!(handled, "click should be handled");
        assert_eq!(
            fired.get(),
            "child",
            "deepest handler (target) must win over ancestor"
        );
    }
}
