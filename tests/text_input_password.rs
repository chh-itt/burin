//! Password-mode TextInput: caret / selection must be positioned against the
//! *masked* display text (bullets), not the raw text. The bullet char '•' is
//! 3 UTF-8 bytes while typical password chars are 1 byte — mapping raw-text
//! byte offsets into the masked buffer landed the caret at ~1/3 of the row.

use auralis_signal::Signal;
use burin::render::wgpu::glyphon_bridge::create_buffer;
use burin::style::TextAlign;
use burin::testing::TestHarness;
use burin::widgets::input::text_editor::render::{cursor_pixel_pos, selection_rects};
use burin::widgets::input::{TextInput, TextInputType};

fn last_glyph_end(buf: &cosmic_text::Buffer) -> f32 {
    buf.layout_runs()
        .flat_map(|r| r.glyphs.iter().map(|g| g.x + g.w).collect::<Vec<_>>())
        .fold(0.0f32, f32::max)
}

#[test]
fn password_cursor_at_end_matches_last_bullet() {
    let raw = "abcdef"; // 6 chars, 1 byte each
    let display = "\u{2022}".repeat(raw.chars().count()); // 6 bullets, 3 bytes each
    let buf = create_buffer(&display, 16.0, 1.4, 400, None, None, TextAlign::Start);

    let expected_end = last_glyph_end(&buf);
    assert!(expected_end > 0.0, "mask buffer must have glyphs");

    // Caret after the last char must sit at the end of the last bullet.
    let (_row, x) = cursor_pixel_pos(&buf, &display, raw.chars().count());
    assert!(
        (x - expected_end).abs() < 0.5,
        "caret at end: expected x≈{expected_end}, got {x} (raw-byte mapping bug)"
    );

    // Caret after char 3 must sit at the start of the 4th bullet.
    let glyph4_x = buf
        .layout_runs()
        .flat_map(|r| r.glyphs.iter().map(|g| g.x).collect::<Vec<_>>())
        .nth(3)
        .unwrap();
    let (_row, x3) = cursor_pixel_pos(&buf, &display, 3);
    assert!(
        (x3 - glyph4_x).abs() < 0.5,
        "caret after 3 chars: expected x≈{glyph4_x}, got {x3}"
    );
}

#[test]
fn password_selection_covers_masked_glyphs() {
    let raw = "abcdef";
    let display = "\u{2022}".repeat(raw.chars().count());
    let buf = create_buffer(&display, 16.0, 1.4, 400, None, None, TextAlign::Start);
    let expected_end = last_glyph_end(&buf);

    // Select all 6 chars: the single rect must span the full bullet row.
    let rects = selection_rects(&buf, &display, 0, 6, 16.0 * 1.4, 0.0, 0.0);
    assert_eq!(rects.len(), 1, "single-line selection produces one rect");
    let r = rects[0];
    assert!(
        (r.width - expected_end).abs() < 1.0,
        "select-all rect must span all bullets: expected w≈{expected_end}, got {}",
        r.width
    );
}

#[test]
fn password_harness_cursor_x_tracks_bullets() {
    let value = Signal::new(String::new());
    let mut h = TestHarness::new(400.0, 100.0);
    let id = h.mount(TextInput::new(value.clone()).input_type(TextInputType::Password));
    h.run_frame();
    h.type_text(id, "abcdef");
    h.run_frame();
    h.run_frame();

    let cursor_x = h
        .find(id)
        .unwrap()
        .cursor_x()
        .expect("password input has cursor_x")
        .get();

    // Expected: end of 6 shaped bullets (same font config as the widget: 14px default).
    let display = "\u{2022}".repeat(6);
    let ref_buf = create_buffer(&display, 14.0, 1.4, 400, None, None, TextAlign::Start);
    let expected = last_glyph_end(&ref_buf);
    assert!(
        (cursor_x - expected).abs() < 2.0,
        "widget caret x: expected ≈{expected}, got {cursor_x}"
    );
}
