//! Table header select-all regression (audit 2026-07-16, round 6).
//!
//! The header checkbox click handler judged disabled-ness by indexing
//! `disabled_cells` — a POOL-slot-sized visual cache — with DATA row
//! indices. In virtual mode (pool ≪ data) this panicked on the 21st row:
//! "index out of bounds: the len is 20 but the index is 20"
//! (gallery "Virtual Table — 10,000 rows", header select-all click).
//!
//! Fix: disabled-ness is judged in data space via the `disabled_rows`
//! signal (the SSOT); Ctrl+A (ActionKind::SelectAll) aligned to the same
//! semantics (it previously ignored disabled rows entirely).

use std::collections::HashSet;

use auralis_signal::Signal;
use burin::testing::selector::by_text;
use burin::testing::TestHarness;
use burin::widgets::display::{ColumnWidth, Table, TableColumn};
use burin::widgets::layout::SizedBox;

fn cols() -> Vec<TableColumn<String>> {
    vec![
        TableColumn::new("A", ColumnWidth::Fixed(160.0)).render(|r: &String, _, _| r.clone()),
        TableColumn::new("B", ColumnWidth::Fixed(100.0))
            .render(|_: &String, ri, _| format!("{ri}")),
    ]
}

fn mount_virtual_table(
    h: &mut TestHarness,
    n_rows: usize,
    multi: &Signal<HashSet<usize>>,
    disabled: Option<&Signal<HashSet<usize>>>,
) -> burin::core::ElementId {
    let rows = Signal::new((0..n_rows).map(|i| format!("Row {i}")).collect::<Vec<_>>());
    let mut t = Table::new(rows)
        .columns(cols())
        .row_height(28.0)
        .virtual_threshold(10)
        .multi_select(multi.clone());
    if let Some(d) = disabled {
        t = t.disabled_rows(d.clone());
    }
    let id = h.mount(SizedBox::new().width(500.0).height(300.0).child(t));
    for _ in 0..6 {
        h.run_frame();
    }
    id
}

/// Locate the header select-all checkbox: the first ☐/☑ glyph in tree
/// order (the header cell is mounted before all body-row checkboxes).
fn header_checkbox(h: &TestHarness) -> burin::core::ElementId {
    let mut all = h.find_all_sel(by_text("\u{2610}"));
    all.extend(h.find_all_sel(by_text("\u{2611}")));
    *all.first().expect("header checkbox mounted")
}

/// The original panic: virtual table (pool 10–20 ≪ 500 rows), click the
/// header select-all. Must select every data row without indexing the
/// pool-sized cache out of bounds.
#[test]
fn header_select_all_does_not_panic_with_virtual_pool() {
    let mut h = TestHarness::new(600.0, 400.0);
    let multi: Signal<HashSet<usize>> = Signal::new(HashSet::new());
    mount_virtual_table(&mut h, 500, &multi, None);

    let hdr = header_checkbox(&h);
    h.click(hdr);
    assert!(
        h.run_frame_safe().is_ok(),
        "header select-all must not panic when data outgrows the pool"
    );
    assert_eq!(
        h.read_signal(&multi).len(),
        500,
        "all 500 data rows selected"
    );

    // Second click clears.
    h.click(hdr);
    h.run_frame();
    assert!(
        h.read_signal(&multi).is_empty(),
        "second click clears the selection"
    );
}

/// Disabled rows are skipped by header select-all — judged via the
/// `disabled_rows` signal in data space, including rows far past the pool.
#[test]
fn header_select_all_skips_disabled_rows_in_data_space() {
    let mut h = TestHarness::new(600.0, 400.0);
    let multi: Signal<HashSet<usize>> = Signal::new(HashSet::new());
    let disabled: Signal<HashSet<usize>> = Signal::new([3usize, 250, 499].into_iter().collect());
    mount_virtual_table(&mut h, 500, &multi, Some(&disabled));

    let hdr = header_checkbox(&h);
    h.click(hdr);
    h.run_frame();
    let sel = h.read_signal(&multi);
    assert_eq!(sel.len(), 497, "all except the 3 disabled rows");
    assert!(!sel.contains(&3) && !sel.contains(&250) && !sel.contains(&499));

    // With every enabled row selected, the next click must CLEAR
    // (the all-selected check also judges disabled in data space).
    h.click(hdr);
    h.run_frame();
    assert!(
        h.read_signal(&multi).is_empty(),
        "toggle clears despite disabled rows"
    );
}

/// Ctrl+A (ActionKind::SelectAll) now shares the header's semantics:
/// disabled rows are skipped instead of being silently selected.
#[test]
fn keyboard_select_all_matches_header_semantics() {
    use burin::event::{Key, Modifiers};

    let mut h = TestHarness::new(600.0, 400.0);
    let multi: Signal<HashSet<usize>> = Signal::new(HashSet::new());
    let disabled: Signal<HashSet<usize>> = Signal::new([7usize].into_iter().collect());
    let table = mount_virtual_table(&mut h, 100, &multi, Some(&disabled));

    // Focus a cell first (keyboard actions target the focused table).
    let first_cell = h
        .find_sel(by_text("Row 0"))
        .expect("first row cell mounted");
    h.click(first_cell);
    h.run_frame();

    h.press_key(
        Key::Character("a".into()),
        Modifiers {
            ctrl: true,
            ..Modifiers::NONE
        },
    );
    h.run_frame();
    let sel = h.read_signal(&multi);
    assert_eq!(sel.len(), 99, "Ctrl+A selects all enabled rows");
    assert!(
        !sel.contains(&7),
        "Ctrl+A skips disabled rows (header parity)"
    );
    let _ = table;
}
