# `bevy_lagrange` input architecture

As-built overview of the `bevy_lagrange` camera-input subsystem in
`crates/bevy_lagrange/src/input/`. It describes the shipped types, data flow, and
invariants. The preset API (preset enum, config structs, slow mode) is documented in
full in [`orbit-cam-preset-api.md`](orbit-cam-preset-api.md); this doc defers to it
where they overlap.

## Goal

`OrbitCam` owns camera behavior (focus, yaw/pitch/radius, smoothing, limits, animation,
transform updates). `bevy_enhanced_input` (BEI) owns the action/context input model.
The controller consumes one per-camera semantic snapshot, `OrbitCamInput`, never raw
device input or binding policy.

## Module layout

```text
input/                shared, camera-neutral input layer (generic over CameraKind)
  mod.rs              public overview docs + re-exports
  actions.rs          public semantic actions + sealed action traits
  action_resolution.rs generic action-resolution shell (resolve_actions_into_camera_input::<K>)
  axis_response.rs    per-axis response shaping
  bindings/           shared binding infrastructure (descriptor, validation, held/source bindings, BindingsError)
  constants.rs        control-summary row/label string constants
  context.rs          OrbitCamInputContext / FreeCamInputContext (BEI context components)
  control_summary.rs  describe_controls / describe_controls_for + CameraControlSummary (plus legacy OrbitCam path)
  disabled.rs         CameraInputDisabled
  events.rs           interaction lifecycle events
  install.rs          CameraInstallKind per-kind enhanced-input install gate
  intent.rs           generic intent core: CameraInputKind, IntentChannels, InputIntent<K>, IntentChannel<D>, activity types
  interaction_state.rs OrbitCamInteractionState (read-only tracker)
  lifecycle.rs        generic finalize_camera_input::<K>, lifecycle event emission
  manual.rs           per-kind manual input writers
  metrics.rs          CameraInputSurfaceMetrics, CameraInputMetricKind
  modes.rs            public mode API only: InputMode<K> (OrbitCamInputMode / FreeCamInputMode aliases), CameraInputModeKind, CameraManual
  mode_reconciliation.rs private PreUpdate runtime: CameraResolvedBindings<K>, CameraInstalledBindings<K>, installation components, CameraInputModesPlugin, reconcile/apply systems
  routing/            camera-neutral routing config, resolved route, latches, blockers
  source_input_gain.rs camera-neutral source-gain traits (MouseInputGain / SmoothScrollInputGain / GamepadInputGain)
  sources.rs          InteractionSources, ManualInputSource
  touch.rs            touch tracking
```

Per-camera intent vocabulary lives beside each camera, not under `input/`:
`orbit_cam/intent.rs` (`OrbitDelta`/`PanDelta`/`CoarseZoomDelta`/`SmoothZoomDelta`/`ZoomDelta`,
`OrbitCamChannels`, alias `OrbitCamInput`, the `pub(crate)` mutators) and
`free_cam/intent.rs` (`TranslateDelta`/`LookDelta`/`RollDelta`, `FreeCamChannels`,
alias `FreeCamInput`). These modules sit at **depth 2** (`src/<camera>/intent.rs`,
private `mod intent;` re-exported through the camera `mod.rs`), not under
`src/<camera>/input/`, because cargo-mend's `forbidden_pub_crate` only permits
`pub(crate)` at crate root or in depth-2 private modules and the mutators' callers
(`input/{lifecycle,interaction_state,manual}.rs`) sit outside the camera trees.
Crate-root paths (`bevy_lagrange::OrbitDelta`, etc.) resolve unchanged.

The per-kind BEI adapter and concrete bindings live under each camera's module, not
under `input/`: `orbit_cam/input/adapter/` + `orbit_cam/input/bindings/` (OrbitCamBindings,
builder, presets, descriptor) and `free_cam/input/adapter.rs` + `free_cam/input/bindings/`.
`input/bindings/` holds only the shared descriptor/validation infrastructure and the
shared `BindingsError`.

The public API is grouped under `bevy_lagrange::input` and re-exported from the crate
root. Private engagement/source actions (`OrbitCamOrbitEngagedAction`, etc.) and adapter
actions are not re-exported. This doc uses OrbitCam as the worked example; the FreeCam
analogs sit in the structurally identical locations and are covered in
[`free-cam.md`](free-cam.md).

## Input mode

The active input mode is a single per-camera enum component, `OrbitCamInputMode`
(`modes.rs`):

```rust
#[non_exhaustive]
pub enum OrbitCamInputMode {
    Preset(OrbitCamPreset),
    Bindings(OrbitCamBindings),
    Manual,
}
```

It is `Clone + Debug + PartialEq + Reflect`, defaults to
`OrbitCamInputMode::with_preset(OrbitCamPreset::simple_mouse())`, and has
`From<OrbitCamPreset>` / `From<OrbitCamBindings>` conversions. The full binding
set is inlined in `Bindings` (not boxed) because at most a handful of these
components exist at once.

`OrbitCam` requires the component (`orbit_cam/mod.rs`):

```rust
#[require(
    Camera3d,
    OrbitDragState,
    OrbitCamInput,
    OrbitCamInputContext,
    OrbitCamInputMode
)]
pub struct OrbitCam { /* camera behavior fields */ }
```

A camera spawned with just `OrbitCam::default()` therefore gets the `SimpleMouse`
preset. `OrbitCam` carries no physical binding fields (mouse buttons, modifiers,
smooth-scroll/touch policy, zoom direction); those belong to the input mode, bindings, or
adapter policy.

| Variant | Meaning | Library writes `OrbitCamInput` |
|---------|---------|--------------------------------|
| `Preset(p)` | Build `OrbitCamBindings` from the preset, install actions + adapter policy, resolve input. | yes |
| `Bindings(b)` | Install the app-provided validated bindings, resolve input. | yes |
| `Manual` | Install/resolve nothing; the app writes `OrbitCamInput` directly. | no |

### Spawn helpers

`OrbitCam::*()` helpers (`orbit_cam/presets.rs`) return `impl Bundle`, pairing
`OrbitCam::default()` with the matching `OrbitCamInputMode`: `simple_mouse()`,
`blender_like()`, `gamepad()`, `with_preset(impl Into<OrbitCamPreset>)`,
`with_bindings(bindings)`, and `manual()`. There are no
`keyboard`/`simple_mouse_keyboard`/`blender_like_keyboard` helpers (zero callers);
those `OrbitCamPreset` variants are still reachable via `with_preset`. `FreeCam`
mirrors this in `free_cam/presets.rs` with `with_preset`/`with_bindings`/`manual`.
There is no `OrbitCamPresetBundle` type.

### Reconciliation and installation

`CameraInputModesPlugin` (`mode_reconciliation.rs`) runs reconciliation in `PreUpdate` inside
`CameraInputInternalSet::InputModes` (a sub-set of `CameraInputPhase::PreInput`).
`reconcile_input_modes` fires on `Changed<OrbitCamInputMode>`/`Changed<FreeCamInputMode>`
or when the installation record is missing, and for each camera:

1. clears `OrbitCamInput`;
2. lowers the enum into runtime state — `Preset`/`Bindings` insert
   `OrbitCamResolvedBindings` and remove the `OrbitCamManual` marker; `Manual` removes
   `OrbitCamResolvedBindings` and inserts `OrbitCamManual`;
3. despawns the previous BEI installation via the
   `Actions<OrbitCamInputContext>` relationship and records a new
   `OrbitCamInputInstallation`;
4. triggers the crate-private `CameraInputModeReplaced` hook, which routing/lifecycle
   cleanup consume.

If `preset.to_bindings()` fails, the camera falls back to the `Manual` runtime state
with a warning. `OrbitCamManual` and `OrbitCamResolvedBindings` are crate-private
runtime markers, not public API. The installation record uses
`OrbitCamInputInstallationOf(camera)` / `OrbitCamInputInstallation { entities }` as a
private ownership relationship (`despawn_related` walks each action subtree once).
Placeholder entities (`OrbitCamInputInstallationPlaceholder`) stand in until the
adapter attaches real BEI action/context entities.

### Reflected descriptor drafts (`reflect-input-modes` feature)

The default-on `reflect-input-modes` feature adds editor/keymap tooling: a mutable
reflected `OrbitCamInputModeDescriptor { mode: OrbitCamInputModeDraft }` where
`OrbitCamInputModeDraft` mirrors `OrbitCamInputMode` with `Bindings` holding an
`OrbitCamBindingsDescriptor`. `apply_input_mode_descriptors` runs before
reconciliation on `Changed<OrbitCamInputModeDescriptor>`:

- success inserts the validated `OrbitCamInputMode`, sets
  `OrbitCamInputModeApplyStatus { state: Applied, .. }`, triggers
  `OrbitCamInputModeApplied`;
- rejection leaves the previous `OrbitCamInputMode` in place, sets
  `state: Rejected` with the error string, triggers `OrbitCamInputModeRejected`
  (carrying the structured `BindingsError`), and warns.

The feature gates only these apply systems and the status/event types. The concrete
descriptor value types and the `OrbitCamInputMode` enum derive `Reflect` and are
available regardless of the feature. `OrbitCamInputModeApplyStatus` stores the error as
a display `String` so the status component stays reflectable without forcing `Reflect`
onto the error enum; it is point-in-time feedback, not a statement of the current
runtime mode.

## Bindings

`OrbitCamBindings` (`orbit_cam/input/bindings/`) is a validated data spec turned into BEI action
entities plus adapter policy. It has private fields and is built through
`OrbitCamBindings::builder()` (`OrbitCamBindingsBuilder`). The builder is behavior-first:
bindings are added to `.orbit(...)`, `.pan(...)`, and `.zoom(...)`, and the binding
value carries the device variant.

```rust
OrbitCamBindings::builder()
    .orbit(OrbitCamMouseDrag::new(MouseButton::Middle))
    .pan(OrbitCamMouseDrag::new(MouseButton::Middle).with_mod_keys(ModKeys::SHIFT))
    .zoom(OrbitCamMouseWheelZoom::default())
    .zoom(OrbitCamTrackpadScroll::default().with_mod_keys(ModKeys::CONTROL))
    .zoom(OrbitCamPinchZoom)
    .build()
```

Binding value types: `OrbitCamMouseDrag`, `OrbitCamTrackpadScroll`,
`OrbitCamMouseWheelZoom`, `OrbitCamPinchZoom` / `PinchGestureZoom`,
`OrbitCamButtonDragZoom` (+ `OrbitCamButtonDragZoomAxis`), `HeldBinding`,
`OrbitCamTouchBinding`, and `InputBinding` (wraps a direct BEI `Binding` plus
composite helpers like `bidirectional_keys`, `gamepad_axes_2d`,
`bidirectional_gamepad_buttons`). `CameraInputGamepadSelectionPolicy` is `Disabled` or
`Active`; selected-device ownership is not implemented. `ZoomDirection`
(`control_summary.rs`) is `In` / `Out`; zoom inversion is carried by `ZoomInversion`.
Empty binding sets are valid — there is no required wheel policy.

### Action typing and validation

Per-action binding sets are newtyped by semantic action
(`OrbitCamOrbitActionBindings`, `OrbitCamPanActionBindings`,
`OrbitCamZoomSmoothActionBindings`, `OrbitCamZoomCoarseActionBindings`) so an orbit
binding cannot be installed as a pan binding even though both output `Vec2`. The action
markers are sealed via `CameraSemanticAction: InputAction + Sealed`, with
`HeldCameraAction` / `ImpulseCameraAction` sub-traits; downstream crates cannot
implement them. Held bindings are one irreducible source-aware entry
(`HeldActionBindingEntry`) pairing motion and engagement; impulse bindings
(`ImpulseActionBindingEntry` with `BindingEngagement::Impulse`) carry no engagement half.

There are nine per-action newtypes (orbit: orbit/pan/zoom-smooth/zoom-coarse/home;
free: translate/look/roll/home). They forward their accessors through one
declarative macro `impl_binding_forwards!` (`input/bindings/binding_forwards.rs`,
reached via a `#[macro_use]` chain `lib.rs` → `input` → `bindings` →
`binding_forwards`, so `mod input;` precedes the camera mods in `lib.rs`); the macro
parameterizes accessor visibility (home newtypes expose narrower accessors), and the
home newtypes' `bindings()` / `to_vec()` stay hand-written. The shared backing
storage is `ImpulseActionBindingSet` / `ImpulseActionBindingEntry` (impulse) and
`HeldActionBindingSet` / `HeldActionBindingEntry` (held), all four in
`input/bindings/action_set.rs`; the impulse pair was renamed from
`ActionBindingSet` / `ActionBindingEntry` to parallel the held names, and
`ActionBindingSet` / `ActionBindingEntry` / `HeldActionBindingEntry` are no longer
publicly re-exported (kept crate-internal — an unreleased leak, zero external
callers). Installed resolved bindings use one generic
`CameraInstalledBindings<K>(K::Bindings)` (`mode_reconciliation.rs`), replacing the
former per-camera `OrbitCamInstalledBindings` / `FreeCamInstalledBindings`.

Every construction path — builder, preset, descriptor, reflection, dynamic keymap —
funnels through the shared `validate_bindings`. Errors are `BindingsError`
(e.g. `InvalidScale`, `InvalidDeadZone`, held-motion/engagement and source-mismatch
variants). `build`/`try_from` return `Result` because binding errors are app/keymap
configuration errors, not library bugs. Reflected editing of runtime bindings is
opaque; editing goes through `OrbitCamBindingsDescriptor` and re-validates before
inserting `OrbitCamBindings`.

### Presets

`OrbitCamPreset` (`orbit_cam/input/bindings/preset/enum_preset.rs`) is a `#[non_exhaustive]` 6-variant
enum: `SimpleMouse` (default), `BlenderLike`, `Keyboard`, `SimpleMouseKeyboard`,
`BlenderLikeKeyboard`, `Gamepad`. `OrbitCamPreset::to_bindings()` returns
`Result<OrbitCamBindings, BindingsError>`, delegating each variant to a concrete
config struct's public `build()`. The configs implement a crate-private sealed
`OrbitCamPresetConfig` trait (`preset/config.rs`) with
`build(self) -> Result<OrbitCamBindings, _>`. Reconciliation always operates on an
`OrbitCamBindings` value internally, so preset and custom modes share validation,
installation, source attribution, and adapter policy. Preset details, config structs,
and slow-mode wiring are covered in [`orbit-cam-preset-api.md`](orbit-cam-preset-api.md).

### Slow mode

Slow (precise) mode scales held input. `CameraInputScalePolicy { normal, slow }` and
`CameraSlowMode { toggle_key, mod_keys, scale }` are camera-neutral types in the shared `input/bindings/descriptor.rs`;
they reach the runtime spec as `OrbitCamBindings.slow_mode: Option<CameraSlowMode>`.
The default toggle is `KeyCode::KeyS` + `ModKeys::ALT` (slow scale `0.05`). Per-camera
toggle state is `CameraSlowModeLatches` (`routing/latches.rs`), flipped on the toggle
key's press edge. Scaling is applied once, in `orbit_cam/input/adapter/resolve.rs` via `AdapterScale`
(`AdapterScale::from_bindings(..., is_slow_mode_active(...))`), across all scaled
sources — there is no double application. The resolver also writes per-kind speed
(`set_orbit_speed` / `set_pan_speed` / `set_zoom_speed`) so the control summary and
`OrbitCamInteractionSpeedChanged` can report Normal vs Slow.

## Semantic actions

Public actions (`actions.rs`) name user intent, not devices:

```rust
#[derive(InputAction)] #[action_output(Vec2)] pub struct OrbitCamOrbitAction;
#[derive(InputAction)] #[action_output(Vec2)] pub struct OrbitCamPanAction;
#[derive(InputAction)] #[action_output(f32)]  pub struct OrbitCamZoomCoarseAction;
#[derive(InputAction)] #[action_output(f32)]  pub struct OrbitCamZoomSmoothAction;
```

Coarse zoom is step-like (line wheel, key/button); smooth zoom is continuous (pixel
scroll, pinch, drag). Private engagement actions track held interaction phase
separately from motion: a user can hold the orbit control still — zero delta but the
interaction is active. The controller needs the engagement edge to keep the orbit-drag
latch (including upside-down yaw). These engagement and adapter source actions stay
private; UI observes lifecycle events and `OrbitCamInteractionState` instead.

## Camera-kind trait family

The per-kind engine registers through a **sealed** trait chain — `CameraKind`,
`CameraInputKind`, `CameraInputModeKind`, `IntentChannels`, `HeldCameraAction` —
each sealed by a private per-file `mod sealed { pub trait Sealed {} }` with the
`impl sealed::Sealed for …` lines in the same file. Sealing (not visibility
demotion, which E0445 blocks) is the mechanism. The only implementers are
`OrbitCamKind` and `FreeCamKind` (plus `OrbitCamChannels` / `FreeCamChannels` for
`IntentChannels`). Two vacuous single-value associated types were dropped:
`CameraActionResolutionKind::{Manual, InstalledBindings}` (`Manual` hardcoded to
`CameraManual<K>`, installed bindings unified into `CameraInstalledBindings<K>`) and
`CameraInputModeKind::Error` (replaced by the concrete `BindingsError`).
`OrbitCamChannels` has a crate-root re-export for parity with the already-public
`FreeCamChannels`.

## Input gain

Input gain is a deliberate **two-layer** design: source-level *gain* scales raw
input (like microphone gain), while separately-named behavior *scale* modifiers tune
what the input drives — keep the two distinguishable (gain = source-side multiplier;
scale = behavior-side). Both cameras carry a per-action gain set: `OrbitCamInputGain`
(`orbit_cam/input/bindings/input_gain.rs`) and `FreeCamInputGain { translate, look,
roll }` (`free_cam/input/bindings/input_gain.rs`), each with const setters,
`uniform`, and `validate`.

The three source-gain traits — `MouseInputGain`, `SmoothScrollInputGain`,
`GamepadInputGain` (each one `type Gain` + one setter) — live in the camera-neutral
`input/source_input_gain.rs` because both cameras implement them. They stay
**unsealed** (deliberate: fluent-setter vocabulary with a genuinely varying `Gain`
type, nothing bounds on them — unlike the sealed kind chain). OrbitCam's per-binding
gain uses a bespoke `OrbitCamBindingWithInputGain` wrapper (`binding_kinds.rs`);
FreeCam's gain uses the existing shared `InputBindingDescriptor.scale` path,
installed as BEI `Scale` by shared `input/install.rs`, with no new binding-kind
type.

**Gotchas (durable):** keyboard roll gain must be applied motion-only via
`HeldBinding::same(InputBinding::bidirectional_keys(...)).with_input_gain(...)`, never
baked into the raw `InputBinding` — `HeldBinding::same` copies the raw binding to
*both* the motion and engagement descriptors, so a low roll gain baked at the
`InputBinding` level could shrink the engagement signal below actuation and stop roll
engaging at all. Relatedly, `InputBinding::with_input_gain` (bakes gain into the
binding, landing on every descriptor it is copied to, including engagement) and
`HeldBinding::with_input_gain` (touches only the motion descriptor) share a name but
differ in reach — for any binding that doubles as its own engagement source, only the
`HeldBinding`-level call is correct.

## Adapter

The adapter (`orbit_cam/input/adapter/`) is a private input-policy shim for source detail BEI does not
carry richly enough: `MouseWheel::unit` line/pixel split, `PinchGesture`, `Touches`
arity, and smooth-scroll routing. `install.rs` attaches BEI actions and private adapter
state to the installation record; `inject.rs` injects adapter-backed values (via
`ActionMock` where useful) before `EnhancedInputSystems::Update`; `resolve.rs` reads BEI
action state plus adapter contributions after `EnhancedInputSystems::Apply` and writes
`OrbitCamInput`, applying slow-mode scaling and per-kind source attribution. Adapter
injection and resolution consult the same route/gating snapshot from `PreInput`; mock
state is cleared for gated cameras. Camera actions are non-consuming so app-owned BEI
contexts still observe shared buttons/motion/wheel/keyboard/gamepad input.

Line scroll → coarse zoom; pixel scroll → smooth input flagged `SMOOTH_SCROLL` (not
`TRACKPAD`, since Bevy does not guarantee physical device identity). Pinch is suppressed
on the routed camera while a configured non-pinch camera modifier or held camera action
is active (Blender-like Shift-pan / Control-zoom modifiers included), scoped to the
routed camera's resolved modifier state.

## Camera intent and manual input

`OrbitCamInput` (`orbit_cam/intent.rs`) is the per-frame semantic snapshot: per-kind movement
deltas plus per-kind active source sets, with read-only public accessors
(`orbit_delta()`, `pan_delta()`, `zoom_coarse_delta()`, `zoom_smooth_delta()`).
Per-kind source sets let simultaneous interactions (mouse orbit + wheel zoom) coexist.
Mutation is crate-private (`*_with_sources` helpers); the only public write path is the
manual writer. Typed deltas name units: `OrbitDelta`, `PanDelta`, `CoarseZoomDelta`,
`SmoothZoomDelta`, each with `screen_pixels`/`amount` constructors and `From` impls.
`OrbitCamInput` holds no cross-frame held-phase enum; held/ending phase is derived and
stored by `OrbitCamInteractionState` and the lifecycle path.

Manual mode (`manual.rs`) writes through `OrbitCamManualInputWriter`, a system param
that yields a writer only for `OrbitCamManual` cameras:

```rust
fn manual_camera_input(mut writer: OrbitCamManualInputWriter) {
    if let Ok(mut cam) = writer.get_mut(camera, ManualInputSource::observed_keyboard()) {
        cam.orbit((-4.0, 0.0)).mark_pan_active();
    }
}
```

`get_mut(camera, ManualInputSource)` returns `OrbitCamManualInput`, whose builder
methods (`orbit`, `pan`, `zoom_coarse`, `zoom_smooth`, `mark_orbit_active`,
`mark_pan_active`, `mark_zoom_active`, `clear`) record intent and chain. The FreeCam
manual writer exposes the matching `translate`, `look`, `roll`, and `mark_*_active`
verbs. Source provenance is fixed by the `ManualInputSource` passed to `get_mut`.
`ManualInputSource` always carries `InteractionSources::MANUAL`; `manual()`,
`with_sources(..)`, and observed-device constructors (`observed_keyboard`,
`observed_gamepad`, ...) add device flags without dropping `MANUAL`. It does not derive
`Reflect` and has no raw-bit constructor, so the `MANUAL` bit cannot be lost. Manual
writes run in `CameraInputPhase::WriteManual` and still respect `CameraInputDisabled`,
egui focus, animation-ignore, and other blockers.

Screen-pixel manual deltas need logical surface metrics. Metrics are derived once per
frame during routing and cached on the resolved route; an explicit
`CameraInputSurfaceMetrics` component overrides only the fields it provides (for
render-to-texture / editor panels). If metrics cannot be derived, screen-pixel input is
dropped, a per-camera one-time `error!` is logged, and `CameraInputMetricsMissing` is
emitted.

## Sources

`InteractionSources` (`sources.rs`) is the only public source-set type, backed by
private `bitflags`. Public constants: `MOUSE`, `KEYBOARD`, `WHEEL`, `SMOOTH_SCROLL`,
`PINCH`, `TOUCH`, `GAMEPAD`, `MANUAL`, plus `NONE`. It exposes `is_empty`, `contains`,
`intersects`, `union`, `difference`, `BitOr`/`BitOrAssign`, and `const` composition; no
public raw-bit constructor or `from_bits`. There is no `CUSTOM` flag — custom is an
input mode, not a source (custom keyboard bindings report `KEYBOARD`, etc.).
`SMOOTH_SCROLL` means Bevy reported pixel scroll, not that the device was a trackpad.

## Interaction events and state

Lifecycle events (`events.rs`) are `EntityEvent`s targeting the camera, carrying
`OrbitCamInteractionKind` (`Orbit`, `Pan`, `Zoom`; `#[non_exhaustive]`) and source sets:
`OrbitCamInteractionStarted`, `OrbitCamInteractionEnded`,
`OrbitCamInteractionSourcesChanged` (with `added_sources()` / `removed_sources()`),
`OrbitCamInteractionSpeedChanged` (Normal↔Slow), and `CameraInputMetricsMissing` (which
carries `CameraInputMetricKind`, not an interaction kind).

Events are interaction-level, not per-source. `OrbitCamInteractionState`
(`interaction_state.rs`) is the read-only per-camera tracker with independent
`orbit_sources()` / `pan_sources()` / `zoom_sources()` accessors; its fields are
internal and mutated only by the finalizer:

```text
previous empty, current non-empty -> Started
previous non-empty, current empty -> Ended
active set changes mid-interaction -> SourcesChanged
```

Held sources start when held state begins and end on release (a zero-delta but still
engaged frame stays active; a release-frame ends even with zero motion). Impulse
sources (line/pixel wheel, pinch, gesture deltas) start and end in the same frame and
do not keep the active set alive into the next frame. Finalization derives all events
from finalized per-kind source deltas in `OrbitCamInput`. App UI should read
`OrbitCamInteractionState` for "is active now?" and use events for edge reactions.

## Control summary

`describe_orbit_cam_controls(&OrbitCamInputMode) -> OrbitCamControlSummary`
(`control_summary.rs`) builds the display model for guidance panels. Label derivation:

- `Preset(p)` → `mode_label = "Preset"`, `mode_value = p.name()`;
- `Bindings(_)` → `mode_label = "Input"`, `mode_value = "custom bindings"`;
- `Manual` → `mode_label = "Input"`, `mode_value = "manual input"`.

`OrbitCamControlRow` carries the interaction `kind`, label, `InteractionSources`,
`ControlSpeed` (`Normal` / `Slow`), and an optional `ZoomDirection` so a panel can
highlight only the engaged zoom direction. `OrbitCamPreset::name()` returns
`kind().name()` — the setting-insensitive `OrbitCamPresetKind` string
(`"SimpleMouse"`, `"BlenderLike"`, `"Gamepad"`, ...) — so a tuned preset still
labels by its kind rather than degrading to custom bindings.

## Routing and ownership

`CameraInputRoutingConfig` (`routing/config.rs`) is a public resource:
`CameraInputRouting::{CursorHitTest, Explicit}`, `explicit_camera: Option<Entity>`, and
`NoPositionFallback::{NoInput, OnlyEligibleCamera}` (default `NoInput`). Config
mutations take effect at the next `PreInput` route phase; later mutations do not
retroactively re-route the current frame. `CameraInputRouting::Explicit` chooses which
camera receives input and is distinct from `Manual` mode (which has the app write
`OrbitCamInput` itself).

The internal `ResolvedCameraInputRoute` (`routing/mod.rs`) is rewritten every frame
and is the only route state that gating, injection, and finalization consult. It carries
the routed camera, per-source held latches, per-camera surface metrics, and the blocker
snapshot. Held sources (mouse drags, keyboard) latch their owning camera until release,
so a drag stays attached to camera A even as the cursor crosses into camera B. Impulse
sources route per event by event window + cursor position. Latch ownership is
mouse-like and keyboard only; gamepad and touch report source attribution but do not
own latches (selected-device / touch-owner policy is future work). Routing precedence
for held no-position sources: matching latch → explicit route → unambiguous cursor-hit
camera → `NoPositionFallback`. Stale latches (despawn, `OrbitCam` removal, mode
replacement, disable, window close, focus loss, gamepad disconnect, or missing held
state) are cleared and rerouted in the same route phase. Ambiguous global gestures are
dropped with a rate-limited `debug!`.

## Disabling and blockers

`CameraInputDisabled` (`disabled.rs`) is the public app-level pause marker; it suppresses
input without changing the selected mode. Transient blockers stay internal: animation
ignore, egui pointer/keyboard focus, inactive camera, unavailable owner. They are
computed once in `PreInput` into `CameraInputBlockers` (the single source of truth)
and consumed by context gating, adapter injection, resolution, and finalization. Two
gates apply: pre-input gating deactivates/resets BEI state for blocked contexts before
`EnhancedInputSystems::Update` so held state and condition timers do not advance
invisibly; finalization clears blocked per-frame intent and emits an `Ended` event
before suppressing further input. `BlockOnEguiFocus` feeds the UI-focus blocker using
`EguiWantsFocus::prev || curr` (no one-frame leak) and respects
`EguiFocusIncludesHover`.

## Scheduling

The public scheduling surface is `CameraInputPhase::{PreInput, WriteManual, Finalize}`
(`system_sets.rs`), chained in `PreUpdate`. Internal finer phases (e.g.
`CameraInputInternalSet::InputModes`) stay `pub(crate)`. Input resolution lives in
`PreUpdate`; the controller stays in `PostUpdate`.

```text
PreUpdate (PreInput, exclusive structural boundary):
  apply changed descriptors -> reconcile input modes + replace installation
  -> route active camera + compute blockers -> gate contexts
  -> inject adapter values -> EnhancedInputSystems::Update
  -> (Finalize) resolve actions + adapter into OrbitCamInput
WriteManual:  user systems write OrbitCamInput for Manual cameras
Finalize:     recover latches, clear blocked/stale input, emit lifecycle events,
              update interaction state
Update:       process_orbit_camera_move_list reads finalized OrbitCamInput
              (animation interrupt: Ignore clears input, Cancel/Complete handle animation)
PostUpdate:   OrbitCam controller reads OrbitCamInput -> updates targets -> clears input
              -> transform propagation -> camera updates
```

`PreInput` is the structural boundary for descriptor apply, mode reconciliation, removal
cleanup, routing, gating, and command-buffered adapter setup: command-buffered changes
needed by BEI must be visible before `EnhancedInputSystems::Update`, so this runs via
exclusive-world access rather than relying on a nearby deferred flush. ECS scheduling
tests guard the ordering against the pinned Bevy / `bevy_enhanced_input` versions.

## Animation interaction

`AnimationConflictPolicy` and `CameraInputInterruptBehavior` are separate axes.
Finalized `OrbitCamInput` is the user-input interrupt signal: `Cancel` cancels the
animation and keeps input; `Complete` finishes the animation and clears input for the
frame; `Ignore` treats the active animation as an input blocker in `Finalize` (animation
continues, input is not observable). Animation interruption checks authoritative state
(`CameraMoveList` plus interrupt policy) directly. Programmatic camera operations
(`ZoomToFit`, `PlayAnimation`, etc.) mutate camera state/targets/animation queues; they
never write `OrbitCamInput` or emit input lifecycle events.

## Dependencies

`bevy_enhanced_input` and `bitflags` are direct `bevy_lagrange` dependencies declared
through workspace entries. `LagrangePlugin` installs `EnhancedInputPlugin` (guarded
against duplicate setup), registers `OrbitCamInputContext` via
`add_input_context`, and initializes the Bevy resources its systems read directly
(`Touches`, `PinchGesture`); event production still comes from Bevy input plugins. The
`reflect-input-modes` feature is default-on. `bevy_egui` and `fit_overlay` are optional.

## Future work

- Selected-gamepad ownership and per-touch ownership latches (current code reports
  gamepad/touch source attribution but does not latch them).
- Roll: Bevy exposes `RotationGesture` and the touch tracker already computes two-finger
  rotation, but the controller does not use it. Adding roll would extend
  `OrbitCamInteractionKind` (kept `#[non_exhaustive]` for this reason), the input
  snapshot, interaction state, presets, and the manual writer.
