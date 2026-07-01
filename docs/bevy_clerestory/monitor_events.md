# Monitor connect/disconnect events

## Status

**Proposed.** Not yet implemented in `bevy_clerestory`. The consuming app
(`hana`) implements the equivalent logic itself for now (see
[Interim implementation in hana](#interim-implementation-in-hana)); this doc
records where the monitor-change signal should eventually live, since
`bevy_clerestory` owns the window/monitor surface.

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

## Proposed API

Add two global events fired alongside the `Monitors` rebuild in
`update_monitors`. Global (not entity-targeted) because the payload *is* the
identity of the changed display; the consumer maps that to its own entity.

```rust
/// A display was connected. Fired after `Monitors` is rebuilt to include it.
#[derive(Event, Debug, Clone, Reflect)]
pub struct MonitorConnected {
    /// The newly present monitor, as recorded in `Monitors`.
    pub monitor: MonitorInfo,
}

/// A display was disconnected. Fired after `Monitors` is rebuilt without it.
#[derive(Event, Debug, Clone, Reflect)]
pub struct MonitorDisconnected {
    /// Geometry of the monitor as last known, before it vanished.
    pub monitor: MonitorInfo,
}
```

`MonitorInfo` (physical position/size + scale + index) is enough for a consumer
to key the change to its own state; `physical_position / scale` yields the
logical top-left used to pair a display with a capture feed.

### Emission point

`update_monitors` already computes `added` (`Added<Monitor>`) and `removed`
(`RemovedComponents<Monitor>`) and rebuilds the resource when either is
non-empty. Extend it to diff the previous `Monitors` against the rebuilt one and
`commands.trigger` a `MonitorConnected` / `MonitorDisconnected` per delta. Keying
the diff on a stable display id (`CGDirectDisplayID` on macOS) is more robust
than position, which can shift when displays are rearranged.

Because `RemovedComponents<Monitor>` yields despawned entities whose components
are already gone, the `MonitorInfo` for a disconnect must come from the *previous*
`Monitors` snapshot, not from the removed entity.

## Consumer contract

- Events fire **after** `Monitors` reflects the change, so a handler can read the
  current `Monitors` consistently.
- A reconnect of the same physical display fires `MonitorConnected` again; a
  consumer must treat connect as idempotent (resume, don't duplicate).
- Capture backends still need their own reconcile on connect — the event is the
  trigger, not the capture session. The consumer re-polls `xcap::Monitor::all()`
  (or equivalent) in response.

## Interim implementation in hana

Until `bevy_clerestory` emits the above, `hana` sources the signal locally:

- `crates/hana/src/screens/connection.rs` — `watch_monitor_changes` watches
  `Added<Monitor>` / `RemovedComponents<Monitor>` and queues
  `hana_video::reconcile_screens` (which diffs `xcap::Monitor::all()` by display
  id, re-opening or dropping capture sessions and flipping
  `ScreenFeed::connected`).
- `sync_screen_connections` turns each per-panel `connected` transition into a
  `MonitorConnected { entity }` / `MonitorDisconnected { entity }` **`EntityEvent`
  targeted at the screen-panel fill** — the panel-space analogue of the
  display-space events proposed here.
- Observers `on_monitor_connected` / `on_monitor_disconnected` do the visible
  work (resume the feed vs. freeze + greyscale halftone + `DISCONNECTED` overlay).

### Migration

When `bevy_clerestory` ships the display-space events, `watch_monitor_changes`
can be deleted and replaced by an observer on `bevy_clerestory::MonitorConnected`
/ `MonitorDisconnected` that queues `reconcile_screens` and, for a brand-new
display, spawns the panel. The panel-space `EntityEvent`s and their observers
stay in `hana` — they are app UI, not a windowing concern.
