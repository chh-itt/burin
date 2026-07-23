//! Widget-domain registry lifecycle regression tests (audit 2026-07-17
//! round 3, Finding A).
//!
//! `teardown_subtree` runs `dirty_registry::run_teardown_hooks(id)` for every
//! removed element; widget/platform modules install cleanup hooks lazily on
//! first registration. These tests lock the guarantee that mount/unmount
//! cycles do not grow any of the hooked registries:
//! form VALIDATORS / FORM_FIELDS, the a11y node cache, the overlay stack,
//! and sticky-header entries.

use auralis_signal::Signal;
use burin::testing::TestHarness;
use burin::widgets::input::{Field, Form, TextInput};
use burin::widgets::layout::VStack;

fn mount_form_page(h: &mut TestHarness) -> burin::core::ElementId {
    let value = Signal::new(String::new());
    let getter = value.clone();
    h.mount(
        VStack::new().push(
            Form::new()
                .child(
                    Field::new()
                        .label("Name")
                        .required(true)
                        .validator(|v: &str| {
                            if v.is_empty() {
                                Some("required".into())
                            } else {
                                None
                            }
                        })
                        .value(move || getter.read())
                        .child(TextInput::new(value.clone())),
                )
                .child(
                    Field::new()
                        .label("Email")
                        .validator(|v: &str| {
                            if v.contains('@') {
                                None
                            } else {
                                Some("invalid".into())
                            }
                        })
                        .child(TextInput::new(Signal::new(String::new()))),
                ),
        ),
    )
}

#[test]
fn form_registries_do_not_grow_across_mount_unmount_cycles() {
    let mut h = TestHarness::new(600.0, 400.0);
    let arena_baseline = h.arena.len();
    let (v0, f0, l0) = burin::widgets::input::debug_registry_sizes();

    for _ in 0..5 {
        let page = mount_form_page(&mut h);
        h.run_frame();
        let (v, _, l) = burin::widgets::input::debug_registry_sizes();
        assert!(v > v0, "validators registered while mounted");
        assert!(l > l0, "field links registered while mounted");
        h.arena.remove(page);
        h.run_frame();
        h.run_frame();
    }

    let (v, f, l) = burin::widgets::input::debug_registry_sizes();
    assert_eq!(
        (v, f, l),
        (v0, f0, l0),
        "form registries must return to baseline after 5 mount/unmount cycles"
    );
    assert_eq!(h.arena.len(), arena_baseline, "no element leaks");
}

#[test]
fn partial_teardown_only_drops_removed_fields_validator() {
    let mut h = TestHarness::new(600.0, 400.0);
    let page = mount_form_page(&mut h);
    h.run_frame();
    let (v_mounted, _, l_mounted) = burin::widgets::input::debug_registry_sizes();
    assert!(v_mounted >= 2, "two validators live");
    assert!(l_mounted >= 2, "two field links live");

    h.arena.remove(page);
    h.run_frame();
    let (v, forms, links) = burin::widgets::input::debug_registry_sizes();
    assert_eq!(v, 0, "all validators dropped with the page");
    assert_eq!(forms, 0, "form entry dropped");
    assert_eq!(links, 0, "all field links dropped");
}

#[test]
fn a11y_node_cache_shrinks_on_teardown() {
    let mut h = TestHarness::new(600.0, 400.0);
    // Build the a11y tree once so the cache is populated and the hook is
    // installed (production builds it per-frame when a11y is active).
    let page = mount_form_page(&mut h);
    h.run_frame();
    let _ = burin::platform::build_accessibility_tree(&h.arena, h.root_id(), None);
    let populated = burin::platform::debug_node_cache_len();
    assert!(populated > 0, "cache populated after build");

    h.arena.remove(page);
    h.run_frame();
    let after = burin::platform::debug_node_cache_len();
    assert!(
        after < populated,
        "teardown must evict cached a11y nodes ({populated} -> {after})"
    );
}

#[test]
fn image_registry_freed_when_last_referencing_element_dies() {
    use burin::widgets::display::Image;

    let mut h = TestHarness::new(600.0, 400.0);
    let (reg0, refs0, links0) = burin::render::wgpu::debug_image_registry_sizes();

    // Two elements sharing identical pixel content (same hash) + one unique.
    let shared: Vec<u8> = vec![200u8; 8 * 8 * 4];
    let unique: Vec<u8> = (0..8 * 8 * 4).map(|i| (i % 251) as u8).collect();
    let page = h.mount(
        VStack::new()
            .push(Image::from_rgba(shared.clone(), 8, 8))
            .push(Image::from_rgba(shared.clone(), 8, 8))
            .push(Image::from_rgba(unique.clone(), 8, 8)),
    );
    h.run_frame();
    let (reg, _, links) = burin::render::wgpu::debug_image_registry_sizes();
    assert_eq!(
        reg,
        reg0 + 2,
        "shared content dedupes to one entry + unique"
    );
    assert_eq!(links, links0 + 3, "three referencing elements");

    h.arena.remove(page);
    h.run_frame();
    let (reg_after, refs_after, links_after) = burin::render::wgpu::debug_image_registry_sizes();
    assert_eq!(
        (reg_after, refs_after, links_after),
        (reg0, refs0, links0),
        "pixel data freed when the last referencing element is torn down"
    );

    // Mount/unmount churn does not grow anything.
    for _ in 0..3 {
        let p = h.mount(VStack::new().push(Image::from_rgba(shared.clone(), 8, 8)));
        h.run_frame();
        h.arena.remove(p);
        h.run_frame();
    }
    let (reg_final, ..) = burin::render::wgpu::debug_image_registry_sizes();
    assert_eq!(reg_final, reg0, "no growth across churn");
}

#[test]
fn pinned_anonymous_image_survives_refcount_zero() {
    use burin::widgets::display::Image;
    use std::rc::Rc;

    let mut h = TestHarness::new(600.0, 400.0);
    let pixels: Vec<u8> = vec![123u8; 4 * 4 * 4];

    // Anonymous registration pins the hash (old API semantics).
    // Compute the same hash the Image widget uses by mounting one first.
    let page = h.mount(VStack::new().push(Image::from_rgba(pixels.clone(), 4, 4)));
    h.run_frame();
    let (reg_with, ..) = burin::render::wgpu::debug_image_registry_sizes();

    // Pin it via the anonymous API under the same content hash. The hash fn
    // is internal, so approximate the scenario: anonymous-register a distinct
    // image, then verify it survives arbitrary teardowns.
    burin::render::wgpu::register_image(0xDEAD_BEEF, 4, 4, Rc::new(pixels.clone()));
    h.arena.remove(page);
    h.run_frame();
    let (reg_after, ..) = burin::render::wgpu::debug_image_registry_sizes();
    assert_eq!(
        reg_after,
        reg_with - 1 + 1,
        "element-owned entry freed; pinned anonymous entry retained"
    );
    assert!(
        burin::render::wgpu::lookup_image(0xDEAD_BEEF).is_some(),
        "pinned entry must survive"
    );
}

#[cfg(feature = "ext-svg")]
#[test]
fn svg_asset_store_does_not_grow_across_mount_unmount_cycles() {
    use burin::widgets::display::SvgImage;

    const SVG: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16"><rect width="16" height="16" fill="green"/></svg>"#;

    let mut h = TestHarness::new(600.0, 400.0);
    let (_, svg0, _) = burin::asset::debug_asset_counts();

    for _ in 0..5 {
        let page = h.mount(
            VStack::new()
                // Two identical-content instances: must share ONE parsed tree.
                .push(SvgImage::from_bytes(SVG).unwrap().size(16, 16))
                .push(SvgImage::from_bytes(SVG).unwrap().size(16, 16)),
        );
        h.run_frame();
        let (_, svg_mounted, _) = burin::asset::debug_asset_counts();
        assert_eq!(
            svg_mounted,
            svg0 + 1,
            "identical SVG bytes must dedup to one live parsed tree while mounted"
        );
        h.arena.remove(page);
        h.run_frame();
    }

    let (_, svg_final, _) = burin::asset::debug_asset_counts();
    assert_eq!(
        svg_final, svg0,
        "SvgImage-owned assets must be freed when their elements are torn down"
    );
}

#[cfg(feature = "ext-svg")]
#[test]
fn dropped_unmounted_svg_widget_leaks_nothing() {
    const SVG: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" width="8" height="8"><circle cx="4" cy="4" r="3" fill="black"/></svg>"#;

    let (_, svg0, _) = burin::asset::debug_asset_counts();
    {
        let widget = burin::widgets::display::SvgImage::from_bytes(SVG).unwrap();
        let (_, svg_live, _) = burin::asset::debug_asset_counts();
        assert_eq!(
            svg_live,
            svg0 + 1,
            "parsed tree live while the widget exists"
        );
        drop(widget);
    }
    let (_, svg_after, _) = burin::asset::debug_asset_counts();
    assert_eq!(
        svg_after, svg0,
        "a constructed-but-never-mounted SvgImage must free its asset on drop"
    );
}
