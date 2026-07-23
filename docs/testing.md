# Testing

Burin provides `TestHarness` — a headless, window-free, GPU-free test driver that
runs the **exact same** `drive_frame_*` pipeline as the production window.

```rust
use burin::testing::TestHarness;
use burin::widgets::input::Button;
use burin::style::Point;

let mut h = TestHarness::new(800.0, 600.0);
let id = h.mount(Button::new("OK"));
h.settle(8);

// Click the button through the real gesture arena + propagation.
h.click_at(Point::new(40.0, 20.0)).run_frame();
h.assert_text(id, "OK");
```

## Core API

```rust
// Create
let mut h = TestHarness::new(width, height);
let id = h.mount(widget);

// Run frames
h.run_frame();                  // one frame
h.settle(max_frames);           // run until quiescent

// Interactions
h.click_at(pos);                // simulated click through real hit test + arena
h.pointer_down_at(pos);         // raw PointerDown (gesture arena activated)
h.pointer_move_at(pos);         // raw PointerMove
h.pointer_up_at(pos);           // raw PointerUp
h.hover_at(pos);                // full hit-test chain diff
h.drag(from, to);               // drag_start → drag_update → drag_end
h.scroll(id, dx, dy);           // scroll propagation
h.press_key(key, mods);         // keyboard → KeyBindingMap → dispatch_action
h.type_text(id, text);          // character-by-character input

// Time
h.advance_time(millis);         // advance virtual clock
h.advance_to_next_deadline();   // advance to scheduler's next deadline

// Signals
h.set_signal(&signal, value);   // set Signal value
h.read_signal(&signal);         // read Signal value

// Assertions
h.assert_text(id, "expected");           // text content
h.assert_visible(id);                    // visibility
h.assert_bounds(id, x, y, w, h);         // screen position
h.assert_focused(id);                    // focus state
h.assert_child_count(id, count);         // child count
h.assert_dirty(id);                      // dirty flag
```

## O(k) Performance Assertions

Unlike any other GUI test framework, Burin exposes quantitative performance
guarantees:

```rust
// Assert unchanged siblings replayed from cache.
h.assert_subtree_cache_hits(4);

// Assert layout stayed incremental (no full-pass escalation).
h.assert_no_relayout_escalation();

// Assert the dirty set processed this frame stayed small.
h.assert_dirty_set_size(10);

// Assert paint command count is bounded.
h.assert_paint_command_count(50);
```

These assertions verify the framework did O(k) **work**, not just produced
correct output.

## Snapshot Regression

```rust
use burin::assert_snapshot;

let mut h = TestHarness::new(400.0, 300.0);
h.mount(Button::new("Primary").primary());
h.settle(8);

assert_snapshot!(h, "button_primary");
```

Compares rendered pixels against `tests/snapshots/<name>.png`. Bless with
`AURALIS_UPDATE_SNAPSHOTS=1`. Requires `backend-tiny-skia` feature.

## Record / Replay

```rust
use burin::testing::TestRecorder;

// Record interactions.
let mut rec = TestRecorder::new(800.0, 600.0);
let id = rec.harness.mount(Button::new("Click me"));
rec.harness.find_mut(id).unwrap().set_test_id("btn");
rec.run_frame();
rec.click_on("btn");
let events = rec.into_events();

// Replay on a fresh harness.
let replayed = replay_events(
    |h| { h.mount(Button::new("Click me")); },
    &events,
);
```

## Perf Regression Suite

```bash
cargo test --profile bench --test perf_suite -- --ignored --nocapture
cargo test --profile bench --test perf_causal --features devtools -- --ignored --nocapture
```

12 dimensions: frame timings, viewport-bounded paint, hover idle, registry
lifecycle, structural invariants, text rebuild, startup cost, signal latency,
idle scaling, arena cleanup, cache boundedness, arena fragmentation.

4 causal dimensions (DevTools): signal→element causal links, frame diff
stability, layout oscillation detection.

## Production Equivalence

The harness runs the **exact same functions** as the real window:

| Subsystem | Production | Harness | Same code? |
|-----------|-----------|---------|:----------:|
| Frame pipeline | `drive_frame_layout` → `drive_frame_paint` | Same functions | ✓ |
| Hit test | `spatial_hit_test` → fallback | Same functions | ✓ |
| Event dispatch | `propagation::dispatch_event` | Same function | ✓ |
| Gesture arena | `process_pointer_event` | Same function | ✓ |
| Keyboard | `KeyBindingMap::find` → `dispatch_action` | Same functions | ✓ |
| Hover chain | Chain diff + leave/enter propagation | Same algorithm | ✓ |
| Accessibility | `build_accessibility_tree` | Same function | ✓ |

This means: if a test passes, the production code path is verified — not a mock.
