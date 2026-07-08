# FreeCam (as-built)

## What it is

`FreeCam` is the second camera kind in `bevy_lagrange`, alongside the original `OrbitCam`. It is a free-flight camera with a **decoupled look direction and translate-along-look motion**: the editor can turn the view away from whatever it is editing and walk forward into empty space to build something new. None of the existing capabilities cover this. `OrbitCam` is pivot-locked — it always faces its focus and cannot turn away. `zoomToFit`/`LookAt` need an existing target to frame and cannot travel to empty space. View-plane pan translates but cannot rotate. `FreeCam` owns a world `Position`, a `LookAngles` (yaw/pitch), and a `Roll`, each an eased `Operation`, and writes them straight to the entity `Transform`. It ships with a mouse-and-keyboard preset (RMB-look + WASD/Space/Ctrl translate + Q/E roll), a twin-stick gamepad preset, and named orientation constructors (`pitch_limited`, `horizon_locked`) for editor-flavored constraint modes.

## How it works

### The `CameraKind` type family

`CameraKind` (`camera_kind.rs`) is a compile-time marker trait implemented by the zero-sized keys `OrbitCamKind` (`orbit_cam/mod.rs`) and `FreeCamKind` (`free_cam/mod.rs`). It is the checklist every camera kind must satisfy. `type Camera: Component` names the component (`OrbitCam` / `FreeCam`), and `add_camera_kind_systems(app)` — the single entry each camera plugin calls — fans out to the required registration hooks:

- `add_controller_systems` — the controller system plus type registration, enhanced-input context, input adapter plugin, and home systems.
- `add_animation_systems` — the per-kind `CameraMoveList`/`PlayAnimation` apply system.
- `add_animate_to_fit_systems`, `add_zoom_to_fit_systems`, `add_look_at_systems` — the fit-family observers (the last registers both `LookAt` and `LookAtAndZoomToFit`).
- `add_camera_kind_support_systems` — optional shared-support hook, defaults to no-op.

`LagrangePlugin` (`lib.rs`) is a composer: it adds the shared plugins (`LagrangeSystemSetsPlugin`, `AnimationPlugin`, `InputPlugin`, `FitPlugin`) then the two per-kind plugins `OrbitCamPlugin` and `FreeCamPlugin`, each of which just calls `K::add_camera_kind_systems`.

The kind key is progressively extended by narrower trait families, so one generic engine can be registered per kind:

- `CameraInputKind: CameraKind + TypePath` (`input/intent.rs`) — adds `Context`, `Input`, `Channels`. Sealed.
- `CameraInputModeKind: CameraInputKind` (`input/modes.rs`) — adds `Preset`, `Bindings`, `default_mode`, `describe_controls`/`describe_controls_for`. Sealed; the former `Error` associated type was dropped (replaced by the concrete `BindingsError`).
- `CameraActionResolutionKind` (`input/action_resolution.rs`) — action entities/queries/frame-state hooks for the resolver shell.
- `CameraInputLifecycleKind` (private, `input/lifecycle.rs`) — interaction kinds/state/event hooks.
- `CameraHomeKind` (`camera_home.rs`) — `HomePose`, `InteractionStarted`, `capture_home`/`apply_home`.
- `CameraInstallKind` (`input/install.rs`) — the per-kind gate action for enhanced-input installation.

### Operation state (shared)

`Operation<V: Smoothable>` (`operation.rs`) is kind-agnostic driven state: a smoothed `current`/`target` pair, a `Sensitivity`, a `Damping`, and a `V::Limit`. `update(delta)` constrains the target then eases current toward it with frame-rate-independent smoothing (`interpolation.rs`). The coordinates are all newtypes with `Smoothable`/`Limit` impls: `OrbitCam` uses `OrbitAngles`/`Focus`/`Radius`; `FreeCam` uses `Position`/`LookAngles`/`Roll`. `AnglePairLimit`, `ScalarLimit`, `RegionLimit` are the limit types. The one per-kind part — how an input delta becomes a target change — lives in each controller, not in `Operation`.

### Input stack (generic shell + per-kind hooks)

`InputIntent<K: CameraInputKind>` (`input/intent.rs`, the generic core) is the per-frame intent accumulator: a `K::Channels` bundle of `IntentChannel<D>` lanes, each carrying a delta, `InteractionSources`, `ControlSpeed`, and an active flag. Each camera's intent *vocabulary* lives beside the camera, not under `input/`: `free_cam/intent.rs` holds the `TranslateDelta`/`LookDelta`/`RollDelta` types, the `FreeCamChannels` struct (private fields), the `FreeCamInput = InputIntent<FreeCamKind>` alias (translate/look/roll channels, plus a `FreeCamActiveDirections` field for panel highlighting), its read accessors (`translate()`, `look()`, `roll()`, `has_look()`, `set_look_speed()`, …), and the `pub(crate)` mutator impl block on `InputIntent<FreeCamKind>`. It sits at **depth 2** (`mod intent;` re-exported through `free_cam/mod.rs`), mirroring `orbit_cam/intent.rs` — cargo-mend's `forbidden_pub_crate` only permits `pub(crate)` at crate root or in depth-2 private modules, and the mutators' callers (`input/{lifecycle,interaction_state,manual}.rs`) sit outside the camera trees. Do not move it deeper or re-widen the channel fields.

`InputMode<K: CameraInputModeKind>` (`input/modes.rs`, the public mode API) is the mode component: `Preset(K::Preset)`, `Bindings(K::Bindings)`, or `Manual`. `OrbitCamInputMode`/`FreeCamInputMode` are the aliases; both cameras `#[require]` theirs. The private `PreUpdate` runtime lives in `input/mode_reconciliation.rs`: `CameraInputModesPlugin` runs one generic `reconcile_input_modes::<K>` per kind that lowers a preset/binding into resolved bindings (`CameraResolvedBindings<K>`) or installs `CameraManual<K>`, keeps a `LastValidInputMode<K>` to roll back on validation failure, and triggers `CameraInputModeReplaced`. `FreeCam` defaults to `FreeCamPreset::keyboard_mouse()`.

The routed input path is camera-neutral (`input/routing/`): `ResolvedCameraInputRoute` picks the focused camera each frame, and `CameraInputBlockers`, the context gate, `CameraInputSourceLatches`, `CameraSlowModeLatches`/`CameraSlowModeState`, and the candidate snapshot collection all cover both kinds.

`resolve_actions_into_camera_input::<K>` (`input/action_resolution.rs`) is the **generic action-resolution shell**. It owns route checks, blocker/gate checks, input clearing, and slow-mode latch/state handling, then delegates the actual channel math to `K::resolve_camera_actions` via `CameraActionResolutionContext`. `OrbitCamKind` supplies orbit/pan/zoom math from its enhanced-input adapter (`orbit_cam/input/adapter/`); `FreeCamKind` supplies translate/look/roll math from `free_cam/input/adapter.rs`. The FreeCam adapter's `FreeCamInputActionEntities` holds the installed action entities (translate, look, roll, their engagement/gate actions, slow toggle, home); the resolved `FreeCamBindings` are wrapped by the generic `CameraInstalledBindings<FreeCamKind>` (`input/mode_reconciliation.rs`), which replaced the former per-camera `FreeCamInstalledBindings` newtype.

The interaction **lifecycle engine** (`input/lifecycle.rs`, private `CameraInputLifecycleKind`) is one generic `finalize_camera_input::<K>` shared by both kinds. It handles source debounce (`CameraInputReportingDebounce`, held per kind in `CameraReportedInteractionSettle<K>`), speed reporting/settling, source latching, and dispatch of the per-kind `*InteractionStarted/Ended/SourcesChanged/SpeedChanged` events. Orbit-only concerns stay per-kind hooks: `apply_metric_guard` (the manual-screen-input surface-metric check) and `update_extra_state` (orbit zoom direction; FreeCam uses it to report move directions).

Control summaries (`input/control_summary.rs`) are shared: `describe_controls(&mode)` / `describe_controls_for(&camera, &mode)` dispatch through `CameraInputModeKind` and produce a `CameraControlSummary` of `CameraControlBinding` values (`action`, `label`, `interaction_sources: InteractionSources`, `speed`, `kind: Direct | Setting { value, activation }`, plus optional `action_label`/`direction` for decomposed FreeCam rows). FreeCam overrides `describe_controls_for` to inspect camera state (e.g. rendering `Roll disabled` when roll is locked). A legacy `OrbitCamControlSummary`/`OrbitCamControlRow`/`describe_orbit_cam_controls` path is kept for existing examples and converts into the shared model.

### FreeCam data flow: input → intent → controller → Transform

1. `reconcile_input_modes::<FreeCamKind>` lowers `FreeCamInputMode` into `FreeCamResolvedBindings` (or manual), installing enhanced-input action entities via the adapter.
2. Devices feed `bevy_enhanced_input`; the FreeCam adapter's `resolve_camera_actions` runs inside `resolve_actions_into_camera_input::<FreeCamKind>` (only the routed, unblocked camera writes), accumulating translate/look/roll deltas — with slow-mode scaling and `FreeCamLookPitch` inversion already applied — into `FreeCamInput`. Manual-mode apps write through `FreeCamManualInputWriter` in `CameraInputPhase::WriteManual`.
3. `finalize_camera_input::<FreeCamKind>` debounces reported sources/speeds and emits interaction events.
4. The `free_cam` controller (`free_cam/controller.rs`, `PostUpdate`, in `CameraControllerSystemSet`) reads `FreeCam`, `CameraBasis`, `FreeCamInput`, and `TimeSource`. On the first pass it runs `initialize_free_cam` (seeding from `Transform` or the pre-seeded pose per `Initialization`, and inserting a provisional `FreeCamHomePose`). Each frame it multiplies input by per-operation sensitivity, adds translate along `transform.rotation` (translate-along-look), adds look/roll to targets, then — if anything changed, the basis changed, `force_update` was requested, or current≠target — eases the three operations and writes `transform.translation` + `rotation_from_pose(basis, look, roll)`.

### Construction: pose and presets

Spawn helpers (`free_cam/presets.rs`, mirroring `orbit_cam/presets.rs`) return
`impl Bundle` pairing `FreeCam::default()` with a `FreeCamInputMode`:

- `FreeCam::with_preset(impl Into<FreeCamPreset>)` → `FreeCamInputMode::with_preset(preset)`
- `FreeCam::with_bindings(FreeCamBindings)` → `FreeCamInputMode::Bindings(..)`
- `FreeCam::manual()` → `FreeCamInputMode::Manual` (installs no home, by design)

The built-in `FreeCamPreset` variants are `keyboard_mouse` (default), `gamepad`,
and `gamepad_southpaw`.

`FreeCam::from_pose(position, look, roll)` (`free_cam/mod.rs`) snaps a starting pose
from bare values: `position: impl Into<Position>`, `look: impl Into<LookAngles>`,
`roll: impl Into<Roll>`. `impl From<(f32, f32)> for LookAngles` (yaw, pitch order,
`operation.rs`) lets a call site pass a tuple with no `LookAngles { .. }` wrapper, and
`Operation::{set_target, snap_to}` take `impl Into<V>` so retargets read
`camera.look.snap_to(pose.look)`. **Exception:** the `FreeCamHomePose` struct
*literal* keeps its newtype wrappers — struct fields cannot take `impl Into`, so
`FreeCamHomePose { position: Position(..), look: LookAngles { .. }, roll: Roll(..) }`
stays fully typed.

**Pose and preset do not compose into one constructor:** `with_preset` forces
`FreeCam::default()`, so a custom pose plus a tuned preset is the explicit tuple
`(FreeCam::from_pose(..), FreeCamHomePose { .. }, FreeCamInputMode::with_preset(..))`,
not one call.

### Input gain (two-layer)

FreeCam mirrors OrbitCam's two-layer input-gain design: source-level *gain* scales
raw input (like microphone gain), while separately-named behavior *scale* modifiers
tune what the input drives (gain = source-side multiplier; scale = behavior-side).
The existing `FreeCamGamepadPreset::{with_move_scale, with_roll_scale,
with_stick_dead_zone}` are the behavior layer (unchanged). `FreeCamInputGain {
translate, look, roll }` (`free_cam/input/bindings/input_gain.rs`) is the source
layer, mirroring `OrbitCamInputGain` one-to-one (const setters, `uniform`,
`validate`). Preset source-gain setters are `impl MouseInputGain for
FreeCamKeyboardMousePreset` and `impl GamepadInputGain for FreeCamGamepadPreset`;
there is no `SmoothScrollInputGain` implementer (FreeCam has no scroll-driven
binding). The three source-gain traits live in the camera-neutral
`input/source_input_gain.rs` (unsealed). Unlike OrbitCam's bespoke
`OrbitCamBindingWithInputGain` wrapper, FreeCam's per-binding gain uses the shared
`InputBindingDescriptor.scale` path, installed as BEI `Scale` by
`input/install.rs`, with no new binding-kind type.

### Animation / fit apply layer (shared plan, per-kind apply)

`CameraMove` (`animation/queue.rs`) has two variants: `ToLookAt { position, target, roll, duration, easing }` and `ToOrbitalLookAt { target, yaw, pitch, radius, roll, duration, easing }`; `roll: Option<Roll>` is the FreeCam roll target (`OrbitCam` ignores it). `CameraMoveList` is the queue; `PlayAnimation` and the lifecycle events live in `animation/events.rs`; `AnimationPlugin` registers the kind-agnostic observers. Per kind, `add_orbit_cam_animation_systems`/`add_free_cam_animation_systems` register `process_orbit_camera_move_list` / `process_free_camera_move_list` — the two apply systems that interpolate the queue for their component.

The `fit/` domain is shared. `FitPlugin` registers the fit-target lifecycle and the feature-gated overlay; the per-kind observers are registered by each `CameraKind`: `on_{orbit,free}_cam_{animate_to_fit,zoom_to_fit,look_at,look_at_and_zoom_to_fit}` (`fit/triggers/`). **Instant** poses are applied by camera-specific helpers in `fit/camera_pose.rs` (`FreeCamFitPose` + `apply_free_cam_pose`, which snaps the three operations and calls `force_update`; `SnapOrbit` + `snap_to_orbit` for orbit). **Timed** fits route through the shared `CameraMove`/`PlayAnimation` plan — FreeCam has no private fit-animation timer.

## Invariants

- **FreeCam is a sibling camera kind, not an `OrbitCamInputMode` variant.** Orbit state (focus/radius/yaw/pitch around a pivot) and free state (position + look + roll) are different camera kinds. `OrbitCamInputMode`/`FreeCamInputMode` (`Preset`/`Bindings`/`Manual`) describe *how input maps within one kind* — never where a new kind lives.
- **Same structure, not just same concept.** A FreeCam analog lives in the structurally identical location as its OrbitCam counterpart — same module, same file, same visibility. `FreeCamInputContext` sits beside `OrbitCamInputContext` in `input/context.rs`; the FreeCam channels/intent accessors sit in `free_cam/intent.rs`, the structural twin of `orbit_cam/intent.rs`; both cameras register through the same `CameraInputModesPlugin`/routing/lifecycle plugins. Two homes for one concept is the divergence this design exists to prevent.
- **Every `CameraKind` must register the full set:** controller, animation apply (`CameraMoveList`/`PlayAnimation`), `AnimateToFit`, `ZoomToFit`, `LookAt`, and `LookAtAndZoomToFit`. `add_camera_kind_systems` is the single call that enforces this; the individual `add_*_systems` methods are mandatory (only `add_camera_kind_support_systems` defaults to no-op). A new kind that omits any of these will not compile.
- **`CameraBasis` is required component state on both kinds** (`#[require(CameraBasis, …)]` on `OrbitCam` and `FreeCam`). Controllers read it from the entity — never assume Y-up. Camera vertical can map to different world axes.
- **Binding-validation errors are a closed enum.** `BindingsError` (`input/bindings/error.rs`) is the shared, non-`#[non_exhaustive]` error for both kinds' binding validation; FreeCam's slow-scale failure surfaces as `BindingsError::InvalidScale`. Do not reopen it or add per-kind error types without an explicit policy decision. (By contrast `CameraControlAction` and `InputMode` are deliberately `#[non_exhaustive]`; the closed rule is specific to validation errors.)
- **Control summaries are derived, read-only descriptions**, not runtime bindings. `CameraControlSummary`/`CameraControlBinding` are UI/help models produced from input-mode settings and camera state; they are not the `bevy_enhanced_input` bindings the controller consumes. Keep them free of runtime coupling.
- **Look-pitch inversion is input-binding policy, not `LookAngles` state.** `FreeCamLookPitch::{Normal, Inverted}` (`free_cam/input/bindings/preset.rs`) negates mouse Y before it reaches the look channel; it is carried by presets/`FreeCamBindings`, not baked into camera angles. Panel copy can say "Invert Y"; the camera API describes the affected look-pitch channel.
- **The generic-shell rule:** when both kinds do the same work with different channel details, extend the kind trait family and register one engine per kind. Do not fork a parallel near-copy system and hand-sync it.

## Calibration / gotchas

- **`FreeCam::force_update()` exists because active-state direct snaps can leave `Transform` stale.** The controller only recomputes when motion changed, the basis changed, or target≠current. After you mutate current state directly (or `apply_free_cam_pose` snaps all three operations to a fit result at zero drift), call `force_update()` — it sets the hidden `FreeCamUpdateRequest::ForceUpdate` that the next controller pass consumes. `apply_free_cam_pose` already does this.
- **The pitch clamp is shared between behavior modes and survives free-flight cycling.** `FreeCam::pitch_limited()` and `horizon_locked()` configure the *existing* `Operation` limits (`look.limit_mut().pitch = ScalarLimit::Clamp`, and for `horizon_locked` also `roll` clamped to `{min:0,max:0}` plus a snap to zero). They are named constructors over `AnglePairLimit`/`ScalarLimit`, not a separate orientation-policy type. Because the clamp lives on the operation, an editor that cycles free-flight → pitch-limit → horizon-lock keeps the pitch clamp consistent across the constrained modes even after passing through unconstrained free flight.
- **Roll-locked is detected from the limit, not a flag.** `describe_free_cam_controls_for` reads `camera.roll.limit()` and treats a `Clamp{min≈0,max≈0}` as roll-disabled, rendering `Roll disabled` in place of Q/E. If you add a new roll-lock mechanism, keep this detection in sync.
- **`FreeCamBindings::builder()` uses named translation keys.** `FreeCamTranslateKeys` with `with_forward/backward/left/right/up/down` avoids the positional `with_translate_keys(f,b,l,r,u,d)` footgun. The internal handoff is named too, so positional ordering does not just move behind the API. Slow-mode scale is validated (`InvalidScale`) at build time.
- **`ToLookAt`/`ToOrbitalLookAt` both carry `roll: Option<Roll>`.** `OrbitCam` silently ignores it; only `FreeCam` reads it. `ToOrbitalLookAt` exists to avoid gimbal lock at ±π/2 pitch where world-space `atan2` decomposition loses yaw.
- **Reporting debounce is reporting-only.** `CameraInputReportingDebounce` (default in `input/constants.rs`) smooths the *reported* interaction state/events so panels don't flicker on bursty input; camera motion reads intent directly and is never delayed. `Duration::ZERO` disables it. Gamepad `Normal` speed is held pending through the window; `Slow` and fresh engages report immediately.
- **Manual-screen metric guard is OrbitCam-only.** The surface-metric check that clears orbit/pan when view/surface sizes are missing lives in the OrbitCam lifecycle hook; FreeCam has no equivalent because translate-along-look does not need screen metrics.
- **Keyboard roll gain must be applied motion-only.** Use `HeldBinding::same(InputBinding::bidirectional_keys(...)).with_input_gain(...)` — never bake the gain into the raw `InputBinding`. `HeldBinding::same` copies the raw binding to *both* the motion and engagement descriptors, so a low roll gain baked at the `InputBinding` level could shrink the engagement signal below actuation and stop roll from engaging at all.
- **`InputBinding::with_input_gain` and `HeldBinding::with_input_gain` share a name but differ in reach.** The `InputBinding` call bakes gain into the binding, landing on every descriptor it is copied to (including engagement); the `HeldBinding` call touches only the motion descriptor. For any binding that doubles as its own engagement source, only the `HeldBinding`-level call is correct.

## Why

- **Why FreeCam is a sibling kind:** its state (free position + look + roll) is genuinely different from orbit state (pivot + radius + angles), and it delivers a capability — turn away and translate into empty space — that no existing mode has. Folding it into `OrbitCamInputMode` would conflate "which camera kind" with "how input maps within a kind," two orthogonal axes.
- **Why generic shell + per-kind hooks instead of parallel copies:** input modes, routing, action resolution, interaction lifecycle, control summaries, and the animation/fit apply layer are the same work with different channel details. One engine registered per kind (via the layered `CameraInput*Kind` traits) shares all the debounce/latch/route/event machinery the type system can carry, while narrow semantic hooks own only the true differences (orbit vs translate math, zoom direction, metric guard). Two hand-synced systems were the failure mode to avoid.
- **Why extract-only-on-the-second-consumer:** shared abstractions (`interpolation.rs`, the `InputPlugin`, `Operation`, the `CameraInput*Kind` families) were pulled out only when FreeCam became the real second consumer that made them concrete — never speculatively. This kept the migration from becoming a multi-day upfront redesign while still landing each FreeCam analog in lockstep with the OrbitCam refactor that exposed it.
- **Why control summaries moved back into Lagrange:** Lagrange owns input semantics, so it should derive the renderable control descriptions from `FreeCamBindings`/camera state; Fairy Dust should render summaries, not synthesize camera controls. Hard-coded FreeCam guidance rows in the panel were removed in favor of `describe_controls`/`describe_controls_for`.
- **Why `CameraMove::ToLookAt` / `ToOrbitalLookAt`:** the names describe the target specification, not an implementation. `ToLookAt` takes a world `position` + look `target`; `ToOrbitalLookAt` takes a `target` + yaw/pitch/radius and exists specifically to stay gimbal-stable at extreme pitch. Both serve both camera kinds through one planning path, with only the instant-pose application diverging per kind.

## Status

The FreeCam buildout is milestone-complete through the `fit/` module restructure: the shared routed input stack, generic action-resolution and lifecycle engines, shared control summaries, the shared `CameraMove`/`CameraMoveList`/`PlayAnimation` apply layer, look-at parity, and the `fit/` geometry/trigger/overlay reorganization have all landed. Two items remain open: (1) the ongoing top-down migration loop has no specific next module chosen yet — the next step is to return to the traversal and pick the next module/type; and (2) one deferred decision stands — no `FreeCam` private controller state is added until a future stateful FreeCam behavior (pointer capture, roll-mode, or routed input ownership) actually requires persisting state across frames beyond the `Operation` state. The first controller stores no private state.
