# The Burin Pipeline

## Why Every GUI Framework Needs a Diffing Strategy

Every retained-mode GUI framework must answer one question: **"what changed, and
what needs to be re-drawn?"**

| Framework | Strategy | Cost |
|-----------|----------|------|
| Flutter | Three trees (Widget → Element → RenderObject) + four dirty markers | O(N) element diff per build |
| React | Virtual DOM + reconciliation | O(N) virtual tree diff |
| Iced | Elm architecture: `Message → update → view()` | O(N) tree rebuild per message |
| GPUI | Per-frame full element tree rebuild | O(N) rebuild every frame |
| Xilem | `View::rebuild(prev, new, state)` | O(N) view comparison |

These strategies exist because JavaScript, Dart, and Elm lack a reactive
primitive. JavaScript has no built-in way to say "this variable changed, notify
its dependents." Dart has `ChangeNotifier` but tracking is manual. Elm's purity
model forbids side-effects entirely. Rust has `Signal<T>` to fill that gap.

---

## `Signal<T>` in Rust

[`auralis-signal`](https://github.com/chh-itt/auralis) provides one primitive
that solves the diffing problem:

```rust
let count = Signal::new(0);

// Reading auto-subscribes the CURRENT observer scope.
// If this read happens inside a paint function, the element auto-subscribes.
let current = count.read();       // subscribes

// Writing auto-notifies every subscriber.
count.set(1);                     // notifies → marks element dirty
```

Two properties make this work:

1. **Read-time subscription.** `Signal::read()` checks whether an observer is
   currently active (`observe_element`, `src/core/signal_bridge.rs:230`). If so,
   the signal registers the element as a dependent. No manual wiring needed.

2. **Write-time notification.** `Signal::set()` traverses the subscriber list and
   fires each callback. Burin's callback is `register_dirty(eid, REPAINT)`.

This is fundamentally different from:
- **Flutter's `setState`**: manual trigger, marks the whole widget subtree.
- **Iced's `Message`**: type-level routing, requires boilerplate for every widget.
- **GPUI's `cx.notify()`**: manual notification, per-entity.
- **Ribir's `Stateful<T>` / rxrust**: Rx-based, tracks field-level changes but
  requires explicit `part_writer` setup.

Burin's `Signal<T>` is zero-boilerplate and auto-tracking, with O(1) notification.

---

## The Full Pipeline: One Direction, No Loops

```
┌─────────────────────────────────────────────────────────────┐
│  winit event loop (src/platform/window.rs:1569)              │
│                                                              │
│  Raw Input → EventTranslator → Event enum                    │
│       │                                                      │
│       ▼                                                      │
│  HitTest (src/event/hit_test.rs:10)                          │
│    → spatial_hit_test (O(1) spatial hash grid)               │
│    → fallback: hit_test_leaf (O(N), only when grid misses)   │
│    → returns HitTestResult { target, path (leaf→root) }      │
│       │                                                      │
│       ▼                                                      │
│  Gesture Arena (src/event/recognizer.rs)                     │
│    → process_pointer_event(phase, position, pointer_id)      │
│    → 7 recognizer types compete:                             │
│      Tap | Drag | EagerDrag | LongPress | DoubleTap |        │
│      Scroll | Custom                                         │
│    → arena sweep: one winner per pointer                     │
│       │                                                      │
│       ▼                                                      │
│  Propagation (src/event/propagation.rs:18)                   │
│    → dispatch_event(arena, event, hit_path, focus, registry) │
│    → PointerDown: propagate_pointer_down (leaf→root)         │
│    → KeyDown: dispatch_action (capture root→leaf,            │
│                then bubble leaf→root)                        │
│       │                                                      │
│       ▼                                                      │
│  EventRegistry callbacks                                     │
│    → on_click, on_drag_start, on_drag_update, on_key_down,   │
│      on_focus_in, on_hover_enter, on_long_press, ...         │
│                                                              │
│  ────── Inside a callback: user modifies a Signal ──────     │
│                                                              │
│  Signal::set(value)                                          │
│    → subscriber callback fires:                              │
│      → element.dirty |= REPAINT | MEASURE | REPOSITION       │
│      → app.register_dirty(eid, flags)       O(1)             │
│      → app.bump_subtree_gen(eid)            cache inval      │
│                                                              │
│  ═══════════════════ on_frame() ═══════════════════════════  │
│                                                              │
│  Phase 1: drive_frame_layout (src/core/frame_driver.rs)     │
│  ┌────────────────────────────────────────────────────────┐ │
│  │ Prepass: blink, frame_tick, scroll kinetic, portals    │ │
│  │                                                        │ │
│  │ process_dirty_set (src/layout/dirty_propagation.rs:11) │ │
│  │   → sort by depth (deepest first)                      │ │
│  │   → walk up ancestor chain for each dirty element      │ │
│  │   → MERGE DirtyFlags at each ancestor                  │ │
│  │   → STOP at containment boundaries:                    │ │
│  │     • affected_by_child_size == false → stop REPOSITION │ │
│  │     • size_independent → stop MEASURE                  │ │
│  │   → returns (paint_roots, has_measure, processed,      │ │
│  │              layout_roots)                              │ │
│  │                                                        │ │
│  │ Taffy incremental layout (4 paths):                    │ │
│  │   1. INCREMENTAL   — relayout only the dirty subtree   │ │
│  │   2. REPOSITION    — single-axis reposition             │ │
│  │   3. ESCALATE      — single-axis → full pass            │ │
│  │   4. FULL          — complete recompute (cold start)    │ │
│  │                                                        │ │
│  │ write_bounds → spatial grid update → hit test valid    │ │
│  └────────────────────────────────────────────────────────┘ │
│                          │                                   │
│  Phase 2: SEAM (platform: a11y, IME, drag, focus)           │
│                          │                                   │
│  Phase 3: drive_frame_paint (src/core/frame_driver.rs)     │
│  ┌────────────────────────────────────────────────────────┐ │
│  │ Animation tick → interpolate → apply → re-check dirty  │ │
│  │                                                        │ │
│  │ paint_element_tree (src/render/paint_tree.rs)          │ │
│  │   → iterate must_paint roots (the paint_roots from     │ │
│  │     process_dirty_set)                                 │ │
│  │   → for each element:                                  │ │
│  │     • dirty? → re-record DrawCommands + text areas     │ │
│  │     • clean? → replay from CachedSubtree (O(1))        │ │
│  │     • subtree_gen == cache_gen? → skip entire subtree  │ │
│  │                                                        │ │
│  │ Clear dirty flags                                      │ │
│  └────────────────────────────────────────────────────────┘ │
│                          │                                   │
│                          ▼                                   │
│  BackendRenderer (src/render/)                               │
│    → GPU: wgpu swapchain present                             │
│    → CPU: tiny-skia pixmap → softbuffer present              │
└─────────────────────────────────────────────────────────────┘
```

---

## What we don't have

### No Virtual DOM

A virtual DOM is a lightweight replica of the real DOM, used to compute what
changed. Burin has **no virtual DOM** because the real element tree IS the
source of truth. When a Signal changes, the element's properties are mutated
in place. There is nothing to diff.

- Flutter: Widget tree (ephemeral) → Element tree (persistent) → RenderObject tree → diff
- Burin: Element tree (persistent) → dirty flag → process

### No Dependency Graph

Many reactive systems maintain an explicit graph: Node A depends on B depends
on C. Burin has **no dependency graph**. Instead, each binding works by
**targeted push**:

During mount, when an element subscribes to a `Signal<T>`, the subscription
closure already knows its target `ElementId` and the specific dirty flag
(e.g. `REPAINT`). When `Signal::set()` fires, it directly calls:

```
register_dirty(element_id, REPAINT)
```

There is no lookup, graph traversal, or topological sort. The binding closure
hard codes where the update should land: the subscriber list is a flat vec,
not a dependency tree.

**Cost?** One closure per binding, stored in the element's `LifecycleComponent`.
When the element is destroyed, `Drop` unsubscribes automatically with no manual
cleanup or dangling callbacks. The executor batches deferred notifications
at the start of each frame, so multiple `set()` calls within one frame merge
into a single `process_dirty_set` pass.

In practice, this means zero relationship maintenance overhead. Rust's `Drop`
handles all cleanup: no graph traversal, no GC, and no manual unsubscribe.
The signal pushes directly to the element. When the element is destroyed,
the subscription dies with it. Every binding relationship is statically determined at compile
time by Rust's type system and ownership model.

- Slint: `Property<T>` runtime dependency tracking via thread-local `CURRENT_BINDING`
- Ribir: rxrust subscription graph
- Burin: targeted push, where each subscriber knows its target `ElementId` at mount time

### No Reconciliation

Reconciliation compares an old tree with a new tree to find what changed.
Burin has **no reconciliation** because elements are mutated in place. The
`DirtyFlags` bitmask records exactly what changed (MEASURE, REPOSITION, REPAINT)
and `process_dirty_set` propagates only as far as necessary.

### No Tree Rebuilding

In Elm-style or immediate-mode frameworks, every state change triggers a full
tree reconstruction. Burin is **retained mode**: elements are allocated once
during mount, and every subsequent update is an in-place mutation. The tree
topology never changes due to state updates.

---

## Ownership and RAII

Burin uses Rust's ownership model at three critical points:

1. **Element lifecycle tied to Signal subscriptions.**
   When an element binds to a Signal via `bind_label_lazy`
   (`src/core/signal_bridge.rs:167`), the subscription is stored in the element's
   `LifecycleComponent`. When the element is removed from the arena, dropping the
   component drops all subscription handles → no leaked callbacks, no ghost dirty
   writes for dead ElementIds.

2. **WeakSignal for async safety.**
   Async callbacks (timers, network responses) use `WeakSignal` to prevent
   use-after-free. If the element was torn down before the callback fires,
   `WeakSignal::upgrade()` returns `None` → the callback is a no-op.

3. **`forbid(unsafe_code)` everywhere except platform FFI.**
   `src/lib.rs:80`: the `#![forbid(unsafe_code)]` directive applies to all
   framework code. The only `unsafe` blocks exist in platform boundary crates
   (winit FFI, accesskit wrappers, clipboard).

---

## Why This Is Testable: TestHarness

The test harness (`src/testing/test_harness.rs`) runs the **exact same**
`drive_frame_*` functions as the production window. This isn't mock testing:
it's the real pipeline running headless.

```
Production:   winit event → dispatch_events → hit_test → dispatch_event
               → on_frame → drive_frame_layout → drive_frame_paint → GPU present

TestHarness:  manual input → hit_test → dispatch_event
               → run_frame → drive_frame_layout → drive_frame_paint → CPU rasterize
```

Key consequences:
- **Signal → dirty → paint** is verifiable via `assert_subtree_cache_hits` and
  `assert_frame_dirty_set_size`
- **Event routing** is verifiable via `click_at` → real `dispatch_event` → real
  `process_pointer_event` (gesture arena)
- **Gesture arbitration** was proven by `tests/gesture_audit.rs`, which found and
  fixed real production bugs in the arena
- **Perf regression** spans 15 dimensions: frame timings, viewport-bounded paint,
  signal latency, idle scaling, arena cleanup, cache boundedness, and causal
  tracing via DevTools `signal_element_links`

---

## Code: The Pipeline in One Function

```rust
use burin::prelude::*;
use auralis_signal::Signal;

fn pipeline_demo() -> impl Widget {
    Compositor::new(|_scope| {
        // ── Step 1: Create a Signal ──
        let text = Signal::new("Hello".to_string());

        // ── Step 2: Bind it to a Text widget ──
        //   During mount: bind_label_lazy() subscribes.
        //   On Signal::set(): register_dirty(eid, MEASURE|REPAINT)
        let display = Text::new("").bind(text.clone());

        // ── Step 3: A button that mutates the Signal ──
        let btn = Button::new("Change").on_click(move || {
            text.set("World!".to_string());
            // ── What happens next ──
            // 1. text.set() fires subscriber callback
            // 2. display.dirty |= MEASURE | REPAINT     (O(1))
            // 3. app.register_dirty(display_eid)         (O(1))
            // 4. Next frame: process_dirty_set → finds display
            //    → walks up to VStack → pct boundary → stop
            // 5. Taffy reposition (text width changed)
            // 6. SubtreeCache: siblings replay, display re-records
            // 7. GPU present
        });

        VStack::new().gap(8.0).push(display).push(btn)
    })
}
```

The framework handles everything from step 2 to step 7. The developer only
writes `Signal::new()`, `.bind()`, and `.set()`.

---

## Code comparison

```rust
// Flutter — manual setState, widget rebuild, element diff
// setState(() { _count++; }); // marks subtree dirty, rebuilds widget tree

// Iced — message enum, update function, view rebuild
// enum Message { Increment }
// fn update(&mut self, msg: Message) { ... }  // manual routing
// fn view(&self) -> Element<Message> { ... }  // full tree rebuild

// GPUI — per-frame full element rebuild via Window::draw
// fn render(cx: &mut WindowContext) -> impl IntoElement { ... }

// Burin — zero-boilerplate signal binding
let count = Signal::new(0);
Button::new("+1").on_click(move || count.set(count.read() + 1));
Text::new("0").bind(count);
// That's it. No setState, no Message enum, no per-frame rebuild.
```

---

## Further Reading

- [docs/PIPELINE.md](PIPELINE.md): this document
- [docs/testing.md](testing.md): TestHarness, snapshot regression, O(k) assertions
- [docs/getting-started.md](getting-started.md): installation, minimal app, feature flags
- [src/lib.rs](../src/lib.rs): module map and design principles
- [src/core/dirty_registry.rs](../src/core/dirty_registry.rs): the dirty propagation system
- [src/event/propagation.rs](../src/event/propagation.rs): event dispatch and action routing
- [tests/ok_assertions.rs](../tests/ok_assertions.rs): O(k) guarantee tests
