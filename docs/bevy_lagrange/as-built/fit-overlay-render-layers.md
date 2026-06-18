# Fit Overlay Render Layers

How `bevy_lagrange`'s `FitOverlay` renders with multiple cameras, custom render
layers, and screen-space overlay cameras. The overlay draws retained Core3d line
meshes plus plain Bevy UI labels.

Source: `crates/bevy_lagrange/src/fit_overlay/`.

## Model

The fit overlay is enabled by inserting `FitOverlay` on a camera entity:

```rust
commands.entity(camera).insert(FitOverlay);
```

Removing that marker disables the overlay:

```rust
commands.entity(camera).remove::<FitOverlay>();
```

`FitOverlay` is a zero-config marker (`components.rs`). Render layers and pass
order come from the camera; `FitTargetOverlayConfig` carries visual appearance.

Lines are retained Core3d mesh entities carrying `Mesh3d`,
`MeshMaterial3d<FitOverlayLineMaterial>`, and `FitOverlayLineVisual { color,
width }`. Each line root also carries `FitOverlayVisual { camera, kind }`, the
source camera's effective `RenderLayers`, identity `Transform`/`GlobalTransform`,
`Visibility`/`InheritedVisibility`/`ViewVisibility`, `NoFrustumCulling`,
`NotShadowCaster`, and `Pickable::IGNORE` (`lines::retained_line_components`).

`FitOverlayLineMaterial` is
`ExtendedMaterial<StandardMaterial, FitOverlayLineDepth>`. The base
`StandardMaterial` is `unlit` with `AlphaMode::Blend` and `cull_mode: None`; the
`FitOverlayLineDepth` extension specializes the pipeline to `depth_compare =
Always` with depth writes disabled, so scene meshes never hide the overlay.
Materials are cached by color in `FitOverlayLineMaterials`.

Labels are plain Bevy UI text nodes (`labels.rs`). They carry `FitOverlayVisual`
ownership identity plus `MarginLabel`/`BoundsLabel` query tags, `GlobalZIndex`,
`Pickable::IGNORE`, and a `UiTargetCamera` chosen so labels render through an
active camera on the same render target. Labels do not copy `RenderLayers`.

This supports multiple simultaneous `FitOverlay` cameras with different layers.
Each camera owns an independent set of line visuals on its own layer:

```rust
camera_a: FitOverlay + RenderLayers::layer(3)  // camera_a lines on layer 3
camera_b: FitOverlay + RenderLayers::layer(7)  // camera_b lines on layer 7
```

## FitOverlay marker

`FitOverlay` is gated behind the `fit_overlay` feature. Its doc comment states
the render-layer behavior:

```rust
#[derive(Component, Reflect, Default)]
#[reflect(Component, Default)]
pub struct FitOverlay;
```

The overlay system generates retained line visual entities and updates Bevy UI
label entities for each camera that has `FitOverlay`. Each generated entity
carries a marker tying it to the source camera and to the overlay part it
represents:

```rust
#[derive(Component, Reflect, Clone, Copy, Debug, PartialEq, Eq)]
#[reflect(Component)]
pub(super) struct FitOverlayVisual {
    pub(super) camera: Entity,
    pub(super) kind:   FitOverlayVisualKind,
}

#[derive(Reflect, Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum FitOverlayVisualKind {
    Rectangle,
    Silhouette,
    MarginLine { edge: Edge },
    MarginLabel { edge: Edge },
    BoundsLabel,
}
```

`camera` is the owning `FitOverlay` camera; `kind` is the overlay part.
Together they form the stable identity used for update, reuse, and cleanup. The
marker is an update/cleanup identity, not a Bevy render visibility filter. ECS
does not enforce uniqueness, so `deduplicate_fit_overlay_visuals` despawns any
duplicate `(camera, kind)` each frame, keeping one deterministic survivor.

`FitOverlayVisual` and `FitOverlayVisualKind` stay internal to `fit_overlay`;
they derive reflection for BRP inspection rather than being public Rust API.

## Render layers

Each generated line visual copies the source camera's effective `RenderLayers`,
defaulting to layer 0 when the component is absent
(`context::effective_render_layers`, via `RenderLayers::cloned().unwrap_or_default()`).
Inheritance here means copy-on-reconcile from the source camera. It is not ECS
hierarchy inheritance, and the fit target entity's render layers do not
contribute to the overlay visual layers.

The effective layer is recomputed every reconciliation pass.
`reconciliation::repair_render_layers` reinserts the layers on a survivor entity
only when they differ from the current value, so changing or removing the source
camera's `RenderLayers` repairs existing line visuals in place (removal falls
back to layer 0) without respawning them.

After the copy, lines follow normal Bevy rendering: any camera with intersecting
effective `RenderLayers` may render them; cameras with non-intersecting layers
may not. Screen-space overlay cameras are not implicit retargeting cameras for
lines — they see overlay lines only through layer intersection or by having
their own `FitOverlay`.

Bevy UI labels do not copy source-camera `RenderLayers`. They render through the
selected `UiTargetCamera` on the same normalized render target.

## Camera order

`FitOverlay` has no order field. `Camera::order` defines when a camera renders
relative to others; configure the camera that owns `FitOverlay` to change
overlay timing. Order controls pass order only — it is not part of visual
identity, upsert keys, or per-visual sorting, and it does not isolate overlay
visuals from another camera on intersecting render layers.

```rust
commands.spawn((
    Camera { order: 10, ..default() },
    FitOverlay,
));
```

## Camera context and desired frame

The overlay separates per-camera context/layout resolution from ECS diffing.
`FitOverlayCameraContext::resolve` produces the render context before layout:

```rust
pub(super) struct FitOverlayCameraContext {
    pub(super) camera:            Entity,
    pub(super) normalized_target: NormalizedRenderTarget,
    pub(super) logical_viewport:  Rect,
    pub(super) layers:            RenderLayers,
    pub(super) order:             isize,
    pub(super) is_active:         bool,
}
```

`normalized_target` is same-target identity for layout and cleanup. The target
and viewport fields are layout inputs and cleanup keys; they do not provide
render isolation. Cameras with intersecting render layers can still see the same
retained line entities even when their targets or viewports differ. Image or
texture targets use their own camera target information rather than falling back
to primary-window sizing.

Layout produces a desired frame per camera:

```rust
pub(super) enum FitOverlayFrame {
    Visible(Box<FitOverlayLayout>),
    Empty(FitOverlayEmptyReason),
}
```

Every camera with `FitOverlay` is processed, even when it has no
`CurrentFitTarget`, the target was despawned, the target has no extractable
mesh, the viewport is unavailable, the render target cannot be normalized, the
camera is inactive, the projection is unsupported, or bounds cannot be computed.
Those cases produce `FitOverlayFrame::Empty(reason)`:

```rust
pub(super) enum FitOverlayEmptyReason {
    InactiveCamera,
    MissingRenderTarget,
    MissingViewport,
    MissingCurrentFitTarget,
    MissingMesh,
    UnsupportedProjection,
    UnprojectableBounds,
    MissingDepths,
}
```

An empty frame (`reconciliation::clear_empty_frame`) removes all visuals for
that camera and removes `FitMarginPercents` from the camera.

## Visual lifecycle

`draw_fit_target_bounds` runs each frame for each camera with `FitOverlay`:

1. Resolve the camera context and desired frame.
2. On `Visible`: upsert the rectangle, silhouette, margin lines, bounds label,
   and margin labels for visible edges, updating geometry, render layers, and UI
   label text/placement in place.
3. Insert `FitMarginPercents` on the camera (via `try_insert`, which skips a
   despawned entity) for BRP inspection.
4. Despawn line visuals and margin labels whose kind is no longer in the desired
   set (`clear_stale_lines`, `cleanup_stale_margin_labels`).
5. On `Empty`: clear all visuals for the camera and remove `FitMarginPercents`.

Line upsert (`FitOverlayLineContext::upsert_polyline`) finds the survivor for a
`(camera, kind)` in the retained line index, updates its mesh in place (mutating
the existing `Assets<Mesh>` entry when possible), repairs render layers, and
reuses the entity; otherwise it spawns. A line whose geometry collapses is
despawned.

Removal and orphan cleanup:

- `on_remove_fit_visualization` (an `On<Remove, FitOverlay>` observer) is the
  removal fast path: it clears all visuals for the removed camera and removes
  `FitMarginPercents`.
- `cleanup_orphan_fit_overlay_visuals` is the backstop sweep: it despawns any
  `FitOverlayVisual` whose owner camera no longer exists or no longer has
  `FitOverlay`, and removes stale `FitMarginPercents`. It is not gated by
  `any_with_component::<FitOverlay>`, so it runs even when the only remaining
  work is stale visuals.

Despawn is recursive (`despawn_visual_root` calls `despawn_children().despawn()`)
so any generated children are swept with the root.

## Scheduling

`FitOverlaySystemSet` runs in `PostUpdate`, ordered
`after(TransformSystems::Propagate)` and
`before(VisibilitySystems::VisibilityPropagate)` and
`before(VisibilitySystems::CheckVisibility)`. Line roots are written after
transform propagation, so they receive matching identity `Transform` and
`GlobalTransform` for same-frame visibility and extraction.

`deduplicate_fit_overlay_visuals` then `draw_fit_target_bounds` run chained in
the set under `run_if(any_with_component::<FitOverlay>)`.
`cleanup_orphan_fit_overlay_visuals` runs in the set without that gate.

`FitOverlaySystemSet` is `pub(crate)`.

## Labels

Labels use the same `FitOverlayVisual` ownership marker as lines, with
`MarginLabel`/`BoundsLabel` as UI query tags only. They are root UI nodes with
`Text`, `TextFont`, `TextColor`, `Node`, `UiTargetCamera`,
`GlobalZIndex(OVERLAY_LABEL_Z_INDEX)` (1_000_000, so overlay labels draw above
ordinary UI), and `Pickable::IGNORE`. Existing label roots are repaired on update
(`repair_label_root`), not only on spawn, so preexisting labels regain z-order
and ignored picking.

`label_ui_camera` selects the highest `(Camera::order, Entity)` active,
UI-renderable camera on the same normalized render target. Bevy UI only extracts
`Camera2d`/`Camera3d` views, so non-UI cameras on the same target are skipped. If
no better same-target UI-renderable camera exists, the source `FitOverlay`
camera is the fallback; for guaranteed label visibility that source camera must
itself be UI-renderable (`Camera2d` or `Camera3d`).

Viewport-pixel anchors convert to absolute UI `Node` placement
(`apply_margin_label_anchor`, label position helpers). The default-font Bevy UI
text path is retained; moving labels to a retained Core3d label backend is a
future task. Do not remove `UiTargetCamera`, `bevy_ui`, or `bevy_text` while
labels use this backend.

## Coordinate spaces

Layout math is kept separate from ECS mutation. The layout layer names its
coordinate spaces — normalized screen points, viewport pixel positions,
overlay-plane world positions, and final visual transforms — so UI pixel anchors
(labels) and world-space render entities (lines) do not mix. Line quads are
built from viewport-pixel offsets converted back to overlay-plane world space,
which keeps `FitTargetOverlayConfig::line_width` a pixel-width setting.

## Public contract

> Insert `FitOverlay` on a camera to create overlay visuals owned and
> reconciled by that camera. Generated Core3d line visuals inherit the camera's
> `RenderLayers`. Any camera with intersecting effective `RenderLayers` may
> render those line visuals in its normal camera pass, ordered by
> `Camera::order`. Generated labels are Bevy UI nodes targeted to an active
> same-target UI camera, falling back to the source camera when it is the only
> suitable UI-renderable camera.

There is no separate overlay order setting and no overlay render-layer override:
users configure line visibility through the source camera's `RenderLayers` (use
non-overlapping layers to keep lines camera-specific, overlapping or default
layers to share them).
