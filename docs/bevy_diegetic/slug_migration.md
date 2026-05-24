# Slug migration — retiring the distance-field text renderer

## Purpose

Remove the SDF / MSDF / MTSDF text renderer so slug is hana's sole
text renderer, leaving a tight codebase with one text path. This
document is the tracked checklist for that removal. The effect-support
analysis that motivates it lives in `slug_fx.md`; this document is the
mechanical removal plan.

## Scope boundary — two unrelated "SDF" subsystems

Signed-distance fields appear in two places that share only a name:

1. **Text glyph SDF / MSDF / MTSDF** — the renderer being removed.
2. **Panel / callout primitive SDF** — `render/sdf_material.rs`,
   `shaders/sdf_panel.wgsl`, `render/sdf_stroke.wgsl`, consumed by
   `render/panel_geometry.rs` and `callouts/render.rs`, demonstrated by
   `examples/sdf.rs`. This draws rounded-rect panel borders and strokes,
   which are vector UI geometry, not glyphs.

This plan removes (1) and leaves (2) completely intact. A naive
"delete everything SDF" would break panel rendering.

## Decisions (locked)

- **Naming.** The `slug_text_spike` module moves under `text/` (as
  `text/slug/`) and the unified plugin is `TextPlugin`. Both the `text`
  module and `TextPlugin` already exist today (the distance-field engine
  owns them), so removal *frees* them for slug — a move-in, not a
  rename-around. No interim name is needed.
- **`Slug*` type prefixes are kept.** `SlugBackend`, `SlugTextMaterial`,
  `SlugRenderMode`, etc. keep their prefix; the prefix names the
  rendering technique (Lengyel's slug method), the way `MSDF` named the
  old one.
- **The renderer-selection seam is deleted entirely** — the
  `TextRenderer` enum, the `TextRendererPreference` resource, and the
  per-entity / per-style `renderer` override fields. With one renderer
  there is nothing to select.
- **Panel SDF is out of scope** and untouched.

## The seam is runtime-only and clean

`render/text_backend.rs` is the entire selector: a
`TextRenderer { DistanceField, Slug }` enum and a
`TextRendererPreference` resource. The two engines never share a
trait — they diverge at three branch points (text shaping, world-text
rendering, backend-change detection) and produce different output
components (`PanelTextQuads` vs `PanelSlugTextRun`). Removal is mostly
deleting one arm of each branch, not untangling shared code.

## The `text/` module splits cleanly

`text/` is not purely the distance-field engine; it also holds
renderer-agnostic font infrastructure that slug already depends on.

| Keep (renderer-agnostic) | Delete (distance-field only) |
| --- | --- |
| `font.rs`, `font_loader.rs`, `font_registry.rs` | `atlas.rs`, `atlas_config.rs`, `atlas_slot.rs` |
| `measurer.rs`, `constants.rs` | `bitmap_dims.rs` |
| `TextPlugin` (the shell stays) | `gpu_rasterizer/` (+ shaders), `msdf_rasterizer/` |

`TextPlugin` already sets up font/registry/measurer. Removal strips
its atlas and GPU-rasterizer initialization; the body of
`SlugTextSpikePlugin` folds into it. The `slug_text_spike/*.rs` files
then move under `text/` as `text/slug/`, and `crate::slug_text_spike::X`
becomes `crate::text::slug::X`. There is never a moment with two
`TextPlugin`s or a placeholder name.

Dependencies `parley`, `ttf-parser`, and `rayon` stay (slug uses them).

## Phases

### Phase 0 — Prove slug as the sole path (no deletion)

Flip the global default to slug and run the example/app suite. Confirm
slug renders everything the distance-field path does today: panel text,
world text, CJK, the render modes (`Text` / `PunchOut` / `SolidQuad`),
and especially shadows — MSDF has `glyph_shadow_proxy_material`; slug
has `SlugRenderMode` plus `shadow_mode`. Any capability only the MSDF
path has surfaces here, while the fallback still exists. Nothing is
deleted in this phase.

- [ ] For the Phase 0 check only, insert `TextRendererPreference::slug()`
      as a throwaway so the suite renders via slug while the DF path
      still exists for A/B. Do not change `#[default]` — the whole
      selector is deleted in Phase 1.
- [ ] Run example suite; confirm panel + world text render via slug.
- [ ] Confirm shadow parity (proxy material vs slug shadow mode).
      Note: `slug_shadow_render_mode` (`render/text_renderer/batching.rs`,
      ~line 547) maps `GlyphShadowMode::Text` to `SlugRenderMode::Text`,
      the same as `None`. Confirm this is intended, not a missing mode.
- [ ] Confirm render-mode parity (Text / PunchOut / SolidQuad).
- [ ] Operational parity, not just eyeballing: confirm OIT depth
      compositing of overlapping text (shadow proxy + visible glyph
      paint order), prepass-shadow behavior, and `depth_bias` /
      `oit_depth_offset` values match between slug and the DF path.
      Concrete suspected gap: the MSDF path emits a `glyph_shadow_proxy`
      with an `is_shadow_proxy` uniform in the prepass; slug's material
      (`material.rs`) has no `is_shadow_proxy` or `oit_depth_offset`
      concept and uses the same shader for prepass and main pass.
      Actively screenshot-compare `GlyphShadowMode::Text` shadows on
      slug vs MSDF — if slug casts no outline shadow, that is the
      hard-gate trigger (decision #5).
- [ ] The throwaway preference does not cover everything: per-entity
      `.with_renderer()` overrides take precedence over
      `TextRendererPreference` (e.g. `text_renderer_gpu_bench.rs` and
      `slug_text.rs` force their own backend). For the Phase 0 check,
      neutralize those `.with_renderer()` calls so the preference
      actually drives the suite — otherwise the parity check silently
      skips overridden text.
- [ ] Verify the `typography_overlay` feature (`debug/`) renders on
      slug and references no removed atlas types.
- [ ] If a real MSDF-only capability gap is found (shadows or
      otherwise), **stop and ask the user** before proceeding — a gap is
      a hard gate on removal, not a follow-up. Do not silently fix or
      silently proceed (see Open decisions #5).

**Phase 0 outcome — complete (2026-05-24). Gate cleared.** The
suspected shadow gap was real: slug spawned a shadow-proxy mesh but had
no main-pass discard, so the proxy painted a visible duplicate glyph in
the color pass (doubling, blue rectangles in PunchOut cells, visible
glyphs in the Invisible row). Surfaced to the user and resolved by
adding an `is_shadow_proxy` uniform + main-pass discard to slug's
material/shader, mirroring MSDF's `GlyphMaterial`; the `shadows.rs`
matrix then matched the MSDF reference. (This fix is removed again in
Phase 7 when the matrix collapses.) Confirmed on slug: world text,
screen-space diegetic-panel HUD text, render modes, small-size
crispness, and CJK glyph geometry (probe: `漢` loads/packs, no
cubic-outline rejection). Verified by shared code path rather than fresh
framing: world-space RTT panel text (same slug text path as the
screen-space panel) and OIT overlap. `typography_overlay` compiles with
the fix. The throwaway `TextRendererPreference::slug()` was reverted.

### Phase 1 — Collapse the seam (lands together with Phase 2)

This phase removes the selector. It does not compile on its own: the
distance-field arms it deletes leave imports, systems, and a material
plugin that only Phase 2 removes, so 1 and 2 land as a single sequence
(see the ordering note below).

- [ ] Delete the distance-field arm in text shaping
      (`render/text_renderer/shaping.rs`): the `else` branch of the
      `TextRenderer` match (the atlas path), the `PanelTextQuads`
      construct/clear/remove calls, and the `apply_panel_quad_result`
      helper. Drop the now-unused `use crate::text::{AtlasSlot,
      GlyphAtlas}` imports.
- [ ] Delete the distance-field arm in world-text rendering
      (`render/world_text/rendering.rs`) and its now-unused atlas imports.
- [ ] Delete the two backend-change systems in
      `render/text_renderer/mod.rs` (`mark_text_pending_on_backend_changed`,
      `clear_slug_storage_on_msdf_backend_changed`) and their
      `.add_systems` registrations; slug is unconditional.
- [ ] In `TextRenderPlugin` (`render/text_renderer/mod.rs`): drop
      `MaterialPlugin::<GlyphMaterial>` (line 46), the `SharedMsdfMaterials`
      `init_resource` (line 54), `build_panel_batched_meshes` from
      `.add_systems` (line 72), the `AtlasSwapStarted` /
      `AtlasSwapCompleted` observer registrations (lines 79–81), AND the
      `sync_panel_hue_offset` registration with its
      `.after(build_panel_batched_meshes)` ordering (line 74) — leaving
      that `.after` in place is a hard compile error once the target is
      deleted.
- [ ] Delete `PanelTextQuads`, `SharedMsdfMaterials`,
      `build_panel_batched_meshes`, and `sync_panel_hue_offset` (the
      last queries `GlyphMaterial`, MSDF-only) in
      `render/text_renderer/batching.rs`. Delete **only** `PanelTextQuads`
      — `PanelTextChild` is renderer-agnostic and persists for slug.
- [ ] Delete `TextRenderer` and `TextRendererPreference`
      (`render/text_backend.rs`) and their `lib.rs` re-exports (lines
      177–178).
- [ ] Delete the `renderer` override on `WorldText`
      (`render/world_text/mod.rs`): the field plus `with_renderer`,
      `with_default_renderer`, `renderer`, `set_renderer`.
- [ ] Delete the `renderer` field on `TextProps<C>`
      (`layout/text_props.rs`, the type behind both `LayoutTextStyle`
      and `WorldTextStyle`) plus its `with_renderer`.
- [ ] Delete `GlyphLoadingPolicy` (`layout/text_props.rs`): the enum,
      the `TextProps<C>` `loading_policy` field, the `loading_policy()`
      accessor, the `with_loading_policy()` builder, its `Reflect` /
      `PartialEq` handling, the two `WhenReady` defaults, two
      propagations, and two destructures; plus the `lib.rs` and
      `layout/mod.rs` re-exports. It is distance-field-only — its sole
      readers are the two shaping arms deleted above, and slug builds
      curve bands synchronously, so the `WhenReady` / `Progressive`
      distinction is already a no-op under slug (text appears once the
      font and glyphs are ready, with no per-glyph reveal window). The
      `typography.rs` and `font_features.rs` call sites are removed in
      Phase 5.
- [ ] Fold `SlugTextSpikePlugin::build` into `TextPlugin::build`.

Removed public APIs (for a caller-migration note): `TextRenderer`,
`TextRendererPreference`, `WorldText::with_renderer` /
`with_default_renderer` / `renderer` / `set_renderer`,
`TextProps::with_renderer`, `GlyphLoadingPolicy`, and
`TextProps::with_loading_policy` / `loading_policy`. No replacement —
the renderer is unconditional and slug reveals text once ready, so
callers delete these calls.

### Phase 2 — Delete the distance-field engine

- [ ] `text/atlas.rs`, `text/atlas_config.rs`, `text/atlas_slot.rs`,
      `text/bitmap_dims.rs`.
- [ ] `text/gpu_rasterizer/` (all files + `shaders/`).
- [ ] `text/msdf_rasterizer/`.
- [ ] `render/glyph_material.rs` and `shaders/glyph_text.wgsl`.
- [ ] The `GlyphRenderMode` discriminant compile-time assertions
      (`layout/text_props.rs`, lines ~901–904) already match slug's
      values (0–3 = Invisible/Text/PunchOut/SolidQuad, same as
      `SlugRenderMode`), so the values need no change — only re-point
      the comment that names `glyph_text.wgsl` to the slug shader.
- [ ] Reword the MSDF-stale doc comments: `GlyphRenderMode::Text`
      ("Normal MSDF text rendering"), `GlyphShadowMode::Text` and
      `::PunchOut` ("MSDF-decoded in prepass") to slug terms.
- [ ] In `DiegeticUiPlugin` (`lib.rs`): remove the
      `embedded_asset!(app, "shaders/glyph_text.wgsl")` registration
      (line 279) and `GpuRasterizerPlugin` from the plugin list (line 284).
- [ ] `drive_atlas_swap`, `target_config`, and the swap events
      (`AtlasSwapStarted` / `AtlasSwapCompleted`) from `text/mod.rs`,
      plus their `pub use` re-exports.
- [ ] The public types these expose: `DistanceField`, `RasterBackend`,
      `RasterQuality`, `AtlasConfig`, `GlyphAtlas`, `AtlasSlot`,
      `AtlasPreference`, `GlyphMaterial`, `GpuRasterizerPlugin`, plus the
      atlas glyph-lookup types defined in `atlas.rs` (`GlyphKey`,
      `GlyphLookup`, `GlyphMetrics`, `GpuAtlasRegion`) — including their
      `lib.rs` and `text/mod.rs` re-exports.

### Phase 3 — Trim dependencies

Ordering: remove the Cargo.toml entries only **after** every Phase 2
file that imports `fdsm` / `fdsm-ttf-parser` is deleted, otherwise
`cargo check` fails on the still-present imports. Checkpoint first:
`cargo build && cargo nextest run` should pass at the end of Phase 2
(the unused deps are still declared but harmless), confirming Phase 2
is complete before this pure-cleanup phase.

- [ ] Workspace `Cargo.toml`: remove `fdsm`, `fdsm-ttf-parser`, dev-dep
      `msdfgen`, dev-dep `ttf-parser_018`.
- [ ] `bevy_diegetic/Cargo.toml`: remove the same.
- [ ] Confirm no remaining reference to the removed crates.

### Phase 4 — Move slug into `text/`

- [ ] Relocate `slug_text_spike/*.rs` to `text/slug/`.
- [ ] Update `crate::slug_text_spike::X` references to `crate::text::slug::X`.
- [ ] Delete the now-empty `slug_text_spike/` directory.
- [ ] Make every `Slug*` type `pub(crate)` and drop all `Slug*`
      re-exports from `lib.rs` (expose-nothing — see Open decisions #3).
      The public text API is the existing agnostic surface only.
      Optional follow-up: tighten types used only within `text/slug/`
      to `pub(in crate::text::slug)`; only the types `render/` consumes
      (`SlugBackend`, `SlugPreparedTextRun`, `SlugRunStorage`,
      `SlugRunStorageKey`, `SlugTextMaterial`, `SlugTextMaterialInput`,
      `SlugRenderMode`) need `pub(crate)`.
- [ ] Update doc comments that still say "Experimental" /  "spike"
      (e.g. `SlugBackend`) to reflect production status.

### Phase 5 — Examples and benches

Build-green caveat: examples are compiled by `cargo build --examples`
and the Phase 6 suite run. `typography.rs` and `text_renderer_gpu_bench.rs`
import types deleted in Phase 2
(`DistanceField` / `AtlasConfig` / `RasterBackend` / `RasterQuality` /
`GlyphAtlas`); `typography.rs` and `font_features.rs` also import
`GlyphLoadingPolicy`, deleted in Phase 1; `preload_text.rs` is built on
the deleted atlas preload API and is itself deleted in this sequence;
`slug_text.rs` uses `Slug*` types that go `pub(crate)` in Phase 4. So the edits that *remove*
deleted-type usage (and the deletions themselves) must land inside the
Phase 1–3 sequence (and the `Slug*` de-references at Phase 4), not be
deferred — otherwise the examples break. What remains genuinely
"Phase 5" is slug-specific polish. The rows below are the authoritative
per-example dispositions; they are listed here for cohesion but
**executed within the Phase 1–3 sequence** (and Phase 4 for `Slug*`
de-references), not as a separate later pass.

| Target | Action |
| --- | --- |
| `slug_text.rs` | Rework to consume the public API (spawn `WorldText`); remove the `.with_renderer(TextRenderer::Slug)` calls and the renderer-toggle UI/state (deleted in Phase 1). **Retain the CJK demonstration** (parallel Latin + CJK render) since it backs the Phase 0 CJK check. Drop only the spike instrumentation that pokes `Slug*` internals (`log_glyph_metrics`, `log_cjk_probe`, `SlugPackedGlyph`, `build_packed_glyph`). Keep as a production text example. |
| `world_text.rs`, `panel_rendering.rs` | Strip the renderer-choice UI/state; one renderer, no toggle. Keep. |
| `typography.rs` | Delete the renderer-toggle entirely: the `switch_text_mode` system + its registration, `TypographyTextMode`, `toggle_backend`, `pick_raster_quality`, and the direct `DistanceField` / `RasterBackend` / `RasterQuality` / `AtlasPreference` / `TextRendererPreference` usage. Also remove the single `.with_loading_policy(GlyphLoadingPolicy::Progressive)` call and the `GlyphLoadingPolicy` import (the policy is deleted in Phase 1). Keep behind `typography_overlay`. |
| `font_features.rs` | Remove all `GlyphLoadingPolicy` usage: the `loading_policy` field threaded through its two helper structs, the `progressive` binding, every `.with_loading_policy(...)` call, and the import. The OpenType-feature demo itself is unaffected — the policy is a no-op under slug. Keep. |
| `shadows.rs` | Keep — uses the agnostic `GlyphRenderMode` / `GlyphShadowMode` API, renders via slug unchanged (the Phase 0 slug throwaway is reverted at the end of Phase 0). Deleted in **Phase 7** when the matrix collapses. |
| `text_renderer_gpu_bench.rs` | Convert by reduction to slug-only: drop the msdf / sdf / mtsdf / empty modes and `AtlasConfig`. Keep as a slug regression/optimization harness (note in `slug-benchmark-procedure.md` that cross-renderer comparison is gone). |
| `atlas_pages.rs` | Delete (visualizes atlas pages; no slug analog). |
| `preload_text.rs` | **Delete.** Built on the distance-field atlas preload API (`GlyphAtlas::preload`) and `GlyphLoadingPolicy`, both removed in the Phase 1–2 sequence. Slug needs no preload demo: per-glyph band-building is sub-millisecond — full printable ASCII preps in ≈ 0.84 ms (after per-curve dedup + 48-band tuning), well below one frame and below frame-timing resolution. There is no warm-up cost worth showcasing and no preload API is shipped. A project with very large glyph sets that notices first-frame lag can warm glyphs with its own Bevy task / async setup — no engine API required. |
| `benches/glyph_rasterization.rs` | Delete (CPU/GPU MSDF rasterizer bench; no slug analog). |
| `examples/sdf.rs` | Untouched (panel SDF). |

### Phase 6 — Verify

- [ ] `cargo build`
- [ ] `cargo nextest run`
- [ ] `cargo +nightly fmt`
- [ ] Run the example suite; screenshot text to confirm parity.

### Phase 7 — Collapse the glyph render/shadow matrix

Rationale and design: `slug_fx.md` §8. With MSDF gone, slug's
`GlyphRenderMode × GlyphShadowMode` matrix and its shadow-proxy mesh
are the last inherited complexity. Decisions: drop mismatched-silhouette
shadows (the sole reason the proxy exists) and `SolidQuad`; make
shadow-only text a documented recipe; keep `PunchOut` as a fill effect.
slug's visible mesh already casts its own matching silhouette shadow —
its prepass discards on coverage, not alpha mode (§8.1) — so removing
the proxy loses nothing for matching shadows. This phase is independent
of the Phase 1–3 sequence and leaves the build green.

- [ ] Delete the slug shadow-proxy material path: the `is_shadow_proxy`
      uniform field (`slug_text_spike/material.rs` `SlugTextUniform` and
      `shaders/slug_text.wgsl`), the `slug_text_shadow_proxy_material`
      constructor plus the `build_slug_text_material` proxy parameter
      (fold back to one `slug_text_material`), the main-pass
      `is_shadow_proxy` discard in the shader, and the `mod.rs` re-export.
- [ ] Delete the proxy spawn paths and markers: `WorldTextShadowProxy`
      + `slug_world_text_shadow_proxy_material` + the `needs_proxy`
      branch (`render/world_text/mesh_spawning.rs`); `DiegeticShadowProxy`
      + `slug_panel_shadow_proxy_material` + the proxy branch
      (`render/text_renderer/batching.rs`); and `slug_shadow_render_mode`
      in both. The old-mesh queries that union in `WorldTextShadowProxy`
      reduce to the visible-mesh marker only.
- [ ] Replace the `needs_proxy` / `suppress_shadow` logic with a plain
      cast-shadow toggle: the visible mesh is a shadow caster unless the
      caller turns shadows off, in which case it gets `NotShadowCaster`.
      No second mesh — the visible mesh casts its own silhouette.
- [ ] Shrink `GlyphRenderMode` to `{ Text, PunchOut }` — drop `Invisible`
      and `SolidQuad`. Update the compile-time discriminant assertions
      (`layout/text_props.rs`), `SlugRenderMode` (`material.rs`), and the
      shader's `RENDER_MODE_SOLID_QUAD` constant plus its `SolidQuad` /
      `Invisible` branches in `render_coverage`.
- [ ] Collapse `GlyphShadowMode` to a cast toggle (`{ None, Cast }`),
      replacing the `None / SolidQuad / Text / PunchOut` silhouette
      choice. Update `with_shadow_mode` and its call sites.
- [ ] Document the shadow-only recipe — spawn a cast-on glyph with fill
      alpha 0 (invisible in color, full silhouette in shadow) — on the
      cast toggle and in `slug.md`, replacing the deleted `Invisible`
      render mode (§8.3).
- [ ] Solid-block backings: callers that used `SolidQuad` compose a
      standard `Mesh3d` rectangle with a `StandardMaterial` (§8.2); there
      is no slug render mode for it.
- [ ] Delete `examples/shadows.rs` — its subject, the matrix, is gone.
- [ ] Update any remaining `with_render_mode(Invisible | SolidQuad)` /
      `with_shadow_mode(...)` call sites in examples and `debug/`.
- [ ] `cargo build`, `cargo nextest run`, `cargo +nightly fmt`; rerun the
      suite and screenshot to confirm Text and PunchOut fills, the
      cast-shadow toggle, and the alpha-0 shadow-only recipe.

Public-API note: `GlyphRenderMode` and `GlyphShadowMode` stay public
(callers name them for fill / shadow intent) but shrink — consistent
with decision #3, exposing only what a feature needs.

## Documentation disposition

The docs are committed, so deletion is recoverable; the recommendation
favors removing what only describes the deleted engine and keeping
everything slug.

| Doc | Recommendation | Reason |
| --- | --- | --- |
| `gpu_rasterizer.md` (1539 lines) | **Delete** | Documents only the GPU SDF/MSDF rasterizer being removed. No slug content. |
| `slug.md` (1275 lines) | **Keep, update status** | The slug backend design and source/license reference. Its framing ("experimental alternative, not a replacement for MTSDF") is now stale and should be updated to reflect slug as the sole renderer. |
| `slug-experiments.md` (1798 lines) | **Keep, fix stale refs** | The experiment log that prevents repeating failed approaches. It references `benches/glyph_rasterization.rs` for CPU prep cost; that bench is deleted in Phase 5, so the prep-cost reference needs a new home or a note. |
| `slug-benchmark-procedure.md` (244 lines) | **Keep, fix stale ref** | Canonical slug benchmark procedure. References `text_renderer_gpu_bench` (kept, converted to slug-only) but also the deleted `glyph_rasterization` bench for prep cost — note that prep cost is no longer tracked, or point at a slug replacement. |
| `slug_fx.md` (641 lines) | **Keep** | The effect-support plan that motivates this migration. |

Other (non-`docs/`) artifacts that reference the removed engine, found
in review — fold these into Phase 5/6:

- `crates/bevy_diegetic/README.md` — describes "MSDF text rendering" as
  the current renderer (several lines incl. the transparency section).
  Reword to slug.
- `scripts/xctrace_text_renderer.sh` — supports `sdf` / `msdf` / `mtsdf`
  modes for `text_renderer_gpu_bench`. Reduce to `slug` / `empty`.
- `ci.yml` — runs `cargo bench` including the deleted `glyph_rasterization`
  bench; drop that bench from CI.

## Build-green ordering

Phase 1 does not compile on its own — deleting the distance-field arms
leaves imports, systems, and the `GlyphMaterial` material plugin
referencing types that only Phase 2 removes. So Phases 1, 2, and 3
land as one sequence with no green checkpoint between them; the suite
compiles again at the end of Phase 3. The example edits that *remove*
deleted-type usage (`typography.rs`, `font_features.rs`,
`text_renderer_gpu_bench.rs`) and the deletion of `preload_text.rs`
also belong in this sequence — examples are compiled by
`cargo build --examples`. Phase 0 (verification) and the
slug-specific polish in Phases 4–6 each leave the build green.

## Open decisions

Surfaced by team review; outcomes recorded here as resolved.

1. **Module layout — Resolved: nested `text/slug/`.** Slug's files
   live under `text/slug/`, preserving the boundary between the renderer
   and the font infrastructure (font/registry/measurer) that it consumes.
   Call sites are `crate::text::slug::X`.
2. **Plugin decomposition — Resolved: unified `TextPlugin`.** One
   plugin owns font setup and slug backend init; they always travel
   together inside `DiegeticUiPlugin`, so no split is warranted.
3. **Public `lib.rs` re-export cut — Resolved: expose nothing.** No
   `Slug*` type appears in any public fn/field signature; the public
   text API is the existing renderer-agnostic surface (`WorldText`,
   `WorldTextStyle`/`TextProps`, `GlyphRenderMode`, `GlyphShadowMode`,
   `GlyphSidedness`). All `Slug*` types become `pub(crate)` and drop
   from `lib.rs`. A type is promoted to public only when a specific
   slug_fx artistic effect requires a developer to name it — none do
   today.
4. **Phase 0 activation — Resolved: no lasting mechanism.** There is
   exactly one renderer after this; `TextRenderer`,
   `TextRendererPreference`, and `DistanceField` are all deleted and
   slug is unconditional. Phase 0 inserts `TextRendererPreference::slug()`
   as a throwaway only to A/B against the still-present DF path during
   the parity check; Phase 1 deletes the entire selector.
5. **Shadow-parity gate — Resolved: hard gate, stop and ask.** A
   parity gap found in Phase 0 (e.g. the suspected outline-shadow gap
   from `slug_shadow_render_mode` mapping `Text` to `Text` like `None`)
   blocks removal. Stop and ask the user — do not silently fix or
   silently proceed.
6. **`preload_text` — Resolved: delete; no preload demo, no preload
   API.** Slug prep is sub-millisecond (full printable ASCII ≈ 0.84 ms
   after per-curve dedup + 48-band tuning), so there is no warm-up cost
   worth demonstrating, and it sits below frame-timing resolution.
   Exposing a preload API would breach expose-nothing (decision #3) for
   no real benefit. A project with a very large glyph set that notices
   first-frame lag can warm glyphs with its own Bevy task / async
   framework. The earlier rewrite-to-slug-preload plan and the deferred
   `slug_preload_feature.md` follow-up are both dropped.

## Proposed user decisions

Surfaced by review; pending sign-off.

- **P1 — Preload feature: in-scope vs follow-up.** *(superseded by P1'
  in cycle 2, then dropped — see below.)*
- **P1' — Preload demo needs no new public API.** *(dropped
  2026-05-24)* Initially approved as a rewrite of `preload_text` to a
  slug preload via existing surfaces. Dropped once the actual numbers
  were checked: slug prep is sub-millisecond (full printable ASCII
  ≈ 0.84 ms after per-curve dedup + 48-band tuning), too small to
  demonstrate and below frame-timing resolution, so a frame-timed demo
  would show noise. Final decision: **delete `preload_text`, ship no
  preload API.** A project that needs warming can use its own Bevy task
  / async setup. See Open decision #6.
