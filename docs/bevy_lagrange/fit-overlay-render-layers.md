# Fit Overlay Render Layers

Design note for making `bevy_lagrange`'s `FitOverlay` render correctly with
multiple cameras, custom render layers, and screen-space overlay cameras.

The public contract below describes the target retained-overlay implementation.
The current implementation uses retained Core3d line meshes and plain Bevy UI
labels.

## Current Model

The fit overlay is enabled by inserting `FitOverlay` on a camera entity:

```rust
commands.entity(camera).insert(FitOverlay);
```

Removing that marker disables the overlay:

```rust
commands.entity(camera).remove::<FitOverlay>();
```

The current overlay draws its lines through retained Core3d mesh entities:

```rust
Mesh3d + MeshMaterial3d<FitOverlayLineMaterial> + FitOverlayLineVisual
```

Each retained line root carries `FitOverlayVisual { camera, kind }`, the source
camera's effective `RenderLayers`, `NoFrustumCulling`, `NotShadowCaster`, and
`Pickable::IGNORE`. `FitOverlayLineMaterial` is an `ExtendedMaterial` over
`StandardMaterial` that disables depth writes and uses `depth_compare = Always`.

The labels are currently plain Bevy UI text nodes. They carry ownership markers
such as `MarginLabel` and `BoundsLabel`, plus a `UiTargetCamera` chosen so
labels render through the top active camera on the same render target.

## Problem

The current model is correct enough for the common case:

```rust
camera: FitOverlay + RenderLayers::layer(3)
```

But it is not a complete public feature model. This case cannot be represented
correctly with a single global gizmo config:

```rust
camera_a: FitOverlay + RenderLayers::layer(3)
camera_b: FitOverlay + RenderLayers::layer(7)
```

Both cameras need independent overlay line visuals. Camera A's line visuals
should render on layer 3. Camera B's line visuals should render on layer 7.
Phase 3 implements that line-visual behavior; labels remain Bevy UI text nodes
in this plan.

Plain UI labels require choosing a UI target camera separately from the camera
that owns the overlay. Phase 4 keeps that Bevy UI path and makes it visible and
non-pickable when the overlay is visible.

## Goals

- Keep `FitOverlay` as the opt-in marker on the source camera.
- Let the source camera's `Camera.order` define render ordering.
- Propagate the source camera's `RenderLayers` to every generated Core3d line
  visual.
- Support multiple simultaneous `FitOverlay` cameras with different render
  layers.
- Track generated overlay entities so they can be updated in place and cleaned
  up when `FitOverlay` is removed.
- Keep labels as Bevy UI text nodes for now; they use `UiTargetCamera`, not
  `RenderLayers`.
- Keep `FitOverlay` zero-config: render layers and order come from the camera,
  while `FitTargetOverlayConfig` remains visual appearance configuration.

## Non-goals

- Do not add a separate overlay camera order setting.
- Do not make `FitOverlay` spawn or manage cameras.
- Do not add a direct overlay render-layer override; users configure overlay
  visibility through the source camera's `RenderLayers`.
- Do not depend on `bevy_diegetic` for this feature while `bevy_lagrange` needs
  to stand alone.

## Target Model

`FitOverlay` remains a marker component on a camera. Its doc comment should
state the render-layer behavior directly:

```rust
/// Enables a retained fit overlay for this camera.
///
/// Generated line visuals copy this camera's effective `RenderLayers` and
/// render in normal Bevy camera passes. Generated labels are Bevy UI nodes
/// targeted through `UiTargetCamera`. `FitOverlay` owns overlay update and
/// cleanup; it does not add any render visibility filter beyond Bevy
/// `RenderLayers` for line visuals.
#[derive(Component, Reflect, Default)]
pub struct FitOverlay;
```

The overlay system generates retained line visual entities for each camera that
has `FitOverlay`. It also updates plain Bevy UI label entities for the same
camera.

Each generated entity carries a marker tying it to the source camera and to the
specific overlay part it represents:

```rust
#[derive(Component, Reflect, Clone, Copy, Debug, PartialEq, Eq)]
struct FitOverlayVisual {
    camera: Entity,
    kind:   FitOverlayVisualKind,
}

#[derive(Reflect, Clone, Copy, Debug, PartialEq, Eq)]
enum FitOverlayVisualKind {
    Rectangle,
    Silhouette,
    MarginLine { edge: Edge },
    MarginLabel { edge: Edge },
    BoundsLabel,
}
```

The `camera` field answers which `FitOverlay` camera owns the visual. The `kind`
field answers which overlay part the visual represents. Together they form the
stable identity used for update, reuse, and cleanup.

The marker is an update and cleanup identity. It is not, by itself, a Bevy
render visibility filter.

## Render Layers

Each generated Core3d line visual inherits the source camera's render layers:

```rust
let layers = camera_layers
    .cloned()
    .unwrap_or_else(|| RenderLayers::layer(0));

commands.spawn((
    FitOverlayVisual {
        camera,
        kind: FitOverlayVisualKind::MarginLine { edge: Edge::Left },
    },
    layers,
    // visual bundle
));
```

Inheritance here means copy-on-reconcile from the source camera. It is not ECS
hierarchy inheritance, and the fit target entity's render layers do not
contribute to the overlay visual layers.

When the source camera's layers change, the overlay system updates all Core3d
line visuals owned by that camera:

```rust
for (entity, visual, visual_layers) in &visuals {
    if visual.camera == camera && visual_layers != &layers {
        commands.entity(entity).insert(layers.clone());
    }
}
```

This makes the public behavior simple:

```rust
camera_a: FitOverlay + RenderLayers::layer(3)
  -> all camera_a Core3d line visuals get RenderLayers::layer(3)

camera_b: FitOverlay + RenderLayers::layer(7)
  -> all camera_b Core3d line visuals get RenderLayers::layer(7)
```

Removing the source camera's `RenderLayers` component is also a change. The
effective layer becomes layer 0, and retained visuals must be repaired to layer
0 during reconciliation.

## Camera Order

The overlay should not have its own order field.

`Camera.order` already defines when a camera renders relative to other cameras.
If a user wants the fit overlay later or earlier, they configure the camera that
owns `FitOverlay`.

`Camera.order` controls camera pass order only. `FitOverlay` should not use
order as part of visual identity, upsert keys, or per-visual sorting. It also
does not isolate retained overlay visuals from another camera on intersecting
render layers; that is a render-layer contract question, not an order question.

This keeps `FitOverlay` from duplicating camera configuration:

```rust
commands.spawn((
    Camera {
        order: 10,
        ..default()
    },
    FitOverlay,
));
```

## Visual Lifecycle

Each frame, for each camera with `FitOverlay`:

1. Compute the current screen-space bounds, silhouette, margin lines, and label
   positions.
2. Resolve the source camera's effective `RenderLayers`, defaulting to layer 0.
3. Upsert the visual entity for each visible `FitOverlayVisualKind`.
4. Update line geometry/render layers and UI label text/placement in place.
5. Despawn any visual entity for that camera whose kind is no longer visible.

When `FitOverlay` is removed from a camera:

1. Remove `FitMarginPercents` from the camera.
2. Despawn all entities where `FitOverlayVisual.camera == camera`.

When the camera entity is despawned, the same cleanup path should remove its
generated visuals.

Orphan cleanup must run even when there are no remaining cameras with
`FitOverlay`. It cannot be gated only by `any_with_component::<FitOverlay>`,
because the only remaining work may be stale `FitOverlayVisual` entities.

## Labels

Labels use the same `FitOverlayVisual` ownership marker as lines:

```rust
FitOverlayVisual {
    camera,
    kind: FitOverlayVisualKind::MarginLabel { edge: Edge::Left },
}
```

They remain plain Bevy UI text nodes and keep the current UI-camera targeting:

```rust
UiTargetCamera(top_camera_on_same_render_target)
```

The label implementation must remain plain Bevy UI until `bevy_diegetic` is a
published dependency that `bevy_lagrange` can use. Until then, labels are not
render-layer-correct in the same way as line meshes; they are UI nodes targeted
to an active camera on the same render target. If no higher-order same-target
UI-renderable camera exists, the source `FitOverlay` camera is used as the
fallback; for guaranteed label visibility, that source camera must be a Bevy
UI-renderable camera (`Camera2d` or `Camera3d`).

## Implementation Phases

These phases are intended as sequential committable units.

### 1. Public Contract And Ownership Markers

**Status:** Complete

- Add or update Rust source doc comments on `FitOverlay` and
  `FitTargetOverlayConfig`.
- Document that generated visuals copy the source camera's effective
  `RenderLayers`, use normal Bevy layer-intersection visibility, and do not add
  any render visibility filter beyond Bevy `RenderLayers`.
- Introduce `FitOverlayVisual` and `FitOverlayVisualKind`.
- Add `FitOverlayVisual { camera, kind }` to labels immediately, even while
  they remain UI nodes. The existing `MarginLabel` and `BoundsLabel` markers may
  remain temporarily as label-specific query tags, but they should no longer be
  the ownership identity.
- Register retained inspection targets for reflection under `fit_overlay` when
  BRP validation needs to inspect them.

#### Retrospective

**What worked:**

- Existing label spawn/update functions in
  `crates/bevy_lagrange/src/fit_overlay/labels.rs` let phase 1 stamp
  `FitOverlayVisual` without changing label rendering.
- Existing `Reflect` component style covered BRP-facing inspection without
  manual type registration.

**What deviated from the plan:**

- `FitOverlayVisual` stayed internal to `fit_overlay`; BRP inspection should use
  the reflected component type path, not a public Rust export.
- `MarginLabel` and `BoundsLabel` became pure query tags immediately instead of
  carrying duplicate `camera`/`edge` ownership fields.

**Surprises:**

- `FitOverlayVisualKind` needs both current label variants and future line
  variants now, so phase 2 can reconcile one identity enum.
- At phase 1, the `FitTargetGizmo` render-layer limitation remained unchanged;
  that phase only changed ownership identity and source docs.

**Implications for remaining phases:**

- Phase 2 can use `FitOverlayVisual { camera, kind }` as the retained identity
  for current labels and future retained line visuals.
- Phase 2 should add a shared helper for copying a source camera's effective
  `RenderLayers`; phase 1 only documents that contract.
- Phase 4 should keep `MarginLabel` and `BoundsLabel` as UI label query tags
  while `FitOverlayVisual` remains the ownership identity.

#### Phase 1 Review

- Phase 2 now names scheduling and orphan cleanup as reconciliation
  foundation work.
- Phase 2 now allows a per-frame index or map, with `Hash` derivation required
  only if a hash-keyed implementation is chosen.
- Phase 2 now calls out the backend-aware layer mismatch: current gizmo lines
  are not render-layer-correct until phase 3 replaces them, while UI labels
  remain `UiTargetCamera`-based by design.
- Phase 3 now names the temporary retained-lines/UI-labels mixed state and its
  focused tests.
- Phase 4 now scopes label work to Bevy UI visibility, target-camera selection,
  high UI z-order, picking ignore, and label cleanup.
- Phase 5 now limits final cleanup to backend features proven stale by the
  retained line implementation and the Bevy UI label path.
- Reflection guidance now keeps `FitOverlayVisual` internal and avoids manual
  registration unless a new generic or non-auto-registered target requires it.

### 2. Desired Frame And Reconciliation

**Status:** Complete

- Split the implementation into context/layout, reconciliation, retained line
  backend, Bevy UI label upkeep, and plugin wiring.
- Introduce `FitOverlayCameraContext` and `FitOverlayFrame`, while still driving
  the old gizmo/UI render backends.
- Process every camera with `FitOverlay` using optional state so missing or
  invalid inputs produce `FitOverlayFrame::Empty`.
- Reconcile retained identities with a per-frame `(camera, kind)` index or map,
  repair copied `RenderLayers` on retained entities where meaningful, remove
  stale visuals, and deduplicate duplicates.
- If the reconciliation implementation uses a hash-keyed map, add `Hash` to the
  internal visual key path; otherwise use an index shape that does not require
  `Hash`.
- Add the shared helper for resolving a source camera's effective
  `RenderLayers`, defaulting to layer 0 when the component is absent.
- Move overlay scheduling into the retained foundation: add
  `FitOverlaySystemSet` in `PostUpdate`, and keep orphan cleanup outside any
  `any_with_component::<FitOverlay>` gate.
- Add orphan cleanup that is not gated only by active `FitOverlay` cameras.

#### Retrospective

**What worked:**

- The context/frame split let phase 2 process every `FitOverlay` camera and
  then keep the old gizmo/UI backend as a consumer of visible frames.
- The phase 1 `FitOverlayVisual { camera, kind }` marker was enough to centralize
  removal, orphan cleanup, and duplicate cleanup before retained render entities
  exist.
- Moving the overlay work to a dedicated `PostUpdate` set after transform
  propagation compiled cleanly and matches the retained-backend scheduling
  target.

**What deviated from the plan:**

- The current UI labels still cannot receive meaningful source-camera
  `RenderLayers`; they participate in retained identity and cleanup, then phase
  4 hardens their UI visibility path.
- The old gizmo backend still has a single global layer config. Phase 2 now
  defaults it back to layer 0 when no active `FitOverlay` camera is present and
  otherwise chooses the highest-order active `FitOverlay` camera as the least
  surprising temporary behavior.
- A full upsert map is deferred until phase 3 has retained line entities to
  update in place. Phase 2 only needs deterministic deduplication and cleanup of
  existing label identities.

**Surprises:**

- `CurrentFitTarget` had to become optional in the camera query so missing
  targets produce an explicit empty frame instead of skipping stale cleanup.
- The plain no-default check is still worth running separately from the
  documented `fit_overlay` gates because the feature split is easy to disturb
  while moving overlay modules.

**Implications for remaining phases:**

- Phase 3 should be the first phase that performs meaningful per-visual
  `RenderLayers` repair, because retained line entities will have real render
  components.
- Phase 3 should preserve the temporary mixed-state tests: retained lines may
  inherit layers while labels still use `UiTargetCamera`.
- Phase 4 remains responsible for ensuring the UI label path is visible,
  non-pickable, target-camera-aware, and documented as a Bevy UI path.

#### Phase 2 Review

- Phase 3 now explicitly owns the first reusable retained-visual upsert and
  layer-repair helper.
- Phase 3 now introduces the core non-Fairy-Dust layer validation harness before
  showcase workaround cleanup.
- Phase 3 now requires recursive cleanup ownership for any generated line child
  render entities.
- Phase 4 now treats `MarginLabel` and `BoundsLabel` as Bevy UI query tags only,
  not ownership identity.
- Scheduling guidance now treats `FitOverlaySystemSet` and the
  `On<Remove, FitOverlay>` observer as phase 2 foundation that remaining phases
  should reuse.
- Visual-kind examples now use the implemented `Rectangle`, `Silhouette`,
  `MarginLine { edge }`, `MarginLabel { edge }`, and `BoundsLabel` variants.

### 3. Retained Line Backend

**Status:** Complete

- Replace transient gizmo line calls with retained Core3d-compatible line
  visual entities.
- Add the first real retained-visual upsert path now that retained line
  entities exist. It should repair copied `RenderLayers` on survivor entities.
- Propagate the source camera's effective `RenderLayers` to every generated
  line visual during reconciliation.
- Implement the line material, depth, visibility, culling, shadow, and picking
  policies defined below.
- Reuse `FitOverlaySystemSet`; retained roots written after transform
  propagation must also receive matching `GlobalTransform` values for
  same-frame visibility and extraction.
- Introduce or extend a core `bevy_lagrange` validation harness here, before the
  showcase workaround is removed, so retained line layer behavior is proven
  without depending on Fairy Dust or
  `selection_gizmo::sync_selection_gizmo_layers`.
- Explicitly test the allowed mixed state: retained lines inherit source-camera
  layers, while labels remain `UiTargetCamera`-based Bevy UI nodes.
- Define recursive cleanup for any generated line children. Either every child
  is owned by a despawn-recursive root with `FitOverlayVisual`, or children
  carry their own cleanup marker and are swept with the root.
- Remove the obsolete gizmo render-layer sync once no overlay line uses the
  gizmo backend.

#### Retrospective

**What worked:**

- `crates/bevy_lagrange/src/fit_overlay/lines.rs` now owns retained Core3d line
  roots for rectangle, silhouette, and margin lines.
- Building line quads from viewport-pixel offsets back into world space kept
  `FitTargetOverlayConfig::line_width` as a pixel-width setting.
- The retained-line index in `lines.rs` gives the line backend a stable
  upsert and layer-repair pattern without forcing a hash-keyed visual map.

**What deviated from the plan:**

- `FitTargetGizmo` and `sync_gizmo_render_layers` were removed in phase 3
  because no overlay line still used the gizmo backend.
- Line roots use `ExtendedMaterial<StandardMaterial, FitOverlayLineDepth>` so
  they keep unlit `StandardMaterial` color handling while specializing the
  Core3d pipeline to `depth_compare = Always` with depth writes disabled.
- The core non-Fairy-Dust coverage added in this phase is ECS/component coverage
  for layer propagation, layer repair, stable entity reuse, and picking ignore;
  render-to-texture or pixel-level checks remain later validation work.

**Surprises:**

- Target mesh extraction and retained overlay meshes share the same
  `Assets<Mesh>` resource, so `draw_fit_target_bounds` now uses one mutable mesh
  resource for both read and update work.
- Bevy 0.19's `World::query_filtered` API was needed for the new line backend
  tests.
- `MaterialPlugin::<FitOverlayLineMaterial>` requires `AssetServer`, so
  `ZoomOverlayPlugin` registers it only when asset infrastructure exists and
  otherwise initializes `Assets<FitOverlayLineMaterial>` for minimal test apps.

**Implications for remaining phases:**

- Phase 4 should keep labels on the existing Bevy UI path and repair the parts
  that make them disappear or intercept picking.
- Phase 4 should verify UI labels keep `FitOverlayVisual` ownership while using
  `UiTargetCamera`, not copied `RenderLayers`.
- Phase 5 should clean up stale `bevy_gizmos` feature wiring only after
  confirming no final fit-overlay or showcase backend depends on it.

#### Phase 3 Review

- Phase 4 now treats current UI label `FitOverlayVisual` identity as existing
  state to preserve, not work to reintroduce.
- Phase 4 now keeps label rendering on Bevy UI nodes and focuses on visibility,
  `UiTargetCamera` selection, `GlobalZIndex`, picking ignore, and cleanup.
- Phase 5 now treats gizmo and showcase-workaround cleanup as a final audit,
  because phase 3 removed only the fit-overlay line gizmo backend.
- Phase 5 now keeps `bevy_ui`, `bevy_text`, and picking-related feature wiring
  required by the final Bevy UI label path.

### 4. Bevy UI Label Visibility

**Status:** Complete

- Keep labels as plain Bevy UI text nodes. Do not introduce a retained,
  diegetic, `Text2d`, `Mesh2d`, or Core3d label backend in this phase.
- Continue using `FitOverlayVisual { camera, kind }` as label ownership
  identity, with `MarginLabel` and `BoundsLabel` as UI query tags only.
- Ensure margin labels and the bounds label are visible whenever the overlay
  frame is visible by targeting an active UI-renderable camera on the same
  normalized render target.
- Do not target cameras that cannot render Bevy UI; the fallback remains the
  source camera when no better active UI-renderable camera exists.
- Add `Pickable::IGNORE` to all generated label roots and repair existing label
  roots during updates.
- Add a high `GlobalZIndex` to generated label roots so overlay labels draw
  above ordinary UI on the selected UI camera, and repair existing label roots
  during updates.
- Preserve existing label text, color, anchor, and cleanup behavior for hidden
  margin edges, empty frames, removed `FitOverlay`, and orphan cameras.
- Add focused tests for UI camera selection, picking ignore, UI z-order, and
  retained ownership identity on Bevy UI label roots.
- Update Rust doc comments or internal comments as needed to state that labels
  are Bevy UI nodes using `UiTargetCamera`; only Core3d line visuals copy
  `RenderLayers`.

#### Retrospective

**What worked:**

- `crates/bevy_lagrange/src/fit_overlay/labels.rs` now uses one label-root
  bundle for `GlobalZIndex` and `Pickable::IGNORE` on both margin and bounds
  labels.
- The existing `FitOverlayVisual { camera, kind }` label identity was enough to
  preserve ownership while labels stayed plain Bevy UI nodes.

**What deviated from the plan:**

- The plan was rewritten before implementation to remove retained label backend
  work; Phase 4 stayed on Bevy UI labels.
- Existing label roots are repaired during update, not only when new labels are
  spawned, so preexisting labels regain z-order and ignored picking.

**Surprises:**

- Bevy UI rendering extracts only cameras with `Camera2d` or `Camera3d`, so
  `label_ui_camera` now filters same-target candidates to UI-renderable
  cameras.

**Implications for remaining phases:**

- Phase 5 should keep `UiTargetCamera`, `bevy_ui`, `bevy_text`, and required
  picking feature wiring.
- Phase 5 should only remove stale gizmo wiring or showcase workarounds after a
  final audit proves they are unused.

### 5. Feature Cleanup And Final Validation

**Status:** Complete

- Remove stale `bevy_gizmos` and replacement render feature flags only after
  retained lines and Bevy UI labels compile and pass focused tests.
- Split the feature audit into library overlay needs and showcase-only needs.
  Remove stale library feature wiring only when the library no longer depends
  on it, and preserve or relocate showcase-only features separately.
- Keep `bevy_ui`, `bevy_text`, picking, asset, and render feature flags required
  by the final line and Bevy UI label implementations.
- Verify showcase behavior no longer depends on the
  `selection_gizmo::sync_selection_gizmo_layers` workaround. Removing that
  workaround must not imply removing the showcase selection gizmo itself.
- Keep and expand the core `bevy_lagrange` validation harness introduced by
  phase 3; final validation must still not depend on Fairy Dust.
- Validate the Bevy UI label fallback contract: when no higher-order same-target
  UI-renderable camera exists, the source `FitOverlay` camera fallback must be
  UI-renderable for labels to be guaranteed visible, or the limitation must be
  documented.
- Name focused final checks for label UI-camera selection, label root repair,
  retained line layer repair, and render-level visibility for retained lines
  plus Bevy UI labels.
- Run the phase-gate checks listed under `Migration Guardrail`.

#### Retrospective

**What worked:**

- `crates/bevy_lagrange/Cargo.toml` no longer puts `bevy_gizmos` or
  `bevy_state` behind the library `fit_overlay` feature.
- `crates/bevy_lagrange/examples/showcase/selection_gizmo.rs` keeps the
  showcase selection outline while removing
  `selection_gizmo::sync_selection_gizmo_layers`.
- The source-camera UI fallback contract is now documented and covered by a
  focused `label_ui_camera` test.

**What deviated from the plan:**

- The showcase selection gizmo now renders on the default scene layer instead
  of mutating the OrbitCam between default and selection layers.
- No render-to-texture or pixel-level fit-overlay regression harness was added;
  this phase used the existing compile, ECS, and nextest gates plus documented
  render-level targets.

**Surprises:**

- Showcase-only state and gizmo needs are already satisfied by the example/dev
  dependency feature set, so they do not need to stay in `fit_overlay`.

#### Phase 4 Review

- Phase 5 now stays audit-only for labels and does not revisit label rendering
  architecture.
- Phase 5 now separates library overlay feature cleanup from showcase-only
  feature needs.
- Phase 5 now validates or documents the source-camera UI fallback contract for
  label visibility.
- Phase 5 now names focused final checks for label camera selection, label root
  repair, retained line layer repair, and render-level visibility.

## Accepted Review Refinements

These refinements were recorded by team review and are considered part of the
current design.

### Desired Frame And Reconciliation

Split the overlay implementation into two layers:

1. Resolve a per-camera `FitOverlayCameraContext` and desired
   `FitOverlayFrame`.
2. Diff that desired frame against retained `FitOverlayVisual` entities.

The frame model should distinguish visible and empty states:

```rust
enum FitOverlayFrame {
    Visible(FitOverlayLayout),
    Empty(FitOverlayEmptyReason),
}
```

Every camera with `FitOverlay` must be processed, even when it has no
`CurrentFitTarget`, the target was despawned, the target has no extractable
mesh, the viewport or target size is unavailable, the primary window cannot be
resolved, the camera is inactive, the projection is unsupported, or bounds
cannot be computed. Those cases produce `FitOverlayFrame::Empty`, which removes
all visuals for that camera and removes `FitMarginPercents`.

Cleanup must also remove orphan visuals whose owner camera no longer exists or
no longer has `FitOverlay`.

### Render Context

`RenderLayers` are one part of the render context, not the whole context. The
implementation should resolve a struct before layout/update work:

```rust
struct FitOverlayCameraContext {
    camera:            Entity,
    normalized_target: NormalizedRenderTarget,
    logical_viewport:  Rect,
    layers:            RenderLayers,
    order:             isize,
    is_active:         bool,
}
```

The effective layer value is recomputed every reconciliation pass. Removing a
camera's `RenderLayers` component updates existing Core3d line visuals back to
layer 0. Phase 2 introduced the shared effective-layer helper and phase 3 made
that repair meaningful for retained line entities. Bevy UI labels are
documented as a separate UI-camera path and do not copy source-camera
`RenderLayers`.

The target and viewport fields are layout inputs and cleanup keys. They do not
provide render isolation. Cameras with intersecting render layers can still see
the same retained overlay entities, even when their targets or viewports differ.

`NormalizedRenderTarget` is same-target identity for layout and cleanup. Image
or texture targets must use their own camera target information; they should not
fall back to primary-window sizing. If the camera lacks enough target or
viewport data to place the overlay, it produces `FitOverlayFrame::Empty`.

### Visual Identity

`FitOverlayVisual { camera, kind }` is a stable identity, but ECS does not
enforce uniqueness. The implementation must either:

- keep an owner index on the camera with slots for each visual kind, or
- build a per-frame map keyed by `(camera, FitOverlayVisualKind)` before
  upserting visuals.

Duplicate visuals with the same `(camera, kind)` should be treated as stale and
deduplicated during reconciliation.

Phase 2 introduced deterministic deduplication and cleanup. Phase 3 should add
the first full retained upsert helper once retained line entities exist. That
helper should prefer a per-frame index or map keyed by
`(camera, FitOverlayVisualKind)` over a retained owner index, keep one
deterministic survivor for each key, update that entity, repair copied
`RenderLayers`, and despawn duplicates. This is an internal implementation
choice, not a public API constraint. If `FitOverlayVisualKind` is hash-keyed,
`Edge` must derive `Hash`; otherwise the key type must avoid requiring `Hash`
on `Edge`.

The visual kind must encode geometry cardinality clearly. The intended
representation for this plan is:

- one mutable polyline mesh for the bounds rectangle,
- one mutable polyline mesh for the silhouette hull,
- one retained margin line visual per visible edge,
- one Bevy UI margin label per visible edge,
- one Bevy UI bounds label.

If a later implementation uses per-segment entities instead, the enum should be
changed to make that arity explicit, for example `BoundsEdge(Edge)` or
`SilhouetteSegment(usize)`.

### Retained Line Backend

The retained line backend must be specified before implementation. The expected
direction is a Core3d-compatible retained mesh path, not Bevy UI and not
Camera2d-only primitives.

The backend must define:

- whether lines are `Mesh3d` quad strips or another Core3d-compatible mesh,
- how `FitTargetOverlayConfig::line_width` stays pixel-stable,
- whether bounds and silhouette share a mutable polyline mesh per kind,
- how materials are cached by color,
- unlit material behavior,
- depth test/write behavior and the replacement for fit-overlay depth bias,
- culling behavior such as explicit bounds or `NoFrustumCulling`,
- shadow and picking behavior,
- the exact `fit_overlay` feature flags needed for the backend.

The expected render components are Core3d-compatible renderables with
`Visibility`, `InheritedVisibility`, `ViewVisibility`, `RenderLayers`,
`Transform`, `GlobalTransform`, explicit culling policy, no shadows, ignored
picking, and a material policy that is visible through the source `Camera3d`
path.

The overlay should preserve the current debug-annotation behavior: unlit
appearance, no depth writes, and a strict depth policy that prevents ordinary
scene meshes from hiding the bounds, silhouette, margin lines, or labels. A
transparent PBR material with `AlphaMode::Blend` is not enough by itself because
it can still depth-compare; the retained backend needs `depth_compare = Always`
or an equivalent Core3d-compatible material or pipeline path.

Picking must be disabled recursively. Every generated Core3d line entity and
every generated Bevy UI label root should carry `Pickable::IGNORE`. If a future
backend spawns generated children, those children must also ignore picking or
be spawned through a path that guarantees ignored picking for all children.

Cleanup must also be recursive for backends that spawn children. Either the
root entity carrying `FitOverlayVisual` owns generated children and cleanup uses
recursive despawn, or each generated child carries a cleanup marker that the
orphan sweep removes with the root.

### Bevy UI Label Backend

Labels remain plain Bevy UI text nodes for this plan. They are root UI nodes
with `Text`, `TextFont`, `TextColor`, `Node`, `UiTargetCamera`,
`GlobalZIndex`, `Pickable::IGNORE`, and `FitOverlayVisual` ownership identity.
`MarginLabel` and `BoundsLabel` remain UI query tags, not ownership identity.

The label path must preserve:

- default-font behavior through Bevy UI text,
- viewport-pixel anchor conversion through absolute UI `Node` placement,
- high UI stack ordering through `GlobalZIndex`,
- ignored picking on label roots,
- cleanup through `FitOverlayVisual { camera, kind }`,
- target-camera selection through `UiTargetCamera`.

UI labels do not inherit `RenderLayers`. They render through the selected
active UI-renderable camera on the same normalized render target. If no better
same-target UI-renderable camera exists, the source camera remains the fallback.
For guaranteed label visibility, that source camera must itself be
UI-renderable (`Camera2d` or `Camera3d`).

Moving labels to `bevy_diegetic` or another retained Core3d label backend is a
future design task outside this plan. Do not remove `UiTargetCamera`,
`bevy_ui`, or `bevy_text` while labels use this backend.

### Coordinate Types

Keep layout math separate from ECS mutation. The layout layer should name its
coordinate spaces explicitly, for example:

- normalized screen points,
- viewport pixel positions,
- overlay-plane world positions,
- final visual transforms.

This avoids mixing current UI pixel anchors with retained world-space render
entities.

### Scheduling

Retained visuals need current camera and target transforms and same-frame render
transforms. Add a dedicated `FitOverlaySystemSet`.

The implementation should either:

- schedule after camera and target `GlobalTransform` values are current, then
  write both `Transform` and `GlobalTransform` on root overlay visuals, or
- explicitly document and test a one-frame overlay latency.

The preferred design is same-frame correctness. Add coverage where the camera
and target move in the same frame.

The concrete schedule should live in `PostUpdate` in a dedicated
`FitOverlaySystemSet`, ordered after camera and target transforms are current
and before Bevy visibility and render extraction see stale overlay transforms.
If the update writes retained roots after transform propagation, it must also
write matching `GlobalTransform` values.

Phase 2 already introduced `FitOverlaySystemSet` with this broad ordering.
Remaining phases should reuse that set. Core3d line roots must keep same-frame
`Transform` and `GlobalTransform` updates; Bevy UI labels use UI `Node`
placement instead.

The set can remain `pub(crate)` unless a separate public scheduling use case
emerges. Do not make it a new public API requirement just to implement the
retained overlay.

Cleanup should be scheduled separately from drawing/reconciliation. The
`On<Remove, FitOverlay>` observer is the removal fast path, and the
owner-liveness sweep remains the required backstop so orphan cleanup still runs
when only stale visuals remain.

### Migration Guardrail

The final state for this plan is deliberately mixed: lines are retained
Core3d entities that copy source-camera `RenderLayers`, and labels are Bevy UI
nodes that use `UiTargetCamera`. The mismatch is expected, documented, and
tested.

The compile-safe migration path is:

1. Current gizmo/UI path.
2. Desired-frame and reconciliation path, still driving old backends.
3. Retained line backend, with the temporary mismatch tested while labels still
   use `UiTargetCamera`.
4. Bevy UI label visibility hardening.
5. Removal of stale feature flags made obsolete by the retained line backend
   and final Bevy UI label path.

Each phase should pass `cargo check -p bevy_lagrange --no-default-features`,
`cargo check -p bevy_lagrange --features fit_overlay`, and focused
`cargo nextest run` coverage before the next phase.

Phase gates should name the allowed backend components, allowed temporary
mismatches, and exact `fit_overlay` feature flags. At minimum, validate:

- `cargo check -p bevy_lagrange --no-default-features --features fit_overlay --all-targets`
- `cargo check -p bevy_lagrange --features fit_overlay --examples`
- `cargo check -p bevy_lagrange --features fit_overlay --example showcase`

The final `FitOverlay` implementation should not require external workarounds
for correctness. Overlay-generated lines and labels should ignore picking
themselves. Plain UI label retargeting through `UiTargetCamera` is part of the
final overlay feature until a separate future label-renderer plan replaces it.

Retained inspection targets such as `FitOverlayVisual`,
`FitOverlayVisualKind`, `FitMarginPercents`, empty-frame state, and backend
markers or resources should derive reflection under the `fit_overlay` feature.
They do not need to become public Rust API. Manual reflection registration is
only needed for targets Bevy does not auto-register, such as required generic
monomorphizations or validation-only state that lacks an automatic component or
resource registration path.

## Testing

Add coverage for:

- A single `FitOverlay` camera with no `RenderLayers` produces visuals on layer
  0.
- A `FitOverlay` camera with `RenderLayers::layer(n)` produces visuals on layer
  `n`.
- Removing a camera's `RenderLayers` component updates existing visuals back to
  layer 0.
- Changing a camera's `RenderLayers` updates existing visuals without
  respawning them.
- Two simultaneous `FitOverlay` cameras with different layers produce separate
  visuals with the correct owner camera and layers.
- Two simultaneous `FitOverlay` cameras on the same/default layer assert the
  configured-layer behavior: distinct owner-keyed visual sets, shared same-layer
  render visibility, and no owner isolation from `Camera.order`.
- Removing `FitOverlay` despawns all visuals for that camera and leaves visuals
  for other cameras intact.
- Labels and lines use the same `FitOverlayVisual` owner identity and cleanup
  rules; only Core3d lines use source-camera layer propagation.
- Invalid source states produce an empty desired frame and clear stale visuals.
- Duplicate `(camera, kind)` visuals are deduplicated in one reconciliation
  pass.
- Inactive source cameras produce an empty desired frame and clear stale
  visuals.
- `Camera::viewport`, second-window targets, image render targets, inactive
  cameras, screen-space overlay cameras, and same-frame camera/target movement.
- Showcase regression coverage confirms that selected mesh plus visible screen
  panels keeps hover/click behavior, and the overlay does not require mutating
  the OrbitCam's render layers through
  `selection_gizmo::sync_selection_gizmo_layers` as a workaround.
- At least one minimal core `bevy_lagrange` validation harness should cover
  overlay correctness without depending on the Fairy Dust showcase path.
- Render behavior, not only component state: at least one render-to-texture or
  pixel-level regression should prove lines appear through the intended
  `RenderLayers` path and labels appear through the intended UI target camera.
- Pixel checks should use deterministic overlay colors and assert line presence
  through an intersecting-layer camera path, line absence from a
  non-intersecting-layer camera path, and label presence through the chosen
  `UiTargetCamera`.
- BRP or ECS inspection should validate owner, layer, visual count, duplicate
  cleanup, and empty-frame state. Pixel checks should use render-to-texture or a
  focused screenshot ROI rather than full screenshot parity.

## Resolved Behavior

`FitOverlayVisual.camera` records update and cleanup ownership. It does not add
any render visibility filter beyond Bevy `RenderLayers`.

Generated Core3d line visuals copy the source camera's effective
`RenderLayers`. After that, they follow normal Bevy rendering behavior: any
camera with intersecting effective `RenderLayers` may render them, and cameras
with non-intersecting effective `RenderLayers` may not.

Generated Bevy UI labels use `UiTargetCamera` and do not copy `RenderLayers`.

This leaves visibility under user control:

- Cameras with non-overlapping `RenderLayers` see separate overlay line
  visuals.
- Cameras with overlapping `RenderLayers`, including the default layer 0, share
  normal Bevy layer-intersection visibility for line visuals.
- Labels render through the selected UI target camera on the same render target.
- `Camera.order` changes pass order only; it does not change layer visibility.
- `FitOverlay` does not add an overlay render-layer override or a separate
  overlay order setting.

## Public Contract

The intended user-facing contract is:

> Insert `FitOverlay` on a camera to create overlay visuals owned and
> reconciled by that camera. Generated Core3d line visuals inherit the camera's
> `RenderLayers`. Any camera with intersecting effective `RenderLayers` may
> render those line visuals in its normal camera pass, ordered by
> `Camera.order`. Generated labels are Bevy UI nodes targeted to an active
> same-target UI camera, falling back to the source camera when it is the only
> suitable UI-renderable camera.

No separate overlay order setting is required.

The retained line implementation is `RenderLayers`-driven. Users configure
line visibility the same way they configure visibility for other Bevy
renderables. To keep overlay lines visible only to a specific camera set, use
non-overlapping camera layers. To share overlay line visibility, use
overlapping layers or the default layer.

Screen-space overlay cameras do not become implicit line retargeting cameras.
They see retained overlay lines only through the normal layer-intersection
contract, or by having their own `FitOverlay`. They may be selected as
`UiTargetCamera` for labels when they are active, same-target, UI-renderable,
and highest order.
