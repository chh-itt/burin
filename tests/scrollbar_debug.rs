//! Diagnostic tests: print column positions for Table.
use auralis_signal::Signal;
use burin::core::ElementId;
use burin::testing::TestHarness;
use burin::widgets::display::{ColumnWidth, Table, TableColumn};
use burin::widgets::layout::SizedBox;

fn dump_element(h: &TestHarness, eid: ElementId, indent: &str) {
    let el = h.find(eid);
    if el.is_none() {
        return;
    }
    let el = el.unwrap();
    let bounds = el.screen_bounds;
    let role = el
        .accessible_role()
        .map(|r| format!("{r:?}"))
        .unwrap_or_default();
    let label = el.accessible_label().unwrap_or_default();

    let extra = if !label.is_empty() {
        format!(" \"{label}\"")
    } else {
        String::new()
    };

    eprintln!(
        "{indent}{eid:?} {role} ({:.1},{:.1},{:.1}x{:.1}){extra}",
        bounds.x, bounds.y, bounds.width, bounds.height
    );

    for &cid in &el.children {
        dump_element(h, cid, &format!("{indent}  "));
    }
}

#[derive(Clone, Debug)]
struct Row {
    name: String,
    age: u32,
    city: String,
}

#[test]
fn debug_table_flex_columns() {
    let rows_sig = Signal::new(vec![
        Row {
            name: "Alice".into(),
            age: 25,
            city: "New York City, USA".into(),
        },
        Row {
            name: "Bob".into(),
            age: 30,
            city: "San Francisco, CA".into(),
        },
        Row {
            name: "Charlie".into(),
            age: 35,
            city: "Austin, Texas".into(),
        },
    ]);

    let mut harness = TestHarness::new(600.0, 400.0);

    let table = Table::new(rows_sig.clone())
        .columns(vec![
            TableColumn::new("Name", ColumnWidth::Fixed(120.0))
                .render(|r: &Row, _, _| r.name.clone()),
            TableColumn::new("Age", ColumnWidth::Fixed(60.0))
                .render(|r: &Row, _, _| r.age.to_string()),
            TableColumn::new("City", ColumnWidth::Flex(1.0)).render(|r: &Row, _, _| r.city.clone()),
        ])
        .row_height(36.0);

    let root_id = harness.mount(SizedBox::new().width(600.0).height(400.0).child(table));
    harness.run_frame();

    let table_container = harness.find(root_id).unwrap().children[0];
    eprintln!("===== FLEX TABLE (600x400) =====");
    dump_element(&harness, table_container, "");
}

#[test]
fn debug_gallery_table_columns() {
    let rows = Signal::new(
        (0..20)
            .map(|i| format!("Row {}", i + 1))
            .collect::<Vec<_>>(),
    );
    let selected = Signal::new(None::<usize>);
    let multi_selected = Signal::new(std::collections::HashSet::new());
    let disabled_set = Signal::new(
        [3usize, 6, 9, 14]
            .iter()
            .copied()
            .collect::<std::collections::HashSet<usize>>(),
    );
    let footer_txt = Signal::new(vec![
        "Total".to_string(),
        "—".to_string(),
        "—".to_string(),
        "—".to_string(),
    ]);

    let mut harness = TestHarness::new(800.0, 600.0);

    let table = Table::new(rows.clone())
        .columns(vec![
            TableColumn::new("Name", ColumnWidth::Fixed(120.0))
                .render(|r: &String, _, _| r.clone()),
            TableColumn::new("Value", ColumnWidth::Fixed(80.0))
                .render(|_: &String, ri, _| format!("{:.1}", ri as f32 * 10.0)),
            TableColumn::new("Category", ColumnWidth::Fixed(100.0))
                .render(|_: &String, ri, _| {
                    match ri % 3 {
                        0 => "Alpha",
                        1 => "Beta",
                        _ => "Gamma",
                    }
                    .to_string()
                })
                .resizable(),
            TableColumn::new("Notes", ColumnWidth::Flex(1.0))
                .render(|_: &String, ri, _| format!("Description for row {}", ri + 1))
                .resizable()
                .min_width(80.0),
        ])
        .selection_signal(selected.clone())
        .multi_select(multi_selected.clone())
        .row_height(28.0)
        .striped(true)
        .disabled_rows(disabled_set)
        .footer(footer_txt.clone());

    let root_id = harness.mount(SizedBox::new().width(500.0).height(300.0).child(table));
    harness.run_frame();

    let table_container = harness.find(root_id).unwrap().children[0];
    eprintln!("===== GALLERY TABLE (500x300, all-Fixed columns) =====");
    dump_element(&harness, table_container, "");
}
