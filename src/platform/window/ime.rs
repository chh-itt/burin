use super::WindowState;
use crate::core::dirty_registry;
use crate::core::ElementId;
use crate::style;
use std::sync::Arc;

/// Per-frame IME caret-area sync (IME area dedup P1).
/// O(1) — reads the focused element's cached cursor rect from ECS.
pub(crate) fn sync_ime_cursor_area(state: &mut WindowState) {
    let Some(ref w) = state.winit_window else {
        return;
    };
    let Some(fid) = state.focus_manager.focused() else {
        return;
    };
    if !state.event_registry.has_text_input(fid) || state.event_registry.is_ime_suppressed(fid) {
        return;
    }
    let bounds = dirty_registry::bounds_of(fid).unwrap_or(style::Rect::ZERO);
    let sf = state.scale_factor as f32;
    let bounds_logical = style::Rect::new(
        bounds.x / sf,
        bounds.y / sf,
        bounds.width / sf,
        bounds.height / sf,
    );
    let (local, _) = crate::core::element::with_ct(|ct| {
        let local = ct.cursor.get(&fid).and_then(|c| c.ime_cursor_rect.get());
        (local, ())
    });
    // Ancestor-accumulated scroll (not the element's own — TextInput may
    // sit inside a ScrollView).  O(1) via generation cache.
    let asc_scroll = dirty_registry::accumulated_scroll_cached(&state.arena, fid);
    let area = crate::platform::ime::compose_ime_surface_rect(bounds_logical, local, asc_scroll);

    // Dedup: skip when the caret hasn't moved beyond 0.5 logical px.
    if let Some(last) = state.last_sent_ime_area {
        if (area.x - last.x).abs() < 0.5 && (area.y - last.y).abs() < 0.5 {
            return;
        }
    }

    let _ = w.request_ime_update(winit::window::ImeRequest::Update(
        winit::window::ImeRequestData::default().with_cursor_area(
            winit::dpi::Position::Logical(winit::dpi::LogicalPosition::new(
                f64::from(area.x),
                f64::from(area.y),
            )),
            winit::dpi::Size::Logical(winit::dpi::LogicalSize::new(
                f64::from(area.width.max(1.0)),
                f64::from(area.height.max(1.0)),
            )),
        ),
    ));
    state.last_sent_ime_area = Some(area);
}

pub(crate) fn request_ime_enable(
    w: &Arc<dyn winit::window::Window>,
    registry: &crate::event::EventRegistry,
    id: ElementId,
) -> bool {
    if registry.has_text_input(id) && !registry.is_ime_suppressed(id) {
        let sf = w.scale_factor() as f32;
        let bounds = dirty_registry::bounds_of(id).unwrap_or(style::Rect::ZERO);
        let bounds_logical = style::Rect::new(
            bounds.x / sf,
            bounds.y / sf,
            bounds.width / sf,
            bounds.height / sf,
        );
        let (local, _) = crate::core::element::with_ct(|ct| {
            let local = ct.cursor.get(&id).and_then(|c| c.ime_cursor_rect.get());
            (local, ())
        });
        // Note: enable fires during focus-transfer, before the first frame
        // layout — bounds may be stale and ancestor scroll unavailable here.
        // sync_ime_cursor_area corrects the position within one frame.
        let area =
            crate::platform::ime::compose_ime_surface_rect(bounds_logical, local, (0.0, 0.0));
        let _ = w.request_ime_update(winit::window::ImeRequest::Enable(
            winit::window::ImeEnableRequest::new(
                winit::window::ImeCapabilities::new()
                    .with_cursor_area()
                    .with_hint_and_purpose(),
                winit::window::ImeRequestData::default()
                    .with_hint_and_purpose(
                        winit::window::ImeHint::NONE,
                        winit::window::ImePurpose::Normal,
                    )
                    .with_cursor_area(
                        winit::dpi::Position::Logical(winit::dpi::LogicalPosition::new(
                            f64::from(area.x),
                            f64::from(area.y),
                        )),
                        winit::dpi::Size::Logical(winit::dpi::LogicalSize::new(
                            f64::from(area.width.max(1.0)),
                            f64::from(area.height.max(1.0)),
                        )),
                    ),
            )
            .unwrap(),
        ));
        true
    } else {
        let _ = w.request_ime_update(winit::window::ImeRequest::Disable);
        false
    }
}
