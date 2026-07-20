# Monitor topology and raw events

## Status

Implemented in `bevy_clerestory` 0.2.0-dev. `Monitors`,
`MonitorTopologyRevision`, `MonitorConnected`, and `MonitorDisconnected` form
one entity-lifetime topology contract.

## Installed topology

`Monitors` is the last installed snapshot of live Bevy `Monitor` entities in
Bevy's cached `WinitMonitors` order. Each `LiveMonitor` contains the current
entity plus an entity-free `MonitorInfo`:

- `identity`: `MonitorIdentity::Verified(MonitorId)` only when complete,
  qualified native evidence identifies one active physical panel;
- `index`: the cached `WinitMonitors` index used by
  `MonitorSelection::Index` for this monitor lifetime;
- `physical_position`, `physical_size`, and `scale`: values supplied by that
  Bevy `Monitor` component when Clerestory performed topology work.

`MonitorId` is an opaque, append-only process token. It is never persisted.
Ambiguous, incomplete, contradictory, or unavailable evidence produces
`MonitorIdentity::Unverified`; matching never substitutes connector, position,
index, geometry, or the first monitor.

The cached winit association is consulted only during a triggered topology
build. It does not call `available_monitors()` or refresh native arrangement,
resolution, origin, scale, or other metadata. If a live `Monitor` component
temporarily has no cached handle, its identity remains unverified and its
deterministic entity-order position after the cached index range is
out-of-range for `MonitorSelection::Index`; Clerestory does not claim an
unobserved native order or redirect the index to another panel.

`MonitorInfo` never contains an entity. Use `Monitors::iter` for current entity
association or `Monitors::entity_by_id` for an exact, unique verified identity.
`Monitors::by_id` and `entity_by_id` return `None` for unverified identities and
for multiple live instances carrying the same token.

`MonitorTopologyRevision` starts at zero with the startup snapshot. It advances
when the installed entity set or identity-to-entity mapping changes. It does not
represent continuous monitor-property observation. A configuration generation
that revalidates identity but reproduces the installed entity/identity snapshot
leaves both `Monitors` and the revision unchanged.

`init_monitors` treats every monitor in a non-empty startup snapshot as a new
entity lifetime relative to an empty pre-install topology. During `PreStartup`,
it installs `Monitors` and `MonitorTopologyRevision(0)` before triggering one
`MonitorConnected` observer per monitor in the installed cached-monitor order.
The presence of startup monitors does not advance the revision. An empty startup
installs the empty snapshot and revision zero without a connection event.

## Entity-lifetime boundary

Bevy 0.19's `bevy_winit` checks `available_monitors()` in `create_monitors()` at
the winit `about_to_wait` boundary. A newly enumerated handle receives a new
Bevy `Monitor` entity, and a missing handle's entity is despawned. Clerestory
uses those component additions/removals as the monitor connect/disconnect
boundary.

A genuine reconnect therefore supplies a new Bevy `Monitor` component. The
returning entity's position, size, and scale enter the new installed snapshot,
including a cross-DPI reconnect.

For an equal handle that remains connected, Bevy does not refresh the existing
component's properties. Clerestory consequently refreshes its topology snapshot
for monitor entity lifetime changes and stable-identity revalidation; it does
not guarantee live metadata refresh when arrangement, resolution, origin, size,
or scale changes while the same entity remains present. Such a change may
produce no `MonitorTopologyRevision` update and must not initiate reconnect
recovery.

Wayland remains compositor-controlled for window positioning. Clerestory does
not promise client-selected windowed placement or manufacture verified physical
identity when the compositor does not expose sufficient evidence.

## Signal-driven producer

The topology producer checks three signals:

1. `Added<Monitor>`;
2. removed `Monitor` components;
3. a changed `MonitorConfiguration` state or generation requiring identity
   revalidation.

When none occurred, it returns before scanning monitor entities, reading
component metadata, looking up winit/native monitor handles, doing identity
work, allocating a snapshot, changing the revision, or causing a native
`current_monitor()` lookup.

On a trigger, the producer scans live Bevy `Monitor` entities once. It builds
the snapshot from their component metadata and compares cached
`WinitMonitors::nth` / entity-handle associations only to align indices with
`MonitorSelection::Index`. Cached handles are not queried for properties. The
producer uses native property access only for stable physical identity evidence
and does not enumerate another native topology for ordering, arrangement,
geometry, resolution, or scale.

macOS, Windows, and X11 configuration callbacks remain world-free and only
advance the atomic `MonitorConfiguration` generation. They exist so a triggered
build can revalidate qualified stable identity, including the rare case where a
native handle is reused. X11 uses the Phase 2 RandR notification listener only;
there is no XCB/RandR current-topology query, XSettings DPI parser,
`RESOURCE_MANAGER` tracker, or XFixes owner tracker in this producer.

The identity registry caches by configuration state/generation. A cached
instance in the same configuration does not invoke its evidence loader. The
append-only evidence history uses an exact
`HashMap<QualifiedEvidence, usize>` index with expected constant-time lookup;
records and `MonitorId` values are never reused or renumbered. Changed qualified
evidence cannot transfer an old verified identity to a different physical
panel.

Raw lifetime deltas use private `MonitorInstanceId` values derived from Bevy
entity lifetimes, not `MonitorId` or metadata. Identity-only revalidation may
change `Monitors` and `MonitorTopologyRevision`, but emits no connect or
disconnect event. Identical revalidation changes neither and emits no raw
event.

## Raw lifetime events

`MonitorConnected` contains:

- `entity`: the new live Bevy monitor entity;
- `monitor`: the entity-free `MonitorInfo` installed for that lifetime.

`MonitorDisconnected` contains:

- `former_entity`: the ended Bevy monitor entity;
- `monitor`: its last installed entity-free `MonitorInfo`.

Connect and disconnect events are raw lifetime facts. Recovery policy may use
them as inputs, but these events do not claim that two lifetimes represent the
same physical panel. Applications decide whether and how to recreate windows.

## Same-update ordering

One queued world operation performs a changed install in this order:

1. install `Monitors`;
2. install `MonitorTopologyRevision`;
3. remove stale `CurrentMonitor` components if no monitors remain;
4. emit optional `monitor-probe` records;
5. trigger `MonitorConnected` and `MonitorDisconnected` observers.

A connect observer can immediately resolve the event entity. A disconnect
observer sees `former_entity` absent. For an empty topology, it also sees stale
`CurrentMonitor` removed.

The `PreStartup` producer uses this same operation at revision zero, so startup
probe records precede startup connect observers. Connected events follow the new
installed snapshot order; disconnected events follow the prior installed
snapshot order. A later `update_monitors` run without another topology or
configuration signal emits no duplicate event and performs no topology work.

The monitor plugin runs `update_current_monitor` after topology installation
with a deferred-command synchronization point. Restore application runs after
that refresh, and settle verification runs after restore application. There is
no timer-based monitor-metadata settling.

`update_current_monitor` stores private inputs for managed/primary registration,
position, physical resolution, base and overridden scale, and window mode. It
performs a native `current_monitor()` lookup only after an installed topology
change or when those relevant window inputs change. Title, focus, visibility,
cursor, and other unrelated edits do not cause a lookup; an idle update does no
native current-window work.

## Diagnostic probe

The disabled-by-default `monitor-probe` Cargo feature emits structured tracing
records under the `bevy_clerestory::monitor_probe` target. Records are tied to
actual producer work: add/remove, identity-changing revalidation, or an
explicitly recorded unchanged identity revalidation. A record may include:

- consumed configuration state and generation;
- producer `FrameCount` and schedule label;
- installed topology revision and transition kind;
- private monitor instance identity;
- evidence provenance and its generation;
- public `MonitorIdentity` and verified `MonitorId`, when present;
- current or former monitor entity and its lifetime state.

The startup producer reports `PreStartup::init_monitors`; the runtime producer
reports `Update::monitor_topology_producer`. Startup connected records carry the
startup configuration and evidence provenance, `MonitorTopologyRevision(0)`,
and the producer's `FrameCount` before the public connect observer runs.

Probe records do not claim that connected-monitor geometry or scale was freshly
queried. They are diagnostic output, not public events, resolvers, matching
inputs, or stable serialization. Applications and recovery code do not depend
on them, and normal builds do not compile the optional tracing dependency.

## Consumer responsibilities

Display-capture consumers still reconcile their own backend after a connect or
disconnect. A monitor entity appearing does not mean a capture frame is ready,
and Clerestory does not own capture sessions, rendered content, cable routing,
window recreation, or application UI policy.
