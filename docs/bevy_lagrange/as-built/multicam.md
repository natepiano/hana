# Multicam — `viewports_windows` example

The `viewports_windows` example composes multiple independent `OrbitCam`s in
one app: multiple viewports in a single window plus a second OS window, each
driven by its own camera, with cursor-routed input and per-camera home poses.

Source: `crates/bevy_lagrange/examples/viewports_windows.rs`

## What it demonstrates

- `Camera::order` layers a minimap overlay on top of the main view.
- `Camera::viewport` clips the minimap overlay to a square in the top-right
  corner of the primary window.
- `RenderTarget::Window(WindowRef::Entity(..))` aims a third camera at a
  second OS window spawned via `bevy_window_manager::ManagedWindow`.
- `ResolvedOrbitCamInputRoute::routed_camera()` resolves which camera the
  cursor is over, so input — and the `H` home key — applies to that camera.

The primary window shows a full-size view plus a minimap overlay in the
top-right corner. A second OS window shows a separate camera angle. `H` homes
whichever camera the cursor is currently over.

## Camera spawning model

Each camera is spawned as a plain entity carrying `OrbitCam` and an
`OrbitCamInputMode`. `OrbitCam` requires `OrbitCamInputMode`; the example sets
it explicitly to `OrbitCamInputMode::Preset(OrbitCamPreset::BlenderLike)` on
all three cameras:

```rust
commands.spawn((
    Name::new(PRIMARY_CAMERA_NAME),
    Transform::from_translation(PRIMARY_CAMERA_TRANSLATION),
    OrbitCam::default(),
    OrbitCamInputMode::Preset(OrbitCamPreset::BlenderLike),
    MainCamera,
));
```

`OrbitCamInputMode` is a runtime three-variant enum
(`crates/bevy_lagrange/src/input/modes.rs`):

- `Preset(OrbitCamPreset)` — a built-in keymap.
- `Bindings(OrbitCamBindings)` — app-owned validated bindings.
- `Manual` — app code writes camera intent.

It defaults to `Preset(OrbitCamPreset::SimpleMouse)`. The
`OrbitCam::blender_like()` / `OrbitCam::simple_mouse()` / `OrbitCam::manual()`
helpers (`crates/bevy_lagrange/src/orbit_cam/preset_helpers.rs`) return an
`impl Bundle` pairing `OrbitCam::default()` with the matching
`OrbitCamInputMode`; the example writes the pair out by hand to keep each
camera's other components inline.

The minimap camera adds a `Camera { order: 1, clear_color:
ClearColorConfig::None, .. }` so it composites on top of the main view; its
`viewport` is left `None` at spawn and filled in by `set_camera_viewports`.
The second-window camera adds
`RenderTarget::Window(WindowRef::Entity(second_window))`.

## Input routing

`ResolvedOrbitCamInputRoute` is a public resource
(`crates/bevy_lagrange/src/input/routing/mod.rs`, re-exported from
`input::mod` and `lib.rs`). It holds the camera currently receiving input plus
per-camera cursor-surface metrics and blocker reasons. The single public
accessor is:

```rust
pub const fn routed_camera(&self) -> Option<Entity>
```

It returns `None` when the cursor is over no orbit-camera viewport. The
example reads this in `home_on_keypress` to pick which camera `H` homes.

## Per-camera home poses

The home pose is built in the example, not by a dedicated multi-camera API.
The fairy_dust home capability is a singleton: the builder's
`.with_camera_home().yaw(..).pitch(..).duration(..).margin(..)` frames the
union of every `CameraHomeTarget` entity, and `CameraHomeEntity(Entity)` is
the resource holding the proxy entity to fit against
(`crates/fairy_dust/src/camera_home.rs`). The example marks the cube with
`CameraHomeTarget` and reuses that single home target across all three
cameras.

Per-camera angle differences live in the example's own `CameraHomes` resource
(a `HashMap<Entity, HomePose>` of `yaw`/`pitch`):

- `home_main_camera_on_startup` waits until the home proxy settles at the cube
  translation, then fires `AnimateToFit::new(camera, home.0)` with
  `Duration::ZERO` so the scene opens already framed.
- `home_on_keypress` reads `ResolvedOrbitCamInputRoute::routed_camera()`,
  looks up that camera's `HomePose`, and fires `AnimateToFit` with that pose's
  `yaw`/`pitch` and the shared `HOME_DURATION`/`HOME_MARGIN`.

`AnimateToFit` (`crates/bevy_lagrange/src/events/fit.rs`) is triggered as an
event; the builder methods `.yaw()`, `.pitch()`, `.duration()`, `.margin()`
configure the pose. The `H Home` control is surfaced through the title bar
(`TitleBar::new().with_title(..).with_anchor(Anchor::TopLeft).control("H
Home")`) and the camera control panel (`.with_camera_control_panel()`).

## Viewport and window lifecycle

- `set_camera_viewports` (Update) listens for `WindowResized` and recomputes
  the minimap camera's `Camera::viewport` as a square in physical pixels in
  the top-right corner. Viewports are physical-pixel rects, so they must be
  recomputed on every resize.
- `cleanup_cameras_on_window_close` (Update) despawns any camera whose
  `RenderTarget::Window` references a `ClosingWindow`, so Bevy's camera system
  does not panic on a stale render target.

## fairy_dust chain

The example uses the canonical `fairy_dust::sprinkle_example()` chain:
`.with_brp_extras()`, `.with_save_window_position()`,
`.with_studio_lighting()`, `.with_ground_plane()`, `.with_cube()` (with
`.face_text(Face::.., ..)` labels on each face), `.with_camera_home()`,
`.with_title_bar(..)`, and `.with_camera_control_panel()`. Cameras are spawned
manually in `setup` (Startup) because the chain's singular `.with_orbit_cam_*`
helpers cannot express three independent cameras across two windows.
