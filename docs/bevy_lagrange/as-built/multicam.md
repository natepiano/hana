# Multicam — `viewports_windows` canonical-fairy-dust conversion plan

> **Archived 2026-06-07 — implemented.** Deleted in fdb9dc0, the commit that
> landed the singleton camera control panel and the viewports home animation.
> `viewports_windows.rs` is the converted example: per-camera home poses via
> `.with_camera_home()`, per-cursor H-key homing, and per-window guidance
> columns. The multi-window screen-panel prerequisite this plan spun off is
> archived at
> [`../../bevy_diegetic/archive/multi-window.md`](../../bevy_diegetic/archive/multi-window.md).
> The canonical guide referenced below now lives at
> `docs/fairy_dust/canonical-example.md`.

Convert `crates/bevy_lagrange/examples/viewports_windows.rs` to the canonical
fairy_dust style, with first-class support for **multiple `OrbitCam`s** —
each with its own home pose, its own column of guidance in a shared per-window
panel, and an `H` key that homes only the camera currently receiving input.

## Canonicalize matrix

Each row maps a `docs/canonical_example.md` requirement to the current state
of `viewports_windows.rs` and the work needed to close the gap. Rows are
independent — pick one, do that chunk, ship.

| # | Canonical item | Current viewports_windows state | Gap to close | Plan phase |
|---|---|---|---|---|
| 1 | `fairy_dust::sprinkle_example()` entry, `.run()` | Raw `App::new()` + four `add_plugins` calls | Switch plumbing to the `sprinkle_example()` chain | 5 |
| 2 | `.with_brp_extras()` | Manual `BrpExtrasPlugin::default()` | Drop manual add, use builder | 5 |
| 3 | `.with_save_window_position()` | Manual `WindowManagerPlugin` | Drop manual add, use builder | 5 |
| 4 | `.with_studio_lighting()` | Single `PointLight` spawned by hand | Delete manual light, use builder | 5 |
| 5 | `.with_ground_plane()` | Manual `Plane3d` (`GROUND_SIZE = 5.0`, green) | Delete manual ground, use builder (default 8×8, tan) | 5 |
| 6 | `.with_cube()` builder | Manual `Cuboid` (size 1.0, tan, at origin) | Replace with builder; no face labels needed | 5 |
| 7 | `.with_orbit_cam(...)` (singular) | Three `OrbitCam` entities spawned manually | Stay manual (multi-cam); add `Camera3d::default()` + explicit `Camera { order, clear_color }` per cam | 5 |
| 8 | `.with_camera_home(...)` (singular) | No home; raw `Transform` only | Use new per-camera `CameraHome` component | 2 (build), 5 (apply) |
| 9 | `.with_title_bar(...)` | None | Add `TitleBar::new("Viewports + windows")` with `H Home` chip | 5 |
| 10 | `.with_camera_control_panel()` (singular) | None — singular wouldn't fit three cams across two windows | Use new **grouped** per-window panel | 3 (build), 4 (chip wiring), 5 (apply) |
| 11 | `Ctrl+Shift+R` hot-restart | Not wired (raw `App::new()` skips fairy_dust) | Comes free with `sprinkle_example()` switch | 5 |
| 12 | Manual-spawn justified only when fairy_dust can't express it | Cube/ground/light: unjustified. Cameras: justified | Delete cube/ground/light; keep camera spawns | 5 |

### Out-of-canonical concerns specific to this example

| # | Concern | Current state | Action | Plan phase |
|---|---|---|---|---|
| A | Multiple `OrbitCam`s sharing input routing | `ResolvedOrbitCamInputRoute` is `pub(crate)` | Promote to `pub`, expose `routed_camera()` | 1 |
| B | Per-window screen-space panel | `DiegeticPanel::screen()` — previous attempt found this was the breaking point | Verify or extend; spawn one panel per render-target window | 3 (open question 1) |
| C | `AnimationBegin`/`End` filterable by camera entity (for chip highlight) | Verify field present | Add `camera: Entity` field if missing | 4 |
| D | Camera `order` / `clear_color` on viewport overlays | Set on minimap, missing on second-window cam | Add explicit `Camera { ... }` to all three cameras | 5 |

## High-level walk: filling the matrix

Reading the matrix as a checklist yields this order. Each step compiles and
leaves `zoom_to_fit` / `world_text` running unchanged.

1. **Promote one type to public (row A).** `ResolvedOrbitCamInputRoute` →
   `pub`. No behavior change; everything below builds on it.
2. **Add per-camera home capability (row 8).** New `fairy_dust::CameraHome`
   component + observer. Single-camera `with_camera_home(...)` stays
   untouched.
3. **Add grouped guidance panel capability (rows 10, B).** New
   `GroupedCameraGuidance` component + per-window panel. **Validate the
   bevy_diegetic per-window screen-space path before committing** — this is
   where the prior attempt died.
4. **Add per-camera `H Home` chip wiring (rows 9, C).** Depends on (2) and
   (3); may require a small `bevy_lagrange` field add.
5. **Convert the example (rows 1–7, 11, 12, D).** Flip to the canonical
   chain, delete manual ground/cube/light spawns, keep manual cameras,
   add explicit `Camera3d` + `Camera { order, clear_color }` on each.
6. **Update `docs/canonical_example.md`.** Add a "Multiple cameras /
   windows" section cross-referencing the new capabilities.

The risky step is **3**. If `bevy_diegetic` doesn't support a render-target
override on screen-space panels, fixing that is a sub-task that lands before
step 3 itself. Step 5 is reversible up to the moment of merge — the previous
attempt's lesson is that "compiles cleanly" doesn't mean "renders correctly,"
so step 5 needs a launched-window check, not just `cargo build`.

## Why this is risky and how this plan stays safe

The previous attempt at this work left the example rendering completely black.
This plan is structured so that **every phase compiles and the affected
examples still launch and render**. Risky surgery (changing the per-OrbitCam
auto-guidance observer, changing camera-home from singleton to per-camera) is
gated behind feature-additive new types so the existing single-camera path
keeps working byte-for-byte.

### Failure modes the previous attempt likely fell into

These are concrete things to watch for, ordered roughly by likelihood:

1. **Camera order collision.** Spawning multiple `Camera` components with the
   default `order: 0` in the same window. Bevy renders them in arbitrary
   order; with `ClearColorConfig::Default` on more than one, each camera
   clears the previous camera's output, ending in the last-drawn camera's
   contents (which can be black for the minimap if its frustum sees nothing).
   **Mitigation:** the main full-window camera in each window stays
   `order: 0` with default clear; every additional viewport camera explicitly
   gets a higher `order` *and* `ClearColorConfig::None`.
2. **No camera in the primary window.** If `sprinkle_example()` auto-spawns
   an `OrbitCam` (it does not currently — `with_orbit_cam_*` is opt-in — but
   `with_camera_control_panel`'s observer treats every `OrbitCam` as a
   candidate), and the example deletes that path but never inserts a
   `Camera3d`-equivalent, the window has nothing to render. **Mitigation:**
   the example explicitly spawns three OrbitCams in `Startup`; the canonical
   chain does **not** call `with_orbit_cam_*` for this example.
3. **`OrbitCam` spawned without a `Camera` component.** `OrbitCam` is a
   controller, not a renderer. Spawning `(OrbitCam, Transform)` alone gives
   you nothing to render. The current viewports_windows source proves this —
   it spawns `(Transform, OrbitCam)` for the primary camera, which works
   only because some other code path adds the `Camera3d` bundle for it.
   We need to verify and, where missing, add `Camera3d::default()`
   explicitly to every OrbitCam we spawn.
4. **Studio lighting + ground plane spawned in the wrong window.**
   `with_studio_lighting()` and `with_ground_plane()` add entities to the
   world, not to a specific window. Both windows see them, which is what we
   want here.
5. **Stable-transparency hooks tied to the single `FairyDustOrbitCam`
   entity.** This example doesn't use stable transparency, but the lookup
   patterns in `crate::camera_home::trigger_initial_animate` use
   `cameras.single()` — that is, "fail if there is more than one." Any
   multi-camera capability that reuses these single-entity queries panics or
   silently no-ops. **Mitigation:** the new per-camera home path uses a
   relationship/marker scheme, never `single()`.
6. **`set_camera_viewports` filtering on the wrong window.** The current
   system reacts to any `WindowResized` event and writes the minimap
   viewport onto whichever camera has `MinimapCamera`. If a second window's
   resize event arrives first, we'd compute the minimap rect against the
   wrong window's resolution. **Mitigation:** filter on
   `resize_event.window == primary_window_entity`.

## Goals

- Convert `viewports_windows.rs` to the canonical fairy_dust chain from
  `docs/canonical_example.md`.
- Add three new fairy_dust capabilities (additive only — existing examples
  keep working):
  1. **Multi-camera guidance panel** — one panel per render-target window,
     N columns inside, active column highlighted by routing.
  2. **Per-camera home pose** — each OrbitCam carries its own framed region;
     `H` homes the camera currently receiving input.
  3. **`H Home` chip per panel** — auto-prepended when a per-camera home
     is registered.
- Expose `bevy_lagrange::ResolvedOrbitCamInputRoute` publicly so fairy_dust
  can read the currently-routed camera.

### Non-goals (defer)

- Naming/role labels for cameras beyond what `Name` provides.
- Click-on-column-to-activate.
- Sharing a single home pose across cameras.
- Migrating any other example.

## Phases — each phase ships green

### Phase 0 — pre-flight (no code changes)

- `cargo run --example viewports_windows -p bevy_lagrange` against today's
  `main`. Confirm baseline visual: ground + cube in primary window, minimap
  in top-right, separate window with a third view, all three navigable with
  the mouse over each respective viewport.
- Take a screenshot of the working baseline and save next to this plan.
  This is what every phase has to keep matching (modulo additive UI).
- Confirm the second window's camera **does** spawn with a `Camera`
  component already (line 116 of the current source) — so the existing
  example only relies on `(Transform, OrbitCam)` being implicitly enough for
  the primary camera. If that's actually broken on a clean rebuild, that's
  pre-existing and out of scope; if it works, the new spawn path must keep
  doing whatever's working today.

**Exit criteria:** baseline screenshot captured; we know what "good" looks
like.

### Phase 1 — expose `ResolvedOrbitCamInputRoute`

File: `crates/bevy_lagrange/src/input/routing.rs:203`
File: `crates/bevy_lagrange/src/input/mod.rs`
File: `crates/bevy_lagrange/src/lib.rs`

Promote `ResolvedOrbitCamInputRoute` and its `routed_camera() -> Option<Entity>`
method from `pub(crate)` to `pub`. Re-export from `input::mod.rs` and from
`lib.rs`'s prelude.

Optional sugar: add a `SystemParam` (`pub struct ActiveOrbitCamera<'w>(...)`)
that wraps `Res<ResolvedOrbitCamInputRoute>` with a `.entity() -> Option<Entity>`
helper, so fairy_dust callers don't pull the resource type into their imports.
Decide during implementation — nothing else in this plan depends on it.

**Exit criteria:** `cargo build -p bevy_lagrange` clean; `zoom_to_fit` example
still runs unchanged.

### Phase 2 — per-camera home pose (additive type, old path untouched)

Add a new fairy_dust capability that lives **alongside** the existing
`camera_home` module, not as a rewrite of it. The existing
`with_camera_home(...)` keeps working for single-camera examples.

File: `crates/fairy_dust/src/per_camera_home.rs` (new)

New types and observers:

```rust
/// Component on an OrbitCam entity: defines the world-space region the
/// camera homes to when `H` is pressed while this camera owns input.
#[derive(Component, Clone, Copy)]
pub struct CameraHome {
    pub transform: Transform, // translation = focus, scale = framed volume
    pub yaw:       f32,
    pub pitch:     f32,
    pub duration:  Duration,
    pub margin:    f32,
}

/// Sidecar component placed on the OrbitCam entity, pointing at the
/// invisible mesh entity spawned for `AnimateToFit::target`.
#[derive(Component)]
pub(crate) struct CameraHomeFrame(pub Entity);
```

Observer (`On<Add, CameraHome>`):
- Spawn an invisible `Mesh3d(Cuboid::from_size(Vec3::ONE))` entity at
  `home.transform` (its scale defines the framed volume).
- Hide via `Visibility::Hidden`.
- Insert `CameraHomeFrame(invisible_entity)` onto the camera.
- Trigger one `AnimateToFit::duration(Duration::ZERO)` for the initial
  framing — gated by a `Local<bool>` on the observer system or by
  inserting a `HomeNeedsInitial` marker the observer consumes next frame.

System (Update):
- Read `keys: Res<ButtonInput<KeyCode>>` for `KeyCode::KeyH`.
- Read `Res<ResolvedOrbitCamInputRoute>` for the active camera entity.
- Look up `(CameraHome, CameraHomeFrame)` on that camera; if both present,
  trigger `AnimateToFit::new(active_camera, frame.0).yaw(...).pitch(...)`.
- No fallback if no active camera — the H key does nothing while the cursor
  isn't over any orbit camera's viewport. That's the right behavior; the
  user understands why immediately.

**Important:** the existing `camera_home` module continues to use
`FairyDustOrbitCam` and `cameras.single()`. Do not change it in this phase.
The new path coexists. (We could merge later, but every merge attempt is
another chance to break single-camera examples — keep them isolated.)

**Public API:**
- `pub use per_camera_home::CameraHome;` in `lib.rs`.
- Examples wishing to use it just insert `CameraHome` directly on their
  OrbitCam entity.
- A new builder method `with_per_camera_home()` on `SprinkleBuilder<NoOrbitCam>`
  installs the plugin (observers + system). Existing `with_camera_home(...)`
  installs its own plugin and is unaffected.

**Exit criteria:** new module compiles; a test example (we can throw together
a `two_cubes_two_homes.rs` scratch or just unit-test the observer) shows two
OrbitCams each homing to a different region.

### Phase 3 — multi-camera guidance panel

Add a sibling type to the existing single-camera control panel — again,
additive only. The current `attach_default_guidance_on_orbit_cam_add`
observer keeps inserting `CameraGuidance::auto()` on every OrbitCam, which
spawns a per-camera panel. We need to **suppress** that auto-spawn for
cameras participating in a group, without changing single-camera behavior.

File: `crates/fairy_dust/src/ui/camera_control_panel/grouped.rs` (new)

New types:

```rust
/// Marker on an OrbitCam entity opting it into a grouped, per-window
/// guidance panel instead of getting its own standalone panel.
#[derive(Component, Clone)]
pub struct GroupedCameraGuidance {
    /// Display label rendered as the column header. Falls back to the
    /// camera entity's `Name` if `None`, then to "Camera N".
    pub label: Option<String>,
}

/// Internal: one panel per render-target window. Spawned lazily by the
/// observer below when at least one grouped camera in that window exists.
#[derive(Component)]
pub(crate) struct GroupedGuidancePanel {
    pub window: Entity,
}
```

Auto-suppress: in `camera_control_panel/mod.rs:58`, the
`attach_default_guidance_on_orbit_cam_add` observer needs an extra negative
filter:

```rust
cameras: Query<
    (),
    (With<OrbitCam>, Without<CameraGuidance>, Without<GroupedCameraGuidance>),
>,
```

That is the **only** change to existing single-camera code. Two crucial
properties: cameras without `GroupedCameraGuidance` keep getting the
standalone panel exactly as today; cameras with it get nothing from the
existing path and the new path takes over.

New observer (`On<Add, GroupedCameraGuidance>`):
- Resolve which window this camera renders to:
  - `RenderTarget::Window(WindowRef::Entity(e))` → `e`
  - `RenderTarget::Window(WindowRef::Primary)` → the primary window entity
    (look up via `Query<Entity, With<PrimaryWindow>>`)
  - Anything else (image, etc.) — skip with a warn-once log; no panel.
- Check whether a `GroupedGuidancePanel { window }` already exists.
- If yes, mark it for re-render.
- If no, spawn one. Anchor `Anchor::BottomRight`; render target = same window
  as the camera. The panel mesh itself is a screen-space `DiegeticPanel`; we
  need to make sure `DiegeticPanel::screen()` honors a render-target
  override, or we spawn a screen-space UI camera per window. Look this up
  in `bevy_diegetic` — this is a known integration point and likely the
  thing that broke the previous attempt.

Layout: extend (or sibling to) `build_guidance_tree` to take a slice of
`(ColumnHeader, CameraGuidanceSnapshot, CameraGuidanceDisplay, IsActive)`
and emit a row layout where each column is the existing single-camera
layout, separated by a vertical rule. Column widths fixed (so they don't
reflow as active changes); active column gets accent border, inactive
columns get dimmed text.

Render system (PostUpdate, after the resolver runs):
- Read `Res<ResolvedOrbitCamInputRoute>` once.
- For each `GroupedGuidancePanel`, query all grouped cameras whose window
  matches, sort by `Camera::order` ascending (so background-first cameras
  are leftmost), resolve each snapshot, mark the one whose entity matches
  `route.routed_camera()` as active.
- If the column set changed (camera added/removed) or any snapshot changed
  or the active column flipped, `commands.set_tree(panel, ...)` with the
  new tree.

**Exit criteria:** a new minimal test example with two OrbitCams (no
windows, just two viewports) shows a single panel with two columns and the
column accent follows the cursor.

### Phase 4 — `H Home` chip in grouped panel

When a grouped panel's columns include cameras that have `CameraHome`,
prepend an `H Home` chip row to that column (matching the existing
single-camera convention). Wire the chip highlight via the same
`AnimationBegin`/`AnimationEnd` observers used by the existing
`camera_home.rs`, but **filtered by camera entity** — the
`AnimationSource::AnimateToFit` triggers carry the camera entity (verify in
`bevy_lagrange::AnimationBegin`); the observer maps that entity to the
column it lives in and highlights only that column's `H Home`.

If the `AnimationBegin` event doesn't carry the camera entity today, that's
a small `bevy_lagrange` change — add a `camera: Entity` field. (Cross-ref
phase 1; this can land in the same change.)

**Exit criteria:** pressing H with the cursor over a viewport highlights
only that column's `H Home` for the duration of its animation.

### Phase 5 — convert `viewports_windows.rs`

Now the example becomes the consumer of the three new capabilities. Target
source file, end-to-end:

```rust
//! Demonstrates multiple viewports in one window and multiple OS windows,
//! each with an independent `OrbitCam`. Cursor over a viewport routes input
//! to that camera; `H` homes the routed camera; the bottom-right panel
//! shows one column per camera in this window, with the routed column
//! highlighted.

use std::time::Duration;

use bevy::camera::RenderTarget;
use bevy::camera::Viewport;
use bevy::prelude::*;
use bevy::window::ClosingWindow;
use bevy::window::PrimaryWindow;
use bevy::window::WindowRef;
use bevy::window::WindowResized;
use bevy_lagrange::OrbitCam;
use bevy_lagrange::OrbitCamPreset;
use bevy_window_manager::ManagedWindow;
use fairy_dust::Anchor;
use fairy_dust::CameraHome;
use fairy_dust::GroupedCameraGuidance;
use fairy_dust::TitleBar;

const CUBE_SIZE: f32 = 1.0;
const CUBE_COLOR: Color = Color::srgb(0.8, 0.7, 0.6);
const CUBE_TRANSLATION: Vec3 = Vec3::new(0.0, 0.5, 0.0);

const HOME_FRAME_SIZE: f32 = CUBE_SIZE * 3.5;
const HOME_FRAME_TRANSFORM: Transform =
    Transform::from_translation(CUBE_TRANSLATION).with_scale(Vec3::splat(HOME_FRAME_SIZE));
const HOME_DURATION: Duration = Duration::from_millis(800);
const HOME_MARGIN: f32 = 0.15;

const MAIN_PITCH: f32 = 0.46;
const MAIN_YAW: f32 = 0.0;
const MINIMAP_PITCH: f32 = 0.9;     // top-down-ish
const MINIMAP_YAW: f32 = 0.0;
const SECOND_PITCH: f32 = 0.30;
const SECOND_YAW: f32 = 0.6;

const MINIMAP_CAMERA_ORDER: isize = 1;
const MINIMAP_VIEWPORT_DIVISOR: u32 = 5;

const SECOND_WINDOW_NAME: &str = "second_window";
const SECOND_WINDOW_TITLE: &str = "Second window";

#[derive(Component)]
struct MinimapCamera;

fn main() {
    fairy_dust::sprinkle_example()
        .with_brp_extras()
        .with_save_window_position()
        .with_studio_lighting()
        .with_ground_plane()
        .with_cube()
            .size(CUBE_SIZE)
            .color(CUBE_COLOR)
            .transform(Transform::from_translation(CUBE_TRANSLATION))
        // Multi-camera example: do NOT call .with_orbit_cam_*. Cameras are
        // spawned manually below.
        .with_per_camera_home()
        .with_multi_camera_control_panel()
        .with_title_bar(
            TitleBar::new("Viewports + windows")
                .with_anchor(Anchor::TopLeft)
                .control("H Home (active camera)"),
        )
        .add_systems(Startup, spawn_cameras_and_second_window)
        .add_systems(
            Update,
            (cleanup_cameras_on_window_close, set_minimap_viewport),
        )
        .run();
}

fn home(yaw: f32, pitch: f32) -> CameraHome {
    CameraHome {
        transform: HOME_FRAME_TRANSFORM,
        yaw,
        pitch,
        duration: HOME_DURATION,
        margin: HOME_MARGIN,
    }
}

fn spawn_cameras_and_second_window(mut commands: Commands) {
    // Primary window — main full-viewport camera.
    commands.spawn((
        Name::new("Main"),
        Camera3d::default(),
        Camera::default(), // order 0, default clear; primary window
        OrbitCam::default(),
        OrbitCamPreset::BlenderLike,
        GroupedCameraGuidance { label: Some("Main".into()) },
        home(MAIN_YAW, MAIN_PITCH),
    ));

    // Primary window — minimap viewport overlay (top-right corner). Must be
    // a higher order than 0 and clear None so it composes over the main view.
    commands.spawn((
        Name::new("Minimap"),
        Camera3d::default(),
        Camera {
            order:       MINIMAP_CAMERA_ORDER,
            clear_color: ClearColorConfig::None,
            ..default()
        },
        OrbitCam::default(),
        OrbitCamPreset::BlenderLike,
        MinimapCamera,
        GroupedCameraGuidance { label: Some("Minimap".into()) },
        home(MINIMAP_YAW, MINIMAP_PITCH),
    ));

    // Second OS window + its camera.
    let second_window = commands
        .spawn((
            Window {
                title: SECOND_WINDOW_TITLE.into(),
                ..default()
            },
            ManagedWindow {
                name: SECOND_WINDOW_NAME.into(),
            },
        ))
        .id();

    commands.spawn((
        Name::new("Second window"),
        Camera3d::default(),
        Camera {
            target: RenderTarget::Window(WindowRef::Entity(second_window)),
            ..default()
        },
        OrbitCam::default(),
        OrbitCamPreset::BlenderLike,
        GroupedCameraGuidance { label: Some("Second window".into()) },
        home(SECOND_YAW, SECOND_PITCH),
    ));
}

/// Despawns cameras whose render-target window is marked `ClosingWindow`.
fn cleanup_cameras_on_window_close(
    mut commands: Commands,
    closing: Query<Entity, With<ClosingWindow>>,
    cameras: Query<(Entity, &Camera)>,
) {
    for (camera_entity, cam) in &cameras {
        if let RenderTarget::Window(WindowRef::Entity(window)) = cam.target
            && closing.get(window).is_ok()
        {
            commands.entity(camera_entity).despawn();
        }
    }
}

/// Resize the minimap viewport on the primary window only.
fn set_minimap_viewport(
    windows: Query<&Window>,
    primary: Query<Entity, With<PrimaryWindow>>,
    mut resize_events: MessageReader<WindowResized>,
    mut minimap: Single<&mut Camera, With<MinimapCamera>>,
) {
    let Ok(primary_entity) = primary.single() else {
        return;
    };
    for resize_event in resize_events.read() {
        if resize_event.window != primary_entity {
            continue;
        }
        let Ok(window) = windows.get(resize_event.window) else {
            continue;
        };
        let size = window.resolution.physical_width() / MINIMAP_VIEWPORT_DIVISOR;
        minimap.viewport = Some(Viewport {
            physical_position: UVec2::new(window.resolution.physical_width() - size, 0),
            physical_size:     UVec2::new(size, size),
            ..default()
        });
    }
}
```

**Exit criteria:**
- `cargo run --example viewports_windows -p bevy_lagrange` matches the
  Phase-0 baseline screenshot (ground/cube visible in both windows; minimap
  visible in primary top-right).
- Two panels appear, one per window. Primary window's panel has two columns
  (Main, Minimap); second window's panel has one column (Second window).
- Cursor over each viewport changes the highlighted column.
- `H` with cursor over Main homes only the main camera; same for minimap
  and second window. The corresponding column's `H Home` chip highlights
  during the animation.

### Phase 6 — canonical_example.md update

Add a "Multiple cameras / windows" section under "Capability rules", and add
`viewports_windows.rs` to the example registry / remove it from the
"future work" list if present.

The new section text (verbatim draft):

> ### Multiple cameras / windows
>
> When an example spawns more than one `OrbitCam`, replace
> `.with_camera_control_panel()` with `.with_multi_camera_control_panel()`,
> and use `.with_per_camera_home()` instead of `.with_camera_home(...)`.
>
> On each camera entity, insert:
> - `GroupedCameraGuidance { label: Some("...".into()) }` — opts the camera
>   out of the standalone panel and into a per-window grouped panel.
> - `CameraHome { transform, yaw, pitch, duration, margin }` — declares
>   that camera's home pose. The `H` key fires `AnimateToFit` on whichever
>   camera currently owns input (per `ResolvedOrbitCamInputRoute`).
>
> Cameras must spawn with `Camera3d::default()` and a `Camera { ... }`. Set
> `Camera.order` and `Camera.clear_color` explicitly for non-primary
> viewports so they compose correctly over the main view.

## Test plan summary

1. After Phase 1: `cargo build -p bevy_lagrange`; existing
   `zoom_to_fit.rs` and `world_text.rs` still run identically.
2. After Phase 2: throwaway two-camera smoke test (in-tree or scratch
   `cargo run --example two_cubes_two_homes` if cheap; otherwise unit
   tests on the observer logic).
3. After Phase 3: same smoke test now shows a two-column panel; cursor over
   each viewport switches the highlighted column.
4. After Phase 4: H-key highlight follows the active column.
5. After Phase 5: `viewports_windows.rs` matches the Phase-0 baseline plus
   per-window panel and per-camera home. **Validate visually with the
   running app** — type checking does not catch black screens.
6. After Phase 6: `cargo run --example zoom_to_fit -p bevy_lagrange` and
   `cargo run --example world_text -p bevy_diegetic` are unchanged.

## Files touched

- **New:**
  - `crates/fairy_dust/src/per_camera_home.rs`
  - `crates/fairy_dust/src/ui/camera_control_panel/grouped.rs`
- **Modified (additive only):**
  - `crates/bevy_lagrange/src/input/routing.rs` — promote
    `ResolvedOrbitCamInputRoute` to `pub`, expose `routed_camera()`.
  - `crates/bevy_lagrange/src/input/mod.rs` — re-export.
  - `crates/bevy_lagrange/src/lib.rs` — re-export from prelude.
  - `crates/bevy_lagrange/src/events/animation.rs` — add `camera: Entity`
    to `AnimationBegin`/`AnimationEnd` if not already present (verify in
    Phase 1 — if absent we update; if present, cite the field).
  - `crates/fairy_dust/src/ui/camera_control_panel/mod.rs` — add
    `Without<GroupedCameraGuidance>` to one query (line 58–67 area).
  - `crates/fairy_dust/src/ui/camera_control_panel/layout.rs` — add a
    sibling `build_grouped_guidance_tree(&[ColumnSpec])`.
  - `crates/fairy_dust/src/builder/sprinkle.rs` — add
    `with_per_camera_home()` and `with_multi_camera_control_panel()`
    builder methods on `SprinkleBuilder<S>`.
  - `crates/fairy_dust/src/lib.rs` — re-export `CameraHome`,
    `GroupedCameraGuidance`.
  - `crates/bevy_lagrange/examples/viewports_windows.rs` — converted.
  - `docs/canonical_example.md` — multi-camera section added.

- **Not modified (deliberately):**
  - `crates/fairy_dust/src/camera_home.rs` — left untouched. Single-camera
    examples keep working unchanged.
  - `crates/fairy_dust/src/orbit_cam.rs` — `FairyDustOrbitCam` and
    `install_with_bundle` unchanged. Multi-camera examples just don't use
    them.

## Open questions to answer during implementation

1. **Does `DiegeticPanel::screen()` honor a render-target / window override?**
   If not, we either need to teach it that, or spawn one screen-space UI
   camera per window and parent each grouped panel to its window's UI
   camera. Worth a 10-minute read of `bevy_diegetic` before Phase 3 starts.
2. **Does `AnimationBegin`/`AnimationEnd` carry a camera entity today?**
   If yes, phase 4 wires the chip highlight per camera trivially. If no,
   add the field — small change, verify nothing else listens.
3. **How does `ResolvedOrbitCamInputRoute` resolve cursor-over-no-viewport?**
   Returns `None`. The grouped panel falls back to "no column active";
   `H` is a no-op. That's the design we want.
4. **What's the right initial column to highlight at startup?** No active
   camera until the user moves their cursor — show all columns dimmed
   equally. Don't pretend one is active.

## Review checklist for the subagent

Please evaluate:

- [ ] Does this plan, as written, guarantee that `zoom_to_fit.rs` and
      `world_text.rs` still run unchanged after Phase 6?
- [ ] Are there hidden coupling points in `bevy_diegetic` /
      `bevy_lagrange` that would force a larger surgery than this plan
      admits? Specifically: cross-window screen-space panels, and the
      public API of the cursor-routing resource (fields, methods,
      change-detection guarantees).
- [ ] Is the `CameraGuidance` auto-attach suppression watertight, or could
      a race spawn the standalone panel before `GroupedCameraGuidance`
      observer fires?
- [ ] Could the per-camera home animation fight with the existing
      single-camera `camera_home` module if both are accidentally
      installed in the same app? (Answer should be "no" because
      `with_camera_home(...)` only fires on the single `FairyDustOrbitCam`,
      which is only spawned by `with_orbit_cam_*`, which the multi-camera
      example never calls. Verify.)
- [ ] Is there a simpler way to surface "this camera owns input" than
      exporting `ResolvedOrbitCamInputRoute`? E.g. an `ActiveOrbitCam`
      marker component that the routing system maintains on the resolved
      camera entity each frame. That's an arguably cleaner API — the
      panel observer just queries `Query<Entity, With<ActiveOrbitCam>>`
      instead of pulling a resource. Worth flagging as an alternative.
- [ ] Have I missed any black-screen failure modes from the list above?
