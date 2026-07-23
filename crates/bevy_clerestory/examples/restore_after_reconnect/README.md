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
the current platform can perform that recovery policy. The unverified-identity,
exclusive-fullscreen, and windowed Wayland limits described below can leave a
registration unarmed.

A replacement window is a new Bevy window entity created after the old entity
was deleted. Clerestory may create one replacement window for either automatic
canonical key. The application attaches a centered Hana Diegetic diagnostics
panel when any of the four probe-window roles appears; it does not add another
`WindowRecovery`. Each panel explicitly targets its own window and identifies
the window even when fullscreen mode removes its title bar. It shows the role,
recovery behavior, original target, Bevy and native current-monitor facts,
window mode, position, and size. Removing the window also removes its panel.

The panel refreshes only after a displayed window or monitor-association fact
changes. It does not poll native monitor metadata every frame. Hana Diegetic
owns the per-window overlay camera used to draw the panel.

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

## Startup mode selector

`CLERESTORY_PROBE_STARTUP_MODE` selects the managed automatic window's initial
`WindowMode` deterministically at launch:

| Value | Initial managed automatic mode |
| --- | --- |
| `windowed` (default when unset) | `WindowMode::Windowed` |
| `borderless` | `WindowMode::BorderlessFullscreen(MonitorSelection::Index(selected))` |
| `exclusive` | `WindowMode::Fullscreen(MonitorSelection::Index(selected), VideoModeSelection::Current)` |

Any other value is rejected at startup with an error naming the variable. The
selector affects only `Managed("hotplug-automatic")`; the primary, application,
and unregistered control windows always start windowed. Both fullscreen values
target the selected startup monitor index, and `exclusive` keeps that monitor's
current video mode. The `probe-session` record retains the selected mode in its
`startup_mode` field using the selector spelling (`windowed`, `borderless`, or
`exclusive`).

Like `CLERESTORY_PROBE_MONITOR_INDEX`, the selector is a run-local startup
input only. Continuity and pass decisions use the verified `MonitorId`, never
the selector, monitor entity, index, or enumeration order.

Mode expectations:

- A verified windowed or monitor-targeted borderless start may arm automatic
  return under `FallbackAndReturn` on macOS, Win32, or X11. On Wayland only the
  borderless start may arm; a verified windowed start stays unarmed.
- Verified automatic exclusive-fullscreen recovery is unarmed on every
  platform. Record the exclusive branch's `recovery-unarmed`/acceptance
  evidence, and test any explicit or startup exclusive-fullscreen restore as
  its own row — never count it as automatic return.
- The runtime `B`/`W` keys below still change the managed automatic window
  after any startup mode, and no mode change replaces the registered monitor
  target.

## Managed automatic controls

Cycle 2 uses two fixed keys. They are disabled whenever
`CLERESTORY_PROBE_EXIT_AFTER_FRAME` is set:

- Focus the window titled `Clerestory Reconnect Consumer - Managed Automatic`
  and press `B` once to set
  `WindowMode::BorderlessFullscreen(MonitorSelection::Current)`.
- Keep that window focused and press `W` once to return it to
  `WindowMode::Windowed`.
- Keep that window focused and press `Shift+C` once to send
  `CancelWindowRecovery` for `Managed("hotplug-automatic")`.

The keys affect only `Managed("hotplug-automatic")`. They do nothing when that
canonical window is absent. `B` and `W` change the window but do not change its
registered monitor target. `Shift+C` is the explicit decision to keep the fallback
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

For a verified target on macOS, Win32, or X11 with a windowed or borderless
managed automatic window, each of the three canonical keys must report
`accepted_records=1` and `arming_state="armed"`. A different count prevents
`recovery-ready`.

Three public platform, mode, or identity branches remain unarmed:

- An unverified target emits `recovery-unarmed` with
  `unverified-monitor-identity`; its acceptance count must be zero.
- A verified exclusive-fullscreen `FallbackAndReturn` key emits
  `recovery-unarmed` with `exclusive-fullscreen-return` on every platform; its
  acceptance count must be zero. The other verified canonical keys still
  require one acceptance record each.
- A verified windowed Wayland `FallbackAndReturn` key emits `recovery-unarmed`
  with `wayland-windowed-placement`; its acceptance count must be zero. The
  classification reads each window's mode: a verified borderless-fullscreen
  Wayland target may arm automatic return, and the verified
  `ApplicationControlled` key still requires one acceptance record.

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
| `recovery-unarmed` | Public identity, exclusive-fullscreen, or Wayland windowed capability prevented acceptance. |
| `recovery-pending` | Key and absent process-local `MonitorId`; must occur after target loss. |
| `recovery-available` | Application-controlled key and returned `MonitorInfo`. |
| `recovery-restore-requested` | Application replacement or surviving entity passed to `RestoreWindow` after its `content-attached` record. |
| `recovery-restored` | Applied entity, key, position, size, mode, and current target index. |
| `recovery-mismatch` | Expected and actual position, size, mode, index, and scale. |
| `recovery-cancellation-requested` | Either the operator pressed `Shift+C` for the managed automatic window or the second application-controlled loss queued `CancelWindowRecovery`. The later behavior proves completion. |
| `content-attached` | A probe-window role received its centered diagnostics panel. It is not registration evidence. |

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

The example target is test-enabled. Its tests verify the startup mode selector
without physical monitor manipulation: each documented
`CLERESTORY_PROBE_STARTUP_MODE` value parses (defaulting to `windowed`, with
undocumented values rejected by an error naming the variable), both fullscreen
values target the selected monitor index, the selector changes only the managed
automatic window at spawn, and the runtime `W` key still overrides a
fullscreen startup mode. Its tests also run the production content system and
verify that each of the four probe-window roles receives exactly one unparented,
centered Hana Diegetic panel targeted to that window. They verify that role
removal and re-addition does not duplicate the panel, window removal cleans it
up, and a borderless window remains identifiable without a title bar. An
accepted replacement automatic window receives the same content once but no
`WindowRecovery` or initial-placement request. Other tests verify that the mode
keys affect only the managed automatic window, `Shift+C` requests its cancellation
exactly once, all three controls are disabled during the startup check, and
first-cycle `RestoreWindow` is sent only after content attachment. The control
and readiness tests run the production systems with a
verified installed monitor. They verify one accepted record for each canonical
key, no readiness before control confirmation, three armed
`pre-unplug-association` records, exactly one `recovery-ready` record, permanent
control confirmation, and total control exclusion from recovery. Further
readiness tests verify that a verified exclusive-fullscreen automatic key
records `recovery-unarmed` with zero acceptances while `recovery-ready` still
appears, and that on Wayland a verified borderless-fullscreen automatic key
arms while the verified windowed primary key stays unarmed. The
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
| Tested source revision | Record `git rev-parse HEAD` before launch. |
| Platform and desktop session | Record before launch. |
| Internal display make/model | Record before launch. |
| External display make/model | Record before launch. |
| External connector or dock path | Record before launch. |
| Selected startup index | Record from `probe-session`. |
| Selected startup mode | Record from `probe-session` (`startup_mode`). |
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
5. Confirm one `content-attached` record for each of the four initial probe
   entities. Confirm the control record says `unregistered`, the panel says it
   has no recovery, and no `recovery-accepted` record exists for its entity or
   title.

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
    registered monitor target. Press `Shift+C` once and confirm a
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

## macOS physical matrix

This section is the macOS release evidence. The retained historical rows below
are supporting evidence only; they predate the startup mode selector and do not
substitute for a fresh post-selector core row recorded against its tested
source revision. The bounded `CLERESTORY_PROBE_EXIT_AFTER_FRAME` startup check
never substitutes for a physical row.

### Recording rules

- Each `MonitorId` is a process-local token. Compare it only across entity
  lifetimes inside one running `App`. Never persist a token and never compare
  token values across separate runs; cross-run continuity is recorded as
  hardware evidence, not token equality.
- Monitor entity or list-index equality is never continuity evidence, and
  recreated front-to-back window order is recorded as an observation, never a
  pass or failure condition.
- Every row records: qualified-evidence availability and provenance,
  `Verified`/`Unverified` state, window entity survival versus linked cascade,
  captured-position state, the supported return mechanism, the expected
  action, the actual action, pass/fail, and the tested source revision.
- Transition/`App`-test proof is labeled separately from the physical
  observation; an automated assertion never stands in for the physical row.
- Each cascade-capable row additionally records: whether window removal
  preceded the installed disconnect topology, whether the process stayed alive
  with no windows, each replacement entity and its canonical key, and whether
  the unregistered control remained absent. A backend that relocates surviving
  windows instead records its surviving branch; macOS cascade ordering is not
  assumed.
- Moving, resizing, or changing a fallback window's mode does not replace its
  registered target; only explicit `CancelWindowRecovery` stops its automatic
  return.
- If a row exposes a defect, fix only the owning monitor/recovery/restore
  module, add an automated regression where possible, rerun the Clerestory
  Build/Test/Lint gates, and record the corrected result. If the correction
  can affect an earlier row, repeat that row or explicitly revalidate it on the
  corrected revision before treating it as release evidence.
- Hardware or a safely observable setup that is not available is marked
  unavailable in the row — never inferred from another row.

### Scenario matrix

Actual/result/revision entries are completed by the operator procedure above;
`Pending operator run` marks a row not yet executed on the current source.

| # | Scenario | Startup mode | Expected action | Actual action and result | Tested source revision |
| --- | --- | --- | --- | --- | --- |
| 1 | Same-panel reconnect, same connector (fresh post-selector core row; two-cycle procedure) | `windowed` | Verified `MonitorId` continuity across entity churn; cascade-capable facts recorded; one replacement per armed automatic key; both automatic keys return; application key restores on request in cycle 1 and honors cancellation in cycle 2; control never recreated; clean exit from primary close | **Pass.** See `macos-usbc-windowed-2026-07-22` below. | `a6a2086113a9108be4db33e1bb1c6ee59a557bc2` plus the Phase 14 worktree changes |
| 2 | Same panel through another port or dock | `windowed` | Same panel evidence re-verifies to the same process-local `MonitorId`; pending keys return; provenance records the changed path | **Pass.** The Dell returned through a different MacBook USB-C port; all three eligible windows returned exactly once. See `macos-usbc-other-port-2026-07-23`. | `a6a2086113a9108be4db33e1bb1c6ee59a557bc2` plus the Phase 14 worktree changes |
| 3 | Different same-model panel at the same position (where available) | `windowed` | Different serial evidence yields a different `MonitorId`; pending keys do not return to the substitute panel | **Unavailable hardware.** The operator has one DELL S3425DW and one Samsung C34J79x; there is no second same-model Dell. | Not run |
| 4 | Simultaneous duplicate identities (where available) | `windowed` | Duplicate evidence stays `Unverified`; no fallback matching by connector, position, index, or first monitor; no automatic return claims | **Unavailable hardware.** The two available external displays are different models, so identical evidence cannot be presented simultaneously. | Not run |
| 5 | Identity change: different panel on the original connector | `windowed` | New panel gets a distinct `MonitorId`; the connector is not identity; pending keys stay pending | **Pass.** The Samsung appeared through the Dell's former USB-C port as its own `MonitorId(2)`; the Dell's `MonitorId(1)` keys stayed pending. See `macos-panel-swap-2026-07-23`. | `a6a2086113a9108be4db33e1bb1c6ee59a557bc2` plus the Phase 14 worktree changes |
| 6 | Lid close/open (built-in display loss and return) | `windowed` | Built-in identity re-verifies on reopen; keys targeting the built-in follow their return path; external-target keys unaffected | **Pass.** Closing the lid removed the built-in display and placed exactly the two automatic replacements on the Samsung. Reopening it returned both plus the application-controlled replacement to the same verified built-in identity. See `macos-lid-close-open-2026-07-23`. | `a6a2086113a9108be4db33e1bb1c6ee59a557bc2` plus the corrected Phase 14 worktree changes |
| 7 | Repeated dock churn (several disconnect/reconnect cycles) | `windowed` | Each accepted generation stays the initial one; exactly one replacement per armed key per cascade; every eligible return completes each cycle | **Pass.** Three Dell disconnect/reconnect cycles in one process retained the initial automatic registrations, created one replacement per automatic key on every loss, and completed one return per key on every reconnect. See `macos-three-cycle-churn-2026-07-23`. | `a6a2086113a9108be4db33e1bb1c6ee59a557bc2` plus the corrected Phase 14 worktree changes |
| 8 | Arrangement-only reorder (no monitor added/removed) | `windowed` | No recovery initiation; record that no Clerestory monitor-lifetime signal was produced; same-entity arrangement changes are not refreshed | **Pass.** macOS moved the existing windows and Clerestory re-verified the same panel identities. Topology revision stayed 0; no connect, disconnect, pending recovery, or replacement occurred. See `macos-arrangement-only-2026-07-23`. | `a6a2086113a9108be4db33e1bb1c6ee59a557bc2` plus the Phase 14 worktree changes |
| 9 | Zero displays (all displays absent at once) | `windowed` | Linked deletion may remove every window; process stays alive with no windows; recovery state survives the entities; returns complete on reconnect | **Unavailable on this setup.** The MacBook's built-in display disappears only when the lid is closed. Removing both external displays at the same time would leave no interactive display for observing the process and may suspend the laptop. This result is not inferred from the lid-close or external-disconnect rows. | Not run |
| 10 | Non-target-first return (another monitor returns before the target) | `windowed` | No return to the non-target monitor; return executes only when the verified target identity is installed | **Pass.** The Samsung returned first with no recovery. All three eligible windows returned exactly once only after the Dell reappeared. See `macos-panel-swap-2026-07-23`. | `a6a2086113a9108be4db33e1bb1c6ee59a557bc2` plus the Phase 14 worktree changes |
| 11 | Rapid/coalesced hotplug | `windowed` | Coalesced revisions are consumed once per installed revision; one revision may carry several lifetime events; queued returns still complete; no duplicate replacements | **Pass; macOS delivered two distinct revisions rather than coalescing them.** The operator unplugged and reconnected within about two seconds. Clerestory created one replacement per automatic key and restored the three eligible windows exactly once, with no duplicates. See `macos-rapid-hotplug-2026-07-23`. | `a6a2086113a9108be4db33e1bb1c6ee59a557bc2` plus the corrected Phase 14 worktree changes |
| 12 | Borderless startup mode | `borderless` | Verified monitor-targeted borderless may arm; the managed automatic window returns to the target in borderless mode; `B`/`W` transitions do not change the registered target | **Pass after correction.** The managed automatic window left fullscreen on the built-in display, moved to the Dell as a window, and entered native borderless fullscreen there with no title bar or menu bar. See `macos-borderless-corrected-2026-07-23`; the preceding failures are retained below it. | `a6a2086113a9108be4db33e1bb1c6ee59a557bc2` plus the corrected Phase 14 worktree changes |
| 13 | Exclusive startup mode | `exclusive` | Automatic exclusive-fullscreen recovery remains unarmed (record the acceptance/unarmed evidence); any explicit or startup exclusive restore is recorded separately, never as automatic return | **Pass.** The exclusive managed window had zero accepted registrations, disappeared with the target, and was not reconstructed or returned. The independently armed primary and application-controlled windows still returned. See `macos-exclusive-unarmed-2026-07-23`. | `a6a2086113a9108be4db33e1bb1c6ee59a557bc2` plus the corrected Phase 14 worktree changes |
| 14 | Cross-DPI reconnect (target and fallback at different scale factors) | `windowed` | Cross-DPI restore phases complete on the returned target with correct scale; DPI handling stays isolated to the matching entity/attempt; result record retained even on mismatch | **Pass in row 1.** The automatic replacements ran on the scale-2 built-in display and returned to the scale-1 Dell. All three cycle-1 results were `recovery-restored`, with no `recovery-mismatch`. | `a6a2086113a9108be4db33e1bb1c6ee59a557bc2` plus the Phase 14 worktree changes |

Row 1 doubles as the windowed-mode row. Row 12 reuses the two-cycle procedure
with a different startup mode. Row 13 ends after the first reconnect because
the exclusive managed window is unarmed and absent; it has no automatic
recovery generation to exercise in a second cycle. An application-created
exclusive restore would be a separate test, not an automatic-return result.
Rows 3 and 4 depend on a second same-model panel; mark them
`Unavailable hardware` if none exists.

### Fresh post-selector result

#### `macos-usbc-windowed-2026-07-22`

The run began on July 22 and closed on July 23, 2026. It used a MacBook Pro
with an Apple M2 Max, its built-in 3456×2234 Liquid Retina XDR display, a
DELL S3425DW connected directly by USB-C, and a second 3440×1440 external
display reported by macOS as C34J79x. The Dell was the selected target. The
tested source revision was
`a6a2086113a9108be4db33e1bb1c6ee59a557bc2` with the Phase 14 startup-selector
worktree changes.

The run started in `windowed` mode with the Dell at index 1, scale 1.0,
entity `242v0`, and `Verified(MonitorId(1))`. The built-in display was scale
2.0. macOS supplied current-generation qualified evidence for each connection;
disconnect records retained the evidence from the monitor's last installed
generation. The three recovery keys each had one accepted registration and
the control had none. The initial entities were primary `96v0`, managed
automatic `247v0`, application controlled `248v0`, and unregistered control
`249v0`. Each registered window had captured windowed geometry at
`(-2936, -990)`, size `800×540`, and the trace reached `recovery-ready` before
the first disconnect.

On the first disconnect, linked deletion removed all four original windows
before the installed disconnect at topology revision 1. The process stayed
alive with no original windows. Clerestory created primary replacement `260v0`
and managed automatic replacement `261v0` exactly once; each received its
content exactly once and no new registration. The application window and
unregistered control remained absent. Both automatic replacements ran on the
scale-2 built-in display while the Dell was absent.

The Dell returned as entity `266v0`, index 2, with the same process-local
`MonitorId(1)`, current-generation evidence, and topology revision 2. The
application created replacement `267v0`, attached its content, and then sent
`RestoreWindow`. Primary `260v0`, managed automatic `261v0`, and application
controlled `267v0` each emitted one `recovery-restored` result on the scale-1
Dell. No result was a mismatch. The operator confirmed that exactly those
three windows were visible on the Dell and that the unregistered control was
absent. This scale-2 fallback to scale-1 return is also the result for row 14.

On the second disconnect, linked deletion again preceded the installed
disconnect, now topology revision 3. Clerestory created primary replacement
`270v0` and managed automatic replacement `271v0` exactly once. The
application immediately requested cancellation after its second pending
record and did not create another replacement. The control remained absent.
The operator moved and resized both automatic fallback windows, including
moving the primary to the other external display. The managed automatic
window then changed to borderless fullscreen and back to windowed before the
operator pressed `C`. The trace recorded the managed automatic cancellation.
These geometry and mode changes did not replace either initial registered
target.

The Dell returned as entity `276v0`, index 2, with the same process-local
`MonitorId(1)`, current-generation evidence, and topology revision 4. Primary
`270v0` emitted one `recovery-restored` result and was visibly back on the
Dell. Managed automatic `271v0` stayed on the built-in display where the
operator had left it. The application-controlled and unregistered windows
remained absent, and the trace contained no second application availability,
replacement, restore request, or content attachment. No key emitted another
acceptance record. The operator then closed the primary window; the trace
recorded its close request and the process exited successfully with status 0.

Physical observation and trace assertions agree: same-panel continuity,
linked-deletion replacement, automatic return, application-controlled return,
both cancellations, control exclusion, cross-DPI return, and clean shutdown
all passed. Recreated front-to-back order was not graded.

#### `macos-usbc-other-port-2026-07-23`

This run started with the DELL S3425DW connected directly to one MacBook USB-C
port, then returned it through a different MacBook USB-C port without changing
the monitor-end connection. The tested source was
`a6a2086113a9108be4db33e1bb1c6ee59a557bc2` with the Phase 14 worktree changes.

The target began as entity `242v0`, index 1, scale 1.0, with
`Verified(MonitorId(1))` and current-generation qualified evidence. All three
keys had one armed registration, the control had none, and the operator saw
all four initial windows on the Dell. Disconnect through the first port caused
linked deletion, one pending record per key, and exactly one primary
replacement (`260v0`) and managed automatic replacement (`261v0`). The
application and control windows were absent.

The Dell returned through the second port as entity `266v0`, index 2, with the
same process-local `MonitorId(1)` and fresh current-generation evidence at
topology revision 2. The application created replacement `267v0` and requested
its restore after content attachment. All three eligible windows emitted one
`recovery-restored` result with no mismatch. The operator saw exactly those
three windows on the Dell and no control window. Closing the primary produced
a clean status-0 process exit. The different connector path did not become the
panel's identity.

#### `macos-panel-swap-2026-07-23`

This run used the same hardware and source as the preceding runs. It began
with the Dell at entity `242v0`, index 1, `Verified(MonitorId(1))`, and the
Samsung at entity `243v0`, index 2, `Verified(MonitorId(2))`. All four initial
windows were visibly on the Dell and all three recovery keys were armed.

The operator first disconnected the Dell. Linked deletion removed the four
original windows, produced one pending record per key, and created primary
replacement `260v0` and managed automatic replacement `261v0` exactly once.
The operator then disconnected the Samsung, leaving only the built-in display.
No recovery result was emitted.

The Samsung was connected through the MacBook USB-C port previously occupied
by the Dell. It returned as entity `266v0`, index 1, with fresh qualified
evidence and its original process-local `MonitorId(2)`. The Dell's keys stayed
pending: there was no recovery availability, restore request, restored result,
or mismatch. The operator confirmed that both automatic fallback windows
remained on the MacBook display and no probe window moved to the Samsung. This
is the passing result for row 5.

The Dell was then connected through the Samsung's former MacBook USB-C port.
It returned second as entity `267v0`, index 2, with fresh qualified evidence
and its original process-local `MonitorId(1)`. Only then did the application
create replacement `268v0` and request its restore. The two automatic windows
and the application window each emitted one `recovery-restored` result with no
mismatch. The operator saw exactly those three windows on the Dell and no
control window. This is the passing result for row 10. Closing the primary
ended the process with status 0. The two display cables remained swapped after
the run.

#### `macos-arrangement-only-2026-07-23`

This run started all four windows on the Dell with the same source and hardware
as the preceding runs. Without disconnecting either cable, the operator moved
the Samsung to a different relative position in macOS Display Settings.

macOS moved the existing probe windows as part of the desktop rearrangement,
but their window entities and monitor relationships survived. The display
notification caused Clerestory to re-verify the three existing monitor
identities against configuration generation 3. Each diagnostic record said
`revalidated-unchanged`, and topology revision remained 0. There was no public
monitor connection or disconnection, no pending recovery, no restore result,
and no replacement content. The operator confirmed that all four existing
windows remained on the Dell. Closing the primary ended the process with
status 0. The result confirms that arrangement alone does not begin recovery;
it does not add live arrangement tracking.

#### `macos-borderless-pre-fix-2026-07-23`

This run started the managed automatic window in borderless fullscreen on the
Dell. Its accepted target was `Verified(MonitorId(1))` at index 1. After the
Dell was disconnected, Clerestory created managed automatic replacement
`260v0` on the built-in display in
`BorderlessFullscreen(MonitorSelection::Index(0))`.

The operator used Mission Control once because the fallback initially left the
menu bar visible. The window then covered the menu bar. The Bevy trace recorded
no later resize or mode change from that interaction, and it recorded no
recovery cancellation.

When the Dell returned at index 2, Clerestory attempted the managed automatic
restore and changed the component mode to
`BorderlessFullscreen(MonitorSelection::Index(2))`. The primary automatic and
application-controlled windows returned visibly to the Dell, but the managed
automatic window remained on the built-in display. Its retained result was a
`recovery-mismatch`: the expected target was index 2 at scale 1.0 with size
`3440×1440`, while the native window still reported index 0 at scale 2.0 with
size `3456×2168`. Mission Control may have affected macOS fullscreen
presentation, but it did not cause Clerestory to skip or cancel the restore.
The corrected implementation must be rerun without Mission Control before row
12 can pass.

#### `macos-borderless-corrected-2026-07-23`

This run used the same Dell target and began with the managed automatic window
in borderless fullscreen. The Dell was entity `396v0`, index 1, scale 1.0,
with `Verified(MonitorId(1))`. All three recovery keys were armed. The tested
source was `a6a2086113a9108be4db33e1bb1c6ee59a557bc2` plus the corrected Phase 14
worktree changes.

Disconnecting the Dell created primary replacement `522v0` and managed
automatic replacement `521v0` on the built-in display. The managed replacement
was borderless fullscreen at index 0. Its native position was `(0, 66)` and its
size was `3456×2168`, leaving the macOS menu bar visible. The operator did not
interact with the window. The trace recorded no duplicate cleanup error and no
recovery cancellation.

The Dell returned as entity `577v0`, index 2, scale 1.0, with the same
process-local `MonitorId(1)` at topology revision 2. Clerestory changed the
managed replacement to windowed mode on the built-in display, requested a
centered position on index 2, and waited until its current monitor was the
Dell. Only then did it request borderless fullscreen on index 2. The native
window finished at `(-4256, -1440)`, size `3440×1440`, on entity `577v0`, and
the trace emitted `recovery-restored` for the managed key. The primary and
application-controlled windows also emitted one restored result each on the
Dell. The operator visually confirmed the managed borderless window on the
Dell.

This ordering corrects the preceding failure: macOS is no longer asked to
switch directly from fullscreen on one display to fullscreen on another. The
automated regression requires the target monitor to be current before the
borderless request is made.

That correction moved the window to the Dell, but a later visual check exposed
a second problem: macOS sometimes left the returned window with a gray title
bar and the menu bar visible. The Bevy `Window` already said
`BorderlessFullscreen`, so component state alone could not prove that macOS had
finished presenting the window as fullscreen. Waiting for AppKit's completed
fullscreen notifications prevented Clerestory from reporting an early result,
but did not fix the presentation.

The remaining difference from startup was window activation. During startup,
winit requests fullscreen and then makes the new native window the key window.
Its runtime fullscreen update requests the mode but does not perform that
second step. Clerestory's macOS return path now follows the same order for the
returning window: request borderless fullscreen, make that exact native window
key and front within the application, then wait for AppKit to confirm the
completed transition on the target display. It does not activate the whole
application or take focus from another application.

The final physical rerun used managed replacement `527v0`. While the Dell was
absent, it was borderless on the built-in display at `(0, 66)`, size
`3456×2168`, with the menu bar visible. The operator did not interact with it.
That fallback presentation is accepted: the window remains borderless
fullscreen, and Clerestory does not force it to become windowed merely to hide
or preserve the built-in display's menu bar. The return presentation is the
graded behavior.
The Dell returned as entity `585v0`, index 2, scale 1.0, with the same
process-local `MonitorId(1)` at topology revision 2. Clerestory changed the
managed window to windowed mode, moved it to the Dell, requested borderless
fullscreen, and waited for the native completion. The window finished at
`(-4256, -1440)`, size `3440×1440`, and emitted one `recovery-restored` result.
The operator confirmed that it covered the Dell with neither a title bar nor
the macOS menu bar visible. The primary and application-controlled windows also
emitted one restored result each. This is the passing row 12 result.

#### `macos-exclusive-unarmed-2026-07-23`

This run used the same Dell target and began with managed automatic entity
`405v0` in exclusive fullscreen. The Dell was entity `398v0`, index 1, scale
1.0, with `Verified(MonitorId(1))`. The tested source was
`a6a2086113a9108be4db33e1bb1c6ee59a557bc2` plus the corrected Phase 14
worktree changes.

Primary `98v0` and application-controlled `406v0` each received one accepted
registration. Managed automatic received zero and emitted `recovery-unarmed`
with reason `exclusive-fullscreen-return`. The trace reached `recovery-ready`,
and the operator confirmed that the managed panel reported `Fullscreen` on the
Dell before disconnecting it.

Disconnecting the Dell removed all four original windows through the monitor's
linked window relationship. Clerestory created only primary replacement
`527v0`. Pending records existed only for Primary and Application Controlled;
there was no managed-automatic pending record or replacement. The operator
confirmed that only Primary Automatic was visible on the built-in display.
winit logged that it could not restore the removed exclusive display's native
mode while tearing down the old window. That native cleanup warning did not
create a recovery registration, replacement, or result.

The Dell returned as entity `556v0`, index 2, scale 1.0, with the same
process-local `MonitorId(1)` at topology revision 2. Primary replacement
`527v0` and application replacement `557v0` each emitted one
`recovery-restored` result. The operator confirmed that exactly those two
windows were visible on the Dell and Managed Automatic remained absent. This
is the expected automatic exclusive-fullscreen result: Clerestory makes no
claim that it can reconstruct and return the exclusive window.

#### `macos-lid-close-open-2026-07-23`

This run targeted the MacBook Pro's built-in display while both external
monitors remained connected. The built-in display began as entity `397v0`,
index 0, scale 2.0, with `Verified(MonitorId(0))`. All three recovery keys had
one accepted registration, and the trace reached `recovery-ready`. The tested
source was `a6a2086113a9108be4db33e1bb1c6ee59a557bc2` plus the corrected Phase 14
worktree changes.

The operator closed the lid while using a Bluetooth keyboard and mouse, and
macOS kept the application running in clamshell mode. Removing the built-in
display deleted all four original windows. Clerestory created primary
replacement `527v0` and managed automatic replacement `528v0` exactly once on
the Samsung, entity `399v0`, `Verified(MonitorId(2))`. The operator confirmed
that those two automatic windows were visible there. The application-controlled
window and unregistered control were absent.

Opening the lid installed the built-in display as entity `585v0`, index 2,
scale 2.0, with the same process-local `MonitorId(0)` at topology revision 2.
The application created replacement `586v0` and requested its restore after
content attachment. Primary `527v0`, managed automatic `528v0`, and application
controlled `586v0` each emitted one `recovery-restored` result on the built-in
display. The operator confirmed all three there, and the unregistered control
remained absent. The application process stayed alive throughout the lid
transition.

#### `macos-three-cycle-churn-2026-07-23`

This run targeted the Dell in windowed mode for three complete
disconnect/reconnect cycles without restarting the process or changing either
automatic fallback window. The Dell began as entity `398v0`, index 1, scale
1.0, with `Verified(MonitorId(1))`. Each of the three keys received exactly one
accepted registration before the first disconnect. The tested source was
`a6a2086113a9108be4db33e1bb1c6ee59a557bc2` plus the corrected Phase 14
worktree changes.

Cycle 1 created managed automatic replacement `527v0` and primary replacement
`528v0` exactly once on the built-in display. The Dell returned as entity
`585v0`, index 2, with the same identity at topology revision 2. Both automatic
windows and application replacement `586v0` emitted one `recovery-restored`
result, matching the operator's three-window observation.

Cycle 2 created managed automatic replacement `566v1` and primary replacement
`565v1` exactly once. The example cancelled its application-controlled key on
this second loss. The Dell returned as entity `489v1`, index 2, with the same
identity at topology revision 4. Each automatic replacement emitted one
restored result, and the operator saw only those two windows on the Dell.

Cycle 3 created managed automatic replacement `564v2` and primary replacement
`563v2` exactly once. The Dell returned as entity `611v1`, index 2, with the
same identity at topology revision 6. Each automatic replacement again emitted
one restored result, matching the operator's observation. No key emitted a new
acceptance record in any cycle. The automatic keys kept their original
recovery registrations across all monitor and window entity changes.

#### `macos-rapid-hotplug-2026-07-23`

This run targeted the Dell in windowed mode. It began as entity `398v0`, index
1, scale 1.0, with `Verified(MonitorId(1))`. The tested source was
`a6a2086113a9108be4db33e1bb1c6ee59a557bc2` plus the corrected Phase 14
worktree changes.

The operator unplugged the Dell and reconnected it within about two seconds.
macOS did not combine the two changes into one notification: Clerestory
installed one disconnect at topology revision 1, then installed one reconnect
about 5.6 seconds later at revision 2. The disconnect created managed
automatic replacement `527v0` and primary replacement `528v0`, each exactly
once.

The Dell returned as entity `585v0`, index 2, with the same process-local
`MonitorId(1)`. The application created replacement `586v0`. Primary `528v0`,
managed automatic `527v0`, and application controlled `586v0` each emitted one
`recovery-restored` result. The operator confirmed that exactly those three
windows were visible on the Dell. There were no duplicate replacements or
restore results. This run proves the rapid physical sequence on macOS; it does
not claim that a coalesced operating-system notification was observed.

### Retained historical rows

These rows are retained unchanged as supporting evidence. Each predates the
startup mode selector and is not recast by later results.

#### Phase 4 — raw hotplug probe (2026-07-20, Dell over direct USB-C)

The original raw probe ran with a Dell display connected directly to a MacBook
Pro by USB-C. The selected display began as entity `213v0` with verified
`MonitorId(1)` and index `1`. Disconnecting it caused Bevy's linked
`HasWindows` relationship to delete both probe windows. Reconnecting the same
display created entity `223v0`, retained `MonitorId(1)`, and assigned index
`2`. That run established the linked-deletion branch and proved that list
index is not reconnect identity. It predates this permanent recovery consumer.

#### Phase 7 — application-controlled registration row

Primary and secondary accepted `MonitorId(1)` on entity `232v0` at index 1.
Disconnect emitted one pending fact for each key. Reconnect installed entity
`242v0` at index 2 with the same `MonitorId(1)` and emitted one available fact
for each key. The process stayed alive and both application windows
intentionally remained absent.

#### Phase 8 — automatic policy row (pre-execution)

Both windows accepted `MonitorId(1)` on entity `233v0` at index 1. Linked
deletion emitted one pending fact per key before topology revision 1.
Reconnect installed entity `243v0` at index 2 with the same ID in revision 2.
No window was reconstructed because Phases 9–10 were not yet implemented.

#### Phase 9 — application-controlled execution row

The target began as entity `237v0`, `MonitorId(1)`, at index 1. Disconnect
revision 1 removed the primary and managed windows and emitted one pending
fact per key. Reconnect revision 2 installed entity `247v0` with the same ID
at index 2. The application created managed replacement `248v0` and primary
replacement `249v0`; both completed runtime restoration on index 2 with no
mismatch.

#### Phase 10 — reconstruction and relocation row

Linked deletion rebuilt the primary and secondary exactly once on the
remaining external monitor. The returned panel kept the same verified
`MonitorId` despite monitor entity/index churn, and both windows returned. The
secondary's 77-physical-pixel OS relocation before queued-return consumption
did not cancel its return. Its changed front-to-back order is permitted, not a
failure.

#### Phase 12 — completed two-cycle automatic-return row

Three recovery keys accepted target entity `241v0`, `MonitorId(1)`, at index
1. The first linked deletion removed all four original windows, created
automatic replacements `259v0` and `260v0`, then returned both plus
application replacement `266v0` when the target reappeared as entity `265v0`
at index 2; the unregistered control stayed absent. The second cycle created
automatic replacements `269v0` and `270v0`, explicitly cancelled the
application-controlled key and managed automatic key, then returned only
primary `269v0` when the target reappeared as entity `275v0` at index 2. The
process exited cleanly from the primary close request.

New macOS, Win32, X11, and Wayland results must use the two-cycle script above
and must preserve the earlier raw trace categories.
