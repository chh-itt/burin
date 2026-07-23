//! Portal owner-linkage regression tests (audit 2026-07-16, round 3, ①).
//!
//! Portals mount as ROOT children so they escape clipping — which means
//! removing the owner widget's subtree does not touch them. The owner
//! registry (`portal::register_portal_owner`) + teardown hook queues owned
//! portals for removal; the frame driver drains the queue (looping for
//! transitively-owned portals) and recursively tears them down.

use auralis_signal::Signal;
use burin::style::{Dimension, Styled};
use burin::testing::TestHarness;
use burin::widgets::input::Select;
use burin::widgets::layout::VStack;

fn px(w: f32) -> Dimension {
    Dimension::Pixels(w)
}

fn mount_select_page(
    h: &mut TestHarness,
    selected: &Signal<Option<&'static str>>,
) -> burin::core::ElementId {
    h.mount(
        VStack::new().width(px(400.0)).height(px(300.0)).push(
            Select::new(selected.clone())
                .options(vec!["Rust", "Go", "Python"])
                .render(|s: &&'static str| s.to_string())
                .placeholder("Choose..."),
        ),
    )
}

/// Removing a page containing a Select removes its portal element too.
#[test]
fn select_portal_removed_with_owner_subtree() {
    let mut h = TestHarness::new(400.0, 300.0);
    let selected: Signal<Option<&'static str>> = Signal::new(None);
    let baseline = h.arena.len();

    let page = mount_select_page(&mut h, &selected);
    h.run_frame();
    assert!(h.arena.len() > baseline, "page + portal mounted");

    h.arena.remove(page);
    // Frame 1 drains the owner-queued portal removal.
    h.run_frame();
    h.run_frame();

    assert_eq!(
        h.arena.len(),
        baseline,
        "portal subtree must be torn down with its owner ({} elements leaked)",
        h.arena.len() - baseline
    );
}

/// Same guarantee while the dropdown is OPEN (modal scope pushed): the
/// portal dies and the scope is cleaned without disturbing other scopes.
#[test]
fn open_select_portal_removed_and_scope_cleaned() {
    let mut h = TestHarness::new(400.0, 300.0);
    let selected: Signal<Option<&'static str>> = Signal::new(None);
    let baseline = h.arena.len();

    let page = mount_select_page(&mut h, &selected);
    h.run_frame();

    // Open the dropdown via its trigger.
    let select_id = h.find(page).unwrap().children[0];
    let trigger = h.find(select_id).unwrap().children[0];
    h.click(trigger);
    h.settle(5);
    assert!(
        burin::event::current_modal_scope_root().is_some(),
        "open dropdown pushes a modal scope"
    );

    h.arena.remove(page);
    h.run_frame();
    h.run_frame();

    assert_eq!(h.arena.len(), baseline, "open portal torn down with owner");
    assert_eq!(
        burin::event::current_modal_scope_root(),
        None,
        "owner teardown must clean the dropdown's modal scope"
    );
}

/// Unmounting a CLOSED dropdown must not pop an unrelated modal scope
/// (the old on_unmount blind-popped the stack top).
#[test]
fn closed_dropdown_unmount_preserves_foreign_scope() {
    let mut h = TestHarness::new(400.0, 300.0);
    let selected: Signal<Option<&'static str>> = Signal::new(None);

    let page = mount_select_page(&mut h, &selected);
    h.run_frame();

    // Simulate another overlay's active scope (owned by the root).
    let foreign_root = h.root_id();
    burin::event::push_modal_scope(foreign_root, burin::event::TraversalEdgeBehavior::Wrap);

    h.arena.remove(page);
    h.run_frame();
    h.run_frame();

    assert_eq!(
        burin::event::current_modal_scope_root(),
        Some(foreign_root),
        "removing a closed dropdown must not evict another overlay's scope"
    );
    burin::event::pop_modal_scope();
}

/// Re-mounting after teardown works (owner links don't go stale).
#[test]
fn remount_after_owner_teardown_is_clean() {
    let mut h = TestHarness::new(400.0, 300.0);
    let selected: Signal<Option<&'static str>> = Signal::new(None);
    let baseline = h.arena.len();

    for _ in 0..5 {
        let page = mount_select_page(&mut h, &selected);
        h.run_frame();
        h.arena.remove(page);
        h.run_frame();
        h.run_frame();
    }
    assert_eq!(
        h.arena.len(),
        baseline,
        "5 mount/remove cycles leak nothing"
    );
    assert_eq!(sig_subs(&selected), 0, "selected signal fully unsubscribed");
}

fn sig_subs<T: 'static>(s: &Signal<T>) -> usize {
    s.subscriber_count()
}
