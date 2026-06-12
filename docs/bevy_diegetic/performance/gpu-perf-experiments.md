# GPU Perf Experiments

Data log for the GPU perf campaign defined in
[`gpu-perf-test-plan.md`](gpu-perf-test-plan.md). One entry per
measurement or experiment, newest last. Canonical bench columns go to
[`slug-benchmark-procedure.md`](slug-benchmark-procedure.md); this doc
holds everything else — decision-signal readings, decompositions, and
Phase 2 experiment verdicts. The May-era log
([`slug-experiments.md`](slug-experiments.md)) is closed; its numbers
are not comparable to anything recorded here.

Every entry declares: scene, build profile, display (drawable size),
`AntiAlias` mode, and instrument (stress overlay = decision signal,
bench + xctrace = verdict).

---

## 2026-06-07 — Phase 1: AA-mode A/B (decision signal)

- **Scene/instrument:** `diegetic_text_stress`, release, stress overlay.
- **Display:** built-in, drawable 3440×2104. Caveats: mid-run process
  (hours old, had spent time on the external display), window unfocused,
  `gpu wait` includes swapchain-acquire blocking.
- **Method:** ad-hoc manual cycle via the new `A`-key control
  (title-bar segmented indicator confirms the active mode).
- **Result:** `Both` 96–103 fps (~10.05 ms); `Off` 100–107 fps
  (~9.66 ms). AA full-off recovers **~0.4–0.6 ms**.
- **Verdict:** the supersample multiplier is not the dominant GPU cost
  in this scene — consistent with the anisotropic stride collapsing to
  1 sample on the mostly-frontal stress grid. Phase 2b/2c deprioritized.
  Re-measure fresh for any table use; spot-check a grazing-heavy scene
  (typography) where the stride actually fires. The ~0.4–0.6 ms is also
  a **lower bound on the text share** of the transparent pass.

## 2026-06-07 — Phase 0c: per-pass GPU decomposition (decision signal)

- **Scene/instrument:** `diegetic_text_stress`, release, fresh launch,
  10 s Metal System Trace attached after ~25 s warmup.
- **Display:** built-in, drawable 3456×2104, `AntiAlias::Both`,
  window unfocused, AutoNoVsync. 1415 GPU frames captured (~141 fps).
- **Method:** `xctrace record --template 'Metal System Trace'
  --time-limit 10s --attach <pid>`; export `metal-gpu-intervals`;
  aggregate with `scripts/rank_gpu_passes.py` (id/ref resolution
  per `parse_gpu_intervals.py`; groups by pass label, sums all
  channels, divides by distinct GPU frame count). Trace bundles are
  deleted after parsing — a 30 s capture produced an 11 GB bundle whose
  post-processing ran past 16 min and was abandoned; 10 s → 4.3 GB and
  a few minutes. Keep capture windows ≤ 10 s.
- **Result** (per-frame GPU ms; channels overlap, so the 7.49 ms total
  exceeds the ~7.1 ms wall frame):

  | per-frame ms | share | pass |
  | ---: | ---: | --- |
  | 2.68 | 35.8 % | `main_transparent_pass_3d` (text + panel SDF backings + OIT writes; Fragment 2.59) |
  | 2.69 | 35.9 % | shadow cascades 0–3 (1.35 / 0.66 / 0.40 / 0.28; Fragment 2.50) |
  | 0.86 | 11.5 % | light clustering passes |
  | 0.31 | 4.1 % | upscaling |
  | 0.29 | 3.8 % | `oit_resolve` |
  | 0.10 | 1.4 % | `main_opaque_pass_3d` (vertex + clear only) |
  | ~0.55 | ~7 % | wgpu-internal compute (draw validation, blits, mesh preprocessing) |

- **Findings:**
  - **OIT resolve exonerated** — 0.29 ms.
  - The frame splits in half between the transparent pass and the
    shadow cascades. Shadow cost is fragment-heavy (2.50 of 2.69 ms),
    which depth-only shadow rendering should not be — points at
    alpha-evaluated materials (text and/or panels) shading into all
    four cascade maps.
  - Text shader experiments (Phase 2) can win at most ~2.6 ms, part of
    which is panel backings, not text.
  - Pass labels under bevy 0.19 still match the parser's
    `main_transparent_pass_3d` filter — Phase 0a's label check passes.
- **Verdict:** still active — decomposition done, attribution within
  the two big passes pending. Next probes (live over BRP, overlay as
  readout): (A) toggle shadow casting off for text labels / panels to
  split the 2.69 ms; (B) hide panels vs text to split the 2.68 ms.
  These pick the Phase 2 entry; the shadow-cascade lever (caster
  opt-out, cascade count/resolution) is a new non-shader candidate.

## 2026-06-07 — Probe A: shadow-cost split by caster (decision signal)

- **Scene/instrument:** `diegetic_text_stress`, release, fresh launch,
  `brp_extras/get_diagnostics` `frame_time_ms.average` (~1 s window),
  median of 3 clean samples per condition (hitch samples discarded).
- **Display:** built-in, drawable 3456×2104, `AntiAlias::Both`,
  unfocused, AutoNoVsync.
- **Caster census (via BRP):** 126 `Mesh3d` entities = 121 panel
  backings (**already `NotShadowCaster`**) + 3 `DiegeticTextBatch`
  (2 casting, 1 already opted out — batches split by shadow mode) +
  2 plain scene meshes (casting).
- **Method:** insert `bevy_light::NotShadowCaster` (`{}` payload) over
  BRP, cumulatively; flip `DirectionalLight.shadow_maps_enabled` last.
- **Result ladder** (frame_time_ms.average, this instance):

  | condition | ms | delta vs prev |
  | --- | ---: | ---: |
  | baseline (shadows on) | ~6.9 | — |
  | + text batches opted out | ~5.96 | **−0.95 text shadow rendering** |
  | + plain meshes opted out (all casters off) | ~5.70 | −0.26 scene geometry |
  | shadow maps disabled entirely | ~5.14 | −0.56 empty-pass overhead + cascade sampling in main-pass shading |

- **Findings:**
  - **Text shadow rendering is the dominant shadow cost: ~0.95 ms**
    (~54 % of the ~1.75 ms wall-clock shadow lever) — the slug
    fragment shader runs alpha-evaluated into 4 cascade maps for just
    2 batches. Panels contribute zero (already opted out).
  - Plain scene geometry (depth-only) is cheap: ~0.26 ms.
  - ~0.56 ms remains even with zero casters — running 4 empty cascade
    passes plus per-fragment cascade sampling in the main passes.
  - **bevy 0.19.0-rc.2 defect found:** flipping
    `DirectionalLight.shadow_maps_enabled` false→true at runtime
    panics in `bevy_pbr::render::light::specialize_shadows`
    (light.rs:2852, "Failed to get directional light visible entities
    for cascade"). Off is a one-way door per process; order probes
    accordingly.
  - Caveat: entity ids are NOT stable across relaunches — re-query
    every instance (a probe in this session silently tagged two
    lights instead of the meshes; readings flagged it).
- **Verdict:** the shadow lever is real and text-specific. Candidate
  fixes, in rising invasiveness: per-label shadow opt-out (API already
  exists — one batch ships opted out), cascade count/resolution
  reduction, or a cheap depth-only/alpha-cutoff shadow path for text
  instead of the full analytic coverage shader. Visual gate needed
  before any of these: what do text shadows contribute to the look?
  Combined with Phase 1: AA (~0.4–0.6) + text shadows (~0.95) ≈
  1.4–1.5 ms of the frame is text fragment work outside the main
  transparent-pass coverage evaluation.

## 2026-06-07 — winding-only shadow cutout (implementation)

- **What:** the shadow fragment (`slug_text.wgsl`, `PREPASS_PIPELINE`)
  now answers inside/outside with one `winding_at` test per fragment
  (punch-out runs invert it), replacing the full `render_coverage`
  evaluation binarized at 0.5. Unconditional — no per-label choice. A
  per-label `GlyphShadowQuality` (`Expensive`/`Cheap` through
  `TextStyle` + `RunRecord` + a per-run shader branch) was built first
  for the A/B, then deleted once the user picked cheap-always: the
  choice bought only code complexity. Net engine diff: the prepass
  fragment body plus the mirror-test hash.
- **Why it is silhouette-identical:** the shadow map is binary
  (discard < 0.5); the full path's AA machinery only moves the
  boundary by less than half a ramp width before binarization —
  sub-texel in the cascade map.
- **Expected cost:** replaces 3 + N `signed_distance` evaluations per
  shadow fragment (each = horizontal band winding + distance solve +
  vertical band solve + up to 4 `any_outside_neighbor` winding loops)
  with one horizontal-band winding loop.
- **Visual gate (2026-06-07): passed.** User A/B'd both in the
  `typography` example and saw no meaningful difference at reading
  distance; fuzzy-up-close is acceptable for a shadow. SMAA was removed
  from that example entirely.
- **Perf verdict:** pending — re-measure the shadow-cascade share on
  `diegetic_text_stress` (Probe A text-batch opt-out delta, or the 0c
  per-pass decomposition) against the ~0.95 ms text-shadow baseline.

## 2026-06-07 — winding-only shadow re-measure + Probe B (decision signals)

- **Scene/instrument:** `diegetic_text_stress`, release, fresh launch,
  `brp_extras/get_diagnostics` `frame_time_ms.average` (~120-frame
  window), median of clean samples per condition; hitch windows
  (history_duration > ~0.9 s or visible spike) discarded.
- **Display:** built-in, `AntiAlias::Both`, unfocused, AutoNoVsync.
- **Caster census:** identical to Probe A — 3 `DiegeticTextBatch`
  (2 casting), 121 panel backings (all `NotShadowCaster`), 2 plain
  scene meshes (casting).

### Measurement 1 — text-shadow cost after the winding-only cutout

| condition | ms |
| --- | ---: |
| baseline (text casting, winding-only shadow shader) | 5.75 (5.747/5.756/5.717) |
| text batches `NotShadowCaster` | 5.75 (5.747/5.863/5.717) |

- **Delta ≈ 0.00 ms.** Opting text out of shadow casting no longer buys
  anything — the winding-only cutout recovered the full ~0.95 ms that
  Probe A attributed to text shadow rendering. **Perf verdict for the
  winding-only entry above: confirmed** (wall-clock signal; xctrace
  per-pass confirmation optional).

### Measurement 2 — Probe B: transparent-pass split (text vs panels)

All states cross-checked within clean windows of the same instance,
text shadow-opted-out throughout:

| condition | ms | delta vs visible |
| --- | ---: | ---: |
| text + panels visible | 5.71 (5.53/5.66/5.80/5.71) | — |
| text hidden (3 batches) | ~5.2 (5.38/5.03) | **−0.5 text** |
| panels hidden (121 backings, batch JSON-RPC) | 5.38 (5.38/5.38/5.28) | **−0.33 panels** |
| both hidden | not captured (external GPU contention window) | — |

- **Findings:**
  - The whole transparent-geometry lever is **~0.8 ms wall-clock**
    (text ~0.5, panels ~0.33, ≈ 60:40) — far below the 2.68 ms
    summed-channel GPU time from 0c. On Apple TBDR the transparent
    pass overlaps other passes, so summed channel ms ≠ recoverable
    wall ms.
  - **Phase 2 shader experiments can recover at most ~0.5 ms wall in
    this scene.** Consistent with Phase 1: AA-off alone recovered
    0.4–0.6 ms, i.e. most of text's wall cost here is the AA
    supersample/stride work, not the base coverage evaluation.
  - The shadow-cascade overhead lever from Probe A (~0.56 ms with zero
    casters) is now as large as the entire text transparent share.
- **Methodology hazard (logged for future probes):** external GPU
  contention (WindowServer ~35 % + Ableton ~40 % CPU) produced
  8.5–9.4 ms plateaus pinned near 120 Hz pacing, persisting for
  minutes and twice coinciding with a state flip — mimicking a real
  effect (hiding text first read as +3 ms). An empty-scene read at
  ~8.8 ms proved contention. **Toggle-back cross-check is mandatory
  before attributing any delta**; the overlay's `gpu wait` row
  distinguishes contention (high wait, quiet CPU rows) from app cost.
