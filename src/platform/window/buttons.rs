// ═══════════════════════ WindowButtons ═══════════════════════

/// Which titlebar buttons are enabled (Close, Minimize, Maximize).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct WindowButtons(winit::window::WindowButtons);

impl WindowButtons {
    pub const CLOSE: Self = Self(winit::window::WindowButtons::CLOSE);
    pub const MINIMIZE: Self = Self(winit::window::WindowButtons::MINIMIZE);
    pub const MAXIMIZE: Self = Self(winit::window::WindowButtons::MAXIMIZE);
    pub const ALL: Self = Self(winit::window::WindowButtons::all());

    pub fn contains(self, other: Self) -> bool {
        self.0.contains(other.0)
    }
    pub fn with(mut self, other: Self) -> Self {
        self.0 |= other.0;
        self
    }
    pub fn without(mut self, other: Self) -> Self {
        self.0 -= other.0;
        self
    }

    pub(crate) fn inner(self) -> winit::window::WindowButtons {
        self.0
    }
}

impl Default for WindowButtons {
    fn default() -> Self {
        Self::ALL
    }
}
