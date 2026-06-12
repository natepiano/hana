# GPU Perf Test Plan

**Goal:** reduce GPU-side frame cost in diegetic text scenes. The CPU side
is done — batched glyph records + vertex pulling
([`../as-built/glyph-instancing.md`](../as-built/glyph-instancing.md)) cut the
stress-scene frame from 20.2 ms to 7.3 ms and the render thread no longer
paces the frame. The largest remaining single render component is
`gpu wait`.

## Current state (2026-06-06)

`diegetic_text_stress`, release, 3440×2104 (the After-4b waterfall column
in the archived instancing plan):

- frame 7.3 ms / 134 fps moving; 7.5 ms / 128 fps paused
- `render` 6.9 ms: `gpu wait` 3.41, `graph` 1.97, `prep` 1.35, `assets` 0.12
- 3 batches / 183 runs / 943 glyphs; transparent-pass items 125
  (~122 SDF panel backings + 3 text batches), shadow 8
- idle floor clean: `layout`/`reconcile`/`shaping`/`mesh` ≤ 0.01 ms,
  zero steady-state uploads

## Why the May numbers don't transfer

The May campaign ([`slug-experiments.md`](slug-experiments.md),
[`slug-benchmark-procedure.md`](slug-benchmark-procedure.md)) measured a
renderer that has since changed four ways:

1. **Batched records + vertex pulling** replaced the per-run mesh path.
   The fragment shader is the same file (`text/slug/shaders/slug_text.wgsl`);
   the vertex stage, draw structure, and buffer model are not.
2. **`AntiAlias::Both` became the default fragment path**
   (`render/mod.rs`). Per edge fragment at grazing angles:
   3 setup `signed_distance` evaluations + up to 16 stride samples
   (`MAX_ANISO_SAMPLES`), each a full double band loop. Every May
   experiment measured one `distance_coverage` call per fragment.
3. **`c3cfcbd` reverted both accepted wins for quality** — per-curve dedup
   (−16.7 % fragment at bench scale) removed for grazing-edge aliasing on
   `L`/`r`, and 48 bands (−25 % prep) raised back to 96 for small-size
   sharpness — and **added** `any_outside_neighbor`
   (`slug_text.wgsl`): 4 extra full winding band loops for
   inside-near-edge fragments, fixing the EB Garamond `g` neck crack.
4. **bevy 0.18 → 0.19.0-rc.2** (wgpu 29); OIT pool average raised to
   8 fragments/pixel.

## Guardrails (May lessons, still binding)

Architecture-independent results from `slug-experiments.md`; no re-test
without new evidence:

- **Per-curve gating inside the curve loop loses.** EDGE_FILTER 1.2→1.0
  and the chord gate both regressed: a 32-lane SIMDgroup pays the slow
  path if any lane takes it. Only wave-coherent skips can win.
- **Metal trig is effectively free.** Shared-sincos and Newton-deflation
  both lost; per-cubic ALU cost is not the target. This also pre-kills
  Proposal C (F16 intermediates).
- **Reduce work quantity, not per-op cost.** The only accepted GPU win
  (per-curve dedup) cut cubic *count*; every per-op attempt failed.
- **Band density/overlap is conserved.** Total per-fragment curve work is
  invariant across band-count/overlap trades; that axis is closed.
  Current 96 bands is a quality choice, not a perf knob.
- **Protocol:** visual gate before any trace; median of 5 back-to-back
  traces; ±0.05 ms signal threshold on the canonical bench.

## Measurement tiers

Two instruments with distinct roles:

- **Decision signal — `diegetic_text_stress` + the `DiegeticPerfStats`
  overlay.** The target scene, read live in the UI: flip variants over
  BRP and watch the waterfall. Used for Phase 0c, Phase 1, and choosing
  which Phase 2 experiments to run. Its limits: frame-level numbers only,
  `gpu wait` conflates swapchain-acquire blocking with GPU work (the
  archived instancing plan's footnote ² documented that trap), the scene
  mutates frame to frame (run/glyph counts vary), and there is no no-text
  floor — it cannot resolve sub-0.3 ms shader deltas.
- **Verdict instrument — `text_renderer_gpu_bench` + xctrace.** Fixed
  window pinned to the built-in display, warmup/sample harness,
  `--mode empty` no-text floor, per-pass vertex/fragment attribution,
  median of 5 traces, ±0.05 ms signal threshold. Every Phase 2
  experiment's verdict comes from here and lands as a column in
  `slug-benchmark-procedure.md`, followed by a confirmation spot-check on
  the stress overlay.

## Phase 0 — re-establish measurement

Nothing measured in May is comparable to anything measured now; the
campaign starts by rebuilding the baseline.

- **0a. Tooling check.** Run one trace of
  `text_renderer_gpu_bench --mode slug` under bevy 0.19 and confirm
  `parse_gpu_intervals.py` still matches intervals (its
  `main_transparent_pass_3d` label filter may have changed across bevy
  releases). Fix the filter if not. Confirm the bench window landed on
  the built-in MacBook display: drawable must be `3200x1800` (scale 2.0),
  not `1600x900`. The bench pins `MonitorSelection::Primary`, which on
  macOS is the main display — the built-in on this machine whether or not
  the external is connected. Scale 2.0 costs more fragments than the
  external; that is the intended measurement surface.
- **0b. Fresh canonical baseline column.** Populate a new column in
  `slug-benchmark-procedure.md` per its procedure, declaring the
  `AntiAlias` mode and the warmup protocol actually run (keep the
  exported trace bundles as evidence).
- **0c. Real-scene GPU decomposition.** xctrace on `diegetic_text_stress`
  (release, perf mode, full window): export GPU intervals for *all* pass
  labels, not just the transparent pass, and rank GPU time per pass —
  text fragments vs OIT resolve vs panel SDF vs shadows vs the rest.
  This decides whether slug fragment work even dominates the 3.41 ms
  `gpu wait`, before any shader work is scoped.
- **0c result (2026-06-07).** 10 s Metal System Trace on a fresh release
  launch, built-in display (3456×2104), `AntiAlias::Both`,
  1415 GPU frames (~141 fps). Per-frame GPU time by pass
  (sum across Vertex/Fragment/Compute channels; channels overlap, so
  the 7.49 ms grand total exceeds the ~7.1 ms wall frame):

  | per-frame ms | share | pass |
  | ---: | ---: | --- |
  | 2.68 | 35.8 % | `main_transparent_pass_3d` (text glyphs + panel SDF backings + OIT writes; Fragment 2.59) |
  | 2.69 | 35.9 % | shadow cascades 0–3 combined (1.35 / 0.66 / 0.40 / 0.28; Fragment 2.50 of it) |
  | 0.86 | 11.5 % | light clustering passes combined |
  | 0.31 | 4.1 % | upscaling |
  | 0.29 | 3.8 % | `oit_resolve` |
  | 0.10 | 1.4 % | `main_opaque_pass_3d` (vertex + clear only) |
  | ~0.55 | ~7 % | wgpu-internal compute (draw validation, blits, mesh preprocessing) |

  Findings: **OIT resolve is exonerated** (0.29 ms). The frame splits
  almost exactly in half between the transparent pass and the shadow
  cascades — shadows were not on the suspect list at this weight. The
  shadow cost is fragment-heavy (2.50 of 2.69 ms), which a depth-only
  pass should not be; that pattern points at alpha-evaluated materials
  (text and/or panels) running real fragment work into 4 cascade maps.
  Within the transparent pass, text vs panel-backing split is not
  visible at pass granularity — the Phase 1 AA delta (~0.4–0.6 ms)
  is a lower bound on the text share. Next probes, both cheap and live
  over BRP: (A) toggle shadow casting off for text labels / panels and
  watch the overlay — splits the 2.69 ms; (B) hide panels vs text —
  splits the 2.68 ms. These pick the Phase 2 entry: 2a only pays if
  text fragments dominate after (A)/(B); the shadow-cascade lever
  (caster opt-out, cascade count/resolution) is a new candidate that
  no shader experiment touches. Tooling: per-pass ranking script at
  `scripts/rank_gpu_passes.py` (id/ref resolution per
  `parse_gpu_intervals.py`); pass labels under bevy 0.19 still match
  the parser's `main_transparent_pass_3d` filter, so Phase 0a's label
  check is satisfied.

## Phase 1 — AA-mode A/B in the real scene

The supersample multiplier is the one fragment cost no May experiment
ever saw. Measure it before optimizing around it.

- **Setup (DONE 2026-06-07):** the `A` key cycles `AntiAlias`
  (`Off → Anisotropic → Supersample → Both`) in `diegetic_text_stress`;
  the title bar shows all four mode names with the active one
  highlighted, next to the `Space Pause` indicator (highlighted while
  paused). Built on fairy_dust segmented title-bar controls
  (`TitleBarControl::segmented`). BRP drives the same key via
  `send_keys`, so the loop works manually and scripted.
- **Method:** `diegetic_text_stress` release at full window on the
  built-in display, perf mode; cycle the modes with `A` and record
  frame / `gpu wait` per mode from the overlay. Repeat on the canonical
  bench for the table.
- **Decision rule:**
  - `Both → Anisotropic` recovers a large share of `gpu wait` → the
    supersample loop is the target: Phase 2b/2c first.
  - delta is modest → per-fragment base cost dominates: Phase 2a first.
  - 0c shows text fragments are a minority of GPU time → the lever is
    outside text (OIT pool, panel SDF, shadows); scope a separate plan
    and stop here.
- **First signal (2026-06-07, ad-hoc):** on the running release
  instance (built-in display, 3440×2104, unfocused, mid-run after time
  on the external): `Both` 96–103 fps (~10.05 ms), `Off` 100–107 fps
  (~9.66 ms) → **AA full-off recovers ~0.4–0.6 ms**. The supersample
  multiplier is not the dominant GPU cost in this scene — consistent
  with the stride collapsing to 1 sample on the mostly-frontal stress
  grid. Phase 2b/2c deprioritized; Phase 0c (per-pass decomposition)
  is the next gate, then 2a. Caveat: decision-signal quality only
  (mid-run, unfocused, `gpu wait` includes acquire-blocking); re-measure
  fresh for the table, and spot-check a grazing-heavy scene (typography)
  where the stride actually fires.
- **Same session, display finding:** the instance had restored onto the
  scale-1.0 external (drawable 1720×1378) and ran at 52–85 fps with
  `gpu wait` 14.16 ms — moving the window to the built-in (drawable
  3440×2104) took it to ~105 fps / `gpu wait` 5.53 ms with 3× more
  pixels. Presenting to the external blocks the render thread in
  swapchain acquire; rendering work was never the difference. This is
  the invalid case the procedure doc's built-in-display rule exists to
  catch.

## Phase 2 — shader experiments

Each experiment: hypothesis → visual gate → 5-trace median on the
canonical bench → spot-check on `diegetic_text_stress` → verdict recorded
in `gpu-perf-experiments.md`. The visual gate must include the two cases
that caused the `c3cfcbd` reverts — grazing-edge `L`/`r` aliasing and the
EB Garamond `g` neck — in addition to the saved g-zoom view; the old
AE-0 g-zoom gate alone did not catch either regression.

- **2a. Re-land per-curve dedup, boundary-safe.** The proven −16.7 %
  fragment win is currently switched off. Variant: dedup only curves
  unambiguously interior to one band's coverage; keep curves near band
  boundaries (within `edge_width` of the boundary at expected scales) in
  both bands. The original proposal's risk note predicted exactly this
  conservative rule.
- **2b. Supersample-aware early-out.** `aniso_band_coverage` already
  computes `sd_center`; `signed_distance` is 1-Lipschitz, so when
  `|sd_center|` exceeds the stride extent plus the band width, every
  stride sample returns saturated coverage and the loop can be skipped.
  Interior fragments at grazing currently pay N× double band loops for a
  constant answer. The skip is spatially coherent across a wavefront —
  the pattern the divergence guardrail says can win.
- **2c. Cheapen `any_outside_neighbor`.** 4 full winding band loops per
  inside-near-edge fragment, re-run inside every supersample evaluation.
  Candidates: evaluate once per fragment at the center and reuse across
  stride samples; or replace the 4-point probe with a quantity already
  computed (e.g. derive the outer-silhouette test from the distance pass).

## Phase 3 — interior/edge split on batched records

The structural lever: the shaded-to-ink waste measurement (2.78×, see
`slug-experiments.md`) means most shaded fragments are far-interior. All
three May split attempts (cached distance-cell, merged single-mesh,
single-span classifier) were rejected on entity/material/draw/vertex
overhead — an objection batching removed: interior fill can be emitted as
solid-mode records in the same batch, zero new entities or draw calls.

Carried-over risks: design-unit edge-band geometry degrades AA at high
zoom (the ribbon trail); punch-out, clip, shadow, and panel paths must
survive. Scope only after Phases 0–2 land, with its own plan doc.

## Recording rules

- Performance columns → `slug-benchmark-procedure.md` (one column per
  configuration; AA mode and protocol declared).
- Experiment entries → `gpu-perf-experiments.md` (new doc, created with
  the first entry; `slug-experiments.md` is closed to new entries).
- Visual gates first, always; screenshots and trace bundles under
  `/private/tmp` (survives `cargo clean`).
