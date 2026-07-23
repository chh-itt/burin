//! Virtual-mode coordinate-space regression tests (audit 2026-07-16,
//! round 6 — List/Tree/Table same-family sweep after the Table header
//! select-all OOB).
//!
//! Family: pool-slot-sized caches (`disabled_cells`, `SelectionBg.ids`)
//! indexed with DATA indices. In non-virtual widgets pool == data length
//! so it happened to work; virtualization splits the domains:
//! - keyboard row_nav / type-ahead skip closures → OOB panic past the pool
//! - shift-range selection filters → OOB panic
//! - CHECKED/DISABLED visuals written per data index landed on the wrong
//!   slot and stuck to it while scrolling (no remap re-derivation)

use std::collections::HashSet;

use auralis_signal::Signal;
use burin::core::config::StateFlags;
use burin::event::{Key, Modifiers};
use burin::testing::selector::by_text;
use burin::testing::TestHarness;
use burin::widgets::display::{ColumnWidth, List, Table, TableColumn};
use burin::widgets::layout::SizedBox;

fn arrow_down(h: &mut TestHarness, times: usize) {
    for _ in 0..times {
        h.press_key(Key::ArrowDown, Modifiers::NONE);
        h.run_frame();
    }
}

// ═══════════════ List ═══════════════

fn mount_virtual_list(
    h: &mut TestHarness,
    n: usize,
    selected: &Signal<Option<usize>>,
    disabled: Option<&Signal<HashSet<usize>>>,
) -> burin::core::ElementId {
    let items = Signal::new((0..n).map(|i| format!("Item {i:03}")).collect::<Vec<_>>());
    let mut l = List::new(items)
        .render(|s: &String, _| s.clone())
        .selected(selected.clone())
        // 200px items shrink the eagerly-built pool to ceil(4320/200)+2 = 24
        // slots so the tests actually cross the pool boundary.
        .item_height(200.0)
        .virtual_threshold(10)
        .item_focus_mode(burin::widgets::display::ItemFocusMode::RovingTabindex);
    if let Some(d) = disabled {
        l = l.disabled_items(d.clone());
    }
    let id = h.mount(SizedBox::new().width(300.0).height(240.0).child(l));
    for _ in 0..6 {
        h.run_frame();
    }
    id
}

/// Keyboard navigation past the pool must not index the pool-sized
/// disabled cache with data indices (was: OOB panic at index == pool).
#[test]
fn list_keyboard_nav_past_pool_does_not_panic() {
    let mut h = TestHarness::new(400.0, 300.0);
    let selected: Signal<Option<usize>> = Signal::new(None);
    let disabled: Signal<HashSet<usize>> = Signal::new([2usize].into_iter().collect());
    let sized = mount_virtual_list(&mut h, 200, &selected, Some(&disabled));

    let list_container = h.find(sized).unwrap().children[0];
    h.click(list_container);
    h.run_frame();

    // Walk well past the pool (24 slots at 200px). Old code panicked in
    // the skip closure at index == pool; safe_fire swallows the panic so
    // navigation silently STOPS at the pool edge — assert the exact final
    // position to catch both the crash and the stall.
    arrow_down(&mut h, 40);
    assert!(
        h.run_frame_safe().is_ok(),
        "nav past the pool must not panic"
    );
    assert_eq!(
        h.read_signal(&selected),
        Some(41),
        "40 ArrowDowns from row 0 skipping disabled row 2 land on 41"
    );
}

/// Type-ahead matching an item beyond the pool must not panic and must
/// land on the match (was: OOB on the pool-sized disabled cache).
#[test]
fn list_type_ahead_past_pool_does_not_panic() {
    let mut h = TestHarness::new(400.0, 300.0);
    let selected: Signal<Option<usize>> = Signal::new(None);
    let items = Signal::new(
        (0..260)
            .map(|i| {
                if i == 200 {
                    "zebra".to_string()
                } else {
                    format!("item {i:03}")
                }
            })
            .collect::<Vec<_>>(),
    );
    let l = List::new(items)
        .render(|s: &String, _| s.clone())
        .selected(selected.clone())
        .item_height(200.0)
        .virtual_threshold(10);
    let sized = h.mount(SizedBox::new().width(300.0).height(240.0).child(l));
    for _ in 0..6 {
        h.run_frame();
    }

    let list_container = h.find(sized).unwrap().children[0];
    h.click(list_container);
    h.run_frame();

    h.press_key(Key::Character("z".into()), Modifiers::NONE);
    assert!(
        h.run_frame_safe().is_ok(),
        "type-ahead past pool must not panic"
    );
    assert_eq!(
        h.read_signal(&selected),
        Some(200),
        "type-ahead lands on the match beyond the pool (List pool covers ~182 slots at 24px)"
    );
}

/// CHECKED must follow the virtual index across a scroll remap — the
/// slot that hosted the selected row shows a different row after
/// scrolling and must lose the highlight.
#[test]
fn list_selection_highlight_rederived_on_scroll() {
    let mut h = TestHarness::new(400.0, 300.0);
    let selected: Signal<Option<usize>> = Signal::new(None);
    let sized = mount_virtual_list(&mut h, 200, &selected, None);
    let list_container = h.find(sized).unwrap().children[0];

    let item0 = h.find_sel(by_text("Item 000")).expect("item 0 mounted");
    h.click(item0);
    h.run_frame();
    assert_eq!(h.read_signal(&selected), Some(0));
    assert!(
        h.find(item0)
            .unwrap()
            .state
            .get()
            .contains(StateFlags::CHECKED),
        "clicked row is CHECKED"
    );

    // Scroll far past the pool; slot 0 now hosts a different virtual row.
    h.scroll(list_container, 0.0, -(10.0 * 200.0));
    for _ in 0..4 {
        h.run_frame();
    }
    assert!(
        !h.find(item0)
            .unwrap()
            .state
            .get()
            .contains(StateFlags::CHECKED),
        "slot highlight must not travel with the pool while scrolling"
    );
}

// ═══════════════ Table ═══════════════

fn cols() -> Vec<TableColumn<String>> {
    vec![
        TableColumn::new("A", ColumnWidth::Fixed(160.0)).render(|r: &String, _, _| r.clone()),
        TableColumn::new("B", ColumnWidth::Fixed(100.0))
            .render(|_: &String, ri, _| format!("{ri}")),
    ]
}

/// Keyboard nav past the pool on a virtual table (was: OOB panic in the
/// row_nav skip closure at index == pool — second crash path of the
/// gallery report).
#[test]
fn table_keyboard_nav_past_pool_does_not_panic() {
    let mut h = TestHarness::new(600.0, 400.0);
    let rows = Signal::new((0..500).map(|i| format!("Row {i}")).collect::<Vec<_>>());
    let sel: Signal<Option<usize>> = Signal::new(None);
    let t = Table::new(rows)
        .columns(cols())
        .row_height(28.0)
        .virtual_threshold(10)
        .selection_signal(sel.clone());
    let _sized = h.mount(SizedBox::new().width(500.0).height(300.0).child(t));
    for _ in 0..6 {
        h.run_frame();
    }

    let first_cell = h.find_sel(by_text("Row 0")).expect("first row mounted");
    h.click(first_cell);
    h.run_frame();

    arrow_down(&mut h, 30);
    assert!(
        h.run_frame_safe().is_ok(),
        "table nav past the pool must not panic"
    );
    // Old code panicked in the skip closure at index == pool (safe_fire
    // swallowed it and navigation stalled at the pool edge). Activate the
    // focused row to observe the exact final position.
    h.press_key(Key::Space, Modifiers::NONE);
    h.run_frame();
    assert_eq!(
        h.read_signal(&sel),
        Some(30),
        "30 ArrowDowns from row 0 land on row 30 (past the ~12-slot pool)"
    );
}

/// Shift+ArrowDown range selection crossing the pool (was: OOB panic in
/// the range filter).
#[test]
fn table_shift_range_past_pool_does_not_panic() {
    let mut h = TestHarness::new(600.0, 400.0);
    let rows = Signal::new((0..500).map(|i| format!("Row {i}")).collect::<Vec<_>>());
    let multi: Signal<HashSet<usize>> = Signal::new(HashSet::new());
    let disabled: Signal<HashSet<usize>> = Signal::new([5usize, 30].into_iter().collect());
    let t = Table::new(rows)
        .columns(cols())
        .row_height(28.0)
        .virtual_threshold(10)
        .multi_select(multi.clone())
        .disabled_rows(disabled.clone());
    let _sized = h.mount(SizedBox::new().width(500.0).height(300.0).child(t));
    for _ in 0..6 {
        h.run_frame();
    }

    let first_cell = h.find_sel(by_text("Row 0")).expect("first row mounted");
    h.click(first_cell);
    h.run_frame();

    for _ in 0..35 {
        h.press_key(
            Key::ArrowDown,
            Modifiers {
                shift: true,
                ..Modifiers::NONE
            },
        );
        h.run_frame();
    }
    assert!(
        h.run_frame_safe().is_ok(),
        "shift range past the pool must not panic"
    );
    let sel = h.read_signal(&multi);
    assert!(
        sel.len() > 25,
        "range extended past the pool (got {})",
        sel.len()
    );
    assert!(
        !sel.contains(&5) && !sel.contains(&30),
        "disabled rows excluded from the range in data space"
    );
}
