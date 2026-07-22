# Restore after reconnect causal probe

This example records the boundary between native monitor enumeration, Bevy's
monitor/window relationships, and Clerestory's installed topology. Both windows
register `WindowRecovery::ApplicationControlled`. The example records factual
target loss and return, but it does not request restoration or create window
content after a linked despawn.

Run it with an external monitor index selected from the operator's physical
inventory:

```sh
CLERESTORY_PROBE_MONITOR_INDEX=1 \
  cargo run -p bevy_clerestory --example restore_after_reconnect \
  --features monitor-probe 2>&1 | tee /tmp/clerestory-hotplug-probe.log
```

The example opens the primary window and one secondary `ManagedWindow`. On
backends where clients control placement, both request centering on the selected
monitor. Wayland leaves placement to the compositor. The example removes its
process-specific persistence file under the system temporary directory before
installing `WindowManagerPlugin`, so every run starts without saved window
state even if the operating system reuses a process ID. `WindowPlugin` uses
`ExitCondition::DontExit`, so removal of the primary window or every window
cannot stop the process before the external display reconnects.

After the replug topology record has been captured, close the primary window.
The probe records that close request and then sends `AppExit::Success`. For an
automated startup smoke that does not use desktop input, set
`CLERESTORY_PROBE_EXIT_AFTER_FRAME` to the frame at which the probe should send
`AppExit::Success`:

```sh
CLERESTORY_PROBE_EXIT_AFTER_FRAME=10 \
  cargo run -p bevy_clerestory --example restore_after_reconnect \
  --features monitor-probe
```

## Ordered trace schema

Every line starts with `clerestory_probe` and contains:

| Field | Meaning |
| --- | --- |
| `sequence` | One process-wide, monotonically increasing record number. |
| `timestamp_unix_micros` | UNIX timestamp derived from one wall-clock sample plus monotonic elapsed time. |
| `frame_count` | Bevy `FrameCount`; topology records retain the producer's frame. |
| `producer` | Observer, schedule, or topology-producer label. |
| `kind` | Record category. |
| remaining fields | Structured values specific to the category. |

The custom `LogPlugin::custom_layer` adapter accepts only the
`bevy_clerestory::monitor_probe` tracing target. It visits every structured
field and forwards it to the same sequence/timestamp owner used by lifecycle
observers and window-message readers. The forwarded fields include private
monitor instance identity and evidence provenance for diagnostics only. The
example does not expose either value as a matching API.

Topology records retain `configuration_state`, `configuration_generation`,
`topology_revision`, `monitor_instance`, `evidence_provenance`,
`evidence_generation`, `monitor_identity`, `verified_monitor_id`,
`monitor_entity`, `monitor_entity_state`, and `topology_change`. Public
`MonitorConnected` and `MonitorDisconnected` records add the transition
`MonitorInfo`. An identity-only `revalidated-unchanged` record does not claim a
fresh native metadata query.

Window records include `WindowKey`, window entity, `OnMonitor` entity,
`native_current_monitor_state`, matched Clerestory entity, and matched public
identity when available. The state is `native-window-unavailable`,
`current-monitor-no-handle`, or `current-monitor-handle-returned`. The match
compares the returned native `MonitorHandle` with each current Clerestory
monitor entity's cached `WinitMonitors` handle. The probe does not format the
native handle or query its name, position, size, scale, or other arrangement
metadata. If no handle is equal, both matched fields are recorded as
`unresolved`; physical position is never used as monitor identity.

Clerestory emits `recovery-accepted` from its registration system only after
the canonical role, native readiness, startup-restore completion, captured
placement, and exact verified `OnMonitor`/`CurrentMonitor` association all pass
the core acceptance checks.
`recovery-pending` contains the canonical key and absent `MonitorId`.
`recovery-available` contains the same key and the returned `MonitorInfo`.
Repeated raw monitor events do not create repeated recovery facts for one
installed topology revision.

Synchronous lifecycle records cover `Monitor`, `HasWindows`, `OnMonitor`,
`Window`, and `ManagedWindow` add/remove/despawn hooks. `OnMonitor` also has
distinct `Discard` and `Insert` producers. `Discard` records the old `OnMonitor`
value before either replacement or removal; it does not prove relinking by
itself. Only a following `Insert` for the same window proves relationship
replacement. Initial relationship addition may produce both `Add` and `Insert`.
These records include window identity and cached-handle match results.

One `MessageReader<WindowEvent>` records OS-backed creation, movement, resize,
close request, and native destruction in the order they occur in that buffer.
Internally produced `WindowClosing` and `WindowClosed` messages use the exact
`window-closing` and `window-closed` kinds under a separate producer label.
They are buffered message evidence, not synchronous `On<Remove, Window>`
component-lifecycle evidence. Sequence values establish order within each
buffer and the time each reader records its entries; they do not claim
production order between those buffers. Close requests record only close
intent. Clerestory classifies Bevy's accepted `ClosingWindow` marker as
cancellation before the later despawn. A declined close request does not add
that marker and does not cancel recovery.

`PostUpdate::trace_window_component_changes` records component state before
Bevy winit's private `Last::changed_windows` system consumes changed `Window`
values. These snapshots are observations and are never used to assign a cause.
Bevy winit calls `create_monitors()` at the native `about_to_wait` boundary;
when a handle disappears it despawns the monitor entity, whose `HasWindows`
relationship has linked-spawn behavior.

The automated topology test installs the example's real layer factory and
shared `ProbeTrace` resource in an `App`. The scheduled Phase 3 producer emits
the revision-zero `PreStartup::init_monitors` record, then triggers the actual
example `MonitorConnected` observer, and a later injected monitor lifetime
causes the real `Update::monitor_topology_producer` path to emit a runtime
record. The test verifies their structured fields and shared sequence. The
existing Phase 3 production-order assertion remains in place.

## Manual physical script

Keep operator-confirmed facts separate from OS enumeration. Copy and complete
the run header before launching the example.

| Run field | Operator entry |
| --- | --- |
| Run ID and date | Record before launch. |
| Platform/session | Record before launch. |
| Internal display make/model/connector | Record before launch. |
| External display make/model/connector | Record before launch. |
| External display selected index | Record after the probe selects the target. |
| Primary window visibly on external display | Confirm before unplug. |
| Secondary window visibly on external display | Confirm before unplug. |
| Wayland compositor placement action | Record when applicable. |
| Exact unplug action | Record during the run. |
| Exact replug action | Record during the run. |

1. Record the physical inventory above from labels/connectors visible to the
   operator. Do not copy the OS monitor list into this table.
2. Launch the example with the selected external monitor index and retain the
   complete trace from process start.
3. On macOS, Win32, or X11, confirm both windows are visibly on that external
   display and record a failed placement as a failed run. On Wayland, use
   compositor controls if needed to place both windows on the external display,
   then record the exact compositor action in the run table. The observation
   interval begins after both window placements have been recorded.
4. Unplug exactly the recorded external cable or dock connection. Do not close,
   move, or resize either window during the observation interval.
5. Wait until the trace has recorded the native enumeration change and all
   resulting lifecycle records.
6. Record each window outcome as one of: entity survived and OS-relocated;
   entity survived without relocation; or entity removed through the
   `HasWindows` cascade. Record the last verified identity and whether
   top-level placement is available on the platform.
   Confirm exactly one `recovery-pending` record exists for each accepted
   window key, including keys whose entities were already removed.
7. Replug the exact connection recorded in step 4. Do not restart the app.
8. Wait for the runtime topology record, then record the new monitor entity,
   verified identity, configuration generation, and topology revision.
   Confirm exactly one `recovery-available` record exists for each pending key
   whose original `MonitorId` returned. No window should be reconstructed,
   re-enabled, moved, resized, or restored by this example.
9. Compare `MonitorId` only with records from this same running process.
10. After the replug evidence is complete, close the primary window if it
    survived. Confirm its close-request record precedes process exit. If the
    primary window was removed by the monitor's `HasWindows` cascade, record
    that no primary window remained to close, stop the probe from the terminal,
    and preserve the full log with the completed inventory table.

## Initial macOS evidence and mitigation conclusion

The following physical run was completed on 2026-07-20. The startup smoke and
automated ordering test remain separate from this operator-confirmed evidence.

| Run field | Operator entry |
| --- | --- |
| Run ID and date | `macos-dell-usbc-2026-07-20` |
| Platform/session | macOS on a MacBook Pro |
| Physical inventory | MacBook Pro built-in display, the selected Dell external display, and one other external display that remained connected; exact models other than Dell were not recorded |
| Selected external connection | Dell display connected by USB-C directly to the MacBook Pro |
| External display selected index | `1` at process startup |
| Primary window visibly on external display | Yes |
| Secondary window visibly on external display | Yes |
| Exact unplug action | Unplugged the Dell display's direct USB-C connection from the MacBook Pro |
| Exact replug action | Reconnected the same Dell using the same direct USB-C connection without restarting the app |
| Primary window outcome | Removed through the selected monitor entity's `HasWindows` cascade |
| Secondary window outcome | Removed through the selected monitor entity's `HasWindows` cascade |
| Reconnect outcome | Dell returned as new monitor entity `223v0`, retained process-local `MonitorId(1)`, and was assigned current index `2` |
| Probe exit | No primary window remained to close; the headless probe was stopped from the terminal after reconnect evidence was captured |

The selected Dell began as monitor entity `213v0` with verified
`MonitorId(1)`. On unplug, its `HasWindows` despawn hook still contained both
window entities (`218v0` secondary and `86v0` primary). The trace then recorded
the secondary and primary window despawns before Clerestory installed topology
revision 1 and emitted `MonitorDisconnected`. This establishes that both
windows were deleted by Bevy's linked relationship; neither window survived for
the operating system to relocate.

On replug, Bevy created monitor entity `223v0`. Clerestory observed fresh
evidence at configuration generation 6, retained verified `MonitorId(1)` for
the same physical Dell within this running process, installed topology revision
2, and emitted `MonitorConnected`. The monitor's current index changed from 1
to 2, confirming that index is not a reconnect identity. As expected after
their entity removal, neither probe window returned.

The causal trace is sufficient; temporary Bevy engine instrumentation is not
required. Later recovery cannot depend on OS relocation or relinking to save
these windows on macOS. It must handle reconstruction only for windows that the
application explicitly registered for automatic return, using the copied
application-owned state already required by the recovery plan. It must never
reconstruct an arbitrary unregistered window or infer application content.

The implemented automatic path therefore reconstructs only a copied
`FallbackAndReturn` window shell. If another monitor remains, Clerestory binds
one replacement to the retained primary or managed key and lets it settle on
the first installed monitor. If no monitor remains, it keeps the generation
entityless. When the target is the first monitor to return, Clerestory binds one
shell directly to the pending restore. `ApplicationControlled` remains the
policy used by this causal probe, so the probe continues to create no windows
after linked deletion.

Phase 16 through Phase 19 will expand this initial result into the complete
macOS, Win32, X11, and Wayland physical matrix.
