//! W1 guards: unified WindowInsets (safe area + IME as ONE channel) and
//! the SafeArea widget (audit 2026-07-19, mobile-groundwork pass).
//!
//! Design: the software keyboard IS a dynamic inset. `WindowInsets`
//! carries a static member (safe_area: notch / rounded corners / custom
//! titlebar) and a dynamic member (ime), consumers read the per-edge max
//! via `effective()`. Desktop acceptance scenario: a custom-drawn
//! titlebar is "the desktop notch" (decorations=false).

use burin::platform::insets::{self, EdgeInsets, WindowInsets};
use burin::testing::TestHarness;
use burin::widgets::display::Text;
use burin::widgets::layout::SafeArea;

/// The model: effective() merges safe_area and ime per edge with max —
/// a keyboard covering the home-indicator does not stack on top of it.
#[test]
fn effective_insets_merge_per_edge_max() {
    let w = WindowInsets {
        safe_area: EdgeInsets {
            left: 0.0,
            top: 40.0,
            right: 0.0,
            bottom: 20.0,
        },
        ime: EdgeInsets {
            left: 0.0,
            top: 0.0,
            right: 0.0,
            bottom: 260.0,
        },
    };
    let eff = w.effective();
    assert_eq!(eff.top, 40.0, "safe-area top passes through");
    assert_eq!(
        eff.bottom, 260.0,
        "ime bottom COVERS the 20px home indicator (max, not sum)"
    );
}

/// SafeArea pads its content by the current effective insets.
#[test]
fn safe_area_pads_content_by_injected_insets() {
    let mut h = TestHarness::new(400.0, 300.0);
    insets::set_window_insets(WindowInsets {
        safe_area: EdgeInsets {
            left: 0.0,
            top: 40.0,
            right: 0.0,
            bottom: 0.0,
        },
        ime: EdgeInsets::ZERO,
    });
    let sa = h.mount(SafeArea::new(Text::new("under the notch")));
    h.run_frame();
    h.run_frame();

    let child = *h
        .find(sa)
        .unwrap()
        .children
        .first()
        .expect("safe-area child");
    let cb = h.find(child).unwrap().screen_bounds;
    assert!(
        cb.y >= 40.0,
        "content starts below the 40px top inset, got y={}",
        cb.y
    );
}

/// Changing insets at runtime (keyboard show) re-pads on the next frame.
#[test]
fn inset_change_applies_on_the_next_frame() {
    let mut h = TestHarness::new(400.0, 300.0);
    insets::set_window_insets(WindowInsets::ZERO);
    let sa = h.mount(SafeArea::new(Text::new("content")));
    h.run_frame();
    h.run_frame();
    let child = *h.find(sa).unwrap().children.first().unwrap();
    let y_before = h.find(child).unwrap().screen_bounds.y;
    assert!(y_before < 1.0, "no insets -> content at the top");

    insets::set_window_insets(WindowInsets {
        safe_area: EdgeInsets {
            left: 0.0,
            top: 32.0,
            right: 0.0,
            bottom: 0.0,
        },
        ime: EdgeInsets::ZERO,
    });
    h.run_frame();
    h.run_frame();
    let y_after = h.find(child).unwrap().screen_bounds.y;
    assert!(
        y_after >= 32.0,
        "custom-titlebar inset applied at runtime, got y={y_after}"
    );
}

/// Edge opt-out: a bottom bar wants side+bottom padding but NOT top.
#[test]
fn safe_area_edges_can_be_opted_out() {
    let mut h = TestHarness::new(400.0, 300.0);
    insets::set_window_insets(WindowInsets {
        safe_area: EdgeInsets {
            left: 10.0,
            top: 40.0,
            right: 10.0,
            bottom: 24.0,
        },
        ime: EdgeInsets::ZERO,
    });
    let sa = h.mount(SafeArea::new(Text::new("bottom bar")).top(false));
    h.run_frame();
    h.run_frame();

    let child = *h.find(sa).unwrap().children.first().unwrap();
    let cb = h.find(child).unwrap().screen_bounds;
    assert!(
        cb.y < 40.0,
        "top edge opted out — content not pushed down, y={}",
        cb.y
    );
    assert!(cb.x >= 10.0, "left inset still applies, x={}", cb.x);
}
