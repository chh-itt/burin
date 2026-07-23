//! Unified window insets: safe area (static) + IME keyboard (dynamic)
//! as ONE channel (mobile-groundwork W1, audit 2026-07-19).
//!
//! The software keyboard IS a dynamic inset — modeling it separately
//! from the notch/home-indicator would repeat the "three parallel
//! gesture systems" mistake. Consumers ([`SafeArea`](crate::widgets::layout::SafeArea),
//! keyboard avoidance) read the per-edge max via [`WindowInsets::effective`].
//!
//! ## Sources
//! - Mobile: the platform backend feeds `winit`'s `safe_area()` +
//!   keyboard notifications into [`set_window_insets`] (W3, Android).
//! - Desktop: a custom-drawn titlebar (decorations=false) injects its
//!   own bar height as `safe_area.top` — the desktop notch.
//! - Tests: inject any value, fully virtual.
//!
//! ## Change propagation
//! `set_window_insets` bumps a per-window generation and queues a
//! deferred root MEASURE. `SafeArea` reconciles its padding against the
//! generation in its frame_tick (Prepass), and an IME change requests a
//! focused-element scroll-into-view (consumed by `drive_frame_platform`,
//! which owns the FocusManager).

use std::cell::Cell;

/// Per-edge logical-pixel insets.
#[derive(Clone, Copy, PartialEq, Default, Debug)]
pub struct EdgeInsets {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}

impl EdgeInsets {
    pub const ZERO: Self = Self {
        left: 0.0,
        top: 0.0,
        right: 0.0,
        bottom: 0.0,
    };

    pub fn all(v: f32) -> Self {
        Self {
            left: v,
            top: v,
            right: v,
            bottom: v,
        }
    }
}

/// The window's inset state: static safe area + dynamic IME keyboard.
#[derive(Clone, Copy, PartialEq, Default, Debug)]
pub struct WindowInsets {
    /// Notch, rounded corners, home indicator, custom titlebar.
    pub safe_area: EdgeInsets,
    /// Software keyboard (usually bottom-only). Zero when hidden.
    pub ime: EdgeInsets,
}

impl WindowInsets {
    pub const ZERO: Self = Self {
        safe_area: EdgeInsets::ZERO,
        ime: EdgeInsets::ZERO,
    };

    /// Effective insets: per-edge MAX of safe area and IME. The keyboard
    /// covers the home indicator — it does not stack on top of it
    /// (Android/iOS convention).
    pub fn effective(&self) -> EdgeInsets {
        EdgeInsets {
            left: self.safe_area.left.max(self.ime.left),
            top: self.safe_area.top.max(self.ime.top),
            right: self.safe_area.right.max(self.ime.right),
            bottom: self.safe_area.bottom.max(self.ime.bottom),
        }
    }
}

/// Per-window insets state (AppContext extension domain).
#[derive(Default)]
struct InsetsDomain {
    insets: Cell<WindowInsets>,
    generation: Cell<u64>,
    /// Set when the IME inset changed; drained by drive_frame_platform
    /// to re-run scroll-into-view on the focused element.
    ime_refocus: Cell<bool>,
}

fn domain() -> std::rc::Rc<InsetsDomain> {
    crate::core::app_context::current_app().extension::<InsetsDomain>()
}

/// The current window's insets.
pub fn window_insets() -> WindowInsets {
    domain().insets.get()
}

/// Monotonic change counter — `SafeArea` reconciles against this.
pub fn insets_generation() -> u64 {
    domain().generation.get()
}

/// Install new insets for the current window. No-op when unchanged.
/// Queues a deferred root MEASURE (the deferred-action liveness gate
/// guarantees a frame), and flags an IME refocus when the keyboard
/// inset changed.
pub fn set_window_insets(new: WindowInsets) {
    let dom = domain();
    let old = dom.insets.get();
    if old == new {
        return;
    }
    dom.insets.set(new);
    dom.generation.set(dom.generation.get().wrapping_add(1));
    if old.ime != new.ime {
        dom.ime_refocus.set(true);
    }
    crate::core::dirty_registry::defer_action(move |arena, root, _| {
        if let Some(el) = arena.get(root) {
            el.dirty
                .set(el.dirty.get() | crate::core::element::DirtyFlags::MEASURE);
        }
        crate::core::dirty_registry::register_dirty(
            root,
            crate::core::element::DirtyFlags::MEASURE,
        );
        crate::core::dirty_registry::bump_subtree_gen(root);
    });
}

/// Drain the IME-change refocus request (drive_frame_platform).
pub(crate) fn take_ime_refocus() -> bool {
    domain().ime_refocus.replace(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn effective_is_per_edge_max() {
        let w = WindowInsets {
            safe_area: EdgeInsets {
                left: 5.0,
                top: 40.0,
                right: 0.0,
                bottom: 20.0,
            },
            ime: EdgeInsets {
                left: 0.0,
                top: 0.0,
                right: 8.0,
                bottom: 260.0,
            },
        };
        let e = w.effective();
        assert_eq!(e.left, 5.0);
        assert_eq!(e.top, 40.0);
        assert_eq!(e.right, 8.0);
        assert_eq!(e.bottom, 260.0);
    }
}
