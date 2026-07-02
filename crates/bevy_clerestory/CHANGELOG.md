# Changelog

## [Unreleased]

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
