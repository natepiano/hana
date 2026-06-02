# Diegetic text performance — options

## Baseline

Conditions: `diegetic_text_stress`, 100 world labels each restrung every frame,
M2 Max, release, `with_perf_mode` (AutoNoVsync + `WinitSettings::continuous`).
Add a column after each phase lands.

| Metric (moving unless noted)        | Baseline (2026-06-02) | After A | After D | After C | After B |
| ----------------------------------- | --------------------- | ------- | ------- | ------- | ------- |
| Frame time                          | ~25 ms                |         |         |         |         |
| FPS                                 | 40                    |         |         |         |         |
| Layout `compute_ms` (alt. frames)   | 0 / 5.8 ms            |         |         |         |         |
| Text `panel_text.total_ms`          | 2.4 ms                |         |         |         |         |
| — of which `mesh_build_ms`          | 1.8 ms                |         |         |         |         |
| Diegetic CPU subtotal               | ~5–8 ms               |         |         |         |         |
| GPU asset churn (derived remainder) | ~7–10 ms              |         |         |         |         |
| Paused frame time                   | ~10 ms                |         |         |         |         |
| Paused FPS                          | 98                    |         |         |         |         |

The derived remainder = moving frame time − measured diegetic CPU; it's the
per-frame GPU asset churn in the panel-text mesh build (`meshes.add()` +
3× `storage_buffers.add()` for every label every frame). The ~10 ms paused floor
is the static render of 600 glyph quads under OIT + shadows + 3-light studio
lighting.

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
