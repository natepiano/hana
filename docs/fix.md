# Bevy 0.19 cross-DPI restore — fix plan

Branch: `update/bevy_0.19.0` (migration to `0.19.0-rc.2`, committed as `9191575`).
Full investigation trail: `~/.claude/projects/-Users-natemccoy-rust-bevy-window-manager/memory/attempts_bevy019_hidpi_size.md`.

## What we found

Multi-monitor testing on macOS (monitor 0 = retina, scale 2; monitor 1 = external, scale 1) surfaced a regression: on a cross-DPI restore, the primary window comes back at **half size** (e.g. 800×600 px instead of 1600×1200 px). Confirmed visually (screenshot framebuffer + NSWindow points), and confirmed as a regression by running the same test on `main` (0.18.1 passes, 0.19.0-rc.2 fails).

Root cause (verified by direct experiment): on a low→high cross-scale move, **bevy 0.19 changed the two winit calls asymmetrically**:

| winit call | 0.19 behavior on cross-scale move | compensation needed? |
|---|---|---|
| `request_inner_size` (size) | resolves at the **target** monitor's scale | **No** — apply full size; compensating halves it (this was the bug) |
| `set_outer_position` (position) | still resolves at the **starting** scale (doubles) | **Yes** — keep `×ratio` |

The `LowerToHigher` strategy was compensating *both* position and size (×ratio). On 0.18 the cross-scale move doubled the compensated size back up; on 0.19 it no longer does, so the size stayed halved. `HigherToLower` / `CompensateSizeOnly` were unaffected because their `ApplySize` phase already applies the full (uncompensated) size after the scale change.

Note: the launch monitor is **environmental** (depends on the active screen at launch), so `same_monitor_restore_mon0` runs `LowerToHigher` only when the window happens to spawn on monitor 1.

## The fix (implemented, verified — uncommitted)

`src/restore/target_position/application.rs`, `MonitorScaleStrategy::LowerToHigher` arm in `try_apply_restore`: keep `compensated_position()`, but apply the full `physical_size` instead of `compensated_size()` (one line + explanatory comment).

Verified: a clean run that actually exercised `LowerToHigher` (launched mon1 → restored mon0) passes every check — position `200,200`, size `1600×1200`, plus the relaunch cycle.

## Current working-tree state

- `src/restore/target_position/application.rs` — the one-line fix + comment (uncommitted).
- `examples/restore_window/main.rs` — reverted (the `borderless_game` experiment was a dead end; no change).
- Migration (Cargo.toml/Cargo.lock + 6 `font_size` wraps) already committed as `9191575`.
- `clippy` / `fmt` not yet run on the fix.

## Plan

### Step 1 — Run the full test suite again (before committing)
Re-run the macOS integration suite (`/test`, i.e. `tests/scripts/run_test.py`, config `tests/config/macos.json`, 13 tests) with the fix in place.
- Run tests **one at a time** — rapid back-to-back relaunches intermittently trigger the known macOS shutdown-hang and cause `Neither WindowRestored… within timeout` errors. Single runs are reliable.
- The launch monitor is environmental, so re-run a test until it exercises the intended strategy when needed (use `RUST_LOG="warn,bevy_window_manager=debug"` and grep `monitor_scale_strategy=`).
- Goal: confirm `LowerToHigher` passes consistently, and that `HigherToLower` / `CompensateSizeOnly` / `ApplyUnchanged` and the multi-window + fullscreen + persistence tests still pass on 0.19.
- If any other strategy regresses, fold it in before committing.

### Step 2 — Commit the one-line fix
Once the suite is green, commit only the `application.rs` change (the verified regression fix) on `update/bevy_0.19.0`.
- Suggested message: `fix: apply full physical_size for LowerToHigher cross-DPI restore (bevy 0.19)`.
- Body: bevy 0.19 resolves `request_inner_size` at the target monitor's scale, so the size must not be ratio-compensated; position compensation is retained because `set_outer_position` still resolves at the starting scale.
- Run `clippy` + `fmt` before committing (project denies `all`/`cargo`/`nursery`/`pedantic`).

### Step 3 — Investigate the removal hypothesis (the real cleanup)
With the regression fixed and committed, test whether the compensation machinery can be reduced now that 0.19 resolves size at the target scale:
- `compensated_size()` now has a single remaining caller — `CompensateSizeOnly`'s initial move (`application.rs:66`). That call is likely redundant on 0.19 (the `ApplySize` phase overwrites it with the full size). If validated, delete `compensated_size()`.
- Check whether the `WaitingForScaleChange → ApplySize` machinery is still needed for size on 0.19, or whether the full size can be applied directly (as `LowerToHigher` now does) — potentially collapsing the cross-DPI strategies.
- Position compensation (`compensated_position()` / `ratio()`) stays — `set_outer_position` is still scale-sensitive (winit #4440, unchanged: same winit 0.30.13 in both versions).
- Validate every change against the full suite, including the high→low direction (hard to trigger because launch monitor is environmental — may need a deterministic way to force the starting monitor).

## Key references
- Fix site: `src/restore/target_position/application.rs` (`try_apply_restore`, `LowerToHigher` arm).
- Compensation helpers: `src/restore/target_position/target.rs` (`ratio`, `compensated_position`, `compensated_size`).
- Strategy enum: `src/restore/target_position/strategy.rs` (`MonitorScaleStrategy`, `WindowRestoreState`).
- 0.18 baseline worktree: `/Users/natemccoy/rust/bevy_window_manager` (branch `main`, bevy 0.18.1).
- winit #4440 workaround context: `Cargo.toml` `workaround-winit-4440` feature comment.
