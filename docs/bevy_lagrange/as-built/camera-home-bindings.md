# Camera home via preset bindings

## What it is

`bevy_lagrange` presets no longer bind a home/reset input by default. A freshly
constructed `OrbitCamPreset` or `FreeCamPreset` carries no home binding; nothing
returns the camera to its stored pose unless a caller asks for it. The
`fairy_dust` example harness opts every installed preset into home through
`.with_camera_home()`: it fills empty presets with **H** on keyboard-family
presets and **H + Select** on gamepad presets, and it owns the meaning of the
H key (reserving it so no example can double-bind it). Those filled inputs drive
Lagrange's stored-pose home glide through `bevy_enhanced_input` (BEI); `fairy_dust`
binds no separate home action of its own. Window-resize refits re-capture the
stored home pose, so after a resize H returns to the post-resize framing rather
than a stale pre-resize one.

The problem this solves: home used to be a preset default, so every consumer of a
preset inherited an H (or Select) binding it never asked for, and the example
harness carried a *second*, private H→refit action on top of it — two systems
fighting over one key. Making presets home-less by default hands the H-key policy
to whoever installs the camera. `fairy_dust` is that owner for examples; a library
user binds whatever they want, or nothing.

## How it works

### Opt-in preset model

Presets are plain `Copy`/`Clone` data. Each of the six preset payloads
(`FreeCamKeyboardMousePreset`, `FreeCamGamepadPreset`, `OrbitCamSimpleMousePreset`,
`OrbitCamKeyboardPreset`, `OrbitCamGamepadPreset`, `OrbitCamBlenderLikePreset`)
stores its home input as a **two-slot array**:

```rust
home: [Option<Binding>; 2],
```

The array keeps the payloads `Copy`. `with_home`/`home` fill the first empty slot;
a third call replaces the second (`crates/bevy_lagrange/src/free_cam/input/bindings/preset.rs`,
`crates/bevy_lagrange/src/orbit_cam/input/bindings/preset/*`):

```rust
pub fn with_home(mut self, home: impl Into<Binding>) -> Self {
    let home = Some(home.into());
    match &mut self.home {
        [first @ None, _] => *first = home,
        [_, second] => *second = home,
    }
    self
}

pub const fn has_home(&self) -> bool { matches!(self.home, [Some(_), _] | [_, Some(_)]) }
```

Bindings are built at install, never stored live. `to_bindings()`/`add_to`/`build`
feed each occupied slot to the bindings-level `.home(...)` appender in call order:

```rust
self.home
    .into_iter()
    .flatten()
    .fold(builder, OrbitCamBindingsBuilder::home)
```

Setter naming differs by kind: FreeCam presets use the `with_*` family, OrbitCam
presets use bare names. `OrbitCamKeyboardPreset` is a fielded struct
`{ home: [Option<Binding>; 2] }` (it derives `PartialEq` but not `Eq`, because BEI
`Binding` is `PartialEq` only).

**Enum wrappers** dispatch home to the underlying variant so an installer holding
an enum never matches variants itself:

- `OrbitCamPreset::home(impl Into<Binding>)` / `has_home()`
  (`orbit_cam/input/bindings/preset/enum_preset.rs`)
- `FreeCamPreset::with_home(impl Into<Binding>)` / `has_home()`
  (`free_cam/input/bindings/preset.rs`)

**Composites route home to their keyboard child.**
`OrbitCamSimpleMouseKeyboardPreset` and `OrbitCamBlenderLikeKeyboardPreset` hold a
`pointer` child and a `keyboard` child; their `home` setter writes only the
keyboard child, and `has_home` is the OR of both children:

```rust
pub fn home(mut self, home: impl Into<Binding>) -> Self {
    self.keyboard = self.keyboard.home(home);
    self
}
pub const fn has_home(&self) -> bool { self.pointer.has_home() || self.keyboard.has_home() }
```

`OrbitCamGamepadPresetBuilder::home` routes through the preset setter, so the
fluent tuning builder and the bare preset share the two-slot behavior.

### Shared `CameraHomeKind` capability

The home/reset mechanics are generic over camera kind
(`crates/bevy_lagrange/src/camera_home.rs`):

```rust
pub trait CameraHomeKind: CameraInputKind {
    type HomePose: Component + Copy + Reflect;
    type InteractionStarted: EntityEvent;
    fn capture_home(camera: &Self::Camera) -> Self::HomePose;
    fn apply_home(camera: &mut Self::Camera, home: &Self::HomePose);
}
```

Each kind realizes `HomePose` as its own registered component holding the eased
channels that kind restores — and the two differ:

- `OrbitCamHomePose { orbit: OrbitAngles, pan: Focus, zoom: Radius }` (`orbit_cam/mod.rs`)
- `FreeCamHomePose { position: Position, look: LookAngles, roll: Roll }` (`free_cam/mod.rs`)

Both derive `Component + Copy + Reflect` (`#[reflect(Component)]`) and expose a
`from_current(&Camera)` const constructor reading each channel's `current()`.
`OrbitCamKind` and `FreeCamKind` both `impl CameraHomeKind`.

`add_home_systems::<K>` — called from each kind's `CameraKind::add_camera_kind_systems`
checklist (`camera_kind.rs`) — registers two observers per kind:

- `on_animation_settled::<K>` (`On<AnimationEnd>`): if the animation was
  `Cancelled`, return; otherwise, if the camera carries `CameraHomePending`,
  `insert(K::capture_home(camera))` and `remove::<CameraHomePending>()`. This is
  the settle-on-first-animation path that upgrades a provisional home to the
  settled pose.
- `on_interaction_locks_home::<K>` (`On<K::InteractionStarted>`): if the camera is
  pending, remove `CameraHomePending` **without recapturing** — the lock-on-first-
  interaction path. The stored home stays whatever it already was.

`CameraHomePending` (`camera_home.rs`, re-exported at `lib.rs:35`) is a transient,
unregistered marker meaning "the captured home is still provisional." Controller
init inserts a `…HomePose` and this marker only when the camera has no
app-provided pose, and the marker only when there is no input yet
(`orbit_cam/controller.rs:325-331`, `free_cam/controller.rs:181`):

```rust
if !has_home {
    commands.entity(entity).insert(OrbitCamHomePose::from_current(&orbit_cam));
    if !input.has_input() {
        commands.entity(entity).insert(CameraHomePending);
    }
}
```

`capture_home` reads the current settled pose. `apply_home` is a **retarget only** —
it calls `set_target` on the camera's eased channels (idempotent each frame while
the button is held, holds on release). It creates **no Animation entity, so there
is no `AnimationEnd`**. That absence is load-bearing downstream (see `AtHome` and
the timed UI holds).

The trait's associated `InteractionStarted` type is the **per-kind interaction-lock
event** — `OrbitCamInteractionStarted` / `FreeCamInteractionStarted`, which carry
kind enums (`Orbit`/`Pan`/`Zoom`, `Translate`/`Look`/`Roll`). It is **not** the home
notification. Do not confuse it with `CameraHomed` below.

### Home reset trigger events

Two per-kind trigger `EntityEvent`s are the single home-snap code path
(`crates/bevy_lagrange/src/input/events.rs`, re-exported at the crate root):

```rust
#[derive(Clone, Copy, Debug, EntityEvent, Reflect)]
#[reflect(Event, FromReflect)]
pub struct ResetOrbitCamToHome { #[event_target] pub camera: Entity }

#[derive(Clone, Copy, Debug, EntityEvent, Reflect)]
#[reflect(Event, FromReflect)]
pub struct ResetFreeCamToHome  { #[event_target] pub camera: Entity }
```

The keybind path and external callers both `world.trigger` one of these. Because
they are concrete `EntityEvent`s deriving `Reflect`, Bevy auto-registers them, so
they are app- and BRP-triggerable. One observer per kind —
`on_reset_orbit_cam_to_home` / `on_reset_free_cam_to_home` (`camera_home.rs`) —
performs the snap that used to be inline in the two apply fns: it calls
`K::apply_home`, emits the `CameraHomed` notification, and removes the transient
`CameraHomeResetSources` attribution component:

```rust
fn on_reset_orbit_cam_to_home(event, mut cameras, mut commands) {
    let camera = event.camera;
    if let Ok((mut orbit_cam, home, reset_sources)) = cameras.get_mut(camera) {
        OrbitCamKind::apply_home(&mut orbit_cam, home);
        commands.trigger(CameraHomed {
            camera,
            sources: reset_sources.map_or(InteractionSources::NONE, |s| s.0),
        });
        commands.entity(camera).remove::<CameraHomeResetSources>();
    }
}
```

The keybind side stays in the two apply fns
(`orbit_cam/input/adapter/resolve.rs` `apply_orbit_cam_home`,
`free_cam/input/adapter.rs` `apply_free_cam_home`). Each detects the home action's
**rising edge** — tracking `HomeActionState { Active, Inactive }` in the per-camera
action-entity struct (`OrbitCamInputActionEntities` / `FreeCamInputActionEntities`,
fields `home`, `home_sources`, `home_state`; the `home` field holds a bare-bool BEI
action entity `OrbitCamHomeAction` / `FreeCamHomeAction`, `input/actions.rs`) —
then attributes the live device sources, inserts them as `CameraHomeResetSources`,
and triggers the reset event. It never applies the snap or emits `CameraHomed`
itself:

```rust
let active = bool_action_active(actions.home, &home_actions, &states);
let next = HomeActionState::from(active);
if actions.home_state != next {
    if matches!(next, HomeActionState::Active) {
        let sources = input::attributed_sources(
            bindings.0.enabled_home_entries(), &inputs, actions.home_sources);
        commands.entity(camera).insert(CameraHomeResetSources(sources));
        commands.trigger(ResetOrbitCamToHome { camera });
    }
    actions.home_state = next;
}
```

Because the home action is now an **impulse** action (see *Home action storage*
below), the edge fires **once** on the press's rising edge, not every held frame.

### The `CameraHomed` notification event

One `EntityEvent` covers every camera kind
(`crates/bevy_lagrange/src/input/events.rs`):

```rust
#[derive(Clone, Copy, Debug, EntityEvent, Reflect)]
#[reflect(Event, FromReflect)]
pub struct CameraHomed {
    #[event_target]
    pub camera:  Entity,
    pub sources: InteractionSources,
}
```

The reset observer emits it on every home snap, carrying the attributed `sources`
from `CameraHomeResetSources` — or `InteractionSources::NONE` for an
externally-triggered reset with no physical device behind it. For the keybind path
this still lands on the home action's rising edge; external `world.trigger` callers
get it whenever they fire a reset event.

Consumers are single observers (each replaced a former OrbitCam/FreeCam pair):

- `on_camera_homed` (`fairy_dust/src/camera_home.rs`) — re-arms `AtHome` and starts
  the title-bar chip flash.
- `refresh_on_camera_homed` (`fairy_dust/src/camera_control_panel/mod.rs`) — pulses
  the control-panel home row.
- `log_camera_homed` (`crates/bevy_lagrange/examples/showcase/event_log.rs`) — logs
  the showcase entry.

`CameraHomed` is a **notification**, not a trigger: it derives `Reflect` but is
**not** `register_type`d anywhere in the workspace, so it is absent from the type
registry and **cannot be triggered over BRP**. To home a camera externally,
trigger `ResetOrbitCamToHome` / `ResetFreeCamToHome` (which are BRP-triggerable);
the observer then emits `CameraHomed`. `CameraHomed` is re-exported at the crate
root.

### Home action storage and impulse routing

Both home actions are `ImpulseCameraAction` (`FreeCamHomeAction`,
`OrbitCamHomeAction` are in the `impl_camera_action!` list, `input/actions.rs`) and
route through the shared impulse conversion path (`action_descriptor_to_entry`,
`input/bindings/validate.rs`). `ImpulseCameraAction` has three implementers:
`OrbitCamZoomCoarseAction`, `OrbitCamHomeAction`, `FreeCamHomeAction`.

Per-action home storage on `OrbitCamBindings` / `FreeCamBindings` is the newtype
`OrbitCamHomeActionBindings` / `FreeCamHomeActionBindings` (each wrapping the shared
`ImpulseActionBindingSet`, `*/input/bindings/action_set.rs`), replacing the former
`home: Vec<Binding>`. The `.home()` accessor returns `&…HomeActionBindings` rather
than `&[Binding]`; the newtype mirrors the slice API (`bindings()`, `to_vec()`,
`is_empty`, `len`) so external callers (fairy_dust) and preset tests compile
unchanged. Two-slot home (keyboard **H** + gamepad **Select**) is preserved; the
apply fns read `bindings.0.enabled_home_entries()` for live attribution.

### Device attribution

`sources` on `CameraHomed` reports the device that actually fired the edge, not a
static guess. The machinery is crate-side
(`crates/bevy_lagrange/src/input/bindings/source_binding.rs`) because BEI 0.26
exposes no per-binding evaluated state:

```rust
pub struct LiveInputs<'a> {
    pub keyboard: Option<&'a ButtonInput<KeyCode>>,
    pub mouse:    Option<&'a ButtonInput<MouseButton>>,
    pub gamepads: &'a [&'a Gamepad],
}

pub trait SourceBinding {
    fn sources(&self) -> InteractionSources;          // static device family
    fn is_active(&self, inputs: &LiveInputs<'_>) -> bool; // live physical state
}

pub fn attributed_sources<'a, T: SourceBinding + 'a>(
    entries: impl IntoIterator<Item = &'a T>,
    inputs: &LiveInputs<'_>,
    fallback: InteractionSources,
) -> InteractionSources
```

`attributed_sources` unions the sources of every home binding whose physical input
is live right now; if none can be attributed it returns the fallback. The fallback
is `actions.home_sources`, the precomputed **static union** of every home binding's
source family (`home_sources(bindings.home())` at install). `impl SourceBinding for
Binding` reads `ButtonInput`/`Gamepad` directly for the live check.

### The fairy_dust ECS fill system

`fill_camera_home_presets` (`crates/fairy_dust/src/camera_home.rs`) is the single
place presets gain home bindings under `.with_camera_home()`:

```rust
pub(crate) fn fill_camera_home_presets(
    config: Option<Res<CameraHomeConfig>>,
    mut orbit_modes: Query<&mut OrbitCamInputMode, Changed<OrbitCamInputMode>>,
    mut free_modes: Query<&mut FreeCamInputMode, Changed<FreeCamInputMode>>,
) {
    if config.is_none() { return; }
    for mut mode in &mut orbit_modes {
        if let Some(filled_mode) = fill_orbit_cam_home(&mode) { *mode = filled_mode; }
    }
    for mut mode in &mut free_modes {
        if let Some(filled_mode) = fill_free_cam_home(&mode) { *mode = filled_mode; }
    }
}
```

- **Gate:** the `CameraHomeConfig` resource exists only when `.with_camera_home()`
  ran; absent it, the system returns immediately and presets stay home-less.
- **Match:** only `Preset` variants — `Manual` and `Custom` modes are untouched.
- **Fill rule:** only when `!preset.has_home()`. The helpers inspect the mode by `&`
  and return `Option<…InputMode>`, so the `Mut` deref (and its change-tick bump)
  only happens when a fill actually occurs — no ping-pong on the no-op path:

  ```rust
  fn fill_orbit_cam_home(mode: &OrbitCamInputMode) -> Option<OrbitCamInputMode> {
      let OrbitCamInputMode::Preset(preset) = mode else { return None; };
      (!preset.has_home())
          .then(|| OrbitCamInputMode::with_preset(orbit_cam_home_preset(preset.clone())))
  }

  fn orbit_cam_home_preset(preset: OrbitCamPreset) -> OrbitCamPreset {
      match preset.kind() {
          OrbitCamPresetKind::Gamepad => preset.home(HOME_KEY).home(HOME_BUTTON),
          _ => preset.home(HOME_KEY),
      }
  }
  ```

  Keyboard-family presets get `HOME_KEY` (H); gamepad presets get **both**
  `HOME_KEY` and `HOME_BUTTON` (H then Select). `free_cam_home_preset` mirrors this
  with `with_home`.
- **Schedule:** registered in `PreUpdate` `.before(CameraInputPhase::PreInput)`. The
  mode reconcile that installs bindings runs *inside* `PreInput`, so the fill
  precedes the first install — one install, no home-less first frame, no double
  install. On the next change-detection pass `has_home()` is true and the system
  writes nothing (it converges).
- **Reach:** because the fill matches on the mode component with a `Changed` filter,
  it covers every preset-mode camera regardless of spawn path — builder baked
  bundles, the Shift+C preset cycle's `switch_to_*` writes, and raw
  `…InputMode::with_preset` spawns in examples all land as mode-component writes the
  system sees. No fill code lives in the builder or the cycle.

`HOME_KEY` and `HOME_BUTTON` are `pub(crate)` in `fairy_dust/src/constants.rs`:
`HOME_KEY: KeyCode = KeyCode::KeyH`, `HOME_BUTTON: GamepadButton = GamepadButton::Select`.

`.with_camera_home()` itself (`crates/fairy_dust/src/builder/sprinkle.rs`,
`crates/fairy_dust/src/builder/camera_home.rs`) inserts `CameraHomeConfig`, reserves
H via `shortcuts::reserve_key(app, HOME_KEY, HOME_CONTROL)`, registers the fill
system and the fit/refit/observer systems in `camera_home::install`, and (unless
`without_title_bar_control()`) prepends the `H Home` title chip.

### `AtHome` and resize refit

`AtHome { Yes, No }` (default `Yes`, `fairy_dust/src/camera_home.rs`) records
whether the camera is still parked at home, so a window resize only re-frames a
camera the user hasn't since moved:

- `on_camera_homed` flips it `Yes` (and pulses the chip). This is the reason the
  rearm rides `CameraHomed` and not animation completion: the home glide is
  `set_target`, which yields no `AnimationEnd`.
- `on_orbit_user_interaction_started` / `on_free_user_interaction_started` flip it
  `No` on any non-empty interaction. A later interaction after a home press is the
  correct sequence even if it interrupts a still-traveling glide.
- `on_home_animation_end` still flips `Yes` on a *completed home fit* (the
  startup-snap and resize-refit `AnimateToFit`s that frame the home cube do produce
  animation events).

`refit_on_window_resized` re-captures the post-resize pose. On a resize, while
`AtHome::Yes` with a single filter-matched camera and meshes ready, it inserts
`CameraHomePending` **before** triggering the zero-duration fit:

```rust
commands.entity(camera).insert(CameraHomePending);
commands.trigger(home_fit(camera, home.0, &config, Duration::ZERO));
```

The marker-before-trigger order guarantees the fit's completion is seen with the
marker present, so `on_animation_settled` recaptures the freshly-framed pose into
`…HomePose`. (Startup `snap_home_on_ready` needs no marker insert — controller init
already placed it. A saved restart pose triggers `RestoreWindowAnimation` instead
of the startup snap.)

### UI surfaces

- **Control-summary home row** (`crates/bevy_lagrange/src/input/control_summary.rs`):
  `orbit_home_bindings` / `home_binding_rows` iterate the `bindings.home()` newtype
  (via its slice-mirroring API) and emit one `CameraControlBinding` per bound slot —
  the row still reads bindings, not events. An empty set yields no row; a
  two-slot gamepad preset yields two rows (an H row and a Select row).
  `home_binding_label` renders `Keyboard` and `GamepadButton` bindings and returns
  `None` for axes/motion.
- **Panel pulse** (`camera_control_panel/mod.rs`): `refresh_on_camera_homed` calls
  `display.pulse_home(event.sources)` when the event's camera is the panel's bound
  camera. `pulse_home` lights the home row and holds it for `HOME_HIGHLIGHT_HOLD`
  (1500 ms), re-arming on each invocation and fading on the timer — again because
  the glide has no discrete end event. A same-camera rebuild carries the pulse
  across explicitly via `home_highlight()`/`restore_home_highlight()`.
- **Title-bar chip flash** (`fairy_dust/src/screen_panels/title_bar.rs`): the chip's
  activation is binary (`TitleBarControlState::set_active`) and the glide has no end
  event, so `on_camera_homed` sets the chip active and starts `HomeTitleBarFlash`,
  a one-shot `Timer` of `HOME_TITLE_BAR_HIGHLIGHT_HOLD` (1500 ms).
  `tick_home_title_bar_flash` clears the chip when the timer finishes. This constant
  and timer are separate from the panel's `HOME_HIGHLIGHT_HOLD` because that one is
  `pub(super)` and not importable from `screen_panels`.

## Invariants

A future change must preserve these:

- **Presets are plain `Copy`/`Clone` data.** Bindings are built at install
  (`to_bindings()`/`build()`), never stored live on the preset.
- **Preset home is two-slot** `[Option<Binding>; 2]`. `with_home`/`home` fill the
  first empty slot, then a third call replaces the second; `has_home()` is true when
  any slot is occupied. Keep it an array so the payloads stay `Copy`.
- **`apply_home` is `set_target` only** — an eased retarget with no Animation entity
  and therefore no `AnimationEnd`. Anything that needs to know home fired must key
  off `CameraHomed`, not animation completion.
- **`capture_home` reads the current settled pose**; `on_animation_settled` is the
  only path that upgrades a provisional (`CameraHomePending`) home.
- **Gamepad bindings route only when the preset opts in** via
  `.gamepad(CameraInputGamepadSelectionPolicy::Active)`.
- **An app-provided `…HomePose` is authoritative.** Controller init captures a pose
  and inserts `CameraHomePending` only when `!has_home` (and the marker only when
  there is no input yet). `CameraHomePending` is transient and unregistered.
- **Setter naming:** FreeCam presets `with_*`; OrbitCam presets bare names.
  Composites delegate home to the **keyboard child** only, and `has_home` ORs both
  children.
- **`CameraHomed` is one `EntityEvent` for every camera kind**, emitted by the
  reset observer (`on_reset_{orbit,free}_cam_to_home`) on every home snap, carrying
  `#[event_target] camera` + `sources`. It is a notification: not `register_type`d,
  not itself BRP-triggerable.
- **The home snap is one code path, reached only by triggering a reset event.**
  `ResetOrbitCamToHome` / `ResetFreeCamToHome` are the deliberate app/BRP entry
  point; the keybind apply fns trigger them rather than snapping inline. Do not
  re-add an inline `apply_home` + `CameraHomed` in the adapters.
- **Home is an impulse action.** `OrbitCamHomeAction` / `FreeCamHomeAction` are
  `ImpulseCameraAction`; the edge fires once per press. Home storage is the
  `…HomeActionBindings` newtype over `ImpulseActionBindingSet`, not `Vec<Binding>`;
  keep the slice-mirroring accessors so external callers compile.
- **`CameraHomeKind::InteractionStarted` is the per-kind interaction-lock event**,
  distinct from `CameraHomed`. Keep them separate — the lock events carry differing
  `kind` enums.
- **The fill runs only when `CameraHomeConfig` exists and only for `Preset` modes
  with `has_home() == false`.** Keyboard-family → H, gamepad → H + Select. It is
  scheduled `PreUpdate` before `CameraInputPhase::PreInput`, and it must converge
  (write only on the fill, inspect by `&`).
- **`missing_docs` is denied** (`lib.rs:1`, workspace `Cargo.toml`); every new public
  item needs a doc comment.

## Calibration / gotchas

- **`CameraInputDisabled` deactivates the camera's entire BEI context**
  (`apply_context_gating`, `orbit_cam/input/adapter/install.rs`). Filled home
  bindings never fire while a camera is input-disabled — they are gated dead
  alongside every other camera control. This is why manual/custom examples that
  toggle `CameraInputDisabled` (`input_manual.rs`, `input_custom.rs`,
  `animation.rs`) hand-wire H with raw `ButtonInput` reads into the public
  `CameraHomeKind::apply_home` rather than relying on the fill.
- **Impulse home fires once per press, not every held frame.** Before the impulse
  reclassification, holding H re-pinned the camera's targets every frame; now the
  edge fires exactly once on the rising edge. Deliberate and correct, but observable
  if the home key is held against a simultaneous drag. A still-held home key re-fires
  exactly one frame after input re-enables (regression-tested).
- **`require_reset: false` re-fires a still-held key the frame a context re-enables.**
  When manual mode drops `CameraInputDisabled` with H still down, the home action
  fires its rising edge the next frame. Do **not** also hand-fire `CameraHomed` (or a
  reset event) in that path — the adapter triggers the reset event, whose observer
  fires `CameraHomed`. (This was the `animation.rs` double-announce bug; the fix left
  the event to the adapter/observer path and deleted the hand-fired trigger.)
- **`.with_camera_home()` reserves H.** `assert_no_reserved_collisions`
  (`fairy_dust/src/shortcuts.rs`) startup-crashes any example that *also* binds H via
  `with_shortcut`. The crash is invisible to `cargo build` and `cargo nextest` — it
  only surfaces by running the example. Hand-wired examples therefore read H with a
  raw `Res<ButtonInput<KeyCode>>`, not a BEI shortcut.
- **BEI 0.26 exposes no per-binding evaluated state** — binding entities hold only
  `Binding` + internal activation, and per-binding values aggregate transiently into
  one `ActionValue`. Live device attribution had to be built crate-side from
  `ButtonInput`/`Gamepad` reads (`source_binding.rs`).
- **`has_home() == false` always reads as "fill me."** With `without_home` gone,
  there is no way to express a *deliberately home-less* preset camera under
  `.with_camera_home()` — an explicit `with_home`/`home` only picks a different key.

Open follow-ups (no owner assigned):

- **Cross-camera `AtHome` rearm.** `on_camera_homed` flips the app-global `AtHome`
  to `Yes` on *any* camera's home event, while `refit_on_window_resized` targets
  only the single filter-matched camera. Inert in every current example, but an app
  mixing one builder camera with other home-capable cameras gets cross-camera
  rearm — H on camera B re-arms a refit that would stomp camera A's user pose. Close
  with an `event.camera`-vs-fit-camera check when such an app exists.
- **Same-frame press/release shows both device rows briefly.** A key pressed and
  released inside one frame can fire the home edge after `ButtonInput::pressed` reads
  false, so attribution falls back to the static union and a gamepad-preset panel
  momentarily shows both an H row and a Select row for a keyboard tap. Cosmetic;
  it is the everyday trigger of the fallback path.
- **No test pins the disable→re-enable single-fire** (`require_reset: false`) that
  the manual-H hand-off depends on. A BEI upgrade or an `action_settings()` change
  could break the hand-off silently. A test driving `CameraInputDisabled` removal
  with a held key and asserting exactly one `CameraHomed` would close it.

## Why

- **Opt-in rather than default home.** A preset is a library primitive; the meaning
  of the H key belongs to whoever assembles the app, not to every preset consumer.
  Default-home forced non-harness consumers to fight a binding they never requested
  and let the example harness carry a second, private H action on top of the
  preset's. Home-less-by-default hands the policy to the installer.
- **One ECS fill system rather than builder-side fill.** The builder bakes presets
  into opaque bundles at chain-call time, so a spawn-time builder fill was
  unimplementable. A single system over `Changed<…InputMode>` reaches every spawn
  path uniformly — builder, Shift+C cycle, raw `with_preset` — with zero per-path
  fill code, and scheduling it before `PreInput` means the first binding install
  already sees the filled preset.
- **`CameraHomed` unified two events.** The former `OrbitCamHomeStarted` and
  `FreeCamHomeStarted` had identical payloads (`camera` + `sources`) and every
  consumer was a duplicated pair. Collapsing them to one `EntityEvent` and one
  observer per consumer removed the duplication. The per-kind *interaction-lock*
  events stayed separate only because they carry different `kind` enums.
- **Two slots, not one.** A single `Option<Binding>` could not carry a key and a
  gamepad button at once: cycling a `FreeCam` to a gamepad preset killed H, and a
  gamepad preset's prepended "H Home" chip advertised a dead key. Two slots let a
  gamepad preset bind both H and Select; keeping it an array preserves `Copy`.
- **Home is a retarget-and-ease with no end event.** `apply_home` just calls
  `set_target` — idempotent while held, holding on release, with no Animation
  entity. That keeps the home glide interruptible and cheap, but it means there is
  no `AnimationEnd` to end the UI highlight. So `AtHome` re-arms on the `CameraHomed`
  rising edge, and both the panel row and the title chip hold on a fixed 1500 ms
  timer instead of clearing on an end signal.
- **`fairy_dust` owns the H-key choice.** `HOME_KEY`/`HOME_BUTTON` live in
  `fairy_dust` constants; the library never hard-codes H. The reservation
  (`reserve_key` + `assert_no_reserved_collisions`) makes that ownership enforceable
  at startup, which is what lets the fill safely claim H across every example.
