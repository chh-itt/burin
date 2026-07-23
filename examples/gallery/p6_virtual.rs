//! Virtual scrolling demo — 10,000-row Table and 5,000-item List.
//!
//! This page exists so the VIRTUAL code path gets human eyes: the classic
//! table/list demos hold ~20 rows, which sits at the default virtualization
//! threshold and never exercises the pooled path. Every historical
//! virtual-scroll bug (rows hit-test-dead after one pool height, stale
//! checkbox glyphs, stripe inversion, pool smaller than the viewport)
//! survived because nothing in the gallery ever scrolled a real pool.
//!
//! What to verify by hand:
//! - Scroll deep (drag the scrollbar to the middle): rows must show the
//!   right numbers, stripes must alternate, no blank bands.
//! - Click rows / checkboxes after scrolling: selection must hit the row
//!   you clicked; checked marks must stay on their data rows while scrolling.
//! - The footer row must sit at the very end of the 10k rows.

use auralis_signal::Signal;
use burin::core::{Compositor, Widget};
use burin::style::Styled;
use burin::widgets::display::{ColumnWidth, List, Table, TableColumn, Text};
use burin::widgets::layout::{HStack, SizedBox, VStack};

use super::{section_sub, section_title};

pub fn virtual_table_section() -> impl Widget {
    Compositor::new(|_scope| {
        let rows = Signal::new(
            (0..10_000)
                .map(|i| format!("Data row #{i}"))
                .collect::<Vec<_>>(),
        );
        let selected: Signal<Option<usize>> = Signal::new(None);
        let multi: Signal<std::collections::HashSet<usize>> =
            Signal::new([1usize, 3, 4990, 5000, 9998].into_iter().collect());
        let sel_display = Signal::new("None".to_string());

        let sel_txt = sel_display.clone();
        VStack::new()
            .gap(8.0)
            .push(section_title("Virtual Table — 10,000 rows"))
            .push(section_sub(
                "20-slot pool recycled in a ring. Pre-checked: #1 #3 #4990 #5000 #9998 — \
                 scroll to them and verify the marks sit on the right rows.",
            ))
            .push(
                SizedBox::new().width(680.0).height(360.0).child(
                    Table::new(rows)
                        .columns(vec![
                            TableColumn::new("Row", ColumnWidth::Fixed(160.0))
                                .render(|r: &String, _, _| r.clone()),
                            TableColumn::new("Index²", ColumnWidth::Fixed(120.0))
                                .render(|_: &String, ri, _| format!("{}", ri * ri)),
                            TableColumn::new("Bucket", ColumnWidth::Fixed(100.0)).render(
                                |_: &String, ri, _| {
                                    match ri % 4 {
                                        0 => "North",
                                        1 => "East",
                                        2 => "South",
                                        _ => "West",
                                    }
                                    .to_string()
                                },
                            ),
                            TableColumn::new("Notes", ColumnWidth::Flex(1.0))
                                .render(|_: &String, ri, _| format!("virtual row {ri}")),
                        ])
                        .row_height(28.0)
                        .striped(true)
                        .multi_select(multi.clone())
                        .selection_signal(selected.clone())
                        .on_select(move |ri| sel_txt.set(format!("Row {ri}")))
                        .footer(Signal::new(vec![
                            "Σ 10,000 rows".to_string(),
                            String::new(),
                            String::new(),
                            "footer pinned to data end".to_string(),
                        ])),
                ),
            )
            .push(
                HStack::new()
                    .gap(8.0)
                    .push(Text::new("Selected:"))
                    .push(Text::new(String::new()).bind(sel_display)),
            )
    })
}

pub fn virtual_list_section() -> impl Widget {
    Compositor::new(|_scope| {
        let items = Signal::new(
            (0..5_000)
                .map(|i| format!("List entry {i} — the quick brown fox"))
                .collect::<Vec<_>>(),
        );
        let selected: Signal<Option<usize>> = Signal::new(None);
        let sel_display = Signal::new("None".to_string());
        let sel_sub = selected.clone();
        let sel_txt = sel_display.clone();
        auralis_signal::subscribe(
            &selected,
            std::rc::Rc::new(move || {
                sel_txt.set(match sel_sub.read() {
                    Some(i) => format!("Entry {i}"),
                    None => "None".into(),
                });
            }),
        );

        VStack::new()
            .gap(8.0)
            .push(section_title("Virtual List — 5,000 items"))
            .push(section_sub(
                "Scroll anywhere and click an item: the selection must land on \
                 the exact item under the cursor.",
            ))
            .push(
                SizedBox::new()
                    .width(480.0)
                    .height(320.0)
                    .child(List::new(items).item_height(30.0).selected(selected)),
            )
            .push(
                HStack::new()
                    .gap(8.0)
                    .push(Text::new("Selected:"))
                    .push(Text::new(String::new()).bind(sel_display)),
            )
    })
}
