//! Portal anchor tracking: when a portal's anchor moves (e.g. its container
//! scrolls) while the portal itself is clean, the reposition MEASURE must
//! still reach taffy. Regression for the phase-ordering bug where
//! `update_portal_positions` ran after the dirty drains and its MEASURE was
//! consumed by recheck_dirty (paint-only), leaving the portal at its old
//! screen position (audit 2026-07-17).

use auralis_signal::Signal;
use burin::core::ElementId;
use burin::style::Point;
use burin::testing::selector::by_role;
use burin::testing::TestHarness;
use burin::widgets::display::Text;
use burin::widgets::input::Select;
use burin::widgets::layout::{ScrollView, SizedBox, VStack};

#[test]
fn portal_follows_anchor_after_container_scroll() {
    // A Select inside a scrollable column: open its dropdown, then scroll the
    // outer container. The dropdown portal must track the moved trigger.
    let sel = Signal::new(None::<String>);
    let mut content = VStack::new().push(Text::new("spacer-top"));
    // Push the select down a bit, with plenty of scrollable content around it.
    for i in 0..10 {
        content = content.push(
            SizedBox::new()
                .width(300.0)
                .height(40.0)
                .child(Text::new(format!("filler {i}"))),
        );
    }
    let select = Select::new(sel)
        .options(vec!["Alpha".into(), "Beta".into(), "Gamma".into()])
        .render(|s: &String| s.clone());
    content = content.push(select);
    for i in 0..20 {
        content = content.push(
            SizedBox::new()
                .width(300.0)
                .height(40.0)
                .child(Text::new(format!("tail {i}"))),
        );
    }

    let mut h = TestHarness::new(500.0, 600.0);
    let mounted = h.mount(
        SizedBox::new()
            .width(400.0)
            .height(500.0)
            .child(ScrollView::new().child(content)),
    );
    for _ in 0..5 {
        h.run_frame();
    }

    // Open the dropdown.
    let trigger = h
        .find_all_sel(by_role(accesskit::Role::ComboBox))
        .first()
        .copied()
        .expect("select trigger");
    let tb = h.find(trigger).unwrap().screen_bounds;
    h.click_at(Point::new(tb.x + tb.width / 2.0, tb.y + tb.height / 2.0));
    for _ in 0..4 {
        h.run_frame();
    }

    let options = h.find_all_sel(by_role(accesskit::Role::ListBoxOption));
    assert!(!options.is_empty(), "dropdown open");
    // The portal element: walk up from an option to the z>0 ancestor.
    let mut portal: Option<ElementId> = None;
    let mut cur = Some(options[0]);
    while let Some(id) = cur {
        if h.find(id).map_or(0, |e| e.z_index) > 0 {
            portal = Some(id);
            break;
        }
        cur = burin::core::dirty_registry::parent_of(id);
    }
    let portal = portal.expect("portal ancestor");
    let portal_y_before = h.find(portal).unwrap().screen_bounds.y;
    let trigger_y_before = h.find(trigger).unwrap().screen_bounds.y;

    // Scroll the OUTER container: the trigger's on-screen position moves up.
    let outer_scroll = {
        let mut found = None;
        let mut stack = vec![mounted];
        while let Some(id) = stack.pop() {
            if h.root().comp_scroll(id).is_some()
                && h.find(id).map_or(false, |e| e.screen_bounds.height > 400.0)
            {
                found = Some(id);
                break;
            }
            if let Some(el) = h.find(id) {
                for &c in &el.children {
                    stack.push(c);
                }
            }
        }
        found.expect("outer scroll container")
    };
    h.scroll(outer_scroll, 0.0, -80.0);
    for _ in 0..4 {
        h.run_frame();
    }

    let trigger_sb = h.find(trigger).unwrap().screen_bounds;
    let (_, scroll_y) = h.root().accumulated_scroll(trigger);
    let trigger_screen_y = trigger_sb.y - scroll_y;
    let portal_y_after = h.find(portal).unwrap().screen_bounds.y;

    println!(
        "[probe] trigger doc_y={} screen_y={trigger_screen_y} scroll_y={scroll_y} portal_y before={portal_y_before} after={portal_y_after}",
        trigger_sb.y
    );

    // The portal (window space) must sit just below the trigger's SCREEN
    // position after the scroll.
    let expected_top = trigger_screen_y + trigger_sb.height;
    assert!(
        (portal_y_after - expected_top).abs() < 12.0,
        "portal must follow the scrolled anchor: expected y≈{expected_top}, got {portal_y_after} (before scroll: {portal_y_before}, trigger before: {trigger_y_before})"
    );
}
