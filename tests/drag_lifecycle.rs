use burin::style::Point;
use burin::testing::TestHarness;
use burin::widgets::display::Text;
use burin::widgets::layout::{SizedBox, SplitPane};

#[test]
fn harness_drag_helper_exists_and_runs() {
    let mut h = TestHarness::new(400.0, 200.0);
    let _root = h.mount(
        SizedBox::new()
            .width(400.0)
            .height(200.0)
            .child(SplitPane::new(Text::new("L"), Text::new("R"))),
    );
    h.run_frame();
    // Drag somewhere near the middle divider; must not panic.
    h.drag(Point::new(200.0, 100.0), Point::new(260.0, 100.0));
    h.run_frame();
}

use auralis_signal::Signal;
use burin::core::ElementId;
use burin::widgets::display::{ColumnWidth, Table, TableColumn};

#[test]
fn table_resize_grows_column_not_collapse() {
    let rows = Signal::new((0..5).map(|i| format!("Row {i}")).collect::<Vec<_>>());
    let mut h = TestHarness::new(600.0, 300.0);
    let t = h.mount(
        SizedBox::new().width(600.0).height(300.0).child(
            Table::new(rows.clone())
                .columns(vec![
                    TableColumn::new("A", ColumnWidth::Fixed(120.0))
                        .render(|r: &String, _, _| r.clone())
                        .resizable()
                        .min_width(40.0),
                    TableColumn::new("B", ColumnWidth::Fixed(120.0))
                        .render(|_: &String, ri, _| format!("{ri}")),
                ])
                .row_height(28.0),
        ),
    );
    h.run_frame();

    let cell = first_header_cell(&h, t);
    let cb = h
        .find(cell)
        .map(|el| el.screen_bounds)
        .expect("cell bounds");
    let press = Point::new(cb.x + cb.width - 2.0, cb.y + cb.height / 2.0);
    let before = cb.width;

    h.drag(press, Point::new(press.x + 60.0, press.y));
    h.run_frame();

    let after = h.find(cell).map(|el| el.screen_bounds.width).unwrap_or(0.0);
    assert!(
        after > before + 40.0,
        "column A should grow by ~60 (before={before}, after={after})"
    );
}

// Helper: structural traversal — the mounted SizedBox's first child is the Table
// container; container.children[0] is the header row; header_row.children[0] is hcell A.
fn first_header_cell(h: &TestHarness, mounted_root: ElementId) -> ElementId {
    let container = h
        .find(mounted_root)
        .and_then(|el| el.children.first().copied())
        .expect("table container");
    let header_row = h
        .find(container)
        .and_then(|el| el.children.first().copied())
        .expect("header row");
    h.find(header_row)
        .and_then(|el| el.children.first().copied())
        .expect("first header cell")
}

fn header_row_of(h: &TestHarness, mounted_root: ElementId) -> ElementId {
    let container = h
        .find(mounted_root)
        .and_then(|el| el.children.first().copied())
        .expect("table container");
    h.find(container)
        .and_then(|el| el.children.first().copied())
        .expect("header row")
}

// The resize affordance must be a thin handle element at the column boundary —
// NOT the whole "Category" header cell (which stays the sort-click zone).
#[test]
fn table_resize_uses_thin_handle_element() {
    let rows = Signal::new((0..5).map(|i| format!("Row {i}")).collect::<Vec<_>>());
    let mut h = TestHarness::new(600.0, 300.0);
    let t = h.mount(
        SizedBox::new().width(600.0).height(300.0).child(
            Table::new(rows.clone())
                .columns(vec![
                    TableColumn::new("Category", ColumnWidth::Fixed(120.0))
                        .render(|r: &String, _, _| r.clone())
                        .resizable()
                        .min_width(40.0),
                    TableColumn::new("Value", ColumnWidth::Fixed(120.0))
                        .render(|_: &String, ri, _| format!("{ri}")),
                ])
                .row_height(28.0),
        ),
    );
    h.run_frame();

    let header_row = header_row_of(&h, t);
    let hr_kids = h
        .find(header_row)
        .map(|el| el.children.clone())
        .expect("header children");
    // 2 header cells + 1 resize handle (boundary between col 0 and col 1).
    assert_eq!(
        hr_kids.len(),
        3,
        "header row should have 2 cells + 1 thin handle, got {}",
        hr_kids.len()
    );

    let cell0 = hr_kids[0];
    let handle = hr_kids[2];
    let c0 = h
        .find(cell0)
        .map(|el| el.screen_bounds)
        .expect("cell0 bounds");
    let hb = h
        .find(handle)
        .map(|el| el.screen_bounds)
        .expect("handle bounds");
    let boundary = c0.x + c0.width;

    assert!(
        hb.width < 10.0,
        "handle must be a thin bar, got width {}",
        hb.width
    );
    assert!(
        hb.width < c0.width * 0.5,
        "handle must be far thinner than the cell ({} vs {})",
        hb.width,
        c0.width
    );
    assert!(
        (hb.x + hb.width * 0.5 - boundary).abs() < 2.0,
        "handle must be centred on the column boundary (handle centre {}, boundary {})",
        hb.x + hb.width * 0.5,
        boundary,
    );
    assert!(
        (hb.height - 28.0).abs() < 2.0,
        "handle must span the header height, got {}",
        hb.height
    );
}

// Reproduce the real-window timing: drag events are interleaved with frames
// (each frame runs the deferred width update + relayout that repositions the
// handle). The harness `drag()` helper fires all updates synchronously without
// intervening frames, so it cannot surface frame-coupled bugs.
#[test]
fn table_first_rightward_drag_resizes() {
    let rows = Signal::new((0..5).map(|i| format!("Row {i}")).collect::<Vec<_>>());
    let mut h = TestHarness::new(600.0, 300.0);
    let t = h.mount(
        SizedBox::new().width(600.0).height(300.0).child(
            Table::new(rows.clone())
                .columns(vec![
                    TableColumn::new("Category", ColumnWidth::Fixed(120.0))
                        .render(|r: &String, _, _| r.clone())
                        .resizable()
                        .min_width(40.0),
                    TableColumn::new("Value", ColumnWidth::Fixed(120.0))
                        .render(|_: &String, ri, _| format!("{ri}")),
                ])
                .row_height(28.0),
        ),
    );
    h.run_frame();

    let header_row = header_row_of(&h, t);
    let cell0 = h
        .find(header_row)
        .and_then(|el| el.children.first().copied())
        .unwrap();
    let handle = h.find(header_row).map(|el| el.children[2]).unwrap();
    let before = h.find(cell0).unwrap().screen_bounds.width;

    let hb = h.find(handle).unwrap().screen_bounds;
    let press_x = hb.x + hb.width * 0.5;
    let py = hb.y + hb.height * 0.5;
    let dummy = Point::new(0.0, 0.0);

    // PointerDown → drag_start (captures anchor), then a frame.
    h.events_mut()
        .fire_drag_start(handle, dummy, Point::new(press_x, py));
    h.run_frame();
    // PointerMove RIGHT → drag_update, then the frame that applies + repositions.
    h.events_mut()
        .fire_drag_update(handle, dummy, Point::new(press_x + 30.0, py));
    h.run_frame();
    h.events_mut()
        .fire_drag_update(handle, dummy, Point::new(press_x + 60.0, py));
    h.run_frame();

    let after = h.find(cell0).unwrap().screen_bounds.width;
    assert!(
        after > before + 40.0,
        "first rightward drag (frame-interleaved) should grow column (before={before}, after={after})",
    );
}
