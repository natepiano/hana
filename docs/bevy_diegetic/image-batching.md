# Image Batching

> **Status: IMPLEMENTATION PLAN — phased, delegate-ready.** Route diegetic image
> and precompose leaves through a batched render family (`ImageBatchStore`) instead
> of per-command child entities, then collapse the four batch families onto a
> generic `BatchStore<F>`.

## Delegation Context
<!-- Shared across all phases. /plan:delegate prepends this to every dispatch. -->

- **Project:** `bevy_diegetic` (workspace member in `hana`, `crates/bevy_diegetic`) — diegetic UI layout engine for Bevy: in-world panels driven by a Clay-inspired layout algorithm, with a batched renderer for SDF surfaces, text, panel shapes, and (this project) images.
- **Stack:** Rust (edition 2024), Bevy `0.19.0` (features incl. `bevy_pbr`/`bevy_render`/`bevy_core_pipeline`/`bevy_image`/`bevy_anti_alias`), `bevy_kana`, `bytemuck`, `smallvec`, WGSL shaders; render path uses `ExtendedMaterial<StandardMaterial, _>`, `ShaderType` storage buffers, OIT (`OrderIndependentTransparencySettings`).
- **Layout:**
  - `crates/bevy_diegetic/src/render/` — batch stores, keys, materials, systems (`fill_batch.rs`, `batch_key.rs`, `draw_order.rs`, `precompose.rs`, `dirty.rs`, `material_table.rs`, `mod.rs`, subdirs `panel_text/`, `panel_shapes/`, `analytic_paths/`).
  - `crates/bevy_diegetic/src/layout/render.rs` — `DrawBatchFamily` enum + `draw_batch_family()` routing.
  - WGSL: `crates/bevy_diegetic/src/shaders/` (`sdf_panel.wgsl`, `sdf_material_table.wgsl`) and `crates/bevy_diegetic/src/render/` (`sdf_stroke.wgsl`, `material_table.wgsl`). New image shader lands under `src/shaders/`.
  - Doc to update as-built: `docs/bevy_diegetic/batching-diagram.md`.
- **Key files:**
  - `crates/bevy_diegetic/src/render/fill_batch.rs` — SDF batch store/key/systems reference implementation to copy-adapt; `FillBatchPlugin` (registration), `OPAQUE_FILL_DEPTH_PUSH_LAYERS` (:89), `contiguous_drawn_run`/`assign_contiguous_runs` (:392), `sort_records`/`refresh_first_draw_order_index` (:597), specialization/entry-point declarations (:807-813), `sdf_batch_alpha_mode` shadow remap (:906), material `depth_bias` from rank (:881), `register_sdf_batch_materials::<T>` (:1180), `update_sdf_batch_world_transforms` marks both dirty flags (:1288-1316), `linear_color` tint (:1940).
  - `crates/bevy_diegetic/src/render/batch_key.rs` — shared `BatchAlphaMode`/`BatchRenderLayers`/`VisualShadow` key fragments (home of any shared shadow-alpha helper — helper removed from image scope per PD-1).
  - `crates/bevy_diegetic/src/render/draw_order.rs` — `ScreenDepthBias` (f32, `PartialEq` only, no `Eq+Hash`, :70), `DrawZIndex`/`DrawZIndexRank` (hashable), `ClipDepthNudge`/`OitDepthOffset`, `DrawOrderIndex`; per-record depth plumbing.
  - `crates/bevy_diegetic/src/render/precompose.rs` — `precompose_image(pixel_size)`, offscreen target `TextureFormat::Bgra8UnormSrgb` (:417) — keep as-is (PD-2).
  - `crates/bevy_diegetic/src/render/panel_text/reconcile.rs` — `PanelImageChild` (:526), `reconcile_panel_image_children` (:562), `collect_panel_image_commands` (:674), empty-clip image cull (:686), precompose `entry(...)?` skip (:697), image hard-`Blend` at `TEXT_Z_OFFSET` (entity-path spawn, deleted in Phase 8), `reconcile_ms` image accumulate (Phase 6 shifted it to ~:668-671; text writer is an assignment). Image test helpers (`record_modified_materials`, `image_reconcile_app`, the three old assertions) were DELETED by Phase 6 — only the guard test `image_batch_family_commands_do_not_spawn_legacy_children` (~:1461) remains, itself deleted in Phase 8.
  - `crates/bevy_diegetic/src/render/panel_text/batching.rs` — text unconditional `Opaque -> Mask(0.0)` remap (:1169), growth-guard test `commit_payloads_keep_a_constant_length_between_growths` (:2220).
  - `crates/bevy_diegetic/src/render/panel_shapes/batching.rs` — `PanelShapeBatchStore` (:344), `draw_batch_family` router-gate model (:825), Shape-only atlas; no buffer-growth test here.
  - `crates/bevy_diegetic/src/render/analytic_paths/batching.rs` + `crates/bevy_diegetic/src/render/analytic_paths/material.rs` — Path family store (`PathBatchStore`, per-run) + `PathExtendedMaterial` (generic-collapse reference).
  - `crates/bevy_diegetic/src/layout/render.rs` — `DrawBatchFamily` enum (:69, has `SdfSurface`/`PanelShape`/`Text` — add `Image`), `draw_batch_family()` returns `None` for `Image`/`PrecomposeLdr` (:138/:143 — route to new `Image`).
  - `crates/bevy_diegetic/src/shaders/sdf_panel.wgsl` — SDF shader; prepass fragment samples alpha and `discard`s (`fill_alpha_for_prepass` ~:312-342) — pattern for the image prepass discard.
  - `crates/bevy_diegetic/src/render/dirty.rs` — shared `Dirty` flag.
  - `crates/bevy_diegetic/src/render/material_table.rs` — `MaterialTablePlugin`, per-frame material register/rebind (:859); image family binds NO `material_table` and must skip register+rebind.
  - `crates/bevy_diegetic/src/render/mod.rs` — `RenderPlugin` (:372/:385), `add_plugins((MaterialTablePlugin, AnalyticPathPlugin, FillBatchPlugin, TextRenderPlugin, PanelGeometryPlugin, PanelShapePlugin))` (:395) — new `ImageBatchPlugin` registers here.
  - `crates/bevy_diegetic/src/render/transparency.rs` — OIT is opt-in (`StableTransparency` on a `Camera3d`); intra-batch order OIT-off relies on `sort_records`.
- **Build:** `cargo build`
- **Test:** `cargo nextest run -p bevy_diegetic`
- **Lint:** run the `clippy` skill (workspace clippy is strict: `all`/`cargo`/`nursery`/`pedantic` denied, plus `unwrap_used`/`expect_used`/`panic`/`unreachable`/`missing_docs` denied).
- **Style:** `zsh ~/.claude/scripts/rust_style/load-rust-style.sh --project-root /Users/natemccoy/rust/bevy_diegetic_batching`
- **Invariants:**
  - Batch keys must be `Eq + Hash`: key on `DrawZIndex` + `DrawZIndexRank`, NOT the f32 `ScreenDepthBias`; derive `depth_bias = z_index_rank.screen_depth_bias().get()`.
  - `DrawOrderIndex` stays per-record, never in the batch key; it feeds `ClipDepthNudge`/`OitDepthOffset`.
  - ShaderBuffer growth guard reproduced explicitly: fixed-capacity payload (`set_data` byte length constant), new buffer on growth + material bind-group re-point, capacity `record_count().max(1).next_power_of_two()`.
  - Records sorted by draw order before upload (`sort_records` + `refresh_first_draw_order_index`); OIT is opt-in so intra-batch order relies on this.
  - `ImageBatchKey` OMITS `contiguous_drawn_run` (images always `Blend`); `alpha_mode` is constant (drop or document always-`Blend`).
  - Per-record world transform: CPU `ResolvedImageRecord` (`record_key {panel, command_index}`, bounds, linear-`Vec4` tint, uv_rect, draw_depth, `transform: Mat4`) + GPU `ImageRenderRecord: ShaderType`; add `update_image_batch_world_transforms` after `TransformSystems::Propagate` marking BOTH dirty flags.
  - Tint is linear `Vec4` multiplied post-sRGB-decode; do not use `StandardMaterial::base_color` for per-record tint.
  - Image material must NOT strip `MATERIAL_BIND_GROUP_INDEX` (its record buffer + texture live there); `ImageExtension` always carries a `#[storage]` entry so the group is never empty (structural, no runtime guard).
  - Declare main/prepass/shadow vertex entry points (vertex-pull from record buffer over inert mesh); image-shadow alpha comes from a prepass fragment that samples texture alpha and `discard`s, NOT the alpha-mode helper.
  - Router is a full per-frame rebuild (model on SDF/`panel_shapes:825` gate, read effective `RenderLayers`/`Resolved<ShadowCasting>` from panel query); preserve empty-clip cull and precompose `entry(...)?` skip (never synthesize `Handle::<Image>::default()`).
  - Atomic cutover: do NOT flip `draw_batch_family(Image)=Some` until the batch path is live — flip + `collect_panel_image_commands` gate + activation in one commit (no double-draw / no no-draw window).
  - `b.image(el, handle, tint)` authoring API must not change; no atlas/bindless in this pass (`uv_rect` forward-compat only).
  - Keep precompose target `Bgra8UnormSrgb` sampled via `base_color_texture` (PD-2, no format change).

## Phases

### Phase 1 — Immediate Fix (entity-path layers + shadow)  · status: done (shipped pre-plan)

#### Work Order

**Goal:** Entity-path image children read effective `RenderLayers` + resolved `ShadowCasting` and never hard-code layer 0.

**Spec:**
Query the owning panel's effective `RenderLayers` and resolved `ShadowCasting`; use those when spawning an image child; store the effective shadow policy in `PanelImageChild`; keep the effective layer as the child entity's `RenderLayers` component; compare cached shadow policy when reusing a child; update the child's `RenderLayers` and `NotShadowCaster` state even when image handle, tint, bounds, and draw depth are unchanged. Correctness fix on the entity path — not the batch path. The two assertions this phase pins ("hard-coded layer 0 gone", "reused child updates `NotShadowCaster` when resolved `ShadowCasting` changes") stay attached here; Phase 8 deletes the code they test and retires them with it.

**Files:**
- `crates/bevy_diegetic/src/render/panel_text/reconcile.rs` — `PanelImageChild` cache field + reuse comparison.

**Constraints from prior phases:** none (Phase 1).

**Acceptance gate:** shipped; `cargo nextest run -p bevy_diegetic` green with tests asserting reused image child updates `NotShadowCaster` on resolved `ShadowCasting` change and no hard-coded layer 0.

### Phase 2 — Image batch types + store  · status: done (uncommitted)

#### Work Order

**Goal:** `ImageBatchKey`, the CPU/GPU record split, and `ImageBatchStore` compile with unit tests proving batch split/merge, the growth guard, and draw-order sort.

**Spec:**
Define the batch key (all fields `Eq + Hash`):
```rust
ImageBatchKey {
    texture: Handle<Image>,
    layers: BatchRenderLayers,
    shadow: VisualShadow,
    z_index: DrawZIndex,
    z_index_rank: DrawZIndexRank,
}
```
Derive the material `depth_bias = z_index_rank.screen_depth_bias().get()` (mirror `fill_batch.rs:881`). OMIT `contiguous_drawn_run` (it is a depth-buffer-regime splitter that only matters because SDF is frequently Opaque; images are always `Blend` and order via per-record `oit_depth_offset` across batch boundaries, so it would only over-split). OMIT `alpha_mode` from the key, or document images as always-`Blend` — it has no authoring source.

CPU record (source of per-image state; `record_key` doubles as the store membership index):
```rust
ResolvedImageRecord {
    record_key: { panel, command_index },
    bounds,
    tint: Vec4,        // linear
    uv_rect,           // 0..1 default; atlas forward-compat
    draw_depth,
    transform: Mat4,   // filled by Phase 4's post-Propagate system
}
```
GPU record:
```rust
ImageRenderRecord: ShaderType {
    transform, size, uv_rect, tint, clip_depth_nudge, oit_depth_offset,
}
```
`ImageBatchStore` copies the `SdfBatchStore` shape (`fill_batch.rs:665`): `HashMap<ImageBatchKey, ImageBatch>` + `record_index: HashMap<record_key, ImageBatchKey>` + `upsert_record`/`remove_record`/`retain_records`/`take_empty_batches`. Per-key `ImageBatch`: `Vec<ResolvedImageRecord>`, batch `entity`, `gpu: Option<Resources>`, `record_upload` + `bounds_update` `Dirty` flags, `first_draw_order_index`, and `upsert_record`/`remove_record`/`sort_records`/`refresh_first_draw_order_index`/`world_bounds`. `sort_records` sorts records by `draw_order_index` before upload — OIT is opt-in, so intra-batch composite order depends on it (`fill_batch.rs:597`).

Reproduce all three ShaderBuffer growth guards explicitly (do not assume inheritance): (a) upload a fixed-capacity payload so `set_data` byte length never changes; (b) allocate a NEW buffer on growth and re-point the material bind group; (c) capacity `record_count().max(1).next_power_of_two()` (never zero).

**Files:**
- new module (e.g. `crates/bevy_diegetic/src/render/image_batch.rs` or an `image/` subdir — match crate module convention; reference `fill_batch.rs`) — key, records, store.
- `crates/bevy_diegetic/src/render/batch_key.rs` — reuse `BatchRenderLayers`/`VisualShadow`.
- `crates/bevy_diegetic/src/render/draw_order.rs` — reuse `DrawZIndex`/`DrawZIndexRank`/`ClipDepthNudge`/`OitDepthOffset`.

**Constraints from prior phases:** none beyond Delegation Context.

**Acceptance gate:** `cargo nextest run -p bevy_diegetic` green + unit tests: same texture + compatible shared state → one batch; different `texture`/`layers`/`shadow`/`DrawZIndexRank` → split; tint differs within one batch; growth keeps capacity stable and re-points the material (mirror `commit_payloads_keep_a_constant_length_between_growths`, `panel_text/batching.rs:2220`); same-key per-record update keeps the entity/buffer; `sort_records` orders by `DrawOrderIndex`.

#### Retrospective

**What worked:** Store/key/record copy-adapted from `SdfBatchStore` faithfully — same padding, `next_power_of_two` capacity, relative `clip_depth_nudge` (record nudge minus `first_draw_order_index.clip_depth_nudge()`), and `sort_records` (draw-order then `command_index` tiebreak). Both reviews clean (blind codex: APPROVE, no findings). Build + 7 new tests green (`612 passed`).

**What deviated from the plan:** New module lives at `crates/bevy_diegetic/src/render/image_batch.rs` (flat file, not an `image/` subdir). GPU-side types landed this phase (`ImageBatchResources`, `allocate_image_batch_resources`/`grow_image_batch_resources`/`commit_image_batch_records`) so the growth-guard test could exercise capacity — as the Work Order permitted. `ImageBatchKey.z_index` is retained in the key (alongside `z_index_rank`), matching `SdfBatchKey`.

**Surprises:**
- `ImageRenderRecord::SHADER_SIZE == 128` (const-asserted); `ImageRecordKey`, `ImageUvRect`, `ImageMaterialBindings`, `ImageBatchResources` are the concrete type names Phase 3+ must reference.
- The whole module is under a crate-level `#[expect(dead_code, ...)]` (in `render/mod.rs`) until Phase 3 wires the router; adding `route_image_batch_records` must consume enough of the surface to drop or narrow that `expect`.
- `ImageMaterialBindings` is a **stand-in** for the real material bind-group handle — Phase 5 must replace it with the actual `ImageExtendedMaterial` binding, not leave a dead type. `grow_image_batch_resources` already re-points it via `set_image_material_record_buffer`, so Phase 5's material must adopt that re-point call.

**Implications for remaining phases:**
- Phase 3 (router): call `ImageBatchStore::upsert_record(ImageBatchKey, ResolvedImageRecord)` / `retain_records(&HashSet<ImageRecordKey>)` each frame; `ResolvedImageRecord::new(...)` sets `transform: Mat4::IDENTITY` (Phase 4's post-Propagate system overwrites `.transform`). `ImageUvRect::default()` = full `0..1`.
- Phase 4 (material): replace `ImageMaterialBindings` with the real bind-group handle and keep the `set_image_material_record_buffer` re-point path on growth. (Reordered ahead of entities so the entity spawn can attach the material — see Phase 2 Review.)
- Phase 5 (entities/GPU): reuse the shipped `allocate_image_batch_resources`/`grow_image_batch_resources`/`commit_image_batch_records` helpers + the `record_upload`/`bounds_update` `Dirty` flags; `ImageBatch::world_bounds()` already exists. Add the inert batch mesh (not shipped in Phase 2) and the post-Propagate transform system.

#### Phase 2 Review

- **Reordered (user-approved):** swapped material and entities — Phase 4 is now material type + plugin, Phase 5 is batch entities + GPU + mesh + transform/bounds. So the entity spawn can attach the material (mirrors `spawn_sdf_batch_entity`) instead of retrofitting `ImageBatchResources` across two phases.
- **Phase 3:** added the world-transform carry-over fix to `ImageBatch::upsert_record` (mirror `fill_batch.rs:610-621`) — without it the per-frame router marks every static batch dirty every frame, defeating the batching; added a "static re-upsert stays clean" gate. Added the concrete store API (`upsert_record(ImageBatchKey, ResolvedImageRecord)` — key is a separate arg, unlike SDF; `ResolvedImageRecord::new`, `ImageUvRect::default()`).
- **Phase 4 (material):** now explicitly replaces the Phase-2 `ImageMaterialBindings`/`set_image_material_record_buffer` stand-ins with the real material handle on `ImageBatchResources` (self-containment fix; a leftover stand-in fails the clippy dead-code deny).
- **Phase 5 (entities):** reworded to REUSE the Phase-2 GPU helpers + `world_bounds` (they already shipped); added the inert batch mesh (`capacity*4` verts / `capacity*6` indices) + growth regen that vertex-pull requires and Phase 2 did not ship; corrected the transform system away from a literal `fill_batch.rs:1288-1316` copy to set `.transform` = panel world matrix (no `local_transform`/`update_world_transform` on the image record).
- **Dead-code guard:** the module-level `#[expect(dead_code)]` in `render/mod.rs` must be narrowed/removed as the module goes live (`unfulfilled_lint_expectations` is denied) — assigned across Phases 3–5 with the `clippy` skill added to their acceptance gates.
- **Phase 6 (shader):** corrected the WGSL spec — the GPU record carries `transform` + `size` (no raw bounds/`half_size`), so build a `size`-quad at origin and apply `transform`.
- **Phase 12:** noted the pre-existing "Phase 9" markers in `render/mod.rs` belong to a separate effort, not this plan.

### Phase 3 — Router + record building  · status: done (uncommitted)

#### Work Order

**Goal:** `route_image_batch_records` populates `ImageBatchStore` from `Image` and `PrecomposeLdr` commands every frame, without flipping `draw_batch_family`.

**Spec:**
Model the router on `route_sdf_batch_records` (full rebuild per frame), NOT the change-filtered `reconcile_panel_image_children`. Read effective `RenderLayers` and `Resolved<ShadowCasting>` from the panel query — both are in the key, so a bare layer/shadow change must re-route the record to a new key. Router-gate model is `panel_shapes/batching.rs:825`.

For each `RenderCommandKind::Image`: build a `ResolvedImageRecord` with bounds, linear-`Vec4` tint, `uv_rect` = `0..1`, `draw_depth`, `record_key {panel, command_index}`. For `RenderCommandKind::PrecomposeLdr`: precompose cache image handle → `ImageBatchKey.texture`, command bounds → record bounds, `Color::WHITE` → tint. Preserve the precompose `entry(...)?` skip (`reconcile.rs:697`) — emit no record when the cache entry is absent; NEVER synthesize `Handle::<Image>::default()` (all not-ready precomposes would collide on one bogus batch). Preserve the empty-clip cull (`reconcile.rs:686`) — emit no `ResolvedImageRecord` when `effective_clip` is empty; partial clipping stays unsupported.

**Carry the maintained world transform across re-upserts.** `ResolvedImageRecord::new` stamps `transform: Mat4::IDENTITY`, but the router rebuilds records every frame while Phase 5's post-Propagate system writes the real world transform onto the stored record. Mirror SDF's `upsert_record` (`fill_batch.rs:610-621`): before comparing/replacing, copy the currently-stored record's `transform` onto the incoming rebuilt record, so an unchanged image compares equal and skips re-upload. Without this, every static batch is marked `record_upload`/`bounds_update` dirty every frame and its transform is transiently reset to identity — defeating the dirty-flag batching. Update `ImageBatch::upsert_record` accordingly.

Do NOT flip `draw_batch_family` — build against `RenderCommandKind::Image`/`PrecomposeLdr` directly. Nothing draws yet (no batch entity until Phase 5); store state is inspectable in tests.

**Files:**
- image batch module — `route_image_batch_records` + `ImageBatchPlugin` system registration.
- `crates/bevy_diegetic/src/render/panel_text/reconcile.rs` — read-path reference only (do not modify).
- `crates/bevy_diegetic/src/render/mod.rs` — add `ImageBatchPlugin` to the `add_plugins` tuple (:395).

**Constraints from prior phases:** `ImageBatchStore`/`ImageBatchKey`/`ResolvedImageRecord` from Phase 2 (names + shapes; key omits `contiguous_drawn_run`/`alpha_mode`; `record_key {panel, command_index}` is the membership index). Concrete store API: call `ImageBatchStore::upsert_record(ImageBatchKey, ResolvedImageRecord)` — the batch key is a SEPARATE argument (unlike SDF's `upsert_record(record)`, `ResolvedImageRecord` carries NO embedded key) — plus `retain_records(&HashSet<ImageRecordKey>)` and `take_empty_batches()` each frame; build records with `ResolvedImageRecord::new(record_key, bounds, tint, uv_rect, draw_depth)` and `ImageUvRect::default()` (= full `0..1`). The `image_batch` module is under a crate-level `#[expect(dead_code)]` in `render/mod.rs` (Phase 2); wiring the router makes `upsert_record`/`retain_records`/`take_empty_batches` live but the GPU/material helpers stay dead, so keep the attribute this phase — run the `clippy` skill to confirm the expectation is still fulfilled.

**Acceptance gate:** `cargo nextest run -p bevy_diegetic` green + the `clippy` skill clean + tests: repeated same `Handle<Image>` + compatible state → one batch record set; different handle splits; a bare `RenderLayers`/`Resolved<ShadowCasting>` flip re-keys the record; empty-clip emits no record; precompose command routes into the store with `WHITE` tint; absent precompose cache entry emits no record; cross-panel same-texture records share one batch keyed by texture with distinct `record_key`s; re-upserting an unchanged record (with the carried transform) leaves the batch NOT dirty.

#### Retrospective

**What worked:** `route_image_batch_records` copy-adapts `route_sdf_batch_records` faithfully — the `Visibility::Hidden` skip, `RenderLayers::layer(0)` default, and `ShadowCasting::On` default all match the reference verbatim (`fill_batch.rs:1241-1250`). Transform carry-over landed in `ImageBatch::upsert_record` (copy stored `transform` onto incoming before the equality check → unchanged record returns early, stays non-dirty). Empty-clip cull via `clip::effective_clip(...)?` and absent-precompose skip via `precompose_cache.entry(...)?` both preserved. Build + 620 tests green (13 new), the `clippy` skill clean, `#[expect(dead_code)]` still fulfilled (GPU/material helpers stay dead). Both reviews confirmed the router logic correct.

**What deviated from the plan:** Codex also edited `docs/bevy_diegetic/batching-diagram.md` (added a premature "image batch routing" section) — outside Phase 3's Files list; Phase 8 owns the diagram. User chose to KEEP the edit rather than revert; Phase 8 revises it when the path actually renders.

**Surprises:**
- Router reads image commands directly off `computed.result().commands` (unlike SDF, which iterates a separate `ResolvedSdfSurfaceRegistry`) — there is no image surface registry; `collect_panel_image_records` filters commands in place.
- Precompose lookup keys on `command.element_idx` (not the enumerate `command_index`) — `PanelPrecomposeCache::entry(element_idx)`; the `record_key.command_index` still uses the enumerate index. These are distinct indices and both matter.
- `depth_for(command_index)?` early-returns (drops the record) when a command has no draw depth — folded into the same `filter_map` as the clip/source skips.

**Implications for remaining phases:**
- Phase 4 (material): `ImageBatchResources.material_bindings: ImageMaterialBindings { records }` + `set_image_material_record_buffer` stand-ins confirmed present and re-pointed on growth (`image_batch.rs:557,584,613`); replace with the real `Handle<ImageExtendedMaterial>`. `ImageBatchKey::depth_bias()` exists (`:93`) for the material.
- Phase 5 (entities): router runs `PostUpdate.after(PanelChildSystems::Build).before(TransformSystems::Propagate).before(BatchResourcesReady)`; the post-Propagate transform system must run AFTER this. the transform system lives in `image_batch.rs`, so it mutates the private `batch.records` field directly (like `update_sdf_batch_world_transforms`, `fill_batch.rs:1301`) — no accessor/setter needed. (Superseded by the Phase 3 Review: the transform is now `panel_matrix * local_transform`, since Phase 5 adds the points→world conversion — `image_record_transform`'s points-space center fold is replaced.)
- Phase 5/8: the module `#[expect(dead_code)]` is still fulfilled after Phase 3 (router consumed `upsert_record`/`retain_records`/`take_empty_batches`; GPU/material helpers + `depth_bias`/`world_bounds`/`remove_record`/`batches_mut`/`get_mut` stay dead). Phase 4 makes `depth_bias` live; Phase 5 makes the GPU helpers live and removes the attribute.

#### Phase 3 Review

- **Coordinate conversion (user-approved, significant):** the whole batch image path was missing the layout-points → world-units + anchor + Y-flip conversion the old entity path did (`reconcile.rs:797-806`); no phase covered it. Assigned to **Phase 5**, mirroring SDF's per-record `local_transform` + world-unit `size` (`fill_batch.rs:341,406-412,438`): the router bakes world-unit geometry, the post-Propagate system composes `panel_matrix * local_transform`. Phase 5 gate gains a world size/position/orientation parity check; Phase 6 `size`/`transform` re-specified as world units.
- **Phase 4/5 material boundary (mechanical):** Phase 4's helpers structurally cannot populate the material (no `key.texture`, no `Assets<ImageExtendedMaterial>` until Phase 5's reconcile). Re-split to match the approved 4/5 swap: Phase 4 = material type + `image_batch_material(key, records)` builder + `MaterialPlugin`; Phase 5 = wire it into `ImageBatchResources`, grow the `allocate`/`grow` signatures for the real material, retire the `ImageMaterialBindings` stand-in.
- **`#[expect(dead_code)]` lifecycle (mechanical):** it is module-level, so it can't be narrowed to items. Corrected Phase 4 to KEEP it unchanged (GPU/material helpers stay dead after Phase 4 → still fulfilled); Phase 5 REMOVES it once the module is fully live.
- **Phase 5 transform accessor (mechanical):** the transform system lives in-module, so it mutates the private `batch.records` directly (like SDF) — removed the earlier "needs an accessor/setter" over-warning.
- **Phase 5 system ordering (mechanical):** image skips material-table register, so SDF's `.after(register_…)` anchor doesn't exist — gave Phase 5 a concrete ordering (post-`Propagate` transform → reconcile → bounds → commit in `BatchResourcesReady`).
- **Name collision + index semantics (mechanical):** qualified `collect_panel_image_records` (new router, keep) vs `collect_panel_image_commands` (old entity path, gate in 7 / delete in 8) in Phases 7-8; carried the `element_idx` (precompose lookup) vs enumerate `command_index` (record key) distinction as a Phase 7 constraint.
- **Phase 7 gate reword (mechanical):** the router is live from Phase 3, so "store holds a record" is already true — re-centered the cutover gate on the entity path going silent (zero `PanelImageChild`, no double-draw).
- **No redundancy:** no remaining phase is redundant; Phases 6, 9-12 unaffected by Phase 3 and remain validly scoped.

### Phase 4 — Image material type + plugin registration  · status: done (uncommitted)

#### Work Order

**Goal:** `ImageExtendedMaterial` — the type, a `image_batch_material(key, records)` builder, and its `MaterialPlugin` — is defined and registered, ready for Phase 5's batch entity to attach; no rendering yet, no wiring into `ImageBatchResources`.

**Spec:**
```rust
ImageExtendedMaterial = ExtendedMaterial<StandardMaterial, ImageExtension>
```
The `StandardMaterial` half owns `base_color_texture` = the batch texture + shared pipeline state. `ImageExtension` binds ONLY the record storage buffer (`#[storage]`). Because that entry is always present, the `MATERIAL_BIND_GROUP_INDEX` layout is never empty and Bevy never strips it — state this as a structural fact; add NO strip logic and NO runtime `Some(texture)` guard. The image family binds NO `material_table`, so it must SKIP both `register_*_batch_materials` and the material-table rebind that SDF/text/Shape perform (`material_table.rs:859`).

**Add a material builder, NOT the resource wiring.** Add `image_batch_material(key: &ImageBatchKey, records: Handle<ShaderBuffer>) -> ImageExtendedMaterial` (mirror `sdf_batch_material`, `fill_batch.rs:864`, used by `spawn_sdf_batch_entity` `:1471`): `base_color_texture = key.texture`, `depth_bias = key.depth_bias()`, `alpha_mode = Blend`, the `#[storage]` record binding = `records`. Do NOT populate `ImageBatchResources.material` and do NOT touch `grow_image_batch_resources`/`set_image_material_record_buffer` here — Phase 5 owns wiring the built material into `ImageBatchResources` (it holds `key.texture` in `reconcile_image_batch_entities`) and retiring the `ImageMaterialBindings` stand-in. The stand-in stays this phase, still dead under the module `#[expect(dead_code)]`.

Register via `ImageBatchPlugin`: add `MaterialPlugin::<ImageExtendedMaterial>::default()` (or the crate's material-plugin convention); the plugin is already in the `render/mod.rs` `add_plugins` tuple from Phase 3.

**Files:**
- image material file (new, e.g. `crates/bevy_diegetic/src/render/image_material.rs`) — `ImageExtension`, `ImageExtendedMaterial`, `image_batch_material`.
- image batch module — reference the material type from the builder (no `ImageBatchResources` change this phase).
- `crates/bevy_diegetic/src/render/mod.rs` — material plugin wiring.

**Constraints from prior phases:** store/records/router from Phases 2-3; GPU `ImageRenderRecord: ShaderType` layout is the storage-buffer row; `ImageBatchKey::depth_bias()` (`image_batch.rs:93`) supplies `StandardMaterial::depth_bias`. `ImageBatchResources` still carries the `ImageMaterialBindings` stand-in re-pointed by `set_image_material_record_buffer` — this phase does NOT touch it (Phase 5 makes it real). The `image_batch` module is under a **module-level** `#[expect(dead_code)]` in `render/mod.rs`; a module-level expect cannot be narrowed to individual items, and the GPU helpers + the new material builder stay dead in a non-test build, so the expectation stays FULFILLED — KEEP the attribute unchanged (do NOT narrow or remove it; Phase 5 removes it once the module is fully live).

**Acceptance gate:** `cargo nextest run -p bevy_diegetic` green; the `clippy` skill clean (module `#[expect(dead_code)]` still fulfilled — do NOT remove it); `cargo build` compiles the material + builder + plugin; no strip logic; no `material_table` binding or register/rebind.

#### Retrospective

**What worked:** `image_material.rs` copy-adapts the SDF/Path material shape faithfully — `ImageExtension` binds ONLY the `#[storage(107, read_only, visibility(vertex, fragment))]` records buffer, `ImageExtendedMaterial = ExtendedMaterial<StandardMaterial, ImageExtension>`, and `image_batch_material(key, records)` mirrors `sdf_batch_material` (`base_color_texture = key.texture`, `depth_bias = key.depth_bias()`, `alpha_mode = Blend`). The four `StandardMaterial` fields codex added beyond the spec (`unlit: true`, `double_sided: true`, `cull_mode: None`, `alpha_mode: Blend`) match the pre-cutover entity path (`reconcile.rs:809-818`) exactly — verified parity, not a regression. Tint correctly stays OUT of `base_color` (the batch design applies it per-record in the Phase-6 shader). `MaterialPlugin::<ImageExtendedMaterial>` registered in `ImageBatchPlugin`; the `image_batch` module `#[expect(dead_code)]` kept, a parallel one added for `image_material`. Build + 620 tests + clippy clean.

**What deviated from the plan:** Codex initially declared all six `MaterialExtension` shader hooks pointing at `embedded://bevy_diegetic/shaders/image_batch.wgsl` — Phase-6 work the spec deferred, and the filename diverged from the plan's `image_panel.wgsl`. Both reviews (blind codex + Claude) caught it; the user instructed Claude to fix it directly. Claude stripped the hooks to an empty `impl MaterialExtension for ImageExtension {}` (defaults) and dropped the now-unused `ShaderRef` import. `MaterialPlugin` was registered inside `ImageBatchPlugin` (`image_batch.rs`), not `mod.rs` as the Work Order Files line implied — `mod.rs` only gained the `mod image_material;` declaration + the second module-level `#[expect(dead_code)]`. Test fixture gained `AssetPlugin` (`MaterialPlugin` requires asset resources).

**Surprises:**
- `ImageExtension` binds at index `107` (continues Path's `100`-`106` numbering) with `visibility(vertex, fragment)`, hardcoded inline — the crate keeps binding consts in `material_table.rs`, but image binds none of them so there is no shared const to reuse.
- An empty `impl MaterialExtension` compiles (all trait methods default) and the material falls back to `StandardMaterial`'s PBR shader until Phase 6 declares the real entry points — so no batch renders correctly between Phase 5 and Phase 6 regardless, which is expected (Phase 7 is the cutover).

**Implications for remaining phases:**
- Phase 5 (entities): wire `image_batch_material(key, records)` into `ImageBatchResources` and retire the `ImageMaterialBindings` stand-in — AND remove BOTH module-level `#[expect(dead_code)]`s (`image_batch` and `image_material`) once the module is live. `AssetPlugin` is already in the `image_batch.rs` test fixture.
- Phase 6 (shader): owns the `MaterialExtension` shader-hook declarations (currently empty) + the WGSL file. Codex's `image_batch.wgsl` reference was removed, so the planned name `image_panel.wgsl` has no conflict — but the shader-path const + `embedded_asset!` registration for it are Phase-6 work (Phase 4 shipped no embedded asset).

#### Phase 4 Review

- **Phase 6 prepass/strip-guard rule (resolved as a determined correctness fact, not a user choice):** SDF/Path carry a stripped-material-group guard + (Path) `enable_prepass() = false` because they can be `Opaque`/`Mask` and hit the depth-only opaque prepass. Images are always `AlphaMode::Blend` (existing Delegation Context invariant), so they never enter that prepass and keep their material bind group on the shadow pipeline — `@binding(107)` survives everywhere. Folded into Phase 6 Spec + Constraints: do NOT copy the strip guard, do NOT override `enable_prepass()`; shadow alpha stays the prepass-fragment `discard` already specified.
- **Phase 6 embedded-asset gap (mechanical):** Phase 4 shipped no WGSL/embedded asset and left `impl MaterialExtension` empty. Added to Phase 6 Files/Spec/gate: fill the shader hooks, add `IMAGE_PANEL_SHADER_PATH` const, and register `image_panel.wgsl` via `embedded_asset!` in `ImageBatchPlugin::build` (pattern `analytic_paths/mod.rs:83`).
- **Phase 6 binding + empty-impl starting state (mechanical):** carried `@binding(107)` (hardcoded inline, no shared const) and "the `MaterialExtension` impl is empty" into Phase 6 Constraints so the WGSL reads the right binding and the delegate knows it fills the hooks.
- **Phase 5 dual dead-code expects (mechanical):** Phase 4 added a SECOND module-level `#[expect(dead_code)]` (`mod image_material;`). Phase 5 Spec/Files/Constraints now say remove BOTH (`image_batch` AND `image_material`), not "the" one.
- **Phase 5 coordinate-Z parity (mechanical):** the entity path placed a non-zero local Z (`TEXT_Z_OFFSET`, `reconcile.rs:823`). Phase 5 Spec now folds `TEXT_Z_OFFSET` into `local_transform`'s Z for world-position parity; gate check updated.
- **Phase 5 manual-AABB now mandatory (mechanical):** image batches are cross-panel, so the `NoAutoAabb` + manual `Aabb` cull path (previously "only if…") is required, not optional. Phase 5 ordering + Files updated to commit to it.
- **Phase 5 between-phase safety invariant (mechanical):** recorded in the Phase 5 gate that the batch entity it spawns produces no visible draw before Phase 6/7 (empty extension → PBR fallback over the all-zero inert mesh, `draw_batch_family` not yet flipped), so no double-draw with the still-live entity path.
- **No redundancy:** Phases 5-12 all retain full scope; Phase 4 shipped only the material type + builder + plugin. Phases 7-12 unaffected (named files/line refs unchanged).

### Phase 5 — Batch entities + GPU buffers + mesh + transform/bounds  · status: done (uncommitted)

#### Work Order

**Goal:** Image batches get a batch entity (inert mesh + real `ImageExtendedMaterial` + records buffer), the layout-points → world-units conversion, post-Propagate world transforms (both dirty flags), and world bounds — reusing the GPU helpers shipped in Phase 2.

**Spec:**
`reconcile_image_batch_entities`: spawn/despawn one batch entity per `ImageBatchKey`, mirroring `spawn_sdf_batch_entity` (`fill_batch.rs:1455`) — attach the inert batch mesh, the `ImageExtendedMaterial` built via Phase 4's `image_batch_material(key, records)`, `RenderLayers` from `key.layers`, and `NotShadowCaster` per `key.shadow`. **Reuse** the GPU helpers Phase 2 already shipped — `allocate_image_batch_resources`, `grow_image_batch_resources`, `commit_image_batch_records` — and the `record_upload`/`bounds_update` `Dirty` flags; do NOT re-implement their bodies.

**Wire the real material (replaces the Phase-2 stand-in).** Phase 4 defined the material + builder but did NOT wire it. This phase: extend `ImageBatchResources` with `material: Handle<ImageExtendedMaterial>` and REMOVE the `ImageMaterialBindings` stand-in + `set_image_material_record_buffer` (both fail the `clippy` dead-code deny once the module goes live). `allocate_image_batch_resources` and `grow_image_batch_resources` gain a `&mut Assets<ImageExtendedMaterial>` param (and the `key`/texture, available in `reconcile_image_batch_entities`) so they can build/re-point the real material — `grow` re-points the material's `#[storage]` record binding to the new buffer (this is the SDF `grow_sdf_batch_assets` shape, `fill_batch.rs:1500`, which already takes `materials`). Signature changes to these two helpers are expected and in-scope; only their growth/capacity LOGIC is the "do not re-implement" part.

**Add the inert batch mesh Phase 2 did not ship.** Vertex-pull needs a mesh sized to `capacity` quads (`capacity*4` verts / `capacity*6` indices), like `inert_sdf_batch_mesh` (`fill_batch.rs:1414`). Extend `ImageBatchResources` with `mesh: Handle<Mesh>`; `grow_image_batch_resources` regenerates it alongside the record buffer on growth (mirror `grow_sdf_batch_assets`).

**Port the layout-points → world-units conversion (was missing from the whole batch path).** The shipped router (`image_batch.rs`) stores RAW layout points; the old entity path scaled + anchored + Y-flipped them (`reconcile.rs:797-806`: `width * points_to_world`, `x*points_to_world + w/2 - anchor_x`, `world_y = -(y*points_to_world + h/2 - anchor_y)`). Mirror SDF's approach (`fill_batch.rs:341,406-412`): add a per-record `local_transform: Transform` (anchored, Y-flipped center in world units) + a world-unit `size`. The router's record build (`bounds_from_command`/`collect_panel_image_records`) reads the panel's `points_to_world` + anchor (same `ImageGeometry`-equivalent source the entity path used) and bakes world-unit `size` + `local_transform`; leave the panel-`GlobalTransform` compose to the transform system below. `points_to_world` is a separate layout scale NOT baked into the panel `GlobalTransform` (`diegetic_panel.rs:564`), so it MUST be applied here — a raw-points record renders enormous, un-anchored, and upside-down. The entity path also placed a non-zero LOCAL Z (`Transform::from_xyz(world_x, world_y, TEXT_Z_OFFSET)`, `reconcile.rs:823`) that lifts the image off the panel surface; fold `TEXT_Z_OFFSET` into `local_transform`'s translation Z so world-POSITION parity holds. The depth-ORDERING levers (`depth_bias` from `z_index_rank`, `oit_depth_offset`) are separate and do NOT replace this geometric Z.

`update_image_batch_world_transforms` runs after `TransformSystems::Propagate`: cross-panel per-record membership forces a per-record transform lookup (one texture used by two panels forms one batch holding records from both; a batch entity gets no transform propagation, so a moving panel would otherwise leave its image behind). It lives in `image_batch.rs`, so it iterates the PRIVATE `batch.records` field directly (exactly like `update_sdf_batch_world_transforms`, `fill_batch.rs:1301` — no accessor/setter needed). Set each record's world transform to `panel_global_transform.to_matrix() * local_transform.to_matrix()` (mirror `fill_batch.rs:438`); it MUST mark BOTH `record_upload` AND `bounds_update` dirty when the transform changes. NOTE: this supersedes the earlier "set `.transform` = panel matrix ONLY" wording — with `local_transform` now carrying the anchored world-unit center, the transform is the panel matrix COMPOSED with `local_transform`, and `image_record_transform` no longer folds a points-space center.

`update_image_batch_bounds`: reuse the shipped `ImageBatch::world_bounds()` (recompute from the world-unit `size` + composed transform).

**System ordering (do NOT copy SDF's `.after(register_…)` chain — image skips register).** SDF orders reconcile/bounds/commit `.after(register_sdf_batch_materials::<…>)` (`fill_batch.rs:1178-1199`); image binds no `material_table` and skips register, so that anchor does not exist. The router already runs `PostUpdate.after(PanelChildSystems::Build).before(TransformSystems::Propagate).before(BatchResourcesReady)` (`image_batch.rs:419`). Order this phase's systems: `update_image_batch_world_transforms` `.after(TransformSystems::Propagate)`, then `reconcile_image_batch_entities` → `update_image_batch_bounds` → `commit_image_batch_records`, all `.in_set(BatchResourcesReady)`. Spawn the batch entity with `NoAutoAabb` + a manual `Aabb::default()` and include the `VisibilitySystems::CalculateBounds`/`CheckVisibility` ordering constraints (`fill_batch.rs:1175,1188,1197,1481-1482`) — this is MANDATORY, not optional: image batches are cross-panel (one texture, records from many panels), so a single mesh's auto-AABB cannot bound them; `update_image_batch_bounds` writes the manual `Aabb` (mirror `update_sdf_batch_bounds`).

**Files:**
- image batch module — `reconcile_image_batch_entities` + transform + bounds systems; extend `ImageBatchResources` with `mesh` + `material`; retire `ImageMaterialBindings`/`set_image_material_record_buffer`; add `local_transform`/world-unit `size` to the record + the router-side points→world conversion; add an inert-mesh builder + growth regen; grow/allocate signature changes for the real material.
- `crates/bevy_diegetic/src/render/mod.rs` — system ordering (post-`Propagate` transform system + `BatchResourcesReady` set); remove BOTH module-level `#[expect(dead_code)]`s — one on `mod image_batch;`, one on `mod image_material;` (Phase 4 added the second) — now that the module is fully live.

**Constraints from prior phases:** store + GPU helpers (`allocate`/`grow`/`commit`) + `ImageBatch::world_bounds()` + the two `Dirty` flags from Phase 2; router populates the store from Phase 3 and stores RAW POINTS (this phase adds the world conversion); `ImageExtendedMaterial` + `image_batch_material(key, records)` builder + its `MaterialPlugin` from Phase 4 (NOT yet wired into `ImageBatchResources` — this phase wires it and removes the `ImageMaterialBindings` stand-in). `ResolvedImageRecord::new` stamps `transform: IDENTITY`; the router re-upserts each frame, so Phase 3's transform carry-over PLUS this post-Propagate system keep static batches from re-uploading every frame. This phase makes the store's GPU helpers + material fully live — REMOVE BOTH module-level `#[expect(dead_code)]`s in `render/mod.rs` (`mod image_batch;` AND `mod image_material;` — Phase 4 added the second, since its material builder stays dead until this phase wires it) (run the `clippy` skill; `unfulfilled_lint_expectations` is denied, so a stale expect now fails).

**Acceptance gate:** `cargo nextest run -p bevy_diegetic` green + the `clippy` skill clean (module `#[expect(dead_code)]` removed, no dead `ImageMaterialBindings`) + tests: batch entity reconcile on key add/remove; buffer + inert mesh growth keeps capacity stable and re-points the real material record binding; cross-panel same-texture places each record at its own panel transform; batch world-bounds correctness; a static (unchanged) batch does NOT re-upload across frames (the Phase-3 transform carry-over holds); **a batched image resolves to the same world size / position / orientation as the pre-cutover entity path** (`points_to_world` + anchor + Y-flip + `TEXT_Z_OFFSET` applied — guards the coordinate conversion). Between-phase safety invariant: the batch entity Phase 5 spawns produces NO visible draw before Phase 6/7 — Phase 4's `MaterialExtension` impl is empty, so it falls back to `StandardMaterial`'s PBR mesh vertex stage over the all-zero inert mesh (every quad collapses to zero area), and `draw_batch_family(Image)` is not yet flipped; the still-live entity path is the sole image draw, so there is no double-draw. The unused `#[storage(107)]` entry in the kept material group is harmless to the PBR shader. A future reorder that populates real vertex positions before Phase 6's shader lands would break this — keep the degenerate mesh until the vertex-pull shader exists.

#### Retrospective

**What worked:** `spawn_image_batch_entity`, `grow_image_batch_resources`, and `update_image_batch_bounds` are byte-faithful mirrors of `spawn_sdf_batch_entity`/`grow_sdf_batch_assets`/`update_sdf_batch_bounds` (`fill_batch.rs:1455,1500,1543`) — same component tuple (`DiegeticImageBatch`, `Mesh3d`, `MeshMaterial3d`, `Visibility::Inherited`, `NoAutoAabb`, `Aabb::default()`, layers; `NotShadowCaster` on `VisualShadow::None`), same capacity-doubling growth, same bounds body (`center=(min+max)*0.5`, `*transform`+`*global`+`*aabb`). `local_transform_from_bounds` reproduces the entity-path formula (`reconcile.rs:797-806`) exactly with `TEXT_Z_OFFSET` folded into local-Z; a parity test (`batched_image_matches_legacy_entity_coordinate_conversion`) pins size + local translation + composed world matrix against a recomputed legacy geometry. `ImageMaterialBindings` stand-in retired for `material: Handle<ImageExtendedMaterial>`; both module-level `#[expect(dead_code)]` removed. Both reviews clean (blind codex APPROVE, no findings; Claude nits only). Build + 625 tests (13 new) + clippy + doc green.

**What deviated from the plan:** `ImageBatchPlugin::build` gained `.init_asset::<Mesh>().init_asset::<ShaderBuffer>()` (idempotent in the production `RenderPlugin` stack; makes the `MinimalPlugins` test app self-contained). `grow_image_batch_resources` added an `else`-branch that rebuilds + re-inserts the material when `materials.get_mut` misses (SDF only re-points, no else) — harmless extra robustness. The per-batch upload helper stayed named `commit_image_batch_records` (Phase 2) and a new `commit_image_batch_buffers` SYSTEM wraps it per the SDF `commit_sdf_batch_buffers` shape.

**Surprises:**
- The transform system is `update_image_batch_world_transforms` (post-`Propagate`); it sets `record.transform = panel_global.to_matrix() * local_transform.to_matrix()` (WORLD-absolute per record), marks BOTH dirty flags only when the composed matrix changed. `image_record_transform(record)` just returns the stored `record.transform` (no points-space fold — Phase 3's `image_record_transform` center-fold is fully gone).
- The batch entity's own `Transform`/`GlobalTransform` is set to the world-bounds CENTER (mirrors SDF) and its `Aabb` is center-zero + half-extents — this drives ONLY visibility culling; Phase 6's vertex-pull shader must read `record.transform` (world-absolute) and IGNORE the mesh model matrix, exactly as SDF's shader does. This is the critical Phase 6 contract.
- System order lands as: `route_image_batch_records` (before `Propagate`) → `update_image_batch_world_transforms` (after `Propagate`) → `reconcile_image_batch_entities` (before `CalculateBounds`) → `update_image_batch_bounds` (after `CalculateBounds`, before `CheckVisibility`) → `commit_image_batch_buffers` (after `CheckVisibility`), all `.in_set(BatchResourcesReady)` except the router.

**Implications for remaining phases:**
- Phase 6 (shader): the batch entity carries a non-identity `GlobalTransform` (bounds center); the WGSL vertex stage MUST use `record.transform` directly and NOT compose the mesh model matrix, or images double-transform. The concrete system/type names are now fixed: `ImageBatchResources { records, mesh, material, capacity }`, `ImageRenderRecord` (SHADER_SIZE 128, fields `transform,size,uv_rect,tint,clip_depth_nudge,oit_depth_offset`), `DiegeticImageBatch` marker, `ImageExtendedMaterial` bound at `@binding(107)`. Both `#[expect(dead_code)]` are gone, so Phase 6 adds no expect churn.
- Phase 7 (cutover): the batch entity is spawned and live from Phase 5 but draws nothing (empty extension → PBR over all-zero mesh); the flip + gate is the only remaining step to make images draw through the batch.
- Phase 10 (generic collapse): SDF and Image now share the post-`Propagate` transform system + the byte-identical spawn/grow/bounds bodies — confirming them as the per-record template pair the extension-point set must cover.

#### Phase 5 Review

- **Phase 6+7 merged (user-approved, significant):** the batch entity is spawned live from Phase 5 and draws nothing only because its shader is an empty placeholder; the instant Phase 6's real shader lands, it draws real quads while the old entity path is still live → double-draw. Merged the atomic cutover (flip `draw_batch_family` + gate the old collector) into Phase 6 so shader + flip + gate are one commit. Phase 7 is now a "merged" stub; downstream "after Phase 7" refs in Phases 8-9 repointed to "Phase 6 cutover"; the vestigial "activation" file/step dropped (the entity already exists).
- **Phase 6 model-matrix contract (significant, resolved as determined fact):** the batch entity carries a non-identity `GlobalTransform` (world-bounds center, culling only), so the WGSL vertex stage MUST use `record.transform` as the full world transform and NOT compose the mesh model matrix (mirror `sdf_panel.wgsl:226`) — folded into Phase 6 Spec + Constraints + a code-review gate item; no render test can catch it under `MinimalPlugins`.
- **Phase 6 size + vertex-pull index (mechanical):** `ImageRenderRecord.size` is the FULL extent (not SDF's half-size), so the shader multiplies by `0.5`; record index = `(vertex_index - mesh[instance_index].first_vertex_index) / 4u`, read from `@binding(107)` (no binding-108/`mesh_records` buffer); inert-mesh winding is byte-identical to SDF so its corner→sign + `box_uv` port unchanged.
- **Phase 7/8 line-ref drift (mechanical):** corrected `collect_panel_image_records` `:476→:525`, precompose `element_idx` lookup `:526→:583`, router `:478`.
- **Phase 8 guardrail (mechanical):** noted the Phase 5 coordinate helpers (`local_transform_from_bounds`/`image_size_from_bounds`/`linear_tint` in `image_batch.rs`) are the NEW path and MUST survive deletion — only the old `reconcile.rs` geometry is removed.
- **Phase 10 extension points (mechanical):** added three facts to the trait's extension-point set — the `grow`/material-rebind hook must tolerate both re-point and rebuild-else; commit is a helper+system split; `type Dirty` may be over-abstraction for the per-record pair (both SDF+Image use the identical two-flag `Dirty`).
- **No redundancy:** Phases 8-12 retain full scope; Phase 9 (border ordering) and Phases 11-12 are untouched by Phase 5.

### Phase 6 — Pipeline specialization + WGSL shaders + atomic cutover  · status: done (uncommitted)

> **Merged (user-approved):** the former Phase 7 (atomic cutover) is now the last section of this phase. The image batch entity has been spawned and live since Phase 5 but draws nothing (empty shader → PBR over an all-zero mesh). The instant this phase's shader lands, that entity draws real quads — so the routing flip + old-entity-path gate MUST land in the SAME commit as the shader, or images double-draw between commits. Shader + flip + gate = one atomic commit.

#### Work Order

**Goal:** The image shader draws one quad per record across main / camera-prepass / shadow passes, with correct alpha, tint, and depth, AND images render EXCLUSIVELY through the batch path — the shader, the `draw_batch_family` flip, and the old entity-path gate land in one atomic commit (no double-draw, no no-draw window).

**Spec:**
Specialization declares main + camera-prepass + shadow vertex entry points (mirror `SdfExtension`, `fill_batch.rs:807-813`); each pulls geometry from the record buffer over the inert batch mesh built in Phase 5 (vertex-pull), else it rasterizes a degenerate mesh. The camera/shadow prepass fragment shader samples the texture alpha and `discard`s (pattern: `sdf_panel.wgsl` `fill_alpha_for_prepass` ~:312-342) — correct image-shadow alpha comes from this discard, NOT from an alpha-mode helper.

**Fill the empty `MaterialExtension` impl + register the shader (Phase 4 shipped neither).** Phase 4 left `impl MaterialExtension for ImageExtension {}` empty and shipped NO WGSL/embedded asset. This phase: (a) declare the shader-hook methods (`vertex_shader`/`fragment_shader`/`prepass_vertex_shader`/`prepass_fragment_shader`, plus `deferred_*` if matching the crate convention) returning an embedded path const; (b) register the shader as an embedded asset — `embedded_asset!(app, "shaders/image_panel.wgsl")` in `ImageBatchPlugin::build` + an `IMAGE_PANEL_SHADER_PATH` const holding the `embedded://…` path (pattern: `analytic_paths/mod.rs:83` + `constants.rs`; SDF mirrors). Without the `embedded_asset!` registration the `ShaderRef` hooks resolve to nothing.

**No strip guard, no `enable_prepass()` override — image is always `Blend`.** SDF and Path carry `material_group_is_stripped` / `SDF_STRIPPED_MATERIAL_GROUP` specialization and (Path) `enable_prepass() = false` (`fill_batch.rs:826-850`, `analytic_paths/material.rs:186,199-244`) because they can be `Opaque`/`Mask` and hit the depth-only OPAQUE prepass, where Bevy substitutes an empty material bind group and vertex-pull's storage bindings vanish. Images are ALWAYS `AlphaMode::Blend` (Delegation Context invariant): they render in the transparent/OIT phase — never the opaque depth-only prepass — and a `Blend` (MAY_DISCARD) material KEEPS its material bind group on the shadow pipeline, so `@binding(107)` survives on every pipeline the image material actually compiles. Do NOT copy the strip guard and do NOT override `enable_prepass()`; the shadow-alpha `discard` above is the only prepass-family concern.

WGSL vertex/fragment: the GPU record carries `transform` (`panel_world * local_transform`, where `local_transform` from Phase 5 holds the anchored, Y-flipped center in WORLD units) and `size` (WORLD-unit quad size, already scaled by `points_to_world`) — there is NO raw bounds, points-space center, or `half_size` to reconstruct in the shader. **`record.transform` is the FULL world transform — the vertex stage MUST output `clip = view_proj * record.transform * vec4(local, 0, 1)` and NEVER compose the mesh model matrix (do NOT call `position_local_to_world`), exactly as `sdf_panel.wgsl:226` does.** Phase 5 sets the batch entity's own `GlobalTransform` to the world-bounds CENTER (`update_image_batch_bounds`) purely for visibility culling; composing that model matrix on top of `record.transform` double-transforms every image. Build a quad at the origin from `size`, but note `size` is the FULL world-unit extent (NOT SDF's already-halved `mesh_half_size`), so multiply by `0.5` for the corner offsets (`local = signs * record.size * 0.5`). Derive the record index from the pulled vertex like SDF — `record_index = (vertex_index - mesh[instance_index].first_vertex_index) / 4u` (`sdf_panel.wgsl:205`); image has NO `mesh_records`/binding-108 buffer, so read the record straight from `@binding(107)`. The inert mesh winding is byte-identical to `inert_sdf_batch_mesh` (`fill_batch.rs:1419`), so SDF's corner→sign mapping and `box_uv` derivation port unchanged. Apply `transform`; sample the batch texture with `uv_rect`; multiply by the record tint AFTER the hardware sRGB decode (linear `Vec4`, mirror `linear_color`, `fill_batch.rs:1940`); apply `ClipDepthNudge` in the vertex path for non-OIT; apply `OitDepthOffset` in the OIT path; preserve the current image `Blend` alpha/depth behavior.

**Atomic cutover (former Phase 7 — MUST land in the same commit as the shader above).** Add the `DrawBatchFamily::Image` variant (`layout/render.rs:69`); `draw_batch_family()` returns `Some(Image)` for `RenderCommandKind::Image` and `PrecomposeLdr` (was `None`, `:143`). Gate the OLD entity-path collector `collect_panel_image_commands` (`reconcile.rs:674`) so it yields nothing once `draw_batch_family(kind).is_some()` (model: `panel_shapes:825`) — do NOT confuse it with the NEW router collector `collect_panel_image_records` (`image_batch.rs:525`), which is unaffected. There is NO separate "activation" step: the batch entity is already spawned/live from Phase 5, so flipping `draw_batch_family` + gating the old collector is the entire cutover. Land the shader + the flip + the gate as ONE commit — the shader alone (no flip) leaves the old path drawing too (double-draw: doubled alpha + shadow casters); the flip alone (no shader) draws nothing over the still-degenerate mesh (no-draw window).

**Files:**
- `crates/bevy_diegetic/src/shaders/image_panel.wgsl` (new).
- `crates/bevy_diegetic/src/render/image_material.rs` — fill the empty `impl MaterialExtension for ImageExtension` with the shader hooks + `specialize` (vertex-pull swap); add the `IMAGE_PANEL_SHADER_PATH` const.
- `crates/bevy_diegetic/src/render/image_batch.rs` — `embedded_asset!` registration in `ImageBatchPlugin::build`.
- `crates/bevy_diegetic/src/layout/render.rs` — `DrawBatchFamily::Image` variant + `draw_batch_family()` routing (`:69`, `:143`).
- `crates/bevy_diegetic/src/render/panel_text/reconcile.rs` — gate the OLD `collect_panel_image_commands` entity-path collector on `draw_batch_family(kind).is_some()`.

**Constraints from prior phases:** `ImageExtendedMaterial`/`ImageExtension` from Phase 4 — its `impl MaterialExtension` is EMPTY (Phase 4 stripped premature shader hooks) and it binds the records at `#[storage(107, read_only, visibility(vertex, fragment))]` (hardcoded inline, no shared const), so the WGSL reads the record buffer at `@binding(107)` in the material group; the inert batch mesh from Phase 5; GPU record layout (`transform`, `size`, `uv_rect`, `tint`, `clip_depth_nudge`, `oit_depth_offset`) from Phase 2, with `transform`/`size` now in WORLD units (Phase 5's points→world conversion) — the shader does NO points scaling. Phase 4 shipped NO embedded asset for the image shader (this phase registers `image_panel.wgsl`). Image is always `AlphaMode::Blend`, so NO stripped-material-group guard / `enable_prepass()` override is needed (unlike SDF/Path). **The Phase 5 batch entity carries a NON-IDENTITY `GlobalTransform`** (world-bounds center, culling only) and `ImageRenderRecord.size` is the FULL world-unit extent (`centered_corners` halves it CPU-side for bounds) — the shader ignores the model matrix and halves `size` itself (see Spec). Concrete shipped names: `ImageBatchResources { records, mesh, material, capacity }`; `ImageRenderRecord` (SHADER_SIZE 128; `transform, size, uv_rect, tint, clip_depth_nudge, oit_depth_offset`); `DiegeticImageBatch` marker; the record buffer at `@binding(107)`. Both module-level `#[expect(dead_code)]` are gone (Phase 5), so this phase adds none. **Cutover:** the batch entity is already live from Phase 5, so the flip is the only remaining draw-source change; preserve the router's two distinct indices (Phase 3) — the precompose lookup keys on `command.element_idx` (`image_batch.rs:583`, `PanelPrecomposeCache::entry`) while `ImageRecordKey.command_index` uses the `enumerate()` index (`route_image_batch_records` at `image_batch.rs:478`); do not conflate them when touching the gate.

**Acceptance gate:** `cargo nextest run -p bevy_diegetic` green; `cargo build` compiles the WGSL + specialization (all three vertex entry points + prepass discard fragment present); `image_panel.wgsl` is registered via `embedded_asset!` and the `MaterialExtension` shader hooks resolve to it. Code-review check (no render test exists under `MinimalPlugins`): the vertex shader multiplies `record.transform` by the origin quad directly and does NOT compose the mesh model matrix, and halves `size` for the corner offsets. Cutover tests: with the flip on, the OLD `collect_panel_image_commands` yields zero `PanelImageChild` (entity path goes silent — the critical no-double-draw check; the router already populated the store in Phase 3, so store-holds-a-record is NOT the new assertion); two overlapping same-texture records at different `DrawOrderIndex` composite in draw order with OIT disabled (proves `sort_records`); precompose output renders (visual parity with pre-cutover).

#### Retrospective

**What worked:** `image_panel.wgsl` mirrors SDF vertex-pull faithfully — record index from `(vertex_index - mesh[instance_index].first_vertex_index) / 4u`, `record.transform` used as the FULL world transform via `position_world_to_clip(record.transform * vec4(local,0,1))` with NO mesh-model-matrix compose (the critical Phase 5 contract), origin quad `local = signs * record.size * 0.5` (halves the full-extent `size`), byte-identical `box_uv` + winding to `inert_sdf_batch_mesh`. No stripped-material-group guard and no `enable_prepass()` override (image is always `Blend`), exactly as specified. Atomic cutover is a 3-line gate (`if cmd.kind.draw_batch_family().is_some() { return None; }`) in `collect_panel_image_commands` + the `DrawBatchFamily::Image` variant + `draw_batch_family()` returning `Some(Image)` for `Image`/`PrecomposeLdr` — shader + flip + gate in one change. Build + 625 tests + clippy green.

**What deviated from the plan:** The `embedded_asset!` registration codex placed in `ImageBatchPlugin::build` (`image_batch.rs`, per the Work Order Files line) resolved to the WRONG asset path and was moved to `lib.rs` (Claude fix, user-authorized). `embedded_asset!(app, "src", "../shaders/image_panel.wgsl")` from `src/render/` makes bevy's `_embedded_asset_path` join `render` + `../shaders/...` into `bevy_diegetic/render/../shaders/image_panel.wgsl` — bevy NEVER normalizes the `..` (neither `_embedded_asset_path` nor `MemoryAssetReader`/`Dir` collapse it; `Dir::insert_asset` creates a literal `..` dir component), so the material's `IMAGE_PANEL_SHADER_PATH` load of the clean `bevy_diegetic/shaders/image_panel.wgsl` missed → shader unresolved → images draw nothing. Both reviews (blind codex + Claude) independently caught this as the sole blocker. Fix: register in `lib.rs` next to `sdf_panel.wgsl` (`embedded_asset!(app, "shaders/image_panel.wgsl")`, clean path from the `src` root), drop the `image_batch.rs` registration + its `use bevy::asset::embedded_asset;` import. Codex also DELETED the three old entity-path image tests + helpers (`tint_only_change…`, `unchanged_image_material…`, `command_index_shift…`, `record_modified_materials`, `ModifiedMaterials`, `image_reconcile_app`, etc.) and replaced them with one cutover test (`image_batch_family_commands_do_not_spawn_legacy_children`) — the gate makes those tests' subjects unspawnable, so they could not stay; this is Phase 8's deletion scope pulled forward.

**Surprises:**
- **bevy `embedded_asset!` gotcha:** registering a shader that lives in a DIFFERENT directory than the calling file (via a `../` path) produces an un-normalized asset path that a clean `embedded://` load can never match. Register from a file whose directory is an ancestor of (or equal to) the shader's directory — e.g. the crate root `lib.rs` for `src/shaders/*` — so no `..` appears. This class of bug is INVISIBLE to `cargo nextest` under `MinimalPlugins`, which never loads/compiles the shader through the render pipeline — code review is the only gate.
- The record index reaches the fragment through the `uv_b` (UV_1) interpolant (`out.uv_b = vec2(f32(record_index), 0)`; fragment recovers it via `u32(floor(in.uv_b.x + 0.5))`). All four quad corners carry the same index so interpolation is exact. `inert_image_batch_mesh` already carries `ATTRIBUTE_UV_1` (byte-identical to `inert_sdf_batch_mesh`), so `VERTEX_UVS_B` is defined and the varying compiles; `ATTRIBUTE_UV_0` drives `VERTEX_UVS_A` for texture sampling. `specialize` is a no-op `Ok(())` — entry-point names (`vertex`/`fragment`) match bevy defaults and the shader-hook `ShaderRef`s override the stages, so no pipeline mutation is needed.
- `deferred_vertex_shader`/`deferred_fragment_shader` also return the image shader path; harmless because a `Blend` material never enters the deferred pass.
- No image example exists (`.image(` is unused in `examples/`), so there is no cheap runtime repro; correctness of the shader-load path rests on the path-resolution trace + code review, not a screenshot.

**Implications for remaining phases:**
- **Phase 8 (deletion):** the three old-image tests + their helpers at `reconcile.rs:1444-1668` (`record_modified_materials`, `ModifiedMaterials`, `image_reconcile_app`, `one_image_tree`, `two_image_tree`, `single_image_child`, etc.) are ALREADY deleted by Phase 6 — Phase 8 must NOT try to re-delete them. Phase 8 still deletes the runtime entity path (`PanelImageChild` `:526`, `ReusableImageChild` `:545`, `ImageVisuals` `:552`, `ImageGeometry` `:718`, `reconcile_panel_image_children` `:562`, `reconcile_existing_image` `:734`, `apply_image_shadow_casting` `:780`, `build_image_visuals` `:793`) and the `reconcile_ms` coupling + stale comments. `collect_panel_image_commands` (`reconcile.rs:674`) is IMAGE-ONLY — its exactly two callers are `reconcile_panel_image_children` (`:604`, deleted by Phase 8) and the guard test `image_batch_family_commands_do_not_spawn_legacy_children` (`:1481`); text uses a different function (`collect_text_commands`). So once Phase 8 deletes the entity system, `collect_panel_image_commands` is orphaned and its gate is vacuous (nothing can spawn legacy children) — Phase 8 deletes BOTH the function AND that guard test. The dead-code deny (`clippy`) forces this; keeping either would fail the gate.
- **Phase 10 (generic collapse):** image + SDF now share a concrete vertex-pull shader shape (record index from `vertex_index`, `record.transform` as full world transform, `@binding(107)` records over an inert UV0+UV1 mesh) — the shader is the per-record render template, not just the CPU store. The embedded-shader-registration location is a per-family concern (SDF registers `sdf_panel.wgsl` in `lib.rs`; image now does too).

#### Phase 6 Review

Mechanical Work Order edits applied to remaining phases (from the architect pass): Phase 8 delete-list refreshed to current line refs and told NOT to re-delete the image tests Phase 6 already removed; Phase 8 Files corrected to wire-out in `panel_text/mod.rs` (not `render/mod.rs`); Phase 9 gate reworded to a data-level ordering assertion (no headless pixel harness) and marked independent of the generic collapse; Phase 10 constraints noting Image is fully shipped after Phase 6 and depends on Phases 2-6, not 8/9.

Two Phase-6 defects surfaced AFTER the dual review (both missed by it):
- **Schedule-overlap panic (fixed, uncommitted):** `route_image_batch_records` was ordered `.after(PanelChildSystems::Build).before(BatchResourcesReady)`. The panel-shape batch systems (`panel_shapes/mod.rs:34-35`) are members of BOTH `PanelChildSystems::Build` AND `BatchResourcesReady`; that dual membership was legal only while the two sets had no ordering. The router's new edge made `Build → BatchResourcesReady` transitively ordered, contradicting the shape systems' membership → `SetsHaveOrderButIntersect`, and `batch_validation` panicked at schedule init. Fixed by anchoring the router to the precompose cache systems it actually reads (`.after(cleanup_retired_precompose_images)`) instead of the whole `Build` set; dropped the now-unused `PanelChildSystems` import. This class of bug is invisible to `cargo nextest` (the 625-test suite builds the schedule per-plugin, never the full app schedule) — only launching the example caught it.
- **F7 verification gate (resolved → new Phase 7b, done):** the batch path had never rendered a pixel — no image example existed, and Phase 6's sole blocker was invisible to the test suite. Per user redirect, Phase 7b extends `batch_validation` to draw real images through the batch path AND validate the image family in the info panel, sequenced BEFORE Phase 8 deletes the entity path (fallback + A/B reference). Implemented and confirmed on screen (four tinted images render; the info panel's image family latches "records routed: ok"). This is also Phase 9's visual-confirmation channel.

### Phase 7 — (merged into Phase 6)  · status: merged

Atomic cutover was merged into Phase 6 (user-approved) — the routing flip + old-entity-path gate must land in the same commit as the shader, because the Phase 5 batch entity draws real quads the instant the shader exists. See Phase 6's "Atomic cutover" Spec section + cutover gate. No separate dispatch: `/plan:delegate … phase 6` covers shader + cutover; the next dispatch after Phase 6 is Phase 7b, then Phase 8.

### Phase 7b — Runtime verification (batch_validation image panel + image family diagnostics)  · status: done (uncommitted)

> **Added (user redirect, resolving the Phase 6 review's F7):** batched images had never been drawn on screen; this phase proves they render — and are batched/counted correctly — in the existing `batch_validation` harness, WHILE the legacy entity path still exists as the fallback and A/B reference. Must land before Phase 8's deletion.

#### Work Order

**Goal:** `batch_validation` draws real images through the batch path and reports the image family in its left info panel, so a launched run visually confirms batched images render (the only gate that catches a shader-load-class bug like Phase 6's blocker) and the routing invariants latch green for the image family.

**Spec (as built):**
- **Crate perf plumbing.** Added `image_breakdown: Vec<BatchSummary>` to `DiegeticPerfStats` (`panel/perf.rs`), populated in `commit_image_batch_buffers` (`image_batch.rs`): clear + one `BatchSummary` per live image batch, built by a new `image_batch_summary(key, record_count)` — image keys carry no `PipelineCompatibility`/`ResourceCompatibility`, so it fills `BatchSummary` directly (`unlit: true`, `alpha_mode: "Blend"`, `textured: true`, layers/shadow/z-index from the key). `commit_image_batch_buffers` gained `ResMut<DiegeticPerfStats>`; `ImageBatchPlugin::build` now `init_resource::<DiegeticPerfStats>()` so the plugin is self-contained (idempotent in production; required by the `image_batch` test fixture).
- **Example — bottom-right panel.** Replaced `build_mixed_panel` (the "Mixed stack" card, index 3 / bottom-right) with `build_image_panel(handle)` drawing four `LayoutBuilder::image(...)` cards that all sample the loaded `array_texture.png` (plain + green/blue/red tint) — one texture → one image batch of four records, demonstrating per-record tint stays in-batch. Deleted the now-dead `mixed_row`/`mixed_row_body`/`mixed_shape_row`/`mixed_shape_group` helpers + `MIXED_ROW_BG`/`MIXED_LABEL_WIDTH` consts + the unused `PanelShape` import.
- **Example — info panel.** `family_breakdowns` returns `[FamilyBreakdown; 4]` (added "image", red, counts derived from `image_breakdown`); `LEDGER_FAMILY_COLORS` and the ledger header/`ledger_row` widened to 4 columns; the stabilization latch (`ValidationStatus::last_observed`, `validate_batch_counts`) tracks the image batch count too; added an "image records" line to the record-detail section. The per-family breakdown loop and `batch_invariant_failures` pick up image automatically, so the image family is validated (rows = draw count, all records routed, no empty batch).

**Files:**
- `crates/bevy_diegetic/src/panel/perf.rs` — `image_breakdown` field.
- `crates/bevy_diegetic/src/render/image_batch.rs` — populate it in `commit_image_batch_buffers` + `image_batch_summary` helper + plugin `init_resource`.
- `crates/bevy_diegetic/examples/batch_validation.rs` — image panel + 4-family info panel.

**Constraints from prior phases:** Phase 6 cutover is live, so `.image(...)` and `PrecomposeLdr` route through the batch path (`DrawBatchFamily::Image`). Note the image family legitimately includes precompose LDR draws — the text panel's precompose group adds two single-record image batches, so the harness shows 3 image batches / 6 records, not 1/4; the invariant is over batches, so this is correct. `ImageBatchKey { texture, layers, shadow, z_index, z_index_rank }`; `commit_image_batch_buffers` runs in `BatchResourcesReady`.

**Acceptance gate (met):** `cargo nextest run -p bevy_diegetic` green (625); `cargo build --example batch_validation` + clippy clean; launched `batch_validation` and confirmed on screen: the bottom-right panel draws the four tinted images through the batch path, the left info panel shows a non-zero image family (draws/records/records-per-draw + breakdown + "image records"), and the validation latch reads "records routed: ok" (image routing invariants pass).

### Phase 8 — Deletion + diagram  · status: done (uncommitted)

#### Work Order

**Goal:** Remove the dead entity image path, fix the `reconcile_ms` coupling, update `batching-diagram.md`.

**Spec:**
NOTE: Phase 6 already pulled forward all image-test deletion — `record_modified_materials`, `ModifiedMaterials`, `image_reconcile_app`, `one_image_tree`, `two_image_tree`, `single_image_child`, and the three old assertions (`tint_only_change…`, `unchanged_image_material…`, `command_index_shift…`) are GONE (verify: `rg record_modified_materials` returns nothing; `reconcile.rs` is ~1543 lines, not the ~1668 the earlier plan assumed). Do NOT try to re-delete them.

Delete the dead entity path (all in `reconcile.rs`, current line refs): `PanelImageChild` (`:526`), `ReusableImageChild` (`:545`), `ImageVisuals` (`:552`), `ImageGeometry` (`:718`), `reconcile_panel_image_children` (`:562`), `reconcile_existing_image` (`:734`), `apply_image_shadow_casting` (`:780`), `build_image_visuals` (`:793`), and any now-orphaned helper (e.g. `bounds_bits` `:831`) they solely used. Also delete `collect_panel_image_commands` (`:674`) AND its sole surviving guard test `image_batch_family_commands_do_not_spawn_legacy_children` (`:1461-1491`): the function is image-only (its two callers are `reconcile_panel_image_children`, deleted above, and that test), so once the entity system is gone the function is dead (clippy dead-code deny) and its "no legacy children spawned" assertion is vacuous (nothing can spawn them). Do NOT touch the router's `collect_panel_image_records` in `image_batch.rs`, which stays. Un-wire the system in `panel_text/mod.rs` — remove the `use self::reconcile::reconcile_panel_image_children;` import (`:27`) and its registration in `TextRenderPlugin` (`:111`, with its `.after(reconcile_panel_text_children)` ordering).

`reconcile_ms`: delete the image `mul_add`/accumulate (the image writer, now at `reconcile.rs:668-671`); text's writer is an assignment (not `+=`), so no accumulate-onto-stale bug — leave it. Delete/rewrite the stale cross-referencing comments: the image-mentioning stale comment in `render/mod.rs` (now `:116`, "text runs, images, glyph meshes, and SDF geometry"), the `reconcile_ms` ordering comment in `panel_text/mod.rs` (`:107-109`), the `perf.rs:52` doc comment referencing `reconcile_panel_image_children`, and any remaining in-`reconcile.rs` cross-ref. Decide whether the image route system re-adds its cost to `reconcile_ms` or accept the metric narrowing (document the choice).

The two Phase-1 old-path assertions ("layer 0 gone", "reused child updates `NotShadowCaster`") are ALREADY gone (Phase 6 deleted every image test except the guard above) — no action, drop this from the checklist. Update `docs/bevy_diegetic/batching-diagram.md` with the as-built image batch path.

#### Phase 8 Review

- **Phase 9 (mechanical):** added to Constraints the Phase-8 removal of `DrawCommandDepth::screen_depth_bias()` — a per-command screen bias is now `draw_depth.z_index_rank().screen_depth_bias()`; plus the `fill_batch.rs`/`draw_order.rs` shared-file collision with Phase 10 (whichever lands second rebases; if Phase 9 first, Phase 10's SDF parity test must preserve border-over-image order).
- **Phase 10 (mechanical):** refreshed the drifted `collect_panel_image_records` ref (`image_batch.rs:525→:534`, Phase 7b inserted `image_batch_summary`+perf plumbing above it); added the reciprocal Phase-9 collision note; noted SDF+Image now key material `depth_bias` purely on `DrawZIndexRank` (the image-only accessor is gone — one fewer thing the generic material hook reconciles).
- **Phase 10 extension-point gap (from Phase 7b, mechanical):** added a `batch_summary`/perf-breakdown extension point to the generic's Spec + gate — SDF/Path/Shape derive `BatchSummary` from `PipelineCompatibility`/`ResourceCompatibility`, Image fills it directly via `image_batch_summary`; this divergence is a second in-phase decision alongside the shadow-alpha rule, not covered by the commit helper/system split.
- **No redundancy / no invalidation:** Phase 8 built no ordering or generic machinery, so Phases 9–12 retain full scope; the `screen_depth_bias` accessor removal invalidated no remaining Work Order (every plan reference keys on the surviving `DrawZIndexRank::screen_depth_bias()`). Phases 11–12 (Path/Shape) are untouched by Phase 8.
- **As-built sibling drift (user-deferred, recorded so future passes do not relitigate):** `precompose.md`, `shadow-casting.md`, `diegetic-panel-perf.md`, and `cascade.md` still describe the deleted entity path; the user chose to defer their reconciliation to `/plan:to_as_built` (runs after Phase 12), not fix them in Phase 8.

**Files:**
- `crates/bevy_diegetic/src/render/panel_text/reconcile.rs` — entity-path deletions (types/systems/`collect_panel_image_commands` + its guard test) + `reconcile_ms` image-accumulate fix + comment cleanup.
- `crates/bevy_diegetic/src/render/panel_text/mod.rs` — remove the `reconcile_panel_image_children` import (`:27`) + its `TextRenderPlugin` registration (`:111`); fix the `reconcile_ms` ordering comment (`:107-109`). (This is where the image system is wired — NOT `render/mod.rs`.)
- `crates/bevy_diegetic/src/render/mod.rs` — stale image-mentioning comment (`:116`).
- `crates/bevy_diegetic/src/panel/perf.rs` — doc comment referencing `reconcile_panel_image_children` (`:52`).
- `docs/bevy_diegetic/batching-diagram.md` — as-built update.

**Constraints from prior phases:** after Phase 6's atomic cutover the batch path is the sole image draw source, so the entity path is fully dead — and Phase 7b confirmed on screen that batched images render correctly, so deleting the fallback here is safe. The Phase 5 coordinate helpers now living in the image batch module (`local_transform_from_bounds`, `image_size_from_bounds`, `linear_tint` in `image_batch.rs`) are the NEW path and MUST survive — only the OLD `reconcile.rs` geometry (`ImageGeometry`, `build_image_visuals`, `reconcile_existing_image`, `apply_image_shadow_casting`) is deleted. The two coordinate implementations are intentionally distinct until this phase removes the old one.

**Acceptance gate:** `cargo nextest run -p bevy_diegetic` green; no dangling references to the deleted types/systems; the `clippy` skill clean (no dead code); diagram updated.

#### Retrospective

**What worked:** The whole legacy image entity path deleted cleanly from `reconcile.rs` (`PanelImageChild`, `ReusableImageChild`, `ImageVisuals`, `ImageGeometry`, `reconcile_panel_image_children`, `reconcile_existing_image`, `apply_image_shadow_casting`, `build_image_visuals`, `bounds_bits`, `collect_panel_image_commands`, and the vacuous guard test) — every symbol gone crate-wide with no dangling reference, `image_batch.rs`'s router + coordinate helpers untouched. System un-wired from `TextRenderPlugin` in `panel_text/mod.rs`. `reconcile_ms` narrowed to the text writer (an assignment, no accumulate-onto-stale). Build + 624 tests + `clippy`/doc/fmt green; both reviews confirmed the deletion complete and correct.

**What deviated from the plan:** Codex also edited `draw_order.rs` + `panel_geometry.rs` (outside the Files list): deleting `build_image_visuals` orphaned `DrawCommandDepth::screen_depth_bias` (its sole production caller), and the workspace dead-code deny forces removal. Codex dropped the `screen_depth_bias` field + accessor from `DrawCommandDepth` and repointed the draw-order/panel-geometry tests to the equivalent `z_index_rank().screen_depth_bias()` (the field was initialized as exactly that, so no behavior change).

**Surprises:** The image route deliberately does NOT re-add its cost to `reconcile_ms` — the metric is now text-only, documented in `perf.rs`, `panel_text/mod.rs`, and the diagram. `reconcile.rs` dropped from ~1543 to ~1114 lines (all remaining image code was already the router in `image_batch.rs`).

**Implications for remaining phases:** Four as-built SIBLING docs still describe the deleted entity path (`precompose.md:27`, `shadow-casting.md:176,215`, `diegetic-panel-perf.md:93,144`, `cascade.md:144`) — **user deferred** the fix to `/plan:to_as_built`'s sibling-reconciliation step (runs after Phase 12). Phases 9–12 are unaffected by the deletion: Phase 9 (border ordering) touches `fill_batch.rs`/`draw_order.rs`; Phases 10–12 (generic collapse) operate on the live batch families, which the deletion did not alter.

### Phase 9 — Border-over-image ordering (PD-3)  · status: todo

#### Work Order

**Goal:** A clipping border that overlaps image size composites ON TOP of the coplanar `Blend` image.

**Spec:**
Border-over-image already fails on `main`: a `Blend` image renders in the transparent/OIT pass ordered by `oit_depth_offset`, while an opaque border is pushed AWAY from the camera by `OPAQUE_FILL_DEPTH_PUSH_LAYERS` (`fill_batch.rs:89`), landing it behind the coplanar image — `ClipDepthNudge` alone cannot fix this. Route the clipping border into the transparent phase (or give it a concrete in-front depth-test offset) so its order resolves by `oit_depth_offset`/screen bias, not the opaque-push. Drive the border's `oit_depth_offset`/screen bias from the same `ClipDepthNudge`/draw-order machinery the image records use, placing it in front of the image at equal world depth. Scope the phase change to the clipping border ONLY; the normal border keeps its current opaque-push behavior.

**Files:**
- `crates/bevy_diegetic/src/render/fill_batch.rs` — clipping-border phase/depth handling.
- `crates/bevy_diegetic/src/render/draw_order.rs` — depth-offset plumbing if extended.

**Constraints from prior phases:** images render `Blend` through the batch path (Phase 6 cutover); per-record `oit_depth_offset`/`ClipDepthNudge` are the shared depth levers. This phase is INDEPENDENT of the generic collapse (Phases 10-12) — the image family is fully shipped and live after Phase 6, so Phase 9 can run before, after, or in parallel with Phases 10-12. Phase 8 removed the per-command `DrawCommandDepth::screen_depth_bias()` accessor (`draw_order.rs`); a per-command screen bias is now derived as `draw_depth.z_index_rank().screen_depth_bias()` — use that when driving the border's screen bias from the draw-order machinery. **Shared-file collision with Phase 10:** both phases edit `fill_batch.rs` (and possibly `draw_order.rs`). They stay logically independent, but whichever lands second rebases onto the other — if Phase 10 (SDF → `BatchStore<F>`) has already migrated the fill family, this phase's clipping-border change re-lands on the generic; if this phase lands first, Phase 10's SDF before/after parity test MUST preserve the border-over-image ordering. NOTE: the crate has NO rendered-pixel/screenshot test harness under `MinimalPlugins` (Phase 6 confirmed no image example exists), so a literal "border pixels composite over image pixels" gate is not buildable headless — assert at the data level instead (see gate).

**Acceptance gate:** `cargo nextest run -p bevy_diegetic` green + a before/after DATA-level regression test (matching how the crate's other batch tests assert ordering, not pixels): with an image + a coplanar clipping border, the clipping border's resolved transparent-phase placement / `oit_depth_offset` / screen bias sorts it IN FRONT of the image record at equal world depth (it does not on `main`, where the opaque-push lands it behind); the normal (non-clipping) border keeps its opaque-push placement unchanged. On-screen visual confirmation piggybacks on Phase 7b's `batch_validation` image panel (add a coplanar clipping border to one of its image cards to eyeball the border-over-image order) — it is not a headless gate here.

### Phase 10 — Generic collapse pt.1: `BatchFamily` trait + SDF+Image  · status: todo

#### Work Order

**Goal:** Extract a `BatchFamily` trait + generic `BatchStore<F>`; migrate SDF and Image (the true nearest per-record pair) behind before/after parity tests; resolve the SDF/text shadow-alpha rule.

**Spec:**
Define a `BatchFamily` trait carrying the associated types and the per-family logic that genuinely differs:
- `type Key: Eq + Hash`, `type Record` (CPU), `type GpuRecord: ShaderType`, `type Resources`, `type Member`, `type Dirty`.
- hooks: `build` (records from command + resolved state), `pack` (→ `GpuRecord`), `material` (produce the batch material), `grow`, `world_bounds`.
Generic `BatchStore<F: BatchFamily>` owns store bookkeeping (`upsert`/`remove`/`retain`/`take_empty`), the batch container, and the six systems as `system::<F>` instantiations. Explicit extension points the cycle-2 review found (do not collapse them away):
- **Material-table register/rebind is opt-in:** SDF/text/Shape register their batch material each frame and get `extension.material_table` rebound (`material_table.rs:859`); Image binds none and skips both — an extension point separate from the "appends rows" build hook.
- **System topology differs:** append families run before `TransformSystems::Propagate`; SDF and Image need a separate post-`Propagate` transform system (cross-panel per-record membership forces per-record transform lookup); Shape folds one transform at build. Add extension points for before/after-`Propagate` gating and transform-update strategy.
- `world_bounds` is a bespoke per-family hook, not covered by `build`.
- **`grow`/material-rebind hook must tolerate two shapes:** SDF only re-points the material's record binding on growth; Image's `grow_image_batch_resources` (`image_batch.rs`) added an `else`-branch that rebuilds + re-inserts the material when `materials.get_mut` misses. The hook must cover both.
- **Commit is a helper + system split:** both SDF (`commit_sdf_batch_buffers`) and Image (`commit_image_batch_buffers` wrapping the per-batch `commit_image_batch_records`) use this shape — model the generic commit the same way.
- **`type Dirty` may be over-abstraction for the per-record pair:** SDF and Image use the identical two-flag `Dirty` (`record_upload` + `bounds_update`). Validate that a per-family `type Dirty` earns its place against Path/Shape before adding it; if not, make the two-flag `Dirty` shared machinery.
- **Per-family perf-breakdown population is bespoke (revealed by Phase 7b):** each family fills its `DiegeticPerfStats::*_breakdown: Vec<BatchSummary>` in its commit system, but the SOURCE differs — SDF/Path/Shape derive `BatchSummary` from their `PipelineCompatibility`/`ResourceCompatibility` key fragments, while Image has neither and fills `BatchSummary` DIRECTLY via `image_batch_summary(key, record_count)` (`image_batch.rs`). This is a divergence the commit helper/system split does NOT cover — add a `batch_summary`/perf-breakdown extension point, and decide (alongside the shadow-alpha rule below) whether to unify on a compatibility-derived summary or keep the per-family fill.

Migrate SDF + Image first (both per-record). Expect ~3 store templates overall: per-record (SDF+Image), per-run (Path), per-panel+atlas (Shape).

**SDF/text shadow-alpha rule (PD-1) MUST be decided in this phase.** SDF remaps `(Opaque, Cast) -> Mask(0.0)` shadow-gated (`fill_batch.rs:906`); text remaps `Opaque -> Mask(0.0)` unconditionally (`batching.rs:1169`) because opaque text loses its material bind group in the camera depth/normal prepass, not just the shadow pass; images are always `Blend` and need neither. Choose one: (a) one shared opaque-remap hook the families parameterize — this forces SDF onto text's unconditional rule and REQUIRES a prepass-strip parity test proving SDF still renders correctly; or (b) a per-family hook that keeps the two rules distinct with a documented reason. Do not carry the divergence forward silently.

**Files:**
- new generic module (`BatchFamily` + `BatchStore<F>`).
- `crates/bevy_diegetic/src/render/fill_batch.rs` — SDF migration.
- image batch module — Image migration.
- `crates/bevy_diegetic/src/render/batch_key.rs` — shadow-alpha hook home.

**Constraints from prior phases:** all four concrete families exist and are live (Image is fully shipped after Phase 6's cutover; SDF/Path/Shape shipped) — Phase 10 depends on Phases 2-6 (the complete image family), NOT on Phase 8 (deletion) or Phase 9 (border ordering), which are independent and may not have run yet. **Shared-file collision with Phase 9:** if Phase 9 has already landed its clipping-border phase/depth change in `fill_batch.rs`, the SDF-family migration here MUST carry it forward and the SDF before/after parity test MUST preserve the border-over-image ordering; if Phase 9 has not run, it later re-lands on the generic `BatchStore<F>`. Phase 8 already removed the image-only `DrawCommandDepth::screen_depth_bias` accessor, so both SDF and Image now key material `depth_bias` purely on `DrawZIndexRank` — one fewer accessor the generic material hook must reconcile. The per-record membership + post-`Propagate` transform pattern is shared by SDF and Image; the image render template is the vertex-pull shader shape shipped in Phase 6 (record index from `vertex_index`, `record.transform` as full world transform, `@binding(107)` records over an inert UV0+UV1 mesh; per-family embedded-shader registration lives in `lib.rs`); the growth-guard + `sort_records` invariants must survive the migration unchanged. Concrete shipped image names Phase 10 references: `local_transform_from_bounds`/`image_size_from_bounds`/`linear_tint`/`centered_corners` (`image_batch.rs`), `collect_panel_image_records` (`image_batch.rs:534`), the `grow_image_batch_resources` `else`-rebuild branch, and the `commit_image_batch_buffers`+`commit_image_batch_records` helper/system split.

**Acceptance gate:** `cargo nextest run -p bevy_diegetic` green + before/after parity tests for SDF and Image (batch output identical pre/post migration, INCLUDING each family's `DiegeticPerfStats::*_breakdown` output); the shadow-alpha decision AND the perf-breakdown decision (compatibility-derived vs per-family fill) recorded in code + the as-built doc; if the shared-hook path (a) is chosen, a passing prepass-strip parity test for SDF.

### Phase 11 — Generic collapse pt.2: Path (per-run template)  · status: todo

#### Work Order

**Goal:** Migrate `PathBatchStore` (per-run membership) onto `BatchStore<F>` — the second store template.

**Spec:**
Path membership is per-run (`upsert_run`) expanding to many quads — exercise the `type Member` extension point for per-run granularity. Path binds `material_table` (register + rebind), so its material hook is the opt-in-ON case. Path runs before `TransformSystems::Propagate` and folds transform at build (no separate post-`Propagate` transform system). Confirm dirty granularity against `analytic_paths/batching.rs`. Migrate behind a before/after parity test.

**Files:**
- `crates/bevy_diegetic/src/render/analytic_paths/batching.rs`, `crates/bevy_diegetic/src/render/analytic_paths/material.rs` — Path migration.
- generic module — per-run `Member` template if additional extension points surface.

**Constraints from prior phases:** `BatchFamily` trait + `BatchStore<F>` + the extension point set (`Member`/`Dirty`/`grow`/`build`/`world_bounds`, material-table opt-in, before-`Propagate` topology) from Phase 10; SDF+Image already migrated as the per-record template.

**Acceptance gate:** `cargo nextest run -p bevy_diegetic` green + a Path before/after parity test.

### Phase 12 — Generic collapse pt.3: Shape (per-panel + atlas template)  · status: todo

#### Work Order

**Goal:** Migrate `PanelShapeBatchStore` (per-panel + atlas) onto `BatchStore<F>` — the third store template; all four families on the generic.

**Spec:**
Shape membership is per-panel (`upsert_panel`). The Shape-only atlas (`PanelShapeBatchStore.atlas`) plus Shape-only per-record `outline` are a Shape-specific store extension, not a shared trait member — model them as an `F`-specific extension, not shared machinery. Shape folds one transform at build (before `Propagate`, no separate transform system). Shape has two GPU grow policies + a shared atlas upload — exercise the `grow` hook accordingly. Migrate behind a before/after parity test. After this phase, per-family code is reduced to key/record definitions + the handful of differing hooks.

**Files:**
- `crates/bevy_diegetic/src/render/panel_shapes/batching.rs` — Shape migration.
- generic module — per-panel `Member` + atlas-extension point.

**Constraints from prior phases:** trait + generic store from Phase 10; Path per-run template from Phase 11; atlas is a Shape-only extension (not the mis-named "Path/Shape" pair the early sketch assumed). Note: `render/mod.rs` carries pre-existing `#[expect(unused_imports)]` reasons on `PrimitiveOrdinal`/`ShapeOrdinal` citing "Phase 2"/"Phase 9" — those belong to a separate draw-order effort and predate this plan's numbering (Shape is Phase 12 here); ignore them, they are unrelated to this migration.

**Acceptance gate:** `cargo nextest run -p bevy_diegetic` green + a Shape before/after parity test; all four families (`SdfBatchStore`, image, `PathBatchStore`, `PanelShapeBatchStore`) run on `BatchStore<F>`; the `clippy` skill clean.
