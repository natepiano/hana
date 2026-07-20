# Restore windows after monitor reconnect

> **Status: IMPLEMENTATION PLAN — phased, delegate-ready.** Add verified
> monitor identity and per-window reconnect recovery to `bevy_clerestory`, then
> integrate the public boundary into Hana.

## Delegation Context

- **Project:** `bevy_clerestory` (`crates/bevy_clerestory`) — Bevy plugin for
  persisted window restoration and multi-monitor management; this work also
  integrates its recovery API into the sibling `hana` visualization
  application.
- **Stack:** Rust 2024; `bevy` 0.19.0 with `reflect_auto_register`,
  `bevy_window`, and `bevy_winit`; `winit` 0.30.13; locked `ron` 0.12.1 and
  `serde` 1.0.228; native macOS, Win32, X11, and Wayland monitor/window APIs.
  Clerestory is `0.2.0-dev`; sibling Hana currently consumes published
  `bevy_clerestory` 0.1.1 and must temporarily use this checkout before moving
  to the resulting release.
- **Layout:**
  - `docs/bevy_clerestory/` — recovery plan and topology contract.
  - `crates/bevy_clerestory/src/monitors/` — monitor identity, topology, and
    current-window domain established in Phase 1.
  - `crates/bevy_clerestory/src/persistence/` — RON format and the planned
    captured-state authority.
  - `crates/bevy_clerestory/src/restore/` — shared startup/runtime target
    preparation, application, settling, and attempts.
  - `crates/bevy_clerestory/src/recovery/` — planned registration and
    policy-specific lifecycle domain.
  - `crates/bevy_clerestory/examples/restore_after_reconnect/` — planned raw
    hotplug probe, complete recovery example, manual script, and physical
    evidence matrix.
  - `../hana/crates/hana/src/{window_recovery.rs,screens/,conduit/}` — real
    editor, display, cable, and output consumers.
- **Key files:**
  - `Cargo.toml` — Clerestory workspace versions/features, including Bevy
    0.19.0 and exact winit 0.30.13.
  - `Cargo.lock` — locked Clerestory dependency graph.
  - `crates/bevy_clerestory/Cargo.toml` — package version, features, examples,
    and platform-native dependencies.
  - `crates/bevy_clerestory/src/lib.rs` — public re-exports, plugin assembly,
    observers, and ordered system-chain registration.
  - `crates/bevy_clerestory/src/constants.rs` — restore stability/timeout and
    cross-platform restore constants.
  - `crates/bevy_clerestory/src/events.rs` — existing restore result events and
    reflected event type data.
  - `crates/bevy_clerestory/src/managed.rs` — canonical
    `ManagedWindowRegistry`, registration deduplication, persistence hooks, and
    managed startup restoration.
  - `crates/bevy_clerestory/src/monitors/mod.rs` — monitor plugin and public
    domain re-exports established in Phase 1.
  - `crates/bevy_clerestory/src/monitors/current_monitor.rs` —
    `CurrentMonitor`, live-window detection, effective mode, and refresh.
  - `crates/bevy_clerestory/src/monitors/identity/` — verified/unverified
    identity domain: `mod.rs` exports, `registry.rs` ambiguity/interner policy,
    `native.rs` qualified evidence, `edid.rs` panel evidence checks, and
    `configuration/` native generation listeners.
  - `crates/bevy_clerestory/src/monitors/topology.rs` — installed monitor
    entity/identity snapshot, cached-winit ordering, topology revision,
    revision-zero startup events, raw lifetime events, and exact lookup.
  - `crates/bevy_clerestory/src/monitors/monitor_probe.rs` — disabled-by-default
    structured `PreStartup`/`Update` topology trace projected from private
    identity evidence and producer state.
  - `crates/bevy_clerestory/src/persistence/mod.rs` — persistence exports and
    registration.
  - `crates/bevy_clerestory/src/persistence/load.rs` — current RON loading;
    refactor to one `PreStartup` read.
  - `crates/bevy_clerestory/src/persistence/save.rs` — current live-query
    persistence; replace with dirty-batch captured-state projection.
  - `crates/bevy_clerestory/src/persistence/format.rs` — versioned RON
    encoding/decoding and compatibility tests.
  - `crates/bevy_clerestory/src/persistence/window_state.rs` — persisted
    `WindowState` and saved window modes.
  - `crates/bevy_clerestory/src/persistence/captured_window_state.rs` —
    **planned/new** `CapturedWindowStates`, placement/persistence/live states,
    promotion, freezing, and projection.
  - `crates/bevy_clerestory/src/restore_window_config.rs` — current
    startup-loaded state holder; consolidate with the one-read captured
    authority.
  - `crates/bevy_clerestory/src/restore/mod.rs` — restore plugin and shared
    system registration.
  - `crates/bevy_clerestory/src/restore/winit_info.rs` — primary `PreStartup`
    loading/preparation and required flush before monitor movement.
  - `crates/bevy_clerestory/src/restore/settle_state.rs` — settle comparison and
    success/mismatch emission.
  - `crates/bevy_clerestory/src/restore/target_position/mod.rs` — target
    position exports.
  - `crates/bevy_clerestory/src/restore/target_position/target.rs` —
    `TargetPosition` and `compute_target_position`.
  - `crates/bevy_clerestory/src/restore/target_position/monitor.rs` — target
    monitor/index resolution.
  - `crates/bevy_clerestory/src/restore/target_position/application.rs` —
    `restore_windows`, geometry/fullscreen application, and cross-DPI phases.
  - `crates/bevy_clerestory/src/restore/target_position/strategy.rs` — DPI and
    fullscreen restore state machines.
  - `crates/bevy_clerestory/src/restore/target_position/run_conditions.rs` —
    pending-target run conditions.
  - `crates/bevy_clerestory/src/restore/restore_attempt.rs` — **planned/new**
    runtime preparation, attempt identity/context, topology replanning,
    validation, timeout, and finalization.
  - `crates/bevy_clerestory/src/platform.rs` — platform detection, coordinate
    capability, fullscreen capability, and restore strategy.
  - `crates/bevy_clerestory/src/x11_position_fix.rs` — X11 frame compensation
    between preparation and application.
  - `crates/bevy_clerestory/src/windows_dpi_fix.rs` — Windows DPI messages that
    must remain isolated to the matching entity/attempt.
  - `crates/bevy_clerestory/src/recovery/mod.rs` — **planned/new** recovery
    plugin, public API re-exports, and domain registration.
  - `crates/bevy_clerestory/src/recovery/registration.rs` — **planned/new**
    one-shot component-add generations, canonical binding, acceptance,
    cancellation, and close/removal classification.
  - `crates/bevy_clerestory/src/recovery/application_controlled.rs` —
    **planned/new** application-controlled lifecycle and explicit restore
    handling.
  - `crates/bevy_clerestory/src/recovery/fallback_and_return.rs` —
    **planned/new** fallback settling, intervention, capability gating, and
    retry lifecycle.
  - `crates/bevy_clerestory/examples/restore_after_reconnect/main.rs` —
    **planned/new** hotplug probe and complete recovery example.
  - `crates/bevy_clerestory/examples/restore_after_reconnect/README.md` —
    **planned/new** manual script and physical evidence matrix.
  - [`docs/bevy_clerestory/monitor-events.md`](monitor-events.md) — current
    topology contract to reconcile with verified identity and entity-bearing
    events.
  - `crates/bevy_clerestory/README.md` — public recovery behavior and API.
  - `crates/bevy_clerestory/CHANGELOG.md` — release-facing recovery changes.
  - `../hana/Cargo.toml` — Hana workspace dependency and temporary override.
  - `../hana/Cargo.lock` — Hana local-override and final-release resolution.
  - `../hana/crates/hana/Cargo.toml` — Hana consumer dependency declaration.
  - `../hana/crates/hana/src/main.rs` — editor construction and plugin setup.
  - `../hana/crates/hana/src/window_recovery.rs` — current missing-window
    respawn heuristic, primary reconstruction/egui rebinding, and close
    behavior.
  - `../hana/crates/hana/src/screens/mod.rs` — screen observer/system
    registration.
  - `../hana/crates/hana/src/screens/connection.rs` — raw monitor
    reconciliation and feed availability.
  - `../hana/crates/hana/src/screens/panel.rs` — monitor metadata and
    panel/output association.
  - `../hana/crates/hana/src/conduit/mod.rs` — cable/output observer and system
    registration.
  - `../hana/crates/hana/src/conduit/jack.rs` — `InputJack` monitor-entity
    ownership and reconnect rebinding.
  - `../hana/crates/hana/src/conduit/window.rs` — unmanaged
    borderless-fullscreen output lifecycle and `MonitorSelection::Entity`.
  - `../hana/crates/hana/src/conduit/cable.rs` — cable retirement and recovery
    cancellation.
- **Build:** Phase-local from this root:
  `cargo check -p bevy_clerestory --all-targets --all-features`. From
  `../hana`: `cargo check -p hana --all-targets --all-features`. Final
  CI-parity builds: `cargo build --release --workspace --all-features --examples`
  in each workspace.
- **Test:** Clerestory phases:
  `cargo nextest run -p bevy_clerestory --all-features`. Hana phases from
  `../hana`: `cargo nextest run -p hana --all-features`. Final workspace gate
  in each workspace: `cargo nextest run --all-features --workspace --tests`;
  Hana's `.config/nextest.toml` supplies its WASM exclusions.
- **Lint:** Run the full `clippy` skill in the affected workspace;
  cross-workspace phases run it here and from `../hana`.
  `/plan:delegate` supplies `auto-proceed`.
- **Style:**
  `zsh ~/.claude/scripts/rust_style/load-rust-style.sh --project-root /Users/natemccoy/rust/bevy_clerestory_reconnect`
- **Invariants:** Both origins are owned by `natepiano`/`hanallc`, so use
  `cargo +nightly fmt` and never plain `cargo fmt`; use `cargo nextest run` for
  tests. `MonitorId` is a process-local, append-only, nonpersisted token for
  complete verified evidence; ambiguity remains unverified and matching never
  falls back to connector, position, index, or first monitor. `MonitorInfo`
  stays entity-free; live/former entities exist only in live views and raw
  topology events. RON is read once, `CapturedWindowStates` is authoritative,
  fallback freezing is per `WindowKey`, and unchanged updates perform no
  persistence read/projection/write or identity allocation. Each non-disabled
  component addition creates one copied, one-shot recovery generation;
  mutation/removal does not alter it, cancellation invalidates it, and only
  cancel/remove/add creates a new generation. Restore identity comes from
  exactly one canonical primary/managed binding; every attempt validates key,
  entity, attempt ID, expected monitor, and topology revision and fully clears
  attempt state on termination. `ApplicationControlled` reports facts and acts
  only after an accepted request; `FallbackAndReturn` arms only with verified
  identity plus restorable coordinates or supported monitor-targeted
  fullscreen. `CapturedWindowPosition::CompositorControlled` never yields
  `WindowPosition::At`; Wayland windowed placement is compositor-owned,
  borderless fullscreen may target an output, and exclusive fullscreen is not a
  Wayland return mechanism. Preserve the explicit topology → recovery →
  current-monitor → window-transition → preparation → X11 → application →
  settling → persistence ordering and required deferred flushes. Public
  non-generic events derive `Reflect`/event type data and rely on
  `reflect_auto_register`; do not add redundant `register_type` calls. Preserve
  the exact `bevy_clerestory::monitors::*` `TypePath` values for
  `CurrentMonitor`, `MonitorId`, `MonitorInfo`, `Monitors`,
  `MonitorConnected`, and `MonitorDisconnected`; every new public reflected
  monitor type uses that same namespace and extends the exact-path regression
  test. `MonitorTopologyRevision` versions only the installed entity and
  identity-to-entity topology: startup is revision zero, identity-only changes
  may advance it without a raw event, and one revision may carry several
  lifetime events. Recovery consumes the installed snapshot once per revision,
  never raw event counts. Same-entity arrangement, resolution, origin, size,
  and scale changes are not refreshed by Clerestory and never start recovery.
  Keep
  transition/`App` tests and native physical evidence as separate gates. Keep
  Hana cable routes, capture sessions, rendered content, readiness, and
  re-enable/UI policy in Hana; Clerestory owns only monitor/window metadata,
  retained placement, persistence, recovery transitions, and restore
  application. New types live with their constructor/mutator/behavior owner;
  do not add generic `types.rs` or `state.rs` buckets. Launch-while-absent
  recovery and persisted physical identity remain out of scope.

## Phases

### Phase 1 — Establish the monitor module boundary  · status: done (`c461c6c2`)

#### Work Order

**Goal:** Replace the flat monitor modules with the planned domain layout
without changing observable monitor or restore behavior.

**Spec:**

- Move `MonitorPlugin` and domain re-exports to `monitors/mod.rs`.
- Move `CurrentMonitor`, effective-mode calculation, live-window monitor
  detection, and its refresh system to `monitors/current_monitor.rs`.
- Move existing `MonitorInfo`, `Monitors`, topology deltas, and raw events to
  `monitors/topology.rs`; move the current identity representation to
  `monitors/identity.rs` as the starting point for Phase 2.
- Preserve all existing public paths through re-exports and preserve plugin and
  schedule registration in `lib.rs`.
- Delete `src/monitors.rs` and `src/monitor.rs` in the same change; do not leave
  parallel flat and directory modules.
- This phase is structural only. Do not introduce recovery behavior or change
  persistence/restore semantics.

**Files:**

- `crates/bevy_clerestory/src/lib.rs` — point module declarations and re-exports
  at the new domain.
- `crates/bevy_clerestory/src/monitors.rs` — remove after moving its contents.
- `crates/bevy_clerestory/src/monitor.rs` — remove after moving its contents.
- `crates/bevy_clerestory/src/monitors/mod.rs` — create domain root.
- `crates/bevy_clerestory/src/monitors/current_monitor.rs` — create from the
  existing current-monitor owner.
- `crates/bevy_clerestory/src/monitors/identity.rs` — create with moved identity
  code.
- `crates/bevy_clerestory/src/monitors/topology.rs` — create with moved topology
  and event code.

**Constraints from prior phases:** None.

**Acceptance gate:** Phase-local Clerestory Build, Test, and Lint commands from
Delegation Context are green; module/public-path tests remain unchanged; neither
flat source file remains.

#### Retrospective

**What worked:**

- `monitors/{mod,current_monitor,identity,topology}.rs` now owns the monitor
  domain while `lib.rs` retains the existing exports and schedule wiring.
- The existing behavior tests and the `restore_window` example passed unchanged.

**What deviated from the plan:**

- The split required custom Bevy `#[type_path]` attributes and an exact-path
  regression test to preserve the six existing reflection and BRP names.

**Surprises:**

- Rust re-exports preserve source paths but do not preserve a derived Bevy
  `TypePath`; the physical declaration module determines that metadata.

**Implications for remaining phases:**

- Monitor-domain refactors must retain the legacy
  `bevy_clerestory::monitors::*` reflected paths, and new public reflected
  monitor types must use that same public module path.

### Phase 1 Review

- The Delegation Context and Phases 2–3 now carry the exact reflected monitor
  namespace contract established by Phase 1.
- Phase 3 now defers the diagnostic bridge needed for Phase 4 to trace private
  identity evidence without turning it into a matching API; Phase 4 names both
  monitor owners that may supply that bridge.
- Phase 7 now defers the canonical reflected namespace for the new recovery API
  until that public metadata decision is approved.
- Phase 11 now includes the topology event owner and exact legacy monitor-path
  assertions in its reflection gate.
- Phases 2–21 remain necessary and in their existing order.

### Phase 2 — Add verified monitor identity and live-entity resolution  · status: done (`11e37182`)

#### Work Order

**Goal:** Expose a safe physical-panel identity contract while keeping monitor
entity lifetimes explicit and topology events unconditional.

**Spec:**

Use these public types:

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Reflect)]
pub struct MonitorId(u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Reflect)]
pub enum MonitorIdentity {
    Verified(MonitorId),
    Unverified,
}
```

- Retain the six existing custom monitor-domain `TypePath` values and their
  exact-path regression test. Give `MonitorIdentity` the
  `bevy_clerestory::monitors::MonitorIdentity` reflected path and extend that
  test.

- `MonitorId` is an opaque process-local token for complete qualified evidence,
  never persistence data. A private, App-lifetime, append-only interner assigns
  checked monotonic `u64` values independently of evidence hashes, compares
  complete evidence, and never deletes or reuses a token. Exhaustion yields
  `Unverified` plus a diagnostic.
- Cache identity per monitor instance. An unchanged refresh performs no
  variable-size evidence extraction, allocation, or interning.
- `Unverified` combines unavailable and ambiguous identity publicly; keep the
  detailed cause private for diagnostics.
- Reject placeholders and values that identify an adapter, connector,
  compositor object, or CRTC rather than one physical panel. Once evidence is
  duplicated or contradicts an accepted instance, keep that evidence ambiguous
  for the process lifetime; disconnecting one duplicate never promotes the
  survivor or revives an old token.
- Backend qualification is: Core Graphics display UUID on macOS rather than
  `CGDirectDisplayID`; stable panel descriptor/serial evidence on Windows and
  X11 rather than device name or RandR CRTC; Wayland remains unverified unless
  the compositor exposes equivalent stable panel metadata, and `wl_output`
  identity alone is insufficient.
- `MonitorInfo` exposes `identity: MonitorIdentity` and remains entity-free.
  Physical matching never falls back to connector, geometry, index, or first
  monitor.
- `CurrentMonitor` refresh compares the complete `MonitorInfo`, including an
  ambiguity downgrade from `Verified` to `Unverified`, rather than only index
  and window mode. Phase 3 makes that refresh same-frame ordered with topology.
- Use a separate private `MonitorInstanceId` for unconditional deltas across
  one Bevy monitor-entity lifetime.
- Raw connect/disconnect events continue for unverified displays. Add
  `entity: Entity` to `MonitorConnected` and `former_entity: Entity` to
  `MonitorDisconnected` without storing stale entities in `MonitorInfo`.
- Expose current entities through this named iterator item and exact lookup:

```rust
pub struct LiveMonitor<'a> {
    pub entity: Entity,
    pub monitor_info: &'a MonitorInfo,
}

impl Monitors {
    pub fn iter(
        &self,
    ) -> impl ExactSizeIterator<Item = LiveMonitor<'_>> + '_;
}
```

  `Monitors::by_id` and `entity_by_id` return only one live exact verified
  match.

**Files:**

- `crates/bevy_clerestory/Cargo.toml` — adjust native dependencies only where
  verified evidence requires them.
- `crates/bevy_clerestory/src/lib.rs` — preserve/re-export the resulting public
  monitor surface.
- `crates/bevy_clerestory/src/platform.rs` — expose only platform capabilities
  needed by identity qualification.
- `crates/bevy_clerestory/src/monitors/mod.rs` — domain exports.
- `crates/bevy_clerestory/src/monitors/current_monitor.rs` — propagate complete
  identity and metadata changes into `CurrentMonitor`.
- `crates/bevy_clerestory/src/monitors/identity.rs` — evidence qualification,
  ambiguity memory, interner, and identity tests.
- `crates/bevy_clerestory/src/monitors/topology.rs` — entity-free snapshots,
  instance identity, events, iterator, and resolvers.

**Constraints from prior phases:** Phase 1 established `monitors/` as the only
owner of monitor-domain code. Its six existing reflected types retain their
exact `bevy_clerestory::monitors::*` paths; new public reflected monitor types
use that namespace and extend the exact-path regression test.

**Acceptance gate:** Phase-local Clerestory Build, Test, and Lint are green.
Synthetic tests prove monotonic nonreused tokens, full-evidence equality,
permanent duplicate ambiguity, no convenience matching, exact `by_id`/
`entity_by_id` behavior, and entity-bearing raw events for both verified and
unverified monitors. A `CurrentMonitor` test proves a duplicate-evidence
ambiguity downgrade cannot leave a stale verified identity.

#### Retrospective

**What worked:**

- `MonitorIdentityRegistry` now assigns nonreused process-local tokens only to
  complete qualified evidence and permanently remembers ambiguity.
- `MonitorInfo`, `LiveMonitor`, the exact resolvers, raw entity-bearing events,
  and `CurrentMonitor` now carry one entity-safe verified/unverified contract.
- The native configuration listeners keep callbacks minimal, revalidate cached
  evidence after OS changes, and retain callback state safely through teardown
  failures.

**What deviated from the plan:**

- `monitors/identity.rs` became an `identity/` module tree so evidence
  qualification, registry policy, and macOS/Windows/X11 notification ownership
  have separate maintainers.
- Reliable cached identity required native configuration-generation listeners,
  bounded revalidation, private `thiserror` diagnostics, and target-gated
  shutdown/cleanup tests beyond the originally named evidence extraction.

**Surprises:**

- winit supplies live monitor handles but not the stable physical-panel
  identity needed here; Core Graphics UUIDs and qualified EDID/display-config
  evidence remain platform-owned work.
- Windows notification teardown needs an independently owned waitable event and
  explicit process-lifetime quarantine when a native cleanup boundary fails.
- The live macOS smoke run enumerated three verified monitor entities even
  though no external monitor was expected to be physically connected, so OS
  enumeration and the physical test setup must be recorded separately.

**Implications for remaining phases:**

- Phase 3 must preserve the generation-aware cache boundary: a triggered build
  in the same configuration uses cached identity, while a changed configuration
  revalidates before the revised topology is installed and observed.
- Phase 3 scheduling must keep native callbacks world-free and make topology,
  identity, and `CurrentMonitor` changes visible together from Bevy systems.
- Phase 4 still needs a real connect/disconnect run; the automated tests and
  startup smoke prove identity and lifecycle safety but not physical hotplug.

### Phase 2 Review

- Delegation Context and Phases 3–4 and 15–19 now name the shipped
  `monitors/identity/` module owners instead of the deleted flat file.
- Phase 3 now extends the existing topology builder, instance deltas, events,
  and resolvers with entity/identity topology revision and ordering work.
- Phase 3 now preserves the world-free native generation boundary and tests
  both generation-driven same-frame identity change and identical rebuilds.
- Phase 4 now reuses the existing example logging as groundwork while requiring
  an operator-recorded physical inventory and explicit unplug/replug action.
- Phase 3's existing diagnostic-projection decision and the Phase 4 trace now
  include the configuration state/generation used by topology production.
- Phase 6 now preserves `ClerestoryPreStartupSet::MonitorsInitialized` before
  shared restore preparation and verifies that startup boundary.
- Phase 11 now retains the exact reflected paths for the complete public
  monitor surface introduced through Phases 1–3.
- Phase 14 and the physical evidence phases now state that `MonitorId` survives
  entity lifetimes only within one running `App` and is never persisted or
  compared across runs.

### Phase 3 — Order monitor lifetime topology and identity revalidation  · status: done (`f46dfa75`)

#### Work Order

**Goal:** Make actual monitor removal/reconnect and stable-identity
revalidation install one ordered entity/identity topology before observers,
current-monitor refresh, restore application, or settling consume it.

**Spec:**

- Extend Phase 2's existing `build_monitors`, generation-aware identity cache,
  private instance-keyed deltas, entity-bearing raw events, and exact resolvers;
  do not rebuild parallel topology or identity machinery.
- This feature handles monitor disappearance and return. Bevy 0.19 creates a
  new `Monitor` entity when winit newly enumerates a handle and despawns the
  entity when that handle disappears. Use those entity lifetimes as the
  deterministic connect/disconnect signal. A returning monitor's new Bevy
  `Monitor` component supplies its origin, size, and scale. On a triggered
  build, Clerestory assigns its index from Bevy's cached `WinitMonitors` order,
  matching `MonitorSelection::Index`. This is a cached lifetime association,
  not a live native topology or metadata refresh.
- Make topology production signal-driven. Before scanning monitor entities,
  check for `Added<Monitor>`, `RemovedComponents<Monitor>`, or a changed
  `MonitorConfiguration` state/generation that requires stable-identity
  revalidation. If none occurred, return before any monitor scan, winit/native
  handle lookup, identity-registry call, snapshot allocation, revision work, or
  current-window lookup.
- On a trigger, scan `Query<(Entity, &Monitor)>` once and build the installed
  snapshot from those component values. Keep deterministic entity and event
  ordering. Associate each entity with its cached `WinitMonitors` handle and
  index without calling `available_monitors()` or reading handle properties.
  Do not perform a second native monitor enumeration for ordering, origin,
  size, resolution, geometry, or scale. Native property lookups in this path
  are only for stable physical identity evidence. If a component temporarily
  has no cached handle association, keep its identity unverified and place it
  after the cached index range in deterministic entity order; the assigned
  out-of-range index must not select or claim an observed native monitor.
- Preserve Phase 2's macOS, Windows, and X11 configuration callbacks and
  cfg-gated injected tests. Their purpose is stable-identity revalidation when
  native handles or their qualified evidence may have changed. On X11, keep
  only the RandR configuration-notification listener; do not add a direct
  XCB/RandR current-topology snapshot, XSettings DPI parsing, root
  `RESOURCE_MANAGER` tracking, or XFixes selection-owner tracking.
- Add `MonitorTopologyRevision` as the version of the installed entity and
  identity-to-entity topology used by recovery. It may advance for monitor
  entity lifetime or identity mapping changes. It does not claim continuous
  observation of arrangement, resolution, origin, size, or scale changes while
  the same monitor entity stays present. An identical generation
  revalidation does not advance it.
- Emit `MonitorConnected` and `MonitorDisconnected` only for private
  monitor-instance lifetime deltas. An identity-only revalidation may update
  `Monitors` and `MonitorTopologyRevision`, but emits no raw lifetime event.
- Install `Monitors` and the new revision before triggering raw topology events.
  Observers must immediately resolve the live entity promised by a connect
  event and see the ended lifetime absent after a disconnect. If the topology
  becomes empty, remove stale `CurrentMonitor` components in the same queued
  world operation before disconnect observers run.
- Keep the identity-aware `CurrentMonitor` refresh after topology installation
  so a new entity's component metadata and identity mapping become visible in
  the same update. Run that refresh only for a changed installed topology or a
  window whose creation, move, resize, scale, position, registration, or mode
  changed; an idle window does no native `current_monitor()` lookup.
- Keep `MonitorConfiguration` registered once by `MonitorPlugin`. Native
  callbacks remain world-free and only advance atomic generation state; a
  changed generation revalidates all live instances before building and
  comparing the snapshot. Stable evidence continuity may retain one verified
  physical identity across entity lifetimes, while changed qualified evidence
  must never let a different panel inherit the former verified identity.
- Add an optional `monitor-probe` Cargo feature, disabled by default. With that
  feature enabled, emit structured tracing records only when the topology
  producer performs add/remove work, installs an identity-changing
  revalidation, or explicitly records an unchanged identity revalidation.
  Records may carry the configuration state/generation, installed revision,
  private instance identity, evidence provenance, public identity, current or
  former entity, `FrameCount`, schedule label, and transition kind.
- Keep the probe record diagnostic-only: it is tracing output, not a public
  recovery event, resolver, matching input, or stable serialization format.
  Normal builds and applications neither enable nor depend on it; all recovery
  and matching decisions continue to use only `MonitorIdentity` and
  `MonitorId`. A record must not imply that connected-monitor geometry or scale
  was freshly queried.
- Preserve evidence provenance in the diagnostic projection. A failed
  revalidation may retain prior evidence inside the identity registry for safe
  token continuity, but the record must distinguish evidence observed for the
  current generation from evidence retained from a named earlier generation
  and from no evidence; it must never label retained evidence as newly observed.
- An idle update performs no topology or native-monitor work. After a topology
  or configuration signal, scan live Bevy monitor entities once and consult
  cached `WinitMonitors` lifetime associations only during that build. The
  evidence registry's exact `HashMap<QualifiedEvidence, usize>` remains
  expected `O(1)` per evidence lookup, with its large-history regression. A
  triggered rebuild performs no identity extraction/interning or variable-size
  identity allocation for an instance whose identity remains cached for the
  observed configuration state.
- A configuration-generation change that rebuilds an identical entity/identity
  snapshot does not increment `MonitorTopologyRevision` or emit raw events, but
  `monitor-probe` emits a structured `revalidated-unchanged` record carrying
  the retained installed revision. Every probe record also carries the
  producer's `FrameCount` and schedule label so Phase 4 can merge it into one
  ordered trace.
- Order restore application and settling after `CurrentMonitor` refresh so a
  restore consumer cannot read the previous topology-derived monitor state in
  the same frame.
- The application owns whether and how to recreate a window after reconnect.
  Do not add debounce timers, delayed metadata settling, per-update topology
  polling, or global native-topology reconstruction. Arrangement-only,
  resolution-only, and scale-only changes on an unchanged live monitor entity
  may produce no Clerestory topology change and must not start recovery.
- Wayland remains compositor-controlled for window positioning. Do not promise
  client-selected windowed placement or manufacture verified physical identity
  where the compositor does not provide sufficient evidence.
- Reconcile [`monitor-events.md`](monitor-events.md) with the implemented
  verified/unverified identity contract, private instance-keyed deltas,
  entity-bearing payloads, current resolver, event ordering, and explicit
  unchanged-entity metadata limitation.

**Files:**

- `crates/bevy_clerestory/Cargo.toml` — add the disabled-by-default
  `monitor-probe` diagnostic feature.
- `Cargo.lock` — record the optional tracing dependency on the package.
- `crates/bevy_clerestory/src/constants.rs` — define the diagnostic tracing
  target only when `monitor-probe` is enabled.
- `crates/bevy_clerestory/src/lib.rs` — topology/current-monitor schedule order.
- `crates/bevy_clerestory/src/monitors/mod.rs` — plugin registration.
- `crates/bevy_clerestory/src/monitors/current_monitor.rs` — event-driven
  refresh inputs and empty-topology cleanup.
- `crates/bevy_clerestory/src/monitors/identity/configuration/mod.rs` — native
  generation state and injected test boundary consumed by topology.
- `crates/bevy_clerestory/src/monitors/identity/mod.rs` — expose private
  identity evidence and diagnostic projections to the topology producer.
- `crates/bevy_clerestory/src/monitors/identity/native.rs` — exact qualified
  evidence hashing for the registry index and platform identity queries.
- `crates/bevy_clerestory/src/monitors/identity/registry.rs` — generation-aware
  cached evidence, provenance, exact hash index, and instance identity.
- `crates/bevy_clerestory/src/monitors/topology.rs` — producer, revision,
  signal gates, Bevy-component snapshot comparison, deltas, ordering, idle-work
  instrumentation, and production-path tests.
- `crates/bevy_clerestory/src/monitors/monitor_probe.rs` — optional diagnostic
  record construction and emission.
- `crates/bevy_clerestory/src/restore/mod.rs` — order restore application and
  settling after current-monitor refresh.
- `docs/bevy_clerestory/monitor-events.md` — update the companion contract.

**Constraints from prior phases:** Phase 2 provides `MonitorIdentity`, private
instance identity, entity-free `MonitorInfo`, `LiveMonitor`, exact
identity-to-entity resolution, `build_monitors`, private instance-keyed deltas,
and entity-bearing raw events. `MonitorConfiguration` is registered once by
`MonitorPlugin`; unchanged generations use cached identity without evidence
work, while changed generations must revalidate all live instances. It
preserves the monitor-domain reflected-path test; if
`MonitorTopologyRevision` derives `Reflect`, give it the same
`bevy_clerestory::monitors` namespace and extend that test.

**Acceptance gate:** Phase-local Clerestory Build, Test, and Lint are green.
Small Bevy `App` tests use the production topology systems with a private
injected identity boundary and require no physical monitor. They prove initial
addition installs state before one connect event; removal installs absence and
clears stale `CurrentMonitor` before one disconnect event, restore, and
settling; reconnect creates a new Bevy entity and uses that component's new
metadata; stable evidence retains physical identity while changed qualified
evidence cannot inherit it; a generation identity change updates the installed
mapping/revision without raw events; and identical revalidation changes
nothing. Instrumentation proves an idle update performs no topology scan,
component read, identity work, native handle/evidence lookup, or native
current-window lookup, while a relevant window change refreshes
`CurrentMonitor` once. A same-entity metadata edit proves the accepted
limitation and performs no producer work. The large append-only identity
history proves exact hash-index lookup. Injected production-system coverage
proves cached-winit index alignment across despawn, reconnect, and entity-slot
reuse, including connect/disconnect payloads and a transient missing cached
association. Default/all-feature and available Linux, Windows, and macOS cfg
checks prove normal behavior does not depend on the diagnostic feature and
retain injected platform callback/identity coverage. No automated result
claims a physical disconnect/reconnect run.

#### Retrospective

**What worked:**

- `topology.rs` now rebuilds only after a monitor lifetime or configuration
  signal, uses Bevy's cached monitor order, and installs topology, revision,
  diagnostic records, and raw events through one queued operation.
- Two-pass identity revalidation, evidence provenance, exact index alignment,
  and event-driven `CurrentMonitor` repair passed the 78-test suite and the
  independent full-diff review.
- The real `restore_window` app emitted three startup connection events at
  revision zero, exposed the primary window's verified current monitor over
  BRP, and shut down cleanly through the application endpoint.

**What deviated from the plan:**

- The first implementation tried to maintain live arrangement, resolution,
  and scale metadata; that work was removed because this feature covers only
  monitor disappearance, return, and stable-identity revalidation.
- Review corrections added scan-order-independent identity projection, cached
  `WinitMonitors` index alignment, retained-evidence provenance, removed
  `CurrentMonitor` repair, and startup connection emission at revision zero.

**Surprises:**

- Building the startup snapshot before the Update producer suppressed every
  startup `MonitorConnected` event until the production `PreStartup` path was
  tested directly.
- The live macOS smoke again enumerated three verified monitor entities despite
  the expected physical setup, so OS enumeration cannot substitute for an
  operator-confirmed hotplug inventory.

**Implications for remaining phases:**

- Phase 4 can merge structured `PreStartup::init_monitors` and
  `Update::monitor_topology_producer` records, including revision-zero startup
  connections, into its causal trace.
- Later recovery phases may react only to installed entity/identity topology
  changes; same-entity arrangement, resolution, or scale edits remain outside
  this feature.
- Phase 4 still requires an operator-confirmed external display and a real
  unplug/replug run before its causal conclusion or checkpoint can pass.

### Phase 3 Review

- Delegation Context and Phase 4 now name the shipped topology/probe owners,
  revision-zero startup records, both producer labels, and the example-local
  tracing layer that merges diagnostic records into one causal trace.
- Phases 7–10 now consume the installed `Monitors` snapshot once per
  `MonitorTopologyRevision`, covering identity-only revisions and coalesced
  lifetime events without counting raw events as recovery transitions.
- Phase 5 retains the approved installed-snapshot capture boundary; the
  suggested native same-entity metadata refresh was not added because live
  arrangement, resolution, and scale maintenance is outside this feature.
- Phase 11 now preserves the reflected topology types and paths already shipped
  in Phase 3 while adding only the missing event type data and registry proof.
- Phase 14 now keeps same-entity display-layout refresh in Hana's capture/panel
  state and prevents it from initiating Clerestory recovery.
- No remaining phase became redundant or required merging or reordering.

### Phase 4 — Build the causal hotplug probe  · status: todo

#### Work Order

**Goal:** Produce a raw primary/secondary hotplug trace that distinguishes OS
relocation from Bevy linked despawn before recovery depends on either path.

**Spec:**

- Create `examples/restore_after_reconnect/` with one primary and one secondary
  `ManagedWindow` on a selected external display. This first version is a probe,
  not a duplicate recovery implementation.
- Every record carries one monotonic sequence number, timestamp, `FrameCount`,
  and producer/schedule label.
- Trace monitor entity, private instance key, evidence provenance, verified
  `MonitorId`, configuration state/generation, topology revision, transition
  `MonitorInfo`, and creation/removal; each `WindowKey`, entity, `OnMonitor`
  entity, and native
  `current_monitor()`; monitor/`OnMonitor`/`HasWindows`/`Window` lifecycle hooks
  at the cascade point; moved/resized/mode events; close/cancel intent; and
  component/entity removal.
- Align the trace with Bevy 0.19 `changed_windows`, `create_monitors`, and
  `about_to_wait` behavior. Do not infer cause from a later update snapshot. If
  lifecycle hooks cannot prove whether relinking preceded cascade removal,
  pause the causal conclusion and add temporary engine-level trace points
  before selecting a mitigation.
- Run the available local physical disconnect/reconnect branch for both
  windows. Record whether each entity survived, was OS-relocated, or was removed
  through `HasWindows`, plus identity and placement capability.
- Establish the evidence schema and manual script in the example README. Every
  run records the operator-confirmed physical inventory and exact unplug/replug
  action separately from OS enumeration. Compare `MonitorId` continuity only
  within one running `App`; Phase 16–19 fill the full platform matrix.
- Declare the example with `required-features = ["monitor-probe"]` and capture
  Phase 3's structured topology tracing records into the same ordered trace as
  the example's window and lifecycle records. The capture layer owns the shared
  sequence number and timestamps; it must preserve the topology producer's
  frame/schedule label and structured fields without promoting private
  instance identity or evidence provenance into application-facing matching
  APIs. It must not report unchanged-entity geometry or scale as freshly
  queried by Clerestory.
- Configure the example's Bevy `LogPlugin::custom_layer` with a tracing layer
  that intercepts only the `bevy_clerestory::monitor_probe` target, visits its
  structured fields, and forwards them into the example trace using the same
  monotonic sequence/timestamp owner as window and lifecycle records. Keep this
  adapter inside the example; do not expose a public diagnostic API.
- Capture revision-zero `PreStartup::init_monitors` records and runtime
  `Update::monitor_topology_producer` records. Preserve their producer frame and
  schedule labels, and prove the startup records enter the shared trace before
  public startup connection observers.

**Files:**

- `crates/bevy_clerestory/Cargo.toml` — register/configure the example if needed.
- `crates/bevy_clerestory/examples/restore_after_reconnect/main.rs` — raw probe.
- `crates/bevy_clerestory/examples/restore_after_reconnect/README.md` — trace
  schema, script, initial evidence, and mitigation conclusion.
- `crates/bevy_clerestory/examples/restore_window/main.rs` — read-only reference
  for BRP-enabled startup behavior.
- `crates/bevy_clerestory/examples/restore_window/debug.rs` — read-only
  reference for identity/entity and raw topology logging.
- `crates/bevy_clerestory/examples/restore_window/input.rs` — read-only
  reference for noninteractive test-mode input handling.
- `crates/bevy_clerestory/src/monitors/topology.rs` — read-only contract
  reference; add diagnostic hooks only if the public/lifecycle surface cannot
  provide required causal facts.
- `crates/bevy_clerestory/src/monitors/monitor_probe.rs` — read-only structured
  field and producer-label reference for the custom tracing layer.
- `crates/bevy_clerestory/src/constants.rs` — read-only monitor-probe target
  reference.
- `crates/bevy_clerestory/src/monitors/identity/native.rs` — evidence production
  reference for the approved diagnostic projection.
- `crates/bevy_clerestory/src/monitors/identity/registry.rs` — private
  instance/evidence-provenance state reference; never expose diagnostic
  provenance as matching input.

**Constraints from prior phases:** Phase 2's `restore_window` logging is probe
groundwork, not physical evidence, and `MonitorId` is comparable only within one
running `App`. Phase 3 guarantees ordered topology events, live/former entity
resolution, `CurrentMonitor` refresh, and disabled-by-default `monitor-probe`
tracing emitted by the topology producer with the consumed configuration
generation, installed revision, private instance/evidence provenance, public
identity, entity, and change kind. Its startup records use revision zero and
`PreStartup::init_monitors`; later records use
`Update::monitor_topology_producer`. Phase 3 does not refresh unchanged-entity
arrangement, resolution, or scale. Do not implement recovery state in this
probe.

**Acceptance gate:** The example builds with all features; the applicable local
physical run records the operator-confirmed inventory, explicit unplug/replug
action, and primary/secondary causal traces using the same schema. The three
verified startup entities and startup smoke do not satisfy this gate. The
automated trace test covers revision-zero `PreStartup` records before startup
connection observers and representative `Update` records in the shared
sequence. The
README states the proven linked-despawn outcome or explicitly records why
temporary engine tracing is still required. No later phase may guess the
mitigation.

### Phase 5 — Make captured window state authoritative  · status: todo

#### Work Order

**Goal:** Replace live-query/file-reload persistence with one `WindowKey`-keyed
authority that can preserve intentional placement while a window or monitor is
absent.

**Spec:**

Implement these states:

```rust
struct CapturedWindowState {
    placement: CapturedPlacement,
    persistence: CapturedPersistence,
    live: Option<LiveWindow>,
}

enum CapturedPlacement {
    PersistedOnly(WindowState),
    Captured(CapturedWindowPlacement),
}

struct CapturedWindowPlacement {
    monitor_snapshot: MonitorInfo,
    position: CapturedWindowPosition,
    logical_size: UVec2,
    saved_window_mode: SavedWindowMode,
    captured_scale: f64,
}

enum CapturedWindowPosition {
    Restorable { logical_offset: IVec2 },
    CompositorControlled,
}

enum CapturedPersistence {
    Writable,
    Frozen,
}
```

- One `CapturedWindowStates` resource reads RON once in `PreStartup` before
  primary preparation. It is both startup snapshot and later persistence
  authority; primary, managed registration, and writes never call
  `load_all_states` again.
- Seed unopened entries as `PersistedOnly`. Promote only after no saved restore
  was needed or startup restoration settled successfully, never from a
  default/fallback placement while restore is pending.
- Binding changes only the live association. `RememberAll` retains unopened or
  cancelled absent entries; `ActiveOnly` removes entries with no live window
  unless an active frozen recovery owns them.
- `monitor_snapshot.identity` is the sole captured identity. Preserve ordinary
  state from `Unverified` without fabricating a `MonitorId`.
- Where top-level coordinates exist, capture a logical offset from the
  capture-time monitor origin. Restore computes:
  `live_origin_physical + logical_offset * live_scale`. Negative origins and DPI
  changes must work.
- Capture uses the installed `CurrentMonitor`/`MonitorInfo` snapshot; it does
  not query native monitor properties or refresh global monitor metadata.
  A reconnect's new Bevy `Monitor` entity supplies fresh origin, size, and
  scale. If those properties change while the same monitor entity remains
  connected, this feature accepts that the installed capture metadata may stay
  unchanged until a lifetime/identity topology signal occurs.
- Wayland captures `CompositorControlled`, projects
  `logical_position: None`, and never creates `WindowPosition::At`. Size/mode
  remain restorable; borderless fullscreen may target an output; exclusive
  fullscreen is not a Wayland return mechanism.
- Treat the existing index/global-logical `WindowState` as an adapter format.
  Project a frozen relative capture back into that format without feeding the
  relative offset through the old global calculation.
- Mutations are per `WindowKey`. A dirty batch produces at most one whole-map
  projection/write. An unchanged update performs no window scan, file read,
  projection, write, identity extraction, or interner work.
- Remove global restore-time persistence pauses; freezing is per key and
  unrelated windows continue saving.

**Files:**

- `crates/bevy_clerestory/src/persistence/mod.rs` — own/init the authority.
- `crates/bevy_clerestory/src/persistence/load.rs` — single startup read.
- `crates/bevy_clerestory/src/persistence/save.rs` — dirty-batch projection.
- `crates/bevy_clerestory/src/persistence/format.rs` — adapter and compatibility
  tests.
- `crates/bevy_clerestory/src/persistence/window_state.rs` — persisted adapter.
- `crates/bevy_clerestory/src/persistence/captured_window_state.rs` — new
  authority and state transitions.
- `crates/bevy_clerestory/src/restore_window_config.rs` — remove duplicate
  loaded-state ownership.
- `crates/bevy_clerestory/src/monitors/current_monitor.rs` — healthy capture
  inputs.
- `crates/bevy_clerestory/src/platform.rs` — position/mode capability.

**Constraints from prior phases:** Phase 2–3 provide entity-free current
`MonitorInfo`, verified/unverified identity, installed Bevy component metadata,
and exact entity association. Phase 3 deliberately does not refresh origin,
size, resolution, or scale for an unchanged live monitor entity; Phase 5 must
not add a native/global metadata refresh or use such a change to start
recovery.

**Acceptance gate:** Phase-local Clerestory Build, Test, and Lint are green.
Tests cover one-read seeding/promotion, `RememberAll`/`ActiveOnly`, frozen-entry
protection across every write/removal path, negative origins and 1x↔2x
rebasing, Wayland compositor-controlled projection, one write per dirty batch,
and instrumented zero work on unchanged updates. A same-entity monitor metadata
edit proves capture performs no native/global refresh, while a returned monitor
entity supplies the new origin and scale used for rebasing.

### Phase 6 — Unify staged restore preparation  · status: todo

#### Work Order

**Goal:** Make primary startup, managed startup, and future runtime recovery
prepare the same `TargetPosition` through one staged path.

**Spec:**

- Consolidate target preparation behind one private builder/context. It waits
  for a live winit window and current-monitor information, then installs
  `TargetPosition` through the existing `X11FrameCompensated` path.
- The builder accepts an already rebased physical position only for
  `CapturedWindowPosition::Restorable`. `CompositorControlled` never passes a
  synthetic coordinate.
- Primary `PreStartup` remains a thin consumer. Preserve its chained
  `apply_deferred` before `move_to_target_monitor` so X11 fullscreen behavior
  does not regress.
- Preserve `ClerestoryPreStartupSet::MonitorsInitialized` before restore
  loading/preparation so the primary and managed paths consume the initialized
  generation-aware monitor snapshot.
- Managed startup uses the same preparation path. A canonical replacement
  bound to an application-controlled recovery key must be able to bypass
  ordinary automatic startup restore until an explicit request.
- Prepared state carries an origin discriminator so startup settling never
  defaults a missing key/entity to primary and runtime callers can later carry
  immutable attempt context.
- Reuse `compute_target_position`, `TargetPosition`, `restore_windows`, and the
  existing settle/result behavior rather than creating a second restore engine.

**Files:**

- `crates/bevy_clerestory/src/lib.rs` — preserve the monitor-initialization
  `PreStartup` set before restore preparation.
- `crates/bevy_clerestory/src/managed.rs` — route managed startup through the
  shared preparation path.
- `crates/bevy_clerestory/src/restore/mod.rs` — register shared preparation.
- `crates/bevy_clerestory/src/restore/winit_info.rs` — retain thin primary
  `PreStartup` consumer and flush.
- `crates/bevy_clerestory/src/restore/settle_state.rs` — preserve explicit
  startup completion.
- `crates/bevy_clerestory/src/restore/target_position/mod.rs` — preparation
  exports.
- `crates/bevy_clerestory/src/restore/target_position/target.rs` — consume an
  already-rebased physical placement.
- `crates/bevy_clerestory/src/restore/target_position/monitor.rs` — shared
  target resolution.
- `crates/bevy_clerestory/src/restore/target_position/application.rs` — shared
  application path.
- `crates/bevy_clerestory/src/restore/restore_attempt.rs` — create the common
  preparation/origin owner; runtime attempt behavior lands in Phase 9.
- `crates/bevy_clerestory/src/x11_position_fix.rs` — preserve compensation
  boundary.

**Constraints from prior phases:** Phase 2 registers
`ClerestoryPreStartupSet::MonitorsInitialized` as the boundary after the initial
generation-aware monitor snapshot exists. Phase 5 supplies the one-read startup
snapshot, `CapturedWindowPlacement`, and rebased-coordinate boundary.

**Acceptance gate:** Phase-local Clerestory Build, Test, and Lint are green.
Primary and managed startup tests still pass; tests prove startup and a
synthetic runtime caller compute equivalent target size/mode/placement, X11
fullscreen preparation keeps its monitor-initialization, flush, and
compensation order, and compositor-owned placement never installs a coordinate.

### Phase 7 — Add one-shot registration and application-controlled recovery  · status: todo

#### Work Order

**Goal:** Register canonical windows once, freeze application-controlled
placement on loss, publish factual availability, and support explicit
cancellation without automatic application decisions.

**Spec:**

Use this registration component:

```rust
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq, Reflect)]
#[reflect(Component)]
pub enum WindowRecovery {
    #[default]
    Disabled,
    ApplicationControlled,
    FallbackAndReturn,
}
```

- One non-disabled component addition creates one private pending generation and
  copies the variant. `Disabled` creates no work. Later mutation/removal does
  not change copied policy; change is cancel, remove, add. Cancellation prevents
  rearming until a new component-add generation.
- Accept a baseline only after exactly one canonical identity
  (`PrimaryWindow` or authoritative `ManagedWindowRegistry`), a healthy live
  winit window and exact monitor entity, complete verified `CurrentMonitor`,
  completed startup restore, and—for `FallbackAndReturn` only—a supported
  return mechanism.
- Same-bundle managed registration waits for name deduplication. Reject both or
  neither canonical identity.
- Keep application-controlled and automatic lifecycles as separate closed
  private enums. This phase implements application-controlled healthy,
  removal-pending, target-absent, target-available, restoring, and
  retryable-failure transitions; zero displays is topology input.
- Drive recovery topology evaluation from the installed `Monitors` snapshot
  and `MonitorTopologyRevision`, once per installed revision. Raw
  `MonitorConnected`/`MonitorDisconnected` events are lifetime facts, not one
  recovery transition apiece: identity-only changes may have no raw event, and
  one replacement revision may contain several raw events. Evaluate the
  revision-zero startup snapshot once when a registration first becomes
  eligible.
- On verified target loss, freeze the captured entry even if the entity
  survives on a fallback. Never adopt that fallback automatically.
- Define factual events with canonical key and monitor facts only:

```rust
pub struct WindowRecoveryPending {
    pub window_key: WindowKey,
    pub monitor_id: MonitorId,
}

pub struct WindowRecoveryAvailable {
    pub window_key: WindowKey,
    pub monitor: MonitorInfo,
}

pub struct RestoreWindow {
    pub entity: Entity,
}

pub struct CancelWindowRecovery {
    pub window: WindowKey,
}
```

  The final derives/reflection surface is completed in Phase 11. Notifications
  never echo `WindowRecovery`.
- `ApplicationControlled` never creates/re-enables content and never applies a
  restore without an accepted `RestoreWindow`. It may retain a verified target
  on compositor-controlled platforms; supported size/mode can be applied later,
  but target-output mismatch remains an explicit result.
- Cancellation invalidates pending/accepted state and any attempt, never
  despawns a live window, and never decides saved-placement retention.
  `RememberAll` retains an absent entry as `PersistedOnly`; `ActiveOnly` removes
  it; a live cancelled entry becomes writable immediately.
- Record programmatic close as cancel then despawn. Record OS close intent from
  `Added<ClosingWindow>` before `Last` despawn; a declined
  `WindowCloseRequested` is not close intent. Close/cancel wins a same-frame
  race with monitor removal; unmarked removal stays recoverable.

**Files:**

- `crates/bevy_clerestory/src/lib.rs` — order recovery topology/window sets
  around monitor and persistence sets.
- `crates/bevy_clerestory/src/monitors/mod.rs` — expose the installed-topology
  schedule boundary to recovery without exposing private producer state.
- `crates/bevy_clerestory/src/events.rs` — integrate existing result types with
  recovery lifecycle facts.
- `crates/bevy_clerestory/src/managed.rs` — canonical replacement binding and
  startup-restore bypass.
- `crates/bevy_clerestory/src/recovery/mod.rs` — create domain root, types, and
  private plugin.
- `crates/bevy_clerestory/src/recovery/registration.rs` — generations,
  canonical acceptance, cancellation, and removal classification.
- `crates/bevy_clerestory/src/recovery/application_controlled.rs` — private
  phases and transitions.
- `crates/bevy_clerestory/src/persistence/captured_window_state.rs` — per-key
  freeze/cancel/promotion transitions.

**Constraints from prior phases:** Phase 5 owns all captured/persisted state;
Phase 6 owns shared restore preparation and canonical replacement startup
bypass. Phase 3 installs `Monitors` before a revision and its raw events;
startup is revision zero, identity-only changes can advance the revision with
no raw event, and several lifetime events can share one revision.

**Pending decision: Canonical reflected paths for the new recovery API**

Actual problem:
Phase 7 introduces public recovery types across several private files, so
derived Bevy paths would leak their physical declaration modules and could
change during later refactors.

What exists now:
- Existing monitor types preserve `bevy_clerestory::monitors::*` paths, while
  existing restore result events keep their `bevy_clerestory::events::*` paths.
- Phase 11 requires remote reflection and event type data but does not name
  stable paths for Phase 7's new component and events.

What should change:
- Assign one canonical namespace to every new public recovery component and
  event, preserve the existing result-event paths, and assert every exact path
  in Phase 11.

Recommendation:
Use `bevy_clerestory::recovery::*` for all new recovery types and retain
`bevy_clerestory::events::*` for existing restore results; add custom
`TypePath` attributes when the types are introduced and exact assertions in
Phase 11.

**Acceptance gate:** Phase-local Clerestory Build, Test, and Lint are green.
Pure and Bevy `App` tests cover first eligible baseline, same-bundle
canonicalization, disabled/mutated/removed/cancelled generations, verified and
unverified targets, freeze of surviving fallbacks, cancellation under both
persistence modes with zero monitors, close races, declined close, and
duplicate pending/available suppression. Production-schedule tests cover
revision-zero registration, an identity-only downgrade/return without raw
events, and one coalesced replacement revision without duplicate lifecycle
transitions.

### Phase 8 — Add automatic fallback-and-return transitions  · status: todo

#### Work Order

**Goal:** Track an OS-relocated window through fallback settling, intervention,
target return, or missing-window state without conflating it with
application-controlled recovery.

**Spec:**

- Implement private automatic phases for healthy, removal-pending,
  fallback-settling, on-fallback, restoring, missing-live-window, and
  retryable-failure. Apply the same model to registered primary and secondary
  windows.
- `FallbackAndReturn` arms only with verified identity plus either
  `CapturedWindowPosition::Restorable` or a supported monitor-targeted
  fullscreen mode. Compositor-controlled Wayland windowed registrations remain
  pending/unarmed; Wayland borderless fullscreen may target the verified output
  without a position; exclusive fullscreen is unsupported.
- On target loss, freeze one original intent and emit pending once. The
  operating system's initial fallback relocation never overwrites it.
- Fallback settling observes monitor identity, captured-position state plus
  physical position when `Restorable`, logical size, and effective mode. Any
  tuple change resets the existing stability timer.
- If the target returns before fallback settle, request restore from frozen
  placement and reject later fallback messages. After settle, a later position,
  size, or mode change is intervention: atomically adopt it, make persistence
  writable, clear only the current return intent, and keep the registration.
  It rearms only when the adopted placement has verified identity and a
  supported return mechanism.
- With zero displays, wait without creating. A different monitor returning
  first leaves a surviving OS fallback alone; a target-first return may proceed.
  A missing window is represented explicitly and no empty all-window query
  authorizes reconstruction.
- Repeated and different-ID reconnects are no-ops; rearrangement alone creates
  no recovery transition.
- Consume the installed topology once per changed `MonitorTopologyRevision`,
  including identity-only revisions with no raw event and replacement revisions
  carrying both disconnect and connect facts. Do not advance automatic phases
  once per raw event.

**Files:**

- `crates/bevy_clerestory/src/recovery/mod.rs` — register automatic lifecycle.
- `crates/bevy_clerestory/src/recovery/registration.rs` — capability-aware
  acceptance/rearming.
- `crates/bevy_clerestory/src/recovery/fallback_and_return.rs` — phase model,
  settling, intervention, and transition tests.
- `crates/bevy_clerestory/src/persistence/captured_window_state.rs` — frozen
  intent/adoption operations.
- `crates/bevy_clerestory/src/platform.rs` — position/fullscreen return
  capability.
- `crates/bevy_clerestory/src/constants.rs` — reuse existing stability
  duration.

**Constraints from prior phases:** Phase 7 supplies one-shot registrations,
canonical keys, close/removal classification, policy-specific registry
ownership, and one installed-snapshot evaluation per topology revision. Phase 5
supplies typed position and atomic persistence transitions.

**Acceptance gate:** Phase-local Clerestory Build, Test, and Lint are green.
Table-driven transition tests cover primary/secondary loss, fallback settling
and reset, target-before-settle, intervention/rearming, verified/unverified and
Wayland capability branches, zero-display/non-target-first order, duplicate
events, fallback-monitor loss, and missing live windows. Tests also prove an
identity-only revision and a coalesced disconnect/connect revision each produce
one automatic topology transition.

### Phase 9 — Execute runtime restore attempts  · status: todo

#### Work Order

**Goal:** Route explicit and automatic runtime requests through the shared
restore pipeline with immutable attempt identity and correct same-frame order.

**Spec:**

Use these private types:

```rust
struct RestoreAttempt {
    id: RestoreAttemptId,
    window_key: WindowKey,
    entity: Entity,
    expected_monitor: MonitorId,
    topology_revision: MonitorTopologyRevision,
    deadline: Instant,
}

enum RestoreOrigin {
    Startup { window_key: WindowKey },
    Recovery(RestoreAttempt),
}
```

- `RestoreWindow` is entity-targeted and carries no caller-supplied key/ticket.
  Derive exactly one canonical key from `PrimaryWindow` or
  `ManagedWindowRegistry`, reject both/neither, verify the current restorable
  phase, and accept either the surviving binding or one canonically bound
  replacement.
- Resolve the target only by frozen `MonitorId`. If absent, leave intent pending
  with no false result.
- Start one attempt deadline at request acceptance, before winit or
  `TargetPosition` necessarily exists. Preserve the full attempt tuple through
  preparation, X11 compensation, application, DPI changes, and settling.
- Extend Phase 6's prepared state with `RestoreOrigin::Recovery`; use the same
  builder and `restore_windows` path as startup.
- Configure and flush this chain:

```text
MonitorTopologyInstall
-> RecoveryTopologyTransitions
-> CurrentMonitorRefresh
-> RecoveryWindowTransitions
-> RestorePreparation
-> X11Compensation
-> RestoreApplication
-> RestoreSettling
-> PersistenceProjection
```

  Component observers enqueue work only. Flush every producer/consumer edge
  that passes registration, attempt, target, compensation, or result state.
- `RecoveryTopologyTransitions` reads the already-installed `Monitors` snapshot
  once for each new `MonitorTopologyRevision`; it never counts raw topology
  events. Attempt invalidation/replanning therefore sees identity-only changes
  and treats a disconnect/connect pair from one install as one topology input.
- Preserve the entity on every `WindowScaleFactorChanged` message and advance
  only the matching entity, attempt, phase, and reported/live target scale.
- Runtime settling submits a private completion containing the full attempt
  tuple to one validator before public `WindowRestored` or
  `WindowRestoreMismatch`. Startup completion remains a separate origin; never
  derive a missing key from current markers or default an unknown entity to
  primary.

**Files:**

- `crates/bevy_clerestory/src/lib.rs` — complete ordered system sets and flushes.
- `crates/bevy_clerestory/src/monitors/mod.rs` — installed-topology schedule
  boundary consumed by recovery transitions.
- `crates/bevy_clerestory/src/recovery/application_controlled.rs` — explicit
  request acceptance.
- `crates/bevy_clerestory/src/recovery/fallback_and_return.rs` — automatic
  request creation and restoring transition.
- `crates/bevy_clerestory/src/restore/mod.rs` — runtime attempt systems.
- `crates/bevy_clerestory/src/restore/restore_attempt.rs` — attempt identity,
  context, preparation, and completion validation.
- `crates/bevy_clerestory/src/restore/settle_state.rs` — route runtime
  completion through the validator.
- `crates/bevy_clerestory/src/restore/target_position/application.rs` —
  attempt-aware restore.
- `crates/bevy_clerestory/src/restore/target_position/strategy.rs` — scoped
  phase transitions.
- `crates/bevy_clerestory/src/restore/target_position/run_conditions.rs` —
  attempt-aware conditions.
- `crates/bevy_clerestory/src/windows_dpi_fix.rs` — entity/attempt-scoped DPI.
- `crates/bevy_clerestory/src/x11_position_fix.rs` — attempt-aware
  compensation.

**Constraints from prior phases:** Phase 6 supplies one staged builder and
startup origin. Phase 7–8 supply canonical lifecycle phases and frozen intent.
Phase 3 supplies topology revisions installed before recovery transitions;
revision zero is the startup snapshot, identity-only changes may have no raw
event, and multiple raw lifetime events may share one installed revision.

**Acceptance gate:** Phase-local Clerestory Build, Test, and Lint are green.
Bevy `App` tests prove surviving and canonical replacement requests produce the
same `TargetPosition` as startup, target absence remains pending, ordered
flushes expose components to the next consumer, successful settle validates the
complete tuple, and concurrent cross-DPI restores advance only the addressed
entity. Topology-transition tests prove identity-only and coalesced replacement
revisions invalidate or replan each attempt exactly once.

### Phase 10 — Harden retry, cleanup, and linked-despawn behavior  · status: todo

#### Work Order

**Goal:** Make every runtime attempt terminate or wait deterministically under
topology churn, stale messages, timeout, cancellation, and window destruction.

**Pending decision: Select linked-despawn mitigation from Phase 4 evidence**

Actual problem:
`OnMonitor`/`HasWindows` linked-spawn may remove a window before Clerestory can
relink it, but OS relocation and linked despawn require different handling.

What exists now:
- The compiled plan predates Phase 4's causal physical trace.
- A missing-window lifecycle state exists; no query result alone authorizes
  reconstruction.

What should change:
- If the trace proves safe relationship-cascade prevention, name and implement
  that narrow mechanism.
- Otherwise implement reconstruction/rebinding only for a pending copied
  `FallbackAndReturn` policy while a monitor is available, associate the
  replacement with the frozen key before persistence, and leave
  application-owned content repair in Hana.

Recommendation:
Use the narrowest mechanism proven by the causal trace; never infer the branch
from an empty all-window query or a missing-window query. Phase review must
replace this block with the selected files/behavior before dispatching Phase 10.

**Spec:**

- Before application or settle, revalidate entity, key, attempt ID, expected
  monitor, and topology revision.
- On each new installed `MonitorTopologyRevision`, process attempts once before
  window transitions. Target loss returns the lifecycle to waiting. An
  identity-only revision with no raw event still invalidates or replans the
  affected attempt; a coalesced disconnect/connect install is one input. A
  reconnected still-verified target may supply changed geometry through its new
  Bevy monitor entity; create a new private attempt generation and recompute
  placement without extending the original deadline.
- One finalizer removes every attempt-scoped component on success, mismatch,
  timeout, cancellation, target loss, or replacement loss. Late results/DPI
  messages from old attempts cannot advance a retry.
- Timeout covers winit creation, X11 compensation, DPI, fullscreen application,
  and settling, then enters the explicit retryable state.
- Mismatch retains frozen target and usable fallback, ends the attempt, and
  never retries every frame. Retry begins only from a later matching topology
  revision or accepted explicit request.
- Zero displays creates no window. Distinguish target-first and non-target-first
  returns. Coalesced disconnect/reconnect and reconnect-before-fallback-settle
  remain deterministic.
- Apply the Phase 4 linked-despawn decision equally to primary and secondary
  automatic-return windows. Never reconstruct arbitrary application content.

**Files:**

- `crates/bevy_clerestory/src/constants.rs` — whole-attempt deadline.
- `crates/bevy_clerestory/src/recovery/registration.rs` — missing/removal
  classification and selected adapter binding.
- `crates/bevy_clerestory/src/recovery/application_controlled.rs` — retryable
  explicit path.
- `crates/bevy_clerestory/src/recovery/fallback_and_return.rs` — retry,
  zero-display, and selected linked-despawn path.
- `crates/bevy_clerestory/src/restore/restore_attempt.rs` — replanning,
  invalidation, timeout, and finalizer.
- `crates/bevy_clerestory/src/restore/settle_state.rs` — validated private
  completion only.
- `crates/bevy_clerestory/src/windows_dpi_fix.rs` — reject late DPI messages.
- `crates/bevy_clerestory/examples/restore_after_reconnect/README.md` — source
  of the causal decision.

**Constraints from prior phases:** Phase 4 must provide the causal decision
that replaces the Pending decision block. Phase 9 provides immutable attempts,
the ordered chain, one installed-snapshot evaluation per topology revision, and
central completion validation. Phase 7 provides close/cancel precedence.

**Acceptance gate:** Phase-local Clerestory Build, Test, and Lint are green.
Table-driven tests cover cleanup at every stage, stale completions after retry,
same-entity old attempt IDs, topology replanning without deadline extension,
target loss/return, mismatch/no-frame-loop, timeout, zero displays,
target-first/non-target-first/coalesced order, identity-only revisions without
raw events, cancellation/replacement loss, and the selected linked-despawn
behavior for primary and secondary windows.

### Phase 11 — Publish the reflected recovery API  · status: todo

#### Work Order

**Goal:** Expose one registration-time recovery API whose events are factual,
remotely observable/triggerable, and backed by the completed private lifecycle.

**Spec:**

- Publicly re-export `WindowRecovery` with `Disabled`,
  `ApplicationControlled`, and `FallbackAndReturn` for both primary and
  secondary managed windows. No role-specific marker types or mutable runtime
  policy surface.
- Publicly expose `WindowRecoveryPending`, `WindowRecoveryAvailable`,
  `RestoreWindow`, and `CancelWindowRecovery` with the exact fields introduced
  in Phase 7. Pending/available events report key/monitor facts only and never
  echo policy.
- `RestoreWindow` remains an entity event with no public recovery ticket or
  caller key. `CancelWindowRecovery` remains global/keyed because the original
  entity may be gone.
- All public Clerestory events derive `Reflect` and include
  `#[reflect(Event)]`; `WindowRecovery` includes `#[reflect(Component)]`.
  Notification/result events must support Bevy 0.19 `world.observe+watch` and
  request/cancel events `world.trigger_event`.
- Rely on `reflect_auto_register` for non-generic monomorphizations. Do not add
  redundant `App::register_type` calls.
- Preserve Phase 3's public `MonitorTopologyRevision`, `MonitorConnected`, and
  `MonitorDisconnected` definitions and exact
  `bevy_clerestory::monitors::*` paths. Add only the missing reflected event
  type data and `AppTypeRegistry` coverage required for remote observation; do
  not duplicate or relocate the topology types.
- Register `RecoveryPlugin` through Clerestory's normal plugin assembly and
  document one-shot registration, cancellation, application ownership,
  capability gating, and Wayland behavior.

**Files:**

- `crates/bevy_clerestory/src/lib.rs` — public re-exports/plugin assembly.
- `crates/bevy_clerestory/src/events.rs` — reflected existing result events.
- `crates/bevy_clerestory/src/monitors/topology.rs` — reflected raw monitor
  events and preserved monitor-domain paths.
- `crates/bevy_clerestory/src/recovery/mod.rs` — final public types/re-exports
  and plugin.
- `crates/bevy_clerestory/src/recovery/registration.rs` — public component
  observer boundary.
- `crates/bevy_clerestory/src/recovery/application_controlled.rs` — request
  observer.
- `crates/bevy_clerestory/README.md` — public API examples and behavioral
  limits.

**Constraints from prior phases:** Phase 7–10 fully own lifecycle state,
canonical identity, restoration, cancellation, and finalization; this phase
exposes that behavior without duplicating state. Phase 3 already publicly
exports and assigns exact reflected monitor paths to
`MonitorTopologyRevision`, `MonitorConnected`, and `MonitorDisconnected`, and
`topology.rs` owns their existing path regression.

**Acceptance gate:** Phase-local Clerestory Build, Test, and Lint are green.
Public API tests cover primary and managed registration, all event payloads,
entity-derived restore identity, absent-key cancellation, and expected
`AppTypeRegistry` event type data for every public event with no manual
registration. Exact `TypePath` assertions include `MonitorConnected` and
`MonitorDisconnected` alongside `CurrentMonitor`, `MonitorId`, `MonitorIdentity`,
`MonitorInfo`, and `Monitors` under `bevy_clerestory::monitors::*`, plus
`MonitorTopologyRevision` under the same namespace.

### Phase 12 — Complete the recovery example  · status: todo

#### Work Order

**Goal:** Turn the raw probe into an end-to-end consumer of both public recovery
policies while retaining its causal diagnostics.

**Spec:**

- Keep the raw sequence/timing/lifecycle trace from Phase 4.
- Configure the primary and secondary managed windows with
  `WindowRecovery::FallbackAndReturn`.
- Add a third application-controlled managed window with minimal
  application-owned content.
- Exercise pending/available observation, surviving canonical restore,
  replacement creation with exactly one canonical identity, result handling,
  cancellation while absent, and proof that later reconnect creates nothing.
- For automatic return, verify target reconnect without intervention returns
  an eligible window and post-settle movement/resize/mode change keeps the
  adopted fallback. Verify unverified identity and Wayland windowed capability
  remain unarmed.
- Demonstrate the Phase 10 linked-despawn solution for both primary and
  secondary windows on any platform where the cascade occurs.
- Keep cameras/content/re-enable decisions in example application code; do not
  move them into Clerestory.
- Update the README script and evidence schema so Phase 16–19 can execute the
  same behavior on every platform.

**Files:**

- `crates/bevy_clerestory/Cargo.toml` — final example configuration.
- `crates/bevy_clerestory/examples/restore_after_reconnect/main.rs` — full
  consumer and retained trace.
- `crates/bevy_clerestory/examples/restore_after_reconnect/README.md` — complete
  script, expected branches, and matrix.
- `crates/bevy_clerestory/README.md` — link the example.

**Constraints from prior phases:** Phase 11 is the only public recovery surface.
Phase 10 supplies the selected linked-despawn behavior. The example must not
reach into private recovery state or recreate it.

**Acceptance gate:** The example builds and its non-hardware logic is covered by
Clerestory Test/Lint gates. On available hardware, the script completes
automatic primary/secondary return or the documented unarmed branch,
intervention adoption, application-controlled surviving/replacement restore,
result handling, and cancellation without content resurrection.

### Phase 13 — Integrate Hana editor recovery  · status: todo

#### Work Order

**Goal:** Make Hana's editor consume Clerestory automatic recovery and preserve
entity-scoped close/egui behavior without duplicating recovery state.

**Spec:**

- Temporarily point Hana at this checkout through one path dependency or
  `[patch.crates-io]` and update its lockfile; do not test published 0.1.1.
- Insert `WindowRecovery::FallbackAndReturn` during primary editor setup. It
  arms only on verified, targetable placement; Wayland windowed remains
  unarmed.
- Replace Hana's “no windows exist” recovery heuristic with Clerestory
  lifecycle/canonical state. Test for a missing `PrimaryWindow`, not an empty
  set of all `Window` entities.
- Retain Hana-specific primary reconstruction and egui remapping only where the
  Phase 4/10 linked-despawn path proves entity replacement is required. Create
  exactly one replacement and bind its canonical identity before restore.
- Inspect the entity carried by `WindowCloseRequested`: closing primary exits;
  closing an output follows output lifecycle rather than terminating Hana.
- Losing primary while a conduit output survives must not block editor
  recovery.
- Exercise an existing ordinary managed secondary window with
  `FallbackAndReturn` if one exists; do not invent a Hana use case solely for
  coverage.
- Feed any API mismatch back into Clerestory immediately rather than retaining
  parallel Hana state.

**Files:**

- `../hana/Cargo.toml` — temporary local dependency override.
- `../hana/Cargo.lock` — local dependency resolution.
- `../hana/crates/hana/Cargo.toml` — consumer declaration if package-local
  changes are needed.
- `../hana/crates/hana/src/main.rs` — editor registration and setup.
- `../hana/crates/hana/src/window_recovery.rs` — lifecycle consumption,
  replacement/egui repair, and entity-scoped close.
- `crates/bevy_clerestory/src/recovery/mod.rs` — adjust only for proven public
  API feedback.
- `crates/bevy_clerestory/src/recovery/registration.rs` — adjust only if Hana
  otherwise must duplicate canonical state.

**Constraints from prior phases:** Phase 12 proves the public API and
linked-despawn behavior independently. Hana must consume only that public
surface and keep the local override until Phase 21.

**Acceptance gate:** Hana phase-local Build, Test, and Lint are green, plus
affected Clerestory gates if its API changes. Tests cover OS-relocated editor
return on coordinate-capable backends, intervention, exactly one replacement
when proven necessary, egui rebinding, surviving conduit output, primary close
exit, and non-primary close behavior.

### Phase 14 — Integrate Hana monitor-backed screens and outputs  · status: todo

#### Work Order

**Goal:** Make Hana's display-specific consumers use Clerestory identity and
availability while Hana remains sole owner of output existence/content.

**Spec:**

- A Clerestory-managed output uses `ApplicationControlled`; an unmanaged output
  consumes raw `MonitorConnected`/`MonitorDisconnected` facts. Choose the path
  that matches each existing Hana lifecycle—do not invent state solely to test
  Clerestory.
- Keep the process-lifetime `MonitorId` separate from the optional live monitor
  entity. It may be retained across monitor-entity lifetimes within one running
  `App`, but is never persisted or compared across application runs. Clear the
  entity on disconnect; on reconnect match only exact verified identity, then
  replace every retained `InputJack`/output target with the new entity before
  re-enable. Never retain/use the disconnected entity.
- An unverified target remains inactive until application selection.
- Losing a cable target marks its in-world screen unavailable and creates no
  fallback output. Reconnect may offer application-controlled re-enable; it
  never causes Clerestory to create content/output automatically.
- Recreated borderless output uses
  `MonitorSelection::Entity(new_entity)`. Wayland windowed placement is never
  promised; borderless targeting follows platform capability.
- Removing a cable while absent sends `CancelWindowRecovery` for a registered
  key, or confirms no entry exists for an unmanaged/raw-event path. Later
  reconnect must not revive it.
- Hana keeps cable routes, capture sessions, rendered content, first-frame
  readiness, and UI/re-enable policy. Do not add any of those concepts to
  Clerestory.
- Treat Clerestory as the source of physical identity and monitor-entity
  lifetime availability, not as a continuous display-layout feed. If Hana's
  screen/capture backend needs same-entity arrangement, resolution, or scale
  refresh, keep that metadata in Hana's backend-owned state; it must not mutate
  Clerestory topology or initiate reconnect recovery.
- Revise the Clerestory boundary if Hana would otherwise infer identity from
  position, retain dead entities, or reproduce private recovery state.

**Files:**

- `../hana/crates/hana/src/screens/mod.rs` — observer/system registration.
- `../hana/crates/hana/src/screens/connection.rs` — identity/availability
  transitions.
- `../hana/crates/hana/src/screens/panel.rs` — current monitor metadata and
  panel association.
- `../hana/crates/hana/src/conduit/mod.rs` — observer/system registration.
- `../hana/crates/hana/src/conduit/jack.rs` — live entity rebinding.
- `../hana/crates/hana/src/conduit/window.rs` — output lifecycle and monitor
  selection.
- `../hana/crates/hana/src/conduit/cable.rs` — retirement/cancellation.
- `crates/bevy_clerestory/src/monitors/mod.rs` — adjust only for proven metadata
  feedback.
- `crates/bevy_clerestory/src/recovery/mod.rs` — adjust only for proven
  application-controlled feedback.

**Constraints from prior phases:** Phase 13 established Hana's local dependency
and editor consumption. Phase 11's factual event/API boundary remains
authoritative; `MonitorId` is process-local rather than persisted identity, and
rejected capture/readiness scope must stay out of Clerestory. Phase 3 refreshes
Clerestory metadata only for monitor entity lifetimes and identity
revalidation; Hana owns any same-entity live display-layout metadata its capture
or panel presentation requires.

**Acceptance gate:** Hana phase-local Build, Test, and Lint are green, plus
affected Clerestory gates for API changes. Tests cover target loss/inactive UI,
verified reconnect/re-enable choice, unverified target, fresh-entity rebinding
across every reference, no fallback output, registered/raw paths, cable
cancellation, and no resurrection. A same-entity Hana backend layout update
changes only Hana-owned capture/panel metadata and starts no Clerestory recovery
transition.

### Phase 15 — Converge the cross-workspace API and automated gates  · status: todo

#### Work Order

**Goal:** Stabilize the Clerestory/Hana boundary after both real consumer passes
and prove all hardware-independent acceptance behavior.

**Spec:**

- Fold concrete feedback from Phases 13–14 into Clerestory only where the
  consumer otherwise duplicates identity, persistence, restore, or recovery
  state. Do not absorb Hana content/capture/UI lifecycle.
- Ensure the local Hana override still targets this checkout while corrections
  are tested.
- Complete test coverage for immediate topology observers, identity ambiguity,
  first eligible registration, persistence promotion/freezing/projection,
  primary/secondary and both policies, attempt cleanup/retry, concurrent DPI,
  reflected public events, zero displays, and Wayland capability gating.
- Run full workspace tests/build/lint in both workspaces, not only package-local
  gates.
- Update public README and unreleased changelog to the stabilized API and
  explicitly separate automated proof from physical-only behavior.
- Do not publish or remove Hana's local override in this phase.

**Files:**

- `crates/bevy_clerestory/src/monitors/identity/mod.rs` — final identity surface
  and internal ownership.
- `crates/bevy_clerestory/src/monitors/identity/native.rs` — final native
  evidence feedback.
- `crates/bevy_clerestory/src/monitors/identity/registry.rs` — final ambiguity,
  interner, and instance-cache feedback.
- `crates/bevy_clerestory/src/monitors/identity/configuration/` — final native
  generation-notification feedback in the owning target module.
- `crates/bevy_clerestory/src/monitors/topology.rs` — final lifetime/identity
  topology feedback.
- `crates/bevy_clerestory/src/persistence/captured_window_state.rs` — final
  persistence feedback.
- `crates/bevy_clerestory/src/recovery/mod.rs` — final public API.
- `crates/bevy_clerestory/src/recovery/registration.rs` — final canonical
  registration.
- `crates/bevy_clerestory/src/recovery/application_controlled.rs` — final
  explicit lifecycle.
- `crates/bevy_clerestory/src/recovery/fallback_and_return.rs` — final automatic
  lifecycle.
- `crates/bevy_clerestory/src/restore/restore_attempt.rs` — final runtime
  attempt behavior.
- `crates/bevy_clerestory/README.md` — stabilized public docs.
- `crates/bevy_clerestory/CHANGELOG.md` — unreleased entry.
- `../hana/Cargo.toml` and `../hana/Cargo.lock` — retain local checkout.
- `../hana/crates/hana/src/window_recovery.rs` — final editor consumer.
- `../hana/crates/hana/src/screens/connection.rs` — final screen consumer.
- `../hana/crates/hana/src/conduit/jack.rs` — final entity rebinding.
- `../hana/crates/hana/src/conduit/window.rs` — final output consumer.

**Constraints from prior phases:** Phase review must propagate the exact API
feedback and changed signatures from Phases 13–14 here. The design boundary and
all public behavior are otherwise fixed.

**Acceptance gate:** In both workspaces, final CI-parity Build, full workspace
Test, and full `clippy` skill gates are green. The Hana dependency resolves to
this checkout, all named hardware-independent cases pass, and docs contain no
claim that requires unrecorded physical evidence.

### Phase 16 — Record the macOS physical matrix  · status: todo

#### Work Order

**Goal:** Prove macOS identity continuity, OS relocation/linked lifetime,
intervention, placement, fullscreen, and real DPI behavior with the completed
example.

**Spec:**

- Run the Phase 12 script on macOS and append rows to the shared README matrix.
- Cover same-panel reconnect; same panel through another port/dock; a different
  same-model panel at the same position where available; simultaneous duplicate
  identities where available; identity change; lid close/open; repeated dock
  churn; reorder; zero displays; non-target-first return; rapid/coalesced
  hotplug; windowed/borderless/exclusive modes; and actual cross-DPI reconnect.
- Record qualified-evidence availability/provenance, `Verified`/`Unverified`,
  entity survival/cascade, captured-position state, supported return mechanism,
  expected action, actual action, and pass/fail. Label transition/`App` proof
  separately from the physical observation.
- Record that `MonitorId` comparisons are valid only within one running `App`;
  never persist a token or compare token values across separate runs.
- Verify an arrangement-only change does not initiate recovery and record when
  it produces no Clerestory monitor-lifetime signal. A fallback intervention
  cancels only the current return intent.
- If physical behavior exposes a defect, fix only the named monitor/recovery/
  restore owner, add an automated regression where possible, rerun Clerestory
  gates, and record the corrected result.

**Files:**

- `crates/bevy_clerestory/examples/restore_after_reconnect/main.rs` — physical
  probe; change only for a proven diagnostic/behavior defect.
- `crates/bevy_clerestory/examples/restore_after_reconnect/README.md` — macOS
  rows/evidence.
- `crates/bevy_clerestory/src/monitors/identity/native.rs` — macOS UUID
  qualification fixes only if evidence disproves it.
- `crates/bevy_clerestory/src/monitors/identity/configuration/macos.rs` — macOS
  display-reconfiguration notification fixes only if observed.
- `crates/bevy_clerestory/src/monitors/topology.rs` — lifecycle/order fixes only
  if observed.
- `crates/bevy_clerestory/src/recovery/fallback_and_return.rs` — transition
  fixes only if observed.
- `crates/bevy_clerestory/src/restore/restore_attempt.rs` — attempt fixes only
  if observed.

**Constraints from prior phases:** Phase 15 freezes the automated/public
baseline. Physical evidence may correct an implementation defect but must not
weaken identity or capability gates. `MonitorId` is process-local and may be
compared across entity lifetimes only inside one running `App`.

**Acceptance gate:** Every applicable macOS scenario has an evidence row and
expected/actual result; unavailable hardware cases are explicitly marked rather
than inferred. Any source correction has a regression test and green
Clerestory Build/Test/Lint gates.

### Phase 17 — Record the Windows physical matrix  · status: todo

#### Work Order

**Goal:** Prove Windows panel identity, relocation/lifetime, intervention,
fullscreen, and entity-scoped real DPI behavior.

**Spec:**

- Execute the shared script on Windows and record the same core identity,
  dock/port, duplicate, reorder, zero-display, reconnect-order, rapid-hotplug,
  mode, and cross-DPI scenarios as Phase 16.
- Confirm verified evidence identifies a physical panel rather than device name
  or adapter and remains unverified when descriptor/serial evidence is missing
  or duplicated.
- Compare `MonitorId` only across entity lifetimes in one running `App`; record
  evidence rather than token equality across separate runs.
- Verify concurrent cross-DPI windows cannot advance each other's attempts and
  exclusive-fullscreen surface creation/restore remains correct.
- Record entity survival versus linked cascade and apply only the mitigation
  selected from causal evidence.
- Fix proven platform defects with automated regressions and rerun gates; never
  promote weak identity to make a row pass.

**Files:**

- `crates/bevy_clerestory/examples/restore_after_reconnect/main.rs` — Windows
  probe corrections only if needed.
- `crates/bevy_clerestory/examples/restore_after_reconnect/README.md` — Windows
  rows/evidence.
- `crates/bevy_clerestory/src/monitors/identity/native.rs` — Win32 display-path
  and EDID acquisition fixes only if observed.
- `crates/bevy_clerestory/src/monitors/identity/edid.rs` — panel-evidence
  qualification fixes only if observed.
- `crates/bevy_clerestory/src/monitors/identity/configuration/windows.rs` —
  Win32 display-configuration notification fixes only if observed.
- `crates/bevy_clerestory/src/windows_dpi_fix.rs` — entity/attempt DPI fixes
  only if observed.
- `crates/bevy_clerestory/src/restore/target_position/application.rs` —
  fullscreen application fixes only if observed.
- `crates/bevy_clerestory/src/restore/restore_attempt.rs` — attempt fixes only
  if observed.

**Constraints from prior phases:** Phase 15 supplies the stable baseline and
Phase 16 established the report schema; platform evidence remains independent.
`MonitorId` is process-local and never comparable across application runs.

**Acceptance gate:** Every applicable Windows scenario has an evidence row and
expected/actual result; unavailable hardware is explicit. Any correction has a
regression test and green Windows Clerestory Build/Test/Lint gates.

### Phase 18 — Record the X11 physical matrix  · status: todo

#### Work Order

**Goal:** Prove X11 panel identity, monitor lifetime, frame-compensated
placement, fullscreen, intervention, and DPI behavior.

**Spec:**

- Execute the shared script in an X11 session and record the core matrix.
- Confirm a RandR CRTC/connector alone never verifies a physical panel; require
  stable descriptor/serial evidence and preserve permanent duplicate ambiguity.
- Compare `MonitorId` only across entity lifetimes in one running `App`; record
  evidence rather than token equality across separate runs.
- Exercise negative origins, arrangement and connected-entity scale changes,
  1x↔2x cross-DPI reconnect, windowed placement, borderless/exclusive
  fullscreen, zero displays, non-target-first return, and rapid hotplug. Record
  unchanged-entity arrangement/resolution/scale cases as the documented
  no-refresh limitation rather than recovery transitions.
- Prove `X11FrameCompensated` remains between preparation and application and
  the thin primary `PreStartup` flush still matches runtime placement.
- Record linked-cascade evidence and apply only the selected mitigation.
- Fix proven defects with automated regressions and rerun gates.

**Files:**

- `crates/bevy_clerestory/examples/restore_after_reconnect/main.rs` — X11 probe
  corrections only if needed.
- `crates/bevy_clerestory/examples/restore_after_reconnect/README.md` — X11
  rows/evidence.
- `crates/bevy_clerestory/src/monitors/identity/native.rs` — X11 RandR/EDID
  acquisition fixes only if observed.
- `crates/bevy_clerestory/src/monitors/identity/edid.rs` — panel-evidence
  qualification fixes only if observed.
- `crates/bevy_clerestory/src/monitors/identity/configuration/x11.rs` — RandR
  configuration-notification fixes only if observed.
- `crates/bevy_clerestory/src/x11_position_fix.rs` — frame-compensation fixes
  only if observed.
- `crates/bevy_clerestory/src/restore/winit_info.rs` — startup ordering fixes
  only if observed.
- `crates/bevy_clerestory/src/restore/restore_attempt.rs` — runtime ordering
  fixes only if observed.

**Constraints from prior phases:** Phase 15 supplies the stable baseline and
the shared report distinguishes physical proof from automated assertions.
`MonitorId` is process-local and never comparable across application runs.

**Acceptance gate:** Every applicable X11 scenario has an evidence row and
expected/actual result; unavailable hardware is explicit. Placement/fullscreen
rows demonstrate compensation order. Any correction has a regression and green
X11 Clerestory Build/Test/Lint gates.

### Phase 19 — Record the Wayland physical matrix  · status: todo

#### Work Order

**Goal:** Prove Wayland behavior without claiming client-controlled windowed
placement or unsupported exclusive fullscreen.

**Spec:**

- Execute the shared script in the available Wayland compositor(s) and record
  the core identity/lifetime/reconnect matrix.
- A `wl_output` object ID alone remains `Unverified`. Record whether the
  compositor exposes equivalent stable physical-panel evidence; do not infer
  continuity from output name, position, or index.
- Compare `MonitorId` only across entity lifetimes in one running `App` if the
  compositor supplies qualified evidence; never compare tokens across runs.
- Windowed capture must be
  `CapturedWindowPosition::CompositorControlled`, project
  `logical_position: None`, never emit `WindowPosition::At`, and leave
  `FallbackAndReturn` unarmed.
- Exercise borderless fullscreen separately: when identity is verified and the
  compositor/winit path supports monitor selection, it may target the returned
  output without a coordinate.
- Exercise exclusive fullscreen separately and record it as unsupported/not a
  return mechanism.
- For `ApplicationControlled`, verify factual availability and supported
  size/mode application; compositor placement mismatch remains explicit.
- Record entity survival/cascade and fix only proven defects with automated
  regressions.

**Files:**

- `crates/bevy_clerestory/examples/restore_after_reconnect/main.rs` — Wayland
  probe corrections only if needed.
- `crates/bevy_clerestory/examples/restore_after_reconnect/README.md` — Wayland
  rows/evidence.
- `crates/bevy_clerestory/src/monitors/identity/native.rs` — compositor evidence
  qualification fixes only if observed.
- `crates/bevy_clerestory/src/monitors/identity/registry.rs` — Wayland
  verification/capability gating fixes only if observed.
- `crates/bevy_clerestory/src/monitors/identity/configuration/mod.rs` — Wayland
  configuration-generation capability fixes only if observed.
- `crates/bevy_clerestory/src/platform.rs` — Wayland capability fixes only if
  observed.
- `crates/bevy_clerestory/src/persistence/captured_window_state.rs` — typed
  position/projection fixes only if observed.
- `crates/bevy_clerestory/src/recovery/fallback_and_return.rs` — capability
  gate fixes only if observed.

**Constraints from prior phases:** Phase 15 supplies the stable baseline.
Wayland's lack of client-controlled windowed positioning is a fixed contract,
not a test failure to work around. `MonitorId` remains process-local and never
comparable across application runs.

**Acceptance gate:** Every applicable Wayland scenario has an evidence row and
expected/actual result; compositor and unavailable-hardware limits are explicit.
Windowed, borderless, and exclusive modes have separate results. Any correction
has a regression and green Wayland Clerestory Build/Test/Lint gates.

### Phase 20 — Release bevy_clerestory  · status: todo

#### Work Order

**Goal:** Publish the validated reconnect recovery API from the existing 0.2
development line without changing Hana's dependency prematurely.

**Spec:**

- Require green Phase 15 automated gates and completed Phase 16–19 evidence
  rows; unresolved platform failures block release.
- Finalize `README.md` and `CHANGELOG.md` with the shipped API, capability
  limits, physical evidence boundary, and migration from 0.1.1.
- Use the repository's full `release` skill for `bevy_clerestory`, targeting
  `0.2.0` from the current `0.2.0-dev` line unless the release workflow exposes
  a conflicting version rule.
- Run the final Clerestory workspace Build, Test, and Lint gates required by the
  release workflow; publish and verify the crate/package through that workflow.
- Do not remove Hana's local override until the published package is verified.

**Files:**

- `Cargo.toml` — workspace release version/dependency metadata.
- `Cargo.lock` — final Clerestory resolution.
- `crates/bevy_clerestory/Cargo.toml` — package version and release metadata.
- `crates/bevy_clerestory/README.md` — released API/limits.
- `crates/bevy_clerestory/CHANGELOG.md` — finalized release entry.
- `crates/bevy_clerestory/examples/restore_after_reconnect/README.md` — final
  evidence reference.

**Constraints from prior phases:** Phase 15 freezes automated behavior; Phases
16–19 supply required native evidence. Hana remains on the local checkout until
publication is independently verified.

**Acceptance gate:** The full `release` workflow completes for
`bevy_clerestory` 0.2.0, the published package is verified, release metadata and
tag/changelog are consistent, and no Hana dependency file changes in this
phase.

### Phase 21 — Move Hana to the published release  · status: todo

#### Work Order

**Goal:** Remove Hana's temporary checkout override and verify the real
application against the published Clerestory recovery release.

**Spec:**

- Replace the temporary path/patch override with the verified published
  `bevy_clerestory` 0.2.0 dependency and refresh `Cargo.lock`.
- Confirm no path or `[patch.crates-io]` entry still points at this checkout.
- Build and test the same editor, screen, conduit, close, identity, restore, and
  cancellation behavior against the registry package.
- Run Hana's final workspace Build, Test, and full `clippy` skill gates.
- Do not change the stabilized public API or reintroduce local duplicated
  recovery state during dependency handoff.

**Files:**

- `../hana/Cargo.toml` — remove temporary override/use published version.
- `../hana/Cargo.lock` — resolve the published package.
- `../hana/crates/hana/Cargo.toml` — update package declaration if required by
  workspace layout.
- `../hana/crates/hana/src/main.rs` — read-only verification target unless a
  package-only compatibility defect appears.
- `../hana/crates/hana/src/window_recovery.rs` — read-only verification target.
- `../hana/crates/hana/src/screens/connection.rs` — read-only verification
  target.
- `../hana/crates/hana/src/conduit/jack.rs` — read-only verification target.
- `../hana/crates/hana/src/conduit/window.rs` — read-only verification target.

**Constraints from prior phases:** Phase 20 supplies the verified published
version. Phases 13–15 already converged Hana against identical local source;
this phase is dependency handoff, not API redesign.

**Acceptance gate:** Hana resolves `bevy_clerestory` 0.2.0 from the registry with
no local override; final Hana workspace Build, Test, and Lint gates are green;
the editor and output integration tests remain green against the published
artifact.
