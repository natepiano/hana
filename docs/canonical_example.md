# Canonical Example Structure

The reference layout every `bevy_hana` example should follow. Use this as the
checklist when adding a new example or converting an existing one.

The current best example of this structure is
`crates/bevy_lagrange/examples/zoom_to_fit.rs`.

## Goals

- Every example launches via `fairy_dust::sprinkle_example()` and ends with `.run()`.
- The scene, lighting, ground plane, camera home pose, and HUD all come from
  fairy_dust capabilities by default.
- Examples only spawn entities manually when fairy_dust cannot express the
  intent (e.g. an entity whose `Entity` ID must be captured for later events).
- The HUD (top-left title bar) uses fairy_dust `TitleBar` and reflects
  example-specific controls. Its title is the example's display name.
- Custom screen-space panels use fairy_dust's shared screen-panel frame and
  material helpers so examples do not copy panel styling.
- `Ctrl+Shift+R` hot-restart works in every example automatically.

## Canonical builder chain

```rust
fairy_dust::sprinkle_example()
    .with_brp_extras()
    .with_save_window_position()
    .with_studio_lighting()
    .with_ground_plane()
    .with_cube()
        .size(CUBE_SIZE)
        .color(CUBE_COLOR)
        .transform(Transform::from_translation(CUBE_TRANSLATION))
        .face_text(Face::Front, "Label", LABEL_SIZE, LABEL_COLOR)
        .insert(CameraHomeTarget)
    .with_orbit_cam(|cam| { /* per-example camera tweaks */ }, OrbitCamPreset::BlenderLike)
    .with_camera_home()
        .pitch(HOME_PITCH)
        .yaw(HOME_YAW)
    .with_title_bar(
        TitleBar::new()
            .with_title("Zoom to Fit")
            .with_anchor(Anchor::TopLeft)
            .control("Z ZoomToFit")
            .control("L LookAt"),
    )
    .with_camera_control_panel()
    .add_systems(Startup, spawn_example_specific_entities)
    .add_systems(Update, keyboard_input)
    .add_observer(on_zoom_begin)
    .add_observer(on_zoom_end)
    .run();
```

Order isn't strictly required, but this top-to-bottom reading roughly matches
the lifecycle: process plumbing → scene primitives → camera → HUD → systems.

## Capability rules

### Always include

- `.with_brp_extras()` — BRP remote control + port in window title.
- `.with_save_window_position()` — window position persists across runs.
- `.with_studio_lighting()` — key/fill/rim lights + clear color. Replaces
  manual `DirectionalLight`/`PointLight`/`GlobalAmbientLight` spawns.
- `.with_ground_plane()` — default 8×8 translucent ground. Override `.size()`
  or `.color()` per example, but do not hand-roll a plane. Do **not** wire a
  click-on-ground observer to re-home the camera — `H Home` (auto-added by
  `.with_camera_home()`) is the standard homing affordance. Click-to-home
  on the ground tends to fire on stray clicks and interferes with picking
  the actual demo entities.
- `.with_camera_control_panel()` — bottom-right camera controls HUD.
- `.with_title_bar(TitleBar::new()...)` — top-left chip bar listing the
  example's keyboard shortcuts. Always set `.with_title(...)` to the example's
  display name, e.g. `"Zoom to Fit"` or `"Render to Texture"`. Title and chip
  strings render literally (no auto-uppercasing) — pass the case you want
  displayed. `H Home` is auto-prepended when `.with_camera_home()` is used.
- `Ctrl+Shift+R` hot-restart — wired up unconditionally inside
  `sprinkle_example()`; no builder call needed.

### Cubes — use the builder

Use `.with_cube().size().color().transform().face_text(...)` for every
demo cube whose `Entity` ID is **not** needed elsewhere. The fairy_dust cube
defaults match the canonical look (tan PBR material, single-mesh primitive).

Use `fairy_dust::cube_face_text(face, text, cube_size, text_size, color)` —
returned as a child bundle on a `commands.spawn` — only when the cube must be
spawned manually because its `Entity` is referenced later (e.g. as the target
of `ZoomToFit::new(camera, target)`).

### Camera

Use `.with_orbit_cam(configure, OrbitCamPreset::BlenderLike)`. Default to
`BlenderLike`; `SimpleMouse` only for examples specifically demonstrating
that preset. The `configure` closure usually does nothing (`|_| {}`) — the
home pose drives the starting view.

### Camera home — define the framed region

Use `.with_camera_home()` plus marker components instead of a hand-rolled
`KeyCode::KeyH` listener whenever the example has a normal fairy_dust-managed
home camera. The home pose is defined by:

- `CameraHomeTarget` — mark every entity whose AABB should contribute to the
  framed home region. Multiple marked entities are unioned.
- `.yaw(...)` / `.pitch(...)` — orbit orientation the camera animates to.
- `.duration(...)` / `.margin(...)` — overrides for the H-key animation.

The startup framing is always instant; the `H` key animates back over
`HOME_DEFAULT_DURATION` (currently 800ms).

Mark the visible scene entity or entities directly. For builder-spawned
primitives, call `.insert(CameraHomeTarget)` on the primitive builder. For
manual spawns, include `CameraHomeTarget` in the spawned bundle. If no target
exists, Fairy Dust logs a warning once and the home camera waits for a target.
`AnimateToFit` resolves the focus and radius so the target union fits the
viewport given the margin.

Lagrange examples that demonstrate camera behavior may spawn cameras manually
or maintain multiple routed cameras. In those cases, still mark the home region
with `CameraHomeTarget`; then either tag the one fairy_dust-owned home camera
with `FairyDustOrbitCam` or trigger `AnimateToFit` against
`CameraHomeEntity` from the example-specific routing code. `swapped_axis.rs`,
`viewports_windows.rs`, and `render_to_texture.rs` are examples of this
exception: camera setup is part of what they demonstrate, while
`CameraHomeTarget` still defines the AABB.

### Stable transparency

Use `.with_stable_transparency()` (only valid after `.with_orbit_cam(...)`)
when the scene contains coplanar `WorldText` or other translucent geometry
that benefits from order-independent transparency.

### Custom screen-space panels

Prefer built-in panels (`TitleBar`, `DescriptionPanel`,
`.with_camera_control_panel()`) when they fit. If an example needs a custom
screen-space panel, build only the contents manually and use the shared
fairy_dust shell:

Descriptive prose should use the same size as title-bar control chips such as
`H Home` and `Z ZoomToFit`: keep the default `DescriptionPanel` body size, or
use `fairy_dust::LABEL_SIZE.0` when setting `.with_body_size(...)` explicitly.
Do not scale descriptive panel text up for emphasis; oversized explanatory
copy reads as toy-like and competes with the example itself.

```rust
let unlit = fairy_dust::screen_panel_material();
DiegeticPanel::screen()
    .size(Fit, Fit)
    .anchor(Anchor::TopRight)
    .material(unlit.clone())
    .text_material(unlit)
    .layout(|builder| {
        fairy_dust::screen_panel_frame(
            builder,
            Sizing::FIT,
            fairy_dust::DEFAULT_PANEL_BACKGROUND,
            |builder| {
                // Custom rows, columns, and text go here.
            },
        );
    })
    .build();
```

Use fairy_dust's exported `TITLE_COLOR`, `TITLE_SIZE`, `LABEL_SIZE`, and
`DEFAULT_PANEL_BACKGROUND` so custom panels match the title bar, help overlay,
and camera control panel.

### `DiegeticUiPlugin`

`DiegeticUiPlugin` is registered automatically inside `sprinkle_example`.
Examples may spawn `WorldText` or `DiegeticPanel` directly without an
explicit `add_plugins` call. Inside `crates/bevy_diegetic/examples/*`,
include a one-line comment at the top of `fn main` noting this so readers
don't go hunting for the registration:

```rust
fn main() {
    // `bevy_diegetic::DiegeticUiPlugin` is registered automatically by
    // `fairy_dust::sprinkle_example`.
    fairy_dust::sprinkle_example()
        // ...
}
```

## Manual-spawn cases

Only spawn entities manually when fairy_dust cannot reach the use case:

- The entity's `Entity` ID is needed elsewhere (e.g. as a `ZoomToFit::target`).
- The entity uses a primitive fairy_dust doesn't expose (sphere, capsule,
  custom mesh, etc.).
- The entity carries example-specific components (markers, custom resources).

Even in these cases:
- Lighting and ground plane still come from fairy_dust.
- The camera home still comes from `.with_camera_home()` plus
  `CameraHomeTarget`.
- The HUD still uses `TitleBar`.

## HUD chip conventions

- Chip labels read `<key> <Action>` — e.g. `Z ZoomToFit`, `L LookAt`,
  `H Home`. Single-letter key first, action word(s) after, space-separated.
- Multi-key modifiers use the literal characters (e.g. `^⇧R`) — but the
  hot-restart chip is intentionally hidden.
- `H Home` is auto-prepended when `.with_camera_home()` is used; do not
  add it manually.

## Observer/event highlighting

When an example wires keyboard actions to camera events (`ZoomToFit`,
`LookAt`, etc.), observe the matching `*Begin`/`*End` events and toggle
`TitleBarControlState::set_active` on the chip:

- `ZoomBegin`/`ZoomEnd` carry `target` — filter by entity ID.
- `AnimationBegin`/`AnimationEnd` carry `source: AnimationSource` — filter
  by source (`LookAt`, `LookAtAndZoomToFit`, etc.).

## What to remove when converting an existing example

- Manual `setup` that spawns lighting → delete, use `.with_studio_lighting()`.
- Manual ground plane spawn → delete, use `.with_ground_plane()`.
- Manual `Camera3d`/`OrbitCam` spawn → delete, use `.with_orbit_cam(...)`.
  Lagrange examples whose purpose is camera spawning, routing, render targets,
  or multi-camera behavior may keep manual camera spawns, but should still use
  fairy_dust for the surrounding app plumbing, HUD, and home target markers.
- Custom `home_camera` keyboard system + `PlayAnimation::new(... ToOrbit ...)` →
  delete, use `.with_camera_home()` plus `CameraHomeTarget`.
- Click-on-ground observer that triggers `ZoomToFit` / `AnimateToFit` back to
  the scene bounds → delete. The `SceneBounds` resource and the ground
  observer go with it. `H Home` is the standard homing affordance.
- Manual top-left HUD built from `DiegeticPanel::screen()` → delete, use
  `.with_title_bar(TitleBar::new(...))`.
- Custom screen-space panels that copy fairy_dust border, padding, radius, or
  material setup → replace the copied shell with `screen_panel_frame(...)` and
  `screen_panel_material()`.
- Inline `info!`-only observers for animation/zoom events → delete; use
  `RUST_LOG` for debugging.
- Debug-overlay toggle code that's not core to the example's intent →
  delete unless the example specifically demonstrates the overlay.

## Open questions / future work

- Should `.with_camera_home()` optionally accept face-label config so
  the invisible home cube can carry visible text without needing a
  separate visible cube?
- Should the example HUD support per-chip mouse-click activation (turn
  chips into buttons)?
- `side_by_side.rs` and `text_stress.rs` still use raw `App::new()` —
  convert to `fairy_dust::sprinkle_example()` per this guide and drop
  their manual `add_plugins(DiegeticUiPlugin)`.
