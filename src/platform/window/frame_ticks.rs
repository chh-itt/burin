use crate::core::element::ElementArena;
use crate::core::ElementId;

/// O(active) frame_tick pass: iterate only elements registered via the
/// active-set (not the full LifecycleComponent table).
pub(crate) fn process_frame_ticks(arena: &ElementArena) {
    use crate::ecs::active::{drain_active, register_active, ActiveTag};
    let active_eids: Vec<ElementId> = drain_active(ActiveTag::FrameTick).into_iter().collect();
    let (to_run, others): (Vec<_>, Vec<_>) = active_eids.into_iter().partition(|eid| {
        let Some(lc) = arena.component_tables.borrow().lc.get(eid).cloned() else {
            return false;
        };
        let Some(_tick) = lc.frame_tick.as_ref() else {
            return false;
        };
        if crate::core::dirty_registry::is_slot_inactive_in_ancestry(*eid, arena) {
            return false;
        }
        if crate::core::dirty_registry::is_reactive_hidden_in_ancestry(*eid) {
            return false;
        }
        true
    });
    let ticks: Vec<_> = to_run
        .iter()
        .filter_map(|&eid| {
            let lc = arena.component_tables.borrow().lc.get(&eid).cloned()?;
            let tick = lc.frame_tick.as_ref()?;
            Some((eid, tick.clone()))
        })
        .collect();
    for (eid, tick) in ticks {
        if let Some(f) = tick.borrow_mut().as_mut() {
            f();
        }
        // Re-register if the tick callback is still installed (it may have been
        // cleared during the callback, e.g. one-shot end-of-animation cleanup).
        let still_installed = arena
            .component_tables
            .borrow()
            .lc
            .get(&eid)
            .and_then(|lc| lc.frame_tick.as_ref())
            .is_some();
        if still_installed {
            register_active(eid, ActiveTag::FrameTick);
        }
    }
    // Re-register filtered-out elements so they can fire when they become
    // visible again (e.g. toast container hidden between toasts).
    for eid in others {
        let still_installed = arena
            .component_tables
            .borrow()
            .lc
            .get(&eid)
            .and_then(|lc| lc.frame_tick.as_ref())
            .is_some();
        if still_installed {
            register_active(eid, ActiveTag::FrameTick);
        }
    }
}
