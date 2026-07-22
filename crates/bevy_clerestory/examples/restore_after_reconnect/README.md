# Restore windows after reconnect

This example opens four visible windows on one selected monitor:

- Clerestory returns the primary window and the managed automatic window
  automatically after the monitor reconnects.
- The application returns the managed application window only after it creates
  or prepares a window entity and sends `RestoreWindow` to Clerestory.
- The unregistered control window is never recreated by Clerestory.

The physical check disconnects and reconnects the same monitor twice without
restarting the application. The second cycle verifies that moving, resizing,
or changing an automatic fallback window does not change its registered target.
It then explicitly cancels that automatic recovery. The application also
cancels its application-controlled recovery.

`WindowKey` is the stable application name for a window. This document calls it
the canonical key because a newly created entity can keep the same application
identity after the previous entity was deleted. The four windows are:

| Visible window | Stable `WindowKey` | `WindowRecovery` policy | Who returns it |
| --- | --- | --- | --- |
| Primary automatic | `Primary` | `FallbackAndReturn` | Clerestory |
| Managed automatic | `Managed("hotplug-automatic")` | `FallbackAndReturn` | Clerestory |
| Managed application window | `Managed("hotplug-application")` | `ApplicationControlled` | The application, by sending `RestoreWindow` |
| Unregistered control | None | None | Nobody; Clerestory never recreates it |

The two automatic windows receive `WindowRecovery` once, after their original
`OnMonitor` and public `CurrentMonitor` values match the selected installed
`MonitorInfo`. An accepted registration is **armed**: Clerestory recorded it and
the current platform can perform that recovery policy. The unverified-identity
and windowed Wayland limits described below can leave a registration unarmed.

A replacement window is a new Bevy window entity created after the old entity
was deleted. Clerestory may create one replacement window for either automatic
canonical key. The application attaches a camera and UI root when the new
`PrimaryWindow` or `ManagedWindow` role is added; it does not add another
`WindowRecovery`. Each UI root is unparented, targets the camera for its window,
and has the same explicit window owner as the camera. Removing the window also
removes both content entities.

The application removes its application-controlled window after the monitor is
lost. On the first reconnect, it creates a replacement `ManagedWindow`, lets the
`Added<ManagedWindow>` content system prepare it, and sends `RestoreWindow`. On
the second loss, it removes the window and sends `CancelWindowRecovery`. The
ordered trace records that request. The later absence of another
`WindowRecoveryAvailable` event, replacement window, or restore request proves
that cancellation took effect.

Linked deletion means Bevy deletes windows associated through `HasWindows` with
a monitor entity that disappeared. `WindowPlugin` uses
`ExitCondition::DontExit`, so linked deletion may remove every window without
ending the process. An operating-system close request for the current primary
window still sends `AppExit::Success`.

The example writes one ordered diagnostic trace. It combines native monitor
identity, Bevy monitor/window relationships, window events, component
lifecycle, topology, recovery acceptance, application requests, and recovery
results. The trace fields and required evidence are listed below.

## Run commands

Run this command from the workspace root. Choose the external monitor's index
from the monitor inventory printed at startup, then set
`CLERESTORY_PROBE_MONITOR_INDEX` to that index:

```sh
CLERESTORY_PROBE_MONITOR_INDEX=1 \
  cargo run -p bevy_clerestory --example restore_after_reconnect \
  --features monitor-probe 2>&1 | tee /tmp/clerestory-reconnect-consumer.log
```

The command saves the complete trace in
`/tmp/clerestory-reconnect-consumer.log`. The example deletes its
process-specific persistence file before it adds `WindowManagerPlugin`, so it
does not use window state from an earlier process. Each `MonitorId` is valid only
inside the current process.

For the bounded startup check, set an exit frame. This command starts the
example and exits without desktop input:

```sh
CLERESTORY_PROBE_EXIT_AFTER_FRAME=10 \
  cargo run -p bevy_clerestory --example restore_after_reconnect \
  --features monitor-probe
```

This startup check does not disconnect a monitor and is not multi-monitor
evidence.

## Managed automatic controls

Cycle 2 uses two fixed keys. They are disabled whenever
`CLERESTORY_PROBE_EXIT_AFTER_FRAME` is set:

- Focus the window titled `Clerestory Reconnect Consumer - Managed Automatic`
  and press `B` once to set
  `WindowMode::BorderlessFullscreen(MonitorSelection::Current)`.
- Keep that window focused and press `W` once to return it to
  `WindowMode::Windowed`.
- Keep that window focused and press `C` once to send
  `CancelWindowRecovery` for `Managed("hotplug-automatic")`.

The keys affect only `Managed("hotplug-automatic")`. They do nothing when that
canonical window is absent. `B` and `W` change the window but do not change its
registered monitor target. `C` is the explicit decision to keep the fallback
window where it is and stop automatic return. After each mode keypress, wait
for its `window-component-changed` record before pressing the next key.

## Pre-unplug gate

Do not disconnect the selected monitor until the trace emits exactly one
`recovery-ready` record. The example emits it only after these public checks
pass; it does not use a timer or Clerestory's private recovery state:

1. Each of the three canonical initial windows has an `OnMonitor` entity present
   in public `Monitors`.
2. Its public `CurrentMonitor.monitor_info` equals that installed monitor's
   `MonitorInfo`.
3. The installed monitor has the selected startup index.
4. The unregistered control emitted `control-association-confirmed` for an
   `OnMonitor` entity that resolves to the same selected installed
   `MonitorInfo`.
5. The example's one-shot accepted-key set contains each canonical key, and each
   entity still has its initial `WindowRecovery` policy.
6. The trace contains the required number of `recovery-accepted` records for the
   exact key and `MonitorInfo`.
7. No `recovery-pending` record exists. That record reports monitor loss; it is
   not evidence that registration succeeded.

The example emits one `pre-unplug-association` record for each canonical key.
Each record contains the window entity, `OnMonitor` entity, full public
`CurrentMonitor`, installed `MonitorInfo`, policy, acceptance-record count, and
arming state. The separate `control-association-confirmed` record contains the
control entity, title, `OnMonitor` entity, and installed `MonitorInfo`. The
control has no canonical key, recovery policy, accepted-key entry, or
acceptance diagnostic.

For a verified target on macOS, Win32, or X11, each of the three canonical keys
must report `accepted_records=1` and `arming_state="armed"`. A different count
prevents `recovery-ready`.

Two public platform or identity branches remain unarmed:

- An unverified target emits `recovery-unarmed` with
  `unverified-monitor-identity`; its acceptance count must be zero.
- A windowed Wayland `FallbackAndReturn` key emits `recovery-unarmed` with
  `wayland-windowed-placement`; its acceptance count must be zero. The verified
  `ApplicationControlled` key still requires one acceptance record. A verified
  borderless-fullscreen Wayland target may arm automatic return, but this
  example starts in windowed mode.

An unarmed record verifies that Clerestory did not accept registration for that
recovery path. It cannot be used to claim that automatic recovery ran.

## Ordered trace schema

Every record begins with `clerestory_probe` and contains:

| Field | Meaning |
| --- | --- |
| `sequence` | One process-wide monotonically increasing record number. |
| `timestamp_unix_micros` | One wall-clock sample plus monotonic elapsed time. |
| `frame_count` | Bevy `FrameCount`; diagnostic records retain the producer frame. |
| `producer` | Observer, schedule, or Clerestory diagnostic producer. |
| `kind` | Record category. |
| remaining fields | Structured values owned by that category. |

The custom log layer accepts both diagnostic routes:

- `bevy_clerestory::monitor_probe` becomes `monitor-topology`.
- `bevy_clerestory::recovery_probe` becomes `recovery-accepted`.

Both routes use the same sequence and timestamp owner as the example's
lifecycle observers and window-message readers. Diagnostic-only monitor
instance and evidence fields remain trace data; the example never uses them to
match a monitor.

Topology records retain `configuration_state`, `configuration_generation`,
`topology_revision`, `monitor_instance`, `evidence_provenance`,
`evidence_generation`, `monitor_identity`, `verified_monitor_id`,
`monitor_entity`, `monitor_entity_state`, and `topology_change`. Public
`MonitorConnected` and `MonitorDisconnected` records include the corresponding
`MonitorInfo` transition.

Window records include the canonical `WindowKey` when one exists, window
entity, title, `OnMonitor` entity, `native_current_monitor_state`, matched
Clerestory monitor entity, and matched public identity. Native state is one of
`native-window-unavailable`, `current-monitor-no-handle`, or
`current-monitor-handle-returned`. Native handle equality is compared only with
current `WinitMonitors` handles. Position, monitor name, size, and scale are not
used as native identity.

Synchronous component records cover `Monitor`, `HasWindows`, `OnMonitor`,
`Window`, and `ManagedWindow` add, insert, discard, remove, and despawn hooks.
`OnMonitor` discard records the old value; only a following insert for the same
window proves relationship replacement. The `Window` add record includes the
title, which identifies the unregistered control without assigning it a
`WindowKey`.

One `MessageReader<WindowEvent>` records OS-backed creation, movement, resize,
close request, and native destruction in buffer order. `WindowClosing` and
`WindowClosed` use a separate producer. Sequence values preserve each reader's
iteration order and record time; they do not assert production order between
the two buffers. Recreated windows may appear in a different front-to-back
order. Record that order, but never use it as a pass or failure condition.

`PostUpdate::trace_window_component_changes` records `Window` state before
Bevy winit consumes changed values in `Last`. These records show observed
position, size, and mode transitions. They do not assign a cause.

Recovery records are:

| Kind | Required fields and interpretation |
| --- | --- |
| `recovery-accepted` | Initial canonical key, entity, monitor entity, monitor snapshot, and policy. Exactly one is permitted per armed key. |
| `pre-unplug-association` | Public association and exact acceptance count recorded before any loss fact. |
| `control-association-confirmed` | Unregistered control entity, title, `OnMonitor` entity, and matching selected installed `MonitorInfo`. It is emitted once for the process lifetime. |
| `recovery-ready` | All three canonical association checks and the unregistered control confirmation have completed. |
| `recovery-unarmed` | Public identity or Wayland windowed capability prevented acceptance. |
| `recovery-pending` | Key and absent process-local `MonitorId`; must occur after target loss. |
| `recovery-available` | Application-controlled key and returned `MonitorInfo`. |
| `recovery-restore-requested` | Application replacement or surviving entity passed to `RestoreWindow` after its `content-attached` record. |
| `recovery-restored` | Applied entity, key, position, size, mode, and current target index. |
| `recovery-mismatch` | Expected and actual position, size, mode, index, and scale. |
| `recovery-cancellation-requested` | Either the operator pressed `C` for the managed automatic window or the second application-controlled loss queued `CancelWindowRecovery`. The later behavior proves completion. |
| `content-attached` | Canonical role addition attached one application camera and UI root. It is not registration evidence. |

Both `recovery-restored` and `recovery-mismatch` are handled outcomes. A
mismatch must be retained with its expected and actual values for platform
analysis; it must not be silently discarded.

## Automated verification

Run before any physical display action:

```sh
cargo check -p bevy_clerestory --all-targets --all-features
cargo nextest run -p bevy_clerestory --all-features
cargo +nightly fmt --all -- --check
```

The example target is test-enabled. Its tests run the production content
systems and verify that
`Added<PrimaryWindow>` and `Added<ManagedWindow>` attach exactly one unparented
UI root and one window-targeted camera, target the UI at that camera, and remove
both owned entities when the window disappears. An accepted replacement
automatic window receives the same content once but receives no
`WindowRecovery` or initial-placement request. Other tests verify that the mode
keys affect only the managed automatic window, `C` requests its cancellation
exactly once, all three controls are disabled during the startup check, and
first-cycle `RestoreWindow` is sent only after content attachment. The control
and readiness tests run the production systems with a
verified installed monitor. They verify one accepted record for each canonical
key, no readiness before control confirmation, three armed
`pre-unplug-association` records, exactly one `recovery-ready` record, permanent
control confirmation, and total control exclusion from recovery. The
second-cycle consumer test records a cancellation request without claiming that
cancellation has already completed.

Clerestory's private automatic-recovery test drives primary and managed
`FallbackAndReturn` registrations through two production disconnect and
replacement cycles, changes the returned monitor index, and verifies that each
original `RecoveryGeneration` remains accepted throughout. The remaining
recovery tests reject unverified identity and windowed Wayland automatic return,
prove that later fallback geometry changes preserve the original target, prove
that explicit cancellation stops return without deleting the live fallback,
and retain `queued_return_survives_same_update_fallback_relocation` for macOS,
Win32, and X11.

## Two-cycle physical procedure

An operator must perform this procedure with a secondary display. Do not
automate the disconnect, display reconfiguration, or either reconnect cycle.
Use the same physical monitor and connector for both cycles.

### Run header

Before launching the example, read the hardware labels and connector and
complete this table. Do not copy the operating system's monitor list into the
physical inventory columns.

| Run field | Operator entry |
| --- | --- |
| Run ID and date | Record before launch. |
| Platform and desktop session | Record before launch. |
| Internal display make/model | Record before launch. |
| External display make/model | Record before launch. |
| External connector or dock path | Record before launch. |
| Selected startup index | Record from `probe-session`. |
| Selected initial monitor entity and identity | Record from the trace. |
| Initial primary entity | Record from the trace. |
| Initial managed automatic entity | Record from the trace. |
| Initial application entity | Record from the trace. |
| Initial unregistered control entity | Record from `control-association-confirmed`. |
| Wayland compositor placement action | Record when applicable. |

### Initial placement and registration

1. Launch the example with the selected external monitor index. Keep the
   complete trace from process start.
2. On macOS, Win32, or X11, confirm all four windows are visibly on the selected
   external display. On Wayland, use compositor controls to place all four
   windows on that display and record the exact action. Visible placement alone
   does not satisfy the pre-unplug gate.
3. Wait for `control-association-confirmed`, then `recovery-ready`. Save the
   control record and all three `pre-unplug-association` records into the run
   evidence. For each canonical window, confirm that its `OnMonitor` entity and
   `CurrentMonitor.monitor_info` name the same selected installed monitor. For
   the control, confirm that its `OnMonitor` entity resolves to that same
   installed monitor. A missing or mismatched control record invalidates the
   run.
4. For an armed branch, confirm exactly one `recovery-accepted` record for each
   key and identity, and zero `recovery-pending` records. For an unarmed branch,
   record the exact `recovery-unarmed` reason and acceptance count. Do not claim
   automatic return for an unarmed key.
5. Confirm one `content-attached` record for each canonical initial entity.
   Confirm the control has no canonical key or recovery policy and that no
   `recovery-accepted` record exists for its entity or title.

### Cycle 1: automatic return

6. Disconnect only the external monitor connection recorded in the run header.
   Do not move, resize, change mode, or close a window while it is disconnected.
7. Wait for the topology and lifecycle records. For every armed key, confirm
   one `recovery-pending` follows monitor loss. Record whether each entity
   survived, was moved by the operating system, or was removed by linked
   deletion through `HasWindows`.
8. If linked deletion occurred, confirm that Clerestory created exactly one
   replacement window for each armed automatic key and assigned it the same
   canonical key. Each replacement must have one `content-attached` record and
   no new
   `recovery-accepted` record. Confirm the application window and unregistered
   control are absent.
9. Reconnect the same display through the same connector without restarting
   the process. Record the new monitor entity, current index, retained
   process-local `MonitorId`, configuration generation, and topology revision.
10. Do not change either armed automatic window. Confirm that each eligible
    surviving or replacement window returns. Retain its `recovery-restored` or
    `recovery-mismatch` record. If fallback movement and monitor return occur in
    one update, retain those trace records and confirm that the queued return
    still completes.
11. Confirm the application key emits `recovery-available`, then
    `recovery-restore-requested`, then one result record. If linked deletion
    occurred, the request entity must be the replacement created by the example.
    Its canonical `ManagedWindow` addition must precede its content attachment,
    and it must not receive `WindowRecovery`.
12. Confirm no new window with the unregistered control title is created. If
    the control survived instead of being removed by linked deletion, record
    that platform behavior without claiming deletion recovery.
13. Record recreated native window front-to-back order as an observation only.

### Cycle 2: explicit automatic and application cancellation

14. Disconnect the same physical display through the same connector a second
    time. Wait until the automatic fallback windows are visible before
    interacting with them.
15. Leave the automatic primary untouched. Click the managed automatic fallback
    to focus it, optionally move and resize it, then press `B` once. Wait for the borderless-fullscreen
    `window-component-changed` record while keeping the same window focused.
    Press `W` once and wait for the windowed record before continuing. Record
    the moved, resized, and mode records. These actions alone do not change the
    registered monitor target. Press `C` once and confirm a
    `recovery-cancellation-requested` record for
    `Managed("hotplug-automatic")`. Do not use the keys while another window
    has focus.
16. Confirm the second application `recovery-pending` record precedes
    `recovery-cancellation-requested`. This record proves only that the request
    was queued. Confirm the application window is absent before the monitor
    returns. The unregistered control must remain absent after linked deletion.
17. Reconnect the same physical display through the same connector without
    restarting. The untouched automatic primary must follow its eligible
    return path. The explicitly cancelled managed automatic window must remain
    on its fallback monitor and must not perform an exact return. Record the
    result and component transitions for each outcome.
18. Confirm the application key emits no second-cycle
    `recovery-available`, creates no replacement, sends no restore request, and
    attaches no content. That absence proves the cancellation request took
    effect. Confirm the unregistered control is not recreated.
19. Confirm neither automatic key emitted another `recovery-accepted` record;
    each accepted generation remains the initial one even if the monitor's
    current list index changed.
20. Close the current primary window. Confirm its `close-intent` record precedes
    `AppExit::Success`. If no primary exists because the platform followed an
    unarmed branch, stop the process from its terminal and record that action.

### Evidence matrix

| Platform branch | Automatic primary | Managed automatic | Application controlled | Unregistered control | Explicit cancellation result |
| --- | --- | --- | --- | --- | --- |
| macOS, verified windowed | Eligible return; exactly one replacement window after linked deletion | Eligible return; exactly one replacement window after linked deletion | First-cycle replacement/restore; second-cycle cancellation | Never recreated after linked deletion | Cancelled fallback remains after cycle 2 |
| Win32, verified windowed | Same requirements as macOS | Same requirements as macOS | Same requirements as macOS | Same requirements as macOS | Same requirements as macOS |
| X11, verified windowed | Same requirements as macOS | Same requirements as macOS | Same requirements as macOS | Same requirements as macOS | Same requirements as macOS |
| Wayland, verified windowed | Unarmed; no automatic-return claim | Unarmed; no automatic-return claim | First-cycle restore and second-cycle cancellation remain testable | Never recreated after linked deletion | Not applicable to unarmed automatic keys |
| Any platform, unverified target | Unarmed | Unarmed | Unarmed | No registry behavior | End the recovery portion after recording the unarmed evidence |

## Earlier macOS lifecycle evidence

The original raw probe ran on 2026-07-20 with a Dell display connected directly
to a MacBook Pro by USB-C. The selected display began as entity `213v0` with
verified `MonitorId(1)` and index `1`. Disconnecting it caused Bevy's linked
`HasWindows` relationship to delete both probe windows. Reconnecting the same
display created entity `223v0`, retained `MonitorId(1)`, and assigned index `2`.

That run established the linked-deletion branch and proved that list index is
not reconnect identity. It predates this permanent recovery consumer. New
macOS, Win32, X11, and Wayland results must use the two-cycle script above and
must preserve the earlier raw trace categories.
