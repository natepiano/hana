# Restore windows after monitor reconnect

## Status

**Proposed.** `bevy_clerestory` detects physical display connection changes,
but it does not yet retain window recovery intent or restore a live window when
its original monitor reconnects.

This design complements [Monitor connect/disconnect events](monitor_events.md).
Those events describe display availability. This document describes how window
lifecycle policy can be layered on top without forcing every window to use the
same behavior.

## Decision summary

- Monitor connection events remain unconditional facts about display
  availability.
- Recovery behavior is selected per registered window, not inferred from
  whether the window is primary or secondary.
- With automatic return selected, the operating system may move any surviving
  window to an available display. Clerestory returns it to its original display
  when that display reconnects unless the user changes the fallback placement.
- `hana` selects automatic return for its primary editor window.
- Application-owned display windows, including cable-driven fullscreen output,
  can select application-controlled recovery. Clerestory reports target
  availability and retains window metadata; the application alone decides what
  the output shows and whether it should exist.
- `bevy_clerestory` retains restore intent, protects it from fallback-state
  persistence, and applies placement through its existing restore pipeline.
- Applications can observe recovery events and explicitly request restoration
  of saved window placement after creating their own window entities.
- Rearranging connected monitors does not initiate recovery. Automatic matching
  uses a verified `MonitorId`, never display position or list index.

## Why recovery needs policy

Bevy associates a `Window` with its current `Monitor` through `OnMonitor`.
`Monitor` owns the inverse `HasWindows` relationship with linked-spawn
semantics. When `bevy_winit` removes a monitor entity, windows still related to
that monitor can be despawned with it.

There are two distinct disconnect outcomes:

- The operating system moves the existing window to an available display. The
  window entity survives, so clerestory can return either a primary or
  secondary managed window without reconstructing its contents.
- Bevy removes a monitor entity while a window is still related to it. Because
  `HasWindows` uses linked-spawn semantics, that can despawn the window entity.
  Preventing or recovering from that linked despawn is a separate implementation
  problem, not a reason to restrict automatic return to primary windows.

Different applications still need different policies. An ordinary editor or
tool window can follow the operating system to a fallback display and return
automatically. A display-specific presentation, such as Hana's cable-driven
fullscreen output, should instead become unavailable and let application code
decide whether to re-enable it after the physical display returns.

Clerestory can save and restore `Window` geometry, size, mode, and monitor
placement, and publish monitor/window recovery metadata. Cameras, render
targets, egui mapping, cable routes, capture sessions, frame readiness, and
everything displayed inside the window remain application state. None of those
become part of clerestory's recovery model.

## Goals

- Apply disconnect/reconnect handling to primary and non-primary windows
  without imposing one lifecycle on all of them.
- Let an application choose automatic or application-controlled recovery.
- Keep the primary editor accessible after its display disappears.
- Keep display-specific output bound to its original physical display.
- Reuse `compute_target_position`, `TargetPosition`, `restore_windows`, and the
  existing settle/result events.
- Preserve the last intentional state for the unavailable display while a
  temporary fallback window exists.
- Make reconnect handling idempotent and insensitive to monitor rearrangement.

## Non-goals

- Reconstruct arbitrary application content associated with a destroyed
  window.
- Move a display-specific output window onto an unrelated monitor merely to
  keep an OS window alive.
- Treat a newly discovered display as the original target when its `MonitorId`
  does not match.
- Persist Hana's cable routing or capture-session state inside clerestory.
- Automatically restore an unmanaged window that has no stable application or
  clerestory key.

## Monitor identity contract

Automatic return requires a verified physical-panel identity. Public monitor
metadata represents that explicitly:

```rust
#[derive(Clone, Debug, PartialEq, Eq, Reflect)]
pub enum MonitorIdentity {
    Verified(MonitorId),
    Unverified,
}
```

`MonitorId` is opaque and can contain the complete platform-specific identity.
`Unverified` covers both unavailable identity and a value that is not unique
among the live displays; clerestory keeps the detailed cause only for
diagnostics because both cases prohibit automatic matching.

`MonitorInfo` exposes `identity: MonitorIdentity` instead of an unconditional
ID. `Monitors::by_id` and `Monitors::entity_by_id` return a match only when one
live monitor has that exact verified ID. They never fall back to connector,
geometry, enumeration index, or the first available monitor. Connection events
still report every topology change, including monitors whose identity is
unverified.

A window registered on an unverified monitor is not armed for automatic
return. Clerestory does not provide an API that declares two unverified monitor
instances to be the same physical panel. Application-controlled code may choose
a live monitor entity and place or recreate its window itself; it can then
cancel and register a new recovery baseline if that monitor has verified
identity.

## Recovery policies

The approved public policy is intentionally small:

```rust
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq, Reflect)]
#[reflect(Component)]
pub enum WindowRecovery {
    /// Do not retain window-specific recovery state.
    #[default]
    Disabled,
    /// Retain the target and notify the application, but never create or move a
    /// window without an application request.
    ApplicationControlled,
    /// Let the operating system keep a surviving window usable on another
    /// display, then return it when its verified original display reconnects.
    FallbackAndReturn,
}
```

The same component configures primary and secondary managed windows. Hana adds
automatic return to its editor during primary-window setup:

```rust
commands
    .entity(primary_window)
    .insert(WindowRecovery::FallbackAndReturn);
```

An application-controlled tool window opts in alongside its stable
`ManagedWindow` key:

```rust
commands.spawn((
    Window::default(),
    ManagedWindow { name: "inspector".into() },
    WindowRecovery::ApplicationControlled,
));
```

`FallbackAndReturn` applies to any registered window that survives the
operating system's fallback move. Once that initial move has settled, a later
position, size, or mode change is treated as user/application intervention:
clerestory adopts the fallback placement and cancels the pending automatic
return.

`ApplicationControlled` retains the verified target and emits availability
events, but it never moves, creates, or re-enables the presentation. This is
the policy for an application that represents the disconnected display as
unavailable and decides whether the user should re-enable its output.

If Bevy's linked monitor relationship despawns the window before the operating
system move is reflected in ECS, generic return no longer has a live entity to
move. A dedicated hotplug example must establish the platform event order and
demonstrate the selected prevention or reconstruction solution for both
primary and secondary windows before that path is considered implemented.

Clerestory copies the policy into its recovery registry while the window is
healthy, so removing the entity does not erase the selected behavior.

The generic `WindowManagerPlugin` does not silently select automatic return. A
window with no `WindowRecovery` component has disabled recovery and no recovery
registry entry. Hana explicitly inserts `FallbackAndReturn` on its editor.

## Ownership boundary

### `bevy_clerestory`

Clerestory owns:

- the last intentional restorable state for registered windows;
- the verified physical `MonitorId` associated with recoverable state;
- detection that a registered window's target became unavailable;
- recovery lifecycle events;
- suppression of temporary fallback persistence;
- resolving a reconnected `MonitorId` to its current monitor index;
- placement calculations and restore settling;
- `WindowRestored` and `WindowRestoreMismatch` results.

### Application

The application owns:

- selecting a recovery policy;
- deciding whether a semantic window should exist;
- creating replacement window entities and their cameras/content when its
  selected lifecycle requires reconstruction;
- keeping route, capture, or tool state alive while an OS window is absent;
- marking application UI active or inactive;
- repairing entity-specific mappings after a replacement window is created;
- explicitly requesting restore for application-controlled windows.

For `hana`, `WindowRecoveryPlugin` remains responsible for user-close behavior
and any Hana-specific repair needed after a primary entity is actually
replaced, such as egui remapping. Clerestory supplies the retained target and
performs the eventual move back for every managed window.

## Recovery identity and state

Clerestory needs two durable, `WindowKey`-keyed records independent of the
window entity: captured window state and recovery lifecycle state.

```rust
struct CapturedWindowState {
    placement: CapturedWindowPlacement,
    persistence: CapturedPersistence,
    live: Option<LiveWindow>,
}

struct CapturedWindowPlacement {
    monitor_id: MonitorId,
    monitor_snapshot: MonitorInfo,
    logical_offset: IVec2,
    logical_size: UVec2,
    mode: WindowMode,
    captured_scale: f64,
}

enum CapturedPersistence {
    Writable,
    Frozen,
}

struct RestoreAttempt {
    id: RestoreAttemptId,
    entity: Entity,
    expected_monitor: MonitorId,
    topology_revision: MonitorTopologyRevision,
    deadline: Instant,
}
```

The captured-state resource is the authority from which persistence projects
the current RON representation. It retains enough of the capture-time monitor
snapshot to serialize an index-based `WindowState` while the physical monitor
is absent. Persistence never reconstructs this authority from the RON file or a
live-window-only query.

Recovery lifecycle is a separate registry. Disabled windows have no recovery
entry. Application-controlled and automatic-return entries use separate
private phase enums so an automatic fallback state cannot exist for an
application-controlled window. The application-controlled path covers healthy,
removal pending, target absent, target available, restoring, and retryable
failure. The automatic-return path covers healthy, removal pending, fallback
settling, on fallback, restoring, a missing live window, and retryable failure.
It is used for both primary and secondary windows. Zero available displays is
topology input, not another phase.

Each accepted restore creates a private `RestoreAttemptId`. Results must match
the canonical `WindowKey`, entity, attempt ID, expected `MonitorId`, and current
topology revision. A mismatch or timeout ends that attempt while retaining the
frozen target; retry begins only from a later matching topology revision or an
explicit application request, never every frame.

`MonitorDisconnected` is observed after Bevy has begun removing monitor-linked
entities, so clerestory cannot rely on querying the lost window at that point.
While a window is healthy, its captured entry retains the canonical key and its
exact live monitor entity. Window removal records a candidate without retiring
that entry. Explicit close or cancellation intent is processed first and wins;
any unmarked removal remains recoverable even if a fast reconnect has already
made the target present again. Programmatic close therefore uses an ordered
cancel-then-despawn operation.

For in-process disconnect/reconnect recovery, `MonitorId` lives in captured
state and active restore attempts. The RON format can remain index-based
initially. Persisting a physical display identity is a separate feature needed
for “launch while the display is absent, then attach it later.”

## Public recovery events and requests

Monitor events remain useful to all applications. Registered recoverable
windows additionally receive derived, window-oriented events.

All public clerestory events derive `Reflect` and include `#[reflect(Event)]`.
This supports Bevy 0.19's `world.observe+watch` for notification/result events
and `world.trigger_event` for request/cancellation events. This workspace's
`reflect_auto_register` feature registers these non-generic types; the plugin
does not add redundant `App::register_type` calls.

### Target unavailable

```rust
#[derive(Event, Debug, Clone, Reflect)]
#[reflect(Event)]
pub struct WindowRecoveryPending {
    pub window_key: WindowKey,
    pub monitor_id: MonitorId,
    pub policy: WindowRecovery,
}
```

This is global rather than entity-targeted because the original entity may
already be gone. It is emitted once per transition into
`WaitingForMonitor`.

### Target available

```rust
#[derive(Event, Debug, Clone, Reflect)]
#[reflect(Event)]
pub struct WindowRecoveryAvailable {
    pub window_key: WindowKey,
    pub monitor: MonitorInfo,
}
```

This is emitted when the matching `MonitorId` returns. Duplicate
`MonitorConnected` events do not emit duplicate availability transitions.

### Application restore request

```rust
#[derive(EntityEvent, Debug, Clone, Reflect)]
#[reflect(Event)]
pub struct RestoreWindow {
    pub entity: Entity,
}
```

An application-controlled flow is:

1. Observe `WindowRecoveryAvailable`.
2. Recreate the `Window` and its application-owned supporting entities.
3. Trigger `RestoreWindow` on the new window entity.
4. Observe the existing `WindowRestored` or `WindowRestoreMismatch` result.

The `RestoreWindow` observer retrieves the retained captured placement,
resolves the live monitor by `MonitorId`, computes a `TargetPosition`, and
inserts it on the new entity. The existing restore systems perform the actual
work.

If the target is no longer present when the request is handled, clerestory
leaves the intent pending and emits no false success.

### Cancel recovery

An application can retire recovery even after the original window entity no
longer exists by addressing its stable clerestory key:

```rust
#[derive(Event, Debug, Clone, Reflect)]
#[reflect(Event)]
pub struct CancelWindowRecovery {
    pub window: WindowKey,
}
```

Cancellation removes the pending recovery entry and invalidates any private
restore attempt. Reconnecting the monitor then produces no window-recovery
availability notification for that key. Cancellation does not despawn a live
window or delete its saved placement; an intentional close is ordered as
cancel, then despawn. `ManagedWindowPersistence::RememberAll` or
`ManagedWindowPersistence::ActiveOnly` remains the sole authority for whether
the placement survives after the window closes.

Clerestory does not know what application action caused cancellation. For
example, Hana may keep its cable route after a physical monitor disconnect, but
send `CancelWindowRecovery` if the user later removes that cable. Clerestory
then prevents reconnect recovery for the registered window key without owning
or modifying the cable state.

## Automatic return flow

With `FallbackAndReturn` selected:

1. Clerestory continually records the managed window's stable state and the
   verified identity in `CurrentMonitor.monitor_info`.
2. The target monitor disappears.
3. Clerestory freezes the original recovery intent and emits
   `WindowRecoveryPending`.
4. If the operating system moves the existing window to another display,
   clerestory waits for that fallback placement to settle and does not persist
   it over the frozen target.
5. If the linked monitor despawn instead removes the window entity, clerestory
   retains the recovery record but cannot move a nonexistent window. The
   hotplug example must demonstrate the selected prevention or reconstruction
   path; Hana may retain a primary-specific replacement adapter until a generic
   solution is proven.
6. A `MonitorConnected` with the frozen `MonitorId` arrives.
7. If the live fallback has not been moved, resized, or changed mode after its
   initial relocation settled, clerestory follows the same internal restore
   path used by explicit application requests.
8. `restore_windows` moves it to the reconnected monitor and reapplies its saved
   position, size, and mode.
9. After `WindowRestored`, clerestory clears the pending recovery and resumes
    ordinary persistence.

If the window changes position, size, or mode after fallback settling,
clerestory treats that as user or application intervention. It adopts the live
placement, clears the frozen return target, and does not move the window when
the old monitor reconnects.

An application selecting `ApplicationControlled` receives the same pending and
available events but decides whether and when to recreate or re-enable its
presentation. With `Disabled`, clerestory does not retain a recovery intent.

User-initiated close remains separate from monitor loss. Hana should inspect
the entity in `WindowCloseRequested`: closing the primary exits Hana, while an
output-window close follows that output's application lifecycle.

## Application-controlled screen output boundary

A cable-driven Hana output illustrates the boundary. Clerestory may provide the
verified monitor identity, availability events, saved window placement, and an
explicit restore operation. Hana decides whether the output is active, what it
shows, whether a reconnect should be offered to the user, and when its own
content is ready. Clerestory neither models nor observes any of those decisions.

If Hana does not persist an output as a `ManagedWindow`, it can consume the raw
`MonitorConnected` / `MonitorDisconnected` metadata directly and clerestory has
no output-window recovery state to cancel. If Hana does register one, it uses
`ApplicationControlled`, creates or removes the window according to its own
state, requests placement restoration only when wanted, and sends
`CancelWindowRecovery` when that semantic output has been retired.

## Hana real-world integration pass

The clerestory implementation is not complete when its isolated example and
state-machine tests pass. Before stabilizing the public recovery API, make a
focused pass through `../hana` and wire the behavior into the existing editor,
screen, and conduit code. These are the real consumers that must validate the
division between automatic return and application-controlled availability.

The pass covers:

- `crates/hana/src/main.rs` and `window_recovery.rs`: upgrade to the workspace
  clerestory version, register the editor with
  `WindowRecovery::FallbackAndReturn`, replace the current “no windows exist”
  recovery heuristic with clerestory lifecycle state, and retain Hana-specific
  entity reconstruction/egui rebinding only where the hotplug example proves a
  linked despawn actually requires it;
- `crates/hana/src/window_recovery.rs`: inspect the entity carried by
  `WindowCloseRequested`; closing the primary exits, while closing an output
  follows its output/cable lifecycle instead of terminating Hana;
- `crates/hana/src/screens/connection.rs`, `screens/panel.rs`,
  `conduit/window.rs`, and `conduit/cable.rs`: consume verified monitor identity
  and availability metadata, select `ApplicationControlled` where Hana
  registers an output with clerestory, avoid fallback placement for
  display-bound output, and send `CancelWindowRecovery` when application state
  says the output has been retired;
- Hana-owned output behavior: confirm that monitor loss is represented as
  unavailable and monitor return can be offered for re-enable without putting
  capture state, cable state, content readiness, or UI policy into clerestory;
- ordinary Hana-managed secondary windows, if any already exist: exercise
  `FallbackAndReturn`; otherwise the dedicated clerestory example remains the
  secondary-window coverage rather than inventing a Hana use case.

The pass is an API feedback gate for save/restore and monitor/window metadata.
If Hana needs to bypass clerestory identity, infer from logical position,
retain a dead monitor entity, or reproduce private window-recovery state,
revise the clerestory API and rerun its example/tests. Requirements concerning
capture, cables, rendered content, or re-enable UI remain in Hana and must not
expand the clerestory API.

## Persistence rules

- A stable, intentionally placed window updates its saved state normally.
- A `FallbackAndReturn` window freezes its original state when the target
  disappears.
- The operating system's initial fallback relocation does not overwrite the
  frozen target while that relocation is settling.
- After the fallback settles, a position, size, or mode change is treated as
  intervention: the current placement becomes intentional and automatic return
  is cancelled.
- Other windows continue to persist while one window is pending recovery;
  persistence suppression is per `WindowKey`, not a global pause.
- An application-controlled window with no fallback leaves its previous saved
  state untouched.
- A cable-driven output does not write clerestory window state; its application
  route remains authoritative.
- Successful restoration resumes persistence from the restored placement.

The settling boundary is what distinguishes compositor-driven events from a
later intervention. Because window events do not reliably identify whether a
change came from the user or application code, either source intentionally has
the same effect after settling.

## Ordering and correctness prerequisites

The recovery implementation depends on these corrections:

1. `update_monitors` must install the rebuilt `Monitors` resource before it
   triggers `MonitorConnected` or `MonitorDisconnected`, matching the public
   event contract.
2. `CurrentMonitor` change detection must compare `MonitorIdentity` and updated
   monitor properties, not only monitor index and window mode.
3. Hana must upgrade from `bevy_clerestory` 0.1.1 to the release containing
   `MonitorId` and monitor connection events.
4. Hana's primary recovery query must test for a missing `PrimaryWindow`, not an
   empty set of all `Window` entities.
5. Recovery state must be captured before the linked monitor/window despawn can
   erase the source components.

## Invariants

- At most one pending recovery intent exists per `WindowKey`.
- A monitor connection affects an intent only when `MonitorId` matches.
- Repeated connection events are idempotent.
- Rearrangement of the same connected monitor IDs creates no recovery
  transition.
- `ApplicationControlled` never creates or moves a window by itself.
- `FallbackAndReturn` applies equally to registered primary and secondary
  windows.
- `FallbackAndReturn` never persists the operating system's initial fallback
  placement over the original target.
- A post-settle fallback move, resize, or mode change cancels automatic return
  and becomes the new intentional placement.
- User close does not become monitor-loss recovery.
- A restore request never reconstructs application-owned supporting entities.

## Test plan

### `bevy_clerestory` hotplug example

Add `crates/bevy_clerestory/examples/restore_after_reconnect/` before choosing
the linked-despawn implementation. The example must create a primary window and
a secondary `ManagedWindow`, place both on a selected external display, and
configure both with `WindowRecovery::FallbackAndReturn`.

Its on-screen diagnostics and logs must show, in order:

- monitor entity and verified `MonitorId` creation/removal;
- each window entity and its `OnMonitor` changes;
- window moved/resized/mode events;
- `Window` component or entity removal;
- clerestory pending, fallback-settled, intervention, and restored transitions.

The manual script is: disconnect the external display, observe whether each
window is relocated with the same entity or removed through `HasWindows`,
reconnect without touching the fallback and verify automatic return, then
repeat while moving the fallback and verify that it stays where the user put
it. Run that script on macOS, Windows, X11, and Wayland where available.

If a platform exhibits linked despawn, the example must be extended to
demonstrate the selected solution—prevent the relationship cascade safely or
reconstruct and rebind the window—before the implementation phase depending on
that solution is accepted. The example is specifically required to prove that
the behavior and mitigation are the same for primary and secondary windows.

### Clerestory state-machine tests

- Primary and secondary target disconnects each record one frozen recovery
  intent.
- An operating-system fallback move does not overwrite frozen state.
- A post-settle fallback move, resize, or mode change adopts that placement and
  cancels automatic return.
- A reconnect with a different `MonitorId` does nothing.
- Reconnecting the target emits availability once.
- Duplicate reconnect events are no-ops.
- Rearranging monitors with unchanged IDs does not initiate recovery.
- An application restore request inserts the same `TargetPosition` that startup
  restoration would compute.
- A request made after the target disappears again stays pending.
- Successful settle clears the intent and resumes persistence.
- Restore mismatch leaves an explicit recoverable/error state rather than
  looping every frame.
- Multiple pending windows on one monitor retain independent `WindowKey`
  records.

### Hana integration tests

- An operating-system-relocated editor returns when its monitor reconnects.
- Moving or resizing the relocated editor prevents automatic return.
- If the platform linked-despawns the editor, Hana creates exactly one
  replacement and clerestory still returns it to the verified target.
- Losing the primary while a conduit output window survives does not block
  editor recovery.
- Egui input remaps to the replacement primary.
- Closing the primary exits; closing an output follows output lifecycle instead
  of exiting Hana unconditionally.
- Losing a cable target marks its in-world screen inactive and creates no
  fallback output.
- Reconnecting the cable target offers application-controlled re-enable and
  does not make clerestory create content or an output automatically.
- Removing the cable while its monitor is absent leaves no active clerestory
  recovery: cancel its key if the output was registered, or verify that no entry
  existed if Hana used raw monitor events only. Reconnecting the monitor does
  not revive the retired output.
- Mixed-DPI reconnect restores primary position and logical size correctly.
- Windowed, borderless-fullscreen, and fullscreen recovery reuse the existing
  mode restoration behavior.

### Manual platform coverage

Run physical disconnect/reconnect checks on macOS first, then Windows and Linux
where available. Tests should include a laptop display plus external monitor,
two external monitors, dock removal, and reconnect after monitor order changes.

## Implementation order

1. Correct monitor-event ordering and `CurrentMonitor` updates, then add the
   approved recovery-grade identity and live monitor resolver.
2. Add the dedicated `restore_after_reconnect` example and use physical hotplug
   runs to establish OS relocation versus Bevy linked-despawn ordering for both
   primary and secondary windows. Select and demonstrate the linked-despawn
   solution in the example before depending on it.
3. Introduce the `WindowKey`-keyed captured-state authority, monitor-relative
   placement, and RON projection while preserving current persistence behavior.
4. Consolidate primary and managed startup restoration behind the staged
   preparation path before exposing runtime callers.
5. Add the private policy-specific recovery lifecycle, fallback-settling and
   intervention rules, attempt validation, the linked-despawn solution selected
   through the example, and a hardware-independent transition harness.
6. Add the approved public registration, notification, restore, cancellation,
   and reflection surface as one usable phase.
7. Run the first Hana integration pass: upgrade clerestory, wire editor
   automatic return and entity-scoped close behavior, and exercise any existing
   ordinary managed secondary window. Treat this as an API feedback gate and
   revise clerestory if Hana would otherwise duplicate its private state.
8. Complete the Hana consumer pass for monitor-backed screens and outputs using
   only clerestory identity, availability, persistence, restore, and
   cancellation APIs. Keep capture, cable, rendered-content, and UI behavior in
   Hana.
9. Feed any save/restore or metadata API mismatch back into clerestory; then add
   clerestory state-machine tests, Hana integration tests, and the physical
   platform matrix.

## Team-review record

### Accepted refinements

- **TR-M1 — Monitor-relative recovery placement (accepted, critical).** Add a
  persistence-neutral captured placement containing the recovery monitor ID,
  logical offset from that monitor's origin, logical size, mode, and capture
  scale, plus the capture-time monitor snapshot needed to project frozen state
  into the existing RON format. Rebase the offset onto the reconnected monitor's
  current origin before computing `TargetPosition`. Treat the serialized
  index-based `WindowState` as an adapter format, not the runtime recovery model.
- **TR-M2 — One staged restore preparation path (accepted, critical).** Primary
  startup, managed startup, explicit runtime restore, and automatic return all
  attach the same private pending-restore context. A later system waits for the
  live winit window and current-monitor information, then installs
  `TargetPosition` through the correct `X11FrameCompensated` path. A recreated
  application-controlled `ManagedWindow` bypasses ordinary automatic startup
  restore until the application requests it.
- **TR-M3 — Hana output internals (re-scoped out of clerestory).** The review
  proposed a durable cable route and ephemeral output instance. Those are Hana
  responsibilities and are not specified by this clerestory design. Clerestory
  supplies only monitor identity/availability and registered-window
  save/restore/cancellation metadata; Hana owns cable, capture, content, and
  output-instance lifecycles.
- **TR-M4 — Retain identity throughout settling (accepted, important).** A
  runtime restore attempt carries its expected `MonitorId` and monitor-topology
  revision through application and settle phases. Re-resolve the current index
  after topology changes, return to the waiting phase if the target disappears,
  and accept settle success only when `CurrentMonitor.monitor_info.identity`
  still contains the expected verified ID.
- **TR-M5 — Causally gate reconstruction after linked despawn (accepted,
  important).** If the hotplug example proves that a platform destroys a
  monitor-linked window, a reconstruction adapter acts only for a pending
  recovery whose copied policy is `FallbackAndReturn` and only while a monitor
  is available. Associate the replacement with the frozen key before
  persistence runs; neither an empty all-window query nor a missing-window
  query alone authorizes reconstruction. Hana supplies any primary-specific
  content repair still required by that path.
- **TR-M6 — Protect frozen state in every persistence path (accepted,
  important).** Use one `WindowKey`-keyed captured-state authority for
  persistence and recovery. Full-map rebuilds, `ActiveOnly`, managed removal,
  persistence-mode changes, and ordinary changed-window writes preserve frozen
  entries and reject fallback placement. Remove global restore-time persistence
  pauses so unrelated windows continue saving.
- **TR-M7 — Scope DPI messages to their window (accepted, important).** Preserve
  the entity carried by each `WindowScaleFactorChanged` message and advance only
  that entity's restore attempt. Add a concurrent cross-DPI test in which only
  one of two restoring windows receives the message.
- **TR-M8 — Bound the whole restore attempt (accepted, important).** Start the
  deadline when a runtime restore request is accepted, covering winit creation,
  X11 compensation, DPI transitions, fullscreen application, and settling.
  Expiry removes attempt components and enters the documented retryable failure
  path instead of leaving persistence suppressed indefinitely.
- **TR-M9 — Classify removal before retiring recovery (accepted, important).**
  Keep recovery ownership, canonical key, and the live monitor entity
  independent of the ECS window. Record explicit close/cancel intent before
  removal and process it first; cancellation wins over a simultaneous monitor
  loss. Any unmarked removal remains recoverable even when disconnect/reconnect
  coalesces before classification. Programmatic close uses an ordered
  cancel-then-despawn operation.
- **TR-M10 — Keep restart recovery out of the initial scope (accepted,
  important).** The initial feature covers reconnect after a healthy in-process
  identity capture. Launching while the saved display is already absent remains
  a separate persistence-format feature and is excluded from the initial
  acceptance tests.
- **TR-M11 — Provide a hardware-independent transition harness (accepted,
  important).** Put recovery transitions behind crate-private functions that
  accept synthetic topology and lifecycle inputs. Use pure state tests plus
  small Bevy `App` tests for ordering and entity-scoped messages; reserve native
  identity continuity, linked despawn, compositor placement, fullscreen, and
  dock behavior for the manual platform matrix.
- **TR-M12 — Hana screen discovery (re-scoped out of clerestory).** Whether Hana
  discovers screens by events or polling is an application implementation
  choice. The clerestory requirement is limited to publishing correct monitor
  identity and availability metadata.
- **TR-M13 — Separate captured state from recovery lifecycle (accepted,
  critical).** Keep one `WindowKey`-keyed captured-state resource with
  monitor-relative placement, capture-time monitor snapshot, live binding, and
  writable/frozen status. Persistence serializes this authority; a separate
  recovery registry owns policy-specific transitions and never reconstructs
  state from live queries or the RON file.
- **TR-M14 — Use closed policy-specific transition models (accepted,
  critical).** Application-controlled and automatic-return recovery use
  separate private phase enums. Automatic return applies to primary and
  secondary registered windows. Zero displays is topology input. Each restore
  retry receives a private `RestoreAttemptId`; success or mismatch is accepted
  only when key, entity, attempt ID, expected monitor, and topology revision all
  match. Mismatch retains the frozen target and does not retry every frame.
- **TR-M15 — Define zero-display and retry behavior (accepted, important).**
  With no displays, wait without creating a window. If the verified target
  returns first, restore or reconstruct directly for it. If another display
  returns first and the live window survived, let the operating system's
  fallback stand; if linked despawn removed it, only the demonstrated,
  policy-gated reconstruction adapter may replace it. Mismatch retains the
  frozen target and any usable fallback, ends the attempt, and retries only on a
  later matching topology revision or explicit request. Tests cover coalesced
  reconnect, fallback-monitor loss, and stale results after retry.
- **TR-M16 — Keep implementation phases usable (accepted, important).** Build
  identity, captured-state authority, and the unified staged restore path before
  publishing recovery requests. Add the clerestory lifecycle/public API before
  the Hana integration pass. Use Hana to validate the save/restore and metadata
  boundary without moving capture, cable, content, or UI responsibilities into
  clerestory.

### Resolved user decisions

#### TR-D1 — Recovery-grade monitor identity and live-entity resolution

- **Status:** approved
- **Severity:** critical
- **Source dimensions:** API/type design; failure modes
- **Class:** design-improvement
- **TR-D1a decision (approved):** Automatic recovery requires verified
  physical-panel identity on every platform. The implementation provides one
  cross-platform contract backed by platform-specific identity extraction.
  Missing or ambiguous identity never falls back to connector, position, index,
  or first-display matching: the editor stays on its fallback and display-bound
  output stays inactive until the application explicitly confirms a live target.
- **TR-D1b decision (approved):** Public `MonitorInfo` exposes a self-documenting
  `MonitorIdentity` with `Verified(MonitorId)` and `Unverified` variants.
  `Unverified` intentionally combines unavailable and ambiguous identity because
  both prohibit automatic recovery; clerestory retains the detailed reason only
  for diagnostics. `MonitorId` is opaque so it can hold the complete
  cross-platform identifier. `Monitors::by_id` and `entity_by_id` resolve only
  one exact live verified match and otherwise return `None`.
- **Problem:** The current `MonitorId` mixes macOS display IDs, Windows device
  names, X11 CRTCs, Wayland output IDs, and a position hash. Those values do not
  provide one physical-panel continuity contract, and `Monitors` cannot resolve
  an accepted ID to the replacement Bevy `Monitor` entity.
- **Impact:** Automatic return can wait forever or select another display, and
  applications cannot reliably associate a reconnected physical display with
  the current Bevy monitor entity.
- **Recommendation:** Make public recovery identity mean verified physical-panel
  continuity, using full display UUID or unique panel descriptor data where the
  backend provides it. Represent missing or duplicate identity explicitly and
  disable automatic return until the application confirms a live target. Drive
  unconditional topology events from private entity-lifetime identity, never a
  position hash. Retain the live association and expose allocation-free
  `by_id`/`entity_by_id` lookups that return a result only for one exact live
  match. Applications may use the verified identity metadata to correlate their
  own subsystems; clerestory does not perform that correlation.

#### TR-D2 — Policy-specific recovery phases and generations

- **Status:** superseded — private lifecycle converged into TR-M14; canonical
  request identity moved into TR-D3
- **Severity:** critical
- **Source dimensions:** correctness; API/type design; failure modes
- **Class:** design-improvement
- **Problem:** The single proposed phase enum cannot represent an available but
  unclaimed application-controlled target, removal awaiting classification,
  zero displays, mismatch, replacement loss, repeated target loss, or stale
  requests. It also permits invalid combinations such as an
  application-controlled fallback phase.
- **Impact:** Recovery can duplicate notifications, revive an intentionally
  closed window, remain stuck, or let an old result mutate a newer attempt.
- **Recommendation:** Omit disabled entries and use policy-specific internal
  phase enums with exhaustive transitions. Include removal-pending,
  target-absent, target-available, restoring, and retryable-failure states as
  applicable; keep fallback only in the automatic-return state. Carry a
  private attempt ID on entity-bound attempts and validate it on results.

#### TR-D3 — Entity-derived restore identity

- **Status:** approved
- **Severity:** important
- **Source dimensions:** API ergonomics; correctness
- **Class:** design-improvement
- **Problem:** A caller-supplied `WindowKey` can disagree with the replacement
  entity, and current settle code treats an unrecognized entity as primary.
- **Impact:** A request can apply another window's placement or complete the
  wrong recovery record.
- **Decision (approved):** `RestoreWindow` is entity-targeted and carries no
  caller-supplied key or public recovery ticket. At acceptance, clerestory
  derives the canonical key from `PrimaryWindow` or the authoritative
  `ManagedWindowRegistry`, rejects both/neither, and validates that the key is in
  a current restorable phase. Cancellation or target absence makes a late
  request invalid. Each accepted request receives a private `RestoreAttemptId`
  so an old settle result cannot complete a newer attempt.

#### TR-D4 — One-shot per-window registration and cancellation

- **Status:** approved
- **Severity:** important
- **Source dimensions:** API ergonomics; type-system leverage; architecture
- **Class:** design-improvement
- **Problem:** The proposed policy did not distinguish a surviving window that
  the operating system relocates from a window entity removed through Bevy's
  linked monitor relationship. It incorrectly treated automatic return as a
  primary-only capability. The mutable component also advertises runtime
  changes with undefined lifecycle semantics, and an absent window has no
  entity on which to target cancellation.
- **Impact:** The API would impose an artificial primary/secondary distinction,
  obscure the actual linked-despawn risk, let ECS configuration disagree with
  the registry, and leave applications unable to retire a pending semantic
  window.
- **TR-D4a1 decision (approved):** Recovery behavior is fixed when a window
  first registers. The initial implementation does not observe live changes to
  recovery configuration. An application changes behavior by explicitly
  cancelling the existing registration and registering again. Runtime switching
  is deferred until a concrete use case warrants defining its transition rules.
- **TR-D4a2 decision (approved):** Use one `WindowRecovery` component with
  `Disabled`, `ApplicationControlled`, and `FallbackAndReturn` choices for every
  registered primary or secondary window. `FallbackAndReturn` freezes the
  verified original placement, allows the operating system to relocate a
  surviving window, and returns it when the original display reconnects. After
  the initial fallback relocation settles, a position, size, or mode change is
  treated as user/application intervention: clerestory adopts that placement
  and cancels automatic return. `ApplicationControlled` reports display loss
  and return but leaves teardown, unavailable UI, reconstruction, and re-enable
  choice to the application. Bevy linked despawn is a separate implementation
  problem rather than a primary/secondary policy distinction. The plan must add
  a dedicated `bevy_clerestory` `restore_after_reconnect` example that exposes
  monitor/window/`OnMonitor` ordering and demonstrates the selected prevention
  or reconstruction solution for both primary and secondary windows on the
  platform matrix.
- **TR-D4b1 decision (approved):** `CancelWindowRecovery { window: WindowKey }`
  globally retires recovery for a stable key even when no window entity exists.
  It invalidates any private attempt and prevents a later monitor reconnect from
  reviving an application feature the user disconnected. It does not despawn a
  surviving window; intentional removal is ordered as cancel, then despawn. A
  physical monitor disconnect alone does not cancel an application-owned route.
  The plan includes a focused `../hana` integration pass that wires this
  distinction into editor, screen, cable, output, and close behavior and serves
  as an API feedback gate before clerestory's recovery surface is stabilized.
- **TR-D4b2 decision (approved):** Cancelling recovery does not delete saved
  placement. It decides only whether clerestory may act on a future reconnect;
  `ManagedWindowPersistence::RememberAll` or `ActiveOnly` remains authoritative
  for placement retention.

#### TR-D5 — Registration-time policy versus runtime mutation

- **Status:** superseded — merged into TR-D4's one-shot registration and
  cancellation surface
- **Severity:** important
- **Source dimensions:** correctness; architecture
- **Class:** design-improvement
- **Problem:** The proposed resource and component are mutable, but enabling,
  disabling, component removal, or changing policy during any recovery phase is
  unspecified.
- **Impact:** The registry can disagree with application configuration and
  ownership of fallback creation or cancellation becomes ambiguous.
- **Recommendation:** Treat policy as registration-time configuration in the
  initial feature. Enabling captures a healthy window; explicit close/cancel
  retires the key; runtime policy switching is deferred until its complete
  transition semantics are designed.

#### TR-D6 — Reflection capability for the public recovery API

- **Status:** approved
- **Severity:** minor
- **Source dimension:** API/type design
- **Class:** design-improvement
- **Verified capability:** Bevy 0.19 BRP supports both
  `world.trigger_event` and `world.observe+watch`. The latter installs a
  reflected observer and streams triggered event values to a raw BRP client by
  server-sent events. The current local `bevy_brp` MCP client exposes event
  triggering and component watches, but does not yet expose
  `world.observe+watch` as a tool.
- **Registration:** This workspace enables `reflect_auto_register`.
  Non-generic types deriving `Reflect` are automatically inserted into the
  `AppTypeRegistry` at startup; do not add redundant `App::register_type`
  calls. Generic monomorphizations remain a manual-registration case.
- **Decision:** The local MCP client will add `world.observe+watch`, so every
  public clerestory event derives `Reflect` and includes `#[reflect(Event)]`.
  This includes `MonitorConnected`, `MonitorDisconnected`,
  `WindowRecoveryPending`, `WindowRecoveryAvailable`, `RestoreWindow`,
  `CancelWindowRecovery`, `WindowRestored`, and `WindowRestoreMismatch`.
  Notification/result events can then be observed remotely; request and
  cancellation events can be triggered remotely. Do not add explicit
  `App::register_type` calls: `reflect_auto_register` registers these
  non-generic reflected types automatically. Generic monomorphizations remain
  the manual-registration exception.

#### TR-D7 — Capture generation and first-frame readiness

- **Status:** rejected as out of scope
- **Severity:** important
- **Source dimensions:** architecture; correctness; failure modes
- **Class:** design-improvement
- **Decision:** Do not add capture identity, generations, first-frame readiness,
  feed lifecycle, or rendered-content state to clerestory. Clerestory is limited
  to window save/restore plus monitor/window metadata. Hana owns what appears in
  a window and when its capture-backed output is ready. The Hana integration
  pass may consume clerestory identity and availability events, but any capture
  API changes belong to a separate Hana design.
