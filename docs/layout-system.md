# Layout System

Burin uses [Taffy](https://github.com/DioxusLabs/taffy) for Flexbox and CSS Grid
layout, with a custom incremental bridge that avoids full-tree recomputation.

## Dirty Flags

`src/core/element.rs` — Three levels of layout dirty:

```
REPAINT    = 0b001   Surface changed (color, border, text). No layout needed.
REPOSITION = 0b011   Position changed. Taffy reposition (single axis).
MEASURE    = 0b111   Size changed. Taffy full remeasure (both axes).
```

## Dirty Propagation

`src/layout/dirty_propagation.rs:11` — `process_dirty_set()`:

1. Collect dirty elements from the global dirty set.
2. Sort by depth (deepest first).
3. For each: walk up ancestors, merge `DirtyFlags`.
4. Stop at containment boundaries:
   - `affected_by_child_size == false` → stop `REPOSITION` upward
   - `size_independent` → stop `MEASURE` upward
5. Return `(paint_roots, has_measure, processed, layout_roots)`.

## Taffy Incremental Paths

| Path | Trigger | Cost |
|------|---------|------|
| `INCREMENTAL` | Reposition within relayout boundary | O(subtree) |
| `REPOSITION` | Single-axis position change | O(subtree) |
| `ESCALATE` | Boundary's dependent axis changed | O(subtree) → O(full) |
| `FULL` | Cold start, or forced | O(N) |

## Layout Boundaries

A widget is a **relayout boundary** when `affected_by_child_size == false`.
Children's size changes stop at that boundary — the parent and its siblings
are not re-laid out.

To make a widget a layout boundary:
```rust
el.set_affected_by_child_size(false);
```

## Common Layouts

```rust
// Flexbox
VStack::new().gap(8.0).push(a).push(b)
HStack::new().gap(4.0).push(a).push(b)

// Fixed size
SizedBox::new().width(200.0).height(100.0).child(widget)

// Expand to fill
Expanded::new(widget)

// Proportional flex
Flexible::new(2.0, widget)

// Grid
GridRow::new().columns(12)
    .push(GridItem::new(widget).cols(6))
    .push(GridItem::new(widget).cols(6))
```
