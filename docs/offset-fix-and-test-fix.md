# Offset persistence fix + test hardening

Plan for two linked defects found in Phase 15:

1. **Format defect.** The persisted window position is an absolute logical desktop coordinate. It
   only means anything paired with the monitor scale that produced it, and that pairing is not used
   on restore. A monitor scale change between save and restore silently relocates the window —
   in the observed case to physical `(-10320, 0)`, off the left edge of every display.
2. **Test defect.** The suite could not see it. Seven specific gaps, listed in §3.

Both were found by hand, not by the suite. §3 exists so that stops being true.

> Review status: both cycles of a 2-cycle `/team_review` are folded in. Corrections with a single
> correct outcome are applied to the text below and logged in §6. Open choices are in §7.
> Cycle 2 verified cycle 1 against source: A1, A2 and A12 confirmed sound; A5, A6, A10 and A15 were
> each partly wrong and are corrected here.

---

## 1. Format defect

### What is stored today (v2)

`CapturedWindowPlacement` holds the correct thing in memory:

```rust
CapturedWindowPosition::Restorable { logical_offset: IVec2 }   // offset from the monitor origin
```

`rebased_position()` restores from it correctly — `live_monitor.physical_position + offset *
live_monitor.scale`. That path has no bug.

The **persisted** path throws the offset away. `persisted_logical_position()` converts it to an
absolute logical desktop coordinate:

```rust
logical_monitor_origin = monitor_snapshot.physical_position / captured_scale
persisted             = logical_monitor_origin + logical_offset
```

and `compute_target_position()` reads it back as `physical = persisted * target_scale`.

### Why that breaks

`logical_monitor_origin` is a function of scale. Monitor 2 in the test VM sits at physical
`x = -6880`; its logical origin is `-6880` at scale 1.0 and `-4587` at scale 1.5. A position written
under one scale denotes a different place when read under another. `monitor_scale` **is** saved and
documented as "informational, not used during restore" — so the one value that would make the
coordinate interpretable is deliberately ignored.

The in-process path is immune because it never round-trips through the absolute form. The bug
therefore only appears across a restart, which is exactly the case the suite covers least.

### Fix: persist the offset

#### `persistence/window_state.rs`

```rust
/// Persisted window placement relative to its monitor.
pub(crate) enum PersistedPosition {
    /// Offset from the target monitor's top-left origin, in logical pixels. Written by v3.
    MonitorOffset(IVec2),
    /// Absolute logical desktop coordinate written by format v1/v2, carried together with the
    /// monitor scale in effect when it was written. Rebased when the target monitor is known.
    Unrebased(UnrebasedDesktopPosition),
    /// No position was saved, or the saved position was rejected.
    Unpositioned,
}
```

`PersistedWindowState.logical_position: Option<(i32, i32)>` is replaced by
`position: PersistedPosition`.

**`Unrebased` must have exactly one constructor, and two mechanisms are needed to get that.**

*Visibility.* `pub(super) fn from_legacy` does **not** deliver it. From `persistence::window_state`,
`pub(super)` means visible throughout `persistence` — and seven of the ten `PersistedWindowState { .. }`
literal sites are inside that module, **including both production sites** (`format.rs::into_current`
and `captured_window_state.rs::project`). It blocks three of ten and neither one that matters.
`pub(in crate::persistence::format)` is not writable from `window_state.rs` either — `pub(in path)`
requires an ancestor and `format` is a sibling. Put the newtype in a **private child module of
`format`**, where plain `pub(super)` already reaches `format` and nothing else, and `pub(super) use`
the type onward. Cycle 1 proposed spelling this `pub(in crate::persistence::format)`; the style guide
forbids every `pub(in ...)` form (`no-pub-in-path.md` — the restricted path encodes a layering
boundary that belongs in the module tree). Relocating into the child module is exactly the remedy
that rule prescribes, and it gives the same confinement.

*Serde.* A derived `Deserialize` is a second constructor, and it is the one reading untrusted files.
Field privacy does not apply, so v3 decode builds the struct straight from RON:

```ron
position: Unrebased((logical: (-6880, 0), captured_scale: 0.0)),   // never calls from_legacy
```

which is precisely the two failure modes the validation exists to stop. This is reachable in normal
operation: any `Unrebased` entry re-serializes as v3 on the next save and is read back unvalidated
thereafter. Use `#[serde(try_from = "UnrebasedWire", into = "UnrebasedWire")]` with a private wire
struct so `from_legacy` is the single constructor on both paths.

```rust
pub(crate) struct UnrebasedDesktopPosition {
    logical:        IVec2,
    captured_scale: f64,
}

impl UnrebasedDesktopPosition {
    /// Sole constructor. Rejects a `captured_scale` that is not finite and greater than zero.
    /// `pub(super)` from inside `format`'s private child module reaches `format` alone.
    pub(super) fn from_legacy(
        logical: IVec2,
        captured_scale: f64,
    ) -> Option<Self>;
}
```

**`Unpositioned`, not `CompositorControlled`.** The state is reachable off Wayland too — any time
`window.position` is not `WindowPosition::At` — and it now also means "the saved value was
rejected". Naming it after Wayland invites the mapping to be got wrong.

#### Migration arithmetic

Run once per entry, when the target monitor resolves:

```
offset = saved_logical - round(monitor.physical_position / captured_scale)
```

**The divisor is the *saved* scale, not the live one.** A helper that reads `MonitorInfo::scale`
reduces the whole restore to `saved × live_scale` — today's bug, byte for byte — and every
self-consistent test fixture still passes. So the helper takes the scale explicitly and there is no
zero-argument variant:

```rust
impl MonitorInfo {
    pub(crate) fn logical_origin_at_scale(&self, scale: f64) -> IVec2;
    pub(crate) fn physical_from_logical_offset(&self, logical_offset: IVec2) -> IVec2;
    pub(crate) fn logical_from_logical_offset(&self, logical_offset: IVec2) -> IVec2;
}
```

`MonitorInfo` is already `pub`, so `pub(crate)` methods do not widen the crate's public API.

**`captured_scale` is untrusted arithmetic input.** `bevy_kana`'s `ToI32 for f64` is `as i32`, so
`scale: 0.0` with origin `-6880` gives `-inf → i32::MIN` and `saved - i32::MIN` overflows — a debug
panic. With origin `0` it gives `NaN → 0`, so the absolute coordinate becomes the offset: `-10320`
again. §3's seeded fixtures produce this deliberately.

**Monitor resolution.** Two guards, both required:

- `build_persisted` must keep matching `MonitorResolutionSource` **first**; a `FallbackToPrimary`
  yields `TargetUnavailable`, never a rebase against `monitors.first()`.
- `build_persisted` must consult `recovery_monitor` the way `build_captured` already does through
  `resolve_captured_monitor`. It currently does not, and **the persisted path is reachable from
  recovery**: `accept_explicit_restore_requests` accepts a `None` entity when `placement(..).is_some()`,
  which holds for `PersistedOnly`, and `accept_automatic_restore_intents` gates on `is_bound_to`,
  true after `seed` + `bind_and_freeze`. Both describe the state a mismatched startup restore leaves
  behind. `recovery_monitor` comes from a *verified* `MonitorId`; the saved index does not.

That second guard matters more under v3 than it did under v2. Today a wrong index only mis-picks
`target_scale` and the fullscreen target — the coordinate is absolute, so the window still lands at
the saved desktop position. Under v3 the index **is** the anchor: `origin(by_index(n)) + offset`.
A renumbered display puts the window on the wrong panel, and `on_window_restored → promote` writes
that position back as a fresh `MonitorOffset` — permanently wrong and no longer distinguishable from
a correct offset. A v3 file carries no monitor identity (`project()` drops `MonitorIdentity`), so
index stability now governs position, not just scale. §7-D5.

**Accuracy.** The migration uses the monitor's *current* physical origin with the *saved* scale, so
its error is `(origin_at_save − origin_now) / captured_scale`. On this VM a 1.75 → 1.5 change on the
3840-wide monitor shifts its neighbours ≈366 physical px; 1.75 → 1.0 shifts ≈1646 px. At a saved
scale of 1.0 that *is* the offset error in logical pixels — larger than a typical window, so the
result lands off the monitor. §7-D1 decides what to do about that.

**"Applies at most once" was wrong and is withdrawn.** The only `Unrebased → MonitorOffset`
conversion is `on_window_restored → promote`, i.e. a fresh live capture replacing the entry — the
migrated value is never itself written back. So:

- A `RememberAll` entry for a window never opened re-serializes as `Unrebased` every save and
  re-migrates against the live layout each launch.
- A startup restore that mismatches leaves the entry `Frozen` and does the same.

This is deferred interpretation, not incomplete migration: the value is never rewritten, so each
launch starts from the same pair. §7-D2 decides whether to force completion.

#### `persistence/format.rs`

**The version bump is atomic with the v2 arm.** `decode` currently uses `CURRENT_STATE_VERSION`
*as* the v2 arm; there is no `PERSISTED_STATE_VERSION_V2`. Bumping the constant to 3 without adding
the arm sends every existing v2 file to `unsupported → None → unwrap_or_default()` → and the first
dirty frame writes an empty map over it. The constant, the new arm, and the bump are one edit. The
regression test must hardcode `version: 2`, not reference the constant.

- `CURRENT_STATE_VERSION` 2 → 3. `PERSISTED_STATE_VERSION_V2 = 2` joins `PERSISTED_STATE_VERSION_V1`
  in `persistence/constants.rs`.
- Add `WindowStateV2` **plus** `PersistedEntryV2` / `PersistedStateV2`. `into_current()` produces
  `Unrebased(from_legacy(logical, monitor_scale)?)`, or `Unpositioned` when the position was `None`.
  Move `default_monitor_scale` next to `WindowStateV2` — it is the `#[serde(default)]` for the v2
  wire field, which is exactly the `from_legacy` divisor. It is **not** dead.
- `WindowStateV1::into_current()` produces `Unrebased(from_legacy(logical, 1.0))`.
- Keep the whole `PERSISTED_STATE_VERSION_V1` path — it is the only v1 decoder, three golden tests
  depend on it, and the module doc promises no state loss.
- Update the module-doc "Supported formats" table and the `RON_HEADER` text; both currently state
  that spatial values are logical and that `monitor_scale` is unused, and both become wrong.
- v3 serializes `PersistedPosition` directly, so an entry not yet rebased round-trips losslessly:

```ron
position: MonitorOffset((120, 80)),
position: Unrebased((logical: (-6880, 0), captured_scale: 1.0)),
position: Unpositioned,
```

Lossless round-tripping is required because state loads in `PreStartup`, before `Monitors` is
populated. (The mechanism is not "winit has no monitors" — `WinitMonitors` is available and
`MonitorsInitialized` chains *before* `PersistenceLoaded`. `Monitors` is empty because
`build_monitors` iterates `Query<(Entity, &Monitor)>` and Bevy spawns those entities on `Resumed`.
The conclusion is the same; the mechanism determines where §7-D2's system could go.)

#### Delete the now-dead scale fields

`CapturedWindowPlacement.captured_scale` has **one production site**, where it equals
`monitor_snapshot.scale`; the other thirteen occurrences are inside `#[cfg(test)]`.
`PersistedWindowState.scale` has zero production readers — the `restore_window` display uses
`window.resolution.scale_factor()`, the Python harness derives scale from BRP plus `MONITOR_i_SCALE`,
and `PersistedWindowState` is not `Reflect` so it never reaches BRP at all.

Both go. The save-time scale then survives in exactly one place,
`UnrebasedDesktopPosition.captured_scale`, which is the type-level statement that it means something
only when paired with a legacy coordinate. Deleting `captured_scale` is independent of the format
change and lands as **step 0**; `PersistedWindowState.scale` cannot move with it because the v2
decoder consumes it.

#### Shared geometry

`persisted_logical_position` and `rebased_position` both convert between a monitor origin and an
offset; they get the `MonitorInfo` helpers. `compute_target_position`'s `PersistedCoordinate` arm
does **not** — it computes `logical × target_scale` with no origin term at all. It acquires a
monitor-relative conversion only when `PersistedOffset` exists, which is step 2, not step 1.

#### Call sites

| File | Change |
|---|---|
| `persistence/window_state.rs` | `PersistedPosition`; `position` replaces `logical_position`; delete `scale` |
| `persistence/format.rs` | v3 encode; `UnrebasedDesktopPosition` in a private child module with the serde wire type; `WindowStateV2` + wrappers; v2 arm; module-doc table; `RON_HEADER` |
| `persistence/constants.rs` | add `PERSISTED_STATE_VERSION_V2` |
| `persistence/captured_window_state.rs` | delete `persisted_logical_position` and `captured_scale`; `project()` emits `MonitorOffset`; use the `MonitorInfo` helpers |
| `restore/target_position/monitor.rs` | `resolve_target_monitor_and_position` drops its position argument — verified dead at both call sites |
| `restore/target_position/target.rs` | `PersistedCoordinate` → `PersistedOffset(IVec2)`; physical via `MonitorInfo` |
| `restore/restore_attempt.rs` | `build_persisted` consults `recovery_monitor`, then matches the resolution source, then resolves the offset; extract the `PreparedWindowPosition` match into a free function so step 2b can test it |
| `events.rs`, `target.rs` | reword three doc sites (below) |

#### What changes for consumers

`TargetPosition.logical_position` keeps its meaning — absolute logical, for event reporting — but is
derived from the **live** monitor rather than the save-time snapshot. The earlier claim that reported
values are unchanged was wrong: it is false in precisely the scenario this fix targets. Three doc
sites carry the same wording and are all fed by that field: `events.rs` `WindowRestored`,
`events.rs` `WindowRestoreMismatch.expected_logical_position`, and `target.rs`'s field comment. The
`WindowRestored` doc must also say that `None` now means *either* nothing was saved *or* the saved
value was rejected — otherwise the two are indistinguishable outside the log.

Rejections are reported with `warn!`, matching the crate's existing convention of
`[function_name]` + `[{window_key}]` for anything the user loses: in `format.rs` for a rejected
`captured_scale`, in `build_persisted` for a §7-D1 drop. No new public event —
`WindowRestored { logical_position: None }` and `WindowRestoreMismatch` already carry the outcome.

#### Fullscreen is not exempt

Position *is* applied for fullscreen restores — `apply_initial_move`'s fullscreen branch on Windows
cross-DPI, and `MoveToMonitor` on X11 — while `check_settle_matches` skips position for fullscreen,
so nothing verifies it. The migration reaches these entries.

#### BRP type paths

`run_test.py` addresses `RestoreDiagnostics` by its private module path
(`bevy_clerestory::restore::target_position::target::RestoreDiagnostics`). §1 already edits that
file. An unregistered path in `data.components` / `filter.with` returns an empty result with no
error, and in `data.option` it is silently dropped — reported by the harness as "RestoreDiagnostics
missing (restore did not run?)", pointing at the wrong subsystem. The crate already solved this for
five `monitors` types with `#[type_path = "bevy_clerestory::monitors"]` plus a pin test. Do the same
for `TargetPosition` and `RestoreDiagnostics`.

---

## 2. What already landed (context, not new work)

Uncommitted on `feature/reconnect`:

- **Arrival signal** — `current_monitor_reached_target_scale`. Ends `WaitingForScaleChange` when the
  window is on a monitor at `target_scale`. Gated to `CompensateSizeOnly`.
  **Correction:** it matches on scale alone and ignores `monitor_index`. `target_position.monitor_index`
  is in scope and `CurrentMonitor` derefs to `MonitorInfo`, so the check is a one-line addition.
  The failure it prevents is *not* a wrong size — a window on a 1.5 monitor genuinely is at 1.5 DPI,
  so `ApplySize` applies the right size. The harm is that settle then compares against the *target*
  monitor's geometry while the window sits on a different one, producing a mismatch two seconds
  later. The reachable trigger is the off-desktop coordinate: `SetWindowPos` to `(-10320, 0)` leaves
  `MonitorFromWindow(…NEAREST)` naming a neighbouring 1.5 monitor. The existing unit test uses a
  monitor index equal to the target's, so it stays green either way — add a negative case or the
  correction ships untested.
- **Deadline** — `SCALE_CHANGE_WAIT_TIMEOUT_SECS` + `TargetPosition.scale_change_wait`, for both
  `CompensateSizeOnly` and `HigherToLower`. Without it a startup restore that never sees a DPI
  transition waits forever with the window hidden.
  **Correction:** it equals `SETTLE_TIMEOUT_SECS` and `RUNTIME_RESTORE_TIMEOUT_SECS`, all 2.0 s, so
  for a *recovery* restore both deadlines expire in the same frame and which one wins is
  system-order dependent. Not a defect — both reveal the window — but the claim holds
  unconditionally only for startup restores.
- **`WM_DPICHANGED` forward** — `windows_dpi_fix.rs`, for winit #4041.

**Three production doc comments are wrong** and were not caught by cycle 1: the arrival signal's doc,
`SCALE_CHANGE_WAIT_TIMEOUT_SECS`'s doc, and the unit test's doc all justify themselves with "a
restore that lands on a monitor already at its target scale". `scale_strategy` returns
`ApplyUnchanged` when `|starting − target| < ε`, so at entry to `WaitingForScaleChange` the window is
always on a monitor at `starting_scale` — that state is unreachable. The real justification is a lost
`WM_DPICHANGED` for a hidden window. Fix all three.

---

## 3. Test hardening

### The seven gaps

1. **No on-screen assertion.** A window restored to `(-10320, 0)` and revealed passes every existing
   check.
2. **Save and restore always happen in one monitor configuration.**
3. **Every case starts from harness-written, self-consistent state.** Files that disagree with the
   live layout — the real-world case — are never tested.
4. **Every cross-DPI case has a real DPI transition**, so `matching_scale_change` always resolves.
5. **A hang is indistinguishable from infrastructure failure.** `wait_for_restore` `die()`s →
   `sys.exit(2)` → `HARNESS_ERROR`; `validate_all_windows` is never reached.
6. **The VDD guarantees a scale difference.** *Closed by construction* once addition 4 folds into a
   seeded fixture — matching-scale hardware is no longer needed at all.
7. **The expected-position formula is the production formula re-typed.** `validate_window` computes
   `exp_x = round(logical × scale)`; production computes `(logical × target_scale).round()`.

### The additions

| # | Addition | Detail |
|---|---|---|
| 1 | **On-screen assertion** | The `Window.visible` flag is near-tautological — set in the same path that emits `WindowRestored`, which the harness already awaits. The work is done by **containment in the case's target monitor**. Computable for every native case: every fixture declares `monitor_index`, role-based cases substitute `${HIGH_SCALE_MONITOR_INDEX}` before parsing, origin comes from `MONITOR_i_POS_X/Y` and extent from `MONITOR_i_WIDTH`/`_HEIGHT` (there is no `MONITOR_i_SIZE_*`) — all physical, same frame as `Window.position`. **Not** computable on Wayland, and all of `linux.json` is Wayland: the new field needs the same `backend == "wayland"` guard the position branch has. macOS's `position_readback_offset: [0, 30]` must be applied to the rect or a bottom-edge window overflows by 30 px. `validate_window` has no `else` arm, so an unrecognized field name is silently ignored — add `else: die(...)` or this ships as a no-op. |
| 2 | **Seeded adversarial fixtures** | A coordinate off every display, an out-of-range monitor index, and `captured_scale: 0.0`. The off-desktop value stays `${}`-derived (`${MONITOR_0_LOGICAL_POS_X-20000}`); a hardcoded `-10320` is off-desktop only on this VM's layout. The out-of-range-index fixture needs a new `expected_monitor` key on `WindowConfig` (mirroring the existing `expected_mode`), because `resolve_target_monitor_and_position` falls back to `monitors.first()` while `validate_window` compares against the out-of-range value parsed from the fixture — so the case fails by construction otherwise. Expected outcome depends on §7-D1. |
| 3 | **Deadlock vs timeout classification** | Classify on three axes: stuck restore state, process alive, and **BRP responsive** — the third separates a wedged event loop from a spinning state machine, the ambiguity that produced two wrong root causes. A19 is answered: `TargetPosition` *is* registered (`reflect_auto_register`) and *does* serialize transitively. But a serialization failure is invisible from BRP — the component is dropped and warned into the *app's* log — so "absent" conflates restore-finished, never-started, and serialize-failed. Add a fourth `with_method_main` entry, `clerestory/restore_state`, following the two existing snapshot methods' pattern of a dedicated `#[derive(Serialize)]` struct rather than exposing a component; make it primary and drop the component-query path. Use `data.has` (never serializes) for presence, `data.option` for values, never `data.components`. |
| 4 | *(folded into addition 2)* | The original spec targeted a state the selector cannot produce: `Platform::scale_strategy` takes only the two *live* scales, so launching at the target scale yields `ApplyUnchanged` — no wait, no deadline — and `validate_launch_monitor` fails the case for not exercising the cross-DPI path. The reachable version is a seeded fixture whose target monitor differs in scale from the launch monitor *and* whose offset lands off every display, so the deadline is the only exit. No new hardware. Assert the deadline actually fired via the existing `expected_log_warning` key. |
| 5 | **Scale change between save and restore** | Direct regression test for §1. Three preconditions: **(a)** `dpi_scale.ps1` needs exit codes — `[Dpi]::Run` returns a `StringBuilder` string, the script's last statement emits it, and there is no `exit`, so it returns 0 on `NO MATCH`, on GET `rc != 0`, and on SET `rc != 0`; `run_suite.py` only checks `returncode`, so that guard is unreachable today. Add `exit 1` on all three, keep the diagnostic on stdout. **(b)** the post-condition cannot live where §3 first put it — between `shutdown_app()` and relaunch no process exists, so `clerestory/monitor_snapshot` is unavailable, and a re-GET returns *relative* scale indices against the OS recommendation, not scale factors. Put it in `run_test.py` after the relaunch's `wait_for_restore()`: one `monitor_snapshot` against the running app, rebuild the `MONITOR_*` map, match the target by `MONITOR_i_NAME` not index. If the target's scale still equals the pre-change value, report UNAVAILABLE — never pass. **(c)** retry wraps the setup-phase `dpi_scale.ps1` call, not the case body; one retry, ~5 s. |
| 6 | **Deadline invariant (unit)** | `scale_change_wait_tests` already covers the three *exit* transitions. The uncovered one is entering. State it conditionally: `begin_cross_dpi_restore` early-returns for `physical_position.is_none()` — centers, reveals, settles — without arming the timer. The invariant is "entering `WaitingForScaleChange` arms the timer", not "the timer is always armed". |
| 7 | **Inverted expectations** | The original proposal was `physical_from_logical_offset` re-typed: better anchor, same multiply-and-round on both sides. Invert instead: `round((actual − origin) / live_scale) == fixture_offset`. Exact for scale ≥ 1 (`|round(o·s)/s − o| ≤ 0.5/s ≤ 0.5`) and no supported configuration is below 1 — Windows' source-DPI minimum is 100%, macOS is 1.0/2.0, X11 exposes one uniform `Xft.dpi` — but derive the margin as `ceil(0.5 / scale)` rather than hardcoding 1. Note the trade-off: the inverse assertion is *weaker* than the forward one, ~±3 physical px of slack at scale 2. **The "three forward conversions" count was wrong.** The real forward-formula assertions are `validate_window` and `_check_position_saved`. `verify_mutations`' second site is a fallback that only runs for size-only mutations. `apply_mutations` computes a position to **set** over BRP — a command, not an expectation, with nothing to invert; it needs the *opposite* change (see addition 10). The remaining site is `_logical_to_physical_size`, a truncating conversion that is not a bijection (801 at 1.5 → 1201 → 800) — inverting it manufactures ±1 failures. Leave it alone. |
| 8 | **`changeable_scale` requirement key** | Addition 5 needs a *changeable* scale, not two different ones. `run_test.py` duplicates the **`different_scales`** rule, not the Wayland rule. Also fix `write_case_result`'s hardcoded `missing_capability="different-scales"`. Under the zero-error basedpyright rule this needs TypedDict work: `changeable_scale: bool` on `TestRequirements`, offset keys on `RonWindowValues`, and `expect_position_saved: list[str]` on `PersistenceValidation` — the last is a **live typing defect today**, read by `validate_persistence` but never declared. |
| 9 | **Saved-file offset assertion** | Cheapest cross-OS guard: no display tooling, no extra launches, runs on macOS and Linux where every other item here is Windows-only or single-scale. Two corrections. **It is a no-op on its only current host case**: `save_position_on_unmoved_window` targets monitor 0, whose origin is `(0, 0)` by definition, so `MonitorOffset((dx, dy))` and the absolute coordinate are byte-identical — a regression writes the same file. Add it to `same_monitor_restore_mon1` on all three platforms and keep monitor 0 as a second host. **It has no observed physical position to invert**: `validate_persistence` runs after `shutdown_app()`, and that case's `validate` list omits `position`, so nothing in the run holds it. Assert bounds instead — `0 ≤ dx < MONITOR_i_WIDTH / scale`, using the `monitor_index` the file itself declares. On a non-zero-origin monitor an absolute coordinate lands far outside that band. The file read is confirmed correct: `ron_path` resolves to `configured_persistence_path`, the same path handed to the app as `CLERESTORY_TEST_PERSISTENCE_PATH`. |
| 10 | **Re-anchor the mutation path** | Under v3 `parse_ron_values` yields an **offset** while `apply_mutations` reads it as an absolute logical coordinate. Concretely: `same_monitor_restore_mon1.ron` is `${MONITOR_1_LOGICAL_POS_X}` today, so it becomes `MonitorOffset((0, 0))`; with `position_offset: [300, 200]` the harness computes `0 × scale + 300` and moves the window to physical `(300, 200)` — monitor 0, not monitor 1 at `x ≈ -6880`. `validate_window`'s `monitor_index` check then fails for a reason unrelated to the fix, and `same_monitor_restore_mon0` is unaffected because its origin is zero, so the break is silent in half the mutation cases. Attach the monitor origin to each `RonWindowValues` at parse time from `MONITOR_{monitor}_POS_X/_Y`, already exported. |
| 11 | **Reconnect containment** | `run_reconnect.py` is blind to this change: `generic_cycle_assertions` checks the disconnect edge, key presence, `verified_id`, key uniqueness, and terminal failure — never *where in the monitor* the window is. An offset applied against the wrong origin that still lands on the right verified panel passes everything. Add one assertion: each returned window's position lies within its `current_monitor` rect. The snapshot already exposes both fields; `automatic_cancellation_assertions` reads exactly those. ~10 lines, and the only check that catches a wrong-panel anchor on hardware. Separately: `CLERESTORY_PROBE_PERSISTENCE_PATH` is a fixed per-case directory and both `ProbeProcess.start` and `run_windows_reconnect` retry into it, so attempt N decodes what attempt N−1 wrote — decide deliberately whether it is cleared between retries. |

### Unit tests (step 2b) — cheaper than most of the above, and cross-platform

Four pure-Rust tests, no display. `compute_target_position` takes no `World`; the fixtures
`monitor(index, scale, physical_position)`, `captured(..)` and `persisted()` already exist (all three
need editing when the scale fields are deleted).

One extraction is needed first: `PreparedWindowPosition` is built only inside `build_persisted`,
which needs `&Monitors`, so a hand-constructed variant would skip the very mapping the migration
changes. Extract that match into a free function over
`(MonitorResolutionSource, &PersistedWindowState, &MonitorInfo)`.

1. **Scale-change round trip.** This is the defect itself:

```rust
#[test]
fn scale_change_between_save_and_restore_keeps_the_window_on_its_monitor() {
    let save_monitor = monitor(1, 1.0, IVec2::new(-6_880, 0));
    let live_monitor = monitor(1, 1.5, IVec2::new(-6_880, 0));
    let saved = captured(save_monitor, IVec2::new(120, 80)).project("test");
    // encode → decode → compute_target_position against the same monitor at 1.5
    assert_eq!(target.physical_position, Some(IVec2::new(-6_700, 120)));
}
```

Today's code yields `(-6880 / 1.0 + 120) × 1.5 = -10140`, outside monitor 1's `[-6880, -4320)` span.

2. **Two-path agreement.** `rebased_position()` and `compute_target_position()` agree for the same
   `(offset, monitor)`. Guards against someone re-inlining the formula.
3. **Containment table.** Scales {1.0, 1.25, 1.5, 2.0} × origins {0, ±6880} × offsets; the rect lies
   inside the monitor whenever it fits. Pins the invariant, not one value.
4. **Migration guards.** `captured_scale` of `0.0`, negative and non-finite are rejected — **on both
   the v1/v2 path and the v3 deserialize path**; a v1/v2-only test passes against the broken version.
   Plus: a migrated entry is rewritten as `MonitorOffset` on the next save.

Add a fifth in the same step: a `process_remote_query_request` test pinning the BRP type paths, on
the model of the existing `events.rs` test that already drives `builtin_methods` from `#[cfg(test)]`.

### Which addition catches the observed failure

Addition 3, not addition 1 — the harness died in `wait_for_restore` before any assertion ran.
Addition 1 becomes a catching assertion only *because* the deadline landed: post-deadline the window
is revealed at `(-10320, 0)`, the existing position assertion **passes** (it mirrors production), and
containment is the only check that fails. Addition 9 catches the regression form on hardware that
cannot reproduce the original at all.

### Fixture format change

```ron
// before
logical_position: Some((${HIGH_SCALE_MONITOR_LOGICAL_POS_X+200}, ${HIGH_SCALE_MONITOR_LOGICAL_POS_Y+200})),
// after
position: MonitorOffset((200, 200)),
```

`parse_ron_values` must read **both** formats simultaneously, not swap: the same parser reads the
seeded template *and* the file the app wrote, and §3 keeps one v2 template per platform, so in that
case the fixture is `logical_position: Some(..)` while the written file is `position: MonitorOffset(..)`.
A swap makes the retained template unparseable and `_check_position_saved` reports
`window_key_not_found` silently. Carry a units tag (`absolute` / `offset`) on each parsed entry and
branch on it.

The retained v2 template must declare `monitor_scale: ${MONITOR_n_SCALE}` and
`${MONITOR_n_LOGICAL_POS_X+200}`: discovery's `LOGICAL_POS` is `round(physical / scale)` at the same
scale the migration divides by, so the roundings cancel and the expected offset is `(200, 200)` —
the same value the v3 cases assert. Left at a hardcoded constant, the VM's real 1.5 against a
declared 1.0 would migrate to an offset off by roughly a third of the desktop. Note that
`monitor_scale: 1.0` is *not* universal today — `no_position_same_monitor.ron` carries 1.5, and it
also has `logical_position: None`, so it exercises `Unpositioned` and is not a candidate for the
retained template.

---

## 4. Platform scope

### Production

| Item | Scope | Why |
|---|---|---|
| Arrival signal (+ index check) | Windows in effect | Gated to `CompensateSizeOnly`, which `scale_strategy` only produces on Windows. |
| Deadline | **Cross-OS** | Set for both cross-DPI strategies. |
| `WM_DPICHANGED` forward | Windows only | Confined to `windows_dpi_fix.rs`. |
| Offset persistence (§1) | **Cross-OS** | Shared `persistence/`, shared on-disk format. Needs only a scale change between save and restore — Retina ↔ non-Retina on macOS, xrandr on X11. |

### Tests

| Item | Scope | Notes |
|---|---|---|
| Unit tests (step 2b) | **Cross-OS** | No display hardware at all. The cheapest coverage in the plan. |
| Saved-file offset assertion (9) | **Cross-OS** | Runs anywhere a position is saved. |
| Deadlock classification (3) | **Cross-OS** | `run_test.py` + one BRP method. |
| Deadline invariant (6) | **Cross-OS** | Rust unit test. |
| Inverted expectations (7), mutation re-anchor (10) | **Cross-OS** | `run_test.py`. |
| On-screen assertion (1) | Native only | Uncomputable on Wayland; all of `linux.json` is Wayland. |
| Reconnect containment (11) | **Cross-OS** | `run_reconnect.py`. |
| Seeded fixtures (2, incl. folded 4) | Mechanism cross-OS, values per platform | Derived from discovery. |
| Scale change mid-test (5) | Windows only today | `dpi_scale.ps1` is DisplayConfig. macOS and Linux need their own tool. |
| `changeable_scale` key (8) | **Cross-OS** plumbing, Windows-only capability | |
| VDD provisioning | Windows only | Entirely. |

Only Windows can be run here. macOS and Linux need their own suite run before the shared-code
changes are trusted there.

---

## 5. Order of work

The step 1 / step 2 boundary in the first draft did not hold; this is the corrected sequence.

**0. Delete `captured_scale`.** One production site, thirteen test sites, no behavior change.
*Gate:* fmt, `clippy -D warnings`, unit suite green.

**1. `MonitorInfo` helpers**, moving `persisted_logical_position` and `rebased_position`'s physical
branch onto them. **Not** `target.rs` — there is nothing there to move yet.
*Gate:* unit suite green with **zero** test edits. If a test needs changing here, a helper is wrong.
No test currently pins `rebased_position`'s reported logical value, so switching its receiver to the
live monitor breaks none of the 199 — but that switch belongs in step 2, where it is coherent with
the format change, not here where it would leave the event reporting live-derived while `project()`
still writes the save-time absolute.

**2. Format v3.** `PersistedPosition`, `UnrebasedDesktopPosition` (private child module + serde wire
type), v1/v2 decode with the atomic version bump, `position` replaces `logical_position`, delete
`PersistedWindowState.scale`, `PersistedOffset` in `target.rs`, live-derived reporting,
`build_persisted` consults `recovery_monitor`, `RON_HEADER` + module-doc table, `#[type_path]`,
the three `events.rs`/`target.rs` doc rewords, the three production doc comments in §2.

This is a **compile break**, not a set of test failures: the six shared test helpers reach roughly
117 named tests across `restore_attempt`, `captured_window_state`, `managed`, the three `recovery`
modules, `format`, `load`, `save`, `persistence/mod` and `winit_info`. Tests whose *meaning* changes,
not just their construction:

| Test | Why |
|---|---|
| `captured_window_state::projection_adds_logical_offset_after_converting_fractional_scale_origin` | Asserts the conversion being deleted. Replace with the v2-rebase test. |
| `captured_window_state::compositor_controlled_projection_has_no_position` | `logical_position == None` → `position == Unpositioned`. |
| `format::decode_legacy_single_window_migrates_to_v2`, `decode_v1_migrates_to_v2`, `golden_legacy::decode_golden_legacy_windowed` | Assert `scale == DEFAULT_SCALE_FACTOR` on a deleted field. Become `position == Unrebased(..)`. |
| `format::encode_then_decode_roundtrip` | Asserts `scale == 2.0` on the decoded entry. |
| `format::decode_v2_distinguishes_primary_and_managed_primary`, `decode_v2_rejects_duplicate_keys`, `encode_sets_version_2` | Build `PersistedState { version: CURRENT_STATE_VERSION }`. Once that is 3 they exercise `decode_v3` under v2 names — passing for the wrong reason. Rename, and add v2 counterparts with a hardcoded `2`. |
| `load::legacy_single_window_read_then_save_rewrites_as_v2` | Keep and rename — the only existing test proving an `Unrebased` entry survives a save. |

*Gate:* crate compiles, unit suite green, the table applied.

**2b. The five unit tests** above, plus §7-D1's validation guard if taken.
*Gate:* all green. Display-free, and they gate every hardware step.

**3. Harness units change.** Bi-format `parse_ron_values` with a units tag, monitor origin attached
at parse time, re-anchored `apply_mutations`, inverted `validate_window` and `_check_position_saved`,
v3 templates, one retained v2 template per platform.
*Gate:* the existing Windows suite passes at 175/150/150 with **no new cases** — this step must be
behavior-preserving. `same_monitor_restore_mon1` is what proves it.

**4. Addition 9, then 1, then 6, then 8's plumbing.** 9 first: cheapest, cross-OS, validates step 2
directly. *Gate:* full Windows suite green.

**5. Addition 2 (with 4 folded in), addition 10, addition 11.** Gated on §7-D1.
*Gate:* full Windows suite plus `run_reconnect.py` green.

**6. Deferred to a follow-up:** addition 3, additions 5 + 8's capability, §7-D4.

Run `/rust_style` before writing any Rust — see the process note in §6.

---

## 6. Auto-recorded review findings

Applied to the text above; no user decision needed. Eight expert lenses over two cycles,
`claude`/`opus`/`max`, readonly. **No premise-challenges in either cycle** — all eight agents judged
the design able to reach the intent.

### Cycle 1 (24 findings, condensed)

Migration must use the *saved* scale (A1, critical) · no rebase against a fallback monitor (A2) ·
"applies at most once" withdrawn (A3) · `captured_scale` needs validation (A4) · confine `Unrebased`
construction (A5) · delete the dead scale fields (A6) · reported values *do* change (A7) ·
`Unpositioned` not `CompositorControlled` (A8) · format mechanics: v2 constant, entry/state wrappers,
module doc, `RON_HEADER` (A9) · arrival signal needs the index check (A10) · fullscreen is not exempt
(A11) · addition 4 targeted an unreachable state (A12, critical) · `dpi_scale.ps1` has no
post-condition (A13, critical) · addition 5 needs the post-change layout (A14) · addition 7 was the
production formula re-anchored (A15, critical) · new addition 9 (A16) · retained v2 templates need
`${MONITOR_n_SCALE}` (A17) · `visible` is near-tautological, `MONITOR_i_WIDTH`, missing `else` arm
(A18) · confirm BRP serialization (A19) · new addition 8 (A20) · seeded fixtures stay
discovery-derived (A21) · unit tests as step 2b (A22) · step 1 is not behavior-free (A23) · ±1
rounding margin (A24).

### Cycle 2 — verification of cycle 1

**Confirmed sound:** A1, A2, A12. Also confirmed: A5's ten-literal-site count, A15's diagnosis of
`validate_window`, A19's premise, and cycle 1's conclusion that no half-migrated write is
representable (independently re-derived).

**Corrected:**

| id | Finding | Sev |
|---|---|---|
| B1 | `pub(super) from_legacy` does not confine construction — seven of ten literal sites are inside `persistence/`, including both production ones. Needs a private child module of `format`. | critical |
| B2 | The derived `Deserialize` is a **second constructor**, and it is the one reading untrusted files — v3 decode bypasses `from_legacy` entirely. Needs `#[serde(try_from = ..)]`, and unit test 4 needs a v3 negative case or it passes against the broken version. | critical |
| B3 | `decode` uses `CURRENT_STATE_VERSION` *as* the v2 arm. Bumping it without adding the arm sends every existing v2 file to `None` → empty seed → overwrite. The destructive path is **forward**, not only on downgrade, and an atomic write does not help — the write succeeds, it just writes nothing. | critical |
| B4 | `build_persisted` never consults `recovery_monitor`; the persisted path *is* reachable from recovery, and under v3 the index is the position anchor. | critical |
| B5 | A15 mischaracterized the sites. Real forward assertions: `validate_window` and `_check_position_saved`. `apply_mutations` is a setter with nothing to invert; the size conversion truncates and is not a bijection. | critical |
| B6 | The real break in `apply_mutations` is a **units** change, not a formula one — `same_monitor_restore_mon1` would move to monitor 0, silently, while `mon0` is unaffected. New addition 10. | critical |
| B7 | Addition 9 is a no-op on its only host case: monitor 0's origin is `(0, 0)`, so offset and absolute are byte-identical. | critical |
| B8 | Addition 9 has no observed physical position — `validate_persistence` runs after `shutdown_app()`. Use the bounds form instead. | critical |
| B9 | Addition 5's post-condition cannot be read where it was placed (no live app; GET returns relative indices). Script needs `exit 1`; the check moves after the relaunch. | critical |
| B10 | §5's step 1 / step 2 boundary does not hold — `target.rs` has no origin term to move, and A23 belongs in step 2. Insert step 0. | important |
| B11 | Step 1 breaks **zero** tests (nothing pins the value). Step 2 is a compile break across ~117 tests; seven change meaning. | important |
| B12 | `parse_ron_values` must read v2 *and* v3 simultaneously with a units tag; a swap breaks the retained template silently. | important |
| B13 | The reconnect suite is blind to this change and carries its state file across retries. New addition 11. | important |
| B14 | BRP addresses these components by private module path; a stale path returns empty, not an error. Use `#[type_path]` as the crate already does for `monitors`. | important |
| B15 | A19 answered: `TargetPosition` is registered and serializes. But a serialization failure is invisible from BRP, so use a dedicated `clerestory/restore_state` method as primary and `data.has` for presence. | important |
| B16 | A10's mechanism is right, its narrative wrong — the harm is settle comparing against the wrong monitor's geometry, not a wrong size. The existing test stays green either way. **And three production doc comments justify themselves with the state A12 proved unreachable.** | important |
| B17 | Containment is uncomputable on Wayland, and all of `linux.json` is Wayland. macOS's readback offset must be applied to the rect. | important |
| B18 | The out-of-range-index fixture cannot declare its expected monitor — needs `expected_monitor` on `WindowConfig`. | important |
| B19 | Addition 8 named the wrong duplicated rule (`different_scales`, not Wayland). Three TypedDicts need keys; `expect_position_saved` is a **live basedpyright defect today**. | important |
| B20 | Dead-code list corrected: `default_monitor_scale`, the v1 path, `rebased_physical_position` and `PersistedWithoutCoordinate` all stay. | important |
| B21 | Step 2b test 1 needs the `PreparedWindowPosition` match extracted from `build_persisted`, or it skips the mapping the migration changes. | important |
| B22 | A6's counts were wrong: one production `captured_scale` site, not fourteen. | minor |
| B23 | A7 named one of three doc sites; and `None` now has two meanings, which the doc must say. | minor |
| B24 | D1's detector is exact to within ±`captured_scale`, not exact — immaterial against a monitor rect. The v1 claim is overstated: v1 stored *physical* sizes relabeled as logical, which this does not fix. | minor |
| B25 | The scale-change deadline ties with the runtime restore deadline (both 2.0 s); §2's claim holds unconditionally only for startup. | minor |
| B26 | Addition 6's invariant needs its conditional form — `begin_cross_dpi_restore` has an early return that never arms the timer. | minor |
| B27 | Derive the rounding margin as `ceil(0.5 / scale)`; note the inverse assertion is weaker than the forward one. | minor |
| B28 | Gap 6 is closed *by construction*, not weakly covered. Residual: use `expected_log_warning` to prove the deadline fired. | minor |
| B29 | `monitor_scale: 1.0` is not universal in today's fixtures, and `no_position_same_monitor` exercises `Unpositioned` — not a retained-template candidate. | minor |
| B30 | The `PreStartup` justification's mechanism was wrong (`WinitMonitors` *is* available; `Monitors` is empty because its source entities spawn on `Resumed`). Conclusion unchanged; the mechanism decides where D2's system could go. | minor |
| B31 | Per-frame cost is net **cheaper** — two divides and two rounds removed per projection, one `f64` off a struct cloned on changed-window frames; `PersistedPosition` is ≤24 bytes, no heap. Free cleanup available: `capture()` clones a whole placement just to build an enum for one comparison. | minor |

**Process note:** the style-guide loader was denied approval in all eight agent runs; several agents
read the individual rule files directly instead. Run `/rust_style` before implementing.

---

## 7. Proposed user decisions

### D1 — Policy for a persisted position that lands off every monitor · `DECIDED: (a)` · critical

**Source:** Risk & failure modes; Correctness (both cycles).
**Problem:** A migrated offset can be wrong by more than a window's width, and `should_clamp_position`
is macOS-only — nothing pulls an off-desktop restore back on Windows or Linux. This also gates
additions 1, 2 and step 5: as written, addition 1 asserts containment on every case while addition 2
seeds an off-desktop fixture, and under option (c) both cannot be green.
**Options:**
- **(a) Validate and drop.** The file carries its own detector: `saved_logical × captured_scale`
  reconstructs the save-time *physical* position, to within ±`captured_scale` px. Test that point
  against the monitor's current rect; outside → drop the coordinate, log, let the window center.
- **(b) Clamp cross-platform** on the persisted path, extending today's macOS behavior.
- **(c) Accept as-is**, relying on the deadline and the on-screen assertion.

**Both cycle-2 lenses that examined it recommend (a),** and the argument is specific rather than
aesthetic: the detector is governed by the same `Δ = P_save − P_now` as the error it guards, so
unchanged layout → inside the rect → keep; target-only scale change → the migration is *exact* and
the point is still inside → keep, no false positive; a neighbour's scale change shifting the origin
by Δ → outside once Δ exceeds the window's inset, which is precisely when the offset error exceeds a
window. It also catches the wrong-panel case in B4 that nothing else in the plan detects, and it
removes the addition-1-vs-2 conflict — the seeded fixture's expected outcome becomes "dropped,
centered on target", which *is* containment. Cost: one multiply and two compares per entry, once.
Against (b): clamping slides a bad offset to a corner and shows a placement the user never chose,
with no signal; `should_clamp_position` is macOS-only by design because Windows and Linux windows may
legitimately span monitors, so extending it changes the common same-scale path to patch an uncommon
one, and it does nothing for a wrong-panel anchor. Against (c): the deadline guarantees *visible*,
not on-screen — the observed failure ended revealed at `(-10320, 0)`.
Test the reconstructed *point*, not the rect: a spanning window's top-left is inside the target by
construction. Do not claim (a) fixes v1 files, whose physical sizes were relabeled as logical.

### D2 — Force migration completion for never-opened entries · `proposed` · important

**Source:** Correctness; Risk; Type system; Bevy integration. **The two cycle-2 lenses split.**
**Problem:** A `RememberAll` entry for a window never opened re-serializes as `Unrebased` forever and
re-migrates against the live layout each launch; a mismatched startup restore leaves the entry
`Frozen` and does the same.
**Options:**
- **(a) Rebase on first monitor availability.** Concrete placement, if taken: a
  `rebase_unrebased_entries` system in `PersistencePlugin`, `run_if(resource_changed::<Monitors>)`,
  `.before(restore::prepare_restore_targets)` in `ClerestoryUpdateSet::RestorePreparation` — the same
  cross-module ordering the plugin already does. Cost: one change-tick check per frame, body idle in
  steady state. `resource_changed` beats a one-shot flag because it also repairs entries whose index
  did not resolve at first install. Must inherit the `Requested`-only rule or it launders a fallback
  offset into the authoritative form.
- **(b) Leave it and document.** Keeps the entry in its most interpretable form — legacy coordinate
  *plus* the scale that wrote it — and re-decides against the live layout whenever it is actually
  needed. This is deferred interpretation, not drift: the value is never written back, so every
  launch starts from the same pair. Sound only because v3 round-trips `Unrebased` losslessly, which
  the plan already provides.
- **(c) Drop the coordinate** for entries not restored within a session.

**The split is real and worth your call.** For (a): the repeated migration is wasted work and the
`Frozen` case is a lingering wrong state. Against (a), and this is the sharper argument: **(a)
destroys `captured_scale`** — converting to `MonitorOffset` discards the very payload D1(a)'s
detector needs, for exactly the never-opened entries no restore ever validates. It also fires when
monitors first appear, which on Windows is before the desktop finishes its DPI re-layout — the
condition addition 5(b) documents. So (a) is only safe *after* D1(a) lands, and it makes D3's backup
more necessary, not less, because it marks the file dirty on every first launch of the new build.

### D3 — File-safety scope: atomic write, version probe, backup · `DECIDED: backup + recreate` · important

**Source:** Risk & failure modes; Bevy integration.
**Problem:** `decode` returning `None` seeds an empty state and the first save **replaces** the file,
permanently. `None` covers unsupported version, duplicate key, and parse error alike, and
`save_all_states` uses a bare `fs::write`. B3 makes this sharper than first stated: the destructive
path is reached going *forward* — a version bump landed without its decode arm does exactly this —
not only on a downgrade.
**Options:** **(a)** all three; **(b)** atomic write only; **(c)** out of scope.

Recommended **(a), with the version probe moved to load time.** The probe is what actually prevents
loss, and probing before each overwrite costs a read per save; probe once at load, where the file is
already parsed, and carry a do-not-overwrite flag beside `PersistenceWriteState` — zero extra reads.
Atomic write alone is insufficient here: the empty write *succeeds*. If scope must be cut, cut the
`.v2` backup, not the probe.

### D4 — Collapse `PreparedWindowPosition` and delete `rebased_position()` · `dropped` · important

**Both cycle-2 lenses independently landed on defer, for reasons that leave no open choice.** The
reduction is smaller than cycle 1 stated — `PersistedWithoutCoordinate`, `CompositorControlled` and
`TargetUnavailable` already produce identical output, and they are three distinct reasons for "no
position" worth keeping for logging. The live question is only whether `PersistedOffset` and
`CapturedOffset` merge, and the sole thing distinguishing them is clamping — which D1 decides, so
D4 cannot be settled first. Doing it in step 2 would also land a structural refactor alongside a
format migration and delete step 2b's agreement test before it has ever run against the new
arithmetic. Recorded as a follow-up; nothing behavioral is lost by deferring.

### D5 — Does v3 persist a monitor identity? · `DECIDED: (a), all three platforms` · critical

**Source:** Correctness; Risk (both cycle-2 lenses, independently).
**Problem:** Under v2 the saved `monitor_index` only selects `target_scale` and the fullscreen
target; a wrong index still put the window at the saved *absolute* desktop position. Under v3 the
index **is** the position anchor. `PersistedWindowState` carries no `MonitorIdentity` — `project()`
drops it — so a reconnect or reorder that renumbers displays anchors the offset to the wrong panel,
and `promote` writes that back as a fresh `MonitorOffset`, permanently wrong and indistinguishable
from a correct one. `build_captured` consults verified identity; `build_persisted` cannot.
**Options:**
- **(a) Persist the identity in v3** — carry `MonitorIdentity` through `project()` and match on it
  before falling back to index. Largest change, and the only one that makes a v3 file self-describing.
- **(b) Rely on D1(a)'s detector** plus the `recovery_monitor` fix (already auto-recorded), and
  accept that a same-geometry renumber is undetectable.
- **(c) Accept the exposure** and document that index stability now governs position.

This is new in cycle 2 and is the one place where the fix makes a pre-existing weakness worse rather
than better. It interacts with D1: under (b) the detector is the only guard, which is a further
argument for D1(a).

### D6 — Scope: land the whole of §3, or the minimum that makes this fail loudly? · `DECIDED: (a)` · important

**Source:** Test-harness efficacy; Risk & sequencing (both cycle-2 lenses, independently).
**Problem:** §3 has grown to eleven additions plus five unit tests plus a fixture conversion across
three platforms, on top of format v3 and its migration, with §5 at seven steps.
**Options:**
- **(a) Everything, in §5's order.**
- **(b) Minimum set now, rest as a follow-up.** The named minimum: step 2b tests 1 and 4 (test 1
  *is* the defect; both are display-free and gate everything else); addition 9 relocated to
  `same_monitor_restore_mon1` as the bounds check; addition 1's containment with the Wayland guard
  and the `else: die(...)`. Steps 0–3 are not optional under either option — they are the fix.
- **(c) Some middle set you name.**

What the rest buys, if deferred: **addition 3** turns `HARNESS_ERROR` plus one line of "timeout" into
a diagnosis, and is why the root cause was missed twice — highest value of the second tier;
**addition 2** is the only coverage for files that disagree with the live layout, which is the
real-world case; **addition 5** is the direct regression test and the only one that changes scale
between save and restore, but also the most machinery and Windows-only reach; **additions 6, 8, 10,
11** are cheap and should ride along regardless.

---

## 8. Decisions taken

**D1 → (a) validate and drop.** At restore, reconstruct the save-time physical point as
`saved_logical × captured_scale` and test it against the target monitor's current rect. Inside → keep
and convert to `MonitorOffset`. Outside → drop the coordinate, `warn!`, let the window center. Test
the reconstructed *point*, never the rect.

**D2 → (b) leave it and document** — not separately raised, taken as the default the review argued
for and now reinforced by D1(a): option (a) converts `Unrebased` to `MonitorOffset` eagerly, which
discards the `captured_scale` that D1(a)'s detector needs, for exactly the never-opened entries no
restore ever validates. `Unrebased` round-trips losslessly, so every launch re-decides from the same
pair rather than drifting.

**D3 → backup and recreate**, replacing the do-not-overwrite flag. On any `decode` failure at load
(unsupported version, parse error, duplicate key, truncation): copy the file to
`window_state.ron.bak.<version|corrupt>` without clobbering an existing backup of that name (suffix
`.1`, `.2`, …), `warn!` naming file, backup path, and reason, then start from an empty state and save
normally. Saves are *not* suppressed — the backup already preserves the data, and suppressing them
silently stops remembering positions for the rest of the session. Known gap: a
newer→older→newer build sequence leaves the newer file only as a backup; recoverable by hand.
Independently, `save_all_states` moves to temp-file-plus-rename so a crash cannot manufacture the
corrupt file. The `CURRENT_STATE_VERSION` bump, the v2 decode arm, and the new
`PERSISTED_STATE_VERSION_V2` constant remain one atomic edit.

**D5 → (a) persist a panel identity, on all three platforms.** Optional v3 field carrying a stable
hash of the panel evidence. Resolution order in `build_persisted`: identity match → saved index →
D1(a)'s rect check → drop. Requires a registry accessor mapping a live `MonitorId` back to its
evidence (the registry holds both directions internally but exposes neither).

- **Windows / X11** — hash the EDID bytes already collected by `qualified_evidence`.
- **macOS** — replace the process-local counter in `MacOsDisplayUuid` with the real value:
  `CGDisplayCreateUUIDFromDisplayID(handle.native_id())` from ColorSync
  (`#[link(name = "ColorSync", kind = "framework")]`), read via `CFUUIDGetUUIDBytes` into `[u8; 16]`,
  then `CFRelease`. Null return → `Unverified`. CF types from `objc2-core-foundation`. This also
  fixes a latent same-session weakness: today macOS identity does not survive a replug either.
  **Unverified by any build** — CI is ubuntu-only (`.github/workflows/ci.yml`, all 8 jobs) and this
  session has no Mac. Flagged for the user's Mac retest.
- Virtual/VM displays with synthetic or absent EDID stay `Unverified` and fall through to index plus
  the rect check — the same condition that produced the original no-EDID blocker on the Windows guest.

**D6 → (a) everything, in §5's order.** The deferral case rested on addition 2 having no defined
expected outcome, which D1(a) supplies ("dropped, centered on target"), and it would have deferred
addition 5 — the direct regression test for the defect being fixed. Platform constraint stands: the
full suite runs here on Windows at 175/150/150; the macOS and X11 harness paths are written and
reviewed but not executed.

---

## 9. As-built: what changed against the plan

All steps landed. Gates at completion: 227 unit tests, 26 harness self-tests, full Windows suite
(14/14 restore, 2/2 cross-DPI, 1/1 physical reconnect) green at 175/150/150; `cargo +nightly fmt`
and `clippy -D warnings` clean **with and without** `--features monitor-probe`.

### Three corrections the plan got wrong

**D1(a) tests the window's center, not its corner.** §7-D1 specified "test the reconstructed
*point*, not the rect — a spanning window's top-left is inside the target by construction." The
Linux `monitor_boundary_detection` fixture disproves that: it deliberately places the corner on
monitor 0 with the center just inside monitor 1, and says so in a comment. A corner test would have
discarded a valid position, discovered only on Linux hardware. The center is also how Windows
(`MonitorFromWindow`) and this crate decide which monitor a window belongs to.

**`WindowStateV2` read the wrong wire field — silent data loss.** The v2 decoder was given
`#[serde(rename = "position")]`, copied from v1. v2's field on disk is `logical_position`. Serde
fills the absent `Option` with `None` and ignores the unknown field, so a real v2 file decoded
*successfully* with the position gone: no error, no warning, and no backup, because nothing failed.
Every existing user file would have lost its window position on upgrade. The unit tests missed it
because their fixtures used the same wrong spelling. The retained v2 hardware template caught it.
Both legacy wire structs now carry `#[serde(deny_unknown_fields)]` so a name mismatch in a frozen
format is a parse error — visible, and backed up — rather than a silent empty value.

**The retained v2 templates were not files the app could have written.** They declared
`monitor_scale: 1.0` while sitting on a 150% monitor, so the rebase was validating a fiction and
D1(a) discarded the position. They now declare `${MONITOR_n_SCALE}`.

### Deviations from the plan's design

- `UnrebasedDesktopPosition` lives in `window_state.rs` with **private fields**, not in a private
  child module of `format`. Field privacy is what actually confines construction; the child module
  existed to reach a `pub(in ...)` spelling that `no-pub-in-path.md` forbids. Same guarantee.
- `PreparedWindowPosition::PersistedCoordinate(IVec2)` became
  `PersistedOffset { physical_position, logical_position }` — the payload changed meaning.
- **D3 is backup-and-recreate, not a do-not-overwrite flag.** Suppressing saves would stop
  remembering positions for the rest of the session to protect a file a copy already protects.
- **D2 was never separately raised.** Taken as (b), leave it and document — reinforced by D1(a),
  which needs the `captured_scale` that (a) would discard.

### D5 adds no public API

The fingerprint was first put on `MonitorInfo` as a `pub` field and then moved. `MonitorInfo` is
public with all-`pub` fields, so making just that one field `pub(crate)` would have been the worst
of the three options — a struct with any private field cannot be built by a literal from outside
the crate, which breaks the in-repo `monitor-probe` example and every downstream consumer that
constructs one. That is a harder break than the additive field, not a softer one.

Only two production sites ever read a fingerprint and both are inside `persistence`, so it lives on
`CapturedWindowPlacement` (already `pub(crate)`), resolved from `MonitorIdentityRegistry` at capture
time in `save.rs`. `PanelFingerprint` is `pub(crate)` and does not derive `Reflect`. Net public API
change: none.

The cost is plumbing: `prepare_restore_targets` and both capture systems now take
`Res<MonitorIdentityRegistry>`.

That was briefly `Option<Res<..>>`, which was wrong. `MonitorPlugin` is `pub(crate)` and added
unconditionally beside `PersistencePlugin` and `RestorePlugin` (`lib.rs:253`), so no app can have
persistence without the registry — the absent-resource state cannot occur. The `Option` existed
only because six hand-built *test* worlds omitted the resource, and letting that dictate a
production signature made an impossible state representable and added a branch that can never run.
Those worlds now `init_resource::<MonitorIdentityRegistry>()` with a comment saying why.

### `PanelIdentity`, not `Option<PanelFingerprint>`

```rust
pub(crate) enum PanelIdentity {
    Fingerprinted(PanelFingerprint),
    #[default] Anonymous,
}
```

Not merely for documentation: `Option`'s derived equality says `None == None`, so two displays that
*cannot be identified* compared equal, and a saved position could anchor itself to whichever
anonymous monitor enumerated first. `PanelIdentity::is_same_panel` makes `Anonymous` match nothing,
including itself — failing to identify two panels is not evidence they are the same panel. The rule
lives in the type instead of in whoever remembers to write the comparison carefully. It also reads
better on disk: `monitor_panel: Anonymous` rather than an absent field.

### Unverified

The macOS `ColorSync` path (`CGDisplayCreateUUIDFromDisplayID` replacing the per-process counter in
`MacOsDisplayUuid`) has never been compiled: CI is ubuntu-only and the implementing session had no
Mac. It is the first thing to check on a Mac build. It also fixes a latent bug — the old counter did
not survive a replug even within one session.
