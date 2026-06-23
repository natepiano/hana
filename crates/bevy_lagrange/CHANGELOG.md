# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

> This crate is based on the work of [Plonq](https://github.com/Plonq) and his
> [bevy_panorbit_camera](https://github.com/Plonq/bevy_panorbit_camera). Thank you
> for graciously allowing this project to build on your foundation.

## [0.2.0] - 2026-06-23

### Changed

- Replace `OrbitCam`'s `{orbit,pan,zoom}_sensitivity` and `{orbit,pan,zoom}_smoothness`
  fields with three `orbit`/`pan`/`zoom` `AxisResponse` values bundling per-axis
  `Sensitivity` and `Damping`; read with `.sensitivity()`/`.damping()`, set with
  `.set_sensitivity()`/`.set_damping()` (breaking).
- Rename the input-sensitivity API to input gain (breaking). The per-axis
  `Sensitivity` type on `AxisResponse` is unrelated and unchanged.
  - Types:
    - `InputSensitivity` → `InputGain`
    - `OrbitCamSensitivity` → `OrbitCamInputGain`
    - `MouseSensitivity` → `MouseInputGain`
    - `GamepadSensitivity` → `GamepadInputGain`
    - `SmoothScrollSensitivity` → `SmoothScrollInputGain`
    - `OrbitCamBindingWithSensitivity<T>` → `OrbitCamBindingWithInputGain<T>`
  - Builder methods:
    - `with_sensitivity` → `with_input_gain`
    - `mouse_sensitivity` → `mouse_input_gain`
    - `smooth_scroll_sensitivity` → `smooth_scroll_input_gain`
    - `gamepad_sensitivity` → `gamepad_input_gain`

### Removed

- `reflect-input-modes` cargo feature and its reflected draft/apply types
  (`OrbitCamInputModeDescriptor`, `OrbitCamInputModeDraft`, the `*PresetDraft`
  mirrors, `OrbitCamInputModeApplyStatus`/`OrbitCamInputModeApplied`/`OrbitCamInputModeRejected`).
  Author input modes directly with `OrbitCamInputMode` (`with_preset`, `Bindings`,
  `Manual`); the runtime preset/binding types and their `Reflect` registration are
  unchanged.

## [0.1.0] - 2026-06-22

### Added
- Source-level input sensitivity on preset APIs — presets can carry mouse, gamepad, and smooth-scroll tuning through construction and conversion, preserved across `impl Into<OrbitCamPreset>` builder bridges.
- Binding-level sensitivity in the device adapter pipeline — mouse wheel, trackpad orbit/pan/zoom, pinch, touch, and button-drag inputs are scaled by their active binding sensitivity.

### Changed
- Change `OrbitCamPreset` variants to carry preset payloads; use helper constructors such as `OrbitCamPreset::blender_like()` or `OrbitCamInputMode::with_preset(...)` instead of unit variants like `OrbitCamPreset::BlenderLike` (breaking).
- A disabled input binding no longer affects camera motion or shows up as an active input source.
- Zero or negative input sensitivities are rejected consistently; a zero-sensitive source is disabled rather than applied.

### Fixed
- Clear cached orbit interaction and reported settle state when an input mode is replaced, so mode swaps no longer retain stale source or debounce data (including slow-mode latch reset).

## [0.0.4] - 2026-06-20

### Added
- `OrbitCamSlowMode` and `OrbitCamSlowModeState` — first-class slow control speed;
  BlenderLike preset adapters scale orbit/pan/zoom while a slow modifier is held, and the
  active speed is surfaced through `OrbitCamInteractionState`.
- `OrbitCamScalePolicy` — configurable input-scaling policy for orbit-cam adapters.
- Per-preset marker types — `OrbitCamSimpleMousePreset`, `OrbitCamSimpleMouseKeyboardPreset`,
  `OrbitCamBlenderLikePreset`, `OrbitCamBlenderLikeKeyboardPreset`, `OrbitCamKeyboardPreset` —
  plus preset helper constructors.
- `FitAnchor` event.

### Changed
- Update to Bevy 0.19.0 (stable) from the `0.19.0-rc.2` release candidate.
- Modularize orbit-cam presets into per-preset modules.

### Removed
- Remove `OrbitCamBindingsProfile`, `OrbitCamPresetLayer`, `OrbitCamPresetLayers`, and
  `PresetLayerSet` — superseded by the per-preset types (breaking).

## [0.0.4-rc.1] - 2026-06-05

### Changed
- Update to Bevy 0.19 (`0.19.0-rc.2`) (breaking).
- `OrbitCamInteractionState::speed(kind)` now returns `Option<ControlSpeed>` — `None` until a gamepad's speed settles (breaking).
- Add `target: Option<Entity>` to the `AnimationBegin`, `AnimationEnd`, and `AnimationRejected` events (breaking).
- Collapse `AnimationCancelled` into `AnimationEnd` and `ZoomCancelled` into `ZoomEnd`, each with a `reason` field (breaking).
- Replace owned `bool` input fields with enums (`PinchGestureZoom`, `CameraMotion`, `FocusFrame`) (breaking).
- Rename `ZoomDirection` to `ZoomInversion` (`Reversed` → `Inverted`) and `zoom_direction` builder method/field to `zoom_inversion`; the name `ZoomDirection` now refers to an unrelated new reporting enum (breaking).
- Rebuild input handling on `bevy_enhanced_input` (new dependency).
- Scroll zoom now uses an exponential curve so zoom in and out are symmetric.
- Move the crate into the [hana](https://github.com/natepiano/hana) monorepo; `repository`/`homepage` metadata updated.
- Fit overlay lines are now retained `Core3d` mesh visuals on the source camera's `RenderLayers` instead of gizmos; overlay labels render as UI text targeted via `UiTargetCamera`.
- Replace raw `OrbitCam` input fields with input-mode components (`OrbitCamPreset`, `OrbitCamBindings`, `OrbitCamManual`).
- Replace manual render-to-texture camera setup with `CameraInputRoutingConfig::explicit(...)` and `CameraInputSurfaceMetrics`.
- Replace keyboard target-mutation examples with `OrbitCamBindings`/`OrbitCamManualInputWriter` examples.

### Added
- `OrbitCamReportingDebounce(Duration)` resource (default 100 ms) — debounces the reported `OrbitCamInteractionState` (per-kind sources and gamepad speed) so a control panel does not flicker. Reporting-only; `Duration::ZERO` disables it.
- Source-attributed camera interaction lifecycle events and `OrbitCamInteractionState`.
- Teaching examples for SimpleMouse, BlenderLike, keyboard, gamepad, manual, and custom bindings.
- `ZoomDirection { In, Out }` and a `zoom_direction` tag on `OrbitCamControlRow`; built-in presets emit one zoom row per direction. `OrbitCamMouseWheelZoom` is now a marker struct.
- `OrbitCamInteractionState::zoom_direction()` reports the active zoom's direction, held through the debounce window and flipped at once on a reversal.
- Gamepad input preset.
- `reflect-input-modes` cargo feature (enabled by default) — reflection support for the input-mode components.
- `swapped_axis` example — multi-engine coordinate-convention showcase.

### Removed
- Remove the `bevy_egui` feature, `EguiWantsFocus`, and the egui example (breaking).

### Fixed
- Route camera input by focused window so overlapping windows with stale cursor positions no longer capture input.
- Suppress pinch zoom while BlenderLike modifier keys are held.
- Split `LookAtAndZoomToFit` into look and fit phases; internal fit work no longer emits `ZoomBegin`/`ZoomEnd`.
- Input teardown no longer double-despawns binding entities (logged "Could not despawn entity" on every preset switch).
- Fit overlay visuals no longer intercept picking, and duplicate/orphaned/stale overlay visuals are reconciled away.

## [0.0.3] - 2026-04-15

### Changed
- Reduced the default `OrbitCam` perspective zoom lower limit from `0.05` to `1e-7` so close-up orbiting can zoom much nearer to the target
- Perspective cameras now keep their near clip plane synchronized to orbit radius, with a minimum floor and far-plane clamp, to avoid clipping the focus target during close zoom

### Added
- Utility tests covering perspective near-plane synchronization for radius tracking, minimum clamping, and far-plane clamping

## [0.0.2] - 2026-04-06

### Changed
- Restructured `OrbitCam` input configuration: flat touch/trackpad fields (`touch_enabled`, `touch_controls`, `trackpad_behavior`, `trackpad_pinch_to_zoom_enabled`, `trackpad_sensitivity`) replaced with `Option<InputControl>` containing `Option<TouchInput>` and `Option<TrackpadInput>` — set to `None` to disable all user input
- Animation events and types (`ZoomToFit`, `LookAt`, `PlayAnimation`, `CameraMove`, etc.) are now always available without requiring the `fit_overlay` feature
- Extracted internal types (`ButtonZoomAxis`, `TrackpadBehavior`, `UpsideDownPolicy`, `ZoomDirection`, etc.) as standalone public types

### Fixed
- Prevent panic when closing a second window
- Idle camera no longer triggers Bevy change detection every frame — `&mut Transform` is now only passed when the camera actually moves

## [0.0.1] - 2026-03-28

### Added
- Pan, orbit, and zoom camera controls with smoothing, customizable sensitivity, and configurable key/mouse bindings
- Orthographic and perspective projection support
- Touch controls (one finger orbit, two finger pan, pinch zoom)
- Trackpad support with optional Blender-style orbit/pan/zoom mode
- Multi-viewport and multi-window support
- Render-to-texture camera support
- Optional `bevy_egui` feature to ignore input consumed by egui
- Optional `fit_overlay` feature: zoom-to-fit, queued camera animations, event-driven camera control (`ZoomToFit`, `LookAt`, `LookAtAndZoomToFit`, `AnimateToFit`, `PlayAnimation`), animation lifecycle events, conflict resolution, and debug overlay with gizmos and screen-space labels
