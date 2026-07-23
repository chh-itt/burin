//! System cursor icons. Newtype wrapper around winit's CursorIcon
//! to keep winit out of the public API.

/// A system cursor icon.
///
/// Use the associated constants (e.g. `CursorIcon::POINTER`), or
/// [`CursorIcon::from_raw`] for a rare variant not listed here.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct CursorIcon(winit::cursor::CursorIcon);

impl CursorIcon {
    pub const DEFAULT: Self = Self(winit::cursor::CursorIcon::Default);
    pub const POINTER: Self = Self(winit::cursor::CursorIcon::Pointer);
    pub const TEXT: Self = Self(winit::cursor::CursorIcon::Text);
    pub const CROSSHAIR: Self = Self(winit::cursor::CursorIcon::Crosshair);
    pub const GRAB: Self = Self(winit::cursor::CursorIcon::Grab);
    pub const GRABBING: Self = Self(winit::cursor::CursorIcon::Grabbing);
    pub const NOT_ALLOWED: Self = Self(winit::cursor::CursorIcon::NotAllowed);
    pub const WAIT: Self = Self(winit::cursor::CursorIcon::Wait);
    pub const PROGRESS: Self = Self(winit::cursor::CursorIcon::Progress);
    pub const HELP: Self = Self(winit::cursor::CursorIcon::Help);
    pub const MOVE: Self = Self(winit::cursor::CursorIcon::Move);
    pub const EW_RESIZE: Self = Self(winit::cursor::CursorIcon::EwResize);
    pub const NS_RESIZE: Self = Self(winit::cursor::CursorIcon::NsResize);
    pub const NESW_RESIZE: Self = Self(winit::cursor::CursorIcon::NeswResize);
    pub const NWSE_RESIZE: Self = Self(winit::cursor::CursorIcon::NwseResize);
    pub const COL_RESIZE: Self = Self(winit::cursor::CursorIcon::ColResize);
    pub const ROW_RESIZE: Self = Self(winit::cursor::CursorIcon::RowResize);
    pub const ZOOM_IN: Self = Self(winit::cursor::CursorIcon::ZoomIn);
    pub const ZOOM_OUT: Self = Self(winit::cursor::CursorIcon::ZoomOut);

    /// Construct from a raw `winit::cursor::CursorIcon`.
    /// Useful for variants not covered by the above constants
    /// (`ContextMenu`, `Cell`, `VerticalText`, `Alias`, `Copy`,
    /// `NoDrop`, `AllScroll`, `DndAsk`, `AllResize`, etc.).
    pub fn from_raw(raw: winit::cursor::CursorIcon) -> Self {
        Self(raw)
    }

    pub(crate) fn inner(self) -> winit::cursor::CursorIcon {
        self.0
    }
}

impl std::fmt::Debug for CursorIcon {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "CursorIcon({:?})", self.0)
    }
}

impl Default for CursorIcon {
    fn default() -> Self {
        Self::DEFAULT
    }
}
