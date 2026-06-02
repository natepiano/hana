# Diegetic text performance — options

## Baseline

Conditions: `diegetic_text_stress`, 100 world labels each restrung every frame,
M2 Max, release, `with_perf_mode` (AutoNoVsync + `WinitSettings::continuous`).
Add a column after each phase lands.

| Metric (moving unless noted)       | Baseline (2026-06-02) | After A     | After D | After C | After B |
| ---------------------------------- | --------------------- | ----------- | ------- | ------- | ------- |
| Frame time ‡                       | ~25 ms                | ~24 ms      |         |         |         |
| FPS ‡                              | 40                    | 42          |         |         |         |
| Layout `compute_ms` (alt. frames)  | 0 / 5.8 ms            | 0 / 5.8 ms  |         |         |         |
| Text `panel_text.total_ms`         | 2.4 ms                | 1.6 ms      |         |         |         |
| — of which `mesh_build_ms`         | 1.8 ms                | **1.29 ms** |         |         |         |
| Diegetic CPU subtotal              | ~5–8 ms               | ~1.6 / 7 ms |         |         |         |
| Render floor (paused, diegetic ≈0) | ~10 ms ‡              | ~18 ms ‡    |         |         |         |
| Paused FPS ‡                       | 98                    | ~55         |         |         |         |

‡ The frame-time, FPS, and paused rows are fill-rate-bound and scale with window
size. `with_save_window_position` restored a larger window this session, so those
absolute numbers are **not** comparable across the Baseline and After-A columns —
the window grew, not the cost. The window-independent A/B is the CPU rows, where
`mesh_build_ms` (1.8 → 1.29 ms) is the clean measure of A's effect.

## Finding — A is CPU-only; this stress test is render-bound

A landed correctly and all 255 tests pass: runs now overwrite their mesh and
three GPU buffers in place behind stable handles keyed by the label entity
(`RunStorageKey::from(entity)`), and the mesh child + material persist instead of
being despawned and re-added every frame. The measured effect is
`mesh_build_ms` 1.8 → 1.29 ms — the per-frame `meshes.add()` + 3×
`storage_buffers.add()` + mesh-entity respawn + `materials.add()` are gone.

But the moving frame time barely moved, and that corrects the earlier model. The
paused frame — text not changing, diegetic CPU ≈ 0 — is still ~18 ms, so that
entire floor is GPU render: OIT transparency + shadows + 3-light studio lighting
over ~600 glyph quads, which A does not touch. Diegetic CPU is only ~1.6–7 ms of
the frame (alternating with the layout toggle), **not** the ~7–10 ms the prior
note attributed to "asset churn." The per-frame text mutation costs ~6 ms
(moving − paused); A optimizes a ~0.5–1 ms slice of it, and the ~18 ms render
floor is the dominant, untouched cost.

Implication for ordering: to move *this* test's frame time, attack the render
(transparency / OIT / shadow / fill-rate) and per-frame upload volume
(instancing — **B**), not CPU churn. **D** still removes the alternating ~5.8 ms
layout CPU. A remains the right foundation — stable per-label identity is what a
later content-hash skip would build on — it just isn't where this test's wall
clock goes.

## Options

**A. Reuse the geometry (in-place mesh/buffer update)**
- Keep each label's existing mesh + buffers and overwrite their contents when the
  text changes, instead of allocating a new asset and dropping the old one.
- Removes the ~400-per-frame allocate/upload/discard churn — the biggest share of
  the moving cost.
- Small change; stays inside the current mesh pipeline.
- **First step.**

**B. True instancing (one shared quad)**
- Stop storing a quad per glyph. Keep one unit quad and draw it N times, feeding a
  per-glyph table (position + glyph id) the GPU expands.
- No per-label meshes; moving text just updates a small table. Also removes the
  duplicate quads.
- Big change — needs a custom instanced render pipeline. The eventual destination.

**C. Share glyph outlines across labels**
- The `curves`/`bands` for `0`–`9` are identical in all 100 labels but each label
  uploads its own copy today. Store each glyph's outline once; labels point at the
  shared copy.
- Cuts duplicate outline memory + upload.
- Medium change.

**D. Skip layout on text-only changes** (separate from the quad work)
- A fixed-width single-line label that only changes its string re-runs full panel
  layout every other frame (the `ReconcileOwned` marker ping-pong). It should not
  re-layout at all after the first pass.
- Saves ~3 ms CPU.
- Small, isolated.

## Suggested order

A → D → C → B.

Detail on the toggle and the mesh/buffer build lives in the memory notes
`project_diegetic_text_perf_targets` and `project_perf_mode_measurement`.
