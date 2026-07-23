//! Virtual scrolling tests for Table widget.

use auralis_signal::Signal;
use burin::core::ElementId;
use burin::testing::TestHarness;
use burin::widgets::display::{ColumnWidth, Table, TableColumn};
use burin::widgets::layout::SizedBox;
use std::collections::HashSet;

fn cols3() -> Vec<TableColumn<String>> {
    vec![
        TableColumn::new("A", ColumnWidth::Fixed(120.0)).render(|r: &String, _, _| r.clone()),
        TableColumn::new("B", ColumnWidth::Fixed(80.0)).render(|_: &String, ri, _| format!("{ri}")),
        TableColumn::new("C", ColumnWidth::Fixed(100.0))
            .render(|_: &String, ri, _| format!("c{ri}")),
    ]
}

/// Mount a virtual-scrolling table with many rows
fn mount_virtual(rows: Signal<Vec<String>>) -> (TestHarness, ElementId) {
    let mut h = TestHarness::new(700.0, 400.0);
    let mounted = h.mount(
        SizedBox::new().width(700.0).height(400.0).child(
            Table::new(rows)
                .columns(cols3())
                .row_height(28.0)
                .virtual_threshold(10),
        ),
    );
    h.run_frame();
    let container = h.find(mounted).unwrap().children[0];
    (h, container)
}

#[test]
fn virtual_table_creates_limited_pool() {
    let rows = Signal::new((0..100).map(|i| format!("Row {i}")).collect::<Vec<_>>());
    let (h, container) = mount_virtual(rows);

    // Count all Row elements under the table
    let mut row_count = 0;
    let mut stack = vec![container];
    while let Some(id) = stack.pop() {
        if let Some(el) = h.find(id) {
            if el.accessible_role() == Some(accesskit::Role::Row) {
                row_count += 1;
            }
            for &c in &el.children {
                stack.push(c);
            }
        }
    }
    // pool = min(10, 100) = 10 body rows + 1 header row = 11
    assert!(
        row_count < 20,
        "virtual table should create at most threshold+header rows, got {row_count}"
    );
    assert!(
        row_count >= 10,
        "should have at least pool_size rows, got {row_count}"
    );
}

#[test]
fn virtual_table_data_signal_updates_text() {
    let rows = Signal::new((0..50).map(|i| format!("Row {i}")).collect::<Vec<_>>());
    let (mut h, _container) = mount_virtual(rows);
    h.settle(3);
}

#[test]
fn virtual_table_does_not_panic_on_scroll() {
    let rows = Signal::new((0..200).map(|i| format!("Item {i}")).collect::<Vec<_>>());
    let (mut h, _container) = mount_virtual(rows);

    for _ in 0..10 {
        h.run_frame();
    }
}

#[test]
fn virtual_table_with_selection() {
    let sel: Signal<Option<usize>> = Signal::new(None);
    let rows = Signal::new((0..100).map(|i| format!("R{i}")).collect::<Vec<_>>());
    let mut h = TestHarness::new(700.0, 400.0);
    let _mounted = h.mount(
        SizedBox::new().width(700.0).height(400.0).child(
            Table::new(rows)
                .columns(cols3())
                .selection_signal(sel.clone())
                .row_height(28.0)
                .virtual_threshold(10),
        ),
    );
    h.run_frame();
    h.settle(3);

    assert_eq!(h.read_signal(&sel), None);
}

#[test]
fn virtual_table_with_multi_select() {
    let multi: Signal<HashSet<usize>> = Signal::new(HashSet::new());
    let rows = Signal::new((0..80).map(|i| format!("R{i}")).collect::<Vec<_>>());
    let mut h = TestHarness::new(700.0, 400.0);
    let _mounted = h.mount(
        SizedBox::new().width(700.0).height(400.0).child(
            Table::new(rows)
                .columns(cols3())
                .multi_select(multi.clone())
                .row_height(28.0)
                .virtual_threshold(10),
        ),
    );
    h.run_frame();
    h.settle(3);

    assert!(h.read_signal(&multi).is_empty());
}

#[test]
fn virtual_table_non_virtual_still_works() {
    // Small table (below threshold) should still work normally
    let rows = Signal::new((0..5).map(|i| format!("R{i}")).collect::<Vec<_>>());
    let mut h = TestHarness::new(700.0, 400.0);
    let _mounted = h.mount(
        SizedBox::new().width(700.0).height(400.0).child(
            Table::new(rows)
                .columns(cols3())
                .row_height(28.0)
                .virtual_threshold(10),
        ),
    );
    h.run_frame();
}
