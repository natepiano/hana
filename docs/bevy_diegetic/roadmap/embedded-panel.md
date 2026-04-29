# Embedded Panels — Design Skeleton

A `DiegeticPanel` embedded as a layout element inside another
`DiegeticPanel`. Each embedded panel is an independent layout island:
its own layout unit, font unit, text measurer context, and size
resolution pass. The parent panel treats it as an opaque-sized box.
Analogy: HTML `<iframe>`.

**Related documents:**
- `docs/panel-size-phase-1.md` — Design B‴ (completed).
- `docs/panel-size-phase-2.md` — Unit coupling + typestate builder
  (completed). Phase 3 note on line 315 is the seed of this doc.

---

## Motivating use cases

- **Paper comparison.** Side-by-side A4 (mm) and US Letter (inches)
  layouts within a single parent panel. Neither should contaminate the
  other's unit system.
- **Mixed-scale dashboards.** A world-space diegetic monitor whose
  bezels are modelled in meters but whose screen content is authored
  in pixels.
- **Reusable panel components.** A shared "status readout" panel
  designed once against its own unit conventions, dropped into
  multiple parent layouts without leaking its dimensions upward.
- **Heterogeneous font budgets.** One section rendered with parley
  shaping at sub-pixel precision while a sibling section uses a
  cached atlas — different font units, different measurers.

---

## Open questions

Each is blocking for a concrete design. Answers drive the API.

### 1. Structural model

- **A.** Embedded panel is a separate Bevy entity, parented via
  `ChildOf`; the outer element reserves layout space but the inner
  renders independently.
- **B.** Embedded panel lives in the same `LayoutTree` as a special
  `ElementContent::Panel { tree, layout_unit, font_unit, ... }`
  leaf, sized by the outer engine and internally re-laid each frame.
- **C.** Embedded panel is pre-built into a detached `LayoutTree`
  once and blitted into the outer tree as an opaque sized leaf.

Tradeoffs: A preserves entity-level composition but splits rendering
into two passes. B keeps rendering unified but doubles layout engine
complexity. C is cheapest but loses reactivity inside the embed.

### 2. Sizing contract

Who sizes whom?

- **Outer-sized.** Outer engine assigns dimensions to the embed slot
  (via `Sizing::Fixed`/`Fit`/`Grow`); inner engine lays out within
  those bounds. Inner `Fit` would require a two-phase handshake.
- **Inner-sized.** Inner engine computes its own content size; outer
  sees an opaque minimum and positions it. Conflicts with outer
  `Grow` siblings needing total-budget knowledge.
- **Negotiated.** Inner computes min/preferred/max, outer allocates
  within those bounds (CSS flex intrinsic sizing).

### 3. Unit isolation boundary

Inside an embed:
- `layout_unit` is the embed's own (e.g. mm in the A4 example).
- `font_unit` is the embed's own.
- `Dimension { unit: None }` values resolve against the *embed's*
  defaults, not the parent's.

Outside:
- The outer engine knows the embed only by its resolved world size
  in the outer panel's layout unit. The conversion happens at the
  embed boundary.

Unresolved: should `Dimension` carry a "unit source" so misuse (parent
dimensions leaking into child subtree) is a compile error? Probably
overkill — runtime invariant is enough given the outer engine never
reaches into the inner tree.

### 4. Rendering composition

- **Separate cameras + RTT.** Each embed renders to a texture, parent
  composites. Clean isolation, GPU cost.
- **Shared draw buffer, parent transform.** Inner geometry is
  generated in the inner unit system then transformed into parent
  space at emit time. Cheaper, trickier for text (MSDF atlas bucket
  sharing across measurers).
- **Single engine, no composition.** If we pick structural model B,
  the outer engine emits everything in one pass and no composition
  is needed — but then "unit isolation" is a contract, not an
  enforced boundary.

### 5. Text measurer scoping

Today `DiegeticTextMeasurer` is a single `Resource`. An embed may
want a different measurer (different font registry, different
shaping cache policy). Options:

- One global measurer; embeds carry a `MeasurerOverride` component.
- Per-panel measurer field on `DiegeticPanel`.
- Measurer pool keyed by some identifier.

### 6. Input / picking / focus

An embedded panel may have interactive elements. Does cursor picking
on the outer panel cascade into the embed? What coordinate space is
delivered? Out of scope for a first cut — document as "future work."

### 7. Invalidation / change propagation

If the inner panel changes, does the outer re-layout? Only if the
embed's resolved size changed. Requires an efficient
"embed-changed-size" signal rather than a blanket "any embed changed"
trigger.

---

## API sketch (tentative — pending Q1–Q7)

Two plausible shapes depending on answers above:

### Shape A — `El::embed(panel)`

```rust
let inner = DiegeticPanel::world()
    .paper(PaperSize::A4)
    .layout(|b| { /* ... */ })
    .build()?;

DiegeticPanel::world()
    .size(FitMax(Mm(500.0)), Fit)
    .layout(|b| {
        b.embed(inner);          // treated as a sized leaf
        b.embed(other_inner);
    })
    .build()?
```

### Shape B — Panel-as-component, ChildOf composition

```rust
commands.spawn((
    DiegeticPanel::world().size(Fit, Fit).layout(...).build()?,
    children![
        (DiegeticPanel::world().paper(PaperSize::A4).build()?, ...),
        (DiegeticPanel::world().paper(PaperSize::Letter).build()?, ...),
    ],
));
```

Shape A keeps everything in one builder closure — ergonomic but
forces the layout engine to know about panels. Shape B leans on
Bevy's existing hierarchy — natural but pushes layout coordination
into a system rather than the builder.

---

## What the first cut should deliberately exclude

- Interactive elements / picking / focus — future work.
- Scrolling embeds (viewport smaller than inner content with scroll
  offset) — related but separable problem.
- Depth / z-ordering between embeds and their parent's own content
  beyond natural layout order.
- Embedding a **screen** panel inside a **world** panel or vice
  versa — the mode boundary deserves its own design pass.

---

## Implementation plan placeholder

Once Q1–Q5 are answered, this section gets populated with:

1. Engine changes (new element variant or new entity-tree traversal).
2. Builder API changes (`El::embed` or equivalent).
3. Rendering changes (RTT pipeline if needed, otherwise emit-time
   transform).
4. Tests (unit isolation, resize propagation, negotiated sizing).
5. Example (paper-comparison demo as the canonical showcase).

---

## Decision log (empty)

Append entries as questions get answered:

- **[DATE]** Q1 resolved to (A/B/C) because …
- **[DATE]** Q2 resolved to … because …
