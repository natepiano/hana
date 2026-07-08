# Fit module restructure plan

The `fit` domain is the right home for target framing, zoom, look, animation-to-fit,
and the debug overlay. The recommended restructure keeps that domain intact while
reducing flat module count, moving camera-pose helpers and geometry math behind
clearer owners, and splitting the oversized files that now have multiple type
clusters.

## Phase overview

| Phase | What | Status |
|-------|------|--------|
| 1 | Placement - group fit geometry, camera-pose helpers, and overlay internals | Done — see `docs/bevy_lagrange/as-built/free-cam.md` |
| 2 | Split `fit/triggers/look.rs` | Done — see `docs/bevy_lagrange/as-built/free-cam.md` |
| 3 | Split `fit/geometry/solve.rs` | Done — see `docs/bevy_lagrange/as-built/free-cam.md` |
| 4 | Split `fit/overlay/render/fit_target_bounds.rs` | Done — see `docs/bevy_lagrange/as-built/free-cam.md` |

## Phase 1 - Placement

**Status:** implemented — see `docs/bevy_lagrange/as-built/free-cam.md` (fit/ restructure, "How it works").

### Proposed layout

```text
fit/
  mod.rs                  # FitPlugin plus fit-domain exports
  constants.rs            # constants shared across fit behaviors
  target.rs               # CurrentFitTarget and SetFitTarget lifecycle
  camera_pose.rs          # FreeCamFitPose + SnapOrbit camera application
  geometry/
    mod.rs                # fit geometry exports
    anchor.rs             # FitAnchor
    projection.rs         # projection basis, projected bounds, vertex extraction
    solve/
      mod.rs              # solve exports
      fit_solution.rs     # FitSolution and FitError
      focus.rs            # focus centering/anchoring correction
      margins.rs          # target/constraining margin math
      radius_search.rs    # calculate_fit and binary search
  triggers/
    mod.rs                # fit trigger exports and observer entry points
    animate.rs            # AnimateToFit event and per-camera observers
    look/
      mod.rs              # look trigger exports
      look_at.rs          # LookAt event and observers
      look_at_and_zoom_to_fit.rs # LookAtAndZoomToFit event and observers
      plan.rs             # shared look-at planning
      support.rs          # local animation trigger helpers
    request.rs            # FitRequest and target mesh extraction front door
    zoom.rs               # ZoomToFit event, lifecycle events, and observers
  overlay/
    mod.rs                # FitOverlayPlugin and overlay exports
    constants.rs          # overlay-only constants
    geometry/
      mod.rs              # overlay screen geometry exports
      context.rs          # FitOverlayCameraContext and empty reasons
      convex_hull.rs      # silhouette hull projection
      edge.rs             # Edge, moved from the fit solver
      frame.rs            # FitOverlayFrame and FitOverlayLayout
      screen_space.rs     # screen-edge and margin coordinate helpers
    render/
      mod.rs              # overlay rendering exports
      fit_target_bounds/
        mod.rs            # bounds-render exports
        config.rs         # FitTargetOverlayConfig
        margin_lines.rs   # margin lines and labels
        target_bounds.rs  # draw_fit_target_bounds system
        ui_camera.rs      # UI camera selection
      labels.rs           # retained UI labels
      lines.rs            # retained line mesh/material helpers
      reconciliation.rs   # stale visual cleanup
      visual.rs           # FitOverlayVisual identity
```

### Moves, with rationale

#### `fit/geometry/`

Move `anchor.rs`, `projection.rs`, and `solve.rs` into `fit/geometry/`.
These files form the pure geometry and solve layer: `FitAnchor` selects the
viewport anchor, `projection.rs` converts world points into screen bounds, and
`solve.rs` computes `FitSolution`. This keeps the core fit math separate from
ECS observers and camera state application.

#### `fit/camera_pose.rs`

Merge `free.rs` and `snap_orbit.rs` into a flat `fit/camera_pose.rs`. Both
clusters apply a fit result to a concrete camera representation:
`FreeCamFitPose` maps fit output onto free-camera position/look/roll, while
`SnapOrbit` maps fit output onto OrbitCam operations. A flat module keeps these
small clusters owned directly by `fit`, so the existing `pub(super)` boundary
continues to work for `fit::triggers::*` descendants without path-scoped
visibility.

#### `fit/overlay/geometry/`

Move `context.rs`, `convex_hull.rs`, `frame.rs`, and `screen_space.rs` under
`overlay/geometry/`, and move `Edge` out of `fit/geometry/solve.rs` into
`overlay/geometry/edge.rs`. `Edge` is currently produced by the solver module
but all production uses are overlay rendering and screen-space helpers. This
move puts the type with its real owner.

#### `fit/overlay/render/`

Move `fit_target_bounds.rs`, `labels.rs`, `lines.rs`, `reconciliation.rs`, and
`visual.rs` under `overlay/render/`. These files own retained overlay visuals,
labels, materials, stale visual cleanup, and the draw system. Grouping them
keeps feature-gated debug rendering separate from overlay geometry.

#### `fit/triggers/`

Move `animate.rs`, `look.rs`, `request.rs`, and `zoom.rs` into
`fit/triggers/`. These files own caller-triggered fit behavior: public trigger
events, per-camera observers, and the shared `FitRequest` front door. Grouping
them brings the fit root to the singleton budget while keeping behavior triggers
separate from geometry and camera-pose application.

### What stays where

- `target.rs` stays at the `fit/` root because `FitPlugin` directly registers
  the `SetFitTarget` lifecycle observer.
- `constants.rs` stays at the `fit/` root because its constants are shared by
  multiple fit behaviors. `overlay/constants.rs` stays at the overlay root for
  the same reason within the overlay feature.
- `overlay/mod.rs` remains the feature-gated plugin root. It should read as a
  table of contents and own plugin registration only.

### Module re-exports

`fit/mod.rs`:

```rust
mod camera_pose;
mod constants;
mod geometry;
mod target;
mod triggers;

#[cfg(feature = "fit_overlay")]
mod overlay;

pub use geometry::FitAnchor;
pub use target::CurrentFitTarget;
pub use target::SetFitTarget;
pub use triggers::AnimateToFit;
pub use triggers::LookAt;
pub use triggers::LookAtAndZoomToFit;
pub use triggers::ZoomBegin;
pub use triggers::ZoomContext;
pub use triggers::ZoomEnd;
pub use triggers::ZoomReason;
pub use triggers::ZoomToFit;
#[cfg(feature = "fit_overlay")]
pub use overlay::FitOverlay;
#[cfg(feature = "fit_overlay")]
pub use overlay::FitTargetOverlayConfig;

pub(crate) use triggers::on_free_cam_animate_to_fit;
pub(crate) use triggers::on_free_cam_look_at;
pub(crate) use triggers::on_free_cam_look_at_and_zoom_to_fit;
pub(crate) use triggers::on_free_cam_zoom_to_fit;
pub(crate) use triggers::on_orbit_cam_animate_to_fit;
pub(crate) use triggers::on_orbit_cam_look_at;
pub(crate) use triggers::on_orbit_cam_look_at_and_zoom_to_fit;
pub(crate) use triggers::on_orbit_cam_zoom_to_fit;
#[cfg(feature = "fit_overlay")]
use overlay::FitOverlayPlugin;
```

`fit/camera_pose.rs`:

```rust
// No child modules. `fit/camera_pose.rs` owns `FreeCamFitPose`, `SnapOrbit`,
// and the small camera-application helpers directly.
```

`fit/geometry/mod.rs`:

```rust
mod anchor;
mod projection;
mod solve;

pub use anchor::FitAnchor;
#[cfg(feature = "fit_overlay")]
pub(super) use projection::ProjectionBasis;
#[cfg(feature = "fit_overlay")]
pub(super) use projection::ProjectionMode;
#[cfg(feature = "fit_overlay")]
pub(super) use projection::ScreenSpaceBounds;
pub(super) use projection::extract_mesh_vertices;
#[cfg(feature = "fit_overlay")]
pub(super) use projection::project_point;
#[cfg(feature = "fit_overlay")]
pub(super) use projection::projection_aspect_ratio;
pub(super) use solve::FitSolution;
pub(super) use solve::calculate_fit;
```

`fit/triggers/mod.rs`:

```rust
mod animate;
mod look;
mod request;
mod zoom;

pub use animate::AnimateToFit;
pub(crate) use animate::on_free_cam_animate_to_fit;
pub(crate) use animate::on_orbit_cam_animate_to_fit;
pub use look::LookAt;
pub use look::LookAtAndZoomToFit;
pub(crate) use look::on_free_cam_look_at;
pub(crate) use look::on_free_cam_look_at_and_zoom_to_fit;
pub(crate) use look::on_orbit_cam_look_at;
pub(crate) use look::on_orbit_cam_look_at_and_zoom_to_fit;
pub use zoom::ZoomBegin;
pub use zoom::ZoomContext;
pub use zoom::ZoomEnd;
pub use zoom::ZoomReason;
pub use zoom::ZoomToFit;
pub(crate) use zoom::on_free_cam_zoom_to_fit;
pub(crate) use zoom::on_orbit_cam_zoom_to_fit;
```

`fit/overlay/mod.rs`:

```rust
mod constants;
mod geometry;
mod render;

pub use render::FitTargetOverlayConfig;
use render::cleanup_orphan_fit_overlay_visuals;
use render::deduplicate_fit_overlay_visuals;
use render::FitOverlayLineMaterial;
use render::FitOverlayLineMaterials;
use render::draw_fit_target_bounds;
use render::on_remove_fit_visualization;
```

`fit/overlay/geometry/mod.rs`:

```rust
mod context;
mod convex_hull;
mod edge;
mod frame;
mod screen_space;

#[cfg(test)]
pub(super) use context::FitOverlayCameraContext;
pub(super) use context::FitOverlayEmptyReason;
pub(super) use convex_hull::convex_hull_2d;
pub(super) use convex_hull::project_vertices_to_2d;
pub(super) use edge::Edge;
pub(super) use frame::FitOverlayFrame;
pub(super) use frame::FitOverlayLayout;
pub(super) use frame::resolve_fit_overlay_frame;
pub(super) use screen_space::MarginBalance;
pub(super) use screen_space::boundary_edge_center;
pub(super) use screen_space::horizontal_balance;
pub(super) use screen_space::margin_percentage;
pub(super) use screen_space::norm_to_viewport;
pub(super) use screen_space::normalized_to_world;
pub(super) use screen_space::screen_edge_center;
pub(super) use screen_space::vertical_balance;
```

`fit/overlay/render/mod.rs`:

```rust
mod fit_target_bounds;
mod labels;
mod lines;
mod reconciliation;
mod visual;

pub use fit_target_bounds::FitTargetOverlayConfig;
pub(super) use fit_target_bounds::draw_fit_target_bounds;
pub(super) use fit_target_bounds::on_remove_fit_visualization;
pub(super) use reconciliation::cleanup_orphan_fit_overlay_visuals;
pub(super) use reconciliation::deduplicate_fit_overlay_visuals;
pub(super) use lines::FitOverlayLineMaterial;
pub(super) use lines::FitOverlayLineMaterials;
```

`fit/overlay/render/fit_target_bounds/mod.rs`:

```rust
mod config;
mod margin_lines;
mod target_bounds;
mod ui_camera;

pub use config::FitTargetOverlayConfig;
pub(crate) use target_bounds::FitMarginPercents;
pub(crate) use target_bounds::draw_fit_target_bounds;
pub(crate) use target_bounds::on_remove_fit_visualization;
```

### Sequencing

1. Move `fit/anchor.rs`, `fit/projection.rs`, and `fit/solve.rs` into
   `fit/geometry/`; update imports and keep geometry-only helpers behind the
   geometry facade.
2. Merge `fit/free.rs` and `fit/snap_orbit.rs` into `fit/camera_pose.rs`;
   keep the existing `pub(super)` helper visibility and update trigger modules
   to import through `camera_pose`.
3. Move `fit/animate.rs`, `fit/look.rs`, `fit/request.rs`, and `fit/zoom.rs`
   into `fit/triggers/`; preserve their public and observer re-exports through
   `fit/triggers/mod.rs` and `fit/mod.rs`.
4. Move overlay geometry files and create `overlay/geometry/edge.rs` from the
   current `Edge` enum in the solver.
5. Move overlay render files under `overlay/render/`; update `overlay/mod.rs`
   to register the same systems and resources through the new modules.
6. Replace production imports from crate-root public re-exports with owner-module
   imports inside `fit/`, leaving public API tests free to import crate-root
   names when they are testing exported API.
7. Run `cargo check -p bevy_lagrange --all-targets`,
   `cargo check -p bevy_lagrange --all-targets --features fit_overlay`, and
   `cargo nextest run -p bevy_lagrange`.

The phase should land as one commit after those checks are green.

## Phase 2 - Split `fit/triggers/look.rs`

**Status:** implemented — see `docs/bevy_lagrange/as-built/free-cam.md` (fit/ restructure, "How it works").

### Target layout

```text
fit/
  triggers/
    look/
      mod.rs
      look_at.rs
      look_at_and_zoom_to_fit.rs
      plan.rs
      support.rs
```

### What goes where

- `look_at.rs`: current `fit/triggers/look.rs` lines 32-72 (`LookAt` and its
  builder methods) plus lines 220-315 (`on_orbit_cam_look_at`,
  `on_free_cam_look_at`). Move the tests that assert look-only snap and timed
  queue behavior with this file.
- `look_at_and_zoom_to_fit.rs`: current lines 75-124
  (`LookAtAndZoomToFit` and its builder methods) plus lines 319-507
  (`on_orbit_cam_look_at_and_zoom_to_fit`,
  `on_free_cam_look_at_and_zoom_to_fit`). Move the tests that assert look-plus-fit
  queueing, fit target updates, roll preservation, and absence of zoom lifecycle
  events with this file.
- `plan.rs`: current lines 126-183 (`LookAtPlan` and its conversion helpers).
  This is the shared planning type used by both look event modules.
- `support.rs`: current lines 185-216 (`trigger_timed_animation`,
  `trigger_completed_animation`). These helpers are local to look events and
  should not move into the root animation module.
- `look/mod.rs`: declare the submodules and re-export only the public events
  and observer functions that `fit/triggers/mod.rs` already exposes.

### Sequencing

1. Replace `fit/triggers/look.rs` with `fit/triggers/look/mod.rs` in the same
   edit batch that introduces the first child modules. Do not leave
   `look.rs` and `look/` side by side between checkpoints.
2. Move `LookAtPlan` and support helpers first; update the new `look/mod.rs`
   body to import those local modules.
3. Move `LookAt` and the look-only observers into `look_at.rs`; move their
   tests with them.
4. Move `LookAtAndZoomToFit` and look-plus-fit observers into
   `look_at_and_zoom_to_fit.rs`; move their tests with them.
5. Preserve the current `fit/triggers/mod.rs`, `fit/mod.rs`, and `lib.rs`
   re-export surface.
6. Run `cargo check -p bevy_lagrange --all-targets`,
   `cargo check -p bevy_lagrange --all-targets --features fit_overlay`, and
   `cargo nextest run -p bevy_lagrange`.

The phase should land as one commit after those checks are green.

## Phase 3 - Split `fit/geometry/solve.rs`

**Status:** implemented — see `docs/bevy_lagrange/as-built/free-cam.md` (fit/ restructure, "How it works").

### Target layout

```text
fit/
  geometry/
    solve/
      mod.rs
      fit_solution.rs
      focus.rs
      margins.rs
      radius_search.rs
```

`Edge` is not part of this split because Phase 1 moves it to
`fit/overlay/geometry/edge.rs`.

### What goes where

- `fit_solution.rs`: current `solve.rs` lines 61-89 (`FitSolution`,
  `FitError`, and `impl Display for FitError`).
- `margins.rs`: current line 33 (`zoom_margin_multiplier`), lines 97-133
  (`calculate_target_margins`), and lines 237-272
  (`find_constraining_margin`).
- `focus.rs`: current lines 231-233 (`viewport_can_map_pixels`) plus
  lines 432-550 (`refine_focus_centering`, `refine_focus_anchoring`,
  `bounds_anchor_point`, `viewport_anchor_point`, `normalized_pixel_offset`).
- `radius_search.rs`: current lines 54-57 (`BoundsSearch`), lines 140-147
  (`FitParameters`), lines 159-229 (`calculate_fit`, `has_pixel_offset`), and
  lines 279-423 (`binary_search_for_fit`, `build_test_projection`).
- `solve/mod.rs`: declare submodules and re-export `FitSolution` and
  `calculate_fit` for existing fit-domain callers. `FitError` stays local to
  the solver's crate-internal API, and margin helpers stay private to the
  solver.

### Sequencing

1. Confirm Phase 1 has already moved `Edge` out of the solver.
2. Replace `fit/geometry/solve.rs` with `fit/geometry/solve/mod.rs` in the
   same edit batch that introduces the first child modules. Do not leave
   `solve.rs` and `solve/` side by side between checkpoints.
3. Extract `fit_solution.rs` and `margins.rs`; update local imports without
   changing caller behavior.
4. Extract `focus.rs`; keep it private to the solve module.
5. Move the remaining search body and `FitParameters` into `radius_search.rs`.
6. Move the relevant tests with the function clusters they verify.
7. Run `cargo check -p bevy_lagrange --all-targets`,
   `cargo check -p bevy_lagrange --all-targets --features fit_overlay`, and
   `cargo nextest run -p bevy_lagrange`.

The phase should land as one commit after those checks are green.

## Phase 4 - Split `fit/overlay/render/fit_target_bounds.rs`

**Status:** implemented — see `docs/bevy_lagrange/as-built/free-cam.md` (fit/ restructure, "How it works").

### Target layout

```text
fit/
  overlay/
    geometry/
      frame.rs             # gains resolve_fit_overlay_frame
    render/
      fit_target_bounds/
        mod.rs
        config.rs
        margin_lines.rs
        target_bounds.rs
        ui_camera.rs
```

### What goes where

- `config.rs`: current `fit_target_bounds.rs` lines 49-74
  (`FitTargetOverlayConfig` and `Default`).
- `margin_lines.rs`: current lines 106-120 (`calculate_edge_color`) and
  lines 145-249 (`DrawContext`, `draw_margin_lines_and_labels`,
  `cleanup_stale_margin_labels`).
- `target_bounds.rs`: current lines 79-103 (`FitMarginPercents` and
  `From<&ScreenSpaceBounds>`), lines 123-142 (`rectangle_points`,
  `silhouette_points`), lines 252-371 (`on_remove_fit_visualization`,
  `draw_fit_target_bounds`), and lines 416-528 (`draw_bounds_for_camera`).
  `fit_target_bounds/mod.rs` should re-export `FitMarginPercents` for
  `reconciliation.rs`.
- `ui_camera.rs`: current lines 530-555 (`label_ui_camera`) plus its tests
  from lines 557-706.
- `overlay/geometry/frame.rs`: move current lines 373-413
  (`resolve_fit_overlay_frame`) into the existing frame owner, because it
  constructs `FitOverlayFrame` / `FitOverlayLayout`.
- `fit_target_bounds/mod.rs`: declare submodules and re-export
  `FitTargetOverlayConfig`, `FitMarginPercents`, `draw_fit_target_bounds`, and
  `on_remove_fit_visualization` to `overlay/render/mod.rs`.

### Boundary cleanup

Replace the `#[cfg(feature = "fit_overlay")]` `sum` and `count` fields on
`projection::PointDepths` with a neutral `average` depth metric. `fit/geometry`
can then stay feature-agnostic while `overlay/geometry/frame.rs` still gets the
average depth needed for retained overlay placement.

### Sequencing

1. Replace `fit/overlay/render/fit_target_bounds.rs` with
   `fit/overlay/render/fit_target_bounds/mod.rs` in the same edit batch that
   introduces the first child modules. Do not leave `fit_target_bounds.rs` and
   `fit_target_bounds/` side by side between checkpoints.
2. Extract `config.rs`, then move `label_ui_camera` and its tests into
   `ui_camera.rs`.
3. Move `resolve_fit_overlay_frame` into `overlay/geometry/frame.rs` and apply
   the `PointDepths::average` cleanup.
4. Extract `margin_lines.rs`.
5. Move the remaining draw-system code into `target_bounds.rs` and make
   `fit_target_bounds/mod.rs` the local re-export table.
6. Run `cargo check -p bevy_lagrange --all-targets`,
   `cargo check -p bevy_lagrange --all-targets --features fit_overlay`, and
   `cargo nextest run -p bevy_lagrange`.

The phase should land as one commit after those checks are green.
