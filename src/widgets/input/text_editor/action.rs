use super::state::EditorState;
use crate::event::action::{Action, ActionKind, ActionOutcome};
use cosmic_text::Buffer;
use ropey::Rope;

/// Dispatch an Action to EditorState. Returns Consumed if handled.
pub fn handle_action(
    state: &mut EditorState,
    action: &Action,
    buffer: &mut Buffer,
) -> ActionOutcome {
    let handled = match action.kind {
        ActionKind::MoveLeft => {
            state.move_left(action.selection);
            true
        }
        ActionKind::MoveRight => {
            state.move_right(action.selection);
            true
        }
        ActionKind::MoveHome => {
            state.move_to_start(action.selection);
            true
        }
        ActionKind::MoveEnd => {
            state.move_to_end(action.selection);
            true
        }
        ActionKind::MoveWordLeft => {
            state.move_word_left(action.selection);
            true
        }
        ActionKind::MoveWordRight => {
            state.move_word_right(action.selection);
            true
        }
        ActionKind::MoveUp => {
            state.move_visual_row(-1, action.selection, buffer);
            true
        }
        ActionKind::MoveDown => {
            state.move_visual_row(1, action.selection, buffer);
            true
        }

        ActionKind::DeleteBackward => {
            state.delete_backward();
            true
        }
        ActionKind::DeleteForward => {
            state.delete_forward();
            true
        }
        ActionKind::DeleteWordBackward => {
            let text = state.text_rope.to_string();
            let pos = EditorState::word_left(&text, state.cursor);
            if pos < state.cursor {
                let mut chars: Vec<char> = text.chars().collect();
                if state.cursor <= chars.len() {
                    chars.drain(pos..state.cursor);
                    let new_text: String = chars.into_iter().collect();
                    state.text_rope = Rope::from_str(&new_text);
                    state.cursor = pos;
                    state.selection_anchor = pos;
                    state.has_selection = false;
                    state.bump_version();
                    state.text_signal().set(state.text_rope.to_string());
                    state.push_snapshot();
                }
            }
            true
        }
        ActionKind::DeleteWordForward => {
            let text = state.text_rope.to_string();
            let pos = EditorState::word_right(&text, state.cursor);
            if pos > state.cursor {
                let mut chars: Vec<char> = text.chars().collect();
                if pos <= chars.len() {
                    chars.drain(state.cursor..pos);
                    let new_text: String = chars.into_iter().collect();
                    state.text_rope = Rope::from_str(&new_text);
                    state.bump_version();
                    state.text_signal().set(state.text_rope.to_string());
                    state.push_snapshot();
                }
            }
            true
        }
        ActionKind::NewLine => {
            if state.config.is_multiline() {
                state.insert_newline();
            } else if let Some(ref cb) = *state.config.on_submit.borrow() {
                // Single-line: Enter triggers submit
                cb(state.text_rope.to_string());
            }
            true
        }
        ActionKind::InsertTab => {
            state.insert_tab();
            true
        }
        ActionKind::SelectAll => {
            state.select_all();
            true
        }

        ActionKind::Undo => state.undo(),
        ActionKind::Redo => state.redo(),

        ActionKind::Cancel => {
            state.has_selection = false;
            state.selection_anchor = state.cursor;
            state.composition = None;
            false
        }
        ActionKind::Submit => {
            if let Some(ref cb) = *state.config.on_submit.borrow() {
                cb(state.text_rope.to_string());
            }
            true
        }

        _ => false,
    };

    if handled {
        ActionOutcome::Consumed
    } else {
        ActionOutcome::Unhandled
    }
}
