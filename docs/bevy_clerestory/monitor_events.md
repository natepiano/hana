# Monitor connect/disconnect events

## Status

**Implemented in `bevy_clerestory` (0.2.0-dev); hana migration pending a
0.2.0 release.** `update_monitors` now triggers `MonitorConnected` /
`MonitorDisconnected`. `hana` still runs its interim raw-`Monitor` observers (see
[Interim implementation in hana](#interim-implementation-in-hana)) because it
depends on the published `0.1.1`; it switches to the display-space events once
`0.2.0` is released.

## Next steps

Ordered path from this branch to hana on the display-space events:

1. **Merge** `feature/monitor-events` → `main`.
2. **Release** `bevy_clerestory` 0.2.0 (`/release bevy_clerestory`): bumps
   `0.2.0-dev` → `0.2.0` and publishes.
3. **Bump hana's dep** `bevy_clerestory` `0.1.1` → `0.2.0` in
   `crates/hana/Cargo.toml`.
4. **Migrate hana's code** per [Migration](#migration) below — swap the interim
   raw-`Monitor` observers for observers on `bevy_clerestory::MonitorConnected` /
   `MonitorDisconnected`, and rename the clashing panel-space events.

## Motivation

An app that mirrors OS displays in-world (live screen capture on 3D panels)
needs to react when a monitor is plugged in or unplugged:

- **Disconnect** — freeze the last captured frame, grey it out, overlay a
  `DISCONNECTED` label, and stop the capture session.
- **Connect / reconnect** — resume the live feed on the existing panel, or spawn
  a new panel for a brand-new display.

Today no crate emits a monitor-change signal:

- `xcap` (the capture backend) goes silent on macOS when a display disconnects —
  no channel close, no error, only repeated recv timeouts — so it cannot be the
  source of truth. A reconnect requires re-polling `xcap::Monitor::all()`.
- Bevy exposes the live set only as `bevy::window::Monitor` entities, spawned and
  despawned each frame by `bevy_winit`'s `create_monitors`. Detecting a change
  means watching `Added<Monitor>` / `RemovedComponents<Monitor>`.
- `bevy_clerestory` already watches exactly that in `update_monitors` to rebuild
  its `Monitors` resource, but it swallows the edge — consumers only see the new
  `Monitors` value via change detection, not *which* display appeared or vanished.

`bevy_clerestory` is the natural home: it already tracks the monitor set and
already defines window-level `EntityEvent`s in `events.rs`.

## API

Two global events, triggered alongside the `Monitors` rebuild in
`update_monitors`. Global (not entity-targeted) because the payload *is* the
identity of the changed display; the consumer maps that to its own entity.

```rust
/// A display was connected. Triggered after `Monitors` is rebuilt to include it.
#[derive(Event, Debug, Clone, Reflect)]
pub struct MonitorConnected {
    /// The newly present monitor, as recorded in `Monitors`.
    pub monitor: MonitorInfo,
}

/// A display was disconnected. Triggered after `Monitors` is rebuilt without it.
#[derive(Event, Debug, Clone, Reflect)]
pub struct MonitorDisconnected {
    /// Geometry of the monitor as last known, before it vanished.
    pub monitor: MonitorInfo,
}
```

`MonitorInfo` carries a new `id: MonitorId` field — a stable, OS-assigned display
id normalized to `u64` across platforms (macOS `CGDirectDisplayID`, X11 /
Wayland output id, hashed GDI device name on Windows), sourced from winit's
`MonitorHandle` via `bevy::winit::WinitMonitors`. It survives display
rearrangement, unlike `index`. `physical_position / scale` still yields the
logical top-left used to pair a display with a capture feed.

### Emission point

`update_monitors` computes `added` (`Added<Monitor>`) and `removed`
(`RemovedComponents<Monitor>`); when either is non-empty it rebuilds the
resource, diffs the previous `Monitors` against the rebuilt one **keyed on
`MonitorId`**, and `commands.trigger`s a `MonitorConnected` /
`MonitorDisconnected` per delta. Id-keying (not position) means rearranging
displays fires nothing — only a genuine appear/vanish does.

The system takes a `NonSendMarker` so it runs on the main thread while reading
winit `MonitorHandle`s. Because `RemovedComponents<Monitor>` yields despawned
entities whose winit handle is already gone, the `MonitorInfo` for a disconnect
(including its `id`) comes from the *previous* `Monitors` snapshot, which
captured the id when the display connected.

## Consumer contract

- Events fire **after** `Monitors` reflects the change, so a handler can read the
  current `Monitors` consistently.
- A reconnect of the same physical display fires `MonitorConnected` again; a
  consumer must treat connect as idempotent (resume, don't duplicate).
- Capture backends still need their own reconcile on connect — the event is the
  trigger, not the capture session. The consumer re-polls `xcap::Monitor::all()`
  (or equivalent) in response.

## Interim implementation in hana

Until `hana` upgrades to `bevy_clerestory` 0.2.0, it sources the signal locally
in `crates/hana/src/screens/connection.rs`:

- `on_monitor_added` (`On<Add, Monitor>`) / `on_monitor_removed`
  (`On<Remove, Monitor>`) observers each queue `hana_video::reconcile_screens`,
  which diffs `xcap::Monitor::all()` by display id, re-opening or dropping
  capture sessions and flipping `ScreenFeed::connection`.
- `sync_screen_connections` turns each per-panel connection transition into a
  `MonitorConnected { entity }` / `MonitorDisconnected { entity }` **`EntityEvent`
  targeted at the screen-panel fill** — the panel-space analogue of the
  display-space events here. These are hana's own events, name-clashing with the
  `bevy_clerestory` ones.
- Observers `on_monitor_connected` / `on_monitor_disconnected` do the visible
  work (resume the feed vs. freeze + greyscale halftone + `DISCONNECTED` overlay).

### Migration

On the 0.2.0 upgrade, replace `on_monitor_added` / `on_monitor_removed` with
observers on `bevy_clerestory::MonitorConnected` / `MonitorDisconnected` that
queue `reconcile_screens` and, for a brand-new display, spawn the panel. The
feed-driven layer stays: `sync_screen_connections` and the panel-space
`EntityEvent`s still fire the visible connect/disconnect once xcap actually
produces (or drops) a frame — a monitor appearing is not the same instant as its
capture feed becoming ready, and a freshly spawned panel connects on its first
frame with no monitor event at all. Rename hana's panel-space events (e.g.
`ScreenFeedConnected` / `ScreenFeedDisconnected`) to clear the name clash with
the display-space events.
