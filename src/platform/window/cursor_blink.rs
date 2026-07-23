use crate::core::element::ElementArena;

/// O(k) cursor blink pass: iterate only cursor-component elements,
/// avoiding full tree walk during paint. Toggles cursor_visible every 500ms.
pub(crate) fn process_cursor_blink(arena: &ElementArena) {
    use crate::ecs::active::{drain_active, register_active, ActiveTag};
    for eid in drain_active(ActiveTag::CursorBlink) {
        let cursor = match arena.component_tables.borrow().cursor.get(&eid).cloned() {
            Some(c) => c,
            None => continue,
        };
        let is_focused = cursor.cursor_focused.get();
        let last_input = cursor.cursor_blink_last_input.get();
        let elapsed = crate::core::clock::now()
            .duration_since(last_input)
            .as_millis() as u64;
        let should_be_visible = if !is_focused {
            false
        } else {
            (elapsed / 500).is_multiple_of(2)
        };
        if cursor.cursor_visible.get() != should_be_visible {
            cursor.cursor_visible.set(should_be_visible);
            if let Some(el) = arena.get(eid) {
                el.mark_repaint();
            }
        }
        if is_focused {
            // Scheduler: next blink toggle at the next 500ms boundary.
            let phase = elapsed / 500;
            let next_ms = (phase + 1) * 500;
            let next_deadline = last_input + std::time::Duration::from_millis(next_ms);
            crate::core::scheduler::schedule_at(
                next_deadline,
                crate::core::scheduler::keys::CURSOR_BLINK,
            );
            register_active(eid, ActiveTag::CursorBlink);
        }
    }
}
