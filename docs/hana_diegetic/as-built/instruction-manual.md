# Panel Layout Instruction Manual

> **Reference manual** for the `hana_diegetic` layout engine and panel-drawing API.
> Goal: build and tune panels from this document without re-reading the engine source.
> Code anchors are given as `file:line` so you can jump to the implementation when you need a detail this manual omits.

The engine is a custom flexbox-like solver in `crates/hana_diegetic/src/layout/`. It is **not** Taffy. Its sizing rules mirror [Clay](https://github.com/nicbarker/clay) (see `engine/clay_parity.rs` for the parity tests). Everything in this manual is re-exported from `hana_diegetic` (see `layout/mod.rs`), so `use hana_diegetic::El;` etc.

---

## 1. Mental model

A panel is a tree of `El` (elements). You build the tree with a `LayoutBuilder`, the engine solves it into per-element boxes (`BoundingBox`, top-left origin, +Y down, in layout points), and a renderer turns boxes + draw data into meshes.

Each element has, along **each axis independently**:

- a **`Sizing`** rule (how big it wants to be),
- a position decided by its parent's **direction** (row / column / overlay) and **alignment**.

Sizing is resolved in **two passes** (`engine/sizing.rs`):

1. **Bottom-up `propagate_fit_sizes` / `propagate_fit_sizes_xy`** — post-order. Computes each element's *content size* (shrink-wrapped from children) and its *minimum size* floor. A `Fit` element gets its final size here.
2. **Top-down `size_along_axis`** — pre-order. Distributes the parent's resolved size to `Grow`/`Percent` children, compresses on overflow, and floors cross-axis children to their minimum.

You mostly do not call these — but understanding the split explains every sizing surprise (see §3 and §13).

---

## 2. Building a tree

```rust
use hana_diegetic::{El, LayoutBuilder, Sizing, Px, TextStyle};

// Root that shrink-wraps its content:
let mut b = LayoutBuilder::with_root(El::new().width(Sizing::FIT).height(Sizing::FIT));

b.with(El::column().gap(Px(4.0)), |b| {
    b.text("Title", title_style.clone());
    b.with(El::row().gap(Px(8.0)), |b| {
        b.text("left", style.clone());
        b.text("right", style.clone());
    });
});

let tree = b.build();
```

Builder entry points (`builder.rs`):

| Call | Use |
| --- | --- |
| `LayoutBuilder::new(w, h)` | Implicit **fixed-size** root of size `w`×`h`, then your content as its child. |
| `LayoutBuilder::with_root(el)` | Your `El` *is* the root (no implicit wrapper). Use with `El::new().width(FIT).height(FIT)` for a shrink-wrap panel. |
| `b.with(el, |b| { .. })` | Push `el` as a child and build its children inside the closure. |
| `b.text(s, style)` | Add a text leaf to the current parent. |
| `b.text(Text::new(s, style).id(id))` | Text leaf with a `PanelElementId` (for editable / addressable text). |
| `b.image(el, handle, tint)` | Image leaf. |
| `b.build()` | Finish, returns a `LayoutTree`. |

`b.with` returns `&mut Self`, so siblings chain. The "current parent" is whatever `with` you are inside.

> **Fit-root panels:** with `with_root(El::new().width(FIT).height(FIT))`, the root element is the visible panel surface. Fit-sized panel viewports resolve from the solved root bounds so root padding and borders stay inside the viewport. `LayoutResult::content_bounds()` still reports the root's first child (element index 1) when you need the inner content box (`layout_engine.rs`).

---

## 3. Sizing — the core (`sizing.rs`)

`Sizing` is set per axis: `El::new().width(..).height(..)` (or `.size(w, h)` for both).

| Constructor | Meaning |
| --- | --- |
| `Sizing::FIT` | Shrink-wrap to content, no floor, no cap. |
| `Sizing::fit_min(min)` | Shrink-wrap, never smaller than `min`. |
| `Sizing::fit_range(min, max)` | Shrink-wrap, clamped to `[min, max]`. |
| `Sizing::GROW` | Expand to fill leftover parent space (floor 0). |
| `Sizing::grow_min(min)` | Expand to fill, **guaranteed at least `min` even with no leftover space**. |
| `Sizing::grow_range(min, max)` | Expand to fill, clamped to `[min, max]`. |
| `Sizing::fixed(size)` | Exact size, ignores content and siblings. |
| `Sizing::percent(frac)` | Fraction `0.0..=1.0` of the parent's content area along the parent's layout axis (after padding + gaps). |

How each behaves across the two passes:

- **`Fit`** — final size is decided bottom-up: `clamp(content, min, max)`. Content = sum of children along the main axis, max of children across the cross axis, plus padding/border/gaps.
- **`Grow`** — bottom-up it is seeded to its content size (so overflow is visible); top-down it absorbs leftover space via the **smallest-first** equalising heuristic (`expand_children`). `min` is a hard floor it never drops below.
- **`Fixed`** — set immediately, never grows or shrinks. A `Fixed` *leaf* is the only childless element that contributes a non-zero content size to its parent without help.
- **`Percent`** — resolved top-down against the parent's content area; contributes its content size upward so `Fit` ancestors still account for it.

**Overflow / compression:** when children do not fit, a `Visible` (non-clipping) parent compresses resizable children **largest-first** down to their minimums (`compress_children`). A `Clipped` parent does not inflate to its content and reports only its own chrome upward (children overflow / scroll).

### The min-floor / Fit-reservation rule (important)

A `min` floor (`grow_min`, `fit_min`) is **only** a guarantee that the element will not be *smaller* than `min`. For a childless leaf it also reserves that `min` as content so a `Fit` ancestor widens to include it (`propagate_fit_sizes` floors a childless leaf to its `min`, `sizing.rs:97`). This is what lets a `grow_min` cell keep a minimum size inside a shrink-wrap (`Fit`) panel even when every sibling has consumed the width — see the connector worked example in §13.

A `min` does **not** force a `Fit` *container* to grow past its children's content: a container's `Fit` size comes from its children's *content* sizes, while `min` floors only feed the container's own minimum, which engages only if an ancestor would otherwise squeeze it below that minimum.

### One fixed outer box; `Fit`/`Grow` inside — never `Grow`×`Grow` over stacked content

**Fix size on the outer box only (the panel), then size every interior element with `Fit` or `Grow` — and give each interior element a *single* growing axis.** Pinning an interior box on *both* axes with `Grow` is the most common way to make a panel's borders overflow even when the frame has obvious empty space. Agents reach for `El::column().width(Grow).height(Grow)` reflexively; on a container that stacks real content (a title, a caption, several rows) it is a trap.

Why it breaks (`engine/sizing.rs`):

- The bottom-up `propagate_fit_sizes` pass runs **before** widths/heights are distributed. A `Grow` element has no resolved size yet, so it seeds toward **0** content (the pass even notes this at `sizing.rs:77`). A `Grow`-height container whose children are themselves `Grow`-height therefore computes a **collapsed** natural/content height — far smaller than what it will actually render.
- The top-down cross-axis pass then floors each child with `MAX(min, MIN(child, max))` (`size_children_cross_axis`, `sizing.rs:579`). Fed the collapsed/foreign reference, the box is resolved to a size that does **not** match the space it occupies, and its border quad — drawn at the element's solved box edges (`render/panel_geometry.rs`), with no clip to the panel — is placed *past the panel edge*. Symptom: the child's top border is clipped and its bottom spills below the frame, while the panel itself still has empty room.

The fix is to give interior boxes **one** growing dimension and let the other be `Fit`:

```rust
// WRONG — both axes Grow on a container that stacks text/rows.
El::column().width(Sizing::GROW).height(Sizing::GROW)   // border overflows

// RIGHT — grow across the parent, fit to content along the stack.
El::column().width(Sizing::GROW).height(Sizing::FIT)    // border stays contained
```

With `height(FIT)` the bottom-up pass measures the real stacked height, so the box (and its border) is contained and any leftover panel height simply stays empty below it.

`Grow`×`Grow` *is* fine for a leaf-ish box whose content is trivial — e.g. a swatch card holding one centered label — because its content minimum is tiny and can never exceed the parent. The trap is specifically a `Grow`-height **container of stacked children**. When in doubt: fixed outer, one growing axis inside.

**The mirror trap on the width axis: a `Grow`-width row that holds inline text.** `width(GROW).height(FIT)` is the safe pattern for a *stacking* container, but if the box is a `row` whose children are text runs, the same "`Grow` seeds toward 0" rule bites the *width*: in the fit pass the row resolves to ~0 width, so each text run wraps **per word** and the row's `Fit` height is measured as a tall multi-line column. The case/cell then balloons vertically even though every box is already `height(FIT)`. The tell is one sub-tree (the one wrapping its values in a `Grow`-width row) overflowing while a sibling that places text directly in its column stays compact.

```rust
// WRONG — inline text runs in a Grow-width row: wrap at ~0 width → tall.
El::row().width(Sizing::GROW).height(Sizing::FIT)   // case height balloons

// RIGHT — measure each run at its intrinsic single-line width, pack left.
El::row().width(Sizing::FIT).height(Sizing::FIT)    // one line, contained
```

Use `Fit` width for a row of inline text. Reach for `Grow` width on a text row only if you actually want the runs justified across the full width *and* you accept that long runs will wrap; for captioned value cells, `Fit` is correct.

---

## 4. Direction, alignment, gap

Three layout directions (`builder.rs`):

- `El::row()` (alias `El::new()`) — children flow left→right; **main axis = X**.
- `El::column()` — children flow top→bottom; **main axis = Y**.
- `El::overlay()` — children stack in the same box (z-order = child order); both axes are "overlay".

Gap (space between children along the main axis): `.gap(Px(n))` on `row()`/`column()`.

Alignment of children inside the parent:

- `.align_x(AlignX::{Left,Center,Right})`
- `.align_y(AlignY::{Top,Center,Bottom})`
- `.alignment(x, y)` for both.

Along the **main** axis, alignment distributes the leftover space before/after the run of children. Along the **cross** axis, it positions each child within the content area. (E.g. in a `row`, `align_y` centers each child vertically; `align_x` shifts the whole row.)

---

## 5. Padding, border, dividers

- `.padding(Padding::all(n))` / `Padding::xy(x, y)` / `Padding::new(l, r, t, b)` — interior inset; child `Percent` is computed against the post-padding content area.
- `.border(Border::all(width, color))` then `.left(w)/.right(w)/.top(w)/.bottom(w)` to override per side. Border insets content like padding.
- `.child_divider(ChildDivider::new(width, color))` — draws a divider line between consecutive children of this element (used for table-row rules). The divider occupies the gap; give the element enough gap so the rule is not cramped against the rows.

---

## 6. Background, corners, material, z-index

- `.background(Color)` — fill color.
- `.corner_radius(CornerRadius::all(r))` or `CornerRadius::new(tl, tr, br, bl)`.
- `.material(StandardMaterial)` — PBR material for the element fill (panels are physical; they respond to scene lighting).
- `.z_index(DrawZIndex)` — draw-order override.
- `.clip()` — clip children to this element's box (enables scrolling; see §9).

---

## 7. Units & `Dimension`

Any size argument is `impl Into<Dimension>`:

- **Bare `f32`** — layout units, scaled by the panel's layout-to-points factor at resolve time.
- **`Px(n)`** — explicit layout points; the common explicit unit for screen panels.
- **`Pt`, `Mm`, `In`** — physical units, for paper-sized / real-world-scaled panels (`PaperSize`, `PanelSize`).

Use `Px` (or bare `f32`) for screen UI. Reach for `Pt`/`Mm`/`In` only when the panel is anchored to a physical size.

---

## 8. Drawing on an element — `PanelDraw` (`draw.rs`, `line.rs`)

`.draw(PanelDraw::lines(..))` or `.draw(PanelDraw::shapes(..))` attaches vector geometry to an element. **Critical property:** `PanelDraw` content does **not** participate in layout measurement and resolves in the element's **local coordinate space** (against that element's resolved box). Draw geometry never changes the element's size.

### Coordinates — `PanelCoord` / `PanelPoint`

A point is `PanelPoint::new(x, y)` where each axis is a `PanelCoord`:

| `PanelCoord` | Resolves to |
| --- | --- |
| `PanelCoord::start(d)` | `d` inward from the **left/top** edge. |
| `PanelCoord::end(d)` | `d` inward from the **right/bottom** edge. |
| `PanelCoord::percent(f)` | fraction `f` of the element's size on that axis (overflow allowed; non-finite → 0, or use `try_percent`). |

Because coordinates resolve against the owning element's box, a line authored as `start(g) → end(0)` automatically spans "from `g` inside the left edge to the right edge" **at whatever width the element ends up** — this is how variable-length connectors work (§13).

### Lines — `PanelLine`

```rust
PanelLine::new(PanelPoint::new(start_x, start_y), PanelPoint::new(end_x, end_y))
    .width(Px(1.0))
    .color(color)
    .start_cap(CalloutCap::None)          // default None
    .end_cap(CalloutCap::arrow().solid()) // arrowhead at the end
    .cap_size(Px(5.0))
    .start_inset(Px(2.0))                 // pull the endpoint in along the line
    .end_inset(Px(2.0))
```

### Caps — `CalloutCap` (`callouts/caps.rs`)

`CalloutCap::arrow()`, `::circle()`, `::square()`, `::diamond()`, `::None`. Modify with `.solid()` / `.open()`, and `.size(n)` / `.width(n)` / `.height(n)` / `.radius(n)` / `.length(n)` / `.color(c)`.

### Shapes — `PanelShape`, `PanelCircle`

`PanelDraw::shapes([..])` for circles and other `PanelShape` primitives (`PanelCircle::new(center, radius)`). See `as-built/panel-shape-api.md` and `as-built/callouts.md` for the full primitive set.

### If you want a shape centered on a panel edge/corner that overflows the frame

(e.g. an anchor marker sitting on a corner so part of it spills outside the panel.)

1. Author it as a `PanelDraw` shape, **not** a layout `El` child. Layout children are positioned *inside* the parent's content box; there is no negative offset, and forcing one via negative `padding` inflates the box and pushes the panel's own border off-screen. `PanelDraw` does not participate in measurement, so it can paint past the frame without disturbing layout.
2. Call `.overflow(DrawOverflow::Visible)` on the `PanelDraw` — by default a draw clips to its owner box (`DrawOverflow::Clipped`); `Visible` lets it spill past the frame (clipped only to the viewport / clipped ancestors).
3. Place the shape against the element's **full box** (coords include the border): a disc at `PanelCircle::new(PanelPoint::new(start(0), start(0)), r)` centers on the top-left outer corner; use `end(0)` / `percent(0.5)` for other edges/center.

### Invisible-when-idle pattern

To show/hide drawn geometry **without** triggering relayout, author the geometry every frame and vary only its **color** — use `Color::NONE` (fully transparent) for the idle state. A color-only change classifies as `VisualOnly` (§12), so the cached layout is reused. Do **not** add/remove lines to toggle visibility (that is a structural change).

---

## 9. Clipping & scrolling

- `.clip()` — clip children to the box. A clipped container does not inflate to its content (`sizing.rs:157`), so children can overflow and scroll.
- `.scroll_y(offset)` / `.scroll_y_from_end(scrollback)` / `.scroll_x(offset)` — scroll the clipped content.

A scroll container needs a **bounded** height ancestor (fixed or `Percent`) — never `Grow` height — or there is nothing to clip against. The clip box must not be inflated by AA padding (engine handles this; relevant if you debug clipped lines).

---

## 10. Text — `TextStyle` (`text_props.rs`)

```rust
let style = TextStyle::new(LABEL_SIZE)         // size: impl Into<Dimension>
    .with_color(color)
    .no_wrap()                                  // or .wrap(TextWrap::..)
    .with_weight(FontWeight::..)
    .with_slant(FontSlant::..)
    .with_align(TextAlign::..)
    .with_shadow_mode(GlyphShadowMode::None)
    .with_render_mode(GlyphRenderMode::..)
    .with_lighting(Lighting::..)
    .with_sidedness(Sidedness::..);
```

A text element's measured size feeds the `Fit` content size (its min **height** is the measured height — text cannot compress vertically; its min **width** is 0 because text wraps unless `.no_wrap()`). Add text with `b.text(s, style)`.

---

## 11. Editable & addressable fields

- `.editable_field(id, ..)` on an `El`, or `Text::id(id)` (e.g. `b.text(Text::new(s, style).id(id))`) — give text a `PanelElementId` so it can be targeted for IME / inline editing (see `as-built/ime.md`).

---

## 12. The relayout fast-path — why recoloring is cheap (`element.rs`, `as-built/tree-change.md`)

When a panel rebuilds its tree (e.g. `set_tree` every interaction-state change), the engine diffs old vs new via `Element::classify_change` → `LayoutTreeChange`:

- **`VisualOnly`** — only paint-level fields changed (color, draw content, material). The cached geometry is reused; **no relayout**.
- **`LayoutAffecting`** — sizing, gap, padding, border, text content, child structure changed → relayout.

Consequences for panel design:

- Keep geometry **state-independent**: author the same elements/lines every frame and vary only color → every interaction highlight is `VisualOnly`.
- Changing **text content** is `LayoutAffecting` (the string is measured). A text-only edit that keeps the same character count/extent can still reuse layout via the geometry-stable skip, but treat text changes as potentially relayout-triggering.
- Adding/removing elements is structural — avoid it for transient visual states.

---

## 13. Worked example — variable-length connector lines

The camera-control panel draws a line from each input word to its intent, of variable length, invisible when idle. It composes several rules above. Geometry lives in the reusable `crates/fairy_dust/src/connector.rs`; the panel wires it in `camera_control_panel/layout.rs`.

Per word row: `[ word (FIT) ][ feeder cell (grow_min(MIN)) ]` inside a `Fit`-width panel.

- The **feeder cell** is `grow_min(FEEDER_CELL_MIN)`. Because all word rows in a group are cross-stretched to the word column's width, and the column's width is `(longest word) + MIN`, a short word's feeder grows long and the **longest** word's feeder lands exactly at `MIN`.
- The **min-floor / Fit-reservation rule** (§3) is what makes this work in a shrink-wrap panel: a childless `grow_min` cell reserves its `MIN` as content, so the `Fit` panel widens by `MIN` and the longest word still gets a `MIN`-length feeder instead of collapsing to zero. (Before that rule was honored in the engine, the longest-word feeder collapsed and the line went straight to vertical.)
- The **feeder line** is `start(FEEDER_START_GAP) → end(0)` at `percent(0.5)` height — local-space, so it auto-spans the feeder cell's resolved width with a fixed leading gap after the word.
- A fixed-width **spacer cell** between the feeders and the single intent label holds the convergence: per off-center word a vertical riser `start(0) @ word_y% → start(0) @ 50%`, plus one shared trunk `start(0) @ 50% → end(TRUNK_END_GAP) @ 50%` carrying the arrow cap. `TRUNK_END_GAP` keeps the arrowhead clear of the intent.
- **Equal horizontals:** feeder visible length `= FEEDER_CELL_MIN − FEEDER_START_GAP`; trunk visible length `= SPACER_WIDTH − TRUNK_END_GAP`. Derive both from one `MIN_CONNECTOR_HORIZONTAL` constant so they stay equal.
- **Invisible when idle:** every line is authored every frame; idle color is `Color::NONE`, active color is the highlight. Recoloring is `VisualOnly` → no relayout (§12).
- **Group separation:** the column holding the group rows uses a *larger* gap than the within-group word gap, so the Orbit / Pan / Zoom groups read as separate blocks rather than one run of lines.

---

## 14. Recipes & gotchas

- **A `Grow` child collapsed to 0 in a `Fit` panel.** Expected: `Fit` panels have no leftover space to distribute, so plain `Grow` floors at 0. Use `grow_min(min)` to reserve a minimum (the reservation rule, §3).
- **`fit_min`/`grow_min` on a *container* did not widen it.** `min` floors the element's own minimum, not its content. Containers shrink-wrap to *children's content*; to reserve interior space, put the floor on a childless cell or use padding.
- **Scroll container shows nothing / does not clip.** It needs `.clip()` and a bounded-height ancestor (fixed/`Percent`), not `Grow` height (§9).
- **Highlight causes a visible relayout hitch.** Something other than color changed — text, sizing, or structure. Keep transient state to color-only (§12).
- **Divider rule cramped against rows.** Increase the element's `.gap()`; the divider sits in the gap (§5).
- **Line drawn off the element.** `PanelDraw` resolves in the element's *local* box; check you attached it to the element whose box you meant, and that `start`/`end`/`percent` are measured from the right edge.
- **Panel border vanished / geometry shifted after adding an edge marker.** You positioned an overflowing shape with a layout `El` child + (negative) padding, which inflates the box. Use the overflow recipe in §8 instead.
- **Percent child smaller than expected.** `Percent` is a fraction of the parent's *content* area (after padding and gaps), not its outer size (§7).
- **A bordered interior box overflows the panel (top border clipped, bottom spills out) even though the frame has empty space.** You gave that box `Grow` on *both* axes while it stacks real content. A `Grow` box seeds toward 0 in the bottom-up pass, so its height is mis-computed and the border is drawn past the panel edge. Fix the size on the *outer* box (the panel) only; give interior stacking boxes a single growing axis — `width(GROW).height(FIT)` (§3, "One fixed outer box").

---

## Anchors

- Sizing rules & helpers: `crates/hana_diegetic/src/layout/sizing.rs`
- Two-pass solver: `crates/hana_diegetic/src/layout/engine/sizing.rs`
- Builder & `El`: `crates/hana_diegetic/src/layout/builder.rs`
- Draw / lines / coords: `crates/hana_diegetic/src/layout/draw.rs`, `line.rs`
- Caps: `crates/hana_diegetic/src/callouts/caps.rs`
- Text: `crates/hana_diegetic/src/layout/text_props.rs`
- Change classification: `crates/hana_diegetic/src/layout/element.rs` (`classify_change`), `as-built/tree-change.md`
- Connector example: `crates/fairy_dust/src/connector.rs`, `crates/fairy_dust/src/camera_control_panel/layout.rs`
