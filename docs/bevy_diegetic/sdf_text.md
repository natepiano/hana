# SDF / MSDF Text Rendering Toggle

## Status

Implementation plan. Adds plain SDF (single-channel) as an alternative to
the current MSDF (multi-channel) text path, with a runtime toggle in the
typography example for A/B comparison.

## Motivation

The current MSDF pipeline rasterizes each glyph at a fixed canonical pixel
size into a 3-channel signed distance field. The shader takes the median
of the three channels per pixel; the median trick preserves sharp corners
that a single-channel SDF would round off.

The cost: on glyphs with thin tapered features at low canonical resolution,
two of three channels disagree about which side of the sub-pixel feature a
pixel is on, the median jumps abruptly, and the rendered silhouette shows
a pointy/wedge artifact instead of the actual curve. The EB Garamond `V`
ear tip is the canonical example — at canonical 64 it renders as a
single-pixel needle; at 128 it's a small wedge; at 256 it's mostly round
with a tiny corner visible only at extreme zoom.

Plain SDF stores one signed distance per pixel. No median, no channel
disagreement. Smooth curves stay smooth at any resolution. The trade-off
is that sharp corners (terminals, intersections, the inside angle of a
`V` apex) get rounded off — single-channel distance fields cannot
represent two intersecting edges meeting at a point.

For typography that's curve-heavy (Old Style serifs like EB Garamond,
italic scripts, hairline weights), plain SDF can render visibly closer to
the source outline. For typography with strong corners (sans-serif,
slab-serif, monospace), MSDF stays superior.

A per-atlas mode toggle lets each app pick the right trade for its font
family, and lets the typography overlay compare both renderings on the
same glyph.

## Non-goals

- Per-glyph mode selection. The toggle is per-atlas — all glyphs in an
  atlas share a single mode.
- Eliminating the MSDF path. Both modes ship; MSDF stays the default.
- Vector / outline rendering. Plain SDF is still a rasterized distance
  field with the same atlas + per-glyph-quad pipeline. Genuine vector
  rendering is a separate, much larger project.
- Switching MSDF/SDF mode mid-frame or per-draw within one atlas. The
  toggle triggers a full atlas regeneration.

## Approach

One enum (`SdfMode`) flows from `AtlasConfig` through the atlas, through
the rasterizer, through the material uniform, into the fragment shader's
distance computation. Everything else (atlas packing, glyph quad
construction, layout, shaping, panel clipping, shadow rendering) is
unchanged.

```text
AtlasConfig.sdf_mode  →  MsdfAtlas.sdf_mode
                              │
                              ├─→ rasterize_glyph(mode) → fdsm generate_msdf | generate_sdf
                              │                            (3-channel)      | (1-channel into R, replicated)
                              │
                              └─→ MsdfMaterial.sdf_mode  →  msdf_text.wgsl: median(rgb) | sample.r
```

## Phases

### Phase 1 — Rasterizer accepts a mode

**Files:** `text/msdf_rasterizer/mod.rs`, `text/msdf_rasterizer/parity.rs`

**fdsm API (verified against fdsm 0.8 source):**
- `fdsm::generate::generate_msdf(...)` — RGB output, already in use.
- `fdsm::generate::generate_sdf(component, range, dest)` — `Luma<P>`
  output (single channel). Takes the un-colored `PreparedComponent`
  directly; no `edge_coloring_simple` step needed for SDF.
- `fdsm::render::correct_sign_sdf(sdf, component, fill_rule)` — same role
  as `correct_sign_msdf` but for `Luma`.
- **No `correct_error` step for SDF** — that's MSDF-specific (fixes
  channel-disagreement artifacts that single-channel can't have).

**Work:**
- Add `pub enum SdfMode { Msdf, Sdf }` to the rasterizer module.
- `rasterize_glyph` gains a final `mode: SdfMode` parameter.
- `Msdf` branch: existing path unchanged
  (`edge_coloring_simple` → `generate_msdf` → `correct_sign_msdf` →
   `correct_error_msdf`).
- `Sdf` branch: skip `edge_coloring_simple` (it's a multi-channel concern);
  call `outline.prepare()` directly on the un-colored outline, then
  `generate_sdf` into a `Luma<f32>` image, then `correct_sign_sdf`. No
  error correction. Convert `f32 → u8`, then replicate the single channel
  into R/G/B of the output `MsdfBitmap.data` so downstream atlas insert,
  gutter replication, and UV mapping need no changes.
- All existing call sites pass `SdfMode::Msdf`.
- Tests:
  - One per-mode test asserting Sdf bitmap has R==G==B everywhere.
  - Both modes return a bitmap of the same dimensions for a given glyph.
  - Degenerate-glyph tests for the Sdf path (matching the MSDF test
    coverage): empty outline returns `None`; very small glyphs don't panic;
    self-intersecting paths produce non-trivial bitmaps. Use the existing
    `eb_garamond_*` and `rasterize_*` tests as the template.
- Parity test (`parity.rs`) stays MSDF-only — msdfgen reference is
  multi-channel; an SDF parity test would need a separate reference and
  isn't part of this scope.

### Phase 2 — Atlas threads mode through

**Files:** `text/atlas.rs`, `text/atlas_config.rs`, `text/constants.rs`
(if a default constant is needed)

- Add `sdf_mode: SdfMode` field to `AtlasConfig`. Default `Msdf`.
  `SdfMode` derives `Clone, Copy, Debug, Default, PartialEq, Eq, Reflect`
  with `#[default] Msdf` so `AtlasConfig::default()` works without manual
  field assignment.
- Add `const fn with_sdf_mode(mut self, mode: SdfMode) -> Self` builder
  on `AtlasConfig` (matches the existing `const fn with_quality` style).
- Add `sdf_mode: SdfMode` field to `MsdfAtlas`. The no-config
  constructors (`new`, `with_size`) default to `Msdf`. `with_config` gains
  a `sdf_mode` parameter.
- Internal rasterize call sites in `atlas.rs` (the sync `get_or_insert_sync`
  and the async worker dispatch path) read `self.sdf_mode` and pass it to
  `rasterize_glyph`. **The async closure captures the mode at dispatch
  time** so a mode change between dispatch and result intake is detectable
  (relevant to Phase 4).
- `pub const fn sdf_mode(&self) -> SdfMode` accessor on `MsdfAtlas` for
  downstream (material build) code.
- Plugin init (`text/mod.rs`) plumbs the `AtlasConfig.sdf_mode` into the
  atlas at construction. Existing callers that don't set `sdf_mode` get
  the default `Msdf`, so no app-level changes are forced.
- Update the existing `default_config_values` test in
  `text/atlas_config.rs` to assert `config.sdf_mode == SdfMode::Msdf`
  (the test will fail to compile until the field exists, then needs the
  new assertion).
- `GlyphMetrics` is **mode-agnostic**: UV rect, bearings, and pixel
  dimensions are identical whether the bitmap was rasterized as MSDF or
  SDF. Document this invariant in the `GlyphMetrics` doc comment and add
  a test asserting that MSDF and SDF rasterizations of the same glyph
  yield identical `GlyphMetrics` (sanity-check the invariant doesn't drift).

### Phase 3 — Shader branches on a uniform

**Files:** `render/msdf_material.rs`, `shaders/msdf_text.wgsl`,
material spawn sites in `render/text_renderer/batching.rs` (5 spawn
sites: lines 429, 445, 522, 590, 606) and
`render/world_text/mesh_spawning.rs` (2 spawn sites: lines 106, 153).

**The good news on spawn-site coverage:** all current spawn sites flow
through the shared helpers `msdf_material::msdf_text_material(...)` and
`msdf_material::msdf_shadow_proxy_material(...)`, which both delegate to
the private `build_msdf_material(...)`. Adding `sdf_mode: SdfMode` to
`MsdfTextMaterialInput` + `MsdfShadowProxyMaterialInput` + the helper
signature means every spawn site becomes a compile error until it passes
the mode. The helper enforces "you can't construct an MSDF material
without specifying the mode." No `callouts/render.rs` exists — the only
callout code (`callouts/render.rs`'s nearest equivalent is panel-edge
rendering in `panel_geometry.rs`) doesn't go through the MSDF material.

**Uniform layout:** `MsdfTextUniform` uses `#[derive(ShaderType)]`
(encase), which handles WGSL std140 alignment automatically. Adding a
`u32 sdf_mode` between `is_shadow_proxy: u32` and `clip_rect: Vec4`
keeps two `u32`s together (8 bytes), pads to 16 for the `Vec4`, and
encase derives the same padding in the WGSL struct via the
`AsBindGroup` machinery. **There is no manual byte-offset arithmetic to
get wrong** as long as the WGSL struct is updated in the same field
order. Add a test that round-trips a known `MsdfTextUniform` instance
through encase's `WriteInto` and asserts the byte length matches what
WGSL expects (`<MsdfTextUniform as ShaderType>::min_size()`).

**Shader change:**
- Current (`shaders/msdf_text.wgsl:95`):
  ```wgsl
  let sd = median(msdf_sample.r, msdf_sample.g, msdf_sample.b) - 0.5;
  ```
- New:
  ```wgsl
  let raw = select(
      msdf_sample.r,
      median(msdf_sample.r, msdf_sample.g, msdf_sample.b),
      uniforms.sdf_mode == 0u,
  );
  let sd = raw - 0.5;
  ```
- The `-0.5` bias and the `screen_px_range()` adaptive AA step
  (lines 76-86) apply identically to both modes. Don't introduce a new
  `msdf_median` function — reuse the existing `median()` at line 72.
- The branch is uniform across all fragments of a draw call; modern GPUs
  have zero perf penalty for uniform control flow.

### Phase 4 — Mode switch triggers atlas regeneration

**Files:** `text/atlas.rs` (regeneration logic), `text/mod.rs` (driver
system), possibly a new `SdfModePreference` resource module.

This is the trickiest phase. The atlas owns thousands of glyph bitmaps
keyed by glyph_id; on a mode switch they all become stale.

- Introduce a `SdfModePreference(SdfMode)` resource separate from
  `AtlasConfig` (config is initial state; preference is mutable runtime
  state).
- A system watches `SdfModePreference` for change.
- On change:
  1. Mark the atlas as `Regenerating { from: SdfMode, to: SdfMode }`.
     New shaping/layout requests during this window are not satisfied
     (return `GlyphLookup::Queued` for everything).
  2. Snapshot the set of cached `GlyphKey`s.
  3. Clear `glyphs`, clear each page's allocator and pixel buffer,
     reset page state to `Dirty`.
  4. Set `atlas.sdf_mode = to`.
  5. Enqueue re-rasterization of every snapshotted key using the new
     mode. Reuse the existing async worker pipeline.
  6. When the queue drains, mark atlas as `Ready`.
- Materials don't need to be rebuilt during the swap — they read the
  uniform fresh each frame, so once the atlas reports `Ready` and the
  uniform on the next dispatch picks up the new mode value, rendering
  resumes correctly.
- Brief visual disruption (text disappears or flickers) during the
  regeneration is acceptable; this is a debug toggle, not a hot path.
- Race-condition guard: any in-flight `RasterizeResult` from a worker
  started before the regeneration that comes back during/after must be
  discarded (its bitmap is in the old mode). A monotonic
  `regeneration_epoch` on the atlas, stamped onto each dispatched job
  and checked on result intake, handles this.

### Phase 5 — Typography example UI

**Files:** `examples/typography.rs`

- Add an `SdfRenderMode` resource (Msdf / Sdf) with the same enum
  pattern as `OverlayState` already in the example.
- Add a key binding (suggest `S`) that toggles the resource.
- Add a chip to the title bar wired via `wire_chip_to_state` (same
  pattern as `T Overlay` and `←/→ Cycle Word`):
  ```rust
  .with_title_bar(
      TitleBar::new()
          .control("T Overlay")
          .control("←/→ Cycle Word")
          .control("S SDF/MSDF"),
  )
  .wire_chip_to_state::<SdfRenderMode, _>("S SDF/MSDF", |state| match state {
      SdfRenderMode::Sdf  => ControlActivation::Active,
      SdfRenderMode::Msdf => ControlActivation::Inactive,
  })
  ```
- A small system mirrors `SdfRenderMode` into the
  `SdfModePreference` resource the atlas watches.
- Initial state is `Msdf` (matches the atlas's default).

## Risks

1. **Phase 4 regeneration races.** In-flight workers must not contaminate
   the new atlas. The epoch guard is the right mechanism; needs careful
   test coverage. Open design questions remain (see Open Decisions
   below).
2. **SDF may round visible corners more than expected.** This is the
   architectural trade and is the whole point of letting the user toggle.
   No mitigation needed beyond confirming via the typography overlay.
3. **Atlas memory.** Both modes use the same RGBA texture format
   regardless of channel count needed. SDF mode wastes 3 channels but
   keeps everything else identical. A future optimization could use a
   single-channel R8 texture for SDF-only atlases, but that requires a
   second pipeline and is out of scope.

**Investigated and addressed in the phases above:**
- fdsm 0.8 has `generate_sdf` (`Luma` output) and `correct_sign_sdf`.
  No error correction step needed for SDF. Spec'd in Phase 1.
- Uniform struct layout: `MsdfTextUniform` uses `ShaderType` derive
  (encase), which handles WGSL std140 padding. Spec'd in Phase 3 with
  a round-trip size test.
- All material spawn sites flow through `build_msdf_material`; adding
  the field to the helper signature makes every spawn site a compile
  error until updated. Spec'd in Phase 3.
- Shader change details (preserve `-0.5` bias, preserve adaptive AA,
  reuse existing `median()` function). Spec'd in Phase 3.
- Default config test + GlyphMetrics doc + GlyphMetrics mode-agnostic
  test. Spec'd in Phase 2.

## Out of scope (deliberately deferred)

- Per-font auto-selection (e.g., "use SDF for serif italics, MSDF for
  sans"). Could be a follow-up once we have data on which font/mode
  combinations look best.
- R8-only atlas format for SDF mode. Memory optimization, structural
  change, defer until justified.
- Mixed-mode atlases (some glyphs MSDF, some SDF). Adds significant
  complexity to atlas storage and material bind groups; not justified
  without a concrete need.
- Sub-pixel positioning improvements (orthogonal concern).
- A control-panel UI for runtime atlas reconfiguration (canonical size,
  SDF range, etc.). The toggle here is one specific debug affordance,
  not a general atlas tuning surface.

## Estimated cost

2.5–3 days of focused work. Phases 1–3 are mechanical and can land in a
single PR; phase 4 is the only one with real design risk and should be
its own PR with regeneration-correctness tests. Phase 5 is small and
follows phase 4.
