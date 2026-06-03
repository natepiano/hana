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
- Example keyboard shortcuts are registered with `.with_shortcut(...)` /
  `.with_held_shortcut(...)` so they never collide with Fairy Dust's own
  modifier chords.
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
        .size(fairy_dust::EXAMPLE_GROUND_SIZE)
    .with_cube()
        .size(CUBE_SIZE)
        .color(fairy_dust::EXAMPLE_CUBE_COLOR)
        .transform(Transform::from_translation(CUBE_TRANSLATION))
        .face_label(Face::Front, "Label")
        .insert(CameraHomeTarget)
    .with_orbit_cam_preset(
        |cam| { /* per-example camera tweaks */ },
        OrbitCamPreset::BlenderLike,
    )
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
    .with_shortcut(KeyCode::KeyZ, zoom_to_fit_target)
    .with_shortcut(KeyCode::KeyL, look_at_target)
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
  or `.color()` per example, but do not hand-roll a plane. For canonical
  cube-focused examples, use `.size(fairy_dust::EXAMPLE_GROUND_SIZE)` so the
  ground footprint stays consistent across the suite. Do **not** wire a
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

Use `.with_cube().size().color().transform().face_label(...)` for every
demo cube whose `Entity` ID is **not** needed elsewhere. The canonical cube
values are:

- `fairy_dust::EXAMPLE_CUBE_SIZE`
- `fairy_dust::EXAMPLE_CUBE_COLOR`
- `fairy_dust::example_cube_on_ground(clearance)`

Use `clearance = 0.1` for ordinary cube examples so the bottom face does not
z-fight with the ground plane. Fairy Dust cube primitives automatically carry
the `FairyDustCube` marker as identity metadata; still insert
`CameraHomeTarget` explicitly when the cube defines the home region.

Use `fairy_dust::cube_face_text(face, text, cube_size, text_size, color)` —
returned as a child bundle on a `commands.spawn` — only when the cube must be
spawned manually because its `Entity` is referenced later (e.g. as the target
of `ZoomToFit::new(camera, target)`).

Prefer `fairy_dust::cube_face_label(face, text, cube_size)` over
`cube_face_text(...)` when the text is the canonical single-line blue cube
label. Use cube face panels for multi-row content:

- `CubeFacePanelStyle::for_cube(cube_size)` for balanced face-relative sizing.
- `CubeFacePanelContent::idle(...)` / `.active(...)` for title and row text.
- `cube_face_panel(...)`, `cube_face_panel_tree(...)`, and
  `set_cube_face_panel_tree(...)` for spawn/update paths.
- `CUBE_FACE_PANEL_RELEASE_HOLD` with `ReleaseHold<T>` when live input text
  should remain visible briefly after release.

### Canonical cube spin

Use `.with_cube_spin::<Marker>(CubeSpinConfig::new())` for decorative cube
rotation in input/preset examples. The default helper:

- registers a `P Pause` title chip;
- binds `KeyCode::KeyP`;
- starts in `CubeSpinMode::Spinning`;
- highlights the chip only in `CubeSpinMode::Paused`.

So the visual default is paused off: the cube spins and the `P Pause` chip is
inactive until the user presses `P`. Use `.cube_spin(config)` on the cube
builder only for single Fairy Dust cube scenes. Use `.without_chip()` and
`.without_key()` when an example needs spin motion without the title affordance.

### Camera

Use `.with_orbit_cam_preset(configure, OrbitCamPreset::BlenderLike)` for a
normal fairy_dust-managed preset camera. Default to `BlenderLike`; use another
preset only when the example is specifically demonstrating that preset. The
`configure` closure usually does nothing (`|_| {}`) — the home pose drives the
starting view.

Use the companion helpers when the input mode is part of the example:

- `.with_orbit_cam_preset(...)` / `.with_orbit_cam_preset_bundle(...)` for
  built-in presets.
- `.with_orbit_cam_bindings(...)` / `.with_orbit_cam_bindings_bundle(...)` for
  app-owned `OrbitCamBindings`.
- `.with_orbit_cam_manual(...)` / `.with_orbit_cam_manual_bundle(...)` for
  manually supplied camera motion.

Use the `_bundle` variants when the camera also needs extra camera-side
components such as `Transform`, `Projection`, render settings, or an
example-specific marker. Use low-level `.with_orbit_cam(configure, bundle)`
only when the mode-specific helpers cannot express the example.

When an example spawns an `OrbitCam` manually to teach bindings, manual input,
or input routing, call `fairy_dust::apply_example_orbit_cam_limits(&mut cam)`
after setting the demonstrated `OrbitCamInputMode`. That keeps pitch limits,
zoom limits, and upside-down behavior consistent without hiding the input mode
being taught.

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
manual spawns, include `CameraHomeTarget` in the spawned bundle. Do not mark
the ground plane just to create a large bound; in multi-object scenes, mark the
visible objects whose AABBs define the subject, such as the showcase cuboid,
sphere, and torus. If no target exists, Fairy Dust logs a warning once and the
home camera waits for a target. `AnimateToFit` resolves the focus and radius so
the target union fits the viewport given the margin.

For canonical single-cube examples, put `CameraHomeTarget` on the cube and use
`.margin(0.5)` when the face text/panels need comfortable framing. Marker
placement defines the subject AABB; margin controls how much space the camera
leaves around it.

Lagrange examples that demonstrate camera behavior may spawn cameras manually
or maintain multiple routed cameras. In those cases, still mark the home region
with `CameraHomeTarget`; then either tag the one fairy_dust-owned home camera
with `FairyDustOrbitCam` or trigger `AnimateToFit` against
`CameraHomeEntity` from the example-specific routing code. `swapped_axis.rs`,
`viewports_windows.rs`, and `render_to_texture.rs` are examples of this
exception: camera setup is part of what they demonstrate, while
`CameraHomeTarget` still defines the AABB.

### Keyboard shortcuts — use the builder

Bind example keyboard shortcuts with `.with_shortcut(key, system)` (runs once
per press) or `.with_held_shortcut(key, system)` (runs every frame the key is
held) instead of a hand-rolled `Res<ButtonInput<KeyCode>>` system. Each handler
is a plain Bevy system; the example never imports `bevy_enhanced_input` for
input.

The builder fires a shortcut only when **no modifier is held**, so a bare key
never also fires when the user presses a Fairy Dust chord on the same letter —
e.g. `Ctrl+Shift+A` toggles the home-AABB gizmo without also triggering a
bare-`A` shortcut. That modifier guard is the one thing a raw `ButtonInput`
reader is missing.

Rules:

- One key per handler. For "key 1–N selects variant N", register one thin
  handler per key (or have each set a small request resource that a single
  system reads), rather than one system that reads every key.
- Two keys, one behavior (e.g. `P` and `Space` both pause) — register the same
  system under both keys.
- `H` (home) and `P` (cube spin) are **reserved** by `.with_camera_home()` and
  `.with_cube_spin()`. Registering a shortcut on a reserved key fails at
  startup with a clear panic. Drive home/pause through those capabilities; if
  an example must run extra logic on home, observe the home fit's
  `AnimationBegin` (an `AnimateToFit` whose target is the home cube) rather than
  reading `H`. `animation.rs` and `swapped_axis.rs` do exactly this.
- A modifier chord an example genuinely demonstrates can't be a bare shortcut
  (it fires only when no modifier is held) — leave that as its own input
  system.

### Stable transparency

Use `.with_stable_transparency()` after installing a Fairy Dust OrbitCam helper
when the scene contains coplanar `WorldText` or other translucent geometry that
benefits from order-independent transparency.

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
Examples may spawn `WorldText` or `DiegeticPanel` directly without an explicit
`add_plugins` call. The `crates/bevy_diegetic/examples/*` examples follow the
same Fairy Dust scene, OrbitCam, lighting, ground, and HUD conventions as
`bevy_lagrange` examples. Inside those examples, include a one-line comment at
the top of `fn main` noting this so readers don't go hunting for the
registration:

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
- `P Pause` is the canonical pause/spin-stop affordance. It starts inactive
  when the example starts unpaused or spinning, and becomes active only while
  the example is paused.
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
- Manual `Camera3d`/`OrbitCam` spawn → delete, use the matching Fairy Dust
  OrbitCam helper, such as `.with_orbit_cam_preset(...)`,
  `.with_orbit_cam_bindings(...)`, or `.with_orbit_cam_manual(...)`.
  Lagrange examples whose purpose is camera spawning, routing, render targets,
  or multi-camera behavior may keep manual camera spawns, but should still use
  fairy_dust for the surrounding app plumbing, HUD, and home target markers,
  and should tag the Fairy Dust-controlled camera with `FairyDustOrbitCam`.
- Custom `home_camera` keyboard system + `PlayAnimation::new(... ToOrbit ...)` →
  delete, use `.with_camera_home()` plus `CameraHomeTarget`.
- Raw `Res<ButtonInput<KeyCode>>` systems that read bare keys for incidental HUD
  shortcuts → replace with `.with_shortcut(key, system)` /
  `.with_held_shortcut(key, system)`. Keep raw input only where it is the point:
  the input-teaching examples (`input_*`, `ime`) or a demonstrated modifier
  chord that can't be a bare key.
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

## Future work

Implemented shared APIs are tracked in `docs/fairy_dust/enhancements.md`.

- Should `.with_camera_home()` optionally accept face-label config so
  the invisible home cube can carry visible text without needing a
  separate visible cube?
- Should the example HUD support per-chip mouse-click activation (turn
  chips into buttons)?
- `side_by_side.rs` and `text_stress.rs` still use raw `App::new()` —
  convert to `fairy_dust::sprinkle_example()` per this guide and drop
  their manual `add_plugins(DiegeticUiPlugin)`.
