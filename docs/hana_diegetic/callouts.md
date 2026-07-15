# Callouts plan

This is the starting plan for making callouts a coherent feature across
screen-space UI, world-space UI, and panel-local drawing.

## Current shape

`hana_diegetic` has one relevant system today:

- Panel-local drawing through `PanelDraw`, `PanelShape`, `PanelLine`, and
  `PanelCircle` in `crates/hana_diegetic/src/layout/`.

`crates/hana_diegetic/src/callouts/` no longer hosts a standalone renderer: the
`CalloutLine` component and its direct SDF child-mesh route were removed, and the
module now owns only the `CalloutCap` cap primitives that `layout::line` resolves
and emits as panel-shape primitives. `PanelLine` shares that `CalloutCap`
vocabulary and cap resolver; its endpoints, units, clipping, draw order, and
lifecycle are panel-local, and it renders through the shared analytic-path
batched renderer.

Text provides the most useful precedent:

- Standalone `DiegeticText::world` / `DiegeticText::screen` is backed by a
  one-element `DiegeticPanel`.
- Text inside a panel is not a nested `DiegeticText`; it is authored directly in
  the current panel tree.

Callouts should follow that split.

## Questions this plan answers

- Should standalone callouts have `screen` and `world` entry points?
- Should standalone callouts be panel-backed?
- If panel-backed, should they render through panel shape primitives?
- Should the panel primitive API be replaced by a callout API?
- Should both APIs coexist?
- If a callout is authored from inside a panel, should it create a recursive
  backing panel?
- What happens to arbitrary non-planar 3D callouts?
- What must be solved before the current direct callout path can be migrated?

## Recommendation

Keep a layered model.

1. Keep `PanelDraw` / `PanelShape` / `PanelLine` as the generic panel primitive
   API.
2. Add semantic callout APIs above the primitive layer.
3. Make screen callouts and planar world callouts panel-backed by default.
4. Implement panel-backed callouts by lowering to `PanelDraw::shapes`,
   `PanelLine`, `PanelCircle`, and future `PanelShape` variants.
5. Do not force arbitrary non-planar `Vec3` callouts into panels.
6. Do not create recursive backing panels for callouts authored inside an
   existing panel.
7. Introduce a shared callout spec before broadening the public API.

The important distinction is:

- Standalone callout facade: may create or select a transparent backing panel.
- Panel-local callout: lowers directly into the current panel's draw data.
- Low-level panel primitive: remains generic and non-semantic.

## API layers

### 1. Semantic callout facade

This is the user-facing feature API. It should express annotation intent:
targets, leader lines, caps, labels, placement, and coordinate-space rules.

Possible names:

```rust
DiegeticCallout::screen(start, end)
DiegeticCallout::world_on_plane(start, end, plane)
PanelCallout::line(start, end)
```

The semantic callout API should not replace panel primitives. It compiles to
them when the callout is panel-backed.

### 2. Neutral shared spec

Introduce shared data that can be adapted into direct callouts, panel-local
callouts, or panel-backed standalone callouts.

Possible pieces:

```rust
CalloutSpec
CalloutStroke
CalloutCaps
CalloutEndpoints<Space>
CalloutTarget
```

The goal is to stop duplicating stroke, cap, inset, target, and styling logic
between `CalloutLine` and `PanelLine`.

### 3. Space-specific adapters

The builder should make context explicit. Prefer separate types or typestate
over a single builder with ignored setters.

Possible modes:

```rust
CalloutBuilder<Screen>
CalloutBuilder<World>
CalloutBuilder<PanelLocal>
```

Screen and world builders may expose `spawn()`. Panel-local builders should not.
Panel-local callouts should produce `PanelDraw`, `PanelShape`, or be accepted by
an element builder.

### 4. Panel primitive backend

Panel-backed callouts should render through the panel shape path:

- `PanelDraw::shapes`
- `PanelShape::Line`
- `PanelShape::Circle`
- future `PanelShape` variants as needed
- shared `CalloutCap` resolution
- analytic path batching

This avoids a second panel-callout renderer and keeps clipping, depth, OIT,
anti-aliasing, cap handling, and batching in one place.

### 5. Direct renderer fallback

The existing direct `CalloutLine` path should remain until the new model can
handle or explicitly reject every use case it covers.

Do not silently flatten arbitrary `Vec3` lines into a panel. Panel backing is
correct for screen-space callouts and classified planar world callouts. It is
not automatically correct for non-coplanar world callouts, world-to-screen
leaders, or mixed-coordinate-space annotations.

## Standalone callouts

Standalone callouts should follow the `DiegeticText` precedent.

Recommended behavior:

- `DiegeticCallout::screen(...)` creates or selects a transparent screen panel.
- `DiegeticCallout::world_on_plane(...)` creates or selects a transparent world
  panel on a known plane.
- `spawn()` returns a stable entity or handle with a `DiegeticCallout` marker.
- The marker entity should be the thing users query, update, despawn, and attach
  app markers to.

Important open design point: grouping.

One transparent panel per standalone callout is conceptually simple, but likely
too expensive and noisy for dense callout sets. The longer-term shape should
allow grouping standalone planar callouts into backing panels by compatible
render context:

- coordinate space
- screen window
- render layers
- camera order
- world plane
- shadow policy
- material and lighting
- anti-alias and hairline policy

The first implementation can be simpler, but the public API should not preclude
grouping.

## Panel-local callouts

Callouts authored inside a panel should never allocate another `DiegeticPanel`
by default.

Inside a panel, the current layout element already owns:

- local coordinates
- resolved bounds
- clipping
- cascade-resolved style
- draw slot
- stable source identity
- render-layer context
- panel transform

Creating a recursive panel would duplicate lifecycle and anchoring behavior,
increase batch churn, risk attachment cycles, and make clipping and draw order
less predictable.

Possible API shape:

```rust
El::new()
    .callout(PanelCallout::line(start, end).end_cap(CalloutCap::arrow()));
```

or:

```rust
El::new()
    .draw(PanelDraw::callouts([PanelCallout::line(start, end)]));
```

The exact spelling can change, but the behavior should be fixed: panel-local
callouts lower into the existing element-owned draw data.

## Panel primitives are not callouts

Do not collapse `PanelDraw` into the callout API.

`PanelDraw` and `PanelShape` are lower-level vector marks. They cover:

- ruler ticks
- guide lines
- dividers
- decorative marks
- metric overlays
- dots and shape primitives
- callout visuals as one use case

Calling the whole primitive layer "callouts" would make non-callout drawing
semantically awkward and would also under-specify real callout behavior such as
targets, labels, leader placement, and coordinate-space policy.

The relationship should be:

```text
Callout API -> PanelDraw / PanelShape -> analytic panel shape renderer
Primitive API -> PanelDraw / PanelShape -> analytic panel shape renderer
```

Both APIs share the backend. They do not share the same semantic level.

## Targets

Raw endpoints are not enough for a generic callout feature.

Callouts should eventually model semantic targets, similar to how IME and panel
anchoring distinguish target spaces and ownership.

Potential target kinds:

- world point
- world entity
- world plane point
- screen point
- screen rect
- panel-local point
- panel element or field
- app-owned anchor

Target type should help choose the backend. For example, two panel-local points
lower directly to panel primitives, while two screen points can be backed by a
screen panel.

Mixed targets need explicit policy. Do not implicitly support world-to-screen or
screen-to-panel callouts by flattening them into an arbitrary panel.

## Units

Units are a real API hazard.

Today:

- direct callouts use `f32` sizes interpreted as world meters
- panel shapes use `Dimension` and contextual units
- `CalloutCap` dimensions can resolve differently depending on path

Before promoting the new callout API, define how unitless values behave.

Recommended direction:

- Use `Dimension`-based styling in the shared callout spec.
- Let typed units such as `Px`, `Pt`, `Mm`, and `In` stay explicit.
- Resolve unitless values only at the final adapter boundary.
- Document contextual defaults:
  - screen callout: pixels
  - panel-local callout: panel layout unit
  - world planar callout: panel/world scale context, not arbitrary meters unless
    that is explicitly the selected mode

## Draw order and render context

Migrating callouts into panel-backed rendering changes their draw-order model.

Direct callouts currently have their own child-mesh order and material depth
behavior. Panel shapes use panel draw slots, shape ordinals, primitive ordinals,
panel material context, OIT offsets, and analytic batching.

Before replacing direct callouts, define:

- where standalone callout backing panels sit relative to other panels
- where callout lines sit relative to panel backgrounds, images, text, and
  borders
- cap vs shaft ordering
- interaction with OIT depth offsets
- interaction with `DrawLayer` or successor concepts

Backing-panel creation must preserve or explicitly set:

- coordinate space
- window and camera order
- render layers
- material
- lighting
- sidedness
- shadow policy
- anti-alias mode
- hairline fade policy

Wrong defaults here can produce invisible screen callouts, changed lighting,
shadow mismatches, or accidental batch splits.

## Avoid `External` as the first route

`PanelShapeSourceKey::External` exists, but it is not yet a complete policy for
post-layout callout producers.

The current panel shape path naturally derives material, clipping, cascade,
anti-aliasing, hairline fade, and cleanup from element-owned `PanelDraw`.
External shape sources would need explicit answers for all of those.

Default route:

- standalone planar callouts create or select transparent panels with
  element-owned draw records
- panel-local callouts use element-owned draw records directly

Only use `External` for callouts after it has explicit lifecycle, ownership,
render context, cascade, and cleanup semantics.

## Rejected paths

### Replace panel primitives with callouts

Rejected. Panel primitives are generic vector marks. Callouts are a semantic
feature above that layer.

### Always make every callout panel-backed

Rejected. Planar screen/world callouts should be panel-backed. Arbitrary
non-planar world callouts should remain direct-rendered, unsupported, or routed
through an explicit projection policy.

### Create recursive panels for panel-local callouts

Rejected. Panel-local callouts should lower directly into the current panel's
draw data.

### Use `External` panel shapes as the default standalone route

Rejected for now. External source semantics are not complete enough.

## Implementation outline

1. Define the neutral callout spec and shared styling vocabulary.
2. Add panel-local conversion from callout spec to `PanelDraw` / `PanelShape`.
3. Add `PanelCallout` or `El::callout` API that cannot spawn entities.
4. Add standalone `DiegeticCallout::screen` backed by transparent screen panels.
5. Add standalone planar world callouts backed by transparent world panels.
6. Preserve direct `CalloutLine` for non-planar or compatibility cases.
7. Define draw-order and render-context rules for backing panels.
8. Add examples covering:
   - screen callout
   - world planar callout
   - panel-local callout
   - primitive `PanelDraw` line that is not a callout
   - direct/non-planar callout compatibility case if retained

## API sketch

```rust
// Standalone screen callout.
DiegeticCallout::screen(start, end)
    .width(Px(2.0))
    .end_cap(CalloutCap::arrow())
    .spawn(&mut commands);

// Standalone planar world callout.
DiegeticCallout::world_on_plane(start, end, plane)
    .width(Mm(0.5))
    .end_cap(CalloutCap::arrow())
    .spawn(&mut commands);

// Panel-local semantic callout.
El::new()
    .callout(
        PanelCallout::line((0.0, 0.0), (PanelCoord::end(0.0), 0.0))
            .end_cap(CalloutCap::arrow())
    );

// Low-level primitive remains valid and non-semantic.
El::new()
    .draw(PanelDraw::lines([
        PanelLine::new((0.0, 8.0), (PanelCoord::end(0.0), 8.0))
    ]));
```

The final names are not fixed. The invariants are the important part.

## Core invariants

- There is one panel shape rendering backend.
- Panel primitives stay generic.
- Callouts are semantic sugar plus target policy.
- Screen and planar world callouts can be panel-backed.
- Arbitrary 3D callouts are not silently flattened.
- Panel-local callouts do not spawn panels.
- Unit resolution happens at the adapter boundary.
- Render context and draw order are explicit, not accidental.
