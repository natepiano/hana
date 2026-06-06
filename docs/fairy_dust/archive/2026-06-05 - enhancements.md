# Fairy Dust Enhancements Plan

## Objective

Move repeated example-scaffolding code from `bevy_lagrange` and `bevy_diegetic`
examples into small Fairy Dust APIs, while keeping examples explicit about the
primary API they are demonstrating.

The intent is not to hide `OrbitCam` setup from examples that teach camera input.
It is to remove repeated presentation, layout, title-chip, and canonical-scene
boilerplate that makes examples drift from one another.

## Implemented Enhancements

This plan has been implemented. The table is kept as the implementation record
for the shared Fairy Dust API surface and the example families it was designed
to cover.

| Functionality | What survey showed | Implemented API surface | Files covered |
|---|---|---|---|
| Canonical theme constants | Fairy Dust already exposes `TITLE_SIZE`, `LABEL_SIZE`, `TITLE_COLOR`, and `DEFAULT_PANEL_BACKGROUND`. Examples still define local panel colors, release timing, and face-panel sizing choices. Public constants should be added only when callers need them for ad-hoc composition; otherwise prefer helper defaults. | Keep existing constants; add narrowly named values only where needed: `CUBE_FACE_PANEL_BLUE`, `CUBE_FACE_PANEL_RELEASE_HOLD`. | lagrange/input_custom, lagrange/input_gamepad, lagrange/input_keyboard, lagrange/input_manual, lagrange/input_preset_blender_like, lagrange/input_preset_simple, lagrange/pausing, diegetic/aa_text |
| Canonical primitive constants | Canonical examples repeat cube color, cube size, cube clearance, and ground size, but they do not all share placement. `basic` and `follow_target` place cubes directly on the plane; input examples intentionally use `0.1` clearance; `world_text` positions the cube as part of its composition. | Prefer builder defaults where possible. Public constants: `EXAMPLE_CUBE_COLOR`, `EXAMPLE_CUBE_SIZE`, `EXAMPLE_GROUND_SIZE`. Helper: `example_cube_on_ground(clearance)` instead of one fixed `example_cube_translation()`. | lagrange/input_custom, lagrange/input_gamepad, lagrange/input_keyboard, lagrange/input_manual, lagrange/input_preset_blender_like, lagrange/input_preset_simple, lagrange/pausing; exact constants only for lagrange/basic, lagrange/focus_bounds, lagrange/follow_target, lagrange/orthographic, diegetic/world_text |
| Cube builder typing | `PrimitiveBuilder` currently represents both cubes and ground planes, while face labels and spin are cube-only. Adding more cube-only methods to a shared builder increases surprise. | Use explicit cube-scoped names until the builder is split: `PrimitiveBuilder::face_label(...)`, `PrimitiveBuilder::cube_spin(...)`, free `cube_face_*` helpers, and explicit `CameraHomeTarget` insertion. | fairy_dust builder internals, then lagrange/input_custom, lagrange/input_gamepad, lagrange/input_keyboard, lagrange/input_manual, lagrange/input_preset_blender_like, lagrange/input_preset_simple, lagrange/pausing, diegetic/aa_text |
| Cube spin and tumble | Repeated cube rotation exists beyond the input examples: `aa_text`, `screen_space`, `render_to_texture`, and `pausing` also spin or rotate cubes. The usages differ: input examples need a title-toggle, `pausing` needs virtual-time semantics, `render_to_texture` rotates on more than one axis, and `aa_text` tumbles around a tilted axis. | Layered API: primitive-owned `.cube_spin(config)` for Fairy Dust cubes; app-level `.with_cube_spin::<Marker>(config)` for manually spawned targets. `CubeSpinConfig::new()` is the canonical `P Pause` helper: `KeyCode::KeyP`, initial `CubeSpinMode::Spinning`, active chip state only when `CubeSpinMode::Paused`. `CubeSpinMotion::{Yaw(f32), AxisAngle { axis, radians_per_second }, Euler { radians_per_second: Vec3 } }`. `CubeSpinTimeSource::{Virtual, Real}`. | lagrange/input_custom, lagrange/input_gamepad, lagrange/input_keyboard, lagrange/input_manual, lagrange/input_preset_blender_like, lagrange/input_preset_simple, diegetic/aa_text |
| Spin target identity | A broad `FairyDustCube` marker is useful metadata, but it is not a safe spin target in multi-cube scenes. Spin runtime must be scoped to the actual target or target group. | `FairyDustCube` is identity metadata on Fairy Dust cube primitives. Primitive-owned spin inserts `FairyDustCubeSpinTarget` on that primitive. App-level spin uses caller markers. Shared input/chip state lives in marker-scoped `CubeSpinControl<M>`. | lagrange/input_custom, lagrange/input_gamepad, lagrange/input_keyboard, lagrange/input_manual, lagrange/input_preset_blender_like, lagrange/input_preset_simple |
| Default spin and home controls | `with_camera_home()` already defaults the `H Home` chip and supports `without_title_bar_control()`. Cube spin should mirror that pattern without becoming call-order sensitive, and future default chips should not each invent their own title-bar merge path. | Add an internal `TitleBarControlRegistry`. Camera home, cube spin, and future helpers register default chips there. Title-bar installation merges registry chips with explicit `TitleBar::control(...)` entries deterministically and dedupes by chip identity. Keep `CameraHomeTarget` explicit, or add an explicit `.home_target()` cube-builder method later. | fairy_dust title-bar/camera-home internals, lagrange/input_custom, lagrange/input_gamepad, lagrange/input_keyboard, lagrange/input_manual, lagrange/input_preset_blender_like, lagrange/input_preset_simple |
| Title chip state mapping | Many examples map enum/resources into `ControlActivation`: input spin enums, SMAA, OIT, TAA, projection choice, overlays, rulers, and cycling state. Some resources drive multiple chips with opposite activation, so a no-argument trait cannot replace the closure API. | Keep `.wire_chip_to_state::<R, _>(chip, extractor)` as the primary flexible API. Add `TitleChipActivation { fn activation(&self) -> ControlActivation }` and `.wire_chip_to_activation::<R>(chip)` only as one-resource/one-chip sugar. | lagrange/input_custom, lagrange/input_gamepad, lagrange/input_keyboard, lagrange/input_manual, lagrange/input_preset_blender_like, lagrange/input_preset_simple, lagrange/orthographic, diegetic/aa_text, diegetic/panel_rendering, diegetic/slug_text, diegetic/typography, diegetic/units, diegetic/world_text |
| Title chip identity | Visible strings were both display labels and identity. That kept typo bugs alive between `.control(...)` and `.wire_chip_to_state(...)`, especially as the plan added more automatic chips. | `TitleChip::new(id, label)` separates stable identity from visible label; `TitleChip::label(label)` and `&'static str` preserve `id == label` convenience. The canonical cube-spin chip is `TitleChip::new("cube_spin_pause", "P Pause")`. | lagrange/input_custom, lagrange/input_gamepad, lagrange/input_keyboard, lagrange/input_manual, lagrange/input_preset_blender_like, lagrange/input_preset_simple, lagrange/orthographic, diegetic/aa_text, diegetic/panel_rendering, diegetic/slug_text, diegetic/typography, diegetic/units, diegetic/world_text |
| Delayed live-label hold | The 300ms stay-visible-after-release behavior is repeated in every input example and is UI timing, not camera logic. Stored active values vary from key labels to full face-panel content, so the helper should not own idle semantics. `input_custom` also needs disabled-state clearing. | `ReleaseHold<T>` owns the last active value and remaining time. `update(&mut self, delta: Duration, active: Option<T>) -> HoldState<'_, T>` where `HoldState` is `Active(&T)`, `Held(&T)`, or `Idle`. Add `clear()` for disabled/override states. Examples map `Idle` to local canonical rows. | lagrange/input_custom, lagrange/input_gamepad, lagrange/input_keyboard, lagrange/input_manual, lagrange/input_preset_blender_like, lagrange/input_preset_simple |
| Cube face panel builder | Cube-mounted panel transform, transparent material, layout tree, title/body/active text, and update code is duplicated across input examples, `pausing`, and `aa_text`. A single high-level helper is too narrow for `aa_text` and too broad if it becomes a second generic panel framework. | Low-level layer: `cube_face_transform`, `cube_face_panel_material`, `cube_face_panel_with_tree(...)`, `set_cube_face_panel_tree(...)`. High-level input layer: `CubeFacePanelStyle`, `CubeFacePanelContent { title: Cow<'static, str>, rows: Vec<Cow<'static, str>>, activity: CubeFacePanelActivity }`, `cube_face_panel(...)`. Content updates rebuild the tree with `cube_face_panel_tree(style, content)` and apply it via `set_cube_face_panel_tree(...)` or `commands.set_tree(...)`; there is no separate content setter. | lagrange/input_custom, lagrange/input_gamepad, lagrange/input_preset_blender_like, lagrange/input_preset_simple for high-level content; lagrange/pausing and diegetic/aa_text for low-level transform/material/tree helpers only |
| Cube face text helper upgrade | `input_keyboard`, `input_manual`, and `zoom_to_fit` use `cube_face_text` directly. `orthographic` reaches similar face-label styling through `PrimitiveBuilder::face_text`. They do not all need full panels, but should share canonical text color and sizing. | Keep `cube_face_text`; add `cube_face_label(face, text, cube_size)` and `PrimitiveBuilder::face_label(...)` using canonical blue and size defaults. | lagrange/input_keyboard, lagrange/input_manual, lagrange/zoom_to_fit, lagrange/orthographic |
| Canonical OrbitCam example policy | Input examples manually spawn cameras to teach input modes. The policy should not become public constants or hide the input mode, but the repeated pitch/zoom/upside-down defaults should not drift. | Internal default in `.with_orbit_cam*`; auxiliary helper only for manual camera examples: `apply_example_orbit_cam_limits(&mut OrbitCam)`. | lagrange/input_custom, lagrange/input_gamepad, lagrange/input_keyboard, lagrange/input_manual, lagrange/input_preset_blender_like, lagrange/input_preset_simple |
| Camera home default chip | Already handled: `with_camera_home()` prepends `H Home`, and `without_title_bar_control()` exists. | No new API. | none |

## Implementation Notes

- Keep chip labels as constants at call sites whenever examples spell them
  manually.
- Prefer enum-shaped state over bool-shaped state for user-visible modes.
- Keep closure-based title-chip mapping for resources that drive multiple chips.
- Do not hide primary example code. Input examples should still show the chosen
  `OrbitCamInputMode`; Fairy Dust should absorb the supporting presentation code.
- Make helpers opt-in and composable. Examples with special behavior such as
  `pausing` should still be able to choose the time source used by cube spin.
- Avoid making `fairy_dust` depend on example-specific semantics. The shared API
  should describe presentation primitives: title-chip state, cube spin, held
  labels, cube face panels, and canonical scene constants.
- Prefer builder-owned defaults over broad public constants. Export constants
  only when examples need them to compose ad-hoc UI or scene code.

## Completed Rollout

This was the implementation order, retained as the audit trail for the sweep.

1. Add non-invasive primitives first: title chip label/activation sugar,
   release-hold helper, cube face transforms, and face-panel style/content types.
2. Add `TitleBarControlRegistry` and route camera-home chip registration through
   it before adding cube-spin chips.
3. Split or clearly specialize the primitive builder for cube-only helpers.
4. Add `FairyDustCube` as identity metadata without changing home or spin
   behavior.
5. Add cube spin helpers in two layers: cube-builder shorthand for Fairy Dust
   cubes and marker-based app-level helpers for manual targets.
6. Migrate one small input preset example and verify the primary API remains
   first-readable.
7. Migrate the remaining input examples.
8. Evaluated `pausing`, `render_to_texture`, `aa_text`, and `screen_space`
   individually. Shared helpers were applied only when they kept the
   demo-specific timing, rotation, or rendering lesson explicit.

## Migration Guardrails

- After each migration, the first readable section of the example must still
  expose its primary symbols, such as `OrbitCamInputMode`,
  `OrbitCamBindings::builder()`, or `OrbitCamManualInputWriter`.
- Migrate only direct one-resource/one-chip states to `TitleChipActivation`
  sugar. Leave multi-chip or inverse mappings on `wire_chip_to_state`.
- `input_gamepad` is special: keep `.without_title_bar_control()`,
  `GAMEPAD_HOME_CONTROL`, `home_on_gamepad_south`, and `finish_gamepad_home`
  local. Shared helpers may cover spin and face-panel scaffolding only.
- `basic`, `follow_target`, and `focus_bounds` now use the canonical Fairy Dust
  scene scaffold. Their camera lesson should stay visible through
  `with_orbit_cam_preset*` helpers and local `configure_camera` functions, not
  by returning to raw `App`/`commands.spawn` setup.
- `screen_space` does not adopt Fairy Dust helpers in this plan.
- `render_to_texture` keeps explicit render-target/routing setup local and
  should not use generic spin helpers unless the resulting call still describes
  the two-axis render-target rotation clearly.
- `pausing` and `aa_text` use only low-level cube-face panel helpers unless a
  higher-level helper preserves their state-machine teaching surface.

## Review Notes

### Cycle 1

Recorded refinements:

- Keep `wire_chip_to_state` as the primary title-chip API; add activation trait
  sugar only for one-resource/one-chip cases.
- Rename the proposed trait away from `TitleControlState` to avoid collision
  with internal `TitleBarControlState`; use `TitleChipActivation`.
- Consider a `TitleChip` newtype for constant chip labels while retaining `&str`
  convenience.
- Scope cube spin state per target and separate primitive-owned spin from
  marker-owned spin.
- Make cube spin config handle optional title controls, angular velocity,
  explicit time source, and enum-shaped initial state.
- Add a Fairy Dust cube marker, but keep `CameraHomeTarget` explicit.
- Make cube-face panel APIs cube-scoped, styleable, and layered between
  low-level geometry/material helpers and simple content helpers.
- Replace fixed cube translation with independent constants and a clearance
  parameter helper.
- Add builder-chain face label coverage for `PrimitiveBuilder::face_text` cases.

Cycle 1 summary: 9 mechanical refinements recorded, 0 proposed user decisions.

### Cycle 2

Recorded refinements:

- Add a general `TitleBarControlRegistry` so default chips are order-independent
  and not home-specific.
- Treat `FairyDustCube` as identity metadata only. Spin target identity remains
  explicit through cube-builder-owned private markers or caller markers.
- Prefer a typed cube builder before adding more cube-only methods. Use explicit
  `cube_*` names only as an interim path if a builder split is too large.
- Replace scalar spin speed with `CubeSpinMotion` so yaw, axis-angle tumble, and
  per-axis rotation are all representable.
- Keep spin runtime state on target entities, with marker-scoped resources only
  for shared input/chip control.
- Promote `TitleChip` from a possible convenience to a real identity type.
- Specify `TitleChipActivation` as resource-level sugar only.
- Specify `ReleaseHold<T>` ownership, `Duration` timing, borrowed return states,
  and `clear()` for disabled/override flows.
- Add tree-level cube-face panel helpers so `pausing` and `aa_text` can keep
  their local state snapshots and layout trees explicit.
- Add migration guardrails for first-readable primary APIs, special gamepad
  home behavior, and examples that should not migrate beyond constants or
  low-level helpers.

Cycle 2 summary: 10 mechanical refinements recorded, 0 proposed user decisions.
