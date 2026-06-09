# Panel Line API Review - 2026-06-09

Decisions from adhoc review of proposed panel-local line drawing API types for diegetic panel rulers.

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

### Phase 1 - Authored Layout Types

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

### Phase 2 - Element Storage And Builder Integration

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
- Exclude draw data from `structure_hash`.
- Add tests proving draw-only changes are classified as visual-only, regenerate line commands through both fresh layout and `render_commands_from_geometry`, and do not change computed layout bounds.

### Phase 3 - Resolve Line Commands After Layout

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
- The renderer prefixes `source_key` with the panel entity to form retained batch/storage keys.
- `visual_bounds` is expanded by stroke width, insets, cap geometry, and coverage/AA padding and is used for culling/mesh allocation.
- Clipped line clip policy: `panel viewport ∩ active parent scissor ∩ owner_bounds`.
- Overflow-visible line clip policy: `panel viewport ∩ active parent scissor`.
- Capture both inherited clip before the owner scissor and active clip inside the owner. Default clipped draws use the owner-bounded active clip; visible-overflow draws use inherited parent/panel clip and deliberately exclude the owner's own clip.
- Use explicit paint lanes such as `Normal(command_index)` and `Overlay(order)` instead of relying on vague "front" ordering.
- Define numeric `depth_bias` and `oit_depth_offset` lanes relative to panel backing layers and batched text before rendering overflow lines.
- Resolve `LineStyle` into finite point-space values before renderer conversion to world space.
- Resolve cap dimensions through dimension-aware cap sizing before public panel-line cap use.
- Resolve each line into render primitives before batching: shaft plus zero or more cap primitives, each with SDF kind, dimensions, color, clip, bounds, stable primitive key, and part order.

Tests:
- Resolve `Start`, `End`, negative `End`, and `Percent` against known element bounds.
- Verify clipped lines emit in element/current order.
- Verify overflow-visible lines emit through the front-of-panel ordering path.
- Verify parent/panel clipping still constrains overflow draw output.
- Verify `Mm`, `In`, and point-space endpoints/stroke widths scale consistently with element sizing.
- Verify visible overflow escapes owner clipping but remains constrained by ancestor clipping and the panel viewport.
- Verify line count/style/order changes remove stale resolved records and preserve stable keys for unchanged lines.
- Verify degenerate or invalid scalar cases resolve deterministically.

### Phase 4 - Line Renderer Prototype

Committable unit: render resolved line commands visually with a simple implementation, focused on correctness and visual proof.

Add a panel line renderer that consumes `RenderCommandKind::Lines`.

Initial renderer goals:
- Draw actual line segments from centerlines, not rectangle backgrounds.
- Use analytic shader coverage for stable subpixel edges.
- Support butt/no-cap ruler ticks and existing callout caps.

Implementation direction:
- Own panel line rendering in a `render/panel_lines` module/plugin, separate from standalone `CalloutLine` ECS rendering.
- Register `PanelLinePlugin` from `RenderPlugin`.
- Run prototype reconcile in `PostUpdate` within `PanelChildSystems::Build`.
- If the prototype spawns panel children, make them present before screen-space layer propagation.
- If the prototype batches records, mirror panel-text transform/visibility ordering before `CheckVisibility`.
- Consume line commands from computed panel output and reconcile generated visuals through panel-owned signatures.
- Panel lines render as panel-owned SDF primitives: resolve base material through the same element/panel/default material path as panel geometry, override color from `LineStyle`, force `AlphaMode::Blend`, use double-sided/no-cull geometry, and inherit the owning panel's `SurfaceShadow` unless a future explicit line shadow mode is added.
- Do not copy standalone `CalloutLine` unlit/order defaults except for reusable cap geometry.
- Add concrete depth/OIT helper functions before rendering:
  - clipped normal lines use backing-like offsets tied to source command order
  - overflow-visible lines use an explicit overlay lane
  - sorted `depth_bias` lanes are split where a batch cannot vary bias per record
- Prototype may reuse existing SDF line helpers to validate visual quality.
- Cap rendering should reuse or extract the existing `CalloutCap` model and rendering logic instead of duplicating arrow/circle/square/diamond behavior.
- Treat the current SDF line helper quality as a starting point, not automatically sufficient.
- Define stale cleanup, color/style updates, render-layer propagation, visibility handling, and depth/OIT ordering in the prototype even if batching comes later.
- Default/no-decorative-cap ruler ticks need an explicit butt shaft-cap shader contract, not accidental rounded segment/capsule behavior.
- Include shader/coverage tests or visual checks for butt endpoints, owner-clipped endpoints, and AA ramp survival at clipped edges.

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
- Define the production batch data shape over resolved primitives: endpoints/local basis or primitive transform, width, color, SDF kind, cap/primitive dimensions, clip group, source key, part order, and depth/paint group.
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
- Typography pilot lines are emitted by a post-layout augmentation path that reads `ComputedDiegeticPanel` and text metrics, resolves through the Phase 3 line resolver, writes line records keyed by panel plus text id/run plus guide kind, and never calls `set_tree`, mutates `LayoutTree`, or updates content bounds.
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
- Use `DrawOverflow::Visible` where guide lines need to cross element bounds.
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

### Phase 9 - Verification And Hardening

Committable unit: close out shared renderer/layout risk after both consumers exist.

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

## Team Review Log

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
