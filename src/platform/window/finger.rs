use crate::core::ElementId;

#[derive(Clone, Default)]
pub(crate) struct FingerState {
    pub(crate) position: crate::style::Point,
    pub(crate) hovered_chain: Vec<ElementId>,
    pub(crate) pressed: Option<ElementId>,
}

pub(crate) fn finger_id_from_source(source: &winit::event::PointerSource) -> (u64, Option<u64>) {
    match source {
        winit::event::PointerSource::Touch { finger_id, .. } => {
            let id = finger_id.into_raw() as u64;
            (id, Some(id))
        }
        _ => (0, None),
    }
}

pub(crate) fn finger_id_from_button(button: &winit::event::ButtonSource) -> (u64, Option<u64>) {
    match button {
        winit::event::ButtonSource::Touch { finger_id, .. } => {
            let id = finger_id.into_raw() as u64;
            (id, Some(id))
        }
        _ => (0, None),
    }
}

pub(crate) fn finger_id_from_kind(kind: &winit::event::PointerKind) -> (u64, Option<u64>) {
    match kind {
        winit::event::PointerKind::Touch(fid) => {
            let id = fid.into_raw() as u64;
            (id, Some(id))
        }
        _ => (0, None),
    }
}
