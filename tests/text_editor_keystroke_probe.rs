//! Probe: per-keystroke cost of `EditorState` text mutation at various
//! document sizes.
//!
//! History (bench profile, cursor at end = lower bound):
//! - Pre-A3 (chars().collect::<Vec<char>>() rebuild ×3 per keystroke):
//!   1k: 6.3µs / 10k: 33µs / 50k: 190µs / 200k: 842µs per insert_char.
//! - Post-A3 (byte-indexed in-place String edits):
//!   1k: 2.2µs / 10k: 22µs / 50k: 114µs / 200k: 582µs.
//!   Remaining cost is the two unavoidable full-text clones per keystroke
//!   (Signal<String>.set + undo snapshot) — memcpy-bound, not iteration.
//!   A rope + Rc-based snapshots would be the next (architectural) step.
//!
//! Run: cargo test --profile bench --test text_editor_keystroke_probe -- --ignored --nocapture --test-threads 1

use std::time::Instant;

use auralis_signal::Signal;
use burin::widgets::input::text_editor::state::{EditorState, TextInputConfig, TextInputType};

fn make_doc(chars: usize) -> String {
    // Mixed ASCII content with newlines every 80 chars (realistic-ish).
    let mut s = String::with_capacity(chars);
    let line = "The quick brown fox jumps over the lazy dog. 0123456789 abcdefghijklmnop. ";
    while s.chars().count() < chars {
        s.push_str(line);
        s.push('\n');
    }
    s
}

#[test]
#[ignore]
fn keystroke_cost_by_document_size() {
    for &size in &[1_000usize, 10_000, 50_000, 200_000] {
        let doc = make_doc(size);
        let sig = Signal::new(doc.clone());
        let mut config = TextInputConfig::default();
        config.input_type = TextInputType::Multiline;
        let state = EditorState::new(sig, config);

        {
            let mut st = state.borrow_mut();
            // Cursor at end-of-document. NOTE: this is the *best* case for
            // the Vec::insert shift (no tail to move) — the measured cost is
            // therefore a LOWER BOUND; mid-document editing is worse.
            st.move_to_end(false);
        }

        // Warm up.
        {
            let mut st = state.borrow_mut();
            st.insert_char('x');
            st.delete_backward();
        }

        const N: u32 = 50;
        let t0 = Instant::now();
        {
            let mut st = state.borrow_mut();
            for _ in 0..N {
                st.insert_char('x');
            }
        }
        let insert_avg = t0.elapsed() / N;

        let t1 = Instant::now();
        {
            let mut st = state.borrow_mut();
            for _ in 0..N {
                st.delete_backward();
            }
        }
        let delete_avg = t1.elapsed() / N;

        eprintln!(
            "doc {size:>7} chars | insert_char avg {insert_avg:>10.2?} | delete_backward avg {delete_avg:>10.2?}"
        );
    }
}
