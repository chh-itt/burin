# Event System

## Event Types

```rust
pub enum Event {
    Click { position: Point, modifiers: Modifiers, finger_id: Option<u64> },
    PointerDown { position: Point, button: MouseButton, finger_id: Option<u64> },
    PointerMove { position: Point, finger_id: Option<u64> },
    PointerUp { position: Point, button: MouseButton, finger_id: Option<u64> },
    Scroll { delta_x: f32, delta_y: f32 },
    KeyDown { key: Key, modifiers: Modifiers },
    KeyUp { key: Key, modifiers: Modifiers },
}
```

## Hit Testing

`src/event/hit_test.rs:10` — Two-tier hit testing:

1. **`spatial_hit_test`** (O(1)): Spatial hash grid indexed by screen coordinates.
   Returns the deepest visible element at the point.
2. **`hit_test_leaf`** (O(N) fallback): Full-depth traversal. Only invoked when
   the spatial grid misses (element not yet registered or moved out of grid).

The result is `HitTestResult { target, path (leaf → root) }`.

## Propagation

`src/event/propagation.rs:18` — `dispatch_event()` routes events through the hit path:

- **Capture phase** (root → leaf): Actions (`KeyDown` → `dispatch_action`) traverse
  this direction first.
- **Bubble phase** (leaf → root): Most pointer events (`PointerDown`, `Click`)
  resolve at the deepest handler.

```rust
// Unit test proving: deepest handler wins
let path = [child, parent];  // leaf → root
propagate_click(&arena, &path, ...);
assert_eq!(fired.get(), "child");  // not "parent"
```

## Gesture Arena

`src/event/recognizer.rs` — 7 recognizer types compete in a single arena per pointer:

| Recognizer | Wins when |
|------------|-----------|
| `TapRecognizer` | Quick press-release without movement |
| `DragRecognizer` | 6px movement threshold crossed |
| `EagerDragRecognizer` | PointerDown (no threshold) |
| `LongPressRecognizer` | 500ms hold without movement |
| `DoubleTapRecognizer` | Two taps within 300ms |
| `ScrollRecognizer` | Touch drag on scrollable surface |
| `Custom` | User-defined logic |

Key guarantee: one `PointerDown` never fires both a tap and a long-press.

## Focus

```rust
// Focus traversal
h.press_key(Key::Tab, Modifiers::NONE);         // next focusable
h.press_key(Key::Tab, Modifiers::SHIFT);         // previous focusable

// Programmatic focus
h.focus_manager.set_focused(Some(element_id));
```

## Keyboard → Action

`src/event/bindings.rs` — `KeyBindingMap` maps chords to actions:

```
Ctrl+A → ActionKind::SelectAll
Ctrl+C → ActionKind::Copy
Ctrl+V → ActionKind::Paste
Ctrl+Z → ActionKind::Undo
Tab    → ActionKind::FocusNext
Enter  → ActionKind::Activate
Escape → ActionKind::Cancel
...
```

`dispatch_action` routes through capture → bubble. If unhandled, `Activate`/`NewLine`
fall back to `fire_click` on the focused element.

## Registering handlers

```rust
MouseRegion::new(widget)
    .on_click(|| {})
    .on_drag_start(|local, abs| {})
    .on_drag_update(|local, abs| {})
    .on_drag_end(|local, abs| {})
    .on_long_press(|| {})
    .on_hover_enter(|| {})
    .on_hover_leave(|| {})
    .on_scroll(|dx, dy| true)  // return true to consume
```
