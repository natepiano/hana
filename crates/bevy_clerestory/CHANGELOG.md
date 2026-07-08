# Changelog

## [Unreleased]

### Added
- `MonitorConnected` / `MonitorDisconnected` events, triggered from `update_monitors` after the `Monitors` resource is rebuilt, one per changed display. Observe them with `On<MonitorConnected>` / `On<MonitorDisconnected>`; the payload is the changed `MonitorInfo`. Disconnect payloads come from the previous `Monitors` snapshot.
- `MonitorId`: a stable, OS-assigned display identifier normalized to `u64` across platforms (macOS `CGDirectDisplayID`, X11 / Wayland output id, hashed GDI device name on Windows). Keys the connect/disconnect diff so it survives display rearrangement.

### Changed
- `MonitorInfo` gains an `id: MonitorId` field. Breaking for code that constructs `MonitorInfo` directly; field access is unaffected.
- Added a direct `winit` dependency (pinned to bevy 0.19's version) to read per-platform native monitor ids.

## [0.1.1] - 2026-07-02

### Fixed
- Fix macOS automatic window tabbing merging same-app fullscreen windows into one tab group (blacking out the vacated monitor): `WindowManagerPlugin` now sets the app-wide `NSWindow.allowsAutomaticWindowTabbing = false` at plugin build, before any OS window exists

## 0.1.0 — Initial release

`bevy_clerestory` (formerly published as `bevy_window_manager`).

- Primary-window position/size persistence across launches.
- Multi-monitor support with scale-factor-correct positioning (mixed
  Retina / non-Retina setups).
- Correct placement when dragging across monitors with different scale factors.
- Platform workarounds: macOS, Windows, Linux X11 and Wayland.
- `Monitors` resource, `MonitorInfo`, `CurrentMonitor`, `ManagedWindow`,
  `ManagedWindowPersistence`, `WindowKey`, `Platform`,
  `WindowRestored` / `WindowRestoreMismatch` events.
- `WindowManagerPlugin` with `with_app_name` / `with_path` / `with_persistence`
  builders.
