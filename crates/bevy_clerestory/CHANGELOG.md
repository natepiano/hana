# Changelog

## [Unreleased]

### Added
- While the application remains running, its windows can now return to the same
  physical monitor after it is disconnected and reconnected. If the operating
  system moves a surviving window to another display, Clerestory can return it
  later. If Bevy deletes the window with the disconnected monitor, Clerestory
  can create one replacement `Window` on an available display and return that
  replacement when the monitor comes back.
- Opt a primary or managed window into reconnect handling by adding
  `WindowRecovery`. Clerestory waits until the window is associated with a
  verified monitor, then remembers that monitor until recovery is cancelled.
  Moving the window or changing/removing the component does not silently choose
  a new target.
- `WindowRecovery` provides three policies:
  - `Disabled` leaves the window outside reconnect handling.
  - `ApplicationControlled` notifies the application when the monitor
    disappears and returns. The application creates or selects the window and
    sends `RestoreWindow` when it is ready.
  - `FallbackAndReturn` tracks a surviving window on another display or creates
    a replacement window, then returns it automatically when the same verified
    monitor comes back.
- Clerestory restores the Bevy `Window` and its settings, but it does not clone
  application-owned cameras, UI, or other content. Applications attach that
  content when a replacement gains its `PrimaryWindow` or `ManagedWindow`
  component.
- `WindowRecoveryPending` reports that a registered monitor disappeared. For
  `ApplicationControlled` recovery, `WindowRecoveryAvailable` reports that it
  returned. Both identify the affected window with its stable `WindowKey`,
  which identifies the primary window or a named managed window across entity
  replacement. `RestoreWindow` names the replacement entity for an
  application-controlled restore.
  `CancelWindowRecovery` uses the stable key, so it still works after the
  original entity has been deleted; it keeps any surviving window where it is
  and stops automatic return.
- Physical monitor matching within one running application. When the operating
  system supplies enough identifying information, Clerestory assigns
  `MonitorIdentity::Verified(MonitorId)` and can recognize the same monitor
  after its Bevy entity or enumeration index changes. Otherwise the monitor is
  `Unverified`, and Clerestory does not guess from its connector, position, or
  index. A `MonitorId` is valid only in the current process and is never saved.
- `MonitorConnected` and `MonitorDisconnected` events report changes to the
  available monitors. Each event includes the affected monitor entity and a
  copy of its `MonitorInfo`; disconnect events retain the last known
  information after the entity is gone.
- `Monitors::iter()` returns `LiveMonitor` values containing each current
  monitor entity and its information. `MonitorTopologyRevision` changes when
  Clerestory installs an updated monitor inventory.
- Recovery notifications, monitor connection events, and restore results can
  be observed through the Bevy Remote Protocol (BRP) with
  `world.observe+watch`. BRP clients can send `RestoreWindow` and
  `CancelWindowRecovery` through `world.trigger_event`.
- The `restore_after_reconnect` example demonstrates both recovery policies,
  records an ordered diagnostic log, includes automated tests, and provides a
  two-cycle manual monitor-disconnect script.

### Changed
- `MonitorInfo` gains an `identity: MonitorIdentity` field. Breaking for code
  that constructs `MonitorInfo` directly; existing field access is unaffected.
- `Monitors::list` is no longer public. Use `Monitors::iter()` for live
  monitor entities and information, or use the existing lookup methods. This
  breaks code that read or replaced the list directly.
- Monitor order and index lookup no longer use the position-sorted list from
  0.1.1. `first()` now returns the first monitor in Bevy's current winit
  enumeration order, which is not necessarily the primary or leftmost monitor.
  `by_index(i)` finds the monitor currently reporting index `i`; it is not the
  same as indexing a dense vector. Use `at()` or `closest_to()` for position,
  and `by_id()` for a verified physical monitor.
- The saved state file is read once during startup. Clerestory then keeps the
  current window state in memory and writes only after that state changes.
  While a window is waiting to return automatically, its saved placement stays
  unchanged until restoration finishes or the application cancels recovery.
- Added a direct `winit` dependency (pinned to Bevy 0.19's version) to read
  platform-specific monitor identification data.

### Verification
- Automated Bevy tests exercise the platform-specific recovery logic for
  macOS, Windows, X11, and Wayland. They cover surviving and deleted windows,
  application-controlled and automatic recovery, operation with no displays or
  no windows, cancellation, and platform capability limits.
- Real monitor disconnects still depend on operating-system behavior that a
  simulated test cannot prove. The example README contains one earlier macOS
  disconnect/reconnect record; the complete two-cycle macOS, Windows, X11, and
  Wayland results have not yet been recorded. Reconstructed windows may also
  return in a different front-to-back order because Clerestory does not
  preserve stacking order.

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
