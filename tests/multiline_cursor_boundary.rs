//! Regression: vertical cursor movement converges to line boundary.
//!
//! Standard editor behavior: pressing Up while already on the first visual row
//! moves the cursor to the very start of the text (not stuck mid-line);
//! pressing Down while on the last row moves it to the end. Previously
//! `move_visual_row` returned early when the row didn't change, leaving the
//! cursor stuck.

use auralis_signal::Signal;
use burin::event::{Key, Modifiers};
use burin::testing::TestHarness;
use burin::widgets::input::{TextInput, TextInputType};

#[test]
fn arrow_up_on_first_row_converges_to_text_start() {
    let sig = Signal::new(String::from("abc\ndef"));
    let mut h = TestHarness::new(400.0, 200.0);
    let id = h.mount(TextInput::new(sig.clone()).input_type(TextInputType::Multiline));
    h.run_frame();
    h.click(id);
    h.run_frame();

    // Cursor starts at end ("...def"). Press Up → lands on first row (~col 3).
    h.press_key(Key::ArrowUp, Modifiers::default());
    h.run_frame();
    // Press Up again → already on first row → must converge to position 0.
    h.press_key(Key::ArrowUp, Modifiers::default());
    h.run_frame();

    // Insert a marker char; if cursor converged to start, it prepends.
    h.type_text(id, "X");
    h.run_frame();

    assert_eq!(
        sig.read(),
        "Xabc\ndef",
        "Up on first row must converge cursor to text start (got {:?})",
        sig.read()
    );
}

#[test]
fn arrow_down_on_last_row_converges_to_text_end() {
    let sig = Signal::new(String::from("abc\ndef"));
    let mut h = TestHarness::new(400.0, 200.0);
    let id = h.mount(TextInput::new(sig.clone()).input_type(TextInputType::Multiline));
    h.run_frame();
    h.click(id);
    h.run_frame();

    // Move cursor to start first (Up twice → converges to 0).
    h.press_key(Key::ArrowUp, Modifiers::default());
    h.run_frame();
    h.press_key(Key::ArrowUp, Modifiers::default());
    h.run_frame();

    // Now press Down → to last row; Down again → converge to end.
    h.press_key(Key::ArrowDown, Modifiers::default());
    h.run_frame();
    h.press_key(Key::ArrowDown, Modifiers::default());
    h.run_frame();

    h.type_text(id, "X");
    h.run_frame();

    assert_eq!(
        sig.read(),
        "abc\ndefX",
        "Down on last row must converge cursor to text end (got {:?})",
        sig.read()
    );
}

#[test]
fn arrow_down_from_first_row_start_reaches_second_row_start() {
    // Regression for the over-eager boundary convergence: pressing Down from
    // the FIRST row's start must land on the SECOND row's start — NOT collapse
    // to the first row's end. (cur_row==0 is not the last row, so Down must
    // advance, not converge.)
    let sig = Signal::new(String::from("abc\ndef"));
    let mut h = TestHarness::new(400.0, 200.0);
    let id = h.mount(TextInput::new(sig.clone()).input_type(TextInputType::Multiline));
    h.run_frame();
    h.click(id);
    h.run_frame();

    // Converge to text start (Up twice).
    h.press_key(Key::ArrowUp, Modifiers::default());
    h.run_frame();
    h.press_key(Key::ArrowUp, Modifiers::default());
    h.run_frame();

    // Down from first-row start → second-row start (position 4, before 'd').
    h.press_key(Key::ArrowDown, Modifiers::default());
    h.run_frame();

    // Insert marker; correct convergence → "abc\nXdef", NOT "abcX\ndef".
    h.type_text(id, "X");
    h.run_frame();

    assert_eq!(
        sig.read(),
        "abc\nXdef",
        "Down from first-row start must reach second-row start, not first-row end (got {:?})",
        sig.read()
    );
}

#[test]
fn arrow_down_can_land_on_empty_line() {
    // Bug 2: the cursor must be able to STOP on an empty line, not skip it.
    // "abc\n\ndef": line 0="abc", line 1="" (empty), line 2="def".
    let sig = Signal::new(String::from("abc\n\ndef"));
    let mut h = TestHarness::new(400.0, 200.0);
    let id = h.mount(TextInput::new(sig.clone()).input_type(TextInputType::Multiline));
    h.run_frame();
    h.click(id);
    h.run_frame();

    // Converge to text start.
    h.press_key(Key::ArrowUp, Modifiers::default());
    h.run_frame();
    h.press_key(Key::ArrowUp, Modifiers::default());
    h.run_frame();

    // Down once → empty line (line 1). Insert here should land ON the empty line.
    h.press_key(Key::ArrowDown, Modifiers::default());
    h.run_frame();
    h.type_text(id, "X");
    h.run_frame();

    assert_eq!(
        sig.read(),
        "abc\nX\ndef",
        "Down must land ON the empty line (insert between the two newlines), got {:?}",
        sig.read()
    );
}

#[test]
fn arrow_down_through_empty_line_preserves_column() {
    // From line 0 col 3 ("abc|"), Down lands on the empty line (col clamps to
    // 0, its only position), and Down again restores column 3 on line 2
    // ("def|") — the desired column is preserved across the zero-width empty
    // line, matching standard editor behavior.
    let sig = Signal::new(String::from("abc\n\ndef"));
    let mut h = TestHarness::new(400.0, 200.0);
    let id = h.mount(TextInput::new(sig.clone()).input_type(TextInputType::Multiline));
    h.run_frame();
    h.click(id);
    h.run_frame();

    // Cursor starts at end (line 2 col 3). Two Ups → line 0 col 3.
    h.press_key(Key::ArrowUp, Modifiers::default());
    h.run_frame();
    h.press_key(Key::ArrowUp, Modifiers::default());
    h.run_frame();
    // Down → empty line; Down → line 2, column 3 preserved.
    h.press_key(Key::ArrowDown, Modifiers::default());
    h.run_frame();
    h.press_key(Key::ArrowDown, Modifiers::default());
    h.run_frame();
    h.type_text(id, "X");
    h.run_frame();

    assert_eq!(
        sig.read(),
        "abc\n\ndefX",
        "column 3 must be preserved through the empty line, got {:?}",
        sig.read()
    );
}
