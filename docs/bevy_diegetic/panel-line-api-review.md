# Panel Line API Plan - 2026-06-09

This is the canonical implementation plan for element-owned panel line drawing in
`bevy_diegetic`.

The plan covers the full path from API design through the two visible
consumers:
- replacing rectangle-backed rulers in `crates/bevy_diegetic/examples/units.rs`
- moving `crates/bevy_diegetic/examples/typography.rs` toward an integrated
  panel-line-backed typography overlay

Completion means:
- `El` can own paint-only `PanelDraw::lines(...)` data without changing
  measurement.
- Layout emits resolved panel-space line commands with stable identity,
  clipping, overflow, and paint-order semantics.
- The renderer draws panel-owned lines with production lifecycle behavior rather
  than one ad hoc entity per tick or guide.
- The `units` rulers use line primitives for ticks, with any retained spine
  rectangles explicitly documented.
- The typography overlay uses post-layout panel-local line records for metric
  guides and annotations, with old callout/gizmo paths removed or explicitly
  exempted.
- Planar callouts have an explicit transparent-panel-backed path so new draw
  features accumulate in one panel-owned model.

The review notes below record the decisions that shape the implementation
phases.

## Decisions

### 1. PanelDraw

Decision: keep `PanelDraw` as an element-owned, paint-only visual layer.

Conceptual contract:
- Layout gives the owning element a measured box through normal layout inputs.
- `PanelDraw` does not contribute intrinsic size and never changes layout measurement.
- Draw coordinates are related to the owning element's resolved box.
- Draw output stores an explicit overflow policy: clipped to the owning element by default, or allowed to overflow through `DrawOverflow::Visible`.

Notes:
- This supports ruler ticks inside a measured track.
- This also leaves room for callouts, arrows, and overlays that reserve one layout box but draw beyond it.
- This decision is provisional and can be revisited as implementation details surface.

### 2. RenderCommandKind::Lines

Decision: add a resolved line-drawing command to the layout render command stream.

Conceptual contract:
- `PanelDraw` is the authored element-level intent.
- After layout resolves element bounds, the layout engine emits a line render command with concrete panel-space geometry.
- The renderer consumes this command without needing to understand layout sizing, grow behavior, padding, or ruler tick division.

Notes:
- The exact payload can still evolve, but the render-command layer needs a first-class line variant or equivalent resolved draw command.
- This keeps line drawing parallel to existing rectangle, border, text, image, and clipping commands.

### 3. PanelLine

Decision: keep `PanelLine` as the authored line primitive for element-owned panel drawing.

Conceptual contract:
- A `PanelLine` has a start point, end point, and style.
- Start and end points are authored relative to the owning element's resolved box.
- Coordinate origin is the element's top-left.
- X grows right; Y grows down, matching layout coordinates.
- A line represents a centerline, not a filled rectangle. Stroke width expands around that centerline during rendering.
- By default, lines are clipped to the owning element's box and use the element/current command paint order.
- If the owning draw is configured to overflow, the line may paint outside the element box and should default to a front-of-panel overlay paint order.

Example:

```rust
El::new()
    .width(Sizing::fixed(Mm(40.0)))
    .height(Sizing::fixed(Mm(20.0)))
    .draw(PanelDraw::lines([
        PanelLine::new(
            PanelPoint::new(Mm(5.0), Mm(10.0)),
            PanelPoint::new(Mm(35.0), Mm(10.0)),
        )
        .width(Mm(0.3))
        .color(Color::WHITE),
    ]))
```

This draws a horizontal line from `(5mm, 10mm)` to `(35mm, 10mm)` inside the owning `40mm x 20mm` element.

### 4. LineStyle

Decision: keep `LineStyle` as the visual styling for `PanelLine`, including cap behavior.

Conceptual contract:
- `LineStyle` controls stroke width, color, cap size, start cap, and end cap.
- Cap behavior should reuse the existing callout cap model instead of creating a separate panel-line-only cap enum.
- The current reusable cap type is `CalloutCap`, which supports no cap, arrow, circle, square, and diamond.
- Panel lines need a dimension-aware cap-resolution boundary because existing `CalloutCap` size overrides are raw `f32` values from world/local callout space.
- We may rename or generalize `CalloutCap` later once panel lines share it, but duplication is worse than the temporary name mismatch.

Current sketch:

```rust
pub struct LineStyle {
    width: Dimension,
    color: Color,
    cap_size: Dimension,
    start_cap: CalloutCap,
    end_cap: CalloutCap,
}
```

Notes:
- Rulers will usually use no caps.
- Callouts/arrows can use `CalloutCap::arrow()` or other existing cap shapes.
- This folds the earlier separate `LineCap` idea into the shared callout cap model.
- The first panel-line implementation should either generalize cap override dimensions to `Dimension`, or restrict panel-line caps to cap shape plus `LineStyle::cap_size` until that generalization lands.
- `LineStyle::default()` should use white color, named positive defaults for width and cap size, `CalloutCap::None` at both ends, and zero insets on `PanelLine`.

### 5. PanelPoint

Decision: keep `PanelPoint` as the authored 2D point type for panel-local line endpoints.

Conceptual contract:
- `PanelPoint` stores an X coordinate and a Y coordinate.
- Points are interpreted relative to the owning element's resolved box.
- The coordinate space uses top-left origin, X growing right, and Y growing down.
- `PanelPoint` should support richer coordinate components than raw absolute values, such as right/bottom-edge offsets and percentages.

Current sketch:

```rust
pub struct PanelPoint {
    x: PanelCoord,
    y: PanelCoord,
}
```

Example forms:

```rust
PanelPoint::new(Mm(5.0), Mm(10.0))
PanelPoint::new(PanelCoord::End(Mm(0.0)), Mm(10.0))
PanelPoint::new(PanelCoord::Percent(0.5), PanelCoord::Percent(0.5))
```

### 6. PanelCoord

Decision: keep `PanelCoord` as the authored coordinate component used by `PanelPoint`.

Conceptual contract:
- `PanelCoord` resolves one X or Y value against the owning element's resolved box.
- `Start(value)` measures from the left edge for X or top edge for Y.
- `End(value)` measures inward from the right edge for X or bottom edge for Y.
- `Percent(value)` measures as a fraction of the resolved element size on that axis.
- Negative `End` values intentionally support drawing outside the element when draw overflow is visible.

Current sketch:

```rust
pub struct PanelCoord {
    kind: PanelCoordKind,
}

enum PanelCoordKind {
    Start(Dimension),
    End(Dimension),
    Percent(f32),
}
```

Examples:

```text
Start(5mm)    = 5mm from left/top
End(0mm)      = exactly on right/bottom edge
End(2mm)      = 2mm inward from right/bottom edge
End(-2mm)     = 2mm outside right/bottom edge
Percent(0.5)  = middle of the element on that axis
```

### 7. ResolvedPanelLine

Decision: keep a resolved line representation for renderer-facing line geometry.

Conceptual contract:
- Authored `PanelLine` uses `PanelPoint` endpoints and `LineStyle`.
- After layout resolves the owning element bounds, the layout/render-command layer resolves those endpoints into concrete panel-space positions.
- The renderer consumes resolved panel-space geometry and does not need to understand element-local coordinate expressions.
- Because `RenderCommandKind` is public, this type should be public and opaque if it appears in the public render stream.

Current sketch:

```rust
pub struct ResolvedPanelLine {
    source_key: PanelLineSourceKey,
    start: Vec2,
    end: Vec2,
    tip_start: Vec2,
    tip_end: Vec2,
    shaft_start: Vec2,
    shaft_end: Vec2,
    style: ResolvedLineStyle,
    owner_bounds: BoundingBox,
    visual_bounds: BoundingBox,
    clip: ResolvedLineClip,
    paint_order: LinePaintOrder,
    source_command_index: usize,
}
```

Notes:
- `RenderCommandKind` is public, so resolved line payload types must either be public opaque types with accessors or stay behind a crate-private command path. The current plan assumes public opaque resolved types.
- `source_key` is a stable layout identity separate from command index and paint order. It includes source element index, draw ordinal, line ordinal, and primitive ordinal where relevant.
- The renderer prefixes `source_key` with the panel entity to form a retained `PanelLineRenderKey`.
- `visual_bounds` includes stroke width and cap geometry so overflow lines are not culled by the owning element's bounds.
- `clip` captures the effective parent/panel clipping state at emission time, so overflow lines can be drawn in a front-of-panel order without losing inherited clipping.
- Authored endpoints are nominal cap-tip anchors. Insets move tips inward, cap geometry shortens the shaft from those tips, and overrun collapses the shaft deterministically.

### 8. Ruler line builder helpers

Decision: keep ruler line generation as example/domain helper code, not as part of the core panel drawing API.

Conceptual contract:
- Ruler helpers convert metric or imperial divisions into `PanelLine` values.
- The general reusable API remains `PanelDraw::lines(...)`.
- Rulers are one producer of panel-local line geometry, not the owner of the line drawing model.

Current helper direction:

```rust
fn metric_vertical_tick_lines(height_mm: i32, color: Color) -> Vec<PanelLine>
fn metric_horizontal_tick_lines(width_mm: i32, color: Color) -> Vec<PanelLine>
fn imperial_vertical_tick_lines(height_sixteenths: i32, color: Color) -> Vec<PanelLine>
fn imperial_horizontal_tick_lines(width_sixteenths: i32, color: Color) -> Vec<PanelLine>
```

Notes:
- These helpers should keep the `units` example readable.
- They should reuse existing tick-size functions where possible.
- They should not leak ruler-specific concepts into the core renderer.

## Implementation Plan

Intent: add intuitive panel-local line drawing so layout can allocate boxes while rendering uses proper line primitives instead of tiny background rectangles. The first visible consumer is the `units` ruler ticks, but the API should also support arrows and callouts that draw from an element without affecting layout measurement.

### Phase 1 - Authored Layout Types (Complete)

Committable unit: add the public authored API types without changing layout behavior or rendering.

Add panel-local draw types near the layout API:

```rust
pub struct PanelDraw {
    kind: PanelDrawKind,
    overflow: DrawOverflow,
}

enum PanelDrawKind {
    Lines(Vec<PanelLine>),
}

pub enum DrawOverflow {
    Clipped,
    Visible,
}

pub struct PanelLine {
    start: PanelPoint,
    end: PanelPoint,
    style: LineStyle,
    start_inset: Dimension,
    end_inset: Dimension,
}

pub struct LineStyle {
    width: Dimension,
    color: Color,
    cap_size: Dimension,
    start_cap: CalloutCap,
    end_cap: CalloutCap,
}

pub struct PanelPoint {
    x: PanelCoord,
    y: PanelCoord,
}

pub struct PanelCoord {
    kind: PanelCoordKind,
}

enum PanelCoordKind {
    Start(Dimension),
    End(Dimension),
    Percent(f32),
}
```

Requirements:
- Define authored types and constructors/builders.
- Place the authored API in the layout module, then re-export it through
  `layout/mod.rs` and the crate root so examples can use `bevy_diegetic::*`.
- Reuse `CalloutCap` for start/end cap shapes.
- Store overflow on `PanelDraw`; `PanelDraw::lines(...)` defaults to `DrawOverflow::Clipped`, with a fluent `.overflow(DrawOverflow::Visible)`.
- Use opaque public types over private storage plus constructors/builders so invalid states can be constrained later without breaking public field access.
- Add `start_inset` and `end_inset` as authored dimensions to preserve callout parity without forcing callers to split endpoints manually.
- Keep dash support out of the core type initially; Phase 7 can expand dashed typography guides into multiple `PanelLine`s through a helper.
- Treat all scalar inputs as finite-only.
- `PanelCoord::percent(f32)` rejects or reports non-finite values through a constructor/`try_percent` path. Values outside `0.0..=1.0` are allowed and documented as intentional overflow-capable coordinates.
- Stroke width must be positive or the line is skipped during resolution.
- Cap size and insets are non-negative.
- Over-inset shafts collapse deterministically to zero length while caps follow the resolved cap policy.
- Line resolution validates every resolved endpoint, width, cap size, inset, and cap override after unit conversion.
- Any non-finite value, non-positive width, or negative cap/inset emits no primitives for that line.
- Infallible builders do not panic; fallible helpers start with `PanelCoord::try_percent`.
- Use `impl Into<PanelCoord>` and `impl Into<Dimension>` builders so examples like `PanelPoint::new(Mm(5.0), Mm(10.0))` and `.width(Mm(0.3))` work naturally.
- Generalize shared cap override storage/resolution to dimension-aware sizing before exposing `CalloutCap` through `LineStyle` for panel lines.
- `RenderCommandKind` is public, so resolved line payload types must be public opaque types with private fields/accessors, or line commands must stay behind a crate-private path. The current plan uses public opaque resolved types.
- Line payload types used by `RenderCommandKind::Lines` must support the existing `Clone + Debug + PartialEq` render-command derive contract.
- Add focused unit tests for `PanelCoord` construction/defaults if useful.

### Retrospective

**What worked:**
- `layout/draw.rs` and `layout/line.rs` match the approved module structure and re-export cleanly through `layout/mod.rs` and `lib.rs`.
- Existing `CalloutCap` `f32` builders could be preserved while internal cap overrides became `Dimension`-backed.

**What deviated from the plan:**
- Phase 1 touched `callouts/caps.rs` and `callouts/render.rs` so `LineStyle` could expose `CalloutCap` without keeping raw-only cap override storage.
- `PanelCoord::percent` is infallible and maps non-finite input to `0.0`; `PanelCoord::try_percent` is the rejecting constructor.

**Surprises:**
- `CalloutCap` variant payload structs are private, so later panel-line resolution must use shared `CalloutCap` helper methods instead of matching cap payload fields from `layout`.
- `PanelDraw` cannot enter `Element` storage until scaling, structure hashing, and visual-only change classification are updated together.

**Implications for remaining phases:**
- Phase 2 must scale draw data, including cap override dimensions stored inside `CalloutCap`.
- Phase 3 should resolve cap override dimensions through shared `CalloutCap` helpers rather than direct cap payload access.
- Phase 3 should make invalid authored scalars produce skipped resolved primitives, preserving the Phase 1 no-panic builder contract.

### Phase 1 Review

- Phase 2: moved line-command regeneration tests out of storage-only work and into Phase 3.
- Phase 2: added an explicit authored-draw scaling helper boundary, including `CalloutCap` override dimensions.
- Phase 3: added a shared cap primitive resolver/extraction boundary for panel-line resolution and rendering.
- Phase 3: narrowed invalid-percent expectations to `try_percent`; infallible `percent` sanitizes non-finite values to `0.0`.
- Phase 3: made inherited-versus-owner clip state an emitter/resolver requirement instead of a render-side reconstruction detail.
- Phase 4: narrowed the prototype to a production-compatible retained-record path so later ruler migration does not depend on per-tick visual entities.
- Phase 7: added a non-element post-layout line-source contract for typography overlay guides.
- Phase 9: recorded that cap storage/model unification is already partially complete, so the phase should focus on the transparent-panel adapter.

### Phase 2 - Element Storage And Builder Integration (Complete)

Committable unit: let elements own paint-only draw commands, but do not emit or render them yet.

Add `draw` storage to `Element` and fluent builders on `El`:

```rust
El::new()
    .draw(PanelDraw::lines(lines))
```

Requirements:
- `PanelDraw` does not affect measurement.
- `PanelDraw` participates in visual-only tree change classification, not layout-affecting classification.
- Existing background, border, text, image, clipping, and editable-field behavior should not change.
- The initial implementation supports `PanelDraw::Lines` only.
- Scale draw dimensions with the rest of the tree in `LayoutTree::scaled()` / `Element::scaled()`, including endpoints, stroke width, cap size, insets, and dimension-aware cap values.
- Add crate- or layout-visible scaling helpers on the authored draw types before wiring them into `Element::scaled()`.
- Scale `CalloutCap` override dimensions through a shared helper; `Element::scaled()` must not match private callout cap payload fields directly.
- Exclude draw data from `structure_hash`.
- Add tests proving draw-only changes are stored, classified as visual-only, scaled, excluded from `structure_hash`, and do not change computed layout bounds.

### Retrospective

**What worked:**
- `El::draw(...)` stores `Option<PanelDraw>` on `Element` without changing positioning or render-command emission.
- `PanelDraw::scaled`, `PanelLine::scaled`, and `CalloutCap::scaled_dimensions` keep authored draw scaling out of `Element::scaled()`.

**What deviated from the plan:**
- The production `LayoutTree` draw accessor was not added in Phase 2; `element_draw` is test-only until Phase 3 needs a resolver-facing reader.
- The implementation keeps a single optional draw layer per element, so Phase 3 source keys should treat `draw_ordinal` as `0` unless multi-draw authoring is added later.

**Surprises:**
- `CalloutCap` scaling needs to live in `callouts/caps.rs` because cap payload fields remain private.
- Storage-only draw tests can verify no emission by comparing command streams before any `RenderCommandKind::Lines` variant exists.

**Implications for remaining phases:**
- Phase 3 must add the production accessor or traversal hook that reads `Element::draw`.
- Phase 3 must define whether the single `PanelDraw` storage model is enough or whether element draw storage expands to multiple ordered draw layers.
- Phase 3 can rely on authored draw data already being scaled to point-space in `LayoutTree::scaled()`.

### Phase 2 Review

- Phase 3: approved the single element-owned `PanelDraw` storage model for now; element-owned `draw_ordinal` is `0` and `line_ordinal` is the retained identity.
- Phase 3: narrowed stable-key guarantees to ordinal-stable unchanged element-owned lines; reorder/insert/remove before a line may churn retained records.
- Phase 3: added a pure resolver plus traversal/emission split so post-layout sources can reuse resolution without `LayoutTree` storage.
- Phase 3: moved remaining unit expectations toward consuming already-scaled point-space data because Phase 2 covers authored draw scaling.
- Phase 3: added regression coverage for command-index shifts after draw-only line command insertion/removal.
- Phase 3: made shared cap primitive extraction a Phase 3 dependency before Phase 4 renderer work.
- Phase 7 and Phase 8: clarified that post-layout typography sources use source-level overflow policy equivalent to `DrawOverflow::Visible`, not literal element-owned `PanelDraw` storage.

### Phase 3 - Resolve Line Commands After Layout (Complete)

Committable unit: convert authored element-local lines into resolved layout render commands.

Add a resolved line command to the layout render stream:

```rust
RenderCommandKind::Lines {
    lines: Vec<ResolvedPanelLine>,
}
```

Command bounds invariant:
- If a command contains one resolved line, `RenderCommand.bounds == line.visual_bounds`.
- If a command groups compatible resolved lines, `RenderCommand.bounds == union(line.visual_bounds)`.
- `owner_bounds` stays on each resolved line for coordinate and clipped-draw semantics.

Resolve authored coordinates from the owning element's box:
- `Start(value)` => leading edge plus value.
- `End(value)` => trailing edge minus value.
- `Percent(value)` => leading edge plus resolved size times value.
- Negative `End` values intentionally resolve outside the owning element.

Ordering and clipping:
- Clipped lines use the owning element/current command paint order.
- Overflow-visible lines default to front-of-panel paint order.
- Parent or panel clipping should still apply; overflow should not imply escaping the panel viewport.
- Keep the current `command_index`-based depth model for non-overflow lines.
- Use a dedicated overlay/front command ordering path for overflow lines rather than relying on neighboring element depth.
- Resolve each line with `source_key`, `source_command_index`, `owner_bounds`, `visual_bounds`, `clip`, and `paint_order`.
- `source_key` should include source element index, draw ordinal, line ordinal, and primitive ordinal where relevant. It must not be derived from paint order alone.
- Element-owned `PanelDraw` currently has one optional draw layer, so `draw_ordinal` is `0` for those sources. `line_ordinal` is the identity of a line within that layer.
- Element-owned line identity is ordinal-stable, not semantic: unchanged lines keep retained keys only while their element, draw ordinal, and line ordinal stay the same.
- Post-layout producers, including typography overlay sources, may provide semantic source keys that survive reorder/insert/remove because they are not limited to `PanelDraw::Lines(Vec<PanelLine>)` storage.
- Defer explicit `PanelLine` ids and multiple ordered element draw layers until a real producer needs reorder-stable element-owned line identity or mixed overflow policies inside one element.
- The renderer prefixes `source_key` with the panel entity to form retained batch/storage keys.
- `visual_bounds` is expanded by stroke width, insets, cap geometry, and coverage/AA padding and is used for culling/mesh allocation.
- Clipped line clip policy: `panel viewport ∩ active parent scissor ∩ owner_bounds`.
- Overflow-visible line clip policy: `panel viewport ∩ active parent scissor`.
- Capture both inherited clip before the owner scissor and active clip inside the owner. Default clipped draws use the owner-bounded active clip; visible-overflow draws use inherited parent/panel clip and deliberately exclude the owner's own clip.
- Carry inherited and owner-bounded clip state through layout command emission or line resolution. Do not rely on reconstructing the pre-owner inherited clip later from only the flat render-command stream.
- Use explicit paint lanes such as `Normal(command_index)` and `Overlay(order)` instead of relying on vague "front" ordering.
- Define numeric `depth_bias` and `oit_depth_offset` lanes relative to panel backing layers and batched text before rendering overflow lines.
- Resolve `LineStyle` into finite point-space values before renderer conversion to world space.
- Resolve cap dimensions through shared `CalloutCap` helpers; `layout` and `render/panel_lines` must not match private cap payload fields directly.
- Add a crate-visible cap primitive resolver/extraction point that returns cap primitive kind, dimensions, color, shaft inset, bounds contribution, and part order.
- Complete the shared cap primitive resolver in Phase 3 before Phase 4 consumes line commands; the renderer prototype must not duplicate `callouts/render.rs` cap matching or reach through private cap payload fields.
- Resolve each line into render primitives before batching: shaft plus zero or more cap primitives, each with SDF kind, dimensions, color, clip, bounds, stable primitive key, and part order.
- Keep the line resolver usable by both element-owned `PanelDraw` and post-layout augmented line sources. The reusable entry point should accept owner bounds, clip policy, paint lane, and an explicit stable source key.
- Split implementation into a pure resolver over already-scaled `PanelLine` data plus an element traversal/emission hook. `positioning.rs` may read `Element::draw` directly during traversal; typography and other post-layout producers should reuse the pure resolver without depending on `LayoutTree` storage.

Tests:
- Verify line commands regenerate through both fresh layout and `render_commands_from_geometry`.
- Resolve `Start`, `End`, negative `End`, and `Percent` against known element bounds.
- Verify clipped lines emit in element/current order.
- Verify overflow-visible lines emit through the front-of-panel ordering path.
- Verify parent/panel clipping still constrains overflow draw output.
- Verify the resolver consumes point-space scaled draw data correctly; Phase 2 covers `Mm`, `In`, endpoint, stroke-width, cap-size, inset, and cap-override scaling.
- Verify visible overflow escapes owner clipping but remains constrained by ancestor clipping and the panel viewport.
- Verify adding or removing line commands before existing text/geometry updates downstream command-index-derived depth/material state and leaves no stale records when cached layout geometry is reused.
- Verify line count/style/order changes remove stale resolved records and preserve stable keys for ordinal-stable unchanged element-owned lines.
- Verify inserting, removing, or reordering element-owned lines before an existing line may churn later ordinal-derived retained keys without leaking stale records.
- Verify degenerate or invalid scalar cases resolve deterministically. Do not expect `PanelCoord::percent(f32::NAN)` to skip a line; target `PanelCoord::try_percent`, raw dimensions, widths, insets, cap sizes, cap overrides, and post-conversion non-finite values.

### Retrospective

**What worked:**
- `RenderCommandKind::Lines` now carries `ResolvedPanelLine` records with line identity, primitive identity, owner bounds, visual bounds, clip, paint lane, layering hints, and shaft/cap primitives.
- `positioning.rs` emits line commands through both fresh layout and cached `render_commands_from_geometry`, so visual-only draw changes can regenerate without a layout solve.
- `callouts/caps.rs` now exposes shared resolved cap primitives, and `callouts/render.rs` consumes them instead of matching cap payloads directly.
- `ResolvedPanelLine` and `ResolvedPanelLinePrimitive` stay public opaque payloads with accessors, matching the public `RenderCommandKind` contract.
- Over-inset lines collapse reversed shafts while keeping cap primitives when caps are present.

**What deviated from the plan:**
- Overlay-visible lines still appear at the element traversal point in the flat command stream; their front-of-panel behavior is represented by `PanelLinePaintOrder::Overlay` and numeric layering hints for Phase 4 to honor.
- `layout/line.rs` kept the resolved types and resolver instead of adding a new resolver module, matching the approved module structure.

**Surprises:**
- `Dimension::to_points()` can turn some non-finite raw inputs into finite sentinels, so Phase 3 rejects non-finite `Dimension.value` before conversion.
- Standalone callout rendering could switch to shared cap primitive extraction without changing its existing mesh/material spawn helpers.

**Implications for remaining phases:**
- Phase 4 must consume `ResolvedPanelLine::primitives`, `PanelLinePrimitiveKey`, `clip`, `paint_order`, and `layering` directly instead of recomputing cap geometry or deriving identity from command order.
- Phase 4 must treat `PanelLinePaintOrder::Overlay` as the front-of-panel path even though the command is emitted during normal DFS traversal.
- Phase 5 can build retained batch records from the primitive keys and resolved primitive geometry introduced in Phase 3.

### Phase 3 Review

- Phase 4: narrowed cap work to mapping Phase 3 resolved primitives into renderer geometry; renderer code must not duplicate `CalloutCap` matching or dimension resolution.
- Phase 4: clarified that the panel-line renderer must use resolved line `clip`, `paint_order`, and `layering`, not stream scissor reconstruction or physical command order.
- Phase 4: added a visible-overflow regression where a line emitted inside an owner scissor still escapes through its resolved clip and overlay paint lane.
- Phase 5: narrowed batching scope to retained membership, compatibility splitting, GPU buffers/materials, bounds, visibility, and stale cleanup over Phase 3 resolved primitives.
- Phase 7: chose a transparent overlay panel with element-owned `PanelDraw::lines` for typography pilot guides; post-layout augmentation stores are fallback-only.
- Phase 9: recorded Phase 3 cap extraction as complete and added over-inset cap parity expectations for planar callouts.

### Phase 4 - Line Renderer Prototype

Committable unit: render resolved line commands visually through a production-compatible retained-record path, focused on correctness and visual proof.

Add a panel line renderer that consumes `RenderCommandKind::Lines`.

Initial renderer goals:
- Draw actual line segments from centerlines, not rectangle backgrounds.
- Use analytic shader coverage for stable subpixel edges.
- Support butt/no-cap ruler ticks and existing callout caps.

Implementation direction:
- Own panel line rendering in a `render/panel_lines` module/plugin, separate from standalone `CalloutLine` ECS rendering.
- Register `PanelLinePlugin` from `RenderPlugin`.
- Run prototype reconcile in `PostUpdate` within `PanelChildSystems::Build`.
- Use retained panel-line records keyed by resolved line/primitive identity. The prototype may split conservatively or use a simple record renderer, but the migration path must not depend on one entity/material/mesh per tick.
- Mirror panel-text transform, visibility, bounds, and buffer-upload ordering before `CheckVisibility`.
- Consume line commands from computed panel output and reconcile generated visuals through panel-owned signatures.
- Panel lines render as panel-owned SDF primitives: resolve base material through the same element/panel/default material path as panel geometry, override color from `LineStyle`, force `AlphaMode::Blend`, use double-sided/no-cull geometry, and inherit the owning panel's `SurfaceShadow` unless a future explicit line shadow mode is added.
- Do not copy standalone `CalloutLine` unlit/order defaults except for reusable cap geometry.
- Add concrete depth/OIT helper functions before rendering:
  - clipped normal lines use backing-like offsets tied to source command order
  - overflow-visible lines use an explicit overlay lane
  - sorted `depth_bias` lanes are split where a batch cannot vary bias per record
- Prototype may reuse existing SDF line helpers to validate visual quality.
- Cap rendering should reuse or extract the existing `CalloutCap` model and rendering logic instead of duplicating arrow/circle/square/diamond behavior.
- Map Phase 3 `ResolvedPanelLinePrimitive` cap kinds into panel-line renderer primitives; do not duplicate `CalloutCap` matching or cap dimension resolution in `render/panel_lines`.
- Collect `RenderCommandKind::Lines` through each `ResolvedPanelLine`'s `clip`, `paint_order`, and `layering`. Do not use `clip::compute_clip_rects` or physical command index order to infer line clipping or overlay order.
- Treat the current SDF line helper quality as a starting point, not automatically sufficient.
- Define stale cleanup, color/style updates, render-layer propagation, visibility handling, and depth/OIT ordering in the prototype even if batching comes later.
- Phase 6 cannot depend on a throwaway renderer path; any visual proof in this phase must preserve the same source keys, clip policies, paint lanes, and lifecycle semantics that Phase 5 will batch.
- Default/no-decorative-cap ruler ticks need an explicit butt shaft-cap shader contract, not accidental rounded segment/capsule behavior.
- Include shader/coverage tests or visual checks for butt endpoints, owner-clipped endpoints, and AA ramp survival at clipped edges.
- Add a regression where a visible-overflow line is emitted after an owner `ScissorStart` but still escapes the owner through its resolved line clip and overlay paint lane.

Quality requirements:
- No SMAA dependency for ruler line legibility.
- Stable line thickness under perspective and shallow angles.
- Clean butt/square cap behavior for ruler ticks.
- Correct OIT/sorted transparency ordering relative to panel backgrounds and text.

### Phase 5 - Batched Line Renderer

Committable unit: replace or harden the prototype renderer so high-frequency line sets are performant.

Requirements:
- Avoid one entity/material/mesh per tick as the long-term design.
- Make the first production batch per-panel, split by paint lane, clip group, and compatible style/material as needed.
- Defer cross-panel batching until it mirrors the text batch transform, bounds, and visibility lifecycle.
- Consume the Phase 3 resolved primitive shape: endpoints/local basis or primitive transform, width, color, primitive kind, clip group, source key, part order, and depth/paint group.
- Focus Phase 5 data-shape work on retained batch membership, compatibility splitting, GPU buffer/material layout, bounds, visibility, and stale cleanup rather than redefining primitive resolution.
- Preserve the same depth, clipping, cap, and overflow semantics from Phase 4.
- Remove records when the owner panel or command disappears, when the panel is hidden, or when clipping removes all visual geometry.
- Keep shader coverage good enough that SMAA remains unnecessary for ruler legibility.
- Add instrumentation or tests where practical to ensure units rulers do not create per-tick visual entities.
- Add a visibility regression analogous to the batched text hidden-panel case so hidden panels do not leave stale line records.
- Include clip rect plus coverage/AA padding in the line batch/shader contract.

### Phase 6 - Units Ruler Migration

Committable unit: migrate the `units` example rulers from rectangle-backed ticks to panel-line-backed ticks.

Replace per-tick `El.background(...)` rectangles in the `units` example with line-producing ruler track elements.

Pure helper boundary:
- `metric_vertical_tick_lines`, `metric_horizontal_tick_lines`, `imperial_vertical_tick_lines`, and `imperial_horizontal_tick_lines` return `Vec<PanelLine>`.
- These helpers are the primary test seam for tick count, endpoint coordinates, major/minor lengths, and stroke widths.
- Acceptance should assert no per-tick `RenderCommandKind::Rectangle` backgrounds remain except explicitly retained spines.
- The current rectangle-backed migration targets are `build_vertical_ticks`,
  `build_horizontal_ticks`, `build_metric_panel_ruler`,
  `build_imperial_panel_ruler`, `build_metric_horizontal_ruler`, and
  `build_imperial_horizontal_ruler` in `crates/bevy_diegetic/examples/units.rs`.

Vertical ruler behavior:
- One element owns the tick track.
- Tick coordinates are element-relative.
- Helpers emit inclusive marks `0..=count` and preserve `tick_size_fn(count)` endpoint behavior.
- Normal ticks sit at measurement slot edges.
- Top and bottom endpoint ticks remain represented.
- Vertical imperial/photo/card rulers resolve ticks inside the measured-height track element, not the full panel with `EDGE_LABEL_EXTRA`; the track stays bottom-aligned inside taller label-bearing panels.

Horizontal ruler behavior:
- One element owns the tick track.
- Tick coordinates are element-relative.
- Helpers emit inclusive marks `0..=count` and preserve `tick_size_fn(count)` endpoint behavior.
- Normal ticks sit at measurement slot edges.
- Left and right endpoint ticks remain represented.

Stroke alignment:
- Ruler tick helpers should treat a measurement edge as the outer edge of the stroke and inset centerlines by half the stroke width so clipped tracks preserve full tick thickness.
- Decide during migration whether ruler spines stay rectangle-backed for the first commit or also become `PanelLine`s; keep this explicit in the commit message.

Keep the existing label layout and ruler sizing logic unless line drawing requires a focused adjustment.

Verification:
- `cargo check -p bevy_diegetic --example units`
- helper-level tests for metric/imperial vertical/horizontal line counts, endpoint positions, and major/minor tick lengths
- card/photo endpoint-position tests covering `EDGE_LABEL_EXTRA`
- visually inspect metric and imperial vertical/horizontal rulers
- confirm the title bar has no SMAA control and ruler legibility does not depend on SMAA

### Phase 7 - Typography Overlay Line Pilot

Committable unit: prove typography overlay can generate overflow panel lines from post-layout text metrics without changing measurement.

Goal:
- Add a visual-only post-layout draw producer for metric-derived lines keyed to the owning text run/element.
- Use panel-local overflow lines for one representative typography debug guide instead of retained gizmo/callout-style line rendering.
- Keep the overlay tied to measured text/panel layout data while drawing guide marks beyond a small owning element when useful.

Requirements:
- Do not mutate the pre-layout tree from `ComputedWorldText` data in a way that can create measurement feedback loops.
- Typography pilot lines are emitted by a transparent overlay panel that reads `ComputedWorldText` / text-run metrics and maps its panel-local coordinate space to the measured text/run bounds.
- Use ordinary overlay-panel elements with `PanelDraw::lines` for horizontal guides, arrows, labels, and future draw primitives. The guide lines are element-owned by the overlay panel, not post-layout records injected into the source text panel.
- The source text panel and its `LayoutTree` / `LayoutResult` remain read-only; the overlay panel may be rebuilt or updated from metrics as its own panel without calling `set_tree` on the source panel or changing source content bounds.
- Post-layout augmentation stores are a fallback only if a concrete guide cannot be represented as a transparent panel with element-owned `PanelDraw`.
- Typography overlay panels may use `DrawOverflow::Visible` on their own element-owned guide lines where guides need to cross element bounds.
- The pilot consumer is the `TypographyOverlay` attached to `DisplayText` in
  `crates/bevy_diegetic/examples/typography.rs`; source data comes from the
  existing `ComputedWorldText` overlay pipeline.
- Define a `TypographyGuideFrame`-style helper that converts existing world/Y-up metric and glyph extents into panel-local/Y-down `PanelPoint`s for the owning overlay panel.
- Replace one representative guide, such as a baseline/cap-height guide plus one arrowed annotation.
- Preserve existing typography overlay behavior for all non-pilot guides.
- Prove overlay metric changes regenerate line commands without changing layout bounds.
- Add lifecycle checks for toggling the overlay off/on and removing `TypographyOverlay`, so pilot line records do not outlive the overlay container.

### Phase 8 - Full Typography Overlay Migration

Committable unit: replace typography overlay line/callout drawing with overflow panel line constructs.

Goal:
- Use panel-local overflow lines for typography debug guides instead of retained gizmo/callout-style line rendering.
- Keep the overlay tied to measured text/panel layout data while drawing guide marks beyond a small owning element when useful.

Requirements:
- Preserve existing typography overlay behavior unless a deliberate visual cleanup is documented.
- Use the source-level overflow-visible clip policy where guide lines need to cross element bounds. Element-owned guides may express that through `DrawOverflow::Visible`; post-layout typography guide sources should use the equivalent non-`PanelDraw` policy.
- Use shared `CalloutCap` for arrowed annotations instead of a separate overlay arrow implementation.
- Keep overlay draw commands paint-only so debug guides never perturb measured text layout.
- Replace or explicitly exempt the concrete typography overlay line paths:
  - `spawn_metric_line_panel`
  - `spawn_metric_arrow_callouts`
  - `build_metric_gizmos`
  - `spawn_bounding_box_callout`
  - `spawn_dashed_callout_line`
  - `callouts::draw_dimension_arrow` if no remaining non-overlay user needs it
- Dashed typography guides can initially expand into multiple resolved-space `PanelLine`s through a helper instead of adding native dash shader support. Do not split unresolved mixed `Start`/`End`/`Percent` authoring coordinates generically.
- Test baseline, ascent, cap-height, x-height, descent, and arrow endpoints against the old world-coordinate positions after panel transform.
- Add lifecycle checks for text/font metric changes, `font_metrics` / `glyph_metrics` visibility changes, and overlay removal.
- Acceptance: no typography overlay line guide should still use the old callout/gizmo path unless the phase documents an intentional exception.
- Verification includes the concrete typography overlay check target if available; otherwise document the exact example or test used.

### Phase 9 - Planar Callout Unification

Committable unit: add a transparent-panel-backed path for planar callouts so
callouts can use the same panel-line draw model as rulers and typography guides.

Goal:
- Preserve `CalloutLine` as the standalone public callout API.
- Add a panel-backed implementation path for callouts that live on a single
  two-dimensional plane.
- Make panel-backed callouts the preferred path for future planar callout
  features once panel-line batching is working.

Requirements:
- Define the boundary between planar and non-coplanar callouts.
- Treat cap storage/model unification and shared cap primitive extraction as complete through Phase 3. Extend shared cap helpers only where the transparent-panel adapter needs more resolved cap data.
- Add an adapter or builder that maps a planar `CalloutLine` into a transparent
  `DiegeticPanel` with `PanelDraw::lines(...)`.
- Keep the adapter's visible behavior aligned with existing callouts: endpoints,
  thickness, color, start/end insets, caps, render layers, visibility, and
  shadow policy.
- Reuse `CalloutCap` semantics and shared line/cap SDF helpers rather than
  creating a second cap model.
- Route panel-backed callouts through the panel-line batch renderer, not the
  current direct callout mesh-spawning path.
- Keep the direct callout renderer available for explicitly non-coplanar cases
  or for callout uses that cannot yet be represented by a panel.
- Document which callout construction path is preferred for new planar examples.

Tests and verification:
- Compare a representative standalone callout and panel-backed callout for the
  same planar geometry.
- Verify endpoint, inset, cap, and thickness behavior against current
  `CalloutLine` semantics.
- Include Phase 3's over-inset behavior in planar callout parity: reversed shafts
  collapse while present caps still render from their inset tips.
- Verify panel-backed callouts batch with other compatible panel lines where
  possible.
- Verify non-coplanar callouts remain supported or are rejected with a clear
  documented boundary.

### Phase 10 - Verification And Hardening

Committable unit: close out shared renderer/layout risk after the ruler,
typography, and callout consumers exist.

Code-level verification:
- `cargo +nightly fmt --all`
- `cargo check -p bevy_diegetic --example units`
- `cargo check -p bevy_diegetic --example <typography-overlay-consumer>` if there is a dedicated example
- focused tests for coordinate resolution and render command emission
- `cargo nextest run -p bevy_diegetic` when the shared layout/render path is touched broadly

Visual verification:
- Run the `units` example and compare ruler ticks against the current rectangle-backed version.
- Compare migrated ruler tick lighting and shadow behavior against the rectangle-backed baseline.
- Confirm SMAA remains unnecessary.
- Check both vertical and horizontal metric/imperial rulers.
- Check a simple overflow line/callout case before considering the overflow policy complete.
- Check typography overlay guides still align with text metrics and no longer depend on callout-only rendering.
- Check planar callouts render through the transparent-panel-backed path where
  intended.

## Team Review Log

## Adhoc Review Decisions

### Module Structure

Decision: start with a three-layer split and revisit it if renderer or shared
SDF/callout ownership changes during later review.

```text
crates/bevy_diegetic/src/
├── layout/
│   ├── mod.rs
│   ├── builder.rs
│   ├── element.rs
│   ├── render.rs
│   ├── draw.rs
│   └── line.rs
├── render/
│   ├── mod.rs
│   ├── batching.rs
│   ├── line_sdf.rs
│   └── panel_lines/
│       ├── mod.rs
│       ├── reconcile.rs
│       ├── batching.rs
│       ├── material.rs
│       └── shader.wgsl
└── callouts/
    ├── caps.rs
    ├── line.rs
    └── render.rs
```

Ownership:
- `layout/draw.rs` owns `PanelDraw` and `DrawOverflow`.
- `layout/line.rs` owns authored and resolved panel-line API types plus
  coordinate resolution helpers.
- `render/panel_lines/` owns panel-owned line reconciliation, lifecycle, and
  batching.
- `render/batching.rs` may hold shared retained-batch utilities extracted from
  the text batching pattern once the panel-line batch shape is concrete.
- `render/line_sdf.rs` is an internal shared rendering helper for line/cap SDF
  primitives.
- `callouts/` keeps the standalone callout ECS API and reuses shared rendering
  helpers where practical.

### Renderer Prototype And Batching

Decision: keep a dedicated panel-line renderer, but make the production target
the same kind of retained, instanced batching used by panel text.

Meaning:
- A dedicated panel-line renderer means line commands are consumed under
  `render/panel_lines/`, not faked as layout rectangles, typography gizmos, or
  standalone callout entities.
- The prototype can prove geometry, clipping, depth, caps, and lifecycle with a
  narrower implementation.
- Hardening means converting that renderer to one batch entity per compatible
  panel-line batch whenever possible.
- The batch model should mirror text: compatibility key, inert capacity-sized
  mesh, storage buffers for per-line/per-part records, vertex pulling, dirty
  uploads, explicit capacity growth, and hand-written bounds before visibility.
- The design goal is one draw per compatible batch per pass, as with text.

Sharing with text:
- Reuse the text batching architecture and scheduling constraints directly.
- Do not reuse text's current `GlyphBatchStore`, `GlyphInstanceRecord`,
  `RunRecord`, `GlyphCache`, or `TextMaterial` as-is; those are glyph-specific.
- Evaluate extracting small generic retained-batch utilities after the first
  panel-line batch shape is clear: route/remove/upsert membership, empty-batch
  tracking, power-of-two capacity growth, padded buffer upload helpers, and
  bounds-dirty bookkeeping.
- Keep panel-line records and material/shader separate because line/cap records
  need different geometry, SDF inputs, clip data, and paint lanes from glyphs.

### SDF, Callouts, And Unified Panels

Decision: share SDF/cap implementation work with callouts, keep the current
standalone callout API for now, and explicitly preserve a path toward
transparent panel-backed callouts.

Reasoning:
- `DiegeticText::world` already followed this pattern: standalone world text
  became a one-element transparent panel so wrapping and panel text shared one
  code model.
- Planar callouts can follow the same evolution. If a callout lives on a
  two-dimensional plane, it can be represented as a transparent panel whose
  tree owns `PanelDraw::lines(...)`.
- A panel-backed callout would inherit panel-line batching, clipping, caps,
  labels, typography, and future draw primitives instead of maintaining a
  parallel one-off renderer.
- The existing `CalloutLine` component still has value as the public standalone
  world/local API and as an adapter target while panel-line rendering lands.
- Arbitrary non-coplanar 3D callouts are not automatically covered by the panel
  model; either constrain them to a plane or keep the direct callout path for
  that case.

Implementation stance:
- Reuse `CalloutCap` semantics immediately for panel-line caps.
- Extract or share low-level SDF line/cap helpers where it improves both paths.
- Do not make panel lines depend on the current callout ECS renderer.
- After panel-line batching is working, implement or select a `CalloutLine`
  adapter or builder that creates a transparent `DiegeticPanel` plus
  `PanelDraw` for planar callouts.
- Treat that adapter as the preferred future direction for new planar callout
  features, so new draw capabilities accumulate in one panel-owned model.

### Cycle 1

Recorded refinements:
- Store overflow on `PanelDraw`; default to clipped and expose visible overflow explicitly.
- Scale draw dimensions with `LayoutTree::scaled()` / `Element::scaled()`.
- Treat draw-only changes as visual-only, exclude draw data from `structure_hash`, and regenerate line commands in the visual-only fast path.
- Use public opaque resolved line payload types if lines are carried by public `RenderCommandKind`.
- Resolve line commands with owner bounds, visual bounds, captured clip policy, and explicit paint order.
- Define numeric normal/overlay paint lanes for both sorted transparency and OIT.
- Keep panel line rendering in a dedicated `render/panel_lines` path, sharing low-level cap/SDF helpers with callouts without reusing standalone `CalloutLine` ECS.
- Add a production batch data shape and hidden-panel/stale-record lifecycle rules.
- Make ruler migration inclusive over `0..=count`, stroke-aware at endpoints, and covered by helper-level coordinate tests.
- Make typography overlay migration concrete by naming current callout/gizmo targets and requiring explicit exemptions.

Surviving user decisions after cycle 1: none.

### Cycle 2

Recorded refinements:
- Use opaque public authored types rather than public data variants so constructors can enforce invariants.
- Require finite scalar resolution, positive stroke widths, non-negative cap sizes/insets, and deterministic over-inset/degenerate-line behavior.
- Generalize shared cap sizing to dimension-aware resolution before exposing `CalloutCap` through panel-line `LineStyle`.
- Add stable `PanelLineKey` identity separate from command index and paint order.
- Specify `RenderCommand.bounds` for line commands as visual bounds or the union of grouped visual bounds.
- Capture both inherited and active owner clips so visible overflow escapes the owner but remains constrained by ancestors and panel viewport.
- Resolve authored lines into shaft/cap primitives before batching.
- Make the first production batch per-panel; defer cross-panel batching.
- Add coverage/AA padding to line visual bounds and clip semantics.
- Preserve imperial ruler measured-track origin inside taller label-bearing panels.
- Make ruler helpers pure `Vec<PanelLine>` seams and require no per-tick rectangles except documented spine retention.
- Split typography overlay work into a post-layout pilot phase and a full migration phase.
- Add a typography guide coordinate-frame helper and lifecycle acceptance for overlay toggles, metric changes, and removal.

Surviving user decisions after cycle 2: none.

### Cycle 3

Recorded refinements:
- Aligned earlier API sketches with the opaque-type rule.
- Split line identity into layout-emitted `PanelLineSourceKey` and renderer-owned retained `PanelLineRenderKey`.
- Added post-unit-conversion validation for endpoints, widths, cap sizes, insets, and cap overrides.
- Added default `LineStyle` expectations and named positive defaults.
- Required line render payloads to satisfy `Clone + Debug + PartialEq` because they appear in `RenderCommandKind`.
- Made `PanelLinePlugin` scheduling explicit under `RenderPlugin` / `PanelChildSystems::Build`.
- Added panel-owned SDF material, blend, culling, and shadow policy for panel lines.
- Added the typography post-layout augmentation boundary so metrics-derived lines never mutate the pre-layout tree.
- Added a ruler lighting/shadow visual comparison against the rectangle-backed baseline.

Surviving user decisions after cycle 3: none.

## Consequences And Future Directions

### Ordinal Element Line Identity

Decision: element-owned `PanelDraw` lines use ordinal identity for now.

Current consequence:
- Phase 2 stores one optional `PanelDraw` per `Element`.
- `PanelDraw::Lines` stores a plain `Vec<PanelLine>` with no explicit line ids.
- Phase 3 treats `draw_ordinal` as `0` for element-owned lines.
- Phase 3 treats the line's index in that `Vec<PanelLine>` as `line_ordinal`.
- A retained renderer record is stable when the same semantic line stays at the
  same element, draw ordinal, line ordinal, and primitive ordinal.
- If a producer inserts, removes, or reorders element-owned lines before an
  existing line, later lines can receive new retained keys. Correct stale-record
  cleanup still makes the output correct, but the renderer may tear down and
  recreate records that could have been updated in place.

Why this is acceptable now:
- Ruler helpers emit stable ordered tick lists.
- Element-owned panel lines remain simple and ergonomic.
- Phase 3 can stay focused on resolution, clipping, command emission, and stale
  cleanup instead of adding public line ids before the renderer proves they are
  needed.
- Post-layout sources such as typography overlays can provide semantic source
  keys outside `PanelDraw::Lines(Vec<PanelLine>)`.

Possible future directions:
- Add explicit `PanelLine` ids if an element-owned producer needs
  reorder-stable retained identity.
- Expand element storage from one optional `PanelDraw` to multiple ordered draw
  layers if mixed overflow policies or authored paint layering become necessary
  inside one element.
- Keep semantic source keys in post-layout sources where those producers already
  have stable domain identity such as text id, run id, guide kind, or callout id.
