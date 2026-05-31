# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

> This crate is based on the work of [Plonq](https://github.com/Plonq) and his
> [bevy_panorbit_camera](https://github.com/Plonq/bevy_panorbit_camera). Thank you
> for graciously allowing this project to build on your foundation.

## [Unreleased]

### Changed
- `OrbitCamInteractionState::speed(kind)` now returns `Option<ControlSpeed>`; `None` reports an active gamepad interaction whose speed has not settled yet, letting consumers hold back the singular `Normal` variant until a slow-gate chord registers (breaking).
- Add `target: Option<Entity>` (the framed entity) to the `AnimationBegin`, `AnimationEnd`, and `AnimationRejected` observer events so consumers can distinguish which fit target an animation framed (breaking).
- Collapse `AnimationEnd`/`AnimationCancelled` and `ZoomEnd`/`ZoomCancelled` into single `AnimationEnd` and `ZoomEnd` events with a `reason: AnimationReason`/`ZoomReason` field — observers that listen for `*End` now fire on both natural completion and cancellation, fixing chip-highlight bugs when one animation interrupts another (breaking).
- Replace owned `bool` fields and accessors on `bevy_lagrange` input types with dedicated enums (`PinchGestureZoom`, `CameraMotion`, `FocusFrame`); renames `EguiWantsFocus.prev`/`curr` to `previous`/`current` (breaking).
- Replaced legacy raw `OrbitCam` input fields with input-mode components:
  `OrbitCamPreset`, `OrbitCamBindings`, and `OrbitCamManual`.
- Replaced manual active-camera setup for render-to-texture with
  `CameraInputRoutingConfig::explicit(...)` plus logical `CameraInputSurfaceMetrics`.
- Replaced keyboard target-mutation examples with `OrbitCamBindings` and
  `OrbitCamManualInputWriter` examples for user input.

### Added
- `OrbitCamReportingDebounce(Duration)` resource (default 100 ms) that holds the reported gamepad speed's return to `Normal` for a short window, smoothing the one- or two-frame flicker when the `rb`/`lb` slow gate is pressed or released a frame apart from its stick or trigger. Reporting-only — camera motion is never delayed, `Slow` reports immediately, and `Duration::ZERO` disables it.
- Source-attributed camera interaction lifecycle events and
  `OrbitCamInteractionState`.
- Teaching examples for SimpleMouse, BlenderLike, keyboard bindings, gamepad
  bindings, manual input, and custom bindings.
- `ZoomDirection { In, Out }` plus a `zoom_direction: Option<ZoomDirection>` tag
  on `OrbitCamControlRow`. Every zoom source in a built-in preset now describes
  one row per direction (gamepad `rt`/`lt`, keyboard `+`/`-`, `wheel ↑`/`↓`,
  `pinch out`/`in`, `scroll ↑`/`↓`) so a control panel can highlight only the
  engaged direction. `OrbitCamMouseWheelZoom` is now a marker struct (its
  removed `WheelZoomPolarity` field never shipped).

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
