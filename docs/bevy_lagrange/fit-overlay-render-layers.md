# Fit Overlay Render Layers

Design note for making `bevy_lagrange`'s `FitOverlay` render correctly with
multiple cameras, custom render layers, and screen-space overlay cameras.

The public contract below describes the target retained-overlay implementation.
The current implementation still uses a global `FitTargetGizmo` configuration
and plain UI label retargeting while the migration is in progress.

## Current Model

The fit overlay is enabled by inserting `FitOverlay` on a camera entity:

```rust
commands.entity(camera).insert(FitOverlay);
```

Removing that marker disables the overlay:

```rust
commands.entity(camera).remove::<FitOverlay>();
```

The current overlay draws its lines through Bevy gizmos:

```rust
Gizmos<FitTargetGizmo>
```

`FitTargetGizmo` has one global `GizmoConfig`. That config can carry one
`RenderLayers` value, so the current implementation copies layers from the first
camera it finds with `FitOverlay`.

The labels are currently plain Bevy UI text nodes. They carry ownership markers
such as `MarginLabel { camera, edge }` and `BoundsLabel { camera }`, plus a
`UiTargetCamera` chosen so labels render through the top active camera on the
same render target.

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

Both cameras need independent overlay visuals. Camera A's overlay should render
on layer 3. Camera B's overlay should render on layer 7.

Plain UI labels are also a weak fit for this feature. They require choosing a UI
target camera separately from the camera that owns the overlay, and they can be
covered by later camera passes unless retargeted.

## Goals

- Keep `FitOverlay` as the opt-in marker on the source camera.
- Let the source camera's `Camera.order` define render ordering.
- Propagate the source camera's `RenderLayers` to every generated overlay
  visual.
- Support multiple simultaneous `FitOverlay` cameras with different render
  layers.
- Track generated overlay entities so they can be updated in place and cleaned
  up when `FitOverlay` is removed.
- Move labels into the same ownership model as lines, so labels do not depend on
  Bevy UI target-camera selection.
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
/// Generated overlay visuals copy this camera's effective `RenderLayers` and
/// render in normal Bevy camera passes. `FitOverlay` owns overlay update and
/// cleanup; it does not add any render visibility filter beyond Bevy
/// `RenderLayers`.
#[derive(Component, Reflect, Default)]
pub struct FitOverlay;
```

The overlay system generates retained visual entities for each camera that has
`FitOverlay`.

Each generated entity carries a marker tying it to the source camera and to the
specific overlay part it represents:

```rust
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct FitOverlayVisual {
    camera: Entity,
    kind:   FitOverlayVisualKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum FitOverlayVisualKind {
    BoundsRect,
    Silhouette,
    MarginLine(Edge),
    MarginLabel(Edge),
    BoundsLabel,
}
```

The `camera` field answers which `FitOverlay` camera owns the visual. The `kind`
field answers which overlay part the visual represents. Together they form the
stable identity used for update, reuse, and cleanup.

The marker is an update and cleanup identity. It is not, by itself, a Bevy
render visibility filter.

## Render Layers

Each generated visual inherits the source camera's render layers:

```rust
let layers = camera_layers
    .cloned()
    .unwrap_or_else(|| RenderLayers::layer(0));

commands.spawn((
    FitOverlayVisual {
        camera,
        kind: FitOverlayVisualKind::MarginLine(Edge::Left),
    },
    layers,
    // visual bundle
));
```

Inheritance here means copy-on-reconcile from the source camera. It is not ECS
hierarchy inheritance, and the fit target entity's render layers do not
contribute to the overlay visual layers.

When the source camera's layers change, the overlay system updates all visuals
owned by that camera:

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
  -> all camera_a overlay visuals get RenderLayers::layer(3)

camera_b: FitOverlay + RenderLayers::layer(7)
  -> all camera_b overlay visuals get RenderLayers::layer(7)
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
4. Update geometry, color, text, transform, and render layers in place.
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

Labels should eventually use the same `FitOverlayVisual` ownership model as
lines:

```rust
FitOverlayVisual {
    camera,
    kind: FitOverlayVisualKind::MarginLabel(Edge::Left),
}
```

That removes the need for the current plain-UI label retargeting workaround:

```rust
UiTargetCamera(top_camera_on_same_render_target)
```

The label implementation must remain vanilla Bevy until `bevy_diegetic` is a
published dependency that `bevy_lagrange` can use. The important design
requirement is that labels become retained overlay render entities that inherit
the source camera's `RenderLayers`, not root UI nodes whose visibility depends
on a separate UI camera pass.

## Implementation Phases

These phases are intended as sequential committable units.

### 1. Public Contract And Ownership Markers

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

### 2. Desired Frame And Reconciliation

- Split the implementation into context/layout, reconciliation, retained line
  backend, retained label backend, and plugin wiring.
- Introduce `FitOverlayCameraContext` and `FitOverlayFrame`, while still driving
  the old gizmo/UI render backends.
- Process every camera with `FitOverlay` using optional state so missing or
  invalid inputs produce `FitOverlayFrame::Empty`.
- Reconcile retained identities with a per-frame `(camera, kind)` map, repair
  copied `RenderLayers`, remove stale visuals, and deduplicate duplicates.
- Add orphan cleanup that is not gated only by active `FitOverlay` cameras.

### 3. Retained Line Backend

- Replace transient `Gizmos<FitTargetGizmo>` line calls with retained
  Core3d-compatible line visual entities.
- Propagate the source camera's effective `RenderLayers` to every generated
  line visual during reconciliation.
- Implement the line material, depth, visibility, culling, shadow, and picking
  policies defined below.
- Remove `sync_gizmo_render_layers` only after no overlay line uses
  `FitTargetGizmo`.

### 4. Retained Label Backend

- Move labels from Bevy UI nodes to retained overlay visual entities.
- Propagate the source camera's effective `RenderLayers` to every label visual.
- Replace `UiTargetCamera` retargeting with the retained label renderer.
- Preserve label positioning, scaling, depth, picking, and visibility behavior
  through the same source-camera render path as the retained lines.

### 5. Feature Cleanup And Final Validation

- Remove stale `bevy_gizmos`, `bevy_ui`, and replacement render feature flags
  only after retained lines and labels compile and pass focused tests.
- Verify showcase behavior no longer depends on the
  `selection_gizmo::sync_selection_gizmo_layers` workaround.
- Keep at least one core `bevy_lagrange` validation harness that does not depend
  on Fairy Dust.
- Run the phase-gate checks listed under `Migration Guardrail`.

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
camera's `RenderLayers` component updates existing visuals back to layer 0.

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

The first implementation should prefer a per-frame map keyed by
`(camera, FitOverlayVisualKind)` over a retained owner index. It should keep one
deterministic survivor for each key, update that entity, and despawn duplicates.
If `FitOverlayVisualKind` is hash-keyed, `Edge` must derive `Hash`; otherwise
the map key must avoid requiring `Hash` on `Edge`.

The visual kind must encode geometry cardinality clearly. The intended retained
representation is:

- one mutable polyline mesh for the bounds rectangle,
- one mutable polyline mesh for the silhouette hull,
- one retained margin line visual per visible edge,
- one retained margin label visual per visible edge,
- one retained bounds label visual.

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
- depth test/write behavior and the replacement for `OVERLAY_GIZMO_DEPTH_BIAS`,
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

Picking must be disabled recursively. Every generated render entity, including
glyph, quad, and child mesh entities, should carry `Pickable::IGNORE` or be
spawned through a backend path that guarantees ignored picking for all children.

### Label Backend

Labels need a concrete retained renderer before the UI labels are removed.
`Text2d` and `Mesh2d` are not enough for the normal `Camera3d` OrbitCam path if
they only queue through 2D rendering. The retained label backend must be
Core3d-compatible.

The label design must specify:

- the vanilla Bevy representation for glyphs, such as textured glyph quads or a
  custom unlit mesh text path,
- font/default-font expectations,
- viewport-pixel anchor conversion to world-space overlay placement,
- billboarding or camera-facing orientation,
- text scale under perspective and orthographic projections,
- clipping or edge behavior near viewport bounds,
- inherited `RenderLayers`,
- depth/write/bias behavior matching the line overlay.

While UI labels temporarily carry `FitOverlayVisual`, that marker is identity
only. Those labels are not considered render-layer-correct until the retained
Core3d-compatible label backend replaces `UiTargetCamera`.

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

The set can remain `pub(crate)` unless a separate public scheduling use case
emerges. Do not make it a new public API requirement just to implement the
retained overlay.

Cleanup should be scheduled separately from drawing/reconciliation. Use
`RemovedComponents<FitOverlay>` as a fast path, but keep an owner-liveness sweep
so orphan cleanup still runs when only stale visuals remain.

### Migration Guardrail

Avoid shipping a mixed final state where lines are retained layer-inheriting
entities but labels still depend on UI target-camera retargeting. Either keep
the old gizmo/UI paths together until retained lines and labels are both ready,
or mark the mixed state as temporary and test the expected mismatch.

The compile-safe migration path is:

1. Current gizmo/UI path.
2. Desired-frame and reconciliation path, still driving old backends.
3. Retained line backend.
4. Retained label backend.
5. Removal of `UiTargetCamera`, `FitTargetGizmo`, `sync_gizmo_render_layers`,
   and stale feature flags.

Each phase should pass `cargo check -p bevy_lagrange --no-default-features`,
`cargo check -p bevy_lagrange --features fit_overlay`, and focused
`cargo nextest run` coverage before the next phase.

Phase gates should name the allowed backend components, allowed temporary
mismatches, and exact `fit_overlay` feature flags. At minimum, validate:

- `cargo check -p bevy_lagrange --no-default-features --features fit_overlay --all-targets`
- `cargo check -p bevy_lagrange --features fit_overlay --examples`
- `cargo check -p bevy_lagrange --features fit_overlay --example showcase`

The final `FitOverlay` implementation should not require external workarounds
for correctness. Overlay-generated visuals should ignore picking themselves,
and plain UI label retargeting should not be part of the final overlay feature.

Retained inspection targets such as `FitOverlayVisual`,
`FitOverlayVisualKind`, `FitMarginPercents`, empty-frame state, and backend
markers or resources should derive and be registered for reflection when the
`fit_overlay` feature is enabled. They do not need to become public API, but BRP
validation needs inspectable component and resource data.

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
- Labels and lines use the same owner and layer propagation rules.
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
  pixel-level regression should prove labels and lines appear through the
  intended camera path.
- Pixel checks should use deterministic overlay colors and assert both presence
  through an intersecting-layer camera path and absence from a
  non-intersecting-layer camera path.
- BRP or ECS inspection should validate owner, layer, visual count, duplicate
  cleanup, and empty-frame state. Pixel checks should use render-to-texture or a
  focused screenshot ROI rather than full screenshot parity.

## Resolved Behavior

`FitOverlayVisual.camera` records update and cleanup ownership. It does not add
any render visibility filter beyond Bevy `RenderLayers`.

Generated overlay visuals copy the source camera's effective `RenderLayers`.
After that, they follow normal Bevy rendering behavior: any camera with
intersecting effective `RenderLayers` may render them, and cameras with
non-intersecting effective `RenderLayers` may not.

This leaves visibility under user control:

- Cameras with non-overlapping `RenderLayers` see separate overlay visuals.
- Cameras with overlapping `RenderLayers`, including the default layer 0, share
  normal Bevy layer-intersection visibility.
- `Camera.order` changes pass order only; it does not change layer visibility.
- `FitOverlay` does not add an overlay render-layer override or a separate
  overlay order setting.

## Public Contract

The intended user-facing contract is:

> Insert `FitOverlay` on a camera to create overlay visuals owned and
> reconciled by that camera. Generated visuals inherit the camera's
> `RenderLayers`. Any camera with intersecting effective `RenderLayers` may
> render those visuals in its normal camera pass, ordered by `Camera.order`.

No separate overlay order setting is required.

The retained implementation is `RenderLayers`-driven. Users configure overlay
visibility the same way they configure visibility for other Bevy renderables. To
keep an overlay visible only to a specific camera set, use non-overlapping
camera layers. To share overlay visibility, use overlapping layers or the
default layer.

Screen-space overlay cameras do not become implicit label or line retargeting
cameras. They see retained overlay visuals only through the normal
layer-intersection contract, or by having their own `FitOverlay`.
