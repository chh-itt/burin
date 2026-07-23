//! B0 re-architecture characterization + regression tests for `Table`.
//!
//! These tests pin the grid invariant that the B0 refactor must preserve:
//! every Body row (initial pool AND rows grown past the pool) carries exactly
//! `data_cols + checkbox` cells. The grow-path test starts RED (documents the
//! known checkbox bug) and must turn GREEN once the grow path routes through
//! `build_data_row`.

use std::collections::HashSet;

use auralis_signal::Signal;
use burin::core::ElementId;
use burin::style::Point;
use burin::testing::TestHarness;
use burin::widgets::display::{ColumnWidth, Table, TableColumn};
use burin::widgets::layout::SizedBox;

fn cols3() -> Vec<TableColumn<String>> {
    vec![
        TableColumn::new("A", ColumnWidth::Fixed(120.0)).render(|r: &String, _, _| r.clone()),
        TableColumn::new("B", ColumnWidth::Fixed(80.0)).render(|_: &String, ri, _| format!("{ri}")),
        TableColumn::new("C", ColumnWidth::Fixed(100.0))
            .render(|_: &String, ri, _| format!("c{ri}")),
    ]
}

const GRID_COLS: usize = 3 + 1; // 3 data columns + 1 checkbox column

/// Mount a checkbox table inside a fixed-size box, run one frame, and return
/// `(harness, table_container_id)`.
fn mount_cb(rows: Signal<Vec<String>>) -> (TestHarness, ElementId) {
    let multi: Signal<HashSet<usize>> = Signal::new(HashSet::new());
    let mut h = TestHarness::new(700.0, 400.0);
    let mounted = h.mount(
        SizedBox::new().width(700.0).height(400.0).child(
            Table::new(rows)
                .columns(cols3())
                .multi_select(multi)
                .row_height(28.0),
        ),
    );
    h.run_frame();
    let container = h.find(mounted).unwrap().children[0];
    (h, container)
}

/// DFS collect every descendant element id under `root`.
fn collect_descendants(h: &TestHarness, root: ElementId) -> Vec<ElementId> {
    let mut out = Vec::new();
    let mut stack = vec![root];
    while let Some(id) = stack.pop() {
        if let Some(el) = h.find(id) {
            for &c in &el.children {
                out.push(c);
                stack.push(c);
            }
        }
    }
    out
}

/// Cell counts of every `Role::Row` element under `container` that has children.
fn row_cell_counts(h: &TestHarness, container: ElementId) -> Vec<usize> {
    collect_descendants(h, container)
        .into_iter()
        .filter_map(|id| h.find(id))
        .filter(|el| el.accessible_role() == Some(accesskit::Role::Row))
        .map(|el| el.children.len())
        .filter(|&n| n > 0)
        .collect()
}

#[test]
fn initial_rows_have_checkbox_grid_count() {
    let rows = Signal::new((0..4).map(|i| format!("R{i}")).collect::<Vec<_>>());
    let (h, container) = mount_cb(rows);
    let counts = row_cell_counts(&h, container);
    assert!(
        counts.iter().any(|&n| n == GRID_COLS),
        "expected a row with {GRID_COLS} cells (3 data + checkbox); got {counts:?}",
    );
    assert!(
        !counts.iter().any(|&n| n == GRID_COLS - 1),
        "no row may be missing the checkbox cell; got {counts:?}",
    );
}

#[test]
fn grown_rows_have_checkbox_grid_count() {
    // initial_pool = max(2, 8) = 8 → growing to 12 forces the defer_action grow path.
    let rows = Signal::new((0..2).map(|i| format!("R{i}")).collect::<Vec<_>>());
    let (mut h, container) = mount_cb(rows.clone());

    rows.set((0..12).map(|i| format!("R{i}")).collect::<Vec<_>>());
    h.run_frames(3);

    let counts = row_cell_counts(&h, container);
    assert!(
        counts.iter().any(|&n| n == GRID_COLS),
        "expected rows with {GRID_COLS} cells; got {counts:?}",
    );
    // The teeth: a grown row missing its checkbox cell shows up as GRID_COLS-1.
    assert!(
        !counts.iter().any(|&n| n == GRID_COLS - 1),
        "REGRESSION: a grown row is missing the checkbox cell ({}); counts {counts:?}",
        GRID_COLS - 1,
    );
}

// The checkbox cell (accepts_mouse=true, a grid child of the row) must win the
// spatial hit-test over its mouse-accepting parent row, so clicks reach its own
// handler. (The end-to-end click→toggle is exercised by the propagation unit test
// `click_targets_deepest_handler_not_ancestor` and the gallery.)
#[test]
fn rows_change_keeps_checkbox_and_aligns_data() {
    // Phase-2 text update must write data into DATA cells (grid index = data+1
    // when a checkbox column exists), never into the checkbox cell at grid 0.
    let rows = Signal::new((0..3).map(|i| format!("R{i}")).collect::<Vec<_>>());
    let (mut h, container) = mount_cb(rows.clone());

    // Change the data — triggers the Phase-2 overlapping-region text update.
    rows.set((0..3).map(|i| format!("X{i}")).collect::<Vec<_>>());
    h.run_frame();

    let header_row = h.find(container).unwrap().children[0];
    let mut body_rows: Vec<ElementId> = collect_descendants(&h, container)
        .into_iter()
        .filter(|&id| {
            id != header_row
                && h.find(id).map_or(false, |el| {
                    el.accessible_role() == Some(accesskit::Role::Row)
                })
        })
        .collect();
    body_rows.sort_by_key(|&id| h.find(id).map(|el| el.tree_order).unwrap_or(0));
    let cells = h.find(body_rows[0]).unwrap().children.clone();
    assert!(cells.len() >= 2, "row must have checkbox + data cells");

    // children[1] = first DATA cell — must hold column A's value "X0".
    // With the indexing bug it would hold column B's render ("0").
    h.assert_text(cells[1], "X0");

    // children[0] = checkbox cell — must NOT have been overwritten with row data.
    let cb_label = h
        .find(cells[0])
        .unwrap()
        .accessible_label()
        .unwrap_or_default();
    assert_ne!(
        cb_label, "X0",
        "checkbox cell must not be overwritten by column-A data"
    );
}

#[test]
fn shift_arrow_extends_multi_selection_range() {
    use burin::event::action::{Action, ActionKind};
    use burin::event::FocusReason;

    let multi: Signal<HashSet<usize>> = Signal::new(HashSet::new());
    let rows = Signal::new((0..5).map(|i| format!("R{i}")).collect::<Vec<_>>());
    let mut h = TestHarness::new(700.0, 400.0);
    let mounted = h.mount(
        SizedBox::new().width(700.0).height(400.0).child(
            Table::new(rows)
                .columns(cols3())
                .multi_select(multi.clone())
                .row_height(28.0),
        ),
    );
    h.run_frame();
    let container = h.find(mounted).unwrap().children[0];

    // Focus the table (focused_cell starts at row 0).
    h.events_mut()
        .fire_focus_in(container, FocusReason::TabNavigation);
    h.run_frame();

    // Shift+ArrowDown twice → contiguous range [0..=2] selected.
    let shift_down = Action::new(ActionKind::MoveDown).with_selection();
    h.events_mut().fire_action(container, &shift_down);
    h.events_mut().fire_action(container, &shift_down);
    h.run_frame();

    let sel = multi.read();
    assert!(
        sel.contains(&0) && sel.contains(&1) && sel.contains(&2),
        "shift+down x2 from row 0 should range-select rows 0,1,2; got {:?}",
        sel,
    );
    assert!(
        !sel.contains(&3) && !sel.contains(&4),
        "range must stop at the focused row; got {:?}",
        sel,
    );
}

#[test]
fn checkbox_cell_wins_spatial_hit_test() {
    use burin::core::dirty_registry as dr;
    let multi = Signal::new(std::collections::HashSet::<usize>::new());
    let rows = Signal::new((0..4).map(|i| format!("R{i}")).collect::<Vec<_>>());
    let mut h = TestHarness::new(700.0, 400.0);
    let mounted = h.mount(
        SizedBox::new().width(700.0).height(400.0).child(
            Table::new(rows)
                .columns(cols3())
                .multi_select(multi.clone())
                .row_height(28.0),
        ),
    );
    h.run_frame();
    let container = h.find(mounted).unwrap().children[0];

    // Prime the spatial grid like window.rs does after taffy.
    let mut all = collect_descendants(&h, container);
    all.push(container);
    all.push(mounted);
    for id in &all {
        if let Some(el) = h.find(*id) {
            dr::register_bounds(*id, el.screen_bounds);
            dr::spatial_register(*id, el.screen_bounds, el.tree_order);
        }
    }

    let header_row = h.find(container).unwrap().children[0];
    let mut body_rows: Vec<ElementId> = collect_descendants(&h, container)
        .into_iter()
        .filter(|&id| {
            id != header_row
                && h.find(id).map_or(false, |el| {
                    el.accessible_role() == Some(accesskit::Role::Row)
                })
        })
        .collect();
    body_rows.sort_by_key(|&id| h.find(id).map(|el| el.tree_order).unwrap_or(0));
    let row0 = body_rows[0];
    let checkbox_cell = h.find(row0).unwrap().children[0];
    let cb = h.find(checkbox_cell).unwrap().screen_bounds;
    let center = Point::new(cb.x + cb.width * 0.5, cb.y + cb.height * 0.5);

    let spatial = dr::spatial_hit_test(&h.arena, center);
    assert_eq!(
        spatial,
        Some(checkbox_cell),
        "spatial hit at the checkbox centre must resolve to the checkbox cell, not the row \
         (spatial={spatial:?}, checkbox={checkbox_cell:?}, row={row0:?})",
    );
}

// End-to-end regression for #1 ("checkbox can't be checked"): clicking the
// checkbox cell at its position toggles multi_selection. Exercises the real
// chain: hit-test -> propagate_click (target-first) -> checkbox on_click.
// Relies on the now-faithful harness click_at (routes through propagate_click).
#[test]
fn page_up_down_navigation_with_multi_select() {
    use burin::event::action::{Action, ActionKind};
    use burin::event::FocusReason;

    let multi: Signal<HashSet<usize>> = Signal::new(HashSet::new());
    let rows_sig = Signal::new((0..30).map(|i| format!("Row {i}")).collect::<Vec<_>>());
    let mut h = TestHarness::new(700.0, 400.0);
    let mounted = h.mount(
        SizedBox::new().width(700.0).height(400.0).child(
            Table::new(rows_sig)
                .columns(cols3())
                .multi_select(multi.clone())
                .row_height(28.0),
        ),
    );
    h.run_frame();
    let container = h.find(mounted).unwrap().children[0];

    h.events_mut()
        .fire_focus_in(container, FocusReason::TabNavigation);
    h.run_frame();

    // Activate toggles row 0 in multi-select
    h.events_mut()
        .fire_action(container, &Action::new(ActionKind::Activate));
    h.run_frame();
    assert!(
        multi.read().contains(&0),
        "Activate should toggle row 0 in multi-select"
    );

    // PageDown (no shift) moves focus but does not change selection
    h.events_mut()
        .fire_action(container, &Action::new(ActionKind::MovePageDown));
    h.run_frame();
    let sel = multi.read();
    assert_eq!(
        sel.len(),
        1,
        "PageDown must not change multi-select; got {sel:?}"
    );
    assert!(
        sel.contains(&0),
        "PageDown must not remove existing selection"
    );
}

#[test]
fn page_up_down_moves_focus_without_selecting() {
    use burin::event::action::{Action, ActionKind};
    use burin::event::FocusReason;

    let rows_sig = Signal::new((0..30).map(|i| format!("Row {i}")).collect::<Vec<_>>());
    let sel: Signal<Option<usize>> = Signal::new(None);

    let mut h = TestHarness::new(700.0, 400.0);
    let mounted = h.mount(
        SizedBox::new().width(700.0).height(400.0).child(
            Table::new(rows_sig)
                .columns(cols3())
                .selection_signal(sel.clone())
                .row_height(28.0),
        ),
    );
    h.run_frame();
    let container = h.find(mounted).unwrap().children[0];

    h.events_mut()
        .fire_focus_in(container, FocusReason::TabNavigation);
    h.run_frame();

    // Initial: no selection
    assert_eq!(h.read_signal(&sel), None, "initial selection must be None");

    // Enter → select row 0 (focused_cell starts at row 0)
    h.events_mut()
        .fire_action(container, &Action::new(ActionKind::Activate));
    h.run_frame();
    assert_eq!(h.read_signal(&sel), Some(0));

    // PageDown → focus moves to ~14, single-selection unchanged
    h.events_mut()
        .fire_action(container, &Action::new(ActionKind::MovePageDown));
    h.run_frame();
    assert_eq!(
        h.read_signal(&sel),
        Some(0),
        "PageDown must not change single-selection"
    );

    // PageUp → focus moves back, single-selection unchanged
    h.events_mut()
        .fire_action(container, &Action::new(ActionKind::MovePageUp));
    h.run_frame();
    assert_eq!(
        h.read_signal(&sel),
        Some(0),
        "PageUp must not change single-selection"
    );
}

#[test]
fn clicking_checkbox_cell_toggles_multi_selection() {
    use burin::core::dirty_registry as dr;
    let multi = Signal::new(std::collections::HashSet::<usize>::new());
    let rows = Signal::new((0..4).map(|i| format!("R{i}")).collect::<Vec<_>>());
    let mut h = TestHarness::new(700.0, 400.0);
    let mounted = h.mount(
        SizedBox::new().width(700.0).height(400.0).child(
            Table::new(rows)
                .columns(cols3())
                .multi_select(multi.clone())
                .row_height(28.0),
        ),
    );
    h.run_frame();
    let container = h.find(mounted).unwrap().children[0];

    // Prime the spatial grid (window.rs does this after taffy).
    let mut all = collect_descendants(&h, container);
    all.push(container);
    for id in &all {
        if let Some(el) = h.find(*id) {
            dr::register_bounds(*id, el.screen_bounds);
            dr::spatial_register(*id, el.screen_bounds, el.tree_order);
        }
    }

    let header_row = h.find(container).unwrap().children[0];
    let mut body_rows: Vec<ElementId> = collect_descendants(&h, container)
        .into_iter()
        .filter(|&id| {
            id != header_row
                && h.find(id).map_or(false, |el| {
                    el.accessible_role() == Some(accesskit::Role::Row)
                })
        })
        .collect();
    body_rows.sort_by_key(|&id| h.find(id).map(|el| el.tree_order).unwrap_or(0));
    let checkbox_cell = h.find(body_rows[0]).unwrap().children[0];
    let cb = h.find(checkbox_cell).unwrap().screen_bounds;
    let center = Point::new(cb.x + cb.width * 0.5, cb.y + cb.height * 0.5);

    assert!(multi.read().is_empty(), "precondition: nothing selected");
    h.click_at(center);
    h.run_frame();
    assert!(
        !multi.read().is_empty(),
        "clicking the checkbox cell must toggle multi_selection (got {:?})",
        multi.read(),
    );
}
