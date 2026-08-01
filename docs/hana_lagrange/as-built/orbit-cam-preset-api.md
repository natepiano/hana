# OrbitCam Preset API

## What it is

A unified orbit-camera input system replacing the deleted layer/profile binding model with concrete preset config structs (`OrbitCamBlenderLikePreset`, `OrbitCamGamepadPreset`, etc.), one-liner `OrbitCam::*()` spawn helpers returning `impl Bundle`, and a tunable slow-mode feature toggled by Alt+S (5% scale by default) that applies uniformly to all seven scaled input sources (mouse-drag orbit/pan, smooth-scroll orbit/pan, wheel zoom, smooth-scroll zoom, pinch zoom) with no double-layer application.

## How it works

**Preset enum and delegation.** `OrbitCamPreset` (`orbit_cam/input/bindings/preset/enum_preset.rs`) holds six `#[non_exhaustive]` variants: `SimpleMouse`, `BlenderLike`, `Keyboard`, `SimpleMouseKeyboard`, `BlenderLikeKeyboard`, `Gamepad`. `OrbitCamPreset::to_bindings()` delegates each variant to a concrete preset config's `build()` (no `_ => unreachable!()` wildcard — inside the defining crate that arm is an unreachable-pattern warning for a `#[non_exhaustive]` enum).

**Sealed config trait + per-preset build.** Each concrete preset implements a crate-private sealed trait `OrbitCamPresetConfig: Sized` (`preset/config.rs`) whose `build(self) -> Result<OrbitCamBindings, BindingsError>` is mirrored by a public inherent `build()` on each struct so callers never import the trait. A private `build_into(self, builder) -> Result<OrbitCamBindingsBuilder, BindingsError>` exists for composed-preset reuse: child configs validate their own invariants *inside* `build_into` before mutating the builder, so a validation failure returns before any partial builder state.

**Config structs and fluent setters.** Each preset is a plain struct with `const fn`, `#[must_use]` fluent setters that consume and return `self`. `OrbitCamBlenderLikePreset` (`preset/blender_like.rs`) carries `zoom_mod_keys: ModKeys` (default `CONTROL`), `slow_toggle_key: Option<KeyCode>` (default `Some(KeyCode::KeyS)`), `slow_toggle_mod_keys: ModKeys` (default `ALT`), and `slow_scale: f32` (default `DEFAULT_SLOW_SCALE = 0.05`). `OrbitCamSimpleMousePreset` and `OrbitCamKeyboardPreset` are now small fielded structs (no longer unit structs): both carry the two-slot `home` opt-in described below, and `OrbitCamSimpleMousePreset` additionally carries input-gain fields.

**Composed presets and child reuse.** `OrbitCamBlenderLikeKeyboardPreset` (`preset/blender_like_keyboard.rs`) composes `pointer: OrbitCamBlenderLikePreset` and `keyboard: OrbitCamKeyboardPreset`. Its child setters (`.blender_like(...)`, `.keyboard(...)`) replace the whole field (no merge). It builds by calling each child's `build_into` on a shared builder, inheriting BlenderLike slow mode from its `pointer` child by default.

**Home opt-in (two-slot).** Every preset payload (`OrbitCamSimpleMousePreset`, `OrbitCamKeyboardPreset`, `OrbitCamBlenderLikePreset`, `OrbitCamGamepadPreset`, and both composites) carries `home: [Option<Binding>; 2]`, empty by default — a preset binds no home/reset input unless a caller opts in. Leaf presets expose bare `home(impl Into<Binding>)` / `has_home()`; `home` fills the first empty slot (a third call replaces the second) and `has_home()` is true when any slot is occupied. `OrbitCamPreset` dispatches `home`/`has_home` to its active variant so an installer holding the enum never matches variants. Composites route `home` to their **keyboard child** only and OR both children for `has_home`. The array keeps every payload `Copy`; because it holds a BEI `Binding` (which is `PartialEq` only), `OrbitCamKeyboardPreset` and `OrbitCamSimpleMousePreset` derive `PartialEq` but not `Eq`. Full home mechanics (the `.with_camera_home()` fill system, the `CameraHomed` event, device attribution) live in [`camera-home-bindings.md`](camera-home-bindings.md).

**Validation.** Each `build()` validates before descriptor construction, reusing existing `BindingsError` variants (`InvalidScale`, `InvalidDeadZone`) — no new error types. `OrbitCamBlenderLikePreset` requires `slow_scale` finite and within `(0.0, 1.0]` when slow mode is enabled. `OrbitCamGamepadPreset` requires all orbit/pan/zoom scales finite and non-negative, slow ≤ fast, and stick dead-zone bounds finite/ordered/in-range.

**Slow-mode data path.** When `slow_toggle_key.is_some()`, `OrbitCamBlenderLikePreset::build_into` constructs

```rust
CameraSlowMode {
    toggle_key: KeyCode::KeyS,
    mod_keys:   ModKeys::ALT,
    scale:      CameraInputScalePolicy { normal: 1.0, slow: self.slow_scale },
}
```

and calls `builder.slow_mode(...)` before `.build()`. `CameraInputScalePolicy` and `CameraSlowMode` are camera-neutral types living in the shared `input/bindings/descriptor.rs` alongside `InputDeadZone`. The value flows `builder → OrbitCamBindingsDescriptor.slow_mode → validate → OrbitCamBindings.slow_mode: Option<CameraSlowMode>`. The adapter reads `bindings.slow_mode()` to learn the toggle key + mod keys and the scale policy.

**Latch + toggle install.** `install.rs` binds the slow-mode toggle action with `Binding::Keyboard { key: toggle_key, mod_keys }` (so the edge only fires with Alt held). A per-camera `CameraSlowModeLatches` resource (`input/routing/latches.rs`) tracks which cameras have slow mode active. `recover_unavailable_latches` clears latches for despawned cameras; `clear_latches_on_mode_replaced` resets a camera's slow latch when its mode/bindings no longer reference slow mode.

**Single-layer scaling.** Scaling is applied exactly once, in `resolve.rs`: it samples the `slow_mode_toggle` action's `Fired` state to toggle the latch (`ResMut<CameraSlowModeLatches>`), then scales the `adapter_orbit`/`adapter_pan`/`adapter_zoom_coarse`/`adapter_zoom_smooth` blocks. The `AdapterScale<'_>` wrapper (`adapter/mod.rs`, internal `pub(super)`, `Copy`) holds the policy borrow + `slow_active` flag and applies `apply_scale(value, policy, slow_active) = value * if slow_active { policy.slow } else { policy.normal }` uniformly across all seven sources. `inject.rs` stages raw adapter values and applies no scaling. This matches the held-input path, which was already resolve-only.

**Mode-agnostic slow mode.** Both `OrbitCamInputMode::with_preset(OrbitCamPreset::blender_like())` and `OrbitCamInputMode::Bindings(built_from_blender_like)` produce bindings with `slow_mode.is_some()`; the adapter reads `OrbitCamBindings.slow_mode` regardless of mode variant — no branch on `OrbitCamInputMode`.

**Spawn helpers.** `OrbitCam::simple_mouse()`, `blender_like()`, `gamepad()`, `with_preset(impl Into<OrbitCamPreset>)`, `with_bindings(OrbitCamBindings)`, and `manual()` (`orbit_cam/presets.rs`) each return `impl Bundle` — a `(OrbitCam::default(), OrbitCamInputMode::…)` tuple, all `#[must_use]`. `mod presets;` extends `OrbitCam` directly; no `pub use` is needed. There are no `keyboard`/`simple_mouse_keyboard`/`blender_like_keyboard` helpers (they had zero callers); the keyboard-family `OrbitCamPreset` variants still exist and are reached through `with_preset`. When `OrbitCam` fields need configuring the helper cannot compose with overrides, so the explicit tuple is the "I know what I'm doing" path:

```rust
commands.spawn((
    Transform::from_xyz(0.0, 1.5, 5.0),
    OrbitCam { target_focus: Vec3::Y, target_radius: 8.0, ..default() },
    OrbitCamInputMode::with_preset(OrbitCamPreset::blender_like()),
));
```

**Pose construction.** `OrbitCam::from_pose(focus, angles, radius)` (`orbit_cam/mod.rs`) snaps a camera to a starting pose from bare values: `focus: impl Into<Focus>`, `angles: impl Into<OrbitAngles>`, `radius: impl Into<Radius>`. `impl From<(f32, f32)> for OrbitAngles` (yaw, pitch order, `operation.rs`) lets a call site pass a tuple —
`OrbitCam::from_pose(CAMERA_FOCUS, (CAMERA_YAW, CAMERA_PITCH), CAMERA_RADIUS)` — with no `Focus(..)`/`OrbitAngles { .. }`/`Radius(..)` wrappers. `Operation::{set_target, snap_to}` (`operation.rs`) likewise take `impl Into<V>`, so retargeting reads `camera.pan.snap_to(self.focus)` rather than `snap_to(Focus(self.focus))`. **Exception:** home-pose struct *literals* keep their newtype wrappers — struct fields cannot take `impl Into`, so an `OrbitCamHomePose { orbit: OrbitAngles { .. }, pan: Focus(..), zoom: Radius(..) }` literal stays fully typed, and the typed consts feeding those literals stay.

**Pose and preset do not compose into one constructor.** The bundle ctors (`with_preset`, `simple_mouse`, …) and the pose ctor (`from_pose`) are separate: `with_preset` forces `OrbitCam::default()`, so a custom pose plus a tuned preset cannot be expressed in a single call. Spawn the explicit tuple — `(OrbitCam::from_pose(..), OrbitCamInputMode::with_preset(..))`.

**Label derivation.** `OrbitCamInputMode` is a three-variant `#[non_exhaustive]` enum (`Preset`, `Bindings`, `Manual`). `describe_orbit_cam_controls` in `input/control_summary.rs` derives the runtime label: `Preset(preset)` → `mode_label = "Preset"`, `mode_value = preset.name()` (e.g. `"BlenderLike"`); `Bindings(_)` and `Manual` → `mode_label = "Input"` with `mode_value` `"custom bindings"` / `"manual input"`.

## Invariants

- `OrbitCamInputMode`, `OrbitCamPreset`, and `OrbitCamInputModeDraft` remain `#[non_exhaustive]`. No public example exhausts a `#[non_exhaustive]` enum without a wildcard arm.
- All binding validation happens before `build()` returns `Ok`. Composed-preset child configs validate inside `build_into` before any builder mutation — no partial builder state on error.
- `CameraInputScalePolicy` is a plain struct (mirrors `InputDeadZone`), validated at preset `build()` time, never at construction. It lives **only** in `CameraSlowMode` — it is not embedded per-descriptor; the adapter reads it uniformly via `bindings.slow_mode().map(|s| &s.scale)`.
- The slow-mode latch is per-camera entity (`CameraSlowModeLatches`). `clear_latches_on_mode_replaced` and `recover_unavailable_latches` must both cover slow-mode latches.
- Slow-mode scaling is applied exactly once, in `resolve.rs` via `AdapterScale`. `inject.rs` applies no scaling.
- `OrbitCamPresetBundle` does not exist — spawn helpers return `impl Bundle`.
- Preset home is a two-slot `[Option<Binding>; 2]` opt-in, empty by default — presets bind no home input unless a caller opts in via `home`/`with_home`. Composites route home to the keyboard child; `OrbitCamKeyboardPreset`/`OrbitCamSimpleMousePreset` therefore derive `PartialEq` but not `Eq`. See [`camera-home-bindings.md`](camera-home-bindings.md).

## Calibration / gotchas

- **`DEFAULT_SLOW_SCALE = 0.05`** (5%). Default toggle is `KeyCode::KeyS` + `ModKeys::ALT` — Alt is chosen to avoid colliding with BlenderLike's `Shift`=pan / `Ctrl`=zoom modifiers.
- **Two scale checks, different scopes.** `validate_scale_policy` enforces `slow ≤ normal` as a *relative* constraint (with `normal` normally `1.0`, `normal=2.0, slow=1.5` would still pass — a valid relative slowdown). The preset-level validation in `OrbitCamBlenderLikePreset` separately caps `slow_scale ≤ 1.0` as a hard user-facing limit.
- **No toggle key ⇒ no slow mode.** `slow_toggle_key(None)` makes `build()` emit `OrbitCamBindings` with `slow_mode == None`; non-BlenderLike presets always produce `slow_mode == None`.
- **Scaling lives only in resolve.** Re-adding scaling in `inject.rs` double-applies it (e.g. `0.05 × 0.05`). Keep it resolve-only.
- **Latch toggles on the press edge.** The latch flips on the `slow_mode_toggle` action's `Fired` state (key-press edge), not key-repeat or OS CapsLock text state; it applies only to the camera whose input context received the routed toggle.
- **`large_enum_variant` is allowed deliberately.** `mod_keys` pushed `OrbitCamInputMode::Bindings` (and the `OrbitCamInputModeDraft` mirror) past the 200-byte threshold. Both carry `#[allow(clippy::large_enum_variant)]` — boxing was rejected because only a handful of these components exist per app, so inlining beats per-camera heap indirection.
- **"Orbit reverses after Alt+S" was not a camera bug.** A controller-boundary trace showed monotonic single-direction coast-and-converge (orientation never flipped, input never reversed sign). The apparent reversal was the `with_cube_spin` showcase cube, made conspicuous because 5% slow shrinks the camera's own drift.

## Why

**Concrete config structs over layer/profile.** The old model carried profile metadata on every binding and routed through `OrbitCamPresetLayers`/`OrbitCamBindingsProfile`. Concrete preset configs that validate once and yield finished `OrbitCamBindings` remove that indirection: each struct is self-contained, fully overridable through fluent setters, and composes via `build_into`.

**`CameraInputScalePolicy` only in `CameraSlowMode`.** Embedding a scale policy per descriptor type (orbit, pan, each zoom source) fragmented the policy and forced the adapter to read it per source. Centralizing it in `CameraSlowMode` lets the adapter read one policy and apply it through a single `AdapterScale` wrapper across all seven sources.

**Helpers return `impl Bundle`, not a named bundle.** A named `OrbitCamPresetBundle` would either need `Copy`/`Clone` (problematic since `OrbitCamBindings` is not `Copy` after slow-mode types gained `Reflect`) or force boxing. `impl Bundle` sidesteps both — callers spawn the tuple directly, and field overrides drop to the explicit tuple path.

**The BlenderLike example spawns `OrbitCam::with_preset(tuned_blender_like_preset())` Preset mode.** An earlier version spawned `OrbitCam::with_bindings(tuned)`, producing `OrbitCamInputMode::Bindings`, which the label system renders as "custom bindings" — misrepresenting a tuned preset as hand-built bindings. Preset mode labels "Preset / BlenderLike" correctly; the custom-bindings story is already owned by `input_custom.rs`.

**Single-layer resolve scaling.** Applying scale in both `inject.rs` and `resolve.rs` double-applied it. Consolidating to resolve-only matches the held-input path (already resolve-only): scaling is intrinsic to action resolution, not adapter injection.
