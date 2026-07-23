use crate::core::dirty_registry;
use crate::core::element::DirtyFlags;
use crate::core::ElementId;
use crate::event::action::{Action, ActionKind};
use crate::event::focus_traversal::Direction;
use crate::event::FocusReason;
use super::cancel_path::cancel_path_for_visible;
use super::ime::request_ime_enable;
use super::WindowState;

pub(crate) fn dispatch_action(state: &mut WindowState, action: &Action, path: &[ElementId]) {
    let outcome = crate::event::propagation::dispatch_action(
        &mut state.arena,
        action,
        path,
        &mut state.event_registry,
        &[],
    );
    if !outcome.is_handled() {
        match action.kind {
            ActionKind::Activate | ActionKind::NewLine => {
                if let Some(fid) = state.focus_manager.focused() {
                    state.event_registry.fire_click(fid);
                }
            }
            ActionKind::FocusNext | ActionKind::InsertTab => {
                // Tab closes any open context menu, then performs normal
                // focus traversal. (RovingTabindex menus have a single Tab
                // stop, so "wrapping inside" would be a no-op; standard
                // desktop menus dismiss on Tab.)
                crate::widgets::overlay::dismiss_context_menu_immediate(&mut state.arena);
                if let Some(next_id) = state.focus_manager.focus_next(&state.arena) {
                    transfer_focus(state, next_id, FocusReason::TabNavigation);
                }
            }
            ActionKind::FocusPrev => {
                crate::widgets::overlay::dismiss_context_menu_immediate(&mut state.arena);
                if let Some(prev_id) = state.focus_manager.focus_prev(&state.arena) {
                    transfer_focus(state, prev_id, FocusReason::TabNavigation);
                }
            }
            ActionKind::MoveDown => {
                if let Some(next_id) = state
                    .focus_manager
                    .focus_in_direction(&state.arena, Direction::Down)
                {
                    transfer_focus(state, next_id, FocusReason::TabNavigation);
                }
            }
            ActionKind::MoveUp => {
                if let Some(next_id) = state
                    .focus_manager
                    .focus_in_direction(&state.arena, Direction::Up)
                {
                    transfer_focus(state, next_id, FocusReason::TabNavigation);
                }
            }
            ActionKind::MoveLeft => {
                if let Some(next_id) = state
                    .focus_manager
                    .focus_in_direction(&state.arena, Direction::Left)
                {
                    transfer_focus(state, next_id, FocusReason::TabNavigation);
                }
            }
            ActionKind::MoveRight => {
                if let Some(next_id) = state
                    .focus_manager
                    .focus_in_direction(&state.arena, Direction::Right)
                {
                    transfer_focus(state, next_id, FocusReason::TabNavigation);
                }
            }
            ActionKind::Copy => {
                if let Some(fid) = state.focus_manager.focused() {
                    #[cfg(feature = "clipboard")]
                    if let Some(text) = state.event_registry.fire_clipboard_copy(fid) {
                        if !text.is_empty() {
                            if let Err(e) = crate::platform::Clipboard.write_text(&text) {
                                crate::core::error::push_error(crate::core::error::UiError::Clipboard(e));
                            }
                        }
                    }
                }
            }
            ActionKind::Cut => {
                if let Some(fid) = state.focus_manager.focused() {
                    #[cfg(feature = "clipboard")]
                    if let Some(text) = state.event_registry.fire_clipboard_copy(fid) {
                        if !text.is_empty() {
                            if let Err(e) = crate::platform::Clipboard.write_text(&text) {
                                crate::core::error::push_error(crate::core::error::UiError::Clipboard(e));
                            }
                        }
                    }
                    let del_action = Action::new(ActionKind::DeleteForward);
                    let path = state.arena.path_to_root(fid);
                    let _ = crate::event::propagation::dispatch_action(
                        &mut state.arena,
                        &del_action,
                        &path,
                        &mut state.event_registry,
                        &[],
                    );
                }
            }
            ActionKind::Paste => {
                if let Some(fid) = state.focus_manager.focused() {
                    match crate::platform::Clipboard.read_text() {
                        Ok(Some(text)) => state.event_registry.fire_clipboard_paste(fid, text),
                        Ok(None) => {}
                        #[cfg(feature = "clipboard")]
                        Err(e) => crate::core::error::push_error(crate::core::error::UiError::Clipboard(e)),
                        #[cfg(not(feature = "clipboard"))]
                        Err(_) => {} // NotAvailable is expected with the feature off
                    }
                }
            }
            ActionKind::Undo | ActionKind::Redo => {
                if let Some(fid) = state.focus_manager.focused() {
                    if let Some(state_undo) = state.arena.get(fid).and_then(|el| {
                        el.get_user_data::<crate::core::undo::ElementUndoState>()
                    }) {
                        match action.kind {
                            ActionKind::Undo => {
                                state_undo.undo_all();
                            }
                            ActionKind::Redo => {
                                state_undo.redo_all();
                            }
                            _ => {}
                        }
                    }
                }
            }
            ActionKind::Cancel => {
                if let Some(root_id) = state.arena.root_id {
                    let path = cancel_path_for_visible(&state.arena, root_id);
                    if !path.is_empty() {
                        let _ = crate::event::propagation::dispatch_action(
                            &mut state.arena,
                            action,
                            &path,
                            &mut state.event_registry,
                            &[],
                        );
                    }
                }
            }
            _ => {}
        }
    }
}

pub(crate) fn transfer_focus(state: &mut WindowState, new_id: ElementId, reason: FocusReason) {
    crate::event::focus_manager::transfer_focus(
        &mut state.arena,
        &mut state.event_registry,
        &mut state.focus_manager,
        new_id,
        reason,
    );
    if let Some(ref w) = state.winit_window {
        request_ime_enable(w, &state.event_registry, new_id);
    }
}

/// Force relayout + repaint of the whole UI after a context-menu portal is
/// removed during the event phase. `arena.remove` marks the parent for
/// relayout but NOT repaint, so the overlay's pixels would otherwise linger
/// until some later event happens to trigger a repaint. This mirrors the
/// repaint side-effect an action callback would normally cause via signals.
pub(crate) fn invalidate_after_menu_change(state: &mut WindowState) {
    state.needs_taffy = true;
    if let Some(rid) = state.arena.root_id {
        dirty_registry::mark_dirty(rid, DirtyFlags::REPAINT);
        dirty_registry::register_dirty(
            rid,
            DirtyFlags::REPAINT,
        );
        dirty_registry::bump_subtree_gen(rid);
    }
    if let Some(ref w) = state.winit_window {
        w.request_redraw();
    }
}
