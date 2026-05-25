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
- [ ] Fold `SlugTextSpikePlugin` into `TextPlugin::build`. The `SlugBackend`
      init and `MaterialPlugin::<SlugTextMaterial>` move into `TextPlugin`
      (added before the font-parse gate so the material type and backend
      always register). The `embedded_asset!(app, "shaders/slug_text.wgsl")`
      registration **cannot** move yet: `embedded_asset!` resolves
      `include_bytes!` / `file!()` relative to the calling file, and the
      shader still lives in `slug_text_spike/shaders/` until Phase 4. So it
      stays as a `register_slug_text_shader(app)` helper in the slug module
      (`material.rs`) that `TextPlugin` calls; the `SlugTextSpikePlugin`
      struct + its `Plugin` impl are deleted, the `mod.rs` re-export becomes
      `pub(crate) use … register_slug_text_shader`, and the `lib.rs`
      `SlugTextSpikePlugin` re-export is dropped. **Phase 4 folds the helper
      in when the shader file moves** (tracked there).

Removed public APIs (for a caller-migration note): `TextRenderer`,
`TextRendererPreference`, `WorldText::with_renderer` /
`with_default_renderer` / `renderer` / `set_renderer`,
`TextProps::with_renderer` / `with_default_renderer` / `renderer`,
`GlyphLoadingPolicy`, `TextProps::with_loading_policy` / `loading_policy`,
and `SlugTextSpikePlugin` (folded into `TextPlugin`; setup is now
automatic). No replacement — the renderer is unconditional and slug
reveals text once ready, so callers delete these calls.

Checklist gaps filled (not enumerated above, but required for the slug
path to stay coherent once the deleted types are gone):
`PanelTextAlpha`'s `CascadePanelChild::EntityOverride` was repointed from
the deleted `PanelTextQuads` to `PanelSlugTextRun`;
`reconcile.rs::poll_atlas_glyphs` lost its `SharedMsdfMaterials` param and
the two `shared_mats.clear()` calls; and the distance-field call-trees that
referenced the deleted `GlyphLoadingPolicy` / `PanelTextQuads` /
`TextRenderer` (`shape_world_text` + its helpers,
`DistanceFieldWorldTextRenderServices`, `shape_text_to_quads`,
`all_glyphs_ready_when_required`, the whole `build_panel_batched_meshes`
batching subsystem) were deleted with them. The still-present
distance-field engine code that only references Phase-2 types
(`glyph_material.rs`, the atlas modules) is left dangling as dead code until
Phase 2 — it compiles (dead-code is a warning, not an error), so it introduces
no Phase-1 errors. (`spawn_world_text_meshes` + `MeshSpawnAssets` in
`mesh_spawning.rs` are now dead **and** will fail to compile once Phase 2
deletes `glyph_material.rs`/`atlas.rs` — Phase 2 must delete them; see the
Retrospective and Phase 2.) **The library builds green** after Phase 1 (the
dead DF code still references Phase-2 types that exist). Only the **examples**
that call the removed public APIs do not build (Phase 5 / the 1–3 sequence).

**Phase 1 outcome — complete (2026-05-24). Verified green.** All ten checklist
items plus the gap-fills above are implemented. `cargo build -p bevy_diegetic
--lib` finishes (dead-code warnings only) and `cargo nextest run --lib` passes
**234/234** (2 skipped). The workspace `nalgebra` bump `0.34 → 0.35.0` that had
broken the `fdsm`-based rasterizers was reverted to `0.34` (nothing in the
workspace needs 0.35 — only the to-be-deleted rasterizers use nalgebra), so the
lib is green now rather than waiting on Phase 2. Not committed.

### Retrospective

**What worked:**
- The seam was runtime-only as the plan claimed — each
  `if selected == Slug { … } else { … }` collapsed to its slug arm
  mechanically once the DF call-trees were traced.
- Zero errors in any Phase-1-touched file; `cargo +nightly fmt` clean.

**What deviated from the plan:**
- The checklist under-specified deletion depth. Deleting `GlyphLoadingPolicy`
  / `PanelTextQuads` / `TextRenderer` forced deleting their DF-only call-trees
  (`shape_world_text` + `build_glyph_quads` / `ensure_all_glyphs_ready` /
  `measure_anchor_offset`; `DistanceFieldWorldTextRenderServices`;
  `shape_text_to_quads` + `TextQuadServices`; `all_glyphs_ready_when_required`;
  the entire `build_panel_batched_meshes` subsystem) — required for compile,
  not enumerated.
- `PanelTextAlpha::EntityOverride` repointed `PanelTextQuads` → `PanelSlugTextRun`
  (slug cascade dependency the checklist missed).
- `reconcile.rs::poll_atlas_glyphs` carried a `SharedMsdfMaterials` param + two
  `shared_mats.clear()` calls — removed (not mentioned in the checklist).
- The `SlugTextSpikePlugin` fold could not be verbatim: `embedded_asset!` is
  locked to its calling file, so shader registration stays as a
  `register_slug_text_shader` helper until the file moves in Phase 4.

**Surprises:**
- The workspace `Cargo.toml` had a `nalgebra` bump `0.34 → 0.35.0` (external to
  this work) that broke the `fdsm`-based `gpu_rasterizer` / `msdf_rasterizer`
  (5 type errors: `fdsm 0.8` pins nalgebra 0.34, so the two versions coexisted
  as incompatible types). Nothing in the workspace needs 0.35 — only the
  to-be-deleted rasterizers use nalgebra — so it was **reverted to `0.34`**,
  restoring a green lib immediately rather than waiting for Phase 2.
- DF code that references only Phase-2 types (`spawn_world_text_meshes` +
  `MeshSpawnAssets` in `world_text/mesh_spawning.rs`, `glyph_material.rs`, the
  atlas modules) is now dead-but-compiling — warnings, not errors.

**Implications for remaining phases:**
- Phase 2 should also delete the now-dead `spawn_world_text_meshes` +
  `MeshSpawnAssets` in `world_text/mesh_spawning.rs` and the
  `atlas::GlyphLookup` (and sibling) re-exports in `text/mod.rs` — currently
  unused-import warnings.
- Phase 4 now owns the `register_slug_text_shader` → `TextPlugin` inlining
  (added to its checklist).

### Phase 1 Review

Architect re-evaluation of Phases 2–7 against the shipped Phase 1. All findings
folded into the plan (none rejected); the only genuine open question — the
external `nalgebra` bump — was raised with the user directly, not gated here.

- **Phase 2** gained four compile-required deletions the checklist had omitted:
  `poll_atlas_glyphs` + `AtlasSlot` import in `reconcile.rs`; the
  `.after(poll_atlas_glyphs)` rewiring of the four surviving systems in
  `text_renderer/mod.rs`; gutting the MSDF atlas init in `TextPlugin::build`;
  and deleting the now-dead `spawn_world_text_meshes` / `MeshSpawnAssets` in
  `world_text/mesh_spawning.rs`.
- **Phase 2** public-type list extended: `AtlasConfigError`,
  `GlyphWorkerThreads`, `GpuEnqueueResult`, `GpuGlyphBudget`, `enqueue_gpu_glyph`.
- **Build-green ordering** corrected: with the `nalgebra` bump reverted, the
  **lib is green after Phase 1** (234 tests pass); only `cargo build --examples`
  is red across the 1→2 gap, since Phase 1 touched no examples. The full suite
  returns at the end of Phase 2.
- **Phase 3** gained `nalgebra` removal from `bevy_diegetic/Cargo.toml` (dead
  there once the rasterizers are gone).
- **`nalgebra` bump reverted** to `0.34` (the cause of the only hard errors;
  nothing needs 0.35) — lib green immediately, no longer a Phase-2 dependency.
- **Phase 4** expose-nothing scope corrected to the full ~28 `Slug*` `lib.rs`
  re-exports (not just the 7 `render/` consumers), with a delete-vs-private note
  for the spike-only types (`SlugPackedGlyph`, `build_packed_glyph`).
- **Phase 7** enumerated the `From<GlyphRenderMode>` impls + `slug_render_mode` /
  `slug_shadow_render_mode` match arms in `mesh_spawning.rs` + `batching.rs`, and
  the stale `GlyphShadowMode` enum doc, as part of shrinking the enums.

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
- [ ] Gut the atlas setup in `TextPlugin::build` (`text/mod.rs`): the
      `AtlasConfig` read, the `GlyphAtlas::with_config` construction, the
      `AtlasSlot` / `AtlasPreference` inserts, and reduce
      `init_atlas_and_embedded_font` to just its `FontRegistered` work (drop
      the atlas upload). All reference Phase-2-deleted types. The slug setup
      added in Phase 1 (`register_slug_text_shader`, `SlugBackend`,
      `MaterialPlugin::<SlugTextMaterial>`) stays. *(Surfaced by Phase 1
      review — not a green-checkpoint until this lands.)*
- [ ] Delete `poll_atlas_glyphs` (`render/text_renderer/reconcile.rs`) plus
      its `use crate::text::AtlasSlot` — it pumps the MSDF atlas
      (`AtlasSlot`, `sync_to_gpu`, `poll_async_glyphs_stats`) that nothing
      reads under slug. `reconcile_panel_text_children` /
      `reconcile_panel_image_children` are renderer-agnostic and stay.
      *(Surfaced by Phase 1 review.)*
- [ ] Rewire `TextRenderPlugin` (`render/text_renderer/mod.rs`): removing
      `poll_atlas_glyphs` strands its four `.after(poll_atlas_glyphs)` anchors
      (`reconcile_panel_text_children`, `reconcile_panel_image_children`,
      `shape_panel_text_children`, `world_text::render_world_text`) as hard
      compile errors. Drop the registration + import and re-anchor the
      survivors (`.after(setup_panel_rtt)` /
      `.after(reconcile_panel_text_children)` / `.chain()`).
      *(Surfaced by Phase 1 review.)*
- [ ] Delete the now-dead DF mesh spawning in
      `render/world_text/mesh_spawning.rs`: `spawn_world_text_meshes` +
      `MeshSpawnAssets` (they import `GlyphMaterial` / `GlyphAtlas` /
      `glyph_quad` and fail to compile once those are deleted). The slug spawn
      path (`spawn_slug_world_text_meshes`, `SlugMeshSpawnAssets`,
      `WorldTextMesh`, `WorldTextShadowProxy`) stays. *(Surfaced by Phase 1
      review — these went dead when Phase 1 deleted their only caller.)*
- [ ] The public types these expose: `DistanceField`, `RasterBackend`,
      `RasterQuality`, `AtlasConfig`, `AtlasConfigError`, `GlyphWorkerThreads`,
      `GlyphAtlas`, `AtlasSlot`, `AtlasPreference`, `GlyphMaterial`,
      `GpuRasterizerPlugin`, `GpuEnqueueResult`, `GpuGlyphBudget`,
      `enqueue_gpu_glyph`, plus the atlas glyph-lookup types defined in
      `atlas.rs` (`GlyphKey`, `GlyphLookup`, `GlyphMetrics`, `GpuAtlasRegion`)
      — including their `lib.rs` and `text/mod.rs` re-exports. (Phase 1 already
      left the `text/mod.rs` `GlyphLookup` re-export as an unused-import
      warning; removing these clears it.)

**Phase 2 outcome — complete (2026-05-24). Verified green.** Every checklist
item plus the gap-fills below is implemented. Deleted: `atlas.rs`,
`atlas_config.rs`, `atlas_slot.rs`, `bitmap_dims.rs`, `gpu_rasterizer/`,
`msdf_rasterizer/`, `render/glyph_material.rs`, `render/glyph_quad.rs`, and
`shaders/glyph_text.wgsl`. `cargo build -p bevy_diegetic --lib` finishes with
**zero warnings**, `cargo clippy --lib` is clean, and `cargo nextest run --lib`
passes **158/158** (1 skipped). The full workspace `cargo build` (all crate
libs + the `fairy_dust` app bin) passes. Only the `bevy_diegetic` examples and
benches remain red — they call removed public APIs and are fixed in Phase 5.
(There is no `bevy_diegetic/tests/` directory; the red surface is examples +
benches only.) Not committed.

The **234 → 158** drop is not a regression: the deleted `atlas` /
`gpu_rasterizer` / `msdf_rasterizer` modules carried ~76 unit tests with them.
158 is the new green lib-test baseline (the Phase 1 note that "the full suite
returns to 234 at the end of Phase 2" was wrong — 234 was the pre-deletion
count).

### Retrospective

**What worked:**
- The enumerated deletions (atlas/rasterizer modules, `glyph_material`, the
  atlas-swap state machine, `poll_atlas_glyphs`, the four `.after` rewirings)
  landed mechanically; the Phase 1 review had already mapped them.
- `cargo +nightly fmt` clean; clippy clean; zero dead-code warnings after the
  constant/field trims.

**What deviated from the plan (gap-fills the checklist omitted):**
- **Deleted `render/glyph_quad.rs`** (+ its `mod glyph_quad`). It built atlas
  UV-quad meshes; its only consumers were the deleted DF mesh-spawn and
  `text_shaping::into_atlas_quad`, so it went fully dead. Not in the checklist.
- **Cleaned `render/text_shaping.rs`:** removed `GlyphQuadPlacement` +
  `into_atlas_quad`, the unused `glyph_key` fn, and the now-dangling
  `GlyphQuadData` / `GlyphKey` / `GlyphMetrics` imports (all named deleted atlas
  types). Compile-required; the file's parley core (`shape_text_cached`,
  `positioned_glyphs`, `PositionedGlyph`, `TextBuildStats`, `GlyphReadiness`,
  `TextShapingContext`) is shared with slug and stayed.
- **Removed the dead `ResolvedFontData.font_id` field** (`font_registry.rs`) —
  its only reader was the deleted `text_shaping::glyph_key`.
- **Trimmed dead constants** (style guide: delete dead code, don't warn):
  `text/constants.rs` reduced to the three surviving font constants;
  `render/constants.rs` dropped `GLYPH_QUAD_LINE_TOLERANCE` +
  `SHADOW_PROXY_ALPHA_MASK_THRESHOLD`; root `constants.rs` dropped
  `EMBEDDED_GLYPH_TEXT_SHADER_PATH`.
- **Renamed `init_atlas_and_embedded_font` → `register_embedded_font`** (single
  private call site) since it no longer touches an atlas — only fires
  `FontRegistered`.
- **Reworded stale MSDF/atlas doc comments beyond the enumerated three:** the
  `lib.rs` module + `DiegeticUiPlugin` docs (deleted the `AtlasConfig`
  "custom atlas configuration" example), `render/mod.rs` `RenderPlugin` doc,
  `text_renderer/mod.rs` `TextRenderPlugin` doc, `text/mod.rs` module doc,
  `font_registry.rs::register_font` doc, and `render/constants.rs`
  `RTT_LIGHT_ILLUMINANCE`.

**Surprises:**
- `glyph_quad.rs` and most of `text_shaping.rs`'s quad helpers were DF-only
  despite living in render-shared-looking modules. Only the parley shaping core
  is shared with slug.
- The `DiegeticPerfStats.atlas` sub-struct is now written by nothing
  (`poll_atlas_glyphs` was its only writer) but still compiles and is read by
  diagnostics — left in place; trimming it is out of Phase 2 scope.

**Implications for remaining phases:**
- Phase 3: `nalgebra` / `fdsm` / `fdsm-ttf-parser` / `msdfgen` / `ttf-parser_018`
  now have **zero** `bevy_diegetic` src users (every consumer was deleted) — the
  dep removals are pure cleanup, ready to land.
- Phase 5: examples/benches/integration-tests are the only remaining red
  targets. Any "expected test count" in Phase 6 must use **158**, not 234.
- Phase 7: `From<GlyphRenderMode> for u32` (+ assertions in `text_props.rs`) and
  `From<GlyphRenderMode> for SlugRenderMode` (`mesh_spawning.rs`) survived Phase
  2 unchanged; Phase 7 still owns collapsing them.

### Phase 2 Review

Architect re-evaluation of Phases 3–7 + Documentation disposition against the
shipped Phase 2. 11 findings; 10 folded into the plan, 1 surfaced to the user.

- **Phase 3** — confirmed safe to drop `nalgebra`/`fdsm`/`fdsm-ttf-parser`/
  `msdfgen`/`ttf-parser_018` workspace-wide: `bevy_lagrange` and `fairy_dust`
  use none of them; the workspace `nalgebra` line goes too (hedge resolved).
  `ttf-parser` 0.25 stays (slug uses it).
- **Phase 4** — `lib.rs` `Slug*` re-export count corrected `~28 → ~31`, adding
  `SlugBackendCompleted`, `SlugRunStorageProfile`, `slug_text_material`,
  `load_glyph_by_id_from_face`.
- **Phase 5** — deletion rows for `atlas_pages`, `preload_text`, and
  `glyph_rasterization` now also strip their `[[example]]`/`[[bench]]` entries
  from `bevy_diegetic/Cargo.toml` (a deleted `.rs` with a live manifest entry
  fails `cargo build`). The `glyph_rasterization` bench is un-gated, so its
  source deletion + manifest-entry removal + CI `cargo bench` edit must land
  together.
- **Build-green ordering** — corrected: the lib is green at the end of Phase 2
  (`--lib` = **158**, not 234); examples/benches stay red across Phases 2–4 and
  turn green only at the end of Phase 5. The earlier "full suite compiles at the
  end of Phase 2" claim was wrong.
- **Phase 6** — verification baseline pinned at **≈158** lib tests; the 234
  figure from the Phase 1 notes is explicitly not the target.
- **"Integration tests"** — phrasing dropped; there is no `bevy_diegetic/tests/`
  directory, so the red surface is examples + benches only.
- **Documentation disposition** — README flagged as a section rewrite (not a
  reword: the MSDF `fwidth` AA subsection); `scripts/parse_gpu_intervals.py`
  added to the artifact list; `slug-experiments.md` prep-cost reference flagged
  as needing a recorded figure or a slug micro-bench once the bench is deleted.
- **Phase 7** — reviewed clean: every named target (`is_shadow_proxy`,
  `slug_shadow_render_mode`, the proxy markers/materials, the `From` impls)
  still exists as described; no re-scope.
- **P1 (resolved — user approved):** the dead `AtlasPerfStats` /
  `DiegeticPerfStats.atlas` / 15 `DIAG_ATLAS_*` diagnostics surface is removed as
  a new end-of-Phase-3 "leftover DF perf-state cleanup" item — a public-API
  removal with zero consumers, consistent with decision #3.

### Phase 3 — Trim dependencies

Ordering: remove the Cargo.toml entries only **after** every Phase 2
file that imports `fdsm` / `fdsm-ttf-parser` is deleted, otherwise
`cargo check` fails on the still-present imports. Checkpoint first:
`cargo build && cargo nextest run` should pass at the end of Phase 2
(the unused deps are still declared but harmless), confirming Phase 2
is complete before this pure-cleanup phase.

- [ ] Workspace `Cargo.toml`: remove `nalgebra`, `fdsm`, `fdsm-ttf-parser`,
      dev-dep `msdfgen`, dev-dep `ttf-parser_018`. **Cross-workspace check done
      (Phase 2 review):** the only other members, `bevy_lagrange` and
      `fairy_dust`, neither declare nor use any of these; `bevy_kana` is an
      external crate, not a member. So `bevy_diegetic` is the sole user and the
      workspace `nalgebra` line goes too (the earlier "leave it unless
      bevy_diegetic was its only user" hedge resolves to "remove it").
- [ ] `bevy_diegetic/Cargo.toml`: remove `nalgebra`, `fdsm`, `fdsm-ttf-parser`,
      `msdfgen`, `ttf-parser_018`. After Phase 2, `nalgebra` / `fdsm` /
      `fdsm-ttf-parser` have **zero** `src` users (the rasterizers are deleted)
      and `msdfgen` / `ttf-parser_018` have zero references anywhere
      (src/examples/benches). **Keep `ttf-parser` 0.25** — slug uses it
      (`slug_text_spike/*`, `text/font.rs`).
- [ ] Confirm no remaining reference to the removed crates.
- [ ] **Leftover DF perf-state cleanup (Phase 2 review, P1).** Phase 2 deleted
      `poll_atlas_glyphs`, the only writer of the atlas perf surface, leaving it
      dead-but-published. Delete: the `AtlasPerfStats` struct + the
      `DiegeticPerfStats.atlas` field (`panel/perf.rs`); the 15 `DIAG_ATLAS_*`
      `DiagnosticPath` constants (`panel/constants.rs` lines ~38–66) and their
      entries in the `register_diagnostic` loop; the 15 `add_measurement` calls
      for them in `publish_perf_diagnostics` (`panel/perf.rs`); and the two
      `pub use ... AtlasPerfStats` re-exports (`lib.rs:137`, `panel/mod.rs:27`).
      `AtlasPerfStats` has zero consumers — a public-API removal consistent with
      decision #3 (expose nothing).

**Phase 3 outcome — complete (2026-05-24). Verified green.** All four checklist
items are implemented. The workspace and crate manifests dropped `nalgebra`,
`fdsm`, `fdsm-ttf-parser`, `msdfgen`, and `ttf-parser_018` (the workspace
`nalgebra` line and its `bevy_diegetic` use both gone). The leftover DF perf
surface is removed: the `AtlasPerfStats` struct, the `DiegeticPerfStats.atlas`
field, the 15 `DIAG_ATLAS_*` `DiagnosticPath` constants, their 15 register-loop
entries, the 15 `add_measurement` calls, and both `pub use … AtlasPerfStats`
re-exports (`lib.rs`, `panel/mod.rs`). `cargo build -p bevy_diegetic --lib`
finishes with **zero warnings**, `cargo clippy --lib` is clean,
`cargo nextest run --lib` passes **158/158** (1 skipped), and the full workspace
`cargo build` (libs + `fairy_dust` bin) passes. Examples and benches stay red
until Phase 5. Not committed.

### Retrospective

**What worked:**
- The dependency drops were a clean no-op on the green lib surface — `nalgebra`
  / `msdfgen` / `ttf-parser_018` had zero source users, so removal changed
  nothing but the manifests + `Cargo.lock`.
- The P1 perf removal was fully mechanical: the `.atlas` field's only readers
  were the 15 `add_measurement` calls deleted alongside it; no external consumer.

**What deviated from the plan:**
- The plan's "deps are unimported by then" was slightly inaccurate. `fdsm` /
  `fdsm-ttf-parser` are still referenced by `benches/glyph_rasterization.rs`
  (a `use fdsm_ttf_parser::…`, deleted in Phase 5) and a `typography.rs` doc
  comment. That bench was already red (it imports Phase-2-deleted `DistanceField`
  / `GlyphAtlas` / `GlyphKey`), so dropping the deps just adds one more error to
  an already-broken target. Plain `cargo build` (lib) does not compile benches,
  so the green surface is unaffected — consistent with the build-green ordering
  (benches red until Phase 5).
- P1 forced a doc-link fix not in the checklist: `PanelTextPerfStats::pending_glyphs`
  documented `[`AtlasPerfStats::in_flight_glyphs`]` and
  `[`AtlasPerfStats::peak_active_jobs`]` intra-doc links, which break when the
  struct is deleted. Reworded to drop the dangling references.

**Surprises:**
- `PanelTextPerfStats` still carries MSDF-era semantics that survived Phases 1–2.
  The `atlas_lookup_ms` field + its `DIAG_PANEL_TEXT_ATLAS_LOOKUP_MS` path
  (`bevy_diegetic/panel_text/atlas_lookup_ms`) name an "atlas lookup" stage that
  no longer exists under slug, and the struct docs reference the deleted
  `PanelTextQuads` and `build_panel_batched_meshes`. The fields are still
  *written* (slug's `text_renderer/shaping.rs` sets `atlas_lookup_ms` /
  `queued_glyphs` / `pending_glyphs` from the shaping aggregate), so this is live
  data with stale naming + docs, not dead code. Out of P1 scope — flagged for the
  Phase 3 review.

**Implications for remaining phases:**
- The stale `PanelTextPerfStats` naming/docs were flagged for the review;
  **resolved there — the `atlas_lookup_ms` field + its diagnostic were deleted
  outright** (see Phase 3 Review).
- `Cargo.lock` changed (deps pruned); Phase 6's `cargo build` sees the same
  pruned graph.

### Phase 3 Review

Architect re-evaluation of Phases 4–7 + Documentation disposition against the
shipped Phase 3. 10 findings: 8 folded into the plan, 1 confirmed clean, 1
surfaced to the user and resolved by deleting the field.

- **Stale `PanelTextPerfStats` surface (findings 1 + 9 — user-resolved, landed
  now).** The kept `PanelTextPerfStats` carried MSDF-era naming after Phase 3:
  the `atlas_lookup_ms` field + the `DIAG_PANEL_TEXT_ATLAS_LOOKUP_MS` diagnostic
  (`bevy_diegetic/panel_text/atlas_lookup_ms`), still written by slug as
  glyph-prep time. **Resolved across three rounds, all landed (lib green,
  158/158):** (1) deleted `atlas_lookup_ms` + its
  `DIAG_PANEL_TEXT_ATLAS_LOOKUP_MS` diagnostic, register entry, `add_measurement`,
  and the two writes in `text_renderer/shaping.rs`; (2) rewrote the stale doc
  comments — deleted `build_panel_batched_meshes` → `build_panel_slug_meshes`,
  and `PanelTextQuads` / atlas / async-rasterization wording → slug terms
  (positioned glyphs, slug meshes, glyph loading); (3) deleted `queued_glyphs` +
  `pending_glyphs` (+ their two diagnostics, register entries, `add_measurement`
  calls, and writes) — both are always 0 under slug (nothing increments them;
  slug builds glyphs synchronously), the same vestigial-MSDF class as
  `atlas_lookup_ms`. `PanelTextPerfStats` now keeps five live fields (`total_ms`,
  `shape_ms`, `parley_ms`, `mesh_build_ms`, `shaped_panels`); the shared internal
  `TextBuildStats` (incl. `atlas_ms`, `queued_glyphs`, `pending_glyphs`) is
  untouched — `GlyphReadiness` + the world-text path still read it.
- **Phase 4** — corrected the `Slug*` `lib.rs` re-export scope to **32 `pub use`
  lines, `lib.rs` 174–206** (was "~31, lines ~180–211"); recorded that
  `slug_text_shadow_proxy_material` is re-exported only from
  `slug_text_spike/mod.rs` (not `lib.rs`) and `register_slug_text_shader` is
  already `pub(crate)`; named `slug_text_spike/constants.rs` (→ `text/slug/`) as
  the home of `SLUG_TEXT_SHADER_PATH`.
- **Phase 5** — added a manifest-listing note (`text_renderer_gpu_bench`,
  `world_text`, `sdf` are auto-discovered with no `[[example]]` row, so only the
  manifest-listed deletions need entry removal); flagged that the
  `glyph_rasterization` bench also imports `Slug*` types Phase 4 makes
  `pub(crate)`; flagged the stale `fdsm` doc comment at `typography.rs:819`.
- **Phase 7** — pinned the single `debug/` shadow-mode edit:
  `label_shadow_mode()` `GlyphShadowMode::Text → Cast` at
  `debug/typography_overlay/mod.rs:108` (~11 `with_shadow_mode` sites); confirmed
  no `Invisible` / `SolidQuad` render-mode usage exists in `debug/`.
- **Reviewed clean (finding 10):** Phase 4 module-move targets, Phase 7
  shadow-proxy targets (`is_shadow_proxy`, `slug_text_shadow_proxy_material`, the
  dual proxy spawn paths), and the `glyph_rasterization` un-gated + `ci.yml:202`
  facts all still match the plan — no re-scope.

### Phase 4 — Move slug into `text/`

- [ ] Relocate `slug_text_spike/*.rs` to `text/slug/`.
- [ ] Update `crate::slug_text_spike::X` references to `crate::text::slug::X`.
- [ ] Delete the now-empty `slug_text_spike/` directory.
- [ ] Fold the Phase 1 `register_slug_text_shader` helper into
      `TextPlugin::build`. Once `slug_text.wgsl` lives under `text/slug/`,
      `embedded_asset!` can be invoked directly from `text/mod.rs` with the
      relocated path, so: inline the `embedded_asset!(app, …)` call into
      `TextPlugin::build` next to the `SlugBackend` / `MaterialPlugin` setup,
      delete `register_slug_text_shader` and its `pub(crate)` re-export, and
      update `SLUG_TEXT_SHADER_PATH` (defined in `slug_text_spike/constants.rs`,
      which itself moves to `text/slug/constants.rs` in this phase) to the new
      embedded path. (Helper added in Phase 1 because the macro resolves paths
      relative to the calling file and the shader had not moved yet.) *(Phase 3
      review confirmed `register_slug_text_shader` is already `pub(crate)`.)*
- [ ] Make every `Slug*` type `pub(crate)` and drop all `Slug*`
      re-exports from `lib.rs` (expose-nothing — see Open decisions #3).
      The public text API is the existing agnostic surface only.
      Scope note (Phase 1 review; count corrected Phase 3 review to **32
      `pub use` lines, `lib.rs` lines 174–206**, also covering
      `SlugBackendCompleted`, `SlugRunStorageProfile`, `slug_text_material`,
      `load_glyph_by_id_from_face`; note `slug_text_shadow_proxy_material` is
      re-exported only from `slug_text_spike/mod.rs`, NOT from `lib.rs` — only
      `slug_text_material` is, at `lib.rs:206`):
      this is the `Slug*` block in `lib.rs`
      (`SlugBandRecord`, `SlugBounds`, `SlugBuiltTextRun`,
      `SlugCurveRecord`, `SlugFontKey`, `SlugGlyph`, `SlugGlyphCache`,
      `SlugGlyphInstance`, `SlugGlyphKey`, `SlugGlyphRecord`, `SlugOutlineError`,
      `SlugPackedGlyph`, `SlugRunRenderData`/`Error`/`Profile`, `SlugTextRequest`,
      `SlugTextRun`, `build_packed_glyph`, `build_slug_run_render_data`,
      `build_slug_text_run`, `load_glyph`, `FIXTURE_TEXT`, `DEFAULT_BAND_COUNT`,
      …), not just the 7 `render/` consumers — plus the matching `pub use`
      lines in `slug_text_spike/mod.rs`. Only the 7 types `render/` consumes
      (`SlugBackend`, `SlugPreparedTextRun`, `SlugRunStorage`,
      `SlugRunStorageKey`, `SlugTextMaterial`, `SlugTextMaterialInput`,
      `SlugRenderMode`) need `pub(crate)`; the rest become module-private
      (`pub(in crate::text::slug)`) or, where they exist only to back the
      Phase-5-deleted spike instrumentation (`SlugPackedGlyph`,
      `build_packed_glyph` — used only by `slug_text.rs`'s `log_*` probes),
      are candidates for outright deletion. Decide delete-vs-private per type
      during the move; expose-nothing (#3) means none stay public.
- [ ] Update doc comments that still say "Experimental" /  "spike"
      (e.g. `SlugBackend`) to reflect production status.

**Phase 4 outcome — complete (2026-05-24). Verified green.** All checklist items
plus the deletions below are implemented. `slug_text_spike/*.rs` moved to
`text/slug/` (via `git mv`); every `crate::slug_text_spike::X` reference rewritten
to `crate::text::slug::X`; `text/mod.rs` declares `pub(crate) mod slug` and inlines
`embedded_asset!(app, "slug/shaders/slug_text.wgsl")` into `TextPlugin::build`
(the `register_slug_text_shader` helper + its re-export deleted); `SLUG_TEXT_SHADER_PATH`
updated to `embedded://bevy_diegetic/text/slug/shaders/slug_text.wgsl`. Expose-nothing
done: all 32 `Slug*` `lib.rs` re-exports dropped, the slug `mod.rs` re-exports the 11
cross-module symbols as `pub(crate)` (`SlugBackend`, `SlugPreparedTextRun`,
`SlugRunStorage`, `SlugRunStorageKey`, `SlugRenderMode`, `SlugTextMaterial`,
`SlugTextMaterialInput`, `slug_text_material`, `slug_text_shadow_proxy_material`,
`DEFAULT_BAND_COUNT`), everything else module-private or deleted. `cargo build -p
bevy_diegetic --lib` finishes with **zero warnings**, `cargo clippy --lib` is clean,
`cargo nextest run --lib` passes **158/158** (1 skipped), and the full workspace
`cargo build` (libs + `fairy_dust` bin) passes. Examples and benches stay red until
Phase 5. Not committed.

The user directed **one path** mid-phase ("just have one path — the production path —
be consistent"). Slug's standalone string-shaping path — `prepare_text_run`,
`SlugTextRequest`, `build_slug_text_run`/`_with_cache`, `shape_slug_text`, the
`VisibleSlugGlyph`/`ShapedSlugGlyph`/`ShapedSlugText` structs, the parallel cache fills
(`insert_missing_packed_parallel`, `get_or_insert_packed`), and `geometry::load_glyph`/
`load_glyph_by_id` — was used by nothing in production (the render path feeds parley
`PositionedGlyph`s through `prepare_positioned_run_with_scale`); it survived only as the
slug unit-test harness. All 8 slug unit tests were re-pointed onto the production path
via a new `#[cfg(test)] text/slug/test_support.rs` (builds `ShapedGlyph` +
`ResolvedFontData` from font bytes — no parley needed), preserving the CJK CFF-cubic
coverage and the dedup/clip checks. The standalone path was then deleted.

### Retrospective

**What worked:**
- The move was mechanical: `git mv` + one `perl` path rewrite + a single
  `embedded_asset!` relocation. `embedded_asset!` resolves relative to the calling
  file, so calling it from `text/mod.rs` with `"slug/shaders/slug_text.wgsl"` yields the
  correct `embedded://bevy_diegetic/text/slug/shaders/...` path with no macro tricks.
- Compiler-driven dead-code sweep: each `cargo build --lib` warning named exactly the
  next thing to delete.

**What deviated from the plan:**
- The plan scoped Phase 4 as a move + visibility change. It did not foresee that
  expose-nothing would reveal a **large dead surface** once `lib.rs` stopped re-exporting
  it. Beyond the standalone-shaping path (deleted per the user's "one path" call), the
  following were dead-in-production and removed: the never-triggered `SlugBackendCompleted`
  completion event + its render observer `mark_text_pending_on_slug_completed` (+
  `mark_all_text_pending`) in `text_renderer/mod.rs`; the write-only backend counters
  (`completed_runs`, `failed_runs`, `generation`, `last_completion`) and `record_failure`
  (plus its two call sites in `world_text/shaping.rs` and `text_renderer/shaping.rs`);
  unused introspection accessors (`SlugRunStorageKey::value`, `SlugGlyph::contour_count`/
  `curve_bytes`/`segment_count`, `SlugPackedGlyph::glyph`/`outline_segments`/
  `duplicated_curves`/`curve_bytes`/`band_bytes`, `SlugGlyphKey::new`,
  `SlugGlyphInstance::new`/`new_scaled`/`advance`, `SlugBackend::glyph_cache`/
  `stored_runs`/`preprocess_version`, `SlugGlyphCache::len`/`is_empty`); unused
  run-metadata fields (`SlugTextRun::bounds`+`advance_width` and the `run_bounds`/
  `shifted_bounds`/`merge_bounds` helpers, `SlugGlyphInstance::advance`,
  `SlugBuiltTextRun::baseline`/`reference_size`); dead `SlugOutlineError` variants
  (`MissingGlyph`, `MissingOutline`, `CubicOutline`); and `FIXTURE_TEXT`.
- The orphan `text/slug/mesh.rs` (never declared in any `mod`, dead on disk since the
  spike) was deleted.
- The `Slug*` re-export count was **32**, matching the Phase 3 review's correction.

**Surprises:**
- `SlugBackendCompleted` was observed (`On<SlugBackendCompleted>`) but **never triggered**
  — a dormant async-completion hook. Slug builds synchronously, so the
  mark-pending-on-completion mechanism is obsolete; removing it (event + observer +
  registration) is correct, not just cleanup.
- Slug's outline loader had three error variants (`MissingGlyph`, `MissingOutline`,
  `CubicOutline`) that the production path never constructs — cubics are converted to
  quadratics, not rejected, so `CubicOutline` was always vestigial.

**Implications for remaining phases:**
- Phase 5: `slug_text.rs`'s spike instrumentation (`log_glyph_metrics`, `log_cjk_probe`,
  `SlugPackedGlyph`, `build_packed_glyph`) now references types that are `pub(crate)` or
  deleted — that example must drop those probes (already in the Phase 5 row). The
  `glyph_rasterization` bench's `Slug*` imports (`SlugBackend`, `SlugFontKey`,
  `SlugTextRequest`, `build_slug_run_render_data`, `DEFAULT_BAND_COUNT`) now name deleted
  or private items, but the bench is deleted in Phase 5 regardless.
- Phase 7: the slug shadow-proxy collapse is unaffected — `slug_text_shadow_proxy_material`,
  `SlugRenderMode`, and the proxy spawn paths all survived as `pub(crate)`.

### Phase 4 Review

Architect re-evaluation of Phases 5–7 + Documentation disposition against the shipped
Phase 4. 15 findings; all folded into the plan (none rejected, none required user
sign-off — the three `significant` ones were doc-maintenance refinements of
already-decided items, applied directly).

- **Phase 5 preamble** — deleted the obsolete "fold the example fixes into the Phase 1–3
  sequence" framing; Phase 5 is now stated as the single standalone post-Phase-4 pass
  that turns the red example/bench surface green.
- **Phase 5 `slug_text.rs` row** — upgraded from "trim spike probes" to **near-total
  rewrite**: it imports a wall of Phase-1/Phase-4-deleted symbols
  (`TextRenderer`/`TextRendererPreference`, `SlugBackendCompleted`, `SlugTextRequest`,
  `prepare_text_run`, `SlugOutlineError::CubicOutline`, `FIXTURE_TEXT`, `build_packed_glyph`,
  …) with no public API to port the backend-poking body to; CJK demo kept as `WorldText`.
- **Phase 5 `text_renderer_gpu_bench.rs` row** — upgraded to **structural rewrite**: the
  whole `BenchMode` enum + `AtlasConfig`/`with_renderer` machinery collapses to one slug
  path (auto-discovered, no manifest edit).
- **Phase 5 `world_text.rs` / `panel_rendering.rs` row** — corrected to a one-line strip
  each (single `TextRendererPreference::slug()` insert; no toggle UI exists).
- **Phase 5 `glyph_rasterization.rs` row** — noted it also calls the Phase-4-deleted
  `backend.glyph_cache()` + non-clip `build_slug_run_render_data`; irrecoverably red,
  deletion confirmed as the only path.
- **Phase 6** — added an explicit `cargo build --examples --benches` gate (plain
  `cargo build` masks the example/bench surface).
- **Phase 7** — corrected stale `slug_text_spike/` paths to `text/slug/`; pinned the
  shadow-proxy re-export to `text/slug/mod.rs:26` (`pub(crate)`, the only surfacing point
  post-Phase-4); added rewording of the slug-module `is_shadow_proxy`/`SlugRenderMode`
  doc comments; recorded the `From<GlyphRenderMode> for SlugRenderMode` seam at
  `mesh_spawning.rs:237` + `slug_render_mode` at `batching.rs:398` must change in lockstep
  with the enum shrink; flagged `shadows.rs` as the enum-shrink canary.
- **Documentation** — `slug.md` upgraded to "status reword **+** audit/rewrite the deleted
  standalone-shaping API references"; `slug-experiments.md` / `slug-benchmark-procedure.md`
  now prefer **recording** the ≈0.84 ms prep figure (the prep API the old bench called was
  deleted, so no drop-in micro-bench survives).

### Phase 5 — Examples and benches

Build-green caveat: examples are compiled by `cargo build --examples`
and the Phase 6 suite run. `typography.rs` and `text_renderer_gpu_bench.rs`
import types deleted in Phase 2
(`DistanceField` / `AtlasConfig` / `RasterBackend` / `RasterQuality` /
`GlyphAtlas`); `typography.rs` and `font_features.rs` also import
`GlyphLoadingPolicy`, deleted in Phase 1; `preload_text.rs` is built on
the deleted atlas preload API and is itself deleted in this sequence;
`slug_text.rs` pokes `Slug*` internals deleted/privatized in Phase 4. **As actually
shipped (Phase 4 review): Phases 1–4 touched no example, so `cargo build --examples`
and `cargo bench --benches` are red continuously from the end of Phase 1 through the
end of Phase 4.** The earlier "fold the example fixes into the Phase 1–3 sequence"
framing is dead — Phase 5 is the single standalone pass that turns the entire red
example/bench surface green, after Phase 4. The rows below are the authoritative
per-example dispositions, all executed here.

**Manifest-listing note (Phase 3 review):** not every example has a `[[example]]`
row in `bevy_diegetic/Cargo.toml` — `text_renderer_gpu_bench`, `world_text`, and
`sdf` are auto-discovered. The "delete the `.rs` and its manifest entry" rule
below applies only to the manifest-listed deletions (`atlas_pages`,
`preload_text`, and the `glyph_rasterization` bench); converting an
auto-discovered example (e.g. `text_renderer_gpu_bench`) needs no manifest edit.
The inverse risk: an auto-discovered example importing a deleted type breaks
`cargo build --examples` with no manifest row to gate it — so every such file
must be edited, not just the listed ones.

| Target | Action |
| --- | --- |
| `slug_text.rs` | **Near-total rewrite, not a probe trim (Phase 4 review).** The file imports a wall of now-deleted/privatized symbols — `TextRenderer`/`TextRendererPreference` (Phase 1), `SlugBackendCompleted`/`SlugTextRequest`/`prepare_text_run`/`SlugOutlineError::CubicOutline`/`FIXTURE_TEXT`/`build_packed_glyph`/`SlugPackedGlyph`/`SlugBuiltTextRun`/`SlugFontKey` (Phase 4) — and there is **no surviving public API to port the `SlugBackend`-poking body to** (`load_preview_text`, `log_*` probes, the `ActiveTextRenderer` toggle state/UI all go). Rewrite as a plain production example that spawns `WorldText`. **Retain the CJK demonstration**, but as on-screen `WorldText` rendering (parallel Latin + CJK), not the `build_packed_glyph` probe. Keep as a production text example. |
| `world_text.rs`, `panel_rendering.rs` | **One-line strip each (Phase 4 review):** neither has toggle UI/state — each only imports `TextRendererPreference` and calls `.insert_resource(TextRendererPreference::slug())` once. Delete that import + line. Keep. |
| `typography.rs` | Delete the renderer-toggle entirely: the `switch_text_mode` system + its registration, `TypographyTextMode`, `toggle_backend`, `pick_raster_quality`, and the direct `DistanceField` / `RasterBackend` / `RasterQuality` / `AtlasPreference` / `TextRendererPreference` usage. Also remove the single `.with_loading_policy(GlyphLoadingPolicy::Progressive)` call and the `GlyphLoadingPolicy` import (the policy is deleted in Phase 1). **Phase 3 review:** also drop the stale doc comment at `typography.rs:819` ("`G` flips the rasterizer backend between CPU (`fdsm`) and GPU") — its subject is the toggle being deleted, and `fdsm` was removed in Phase 3. Keep behind `typography_overlay`. |
| `font_features.rs` | Remove all `GlyphLoadingPolicy` usage: the `loading_policy` field threaded through its two helper structs, the `progressive` binding, every `.with_loading_policy(...)` call, and the import. The OpenType-feature demo itself is unaffected — the policy is a no-op under slug. Keep. |
| `shadows.rs` | Keep — uses the agnostic `GlyphRenderMode` / `GlyphShadowMode` API, renders via slug unchanged (the Phase 0 slug throwaway is reverted at the end of Phase 0). Deleted in **Phase 7** when the matrix collapses. **Phase 4 review — canary:** this is the only example exercising all four render modes + all four shadow modes (incl. `Invisible`/`SolidQuad`/`GlyphShadowMode::{SolidQuad,Text,PunchOut}`); it must compile clean at the end of Phase 5 and is then deleted in Phase 7 — if Phase 7 slips, `shadows.rs` breaks the moment those variants are removed. |
| `text_renderer_gpu_bench.rs` | **Structural rewrite, not a reduction (Phase 4 review):** the whole `BenchMode { Empty, Msdf, Mtsdf, Sdf, Slug }` enum with its `text_renderer()` / `distance_field()` methods, the `AtlasConfig::new()…with_backend(Gpu)` setup, the CLI mode parsing, and `.with_renderer(...)` are woven through setup and all reference deleted DF types (`AtlasConfig`/`AtlasPreference`/`DistanceField`/`RasterBackend`/`RasterQuality`/`TextRenderer`/`TextRendererPreference`). Collapse to a single unconditional slug path. Auto-discovered — no manifest edit. Keep as a slug regression/optimization harness (note in `slug-benchmark-procedure.md` that cross-renderer comparison is gone). |
| `atlas_pages.rs` | Delete (visualizes atlas pages; no slug analog). **Also remove its `[[example]] name = "atlas_pages"` entry from `bevy_diegetic/Cargo.toml`** — deleting the `.rs` while leaving the manifest entry makes `cargo build` fail with "can't find target". |
| `preload_text.rs` | **Delete.** Built on the distance-field atlas preload API (`GlyphAtlas::preload`) and `GlyphLoadingPolicy`, both removed in the Phase 1–2 sequence. Slug needs no preload demo: per-glyph band-building is sub-millisecond — full printable ASCII preps in ≈ 0.84 ms (after per-curve dedup + 48-band tuning), well below one frame and below frame-timing resolution. There is no warm-up cost worth showcasing and no preload API is shipped. A project with very large glyph sets that notices first-frame lag can warm glyphs with its own Bevy task / async setup — no engine API required. **Also remove its `[[example]] name = "preload_text"` entry from `bevy_diegetic/Cargo.toml`.** |
| `benches/glyph_rasterization.rs` | Delete (CPU/GPU MSDF rasterizer bench; no slug analog). **Also remove its `[[bench]] name = "glyph_rasterization"` entry from `bevy_diegetic/Cargo.toml`.** This bench has no `required-features`, so `cargo bench --benches` (CI, `ci.yml:202`) compiles it and it imports deleted `DistanceField` / `GlyphAtlas` / `GlyphKey` — it is **red from the end of Phase 2 until this deletion lands**, so the source deletion, the manifest-entry removal, and the CI `cargo bench` edit must land together. **Phase 3 review:** this bench *also* imports `Slug*` types (`SlugBackend`, `SlugFontKey`, `SlugTextRequest`, `build_slug_run_render_data`, `DEFAULT_BAND_COUNT`) that Phase 4 makes `pub(crate)`; since the bench is already red, Phase 4's cut only adds errors to a dead target — but its deletion must not be deferred past Phase 4 for anyone running `cargo bench`. **Phase 4 review:** the bench also calls `backend.glyph_cache()` and the non-clip `build_slug_run_render_data` — both deleted in Phase 4 — so it is irrecoverably red; deletion is the only path. |
| `examples/sdf.rs` | Untouched (panel SDF). |

**Phase 5 outcome — complete (2026-05-24). Verified green.** Every table row plus
the coupled non-`docs/` artifacts is executed. One-line strips: `world_text.rs`,
`panel_rendering.rs` (dropped the `TextRendererPreference` import + `slug()` insert).
`font_features.rs`: removed all `GlyphLoadingPolicy` usage (import, the `progressive`
binding, the `loading_policy` param threaded through `build_feature_grid` /
`build_feature_column`, and five `.with_loading_policy(...)` calls). `typography.rs`:
deleted the renderer-toggle (the `TypographyTextMode` enum, `switch_text_mode` /
`toggle_backend` / `pick_raster_quality` / `refresh_quality_panel`, the M/S/X/L/G
chips, the `AtlasPreference` / `RasterQuality` / `TextRendererPreference` inserts) **and
the whole MSDF quality panel** (`QualityPanel`, `build_quality_panel` /
`build_quality_keys_column` / `build_quality_labels_column` / `quality_label`,
`QUALITY_KEYS`, the two `QUALITY_PANEL_*` constants) — all of it named the deleted
`RasterQuality` / `AtlasPreference`; kept behind `typography_overlay`, overlay + font
switch + word cycle intact. `slug_text.rs`: near-total rewrite to a production
`WorldText` example — Latin headline + a small line (small-size crispness) + a CJK row
spawned from an `on_font_registered` observer once `Noto Sans CJK SC` resolves (no
`SlugBackend` poking, no probes). `text_renderer_gpu_bench.rs`: collapsed
`BenchMode { Empty, Msdf, Mtsdf, Sdf, Slug }` → `{ Empty, Slug }`, dropped
`text_renderer()` / `distance_field()` and the `AtlasConfig` / `AtlasPreference` /
`TextRendererPreference` setup; `empty` survives as the no-text baseline.
`atlas_pages.rs`, `preload_text.rs`, `benches/glyph_rasterization.rs` deleted with
their three `Cargo.toml` manifest entries. `shadows.rs` / `examples/sdf.rs` untouched.
CI: `ci.yml:202` runs `cargo bench --benches` (a glob, no named target) — deleting the
bench source + manifest entry is sufficient, **no CI edit needed**. Scripts:
`xctrace_text_renderer.sh` mode list reduced to `empty`/`slug`;
`parse_gpu_intervals.py` filters on the kept `text_renderer_gpu_bench` process name, so
it needed no change. `cargo build --workspace`, `cargo build --examples --benches
--features typography_overlay,bench_support`, `cargo clippy --all-targets` (clean),
`cargo nextest run --lib` **158/158** (1 skipped), `cargo +nightly fmt` all green. Not
committed.

### Retrospective

**What worked:**
- The grep for deleted-type references pinned the surface to exactly the nine table
  rows up front; each file was then a mechanical strip or delete with no surprises.
- The `text_renderer_gpu_bench` collapse kept the `Empty` baseline, so the harness
  still measures slug glyph cost vs. an empty scene — the cross-renderer comparison was
  the only thing lost, matching `slug-benchmark-procedure.md`'s scope.

**What deviated from the plan:**
- `typography.rs` was larger than "delete the renderer toggle": the **entire MSDF
  quality panel** (`build_quality_panel` + `build_quality_keys_column` +
  `build_quality_labels_column` + `quality_label` + `QUALITY_KEYS` + `QualityPanel` +
  two constants) had to go too, because every one named the deleted `RasterQuality` /
  `AtlasPreference`. Removing the second HUD panel also turned the fonts panel's last
  `unlit.clone()` into a `redundant_clone` (clippy nursery) — fixed by moving `unlit`.
- The CI `cargo bench` line needed **no edit** — it is a `--benches` glob, not a named
  target, so the source + manifest deletion fully removes `glyph_rasterization` from CI.
  The plan's "the CI `cargo bench` edit must land together" framing assumed a named
  reference that does not exist.

**Surprises:**
- Three latent clippy lints lived in `src/text/slug/run_render.rs` **test** code from
  Phase 4 (`redundant_closure_for_method_calls` ×2, `manual_midpoint`). Phase 4 verified
  with `cargo clippy --lib`, which does **not** compile `#[cfg(test)]` code, so they
  were never checked; `cargo nextest run` compiled them fine (rustc does not enforce
  pedantic). Surfaced here by `cargo clippy --all-targets` and fixed in place — a clean
  tree now needs `--all-targets`, not `--lib`, for clippy.
- `slug.md` does **not** reference the deleted standalone-shaping APIs
  (`SlugTextRequest` / `prepare_text_run` / `build_slug_text_run` / `load_glyph` / the
  dropped `SlugOutlineError` variants) — a crate-wide grep found zero hits. The Phase 4
  review's suspicion ("a design doc this long almost certainly documents them") was
  wrong; `slug.md`'s only staleness is the "experimental alternative" framing.

**Implications for remaining phases:**
- **Documentation disposition is now the largest unexecuted block and has no owning
  phase.** Phase 5 executed only the doc items coupled to its own deletions (the two
  stale `glyph_rasterization` bench references in `slug-benchmark-procedure.md` /
  `slug-experiments.md`, plus the obsolete `slug_text_spike` spike-only rule). Still
  unexecuted across the whole migration: **delete `gpu_rasterizer.md`** (1539 lines,
  documents only the removed engine — dead since Phase 2), the **README MSDF
  anti-aliasing section rewrite** (Phase 2 review flagged ~lines 13–116), and the
  **`slug.md` status reword**. None of Phases 0–5 executed any `docs/` deletion or
  rewrite — every review only *refined the disposition*. Phase 6 is "Verify" and Phase 7
  is the matrix collapse; neither currently owns this doc backlog.
- Phase 6's `cargo clippy` gate must be `--all-targets` (or `--all-targets --features
  typography_overlay,bench_support`), not `--lib`, or it will miss test/example/bench
  clippy debt like the three lints found here.
- Phase 7 deletes `examples/shadows.rs`; Phase 5 confirmed it compiles clean on slug
  today, so the only Phase-7 risk for it remains the enum-variant removal timing already
  noted.

### Phase 5 Review

Architect re-evaluation of Phases 6–7 + Documentation disposition against shipped
Phase 5. 10 findings; 9 folded into the plan, 1 surfaced to the user (approved).

- **Phase 6** gained an explicit `cargo clippy --all-targets --features
  typography_overlay,bench_support` gate — Phase 4's `clippy --lib` skipped test/
  example/bench code, hiding the three `run_render.rs` test-code lints Phase 5 fixed;
  and its final step was re-scoped from "screenshot to confirm parity" to a per-example
  smoke check (no DF path left to A/B against).
- **Phase 7 — `examples/sdf.rs:529`** added to the `with_shadow_mode` migration list:
  it calls `.with_shadow_mode(GlyphShadowMode::Text)` and is otherwise "untouched," so
  it breaks when the `Text` variant is removed (only the one text call site moves; the
  panel-SDF body stays out of scope).
- **Phase 7 — `GlyphShadowMode` default** called out: the enum's `#[default]` is `Text`
  (`text_props.rs:141`), so the `{ None, Cast }` collapse must put `#[default]` on `Cast`
  or all shadow-mode-unset text (most of `typography.rs` / `slug_text.rs` / `sdf.rs`)
  silently stops casting.
- **Phase 7 line-number corrections:** the `GlyphRenderMode` discriminant assertions are
  at `text_props.rs` ~838–841 (not the Phase-2 checklist's "~901–904"); the `material.rs`
  doc ranges are ~17–30 / ~41–51 (not ~20–31 / ~43–51). The WGSL coverage-discard
  evidence (`slug_text.wgsl` ~348 prepass, ~366 main-pass) was cited so the proxy's
  redundancy can be verified before deletion. Remaining Phase-7 targets (the proxy spawn
  paths, `From<GlyphRenderMode>`, helpers, the `typography_overlay` `Text→Cast` edit) all
  confirmed live post-Phase-4-move.
- **`slug.md` API-audit clause struck:** the Phase 4 review assumed `slug.md` documents
  the deleted standalone-shaping API; a crate-wide grep found zero references, so the
  file's only work is the status reword.
- **`slug-experiments.md` / `slug-benchmark-procedure.md` / `xctrace_text_renderer.sh` /
  `parse_gpu_intervals.py` / `ci.yml`** marked done-or-no-change-needed (executed in
  Phase 5).
- **New Phase 8 — Documentation (user-approved):** the Documentation disposition
  (`gpu_rasterizer.md` deletion, README MSDF-AA rewrite, `slug.md` status reword) had no
  owning phase — flagged since Phase 2 but never executed. Added as a dedicated final
  doc-only phase after Phase 7, with the `slug.md` reword allowed to fold into Phase 7's
  existing `slug.md` edits.

### Phase 6 — Verify — complete (2026-05-25)

- [x] `cargo build`
- [x] `cargo build --examples --benches` (Phase 4 review) — plain `cargo build`
      does **not** compile examples/benches, so assert this explicitly; it is the
      only red surface across Phases 1–5 and turns green only at the end of Phase 5.
- [x] `cargo nextest run` — expected baseline is **≈158 lib tests** (not 234;
      Phase 2 deleted the atlas/rasterizer modules and their ~76 tests), plus
      whatever example/bench targets compile after Phase 5. Do not treat the
      234 figure from the Phase 1 notes as the target. **Result: 252 workspace
      tests pass / 1 skipped; bevy_diegetic lib = 158.**
- [x] `cargo clippy --all-targets --features typography_overlay,bench_support`
      (Phase 5 review) — **`--all-targets`, not `--lib`**. Phase 4 verified with
      `cargo clippy --lib`, which does not compile `#[cfg(test)]` / example / bench
      code, so it missed three pedantic lints that lived in `run_render.rs` test code
      (caught + fixed in Phase 5 only because that pass ran `--all-targets`). The lib
      build and `cargo nextest` do not enforce pedantic, so clippy is the only gate
      that sees this class of debt.
- [x] `cargo +nightly fmt`
- [x] Run the example suite; screenshot each example to confirm it renders correctly
      (Phase 5 review) — a per-example smoke check, **not** an MSDF parity comparison.
      Phases 1–3 deleted the distance-field path, so there is no second renderer left
      to A/B against; the only reference is "does slug render this example correctly."

### Retrospective

**What worked:**
- All five static gates green on the slug-only tree.
- The per-example smoke check earned its place: it found three defects the static
  gates cannot see (a deterministic crash, a z-fight wash-out, a dead effect).

**What deviated from the plan:**
- Phase 6 was scoped as pure verification but drove three substantial changes. Two
  became new phases — the `text_alpha` crash → **Phase 9** (with the `type Exclude`
  small fix landed here), the `units` z-fight → **Phase 10** (full OIT/transparency
  stack removed, complete). The third — dead `HueOffset` — was removed this session
  (component + `hue_offset` example deleted, `text_stress` reworked to a static
  rainbow), resolving the `slug.md` Phase 12 `HueOffset` open decision to "removed".

**Surprises:**
- `HueOffset` was already a no-op before this session: `batching.rs` queried it as
  `_hue_offset` and ignored it; no shader applied hue. The MSDF-era
  `sync_panel_hue_offset` system was deleted in the migration without a slug
  equivalent. Validated against `slug_fx.md`: hue rotation transforms one per-run
  `fill_color`, so it is a CPU color-input animation, not a per-pixel shader effect.
- Two examples carry no `BrpExtrasPlugin` (`two_window_panels`,
  `text_renderer_gpu_bench`), so they cannot be BRP-screenshotted; verified by
  launch-without-panic + slug code-path equivalence.

**Implications for remaining phases:**
- Phase 10 (OIT) is done; `depth_bias` now orders coplanar text directly — relevant
  to Phase 7's cast-shadow toggle and the shadow-only recipe (no OIT/`Msaa::Off`
  constraint anymore).
- The `HueOffset` removal already edited `batching.rs` (dropped the
  `Option<&HueOffset>` query field). Phase 7 also edits `batching.rs` (proxy spawn
  paths) — it should not reintroduce `HueOffset` language. (`slug.md`, which carried
  the stale `HueOffset`-as-future framing, is deleted in Phase 8.)
- `text_alpha.rs` and `units.rs` were reworked in Phase 10; Phase 7's references to
  example shadow/alpha call sites should be checked against the current files.

### Phase 6 Review

- **Phase 8 `slug.md` (user decision)** — `slug.md` deleted as obsolete rather than
  reworded. It documented only deleted/superseded concepts (`TextRendererPreference`
  switching, the four-mode matrix, HueOffset-as-future) with no salvageable
  unimplemented work; effects live in `slug_fx.md`, migration here. Phase 8 "reword"
  item → "deleted (done)"; the Documentation-disposition row updated; Phase 7's
  shadow-only-recipe doc target retargeted from `slug.md` to `slug_fx.md` §8.3.
- **Phase 8 README** — scope shrank to README lines 13 and 17; Phase 10 already
  rewrote the transparency section (~34–55) and TAA/AA wording (~80–98), so the
  earlier "~36–116 subsection rewrite" estimate no longer applies.
- **Phase 7 line refs** — refreshed against current code: prepass coverage-discard
  `slug_text.wgsl:340`, main-pass `is_shadow_proxy` discard `:358`, `slug_render_mode`
  `batching.rs:397`.
- **Phase 7 `GlyphShadowMode` collapse** — added the two explicit constructor defaults
  (`shadow_mode: GlyphShadowMode::Text` at `text_props.rs:560` and `:664`) to the
  migration task; noted `GlyphShadowMode::None` survives the `{ None, Cast }` collapse,
  so the `::None` example sites (`font_loading`, `text_stress`, `gpu_bench`) need no
  edit and `sdf.rs:529` is the only `::Text` site.
- **Phase 9 `Exclude` removal** — added the test-impl `type Exclude = ExcludeNone;`
  sites (`cascade/target.rs:127, 244`), the `use` at `target.rs:111`, and the doc text
  to the removal task.
- Findings on Phase 7's shadow-proxy targets, discriminant-assertion refs, the
  `batching.rs` HueOffset/proxy region overlap, the Phase 9 three-topology premise, and
  the `gpu_rasterizer.md` deletion were reviewed and confirmed accurate — no change.

### Phase 7 — Collapse the glyph render/shadow matrix

Rationale and design: `slug_fx.md` §8. With MSDF gone, slug's
`GlyphRenderMode × GlyphShadowMode` matrix and its shadow-proxy mesh
are the last inherited complexity. Decisions: drop mismatched-silhouette
shadows (the sole reason the proxy exists) and `SolidQuad`; make
shadow-only text a documented recipe; keep `PunchOut` as a fill effect.
slug's visible mesh already casts its own matching silhouette shadow —
its prepass discards on coverage, not alpha mode (§8.1) — so removing
the proxy loses nothing for matching shadows. This phase is independent
of the Phase 1–3 sequence and leaves the build green. **Phase 5 review —
evidence:** the prepass `fragment` in `text/slug/shaders/slug_text.wgsl`
discards where `render_coverage(in.uv, glyph) < 0.5` (line 340, under
`#ifdef PREPASS_PIPELINE`); the main-pass `is_shadow_proxy` discard is at
line 358. Confirm the coverage-discard still drives the prepass before
deleting the proxy — it is the reason the visible mesh casts a matching
silhouette without a second mesh.

- [ ] Delete the slug shadow-proxy material path: the `is_shadow_proxy`
      uniform field (`text/slug/material.rs` `SlugTextUniform` and
      `text/slug/shaders/slug_text.wgsl` — **paths moved in Phase 4**), the
      `slug_text_shadow_proxy_material` constructor plus the
      `build_slug_text_material` proxy parameter (fold back to one
      `slug_text_material`), the main-pass `is_shadow_proxy` discard in the
      shader, and the **`pub(crate)` re-export at `text/slug/mod.rs:26`** (Phase 4
      removed all `lib.rs` slug re-exports, so this is the only surfacing point).
      **Phase 4 review:** also reword the slug-module doc comments that describe
      the deleted behavior — the `is_shadow_proxy` uniform doc (`material.rs` ~41–51)
      and the `SlugRenderMode` variant docs (`material.rs` ~17–30), since
      `Invisible`/`SolidQuad`/`is_shadow_proxy` are being deleted. *(Phase 5 review
      corrected these line ranges from the Phase 4 review's ~43–51 / ~20–31.)*
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
      (`layout/text_props.rs` — **Phase 5 review:** the `const _: () = assert!(…)`
      block is at lines ~838–841 and the enum + `discriminant()` fn at ~99–122, not
      the "~901–904" the Phase 2 checklist cited), `SlugRenderMode` (`material.rs`), the
      shader's `RENDER_MODE_SOLID_QUAD` constant plus its `SolidQuad` /
      `Invisible` branches in `render_coverage`, and (Phase 1 review) the
      `slug_render_mode` / `slug_shadow_render_mode` helpers plus any
      `From<GlyphRenderMode> for SlugRenderMode` match arms in **both**
      `render/world_text/mesh_spawning.rs` and
      `render/text_renderer/batching.rs` — they match on the dropped
      `Invisible` / `SolidQuad` variants. **Phase 4 review:** the single
      conversion seam is `From<GlyphRenderMode> for SlugRenderMode` at
      `render/world_text/mesh_spawning.rs:237`; `slug_render_mode` is a sibling
      `const fn` at `render/text_renderer/batching.rs:397`. The enum shrink, the
      `From` impl, and both helpers must change in the same commit to keep the
      build green.
- [ ] Collapse `GlyphShadowMode` to a cast toggle (`{ None, Cast }`),
      replacing the `None / SolidQuad / Text / PunchOut` silhouette
      choice. Update `with_shadow_mode` and its call sites, and (Phase 1
      review) reword the `GlyphShadowMode` enum doc in `layout/text_props.rs`
      — it currently describes spawning "a separate shadow proxy mesh with
      `AlphaMode::Mask` … contributes to the shadow prepass," which this phase
      makes false. **Phase 5 review — set the new `#[default]` and preserve current
      behavior:** `GlyphShadowMode` currently derives `Default` with `#[default]` on
      `Text` (`text_props.rs:141`), so every `WorldTextStyle::new(…)` that omits a
      shadow mode today casts a shadow. The shrink must put `#[default]` on `Cast`
      (the `Text → cast` equivalent), matching the "visible mesh is a caster unless
      the caller turns shadows off" rule above — otherwise all shadow-mode-unset text
      (most of `typography.rs`, `slug_text.rs`, `sdf.rs`) silently stops casting.
      **Phase 6 review:** `GlyphShadowMode::None` survives the `{ None, Cast }`
      collapse, so the `::None` sites in `font_loading.rs` / `text_stress.rs` /
      `text_renderer_gpu_bench.rs` need no edit; `examples/sdf.rs:529` (below) is the
      only `::Text` example site. Beyond the `#[default]`, two explicit constructors
      hardcode `shadow_mode: GlyphShadowMode::Text` at `text_props.rs:560` and `:664` —
      migrate both to `Cast` in the same edit or they won't compile (the sibling
      `render_mode: GlyphRenderMode::Text` at `:559`/`:663` is unaffected — `Text`
      survives in `GlyphRenderMode`).
      **Phase 3 review:** the only `debug/` call site is `label_shadow_mode()`
      returning `GlyphShadowMode::Text` (`debug/typography_overlay/mod.rs:108`),
      consumed by ~11 `with_shadow_mode(...)` sites in `glyph.rs` / `labels.rs`; the
      single edit is `Text → Cast` there. No `Invisible` / `SolidQuad` render-mode
      usage exists in `debug/`, so `typography_overlay` needs only the shadow-mode
      change for this phase.
- [ ] Document the shadow-only recipe — spawn a cast-on glyph with fill
      alpha 0 (invisible in color, full silhouette in shadow) — on the
      cast toggle and in `slug_fx.md` §8.3, replacing the deleted `Invisible`
      render mode. (`slug.md` was deleted as obsolete — see Phase 8.)
- [ ] Solid-block backings: callers that used `SolidQuad` compose a
      standard `Mesh3d` rectangle with a `StandardMaterial` (§8.2); there
      is no slug render mode for it.
- [ ] Delete `examples/shadows.rs` — its subject, the matrix, is gone.
- [ ] Update any remaining `with_render_mode(Invisible | SolidQuad)` /
      `with_shadow_mode(...)` call sites in examples and `debug/`. **Phase 5 review:**
      `examples/sdf.rs:529` calls `.with_shadow_mode(GlyphShadowMode::Text)` on a
      `WorldText` row label — it is "untouched" in Phases 5 and 7's main list but will
      not compile once the `Text` variant is removed. Migrate it to `Cast` here. (The
      panel-SDF body of `sdf.rs` stays out of scope; only this one text call site moves.)
- [ ] `cargo build`, `cargo nextest run`, `cargo +nightly fmt`; rerun the
      suite and screenshot to confirm Text and PunchOut fills, the
      cast-shadow toggle, and the alpha-0 shadow-only recipe.

Public-API note: `GlyphRenderMode` and `GlyphShadowMode` stay public
(callers name them for fill / shadow intent) but shrink — consistent
with decision #3, exposing only what a feature needs.

### Phase 8 — Documentation

Added by the Phase 5 review: the **Documentation disposition** (below) was flagged
across the Phase 2–4 reviews but never executed in any phase — Phases 0–7 are code,
verification, and the matrix collapse. This final phase owns the doc backlog so the
migration does not finish with `docs/` describing the deleted distance-field engine.
It is pure documentation; the build is already green. See the **Documentation
disposition** table below for the per-file detail.

- [ ] Delete `docs/bevy_diegetic/gpu_rasterizer.md` (1539 lines) — documents only the
      removed GPU SDF/MSDF rasterizer; dead since Phase 2. The docs are committed, so
      this is recoverable.
- [ ] Rewrite the two stale MSDF lines left in `crates/bevy_diegetic/README.md`: the
      intro at **line 13** ("Text is rendered via MSDF … atlas rasterization") and the
      feature bullet at **line 17** ("MSDF text rendering with per-glyph async
      rasterization, multi-page atlas"). **Phase 6 review — scope shrank:** Phase 10
      already rewrote the "Text transparency" section (~34–55) and the TAA/AA wording
      (~80–98) to coverage-based slug content, so the earlier "~36–116 subsection
      rewrite" estimate no longer applies — only lines 13 and 17 remain. The panel SDF
      references (`render/sdf_material.rs`, `examples/sdf.rs`) stay — that subsystem is
      out of scope (see the scope-boundary section).
- [x] Deleted `docs/bevy_diegetic/slug.md` (~1275 lines, 2026-05-25) — the obsolete slug
      *feasibility* design doc. **Phase 6 review:** it documents only deleted or superseded
      concepts (the `TextRendererPreference` MTSDF-switching model, the four-mode
      render/shadow matrix Phase 7 removes, HueOffset-as-future) with no unimplemented work
      to salvage — its effects roadmap lives in `slug_fx.md` and the migration in this doc.
      Repo-wide grep found zero inbound references, so deletion broke no links. Recoverable
      from git.
- [ ] Confirm no remaining `docs/` reference describes the distance-field engine as a
      live renderer (the plan itself, `slug_migration.md`, documenting the removal is
      fine).

### Phase 9 — Unify the cascade into one parent-walking hierarchy

Surfaced by the **Phase 6** example smoke check: `text_alpha` crashed on every
alpha-mode switch. Root cause — the 2-tier standalone cascade decides membership
by "does this entity hold the `Override` component?", and `WorldTextStyle` (the
`WorldTextAlpha`/`WorldFontUnit` override) lives on **both** standalone world text
and panel labels. So panel labels were enrolled as standalone targets, carried a
`Resolved<WorldTextAlpha>` nothing reads (panel labels render from
`Resolved<PanelTextAlpha>`, the 3-tier cascade), and the global-default propagate
loop wrote to them — including the frame a HUD rebuild despawned them, landing a
deferred `insert` on a freed entity → panic.

**Small fix already landed in Phase 6** (not deferred): added `type Exclude:
Component` to `CascadeTarget`, filtered both write paths
(`on_cascade_target_added`, `propagate_global_default_to_entity`) with
`Without<A::Exclude>`, and set `Exclude = PanelTextChild` on `WorldTextAlpha` /
`WorldFontUnit` (other impls use the new `ExcludeNone` sentinel). The standalone
cascade now never enrolls panel labels; the crash is gone, verified by cycling all
seven alpha modes with no panic. This phase is the **root-structure** follow-up the
small fix points at.

The deeper issue the bug exposes: the cascade module has **three fixed-depth
topologies** (`mod.rs` table) — entity-targeted (entity → global), panel-targeted
(panel → global), child-of-panel (child → panel → global). The last two are the
*same chain* (`global → panel → child`) cut at different depths and built as
separate traits/plugins. The design enumerates "how many tiers does this attribute
have" instead of propagating along the entity tree. That is why one logical value
(text alpha) exists as two `Resolved` types (`WorldTextAlpha` and `PanelTextAlpha`)
and why a category-error in membership was even possible.

**Goal:** replace the three topologies with one parent-walking resolution. One
`Resolved<A>` per attribute; one rule — *my override, else my parent's `Resolved<A>`,
else the global default at the root* — applied by following `ChildOf` links.
Standalone text is depth-1 off the root, a panel is depth-1, a panel label is
depth-2; a future deeper nesting needs no new type. Membership becomes "position in
the tree," so an entity is never enrolled into the wrong cascade by an incidental
shared component, and the `Exclude` marker introduced by the small fix is no longer
needed.

This phase is independent of the Phase 1–3 sequence and leaves the build green.

- [ ] Define one uniform "override for attribute `A` at this node" accessor across
      the node-bearing components (`WorldTextStyle`, `DiegeticPanel`, panel-child
      override) so the walk reads every level the same way — the main new
      abstraction this requires.
- [ ] Collapse the per-role attribute types into one per value: `WorldTextAlpha` +
      `PanelTextAlpha` → one `TextAlpha`; `WorldFontUnit` + `PanelFontUnit` +
      panel-child font unit → one `FontUnit`. One `Resolved<TextAlpha>` /
      `Resolved<FontUnit>` per entity, read by both standalone and panel render
      paths (drop the `Without<PanelTextChild>` / `With<PanelTextChild>` split on the
      read side).
- [ ] Replace `CascadeTarget` (2-tier) and `CascadePanelChild` (3-tier) plus their
      three plugins (`CascadeEntityPlugin`, `CascadePanelPlugin`,
      `CascadePanelChildPlugin`) with one hierarchical cascade plugin that walks
      parent links with a global-default fallback at the root.
- [ ] Remove the `Exclude` associated type and `ExcludeNone` sentinel added by the
      Phase 6 small fix — the tree-position membership makes them unnecessary. This
      also removes the in-module test impls' `type Exclude = ExcludeNone;`
      (`cascade/target.rs:127, 244`), the `use crate::cascade::ExcludeNone;`
      (`target.rs:111`), and the `Exclude` / `ExcludeNone` doc text (`resolved.rs`
      and the `mod.rs` re-export).
- [ ] Re-verify `text_alpha` (cycle all alpha modes), panel text, and standalone
      world text all resolve correctly, and the crash stays fixed.

**Risk:** the uniform override accessor and a virtual global root at the top of every
chain are real new abstraction; this is a cascade-module rewrite, larger than the
Phase 6 small fix. Sequence it after the rest of the migration so it lands against a
green, slug-only tree.

### Phase 10 — Remove the OIT / transparency workaround stack — complete (2026-05-25)

Surfaced by the **Phase 6** example smoke check: world-space panel text in
`units` (A4 paper, business card, photo) z-fought with the panel surface and
washed out — a 72pt heading that should be the largest text on screen was barely
visible. Root cause was a stack of **distance-field-era workarounds**, not a slug
bug:

- The old MSDF renderer emitted one `AlphaMode::Blend` quad **per glyph**. Many
  coplanar transparent quads sort-flipped by view angle, so a `StableTransparency`
  camera marker enabled Bevy **order-independent transparency (OIT)** to stabilize
  them (`render/transparency.rs`).
- OIT stores the **unbiased** `in.position.z`, so `depth_bias` no longer separated
  layers. The code compensated with a manual `OIT_DEPTH_STEP` / `oit_depth_offset`
  applied in the shader before `oit_draw`, and OIT forced `Msaa::Off`, which in turn
  required aggressive cross-camera MSAA management.

Slug emits **one mesh per text run**, not per glyph, so the per-glyph sort-flip
cannot occur. With OIT removed, `depth_bias` works again and orders coplanar text
against the panel surface directly. Validated: removing `StableTransparency` from
`units` cleared the z-fight (72pt heading crisp); `world_text` — the example OIT was
originally added for — renders clean at a steep grazing angle over the coplanar
ground text, no shading-shift artifact.

**Removed (workspace-wide):**

- `render/transparency.rs` (the `StableTransparency` marker + the three
  OIT/MSAA-management observers); its registration and `pub use` in `render/mod.rs`;
  the `pub use render::StableTransparency` in `lib.rs` (public-API removal).
- `OIT_DEPTH_STEP` (`render/constants.rs`) and the `oit_depth_offset` uniform field
  threaded through `render/sdf_material.rs`, `callouts/render.rs`, and
  `panel_geometry.rs`.
- The `#ifdef OIT_ENABLED` / `oit_draw` blocks in `text/slug/shaders/slug_text.wgsl`
  and `shaders/sdf_panel.wgsl`.
- fairy_dust: `src/transparency.rs` capability + the `with_stable_transparency()`
  builder methods (`sprinkle.rs`/`primitive.rs`/`title_bar.rs`/`camera_home.rs`) +
  doc references.
- `.with_stable_transparency()` from the `world_text`, `slug_text`, `typography`,
  and `units` examples.

**Reworked:** `examples/text_alpha.rs` — its purpose was demoing this machinery (the
`C`-key cycle was MSAA → `StableTransparency` → Off). Dropped the camera-state cycle;
kept the `1`–`7` alpha-mode demo. Rewrote the per-mode descriptions for slug (they
referenced "the MSDF shader" and recommended `StableTransparency`). README "Text
transparency" section rewritten to the single coverage-based path.

**Out of scope (left as-is):** `examples/sdf.rs` (the unrelated procedural-SDF demo —
see the scope-boundary section) keeps its own `oit_depth_offset: 0.0`.

Verified green: `cargo build --workspace --examples --features
typography_overlay,bench_support`; `cargo clippy --workspace --all-targets …` clean;
`cargo nextest run --lib` 158/158 (1 skipped); `cargo +nightly fmt` clean; plus the
example checks above.

The earlier `project_oit_fixes_worldtext_lighting_shift` and
`project_depth_bias_oit_bug` memory notes describe the **former** constraints and are
superseded by this phase.

## Documentation disposition

The docs are committed, so deletion is recoverable; the recommendation
favors removing what only describes the deleted engine and keeping
everything slug.

| Doc | Recommendation | Reason |
| --- | --- | --- |
| `gpu_rasterizer.md` (1539 lines) | **Delete** | Documents only the GPU SDF/MSDF rasterizer being removed. No slug content. |
| `slug.md` (1275 lines) | **Deleted (2026-05-25)** | Obsolete slug *feasibility* design doc. Documents only deleted/superseded concepts: the `TextRendererPreference` MTSDF-switching model, the four-mode render/shadow matrix (Phase 7 removes it), HueOffset-as-future (resolved: removed). No unimplemented work to salvage — the effects roadmap is in `slug_fx.md`, the migration in this doc. Zero inbound references repo-wide. Recoverable from git. |
| `slug-experiments.md` (1798 lines) | **Done (Phase 5)** | The CPU-prep-cost line that pointed at `benches/glyph_rasterization.rs` was rewritten to record the figure as a one-time measurement (full printable ASCII ≈ 0.84 ms, 2026-05-24, JetBrains Mono 128 px, after per-curve dedup + 48-band tuning) and note that a replacement micro-bench would target `prepare_positioned_run_with_scale` + `ensure_run_storage`. |
| `slug-benchmark-procedure.md` (244 lines) | **Done (Phase 5)** | The "Prep-Time Benchmark" step that ran the deleted `glyph_rasterization` Criterion group was rewritten to state prep cost is no longer bench-tracked (record the ≈0.84 ms figure), and the obsolete `slug_text_spike` "spike-only changes" comparability rule was deleted (no spike/production split survives). |
| `slug_fx.md` (641 lines) | **Keep** | The effect-support plan that motivates this migration. |

Other (non-`docs/`) artifacts that reference the removed engine, found
in review — fold these into Phase 5/6:

- `crates/bevy_diegetic/README.md` — **section rewrite, not a reword**
  (Phase 2 review). **NOT yet executed.** MSDF/atlas references span lines ~13,
  17, 36–42, 111, 116, including a whole "Transparency / Preserves MSDF
  anti-aliasing?" subsection (~36–116) describing `fwidth`-based MSDF edge AA.
  slug's coverage-based AA differs, so this is rewritten content, not find/replace.
- `scripts/xctrace_text_renderer.sh` — **Done (Phase 5).** The `record` / `export`
  usage, the `require_mode` case, and both `record-all` / `export-all` loops were
  reduced from `empty|slug|sdf|msdf|mtsdf` to `empty|slug`.
- `scripts/parse_gpu_intervals.py` (Phase 2 review) — **Reviewed, no change needed
  (Phase 5).** It filters on `PROCESS_FILTER = "text_renderer_gpu_bench"` (the
  kept process name), not on a renderer mode, so reducing the xctrace mode list
  does not affect it.
- `ci.yml` — **Done (Phase 5): no edit needed.** `ci.yml:202` runs
  `cargo bench -p bevy_diegetic --benches …`, a `--benches` glob with no named
  target, so deleting `benches/glyph_rasterization.rs` + its `[[bench]]` manifest
  entry removes it from CI on its own. The earlier "drop that bench from CI in the
  same change" framing assumed a named reference that does not exist.

## Build-green ordering

As implemented, Phase 1 deleted each distance-field arm **together with its
exclusive call-tree** (not just the `if`/`else` branch), so no live code is left
referencing a deleted type. The surviving DF engine (`glyph_material.rs`, the
atlas modules, and the now-dead `spawn_world_text_meshes` / `TextPlugin` atlas
init / `poll_atlas_glyphs`) still references only types that exist until Phase 2,
so it compiles as dead code. With the external `nalgebra` bump reverted to
`0.34`, **the library is green at the end of Phase 1** (`cargo nextest run --lib`
passes 234/234). The earlier expectation that "Phase 1 does not compile on its
own" assumed a shallower arm-deletion; the thorough version leaves the lib green.

What stays red after Phase 1 is **only the examples and benches** — they call
the removed public APIs and were not touched in Phase 1, so `cargo build
--examples` / `cargo bench --benches` is red until those edits land. **As
actually implemented (Phase 2 review):** Phase 2 deleted `glyph_material.rs` /
`atlas.rs` and their dead consumers (`spawn_world_text_meshes` +
`MeshSpawnAssets`, `TextPlugin` atlas init, `poll_atlas_glyphs`, plus the
revealed-dead `glyph_quad.rs` and `text_shaping.rs` quad helpers) in one pass,
so the **lib is green at the end of Phase 2 — `cargo nextest run --lib` = 158**
(not 234; the deleted modules took ~76 tests with them) and the full workspace
`cargo build` (libs + `fairy_dust` bin) passes. Phase 2 did **not** fix the
examples/benches — that is Phase 5 — so `cargo build --examples` /
`cargo bench --benches` stays red across Phases 2–4 and turns green only at the
end of Phase 5. Phase 3 is pure manifest cleanup
(`nalgebra`/`fdsm`/`fdsm-ttf-parser`/`msdfgen`/`ttf-parser_018` are unimported
and harmless by then, so dropping them is a no-op on `cargo build`). Phase 0
(verification) and the slug-specific polish in Phases 4–6 each leave the lib
green.

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
