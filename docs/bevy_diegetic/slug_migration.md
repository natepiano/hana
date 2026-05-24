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
      update `SLUG_TEXT_SHADER_PATH` to the new embedded path. (Helper added
      in Phase 1 because the macro resolves paths relative to the calling
      file and the shader had not moved yet.)
- [ ] Make every `Slug*` type `pub(crate)` and drop all `Slug*`
      re-exports from `lib.rs` (expose-nothing — see Open decisions #3).
      The public text API is the existing agnostic surface only.
      Scope note (Phase 1 review; count corrected Phase 2 review to **~31
      `pub use` lines**, also covering `SlugBackendCompleted`,
      `SlugRunStorageProfile`, `slug_text_material`, `load_glyph_by_id_from_face`):
      this is the `Slug*` block in `lib.rs`
      (lines ~180–211: `SlugBandRecord`, `SlugBounds`, `SlugBuiltTextRun`,
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
deferred — otherwise the examples break. **Note (Phase 1 review): Phase 1 did
not touch any example**, so `cargo build --examples` is red from the end of
Phase 1 until these edits land — the example fixes are now Phase-2-or-later
work, not Phase-1 work, even though the label says "Phase 1–3 sequence." What remains genuinely
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
| `atlas_pages.rs` | Delete (visualizes atlas pages; no slug analog). **Also remove its `[[example]] name = "atlas_pages"` entry from `bevy_diegetic/Cargo.toml`** — deleting the `.rs` while leaving the manifest entry makes `cargo build` fail with "can't find target". |
| `preload_text.rs` | **Delete.** Built on the distance-field atlas preload API (`GlyphAtlas::preload`) and `GlyphLoadingPolicy`, both removed in the Phase 1–2 sequence. Slug needs no preload demo: per-glyph band-building is sub-millisecond — full printable ASCII preps in ≈ 0.84 ms (after per-curve dedup + 48-band tuning), well below one frame and below frame-timing resolution. There is no warm-up cost worth showcasing and no preload API is shipped. A project with very large glyph sets that notices first-frame lag can warm glyphs with its own Bevy task / async setup — no engine API required. **Also remove its `[[example]] name = "preload_text"` entry from `bevy_diegetic/Cargo.toml`.** |
| `benches/glyph_rasterization.rs` | Delete (CPU/GPU MSDF rasterizer bench; no slug analog). **Also remove its `[[bench]] name = "glyph_rasterization"` entry from `bevy_diegetic/Cargo.toml`.** This bench has no `required-features`, so `cargo bench --benches` (CI, `ci.yml:202`) compiles it and it imports deleted `DistanceField` / `GlyphAtlas` / `GlyphKey` — it is **red from the end of Phase 2 until this deletion lands**, so the source deletion, the manifest-entry removal, and the CI `cargo bench` edit must land together. |
| `examples/sdf.rs` | Untouched (panel SDF). |

### Phase 6 — Verify

- [ ] `cargo build`
- [ ] `cargo nextest run` — expected baseline is **≈158 lib tests** (not 234;
      Phase 2 deleted the atlas/rasterizer modules and their ~76 tests), plus
      whatever example/bench targets compile after Phase 5. Do not treat the
      234 figure from the Phase 1 notes as the target.
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
      (`layout/text_props.rs`), `SlugRenderMode` (`material.rs`), the
      shader's `RENDER_MODE_SOLID_QUAD` constant plus its `SolidQuad` /
      `Invisible` branches in `render_coverage`, and (Phase 1 review) the
      `slug_render_mode` / `slug_shadow_render_mode` helpers plus any
      `From<GlyphRenderMode> for SlugRenderMode` match arms in **both**
      `render/world_text/mesh_spawning.rs` and
      `render/text_renderer/batching.rs` — they match on the dropped
      `Invisible` / `SolidQuad` variants.
- [ ] Collapse `GlyphShadowMode` to a cast toggle (`{ None, Cast }`),
      replacing the `None / SolidQuad / Text / PunchOut` silhouette
      choice. Update `with_shadow_mode` and its call sites, and (Phase 1
      review) reword the `GlyphShadowMode` enum doc in `layout/text_props.rs`
      — it currently describes spawning "a separate shadow proxy mesh with
      `AlphaMode::Mask` … contributes to the shadow prepass," which this phase
      makes false.
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
| `slug-experiments.md` (1798 lines) | **Keep, fix stale refs** | The experiment log that prevents repeating failed approaches. It references `benches/glyph_rasterization.rs` for CPU prep cost; that bench is deleted in Phase 5. With both that bench and `preload_text` gone, the ≈0.84 ms prep-cost figure has no measuring harness left (Phase 2 review) — either record it in the doc as a one-time measurement (with date + conditions) or preserve a slug prep micro-bench; do not leave a dangling reference to a deleted bench. |
| `slug-benchmark-procedure.md` (244 lines) | **Keep, fix stale ref** | Canonical slug benchmark procedure. References `text_renderer_gpu_bench` (kept, converted to slug-only) but also the deleted `glyph_rasterization` bench for prep cost — note that prep cost is no longer tracked, or point at a slug replacement. |
| `slug_fx.md` (641 lines) | **Keep** | The effect-support plan that motivates this migration. |

Other (non-`docs/`) artifacts that reference the removed engine, found
in review — fold these into Phase 5/6:

- `crates/bevy_diegetic/README.md` — **section rewrite, not a reword**
  (Phase 2 review). MSDF/atlas references span lines ~13, 17, 36–42, 111, 116,
  including a whole "Transparency / Preserves MSDF anti-aliasing?" subsection
  (~36–116) describing `fwidth`-based MSDF edge AA. slug's coverage-based AA
  differs, so this is rewritten content, not find/replace.
- `scripts/xctrace_text_renderer.sh` — supports `sdf` / `msdf` / `mtsdf`
  modes for `text_renderer_gpu_bench`. Reduce to `slug` / `empty`.
- `scripts/parse_gpu_intervals.py` (Phase 2 review) — hard-codes
  `PROCESS_FILTER = "text_renderer_gpu_bench"` (kept, so the script keeps
  working) but pairs with `xctrace_text_renderer.sh`'s removed modes; sanity-
  check it when reducing the mode list.
- `ci.yml` — runs `cargo bench` (`ci.yml:202`) including the deleted
  `glyph_rasterization` bench; drop that bench from CI **in the same change
  that deletes the bench** (see Phase 5 — the bench is red between Phase 2 and
  that deletion).

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
