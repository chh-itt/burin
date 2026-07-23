# A Word from the Author

## Why Burin

Everyone building a GUI framework is trying to answer the same question:

> **"How should we write interfaces?"**

Over the past four decades, each generation of frameworks has given its own answer, and each has pushed some boundary forward. We respect all of them.

Burin doesn't exist because others got it wrong. It exists because we wanted to try a different direction.

---

## On Those Who Came Before

**Flutter** proved that retained-mode UI with incremental updates works at scale. Its declarative diff paired with relayout and repaint boundaries remains one of the most complete cross-platform GUI architectures ever built. Without Flutter blazing that trail, many of our design decisions would lack a reference point.

**Iced** brought the Elm Architecture into Rust. The `Model → update(Msg) → view()` loop is elegant in the right context, and its type-safe message dispatch is one of the best guarantees Rust can offer.

**Slint** uses a DSL compiler to generate code, with runtime dependency tracking via `Property<T>` — `get()` auto-registers dependencies, `set()` recursively marks downstream bindings dirty with lazy re-evaluation. At the compiler level, constant folding and binding loop detection are performed, but the dependency graph is built and updated at runtime. Under the hood, this mechanism is remarkably similar to Burin's Signal system. The difference: Slint requires a DSL + compiler to generate this code, which gives it multi-language host support (Rust / C++ / JS / Python) and compiler-level static analysis. The cost: UIs must be written in `.slint` files — no pure Rust composition.

**Egui** proved that immediate mode can be blazingly productive for prototypes and tools — zero lifecycle management, zero state synchronization, scripting-like ergonomics.

**SolidJS and Leptos** showed that runtime fine-grained reactivity works for the web: no virtual DOM diff, automatic subscription on read, precise notification on write. That insight shaped our thinking deeply.

None of these are "competitors." They are explorers on different paths.

---

## We Chose a Different Path

Burin's core hypothesis is simple:

> **What happens if you don't adapt Rust to an existing GUI paradigm, but let a GUI paradigm grow out of Rust itself?**

Iced asks: "How does Elm run in Rust?"
Xilem asks: "How does React / SwiftUI run in Rust?"
Slint asks: "How does a compile-time DSL run across multiple languages?"

Burin asks a different question:

> **Rust's ownership, composition, and zero-cost abstractions already suggest a GUI architecture. Build that architecture.**

This hypothesis led to three design choices:

### 1. Signals instead of messages

The Elm Architecture's `update(Msg)` is modular in Elm. In Rust, message enums must be defined centrally — every new widget adds a variant to the same enum, every state change routes through the same `update` function. Elegant for small apps; a single-point coupling nightmare at scale.

We use `Signal<T>` instead:
- `Signal::read()` auto-subscribes
- `Signal::set()` auto-notifies all subscribers
- When an Element is dropped, subscriptions are cleaned up automatically

No central message enum. No central `update` function. State lives in individual `Signal`s. UI composition is logic composition.

### 2. Reactivity that spans the entire pipeline

SolidJS proved runtime reactivity works for the web. But its tracking ends at DOM operations — the browser's internal layout, painting, and compositing are a black box.

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

Because we own the entire pipeline, the reactive system doesn't just tell the framework "what changed." It tells the layout engine "which nodes need re-layout," the cache system "which subtrees can be reused," and the paint layer "which regions don't need re-recording."

This isn't about being clever. It's about **Rust giving us control that browsers will never give web frameworks**.

### 3. Pure Rust. No DSL. No macro DSL. No code generation.

Burin UIs are plain Rust function calls:

```rust
VStack::new()
    .gap(12.0)
    .push(Text::new("Hello").font_size(24.0))
    .push(Button::new("Click me").on_click(|| println!("clicked")))
```

No template language to learn. No code generation to wait for. Autocomplete, go-to-definition, and refactoring all work out of the box — because the editor sees ordinary Rust code.

This isn't "we hate DSLs." DSLs have immense value in the right context (Slint proved this for embedded). But our bet is: **when the language itself is expressive enough, a DSL's learning cost and tooling fragmentation aren't worth paying.**

---

## Auralis: A Reactive Kernel in Rust

Burin's reactivity comes from [Auralis](https://github.com/chh-itt/auralis) — a standalone open-source kernel of three crates with zero platform dependencies.

`auralis-signal` is roughly 1,300 lines at its core. `Signal<T>` is internally `Rc<RefCell<>>` — no dependency graph topological sort, no Clean/Check/Dirty state machine, no arena allocation. Read pushes a subscriber; write iterates and notifies. You can understand the entire implementation over a cup of coffee.

This isn't about "fewer lines is better." It's a bet: **simple means understandable. Understandable means trustworthy. Trustworthy means you can bet your production system on it.**

Auralis is released independently — you can use it for anything, not just GUIs. SSR, CLI tools, game logic, embedded devices. Burin is simply one consumer of Auralis.

---

## We Are Not a Replacement

Burin has its sweet spot, and there are places where it's the wrong tool:

**Good fit:**
- Medium-to-large desktop applications — 60 built-in widgets, Material 3 theming, standard Taffy layout
- Scenarios needing GPU + CPU fallback — one Painter API, automatic CPU fallback when GPU is unavailable
- Projects demanding high test coverage — TestHarness makes headless GUI testing feasible
- Applications needing precise gesture handling — 7 recognizers in a single GestureArena

**Not a good fit:**
- Quick prototypes → Use Egui. Immediate mode's simplicity can't be beaten for throwaway UIs.
- Embedded / MCU → Slint's compile-time optimization + `#![no_std]` is the right answer.
- Cross-platform projects with an existing Dart team → Flutter's ecosystem is irreplaceable.
- Web-first → Dioxus/Tauri's WebView approach is more pragmatic.

---

## An Honest Closing

We can trace the entire path from `Signal::set()` to GPU command submission and know exactly what changed and what didn't — not because we're more talented, but because **Rust's type system and ownership model provide guarantees that other languages must maintain through sheer engineering effort**:

- Who owns subscriptions? What manages their lifecycle? → Rust: `Rc` + `Drop`. No manual tracking.
- When is state mutable? How is thread safety ensured? → Rust: `Cell`/`RefCell` + `!Send + !Sync`. Compile-time guarantees.
- How does dirty marking propagate? Can pointers dangle? → Rust: `ElementId` indexing + arena allocation. The borrow checker guards the rest.

Maybe following Rust's grain is the right direction. Maybe it isn't. We don't know yet.

Whether that direction is right — time and you will tell.

---

*Burin is still early. If you'd like to explore what Rust-native GUI should look like together — issues, PRs, or just a conversation — you're welcome.*

---

## Postscript: What We Found After Writing This

After drafting this piece, we went back to read the source code of Flutter, Slint, and Xilem — to make sure we hadn't misrepresented them, and to check whether our own claims held up.

We found something we didn't expect.

Flutter's `BuildOwner._dirtyElements` + `PipelineOwner._nodesNeedingLayout`, Slint's `Property<T>` runtime dependency tracking, and our `Signal<T>` + `dirty_registry` — all three share the same fundamental dirty-marking model beneath the surface: **O(1) mark + O(k) process + boundary short-circuit**.

The difference isn't "who is more advanced." The difference is how each arrived there:
- Flutter got there through years of Google engineering and millions of lines of code
- Slint got there through a DSL compiler that generates `Property<T>`-based code
- Burin got there with just Rust's `Signal<T>` + ownership + `Drop`, reaching the same architectural quality

This isn't design cleverness. It's Rust's language capabilities having already paved this road — we simply walked it first.

If this observation holds up under community scrutiny — then maybe, just maybe, we've stumbled onto a path that was always meant for Rust.

We didn't find a shortcut. The shortcut was always there.

*This document and the majority of Burin's source code were co-authored with DeepSeek.*
