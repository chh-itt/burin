//! IME composition caret mapping: during preedit, the caret must sit INSIDE
//! the composition text at the IME-reported cursor offset — not at the splice
//! point (audit 2026-07-16 follow-up #4).

use auralis_signal::Signal;
use burin::render::wgpu::glyphon_bridge::create_buffer;
use burin::style::TextAlign;
use burin::testing::TestHarness;
use burin::widgets::input::TextInput;

fn glyph_x(buf: &cosmic_text::Buffer, idx: usize) -> f32 {
    buf.layout_runs()
        .flat_map(|r| r.glyphs.iter().map(|g| g.x).collect::<Vec<_>>())
        .nth(idx)
        .expect("glyph present")
}

#[test]
fn preedit_caret_sits_inside_composition() {
    let value = Signal::new(String::new());
    let mut h = TestHarness::new(400.0, 100.0);
    let id = h.mount(TextInput::new(value.clone()));
    h.run_frame();
    h.click(id); // focus so arrow keys route to the editor
    h.type_text(id, "abcd");
    // Move the caret between 'b' and 'c' (cursor = 2).
    h.press_key(burin::event::Key::ArrowLeft, burin::event::Modifiers::NONE);
    h.press_key(burin::event::Key::ArrowLeft, burin::event::Modifiers::NONE);
    h.run_frame();

    // IME preedit "xyz" with its internal cursor after the first byte ('x').
    h.events_mut()
        .fire_ime_preedit(id, "xyz".into(), Some((1, 1)));
    h.run_frame();
    h.run_frame();

    let cursor_x = h
        .find(id)
        .unwrap()
        .cursor_x()
        .expect("cursor_x present")
        .get();

    // display = "ab" + "xyz" + "cd"; caret at char 2 (splice) + 1 (inside) = 3
    // → the x of glyph index 3 ('y') in the shaped display text.
    let ref_buf = create_buffer("abxyzcd", 14.0, 1.4, 400, None, None, TextAlign::Start);
    let expected = glyph_x(&ref_buf, 3);
    assert!(
        (cursor_x - expected).abs() < 2.0,
        "caret inside preedit: expected x≈{expected}, got {cursor_x}"
    );
}

#[test]
fn preedit_without_cursor_range_puts_caret_at_end() {
    let value = Signal::new(String::new());
    let mut h = TestHarness::new(400.0, 100.0);
    let id = h.mount(TextInput::new(value.clone()));
    h.run_frame();
    h.type_text(id, "ab");
    h.run_frame();

    h.events_mut().fire_ime_preedit(id, "xy".into(), None);
    h.run_frame();
    h.run_frame();

    let cursor_x = h.find(id).unwrap().cursor_x().unwrap().get();
    // display = "abxy"; caret after the whole preedit → x of glyph end.
    let ref_buf = create_buffer("abxy", 14.0, 1.4, 400, None, None, TextAlign::Start);
    let expected = ref_buf
        .layout_runs()
        .flat_map(|r| r.glyphs.iter().map(|g| g.x + g.w).collect::<Vec<_>>())
        .fold(0.0f32, f32::max);
    assert!(
        (cursor_x - expected).abs() < 2.0,
        "caret at preedit end: expected x≈{expected}, got {cursor_x}"
    );
}

// ═══ P0: atomic IME commit (audit: display-text splice pass, phase 0) ═══

#[test]
fn ime_commit_inserts_atomically() {
    let value = Signal::new(String::new());
    let mut h = TestHarness::new(400.0, 100.0);
    let id = h.mount(TextInput::new(value.clone()));
    h.run_frame();
    h.click(id);
    h.events_mut()
        .fire_ime_preedit(id, "nihao".into(), Some((5, 5)));
    h.run_frame();
    // winit contract: an empty preedit arrives right before Commit.
    h.events_mut().fire_ime_preedit(id, String::new(), None);
    let handled = h.events_mut().fire_ime_commit(id, "你好".to_string());
    assert!(
        handled,
        "TextInput must register an atomic ime_commit handler"
    );
    h.run_frame();
    assert_eq!(value.with(String::clone), "你好");
}

#[test]
fn ime_commit_is_single_undo_step() {
    let value = Signal::new(String::new());
    let mut h = TestHarness::new(400.0, 100.0);
    let id = h.mount(TextInput::new(value.clone()));
    h.run_frame();
    h.click(id);
    h.type_text(id, "ab");
    h.run_frame();
    h.events_mut().fire_ime_commit(id, "你好".to_string());
    h.run_frame();
    assert_eq!(value.with(String::clone), "ab你好");
    h.press_key(
        burin::event::Key::Character("z".into()),
        burin::event::Modifiers {
            ctrl: true,
            ..burin::event::Modifiers::NONE
        },
    );
    h.run_frame();
    assert_eq!(
        value.with(String::clone),
        "ab",
        "one undo must remove exactly the committed text"
    );
}

#[test]
fn ime_commit_without_handler_returns_false() {
    let value = Signal::new(String::new());
    let mut h = TestHarness::new(400.0, 100.0);
    let _id = h.mount(TextInput::new(value));
    h.run_frame();
    let root = h.root_id();
    assert!(
        !h.events_mut().fire_ime_commit(root, "x".to_string()),
        "elements without a handler must report false so the caller can fall back"
    );
}

#[test]
fn commit_preedit_flushes_composition_text() {
    // Focus-transfer path: registry.commit_preedit must flush the live
    // preedit into the document (it was a silent no-op — buffer never written).
    let value = Signal::new(String::new());
    let mut h = TestHarness::new(400.0, 100.0);
    let id = h.mount(TextInput::new(value.clone()));
    h.run_frame();
    h.click(id);
    h.events_mut()
        .fire_ime_preedit(id, "wo".into(), Some((2, 2)));
    h.run_frame();
    h.events_mut().commit_preedit(id);
    h.run_frame();
    assert_eq!(value.with(String::clone), "wo");
}

// ═══ P0: composition underline must reach the painted scene ═══

#[test]
fn composition_underline_is_painted() {
    let value = Signal::new(String::new());
    let mut h = TestHarness::new(400.0, 100.0);
    let id = h.mount(TextInput::new(value.clone()));
    h.run_frame();
    h.click(id);
    h.events_mut()
        .fire_ime_preedit(id, "nihao".into(), Some((5, 5)));

    // The painting frame is the one where the display-text rebuild lands; a
    // settled frame clears `last_scene`, so scan each frame's scene.
    let mut found = false;
    for _ in 0..3 {
        h.run_frame();
        let b = h.find(id).unwrap().bounds();
        found = h.last_scene.iter().any(|cmd| match cmd {
            burin::render::DrawCommand::FillRect { rect, .. } => {
                (rect.height - 2.0).abs() < 0.5      // underline thickness
                    && rect.width > 4.0               // spans glyphs (caret is only 2px wide)
                    && rect.x >= b.x
                    && rect.x <= b.x + b.width
                    && rect.y >= b.y
                    && rect.y <= b.y + b.height
            }
            _ => false,
        });
        if found {
            break;
        }
    }
    assert!(
        found,
        "composition underline FillRect missing from scene:\n{}",
        h.dump_scene()
    );
}

// ═══ P1: IME cursor area grounds at the caret, not window origin ═══

#[test]
fn ime_cursor_area_follows_caret() {
    let value = Signal::new("你好世界".to_string());
    let mut h = TestHarness::new(400.0, 100.0);
    let id = h.mount(TextInput::new(value.clone()));
    h.run_frame();
    h.click(id);
    // Move cursor after the second CJK char (cursor=2).
    h.press_key(burin::event::Key::ArrowLeft, burin::event::Modifiers::NONE);
    h.press_key(burin::event::Key::ArrowLeft, burin::event::Modifiers::NONE);
    h.run_frame();

    let bounds = burin::core::dirty_registry::bounds_of(id).unwrap();
    let local_rect_opt = h.find(id).unwrap().ime_cursor_rect().and_then(|c| c.get());
    assert!(
        local_rect_opt.is_some(),
        "ime_cursor_rect must be populated after typing and focusing"
    );
    let surface = burin::platform::compose_ime_surface_rect(bounds, local_rect_opt, (0.0, 0.0));
    assert!(
        surface.x >= bounds.x,
        "IME area must be inside element bounds"
    );
    assert!(surface.y >= bounds.y);
    assert!(
        surface.x < bounds.x + bounds.width,
        "IME area x must be within element: surface_x={} bounds_right={}",
        surface.x,
        bounds.x + bounds.width
    );
}

#[test]
fn preedit_cursor_moves_within_same_text() {
    // Same preedit text, different cursor_range — the caret must move
    // (regression for the render-key guard missing cursor_range).
    let value = Signal::new(String::new());
    let mut h = TestHarness::new(400.0, 100.0);
    let id = h.mount(TextInput::new(value.clone()));
    h.run_frame();
    h.events_mut()
        .fire_ime_preedit(id, "xyz".into(), Some((0, 0)));
    h.run_frame();
    h.run_frame();
    let x0 = h.find(id).unwrap().cursor_x().unwrap().get();

    h.events_mut()
        .fire_ime_preedit(id, "xyz".into(), Some((3, 3)));
    h.run_frame();
    h.run_frame();
    let x3 = h.find(id).unwrap().cursor_x().unwrap().get();

    assert!(
        x3 > x0 + 1.0,
        "caret must advance when the IME cursor moves within the preedit: x0={x0} x3={x3}"
    );
}
