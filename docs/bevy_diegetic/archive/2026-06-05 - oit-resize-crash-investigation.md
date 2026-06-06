# OIT resize kernel-panic investigation (archive)

**Date:** 2026-06-06
**Branch:** `update/0.19.0-rc.2`
**Hardware:** Apple M2 Max, macOS 26.5, 64 GB
**bevy:** 0.19.0-rc.2 · **wgpu:** 29 (Metal)
**Outcome:** Not a reproducible bevy OIT defect. Most likely an externally-induced GPU/driver fault (see [Reframe](#reframe)). The `oit_guard` module is cheap kernel-panic insurance, not a fix for a live bevy bug.

This document is the durable record of the investigation. The instrumentation it describes was reverted after it was written; the [Instrumentation reference](#instrumentation-reference) section is enough to recreate it if the fault ever recurs.

---

## TL;DR

- bevy 0.19.0-rc.2's OIT shaders (`oit_draw.wgsl`, `oit_resolve.wgsl`) index `oit_heads`/`heads` with **no bounds check**, compiled `ShaderRuntimeChecks::unchecked()`. An out-of-bounds index there is an unchecked GPU memory access — on Apple Silicon that has produced AGX `DATA ABORT` kernel panics.
- The OOB only fires if the rasterized fragment position exceeds the `oit_heads` length. The buffer is sized from `ExtractedCamera.physical_target_size`, and so is the render target — so under normal operation they stay in lockstep and the OOB cannot arise.
- We tried to reproduce the panic organically across **8 controlled runs** (window resize, cross-DPI restore, with OIT confirmed live). It never reproduced. On every frame `oit_heads.capacity() == view area`.
- Even the **literal pre-guard source** (raw shaders, no guard module, crates.io bwm `0.21.0-rc.2`) does not crash on resize now.
- **Reframe:** the editor (Zed) had its own resize/screen GPU issue around the same time. The panic was probably an AGX/Metal driver-state fault — plausibly Zed corrupting shared driver state, with the typography resize as the trigger — not the OIT code. This fits all the evidence.

---

## The original problem

The `typography` example uses `StableTransparency`, which routes the 3D camera through bevy's Order-Independent Transparency so coplanar `AlphaMode::Blend` text composites by depth instead of view-dependent draw order. During development the example kernel-panicked the machine "very easily — just start a resize," reportedly several times in a row. A runtime shader guard (`oit_guard.rs`) was added that patches a bounds check into bevy's OIT shaders, and resize was then verified safe.

The goal of this investigation: reproduce the organic panic on demand, understand the exact trigger, and decide whether an upstream bevy issue + the guard fix was warranted.

---

## The bug mechanism (real, but normally unreachable)

Both OIT shaders compute a per-pixel index and use it without a bounds check:

- `oit_draw.wgsl`: `let screen_index = u32(floor(position.x) + floor(position.y) * view.viewport.z);` then `atomicExchange(&oit_heads[screen_index], ...)` — an unguarded **write**.
- `oit_resolve.wgsl`: the same `screen_index` then `heads[screen_index]` — an unguarded **read** and clear.

Shaders compile with `ShaderRuntimeChecks::unchecked()`, so the GPU does not clamp the index. If `screen_index >= arrayLength(&oit_heads)`, the access lands past the buffer.

### Why it is normally unreachable

`prepare_oit_buffers` (bevy_core_pipeline `oit/mod.rs`) sizes `oit_heads` to `max_size.x * max_size.y`, where `max_size` is the max over a camera query filtered `Changed<ExtractedCamera> AND Changed<OrderIndependentTransparencySettings>`. The buffer is *used* by every OIT camera (the unfiltered `camera_oit_uniforms`).

That `Changed` filter looks like it could undersize the buffer (size from only the changed cameras, use from all). It does not, because extraction re-inserts both components **every frame**:

- `extract_cameras` (bevy_render `camera.rs`) does `commands.entity(render_entity).insert((ExtractedCamera {..}, ..))` for every active camera each frame → `Changed<ExtractedCamera>` is always true.
- `OrderIndependentTransparencySettings` uses `ExtractComponentPlugin` → re-inserted each frame → `Changed<...>` is always true.

So the filter always passes, `max_size` is always the true max over all active OIT cameras, and `oit_heads` is sized to fit every view every frame. The render target is also sized from `physical_target_size`, so the rasterized position is bounded by the same value that sizes the buffer. The unguarded index cannot exceed the buffer organically.

This was confirmed empirically: across all runs, `oit_heads.capacity()` equalled the view area on every frame. If the filter had ever excluded the rendering camera, the buffer would have collapsed and OIT would have broken visibly — which never happened.

### The one axis the buffer logic does not cover

`oit_heads` is sized to the CPU **snapshot** (`physical_target_size`). The documented panic mechanism is the snapshot vs the actual **Metal drawable**: during a macOS live-resize the `CAMetalLayer` drawable can momentarily be larger than the snapshot for a frame. If the OIT pass rasterizes into that larger drawable while `oit_heads` is sized for the smaller snapshot, the index runs past the buffer. This is a sub-frame timing divergence between the GPU's real drawable and bevy's extracted size — and it is invisible to CPU instrumentation (every extracted value — `physical_target_size`, the extracted window size, the buffer capacity — tracks the snapshot, not the live drawable).

---

## The investigation — 8 attempts

All runs used the per-frame, fsync'd trace (see [Instrumentation reference](#instrumentation-reference)). "OIT live" means the render-world trace logged OIT frames.

| # | Setup | Result |
|---|-------|--------|
| 1 | `OIT_DISARM` (unguarded OIT), launch restore, no DPI cross | No crash. `heads == area` every frame. Window stayed scale 2; the cross-DPI trigger never fired. |
| 2 | `OIT_CLASSIFY`, launched from the 1× monitor | No magenta, no crash. Frame 0 = scale 1 (pre-placement default), frame 1 already scale 2; the scale flip happened in ~135 ms, **before** OIT activates. |
| 3 | `OIT_CLASSIFY` + bwm restore trace, cross-DPI restore | No crash. Merged timeline: scale 1→2 crossing at ~705 ms; OIT's first frame at ~739 ms — **34 ms after** the crossing. Physical size held constant (`LowerToHigher` keeps physical, halves logical). |
| 4 | `OIT_DISARM` + bwm trace, cross-DPI restore | No crash. OIT's first frame again ~34 ms after the crossing. Disarm removed the guard but did **not** move activation earlier than the pipeline compile (~935 ms), which is gated by the same ~900 ms of startup as the crossing. **Launch-restore path ruled out: OIT is structurally never live during it.** |
| 5 | `OIT_CLASSIFY`, post-startup move + edge-resize, 25 s | 1377 OIT frames, view swept 1279→3130 wide, scale flipped 1↔2. `heads == area` every frame, no magenta. |
| 6 | `OIT_CLASSIFY`, SMAA **off** + cross-monitor + resize | `cam_target == win_phys` all frames, no OOB, no magenta. SMAA ruled out (and the original crashes were SMAA-**on**, the default). |
| 7 | Reverted bwm to crates.io `0.21.0-rc.2`, `OIT_DISARM`, SMAA on, real edge-resize | OIT live (1012→2742), `heads == area` all frames, no crash. **bwm version ruled out.** |
| 8 | **Pre-guard worktree** (`b3948cc`: raw OIT, no guard module, crates.io bwm rc.2), resize | **No crash.** The literal original source + deps does not reproduce. |

### What each ruled out

- **bwm version** (local `0.21.0-dev` vs crates.io `0.21.0-rc.2`): both safe.
- **SMAA on/off**: both safe. With SMAA on, OIT renders to an intermediate target (= snapshot); with SMAA off, to the swap chain. Neither produced a CPU-visible divergence or a crash.
- **Guard on/off** (`OIT_CLASSIFY` vs `OIT_DISARM`): both safe.
- **bevy version**: the crash was on `0.19.0-rc.2`, same as now — not a version difference.
- **The guard module's presence**: the literal pre-guard source (attempt 8) is also safe.

---

## Reframe

The user noted that Zed (the editor) had its own resize/screen GPU issue around the time of the crashes. The prior assumption was that the example's resize caused Zed's trouble. The reverse is at least as likely: **Zed corrupted shared AGX/Metal driver state, and the typography resize merely touched the already-bad GPU**, producing the panic.

This is the only theory consistent with all of it:

- The OIT buffer logic is provably correct (`heads == area` every frame).
- The literal original source does not crash now.
- No variable we changed (bwm, SMAA, guard, version) ever mattered.

If the source that crashed easily then is the source that will not crash now, the cause was not in that source. AGX kernel panics are driver-level and can be induced by one process's GPU misuse, then tripped by another process's ordinary work.

---

## Conclusion

- **No upstream bevy issue is warranted.** A "OIT crashes on resize" report would be correctly declined — the buffer-sizing code keeps `oit_heads >= view`, and the unguarded access cannot be reached organically. The unchecked indexing is a latent footgun, not a live defect on this version.
- **The unchecked indexing is real** and is demonstrable deterministically by `oit_oob_isolate.rs` (`FORCE_SHRINK = true`): it forces `oit_heads` to a quarter of the view, and the resolve classifier paints magenta where the unguarded index would have gone out of bounds, plus shows the 2-line guard removes it. That example is the mechanism proof; it forces a state this bevy version does not produce on its own.
- **`oit_guard` is cheap insurance**, not a fix for a reproducible bug. It converts a potential kernel panic into a dropped fragment if the GPU is ever in a bad state again.

---

## `oit_guard` maintainability (anchor fragility)

The guard injects its bounds check by finding **exact anchor strings** in bevy's OIT WGSL:

- `DRAW_ANCHOR` — `let screen_index = u32(floor(position.x) + floor(position.y) * view.viewport.z);`
- `RESOLVE_ANCHOR` — the resolve-pass equivalent.
- `RESOLVE_WALK` — `while current_node != LINKED_LIST_END_SENTINEL {`

If a future bevy rewrites those shaders, the anchors stop matching. The code already fails safe: `patch_shader` returns `GuardStatus::Failed`, logs `error!("oit_guard: anchor not found ... bevy shader text changed?")`, and the activation gate (`OitGuardState::ready()`) keeps OIT **off** rather than running unpatched. No fault — but OIT silently does not activate, and view-angle-stable transparency is lost, detectable only from the error line in the log.

**Recommended future-proofing if the guard is kept:** a test that reads bevy's embedded OIT shader source and asserts the three anchor strings are present. A bevy bump that rewrites the shaders then fails CI, signalling that the anchors need updating — or that upstream added its own bounds check and the guard can be removed. Without this test, a bevy update quietly disables OIT.

---

## Instrumentation reference (recreate if the fault recurs)

All of the below was reverted after this writeup. Recreate it to investigate a recurrence.

### `oit_trace` (bevy_diegetic, `render/oit_trace.rs`)

- Env-gated by `OIT_TRACE`; zero cost when unset.
- Per-frame line, **fsync after every line** so it survives a kernel panic. Leading `wt=<unix_epoch_ms>` so files merge by timestamp.
- `/private/tmp/oit_crash_trace_main.log` — main world: `scale`, `win_phys`.
- `/private/tmp/oit_crash_trace_render.log` — render world, in `RenderSystems::PrepareResources` after `prepare_oit_buffers` (before the render graph runs the OIT pass): `cam_target` (`ExtractedCamera.physical_target_size`), `heads_cap` (`OitBuffers.heads.capacity()`), `area`, `win_phys` (`ExtractedWindows`), with `WIN_SIZE_CHANGED` / `OOB cap<area` markers.
- Registered in `RenderPlugin::build` (main-world system) and a `RenderPlugin::finish` (render-world system, where `RenderApp` exists).

### `OIT_DISARM` / `OIT_CLASSIFY` (bevy_diegetic, `render/oit_guard.rs`)

- `OIT_DISARM`: `guard_oit_shaders` marks both shaders `Applied` **without** inserting any bounds check, so OIT activates on bevy's unguarded shaders (reproduces the pre-guard state, except the gate/preload machinery is still present). Warns once.
- `OIT_CLASSIFY`: keeps the draw guard, but the resolve pass paints out-of-bounds pixels instead of discarding — **magenta** when the buffer is smaller than the view, **cyan** when the index runs past a buffer that is at least view-sized. Makes a GPU-side OOB visible non-destructively.

### bwm restore trace (`bevy_window_manager`, `src/trace.rs`)

- Env-gated by `OIT_TRACE` (same switch). `/private/tmp/oit_crash_trace_bwm.log`, `wt=<unix_epoch_ms>`, fsync per line.
- Calls in `restore/target_position/application.rs` (`BEGIN_CROSS_DPI` start+target snapshot, `APPLY` strategy, `SCALE_CHANGED` reception, `TRANSITION` state changes, `COMPLETE` applied geometry) and `restore/settle_state.rs` (per-settle-frame actual-vs-target size/scale/monitor).
- Requires pointing the workspace dependency at the local path: `bevy_window_manager = { path = "/Users/natemccoy/rust/bevy_window_manager_bevy_update" }`. crates.io has no instrumentation. Revert to `"0.21.0-rc.2"` after.

### Pre-guard worktree

`git worktree add /Users/natemccoy/rust/bevy_hana_preguard b3948cc` — the commit before the guard (`00c7095`): raw OIT, no guard module, crates.io bwm rc.2. Build with `cargo build -p bevy_diegetic --example typography --features typography_overlay` (needs unsandboxed because the rustc wrapper is sccache).

### Merging the timeline

`cat /private/tmp/oit_crash_trace_*.log | sort -t= -k2 -n` — all three files share the `wt=` epoch prefix.

---

## If it recurs

1. Recreate the instrumentation above.
2. Run `OIT_TRACE=1` (add `OIT_DISARM=1` only to confirm a fault; it is destructive).
3. Resize / restore as when it faulted, then merge the three logs.
4. If the trace shows `oit_heads == view area` right up to the panic (as it did here), the divergence is the GPU drawable vs the snapshot — confirm by also capturing the actual swap-chain texture size after `prepare_windows`, the one size not in any extracted value.
5. Check whether another GPU-heavy process (editor, compositor) was misbehaving at the same time — the leading theory is an external driver-state fault, not the OIT code.
