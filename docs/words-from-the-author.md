# A Word from the Author

## Why Burin

Everyone building a GUI framework is trying to answer the same question:

> **"How should we write interfaces?"**

Over the past four decades, each generation of frameworks has given its own answer, and each has pushed some boundary forward. We respect all of them.

Burin doesn't exist because others got it wrong. It exists because we wanted to try a different direction.

---

## On those who came before

**Flutter** proved that retained-mode UI with incremental updates works at scale. Its declarative diff paired with relayout and repaint boundaries remains one of the most complete cross-platform GUI architectures ever built. Without Flutter blazing that trail, many of our design decisions would lack a reference point.

**Iced** brought the Elm Architecture into Rust. The `Model → update(Msg) → view()` loop is elegant in the right context, and its type-safe message dispatch is one of the best guarantees Rust can offer.

**Slint** uses a DSL compiler to generate code, with runtime dependency tracking via `Property<T>`. `get()` auto-registers dependencies, and `set()` recursively marks downstream bindings dirty with lazy re-evaluation. At the compiler level, constant folding and binding loop detection run, but the dependency graph is built and updated at runtime. Under the hood, this mechanism is similar to Burin's Signal system. The difference: Slint requires a DSL and compiler to generate this code, which gives it multi-language host support (Rust, C++, JS, Python) and compiler-level static analysis. The tradeoff is that UIs must be written in `.slint` files, without pure Rust composition.

**Egui** proved that immediate mode can be productive for prototypes and tools, with no lifecycle management or state synchronization to maintain.

**SolidJS and Leptos** showed that runtime fine-grained reactivity works for the web, with automatic subscription on read and precise notification on write, without a virtual DOM diff.

None of these are "competitors." They are explorers on different paths.

---

## We chose a different path

Burin's core hypothesis is simple:

> **What happens if you don't adapt Rust to an existing GUI paradigm, but let a GUI paradigm grow out of Rust itself?**

Iced asks: "How does Elm run in Rust?"
Xilem asks: "How does React / SwiftUI run in Rust?"
Slint asks: "How does a compile-time DSL run across multiple languages?"

Burin asks a different question:

> **Rust's ownership, composition, and zero-cost abstractions already suggest a GUI architecture.**

This hypothesis led to three design choices:

### 1. Signals instead of messages

The Elm Architecture's `update(Msg)` is modular in Elm. In Rust, message enums must be defined centrally. Every new widget adds a variant to the same enum, and every state change routes through the same `update` function. What is elegant for small apps becomes a coupling nightmare at scale.

We use `Signal<T>` instead:
- `Signal::read()` auto-subscribes
- `Signal::set()` auto-notifies all subscribers
- When an Element is dropped, it cleans up subscriptions automatically

There is no central message enum or central `update` function. State lives in individual `Signal`s, and UI composition is logic composition.

### 2. Reactivity that spans the entire pipeline

SolidJS proved runtime reactivity works for the web. But its tracking ends at DOM operations. The browser's internal layout, painting, and compositing are a black box.

Burin's reactive tracking runs through the entire rendering pipeline:

```
Signal::set()
  → register_dirty(O(1))
  → process_dirty_set(O(k) ancestor walk)
  → Taffy incremental layout (MEASURE / REPOSITION / skip)
  → SubtreeCache check
  → paint only dirty subtrees
  → GPU / CPU
```

Because we own the entire pipeline, the reactive system tells the layout engine which nodes need re-layout, the cache system which subtrees can be reused, and the paint layer which regions do not need re-recording.

This is not about cleverness. It is about Rust giving us control that browsers will never give web frameworks.

### 3. Pure Rust, no DSL, no code generation

Burin UIs are plain Rust function calls:

```rust
VStack::new()
    .gap(12.0)
    .push(Text::new("Hello").font_size(24.0))
    .push(Button::new("Click me").on_click(|| println!("clicked")))
```

There is no template language to learn and no code generation to wait for. Autocomplete, go-to-definition, and refactoring all work out of the box because the editor sees ordinary Rust code.

To be clear, DSLs have value in the right context. Slint proved this for embedded. But our bet is that when the language itself is expressive enough, the learning cost and tooling fragmentation of a DSL are not worth paying.

---

## Auralis: a reactive kernel in Rust

Burin's reactivity comes from [Auralis](https://github.com/chh-itt/auralis), a standalone open-source kernel of three crates with no platform dependencies.

`auralis-signal` is roughly 1,300 lines at its core. `Signal<T>` is internally `Rc<RefCell<>>`, without dependency graph topological sort, Clean/Check/Dirty state machines, or arena allocation. Read pushes a subscriber; write iterates and notifies. You can understand the entire implementation over a cup of coffee.

This is not about "fewer lines is better." It is a bet: simple means understandable, understandable means trustworthy, and trustworthy means you can bet your production system on it.

Auralis is released independently, so you can use it for anything beyond GUIs, including SSR, CLI tools, game logic, and embedded devices. Burin is one consumer of Auralis.

---

## We are not a replacement

Burin has its sweet spot, and there are places where it's the wrong tool:

**Good fit:**
- Medium-to-large desktop applications: 60 built-in widgets, Material 3 theming, standard Taffy layout
- Scenarios needing GPU and CPU fallback: one Painter API, automatic CPU fallback when GPU is unavailable
- Projects that need high test coverage: TestHarness makes headless GUI testing feasible
- Applications that need precise gesture handling: 7 recognizers in a single GestureArena

**Not a good fit:**
- Quick prototypes: use Egui. Immediate mode is simpler for throwaway UIs.
- Embedded or MCU: Slint's compile-time optimization and `#![no_std]` support fit better.
- Cross-platform projects with a Dart team: Flutter's ecosystem is already established.
- Web-first: Dioxus or Tauri's WebView approach is more practical.

---

## An honest closing

We can trace the entire path from `Signal::set()` to GPU command submission and know exactly what changed and what did not. This is not because we are more talented, but because Rust's type system and ownership model provide guarantees that other languages must maintain through sheer engineering effort:

- Who owns subscriptions? What manages their lifecycle? Rust: `Rc` + `Drop` handles it. No manual tracking.
- When is state mutable? How is thread safety ensured? Rust: `Cell`/`RefCell` + `!Send + !Sync` give compile-time guarantees.
- How does dirty marking propagate? Can pointers dangle? Rust: `ElementId` indexing and arena allocation. The borrow checker guards the rest.

Maybe following Rust's grain is the right direction. Maybe it isn't. We don't know yet.

Whether that direction is right, you and time will tell.

---

*Burin is still early. If you would like to explore what Rust-native GUI should look like together, whether through issues, PRs, or just a conversation, you are welcome.*

---

## Postscript: what we found after writing this

After drafting this piece, we went back to read the source code of Flutter, Slint, and Xilem to make sure we had not misrepresented them, and to check whether our own claims held up.

What we found was unexpected. Flutter's `BuildOwner._dirtyElements` + `PipelineOwner._nodesNeedingLayout`, Slint's `Property<T>` runtime dependency tracking, and our `Signal<T>` + `dirty_registry` all share the same fundamental dirty-marking model beneath the surface: O(1) mark + O(k) process + boundary short-circuit.

The difference is not who is more advanced, but how each arrived there:
- Flutter got there through years of Google engineering and millions of lines of code
- Slint got there through a DSL compiler that generates `Property<T>`-based code
- Burin got there with just Rust's `Signal<T>` + ownership + `Drop`, and reached the same architectural quality

This is not about clever design. Rust's language capabilities paved this road, and we walked it first.

If this observation holds up under community scrutiny, then maybe we have stumbled onto a path that was always meant for Rust.

We did not find a shortcut. The shortcut was always there.

*This document and the majority of Burin's source code were co-authored with DeepSeek.*
