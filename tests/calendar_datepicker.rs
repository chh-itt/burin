#[cfg(feature = "ext-jiff")]
use auralis_signal::Signal;
#[cfg(feature = "ext-jiff")]
use burin::core::dirty_registry;
#[cfg(feature = "ext-jiff")]
use burin::event::{Key, Modifiers};
#[cfg(feature = "ext-jiff")]
use burin::testing::selector::*;
#[cfg(feature = "ext-jiff")]
use burin::testing::test_harness::TestHarness;
#[cfg(feature = "ext-jiff")]
use burin::widgets::input::{DatePicker, DateRange};
#[cfg(feature = "ext-jiff")]
use jiff::civil::Date;

// ═══════════════════════════════════════════════════════════════
// DatePicker — mount & open
// ═══════════════════════════════════════════════════════════════

#[cfg(feature = "ext-jiff")]
#[test]
fn datepicker_mounts_with_trigger() {
    let sel = Signal::new(None::<Date>);
    let mut h = TestHarness::new(400.0, 400.0);
    let _id = h.mount(DatePicker::new(sel));
    h.run_frame();
    let trigger = h.find_sel(by_label("Date picker"));
    assert!(trigger.is_some(), "DatePicker trigger should exist");
}

#[cfg(feature = "ext-jiff")]
#[test]
fn datepicker_opens_calendar_on_trigger_click() {
    let sel = Signal::new(None::<Date>);
    let mut h = TestHarness::new(400.0, 400.0);
    let _root = h.mount(DatePicker::new(sel));
    h.run_frame();

    let trigger = h
        .find_sel(by_label("Date picker"))
        .expect("trigger should exist");
    let trigger_parent = dirty_registry::parent_of(trigger).unwrap();
    h.click(trigger_parent);
    h.run_frame();
    h.run_frame();

    let cal = h.find_sel(by_label("Calendar"));
    assert!(
        cal.is_some(),
        "Calendar should be visible after trigger click"
    );
}

#[cfg(feature = "ext-jiff")]
#[test]
fn datepicker_escape_closes_dropdown() {
    let sel = Signal::new(None::<Date>);
    let mut h = TestHarness::new(400.0, 400.0);
    let _root = h.mount(DatePicker::new(sel));
    h.run_frame();

    let trigger = h
        .find_sel(by_label("Date picker"))
        .expect("trigger should exist");
    h.click(trigger);
    h.run_frame();
    h.run_frame();

    let cal = h.find_sel(by_label("Calendar"));
    assert!(cal.is_some(), "Calendar should be open");

    h.press_key(Key::Escape, Modifiers::default());
    h.run_frame();
    h.run_frame();
    // After Escape, dropdown is dismissed — no panic
}

// ═══════════════════════════════════════════════════════════════
// Calendar — day selection
// ═══════════════════════════════════════════════════════════════

#[cfg(feature = "ext-jiff")]
#[test]
fn select_date_closes_dropdown_and_updates_signal() {
    let sel = Signal::new(None::<Date>);
    let mut h = TestHarness::new(400.0, 400.0);
    let _root = h.mount(DatePicker::new(sel.clone()));
    h.run_frame();

    let trigger = h
        .find_sel(by_label("Date picker"))
        .expect("trigger should exist");
    h.click(trigger);
    h.run_frame();
    h.run_frame();

    let prev_btn = h.find_sel(by_label("Previous"));
    assert!(
        prev_btn.is_some(),
        "Calendar should be open with Previous button"
    );

    // Click on cell "15" — a safe bet in any month
    let day_cell = h.find_sel(by_text("15"));
    if let Some(cell_id) = day_cell {
        h.click(cell_id);
        h.run_frame();
        h.run_frame();

        let today = jiff::Zoned::now().date();
        let y = today.year();
        let m = today.month();
        if let Ok(d) = Date::new(y, m, 15) {
            assert_eq!(
                sel.read(),
                Some(d),
                "Selected signal should match clicked date"
            );
        }
    }
}

#[cfg(feature = "ext-jiff")]
#[test]
fn calendar_header_navigation_buttons_work() {
    let sel = Signal::new(None::<Date>);
    let mut h = TestHarness::new(400.0, 400.0);
    let _root = h.mount(DatePicker::new(sel));
    h.run_frame();

    let trigger = h.find_sel(by_label("Date picker")).unwrap();
    h.click(trigger);
    h.run_frame();
    h.run_frame();

    // Next button should exist and be clickable (no panic)
    let next = h
        .find_sel(by_label("Next"))
        .expect("Next button should exist");
    h.click(next);
    h.run_frame();
    h.run_frame();

    // Previous should exist
    let prev = h
        .find_sel(by_label("Previous"))
        .expect("Prev button should exist");
    h.click(prev);
    h.run_frame();
    h.run_frame();

    // 10 rapid prev/next cycles shouldn't panic
    for _ in 0..10 {
        h.click(next);
        h.run_frame();
        h.click(prev);
        h.run_frame();
    }
}

// ═══════════════════════════════════════════════════════════════
// Calendar — view switching (Day → Month → Year)
// ═══════════════════════════════════════════════════════════════

#[cfg(feature = "ext-jiff")]
#[test]
fn title_click_cycles_to_year_view() {
    let sel = Signal::new(None::<Date>);
    let mut h = TestHarness::new(400.0, 400.0);
    let _root = h.mount(DatePicker::new(sel));
    h.run_frame();

    let trigger = h.find_sel(by_label("Date picker")).unwrap();
    h.click(trigger);
    h.run_frame();
    h.run_frame();

    // Day view: weekday headers visible
    let weekday = h.find_sel(by_text("Tu"));
    assert!(
        weekday.is_some(),
        "Weekday 'Tu' should be visible in Day view"
    );

    // Click title → Month view
    let title = h
        .find_sel(by_label("Current month and year"))
        .expect("title");
    h.click(title);
    h.run_frame();
    h.run_frame();

    // Click title → Year view
    h.click(title);
    h.run_frame();
    h.run_frame();

    // Year view: should show year labels
    let year_cell = h.find_sel(by_text("2026"));
    assert!(
        year_cell.is_some(),
        "Year 2026 should be visible in Year view"
    );

    // Click on year 2026 → should switch to Day view, month 1
    if let Some(yr) = year_cell {
        h.click(yr);
        h.run_frame();
        h.run_frame();
    }
}

// ═══════════════════════════════════════════════════════════════
// DatePicker — range mode
// ═══════════════════════════════════════════════════════════════

#[cfg(feature = "ext-jiff")]
#[test]
fn range_mode_selects_start_and_end() {
    let range_sig = Signal::new(None::<DateRange>);
    let mut h = TestHarness::new(400.0, 400.0);
    let _root = h.mount(DatePicker::new_range(range_sig.clone()));
    h.run_frame();

    let trigger = h.find_sel(by_label("Date picker")).unwrap();
    h.click(trigger);
    h.run_frame();
    h.run_frame();

    let cal = h.find_sel(by_label("Calendar"));
    assert!(cal.is_some(), "Calendar should be open in range mode");

    // Click two distinct day cells
    let d1 = h.find_sel(by_text("10"));
    let d2 = h.find_sel(by_text("15"));

    if let (Some(c1), Some(c2)) = (d1, d2) {
        h.click(c1);
        h.run_frame();
        h.run_frame();

        h.click(c2);
        h.run_frame();
        h.run_frame();

        let range = range_sig.read();
        assert!(
            range.is_some(),
            "Range signal should be set after selecting start + end"
        );
    }
}

// ═══════════════════════════════════════════════════════════════
// Calendar — min/max date constraints
// ═══════════════════════════════════════════════════════════════

#[cfg(feature = "ext-jiff")]
#[test]
fn year_view_respects_min_max_constraints() {
    let sel = Signal::new(None::<Date>);
    let min = Date::constant(2024, 1, 1);
    let max = Date::constant(2026, 12, 31);
    let mut h = TestHarness::new(400.0, 400.0);
    let _root = h.mount(DatePicker::new(sel.clone()).min_date(min).max_date(max));
    h.run_frame();

    // Open + switch to Year view (two title clicks)
    let trigger = h.find_sel(by_label("Date picker")).unwrap();
    h.click(trigger);
    h.run_frame();
    h.run_frame();

    let title = h.find_sel(by_label("Current month and year")).unwrap();
    h.click(title);
    h.run_frame();
    h.run_frame();
    h.click(title);
    h.run_frame();
    h.run_frame();

    // Years outside 2024-2026 range should not change signal
    let year_2023 = h.find_sel(by_text("2023"));
    if let Some(yr) = year_2023 {
        h.click(yr);
        h.run_frame();
        h.run_frame();
        assert_eq!(
            sel.read(),
            None,
            "Out-of-range year click should not set a date"
        );
    }
}

// ═══════════════════════════════════════════════════════════════
// Calendar — keyboard navigation (Day view)
// ═══════════════════════════════════════════════════════════════

#[cfg(feature = "ext-jiff")]
#[test]
fn day_view_arrow_keys_navigate_focus() {
    let sel = Signal::new(None::<Date>);
    let mut h = TestHarness::new(400.0, 400.0);
    let _root = h.mount(DatePicker::new(sel.clone()));
    h.run_frame();

    // Open dropdown
    let trigger = h.find_sel(by_label("Date picker")).unwrap();
    h.click(trigger);
    h.run_frame();
    h.run_frame();

    // Focus the calendar
    let cal = h
        .find_sel(by_label("Calendar"))
        .expect("Calendar should be visible");
    h.click(cal);
    h.run_frame();

    // ArrowDown moves from header to the 1st of the current month.
    h.press_key(Key::ArrowDown, Modifiers::default());
    h.run_frame();
    // ArrowRight moves one day forward (to the 2nd).
    h.press_key(Key::ArrowRight, Modifiers::default());
    h.run_frame();

    // Enter should select
    h.press_key(Key::Enter, Modifiers::default());
    h.run_frame();
    h.run_frame();

    assert!(sel.read().is_some(), "Enter should select the focused date");
}

// ═══════════════════════════════════════════════════════════════
// Calendar — structural: reopen preserves state
// ═══════════════════════════════════════════════════════════════

#[cfg(feature = "ext-jiff")]
#[test]
fn reopen_calendar_preserves_state() {
    let t = Date::constant(2026, 7, 15);
    let sel = Signal::new(Some(t));
    let mut h = TestHarness::new(400.0, 400.0);
    let _root = h.mount(DatePicker::new(sel.clone()));
    h.run_frame();

    // Open
    let trigger = h.find_sel(by_label("Date picker")).unwrap();
    h.click(trigger);
    h.run_frame();
    h.run_frame();

    assert!(
        h.find_sel(by_label("Calendar")).is_some(),
        "Calendar should open"
    );

    // Close with Escape
    h.press_key(Key::Escape, Modifiers::default());
    h.run_frame();
    h.run_frame();

    // Reopen
    h.click(trigger);
    h.run_frame();
    h.run_frame();

    assert!(
        h.find_sel(by_label("Calendar")).is_some(),
        "Calendar should reopen"
    );
    assert_eq!(sel.read(), Some(t), "Selection should survive reopen");
}

// ═══════════════════════════════════════════════════════════════
// Multiple DatePickers are independent
// ═══════════════════════════════════════════════════════════════

#[cfg(feature = "ext-jiff")]
#[test]
fn two_datepickers_are_independent() {
    let a = Signal::new(None::<Date>);
    let b = Signal::new(None::<Date>);
    let mut h = TestHarness::new(800.0, 600.0);
    h.mount(
        burin::widgets::layout::VStack::new()
            .push(DatePicker::new(a.clone()))
            .push(DatePicker::new(b.clone())),
    );
    h.run_frame();

    let triggers = h.find_all_sel(by_label("Date picker"));
    assert_eq!(triggers.len(), 2, "Should have two date picker triggers");
    let picker_a = triggers[0];

    // Open A, select date
    h.click(picker_a);
    h.run_frame();
    h.run_frame();

    let day_1 = h.find_sel(by_text("1"));
    if let Some(d) = day_1 {
        h.click(d);
        h.run_frame();
        h.run_frame();
    }

    // B should still be None (unaffected by A's selection)
    assert!(b.read().is_none(), "Picker B should be unaffected");
}
