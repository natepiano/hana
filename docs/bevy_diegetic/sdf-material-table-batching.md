# SDF Material Table Batching

> **Status: IMPLEMENTATION PLAN — phased, delegate-ready.** Batch SDF panel
> surfaces first, using the shared material-table architecture from the start;
> then migrate text and panel-shape batches onto the same table.

## Delegation Context
<!-- Shared across all phases. /plan:delegate prepends this to every dispatch. -->

- **Project:** `bevy_diegetic` — in-world diegetic UI panel renderer for Bevy with SDF panel geometry, analytic-path text, and analytic panel-shape rendering.
- **Stack:** Rust 2024 edition + Bevy 0.19.0-rc.2; wgpu 29; vertex-pulled analytic paths; OIT (`StableTransparency`) for translucent world panels.
- **Layout:**
  - `crates/bevy_diegetic/src/render/` — render module wiring, batch keys, draw ordering, SDF panel geometry/materials, shared material resolution.
  - `crates/bevy_diegetic/src/render/material_table.rs` — new frame-built material table, GPU upload, measurement, and registered-material rebind scheduling.
  - `crates/bevy_diegetic/src/render/fill_batch.rs` — new SDF/fill batch store, GPU record upload, bounds, and production SDF visual route.
  - `crates/bevy_diegetic/src/render/analytic_paths/` — current vertex-pulled text/panel-shape path records, materials, shaders, and batch store primitives.
  - `crates/bevy_diegetic/src/render/panel_text/` — text reconciliation and batch routing.
  - `crates/bevy_diegetic/src/render/panel_shapes/` — panel-shape analytic-path batching.
  - `crates/bevy_diegetic/src/callouts/` — callout rendering currently uses `LegacySdfExtendedMaterial` and `sdf_panel.wgsl`; migrate callout visuals off SDF primitives and onto analytic path / panel-shape rendering, then delete the callout SDF route.
  - `crates/bevy_diegetic/src/layout/` — `RenderCommand`, `RenderCommandKind`, `DrawZIndex`, panel tree material/color inputs.
  - `crates/bevy_diegetic/src/panel/` — panel state, `SurfaceShadow`, diagnostics/perf stats.
  - `crates/bevy_diegetic/src/shaders/` and `crates/bevy_diegetic/src/render/analytic_paths/*.wgsl` — current SDF and analytic-path shader code.
  - `crates/bevy_diegetic/examples/` — visual validation and perf examples.
- **Key files:**
  - `crates/bevy_diegetic/src/render/batch_key.rs` — `BaseMaterialId`, `BatchAlphaMode`, `BatchRenderLayers`, `VisualShadow`, `VisualBatchKey`, and current `VisualMaterialInterner`; current interner hashes scalar material values and must not be reused unchanged for material-as-data fill batching. `BaseMaterialId` remains old-interner identity until the interner is removed; it is not the material-table row type.
  - `crates/bevy_diegetic/src/render/material_slot_lifetime_probe.rs` — completed `#[cfg(test)]` investigation probe proving the selected bare-slot lifetime and ownership model; keep or rename only if it remains useful as durable regression coverage.
  - `docs/bevy_diegetic/investigation/material-slot-lifetime-and-ownership-evaluation.md` — evidence record for the slot lifetime, scheduler, and ownership decisions used by this plan.
  - `crates/bevy_diegetic/src/render/material_table.rs` (new) — frame-built material table, GPU upload, measurement, and registered-material rebind scheduling.
  - `crates/bevy_diegetic/src/render/fill_batch.rs` (new) — private/test fill batching first, then production SDF fill route, including `SdfRenderRecord`, batch resources, retained CPU records, bounds, uploads, and perf counters.
  - `crates/bevy_diegetic/src/render/draw_order.rs` — `DrawOrderProjection`, `DrawCommandDepth`, `ordinal_index()`, `depth_bias()`, `oit_depth_offset()`, `text_batch_depth_bias`, and `line_batch_depth_bias`; add fill/SDF batch lane helpers here.
  - `crates/bevy_diegetic/src/render/panel_geometry.rs` — current per-surface SDF path: `PanelSdfMesh`, `PanelSdfSurface`, `ElementSurface`, `gather_surfaces`, `desired_surfaces`, `build_sdf_quad`, `reconcile_sdf_quads`, `spawn_sdf_quad`, `PanelInteractionMesh`, overflow guard helpers, and SDF geometry tests.
  - `crates/bevy_diegetic/src/render/sdf_material.rs` — current SDF extended material path, renamed by Phase 0 to `LegacySdfExtendedMaterial` / `LegacySdfExtension`, plus `SdfPanelUniform`, forced blend/double-sided settings, `fill_alpha` shadow-prepass behavior, `sdf_panel_material`, and current SDF primitive material construction that is deleted when callouts leave SDF.
  - `crates/bevy_diegetic/src/callouts/render.rs` — current callout SDF usage; migrate segments and caps to analytic path / panel-shape rendering and delete SDF primitive material usage.
  - `crates/bevy_diegetic/src/render/material.rs` — `resolve_material`; currently folds element color into `StandardMaterial::base_color`, which must feed the frame material table rather than the fill batch key.
  - `crates/bevy_diegetic/src/render/analytic_paths/packing.rs` — current `PathRecord`, `PathInstanceRecord`, and `RunRecord`; Phase 6 renames them to `PackedPathRecord`, `PathQuadRecord`, and `PathRenderRecord` while replacing `RunRecord::fill_color` with `MaterialSlotId`.
  - `crates/bevy_diegetic/src/render/analytic_paths/batching.rs` — text path `PathBatchStore`, current `BatchGpu` resource packet renamed by this plan to `PathBatchResources`, padded buffer discipline, and `VisualMaterialInterner` use.
  - `crates/bevy_diegetic/src/render/panel_text/batching.rs` — `update_panel_text_batches`, `write_batch_run_transforms`, `update_batch_bounds`, `commit_batch_buffers`, capacity growth, and text batch tests including hidden-panel routing and z-level behavior.
  - `crates/bevy_diegetic/src/render/panel_shapes/batching.rs` — current panel-shape `PanelShapeBatchStore`, panel-upsert lifecycle, atlas rebuild, batch bounds, padded uploads, and shape ordering tests.
  - `crates/bevy_diegetic/src/render/analytic_paths/material.rs` — `PathExtendedMaterial`, `PathExtension`, storage bindings 101-105, `vertex_pull` specialization, and the material-group stripping guard for vertex-pulled depth/prepass pipelines.
  - `crates/bevy_diegetic/src/render/analytic_line_probe.rs` — analytic probe path that constructs current run/material inputs; migrate it through the shared-table bridge or remove it when `PathRenderRecord` changes.
  - `crates/bevy_diegetic/src/render/analytic_paths/analytic_path_vertex_pull.wgsl` — vertex-index correction via `mesh[instance_index].first_vertex_index`, capacity-tail guards, path-render table reads, and non-OIT clip-depth nudge.
  - `crates/bevy_diegetic/src/render/analytic_paths/analytic_path.wgsl` — current analytic-path fragment PBR path and OIT handling; later reads the frame material table.
  - `crates/bevy_diegetic/src/shaders/sdf_panel.wgsl` — current SDF fragment math, clip discard, prepass behavior, OIT offset, and PBR integration.
  - `crates/bevy_diegetic/src/render/constants.rs` — `DRAW_LEVEL_GEOMETRY_LANES`, `DRAW_LEVEL_STRIDE`, `DRAW_LEVEL_TEXT_SUBLANE`, `LAYER_DEPTH_BIAS`, `OIT_DEPTH_STEP`, `OIT_FOCUS_DEPTH`, and `SDF_AA_PADDING`.
  - `crates/bevy_diegetic/src/layout/render.rs` — `RenderCommand`, `RenderCommandKind::{Rectangle, Text, Border, Shapes, Image, ScissorStart, ScissorEnd}`, and `draw_step()`.
  - `crates/bevy_diegetic/src/panel/perf.rs` and `crates/bevy_diegetic/src/panel/constants.rs` — diagnostics/perf counters, including current `PanelGeometryPerfStats::sdf_quads`.
  - `crates/bevy_diegetic/examples/batch_validation.rs` — canonical mixed-content validation scene. It starts runnable before batching implementation and stays in sync after every phase, showing SDF fills/borders, text runs/glyphs, panel shapes/primitives, draw counts, and material-table counters as they become available.
  - `crates/fairy_dust/src/screen_panels/performance.rs` — shared Fairy Dust screen-panel helpers used by `batch_validation` and `diegetic_text_stress` for reusable stats/meter panel shells and row-group presentation.
  - `crates/bevy_diegetic/src/lib.rs` — internal shader asset registration.
  - `crates/bevy_diegetic/src/text/slug/glyph/coverage_probe.rs` — analytic shader FNV tripwire; update only when `analytic_path.wgsl` changes.
  - `Cargo.toml`, `crates/bevy_diegetic/Cargo.toml`, and repo CI/config — command targets and workspace shape.
- **Build:** `cargo build -p bevy_diegetic`; full example build when a phase touches examples: `cargo build --workspace --all-features --examples`.
- **Test:** `cargo nextest run -p bevy_diegetic`; never use `cargo test` for this repo family.
- **Lint:** `cargo clippy -p bevy_diegetic --all-targets`; `cargo +nightly fmt --all -- --check` for format verification and `cargo +nightly fmt --all` when applying format.
- **Style:** `zsh ~/.claude/scripts/rust_style/load-rust-style.sh --project-root /Users/natemccoy/rust/bevy_diegetic_sdf_draws` before writing Rust code.
- **Invariants:**
  - **One implementation target.** There is no SDF-local material interner and no shipped record-local SDF color implementation. SDF/fill batching uses the frame material table from its first production route.
  - **Old SDF path is a parity scaffold, not a shipped alternate route.** Existing `PanelSdfMesh` code may remain only while the new path is private/test-only. It must not receive a public feature flag, config switch, or new API. The production switch phase deletes `PanelSdfMesh`, `PanelSdfSurface`, and `spawn_sdf_quad` routing before the plan can proceed to text/panel-shape migration.
  - **SDF is for panel fills and borders only.** Callouts currently use `LegacySdfExtendedMaterial` and `sdf_panel.wgsl`, but that route is deleted instead of batched. Callout segments and caps move to analytic path / panel-shape rendering. Do not add new SDF primitive forms for callouts, arrows, caps, circles, triangles, or diamonds.
  - **Simple material table first.** The production plan starts with a frame-built dense material table, not a persistent slot allocator. Each frame builds the current material rows and the matching draw records together, then measures the cost. The durable allocator/retirement model is documented only as an appendix alternative and is not part of the active implementation plan.
  - **Old interner cleanup is mandatory.** `VisualMaterialInterner`, scalar-value `BaseMaterialId` batching, and `VisualBatchKey` are migration targets, not acceptable final infrastructure. Phase 10 removes them from production and probe-side analytic material routing. If `BaseMaterialId` remains for a non-table concept after Phase 10, that role must be explicitly documented and must not encode scalar material identity.
  - **Relationships are source traversal, not slot ownership.** Use Bevy relationships only where the render source already is an entity, matching the existing `TextRunOf` / `PanelTextRuns` pattern. The frame material table is plain render data for the current frame; do not create material-slot entities, per-SDF-surface entities, or per-panel-shape-primitive entities just to use relationships.
- **Material-as-data.** Batch keys carry no scalar/vector PBR values. `MaterialSlotValues` is the generic per-record PBR value payload: base color including its alpha channel, emissive, metallic, perceptual roughness, reflectance, transmission, thickness, attenuation, clearcoat, anisotropy, `ior`, the `uv_transform` UV affine, and any other supported scalar/vector `StandardMaterial` fields live in the material-table row selected by `MaterialSlotId`. `AlphaMode` is not the same thing as base-color alpha; it is compatibility data for SDF, text, and panel-shape render families. Two fills/text runs/panel-shape runs differing only in table values share a batch.
- **Diegetic draw order owns depth.** Source-material `StandardMaterial::depth_bias` is not a material-table value and is not an authored material splitter in this plan. Current panel SDF construction overwrites the base material's `depth_bias` from `DrawCommandDepth`; the batched path keeps depth/order data in `DrawOrderProjection`, `SdfRenderRecord`, `PathRenderRecord`, and batch sort fields.
- **Individual `StandardMaterial` sources are first-class.** Text runs, SDF roles, and panel-shape primitives may each resolve from distinct `StandardMaterial` sources. They still share a batch when those sources differ only by scalar/vector table values such as base color, emissive, metallic, perceptual roughness, reflectance, or `ior`. They split when the resolved material changes pipeline/resource compatibility that the renderer honors, such as alpha mode, texture resources, sampler/bind-group requirements, shader-def-driving flags, culling/sidedness, lighting mode, or shadow/prepass participation.
- **SDF exposes render policy instead of forcing it.** Current SDF code forces `AlphaMode::Blend`, `double_sided = true`, and `cull_mode = None`; that is legacy behavior to migrate away from, not the target batching rule. The target SDF material ladder may provide those SDF-friendly defaults, but authored `AlphaMode`, `double_sided`, and `cull_mode` remain user-controlled artistic choices. SDF batches split on those fields when they differ, just like text and panel-shape batches.
- **Material source ladders are explicit.** Each draw family owns a named material-source ladder; do not infer one family from another accidentally. Current code has these facts:
  - text currently has a panel-level text material (`DiegeticPanelBuilder::text_material`) plus `TextStyle` scalar/policy overrides; the current batched text renderer falls back from `panel.text_material()` to `default_panel_material()` and does not read `El::material` as a text material source;
  - SDF backgrounds/borders and panel shapes currently use `El::material` before `DiegeticPanelBuilder::material`, then `default_panel_material()`;
  - panel shapes currently have per-shape color authoring, but no per-shape full `StandardMaterial` authoring and no shape-specific panel material default.
  The target design keeps those ladders explicit and cascade-shaped: local override, else panel override, else global/default. Text adds `TextStyle::with_material(Handle<StandardMaterial>)` as the local material override over `DiegeticPanelBuilder::text_material`. Panel shapes add the same shape of ladder with shape-local, panel-level, and global/default rungs. `El::material` may remain the compatibility source for element surfaces, but using it for text or shape material must be a deliberate documented API decision, not renderer-side inference.
- **Material ladders use the cascade.** Generalize the existing cascade from `Copy` attributes to `Clone` attributes and use `Handle<StandardMaterial>` as the cascaded material value. This keeps the rule the same as the current cascade: local override, else parent/panel override, else default. Do not cascade owned `StandardMaterial` values. Material projection into `MaterialSlotValues` reads the current `Assets<StandardMaterial>` value behind the resolved handle, so edits to a shared material asset update table rows without changing the cascade value.
- **Cascade material types are source handles only.** The names below are the only material names that should participate in the cascade:
  - `SdfMaterial(pub Handle<StandardMaterial>)` cascades the source material for SDF backgrounds, borders, and other element surfaces.
  - `TextMaterial(pub Handle<StandardMaterial>)` cascades the source material for text runs.
  - `ShapeMaterial(pub Handle<StandardMaterial>)` cascades the source material for panel-shape primitives.
  These source-handle types resolve before material-table projection. They are not Bevy render material asset types and they do not own shader bindings.
- **Render material types do not cascade.** `SdfExtendedMaterial = ExtendedMaterial<StandardMaterial, SdfExtension>` and `PathExtendedMaterial = ExtendedMaterial<StandardMaterial, PathExtension>` are GPU render material asset types. They own shader extension bindings and batch plumbing, not authored source-material identity. They may read material-table rows after source handles have been resolved and projected, but they are never cascade attributes.
- **Render material extensions are batch plumbing, not source material identity.** Text and panel shapes already share `PathExtendedMaterial = ExtendedMaterial<StandardMaterial, PathExtension>`. `PathExtension` owns shader plumbing such as path/quad/run storage buffers and batch uniforms; it is not a text- or shape-source material. Do not add separate `TextExtension` and `ShapeExtension` unless a proven pipeline requirement appears. The final SDF batch path uses `SdfExtendedMaterial = ExtendedMaterial<StandardMaterial, SdfExtension>`. `SdfExtension` owns the `SdfRenderRecord` buffer, frame material table buffer, and batch-level shader plumbing for panel fill/border SDFs only. Per-surface geometry/material identity lives in `SdfRenderRecord` and the frame material table. The old per-surface SDF route is named `LegacySdfExtendedMaterial = ExtendedMaterial<StandardMaterial, LegacySdfExtension>` during migration and is removed once panel fills/borders use `SdfExtendedMaterial` and callouts no longer use SDF.
- **Texture-backed material sampling.** Every sampled `StandardMaterial` texture channel resolved from a record's source material — `base_color_texture`, `emissive_texture`, `metallic_roughness_texture`, `normal_map_texture`, `occlusion_texture`, and any other supported channel — is sampled by all three render families (SDF fills, text glyphs, panel shapes), not only base color and not only SDF fills. Each record samples through the `StandardMaterial` half of its `ExtendedMaterial` exactly as Bevy's `pbr_input_from_standard_material` does, so the channels feed PBR the same way a normal mesh would: the SDF/glyph/stroke coverage composites the sampled base color before writing alpha, while emissive/metallic-roughness/normal/occlusion contributions flow into `apply_pbr_lighting` unchanged. All channels share one UV: the record's element-local box UV, `0..1` across the resolved layout box of the SDF surface, text run, or panel-shape silhouette, so the material maps once across the whole record and the coverage/outline/stroke stencils it. This is the same mapping SDF fills already get from the quad's `0..1` UV. Because the target text and shape render families share `PathExtendedMaterial` / `PathExtension` (see the render-extension principle above), the glyph and shape sampling is added once in the shared analytic path shader; SDF samples in `SdfExtension`, which already runs `pbr_input_from_standard_material` and therefore already applies the non-base-color channels today. The box UV is per-record render data carried in `SdfRenderRecord`, the per-glyph instance record, and `PathRenderRecord`; it is never a material-table value and never a batch-key field. Texture presence per channel, and any channel-specific mesh-attribute requirement such as tangents for `normal_map_texture`, are `ResourceCompatibility` / pipeline splitters per the Phase 2 classification, so a texture-backed record forms its own batch while the material table holds only scalar/vector values; channels that need tangents add that mesh-attribute requirement to the batch geometry. To texture a single glyph or a sub-span, author it as its own run/record. `ElementContent::Image` leaves remain a separate, unbatched compositing path; this principle is per-record material texturing of SDF/text/shape surfaces, not image-leaf batching. The record's authored `uv_transform` — a `StandardMaterial` affine (scale/offset/rotation) applied to UVs, not a texture — is supported as a per-record material-table value: each shader reads it from the record's table row and composes it with the box UV as `final_uv = uv_transform * box_uv` before sampling every channel, so two records sharing the same textures but tiling/offsetting/rotating differently still batch together and differ only in table data. An identity `uv_transform` maps each texture once across the box; tiling past `0..1` repeats only when the bound texture's sampler uses a `Repeat` address mode, which is a property of the texture resource and therefore already part of `ResourceCompatibility`.
- **Texture boundary.** The material table cannot vary sampled texture resources inside one draw. Texture handles and texture-present pipeline/bind-group requirements stay in the batch key, so a texture-backed record forms its own batch while the material table continues to hold only scalar/vector values. Diegetic fills are solid SDF surfaces; image leaves are not batched by this plan.
  - **Frame-local slot identity.** Each rendered material role receives a `MaterialSlotId` for the current frame's dense table. `MaterialSlotId` is a row index used by GPU records and is not durable across frames in the active plan.
  - **Repo-owned scalar/vector slot values.** The frame table storage layout is `MaterialSlotValues` plus a WGSL mirror, not raw `StandardMaterialUniform`. It may contain only fields that can vary per slot inside one draw. Texture presence, alpha mode, shader-def-driving flags, culling/sidedness, unlit/lighting mode, and shadow/prepass participation remain batch-key or pipeline splitters, with debug assertions that all records in a batch agree on them.
  - **Resource-only batch keys.** SDF, text, and panel-shape batch keys carry `pipeline_compatibility: PipelineCompatibility` and `resource_compatibility: ResourceCompatibility` directly, with scalar/vector PBR fields unrepresentable. These shared compatibility types contain only shared material-derived splitters. `PipelineCompatibility` contains material-driven pipeline/specialization facts such as alpha mode, shader defs, lighting mode, `double_sided`, and `cull_mode`. Reuse the existing `BatchAlphaMode` representation for hashable alpha mode; do not add a second alpha-key enum. Do not collapse facing compatibility into the current two-value `Sidedness` enum: `Sidedness` remains an authoring/cascade convenience that maps to `double_sided` and `cull_mode`, while direct `StandardMaterial` handles can still express `cull_mode = Some(Face::Front)`, `Some(Face::Back)`, or `None`. `ResourceCompatibility` contains material texture/sampler/bind-group resources. Renderer-specific splitters stay as direct fields on `SdfBatchKey` or `PathBatchKey` with doc comments explaining why they split that renderer. Use constructor-only APIs and tests so scalar/vector PBR fields cannot enter any batch key. `VisualBatchKey`, `InternedMaterialKey`, `VisualMaterialInterner`, and `BaseMaterialId` must not appear in new SDF keys or migrated analytic keys except in explicitly named migration bridge code that is deleted in Phase 10.
  - **Batch key/resource/record relationship.** `SdfBatchKey` is the key in the SDF batch store, for example `HashMap<SdfBatchKey, SdfBatchResources>`. `SdfBatchResources` owns or tracks the batch entity, `SdfExtendedMaterial`, `SdfRenderRecord` buffer handle, capacity, and current record count for one compatible SDF batch. `PathBatchKey` is the key in the analytic-path batch store, for example `HashMap<PathBatchKey, PathBatchResources>`. `PathBatchResources` owns or tracks the batch entity, `PathExtendedMaterial`, `PathRenderRecord` buffer handle, related path/quad buffers, capacity, and current record count for one compatible text/shape path batch.
  - **Sorted/OIT parity.** `DrawOrderProjection` orders any two commands identically on the sorted screen `depth_bias` axis and OIT `oit_depth_offset` axis. Existing parity tests in `draw_order.rs` must stay green.
  - **Batch sort fields.** A batch is one `Transparent3d` item, so its key must include the fields needed to keep Bevy's item sorting and this renderer's in-batch record order correct, such as panel scope, z-level, render layers/view scope, and `ContiguousDrawnRun` when needed to preserve interleaving. These fields live directly on `SdfBatchKey` or `PathBatchKey`; do not introduce separate `SdfScreenBatchKey` or `PathScreenBatchKey` wrapper types unless implementation proves they remove real duplication.
  - **Per-level screen-band ceiling.** Batching fills into one draw does not remove the 64 geometry-lane budget inside one z-level. The overflow warning must survive deletion of the old per-quad path.
  - **ShaderBuffer rebind hazard.** `ShaderBuffer::set_data` with changed byte length can recreate the wgpu buffer while old bind groups still point at the old buffer. Every record/table buffer upload is padded to capacity; growth creates new buffer assets and rewrites material handles in an ordered pass before material prepare observes them.
  - **Batch bounds are part of first production correctness.** SDF/fill batches use inert meshes, so they must hand-write `Aabb`, batch transforms, and `NoAutoAabb` before visibility checks. Bounds must include transformed `mesh_half_size` corners plus AA padding/clip behavior.
  - **SDF quality preservation.** Rounded corners, per-side borders, clip expansion with `SDF_AA_PADDING`, subpixel inflation, OIT offset behavior, border-only shadow/prepass behavior, PBR lighting, render layers, and interaction meshes must preserve current behavior.
  - **New type comments are required.** Every new or renamed struct, enum, newtype, type alias, and field introduced by this plan must have a doc comment that states what it represents and what it connects to directly. This is especially required for material-table rows/ids, CPU retained records, GPU render records, batch keys, batch resources, render material aliases/extensions, and cascade material source handles.
  - **Build green each phase.** Every phase leaves the tree compiling, formatted with nightly rustfmt, clippy-clean for the touched target, and nextest-green for `bevy_diegetic`.
  - **Validation example stays runnable.** `crates/bevy_diegetic/examples/batch_validation.rs` is the visual progress gate for this plan. Every implementation phase that changes render paths, counters, or public example-facing names must keep it compiling and update its readout rows in the same phase. The example uses shared Fairy Dust instrumentation helpers rather than copied screen-panel styling.

## Phases

### Phase 0 — Rename Render Materials And Delete SDF Primitive Route · status: todo

#### Work Order

**Goal:** Mechanical render-material renames and the SDF primitive/callout cleanup land before batching work, so later implementation phases start from a codebase where SDF means panel fills/borders only.

**Spec:**
- Start implementation by handing these exact renames to the user/editor as a mechanical rename pass:
  - `SdfPanelMaterial` -> `LegacySdfExtendedMaterial`;
  - `SdfPanelExtension` -> `LegacySdfExtension`;
  - `PathMaterial` -> `PathExtendedMaterial`;
  - current analytic-path `BatchKey` -> `PathBatchKey`;
  - current analytic-path `BatchGpu` -> `PathBatchResources`.
- Keep `PathExtension` unchanged. It is already the shared analytic-path extension for text and panel shapes.
- Do not rename `ShapeBatchKey` mechanically in Phase 0 unless doing so is no-behavior-change and low-risk. The desired final key name for the shared analytic path renderer is `PathBatchKey`; if `ShapeBatchKey` remains during the migration, Phase 6 must either remove it, fold it into `PathBatchKey`, or document a deliberately narrower remaining role.
- Do not rename the source-material cascade attributes in this phase. `SdfMaterial`, `TextMaterial`, and `ShapeMaterial` are introduced later as `Handle<StandardMaterial>` cascade values and are intentionally distinct from `SdfExtendedMaterial`, `PathExtendedMaterial`, and the migration-only `LegacySdfExtendedMaterial`.
- Delete the standalone/callout SDF primitive route in this phase:
  - remove `SdfPrimitiveKind` and the triangle/circle/diamond/line/cap SDF selector path instead of renaming it;
  - remove callout use of `LegacySdfExtendedMaterial` and `sdf_panel.wgsl`;
  - preserve the public `CalloutLine` / `CalloutCap` API unless this phase deliberately documents and tests its removal as a breaking API deletion;
  - for retained `CalloutLine` support, replace the current direct SDF child renderer with a concrete analytic callout adapter before deleting the old SDF primitives. The adapter may reuse analytic-path packing/render resources, but it must be a non-panel source route with its own source identity, ownership/cleanup path, transform/bounds update, render-layer propagation, depth/OIT behavior, and `SurfaceShadow` mapping. Do not pretend standalone callouts are panel shapes unless this phase deliberately materializes them through an existing panel-shape-compatible source model;
  - preserve the current `update_callout_lines` behavior: changed `CalloutLine` or `RenderLayers` despawns old `CalloutVisual` children, zero-length shafts skip rendering, render layers are inherited/fallbacked the same way, `SurfaceShadow` maps to shadow-caster behavior, and per-child depth/OIT ordering remains stable;
  - map current callout primitives to analytic geometry: shaft and open-arrow wings become analytic segments; solid triangle, circle, square, and diamond caps become analytic closed forms using the existing cap-resolution dimensions and colors;
  - migrate any other retained callout visual support to existing analytic path / panel-shape rendering, or remove only obsolete SDF-specific callout example/probe code when there is no production visual to preserve;
  - delete `crates/bevy_diegetic/examples/sdf.rs` if it exists only to demonstrate standalone SDF primitives.
- Do not introduce new batching code or material-table logic in this phase.
- Update comments/docs in touched files so first-use comments explain:
  - `LegacySdfExtendedMaterial` is the existing SDF render material asset type, `ExtendedMaterial<StandardMaterial, LegacySdfExtension>`, used by panel SDF quads and the soon-deleted callout SDF route before panel fills/borders migrate to `SdfExtendedMaterial`;
  - `SdfExtendedMaterial` is reserved for the final batched SDF render material, `ExtendedMaterial<StandardMaterial, SdfExtension>`;
  - `PathExtendedMaterial` is the shared analytic render material asset type, `ExtendedMaterial<StandardMaterial, PathExtension>`, used by text and panel-shape batches;
  - `PathBatchResources` is the per-compatible-batch resource packet for one analytic path batch, not a renderer-wide GPU resource store;
  - neither type is a source-material cascade value.

**Files:**
- `crates/bevy_diegetic/src/render/sdf_material.rs` — type alias, extension struct, comments, and local tests.
- `crates/bevy_diegetic/src/render/panel_geometry.rs` — SDF render material type references.
- `crates/bevy_diegetic/src/callouts/render.rs` and an implementation-local analytic callout adapter module if needed — remove old SDF primitive/callout usage; route retained callout visuals through an explicit non-panel analytic path if production callout visuals remain.
- `crates/bevy_diegetic/src/callouts/mod.rs` — update module docs so public callouts no longer claim to render directly from SDF meshes/materials.
- `crates/bevy_diegetic/src/callouts/caps.rs` and `crates/bevy_diegetic/src/callouts/line.rs` — preserve public cap/line builders while changing only the renderer, unless the phase explicitly removes the API.
- `crates/bevy_diegetic/examples/sdf.rs` — delete the standalone SDF primitive example.
- `crates/bevy_diegetic/src/render/mod.rs` — re-exports.
- `crates/bevy_diegetic/src/render/analytic_paths/material.rs`, `crates/bevy_diegetic/src/render/analytic_paths/batching.rs`, `crates/bevy_diegetic/src/render/panel_text/batching.rs`, `crates/bevy_diegetic/src/render/panel_shapes/batching.rs`, and `crates/bevy_diegetic/src/render/analytic_line_probe.rs` — analytic render material type references.
- `docs/bevy_diegetic/**` — plan/current docs should use the new names; historical passages may include the old names only with a first-use mapping.

**Acceptance gate:** `cargo build -p bevy_diegetic`, `cargo build --workspace --all-features --examples`, `cargo +nightly fmt --all -- --check`, `cargo clippy -p bevy_diegetic --all-targets`, and `cargo nextest run -p bevy_diegetic` pass. `rg -n "SdfPanelMaterial|SdfPanelExtension|\\bPathMaterial\\b|struct BatchKey|pub\\(crate\\) use analytic_paths::BatchKey|\\bBatchGpu\\b" crates/bevy_diegetic/src docs/bevy_diegetic` returns only explicitly historical passages with a first-use mapping to `LegacySdfExtendedMaterial`, `LegacySdfExtension`, `SdfExtendedMaterial`, `SdfExtension`, `PathExtendedMaterial`, `PathBatchKey`, or `PathBatchResources`; this plan's own Phase 0 rename map is allowed while the plan is active. `rg -n "SdfPrimitiveKind|SdfShapeKind|sdf_primitive|sdf_kind|sdf_params|oriented cap|SDF primitive|examples/sdf.rs" crates/bevy_diegetic/src crates/bevy_diegetic/examples docs/bevy_diegetic` has no active-plan or production/example hits except this plan's deletion instructions and historical notes explaining the deletion. If public `CalloutLine` remains, tests or visual probes cover shaft, open-arrow wings, triangle, circle, square, and diamond caps through the analytic route; changed-line cleanup still despawns old `CalloutVisual` children; `RenderLayers`, `SurfaceShadow`, depth bias, and OIT offset behavior match the old route closely enough for existing examples.

### Phase 1 — Resolve SDF Surfaces And Move Draw-Order Limits · status: todo

#### Work Order

**Goal:** SDF surface resolution and draw-order limit warnings are independent of the old per-quad renderer, with no render behavior change.

**Spec:**
- Introduce a render-neutral `ResolvedSdfSurface` in `panel_geometry.rs` or a new private `render/panel_geometry/surface.rs` module. It must be derived from the existing `ElementSurface` plus `PanelReconcileContext` and hold all values needed by both the old path and the future fill batch:
  - surface identity: `(panel_entity, command_index)`;
  - the full `DrawCommandDepth`, not just scalar depth fields;
  - resolved fill and border `StandardMaterial` inputs for the future table slots;
  - panel-local center and transform inputs;
  - panel-local SDF half size and mesh half size;
  - local corner radii and border widths;
  - local clip rect after `SDF_AA_PADDING` expansion;
  - render layers and `SurfaceShadow`;
  - any SDF selector/params needed to preserve current rounded-rect behavior.
- Do not bake post-propagation world transform, world bounds, or transparent sort center into the pre-propagation resolver. The batched path will compute those after `TransformSystems::Propagate`; the old-path adapter may keep producing the old world-space quad data for parity until deletion.
- Split `build_sdf_quad` into:
  - `resolve_sdf_surface(...) -> ResolvedSdfSurface`;
  - `build_sdf_quad_from_resolved(...) -> BuiltSdfQuad`.
- Keep old rendering behavior unchanged in this phase. `reconcile_sdf_quads` should still spawn/recolor/despawn `PanelSdfMesh` children, and existing tests that query `PanelSdfSurface` should still pass.
- Move the load-bearing clip-padding block from `build_sdf_quad` into the resolver intact. The `- pad` / `+ pad` expansion around `SDF_AA_PADDING` is required to preserve anti-aliased ruler/panel edges at clip boundaries.
- The old-path adapter, not the resolver, should write `surface.draw_depth.depth_bias().get()` into per-surface `StandardMaterial::depth_bias`. Future batch materials will use a batch lane plus per-record depth data.
- Move the overflow guard out of `reconcile_sdf_quads` into a shared draw-order limits system or module, for example `render::draw_order_limits::warn_panel_draw_order_limits(panel_entity, &DrawOrderProjection)`.
- Move with the guard:
  - `per_level_band_capacity`;
  - `per_level_band_overflows`;
  - `oit_depth_budget`;
  - `oit_total_overflows`;
  - the tests currently covering those helpers.
- The moved guard must read `DrawOrderProjection::level_occupancy()` and the full command count so it still warns for text-only, panel-shape-only, and fill panels after the old SDF path is deleted.

**Files:**
- `crates/bevy_diegetic/src/render/panel_geometry.rs` — add `ResolvedSdfSurface`, split `build_sdf_quad`, remove draw-order limit warning/helpers after moving them, keep render behavior unchanged.
- `crates/bevy_diegetic/src/render/draw_order_limits.rs` (new) — shared draw-order limit warning helper/system, moved capacity/OIT predicates, moved tests.
- `crates/bevy_diegetic/src/render/mod.rs` — wire the new draw-order limits module/system.
- `crates/bevy_diegetic/src/render/constants.rs` — source of unchanged lane/OIT constants.
- `crates/bevy_diegetic/examples/units.rs` — manual visual check target for clip padding.

**Constraints from prior phases:** None.

**Acceptance gate:** `cargo build -p bevy_diegetic`, `cargo +nightly fmt --all -- --check`, `cargo clippy -p bevy_diegetic --all-targets`, and `cargo nextest run -p bevy_diegetic` pass. Existing panel geometry tests still pass without asserting new batch behavior. The overflow warning tests live outside the per-quad SDF path. Manual follow-up target remains `units` for clip-padding regression.

### Phase 2 — Frame Material Table Foundation And Measurement · status: todo

#### Work Order

**Goal:** A simple frame-built dense material table exists, can feed GPU records, and has stress measurements before any durable slot allocator is considered.

**Spec:**
- Add a `render/material_table.rs` module and wire it through `render/mod.rs`.
- Implement the active simple-first model: build a dense material table from current-frame rendered material entries. Do not add `SharedMaterialAssignments`, `SharedMaterialTableEpoch`, `MaterialSlotAllocator`, a retirement queue, `MaterialSlotGeneration`, or `MaterialSlotRef` in this phase.
- Add `MaterialSlotId` in `render/material_table.rs`:
  - keep it a distinct compiler-checked type;
  - do not add `#[repr(transparent)]`; GPU-facing records convert it explicitly to a bare `u32`;
  - use the existing `bevy_kana` conversion helper pattern for `u32` access instead of hand-rolled accessor boilerplate when the helper fits;
  - provide safe outward conversion to `u32`; do not implement infallible raw `u32 -> MaterialSlotId` conversion because `u32::MAX` is reserved for the GPU no-read sentinel;
  - keep existing `Clone`, `Copy`, `Debug`, `Eq`, `Hash`, and `PartialEq`.
- CPU APIs return valid frame-local `MaterialSlotId` values only; GPU records store a bare `u32` produced at the GPU boundary.
- Define `SdfPaintMaterial { Authored(MaterialSlotId), NotAuthored }` for CPU-side SDF fill/border material presence. Use it for `SdfCpuRecord::fill_material` and `SdfCpuRecord::border_material` so the absence reason is self-documenting.
- Define a GPU-boundary conversion that maps `SdfPaintMaterial::Authored(slot)` to the slot's `u32` row and maps `SdfPaintMaterial::NotAuthored` to `INVALID_GPU_MATERIAL_SLOT: u32 = u32::MAX`, mirrored in WGSL. `MaterialSlotId` itself must not have an invalid variant or public invalid constant. `TryFrom<u32> for MaterialSlotId` must reject `u32::MAX`, and the builder must assert it never emits that raw value.
- Do not use `BaseMaterialId` as the material-table row id. It is old value-interner identity and is removed from migrated production routing by Phase 10.
- Do not add table-row source keys.
- Define newtypes for raw ordinals that cross APIs, for example `CommandIndex`, `ElementIndex`, `ShapeOrdinal`, and `PrimitiveOrdinal`.
- Implement `FrameMaterialTable` or equivalent as the current-frame table payload:
  - `rows: Vec<MaterialSlotValues>`;
  - `row_count()`, `capacity()`, and `upload_bytes()` stats;
  - no free list, no retained owner map, and no row reuse logic.
- Implement `FrameMaterialTableBuilder` or equivalent:
  - starts empty each frame;
  - appends one row per live rendered material role that needs a table read;
  - returns the assigned `MaterialSlotId` immediately to the record builder for that same frame;
  - preserves deterministic row order from draw-order/source traversal so tests and stats are stable;
  - does not deduplicate by material value unless a later measured phase proves that is both useful and simpler.
- Add one owning build resource, for example `FrameMaterialTableBuild`, so the frame table cannot be accidentally cleared or forked per producer:
  - clear exactly once before SDF, text, and panel-shape producers append rows;
  - expose the single `FrameMaterialTableBuilder` to all current-frame record builders;
  - freeze or commit exactly once after all producers finish;
  - forbid new rows after the freeze point and before extraction;
  - test mixed SDF/text/shape frames for unique slot ids, deterministic order, and a total row count equal to all live rendered material entries.
- Add simple doc comments for every new struct, enum, and field in `material_table.rs`, in addition to the plan-wide new-type comment invariant. The comments must explain the field's direct relationship to the table model, for example row storage, current-frame row output, measurement data, or GPU upload data.
- Define the active data flow without an assignment snapshot:
  - producers resolve current material sources and call the frame table builder while constructing records for that frame;
  - the builder appends `MaterialSlotValues` and returns `MaterialSlotId`;
  - records and rows are extracted/uploaded together;
  - hidden, clipped, missing, or removed sources simply do not append rows and do not need frees, tombstones, or parked retained slots.
- Define repo-owned `MaterialSlotValues` and a WGSL mirror. Project `StandardMaterial` into these scalar/vector slot values. The target coverage is all `StandardMaterialUniform` value fields that can vary per record without changing the material bind group, pipeline specialization, pass routing, or required mesh attributes. Texture handles, texture-present resource requirements, alpha mode, shader-def-driving flags, culling/sidedness, unlit/lighting mode, and shadow/prepass participation are not blindly representable per slot and must remain in batch keys or pipeline/material group choices unless a field-specific test proves a flag can vary safely per table row.
- Start the field list from Bevy 0.19's `StandardMaterialUniform`, including at least: `base_color`, `emissive` plus emissive exposure weight, `attenuation_color`, `reflectance`, `roughness`, `metallic`, `diffuse_transmission`, `specular_transmission`, `thickness`, `ior`, `attenuation_distance`, `clearcoat`, `clearcoat_perceptual_roughness`, `anisotropy_strength`, and `anisotropy_rotation`. Classify `uv_transform`, `flags`, `alpha_cutoff`, parallax/depth-map fields, texture-channel fields, and feature-gated fields explicitly as table data, compatibility data, unsupported, or deferred with a test-backed reason.
- Add the classification as code-adjacent documentation before implementing projection. The initial classification must be:
  - table values: `base_color`, `emissive` with emissive exposure weight, `attenuation_color`, `reflectance` as the Bevy-computed specular tint times reflectance `Vec3`, `roughness`, `metallic`, `diffuse_transmission`, `specular_transmission`, `thickness`, `ior`, `attenuation_distance`, `clearcoat`, `clearcoat_perceptual_roughness`, `anisotropy_strength`, `anisotropy_rotation` as the Bevy-computed `Vec2::from_angle(...)`, and `uv_transform` as the `StandardMaterial` UV affine, which each render shader reads from the table row and composes with the element-local box UV (`final_uv = uv_transform * box_uv`) before sampling every texture channel per the "Texture-backed material sampling" principle. `uv_transform` does not split batches; sampler address mode (which governs tiling vs clamping) is a texture-resource property carried by `ResourceCompatibility`;
  - compatibility values: all texture handles and texture-present flags, texture UV-channel selection, alpha mode and alpha cutoff, `double_sided`, `cull_mode`, `unlit`, `fog_enabled`, two-component normal-map status, `flip_normal_map_y`, attenuation-enabled flag, feature-gated texture flags, normal-map/clearcoat-normal requirements, parallax/depth-map presence, `opaque_render_method`, deferred lighting pass id, and any mesh-attribute requirement such as tangents or UVs;
  - draw-order values: `StandardMaterial::depth_bias` is ignored as source-material data for diegetic SDF/text/shape batching; diegetic draw-order types own depth bias and OIT offsets;
  - deferred until their backing feature is intentionally designed: parallax/depth-map scalar fields without a depth-map resource, lightmap exposure, and any feature-gated texture channel value whose texture is absent. Material texture sampling and `uv_transform` are not deferred: all sampled `StandardMaterial` texture channels use the element-local box UV defined by the "Texture-backed material sampling" principle, and `uv_transform` is a table value composed with that box UV per record. If implementation proves one of these deferred fields is needed with no resource binding or shader-def cost, move it into table values with a dedicated test.
- `PipelineCompatibility::from(&StandardMaterial)` must preserve exact material-facing facts, not just cascade-level sidedness. It stores hashable alpha mode via `BatchAlphaMode`, plus `double_sided` and `cull_mode` from the resolved `StandardMaterial`. Tests must cover all supported `AlphaMode` variants and at least `cull_mode = None`, `Some(Face::Back)`, and `Some(Face::Front)`.
- Projection tests must compare `MaterialSlotValues::from(&StandardMaterial)` with Bevy's `StandardMaterialUniform` conversion for every table value above. Compatibility tests must prove that a field classified as compatibility data cannot enter `MaterialSlotValues` and cannot be hidden inside `SdfBatchKey` / `PathBatchKey` scalar fields.
- Treat per-element/per-run/per-primitive `StandardMaterial` inputs as a core API case. Add tests where two independent material sources with different base color/emissive/roughness/metallic/reflectance/transmission/clearcoat/anisotropy values produce different `MaterialSlotValues` but identical `PipelineCompatibility` and `ResourceCompatibility`, and another material source with a different `AlphaMode`, `double_sided`, `cull_mode`, or texture resource produces a different compatibility field.
- Add field-by-field projection tests comparing `MaterialSlotValues` against Bevy's `StandardMaterialUniform` conversion for every scalar/vector field the table stores. Any new Bevy `StandardMaterial` scalar/vector field must be explicitly classified as table data, compatibility-key data, unsupported, or deferred with a documented reason.
- Define a focused shared projection contract for material-table producers:
  - `MaterialSlotCandidate { values: MaterialSlotValues, pipeline_compatibility: PipelineCompatibility, resource_compatibility: ResourceCompatibility }`;
  - `impl From<&StandardMaterial> for MaterialSlotCandidate`, internally using `MaterialSlotValues::from(material)`, `PipelineCompatibility::from(material)`, and `ResourceCompatibility::from(material)`;
  - a private `MaterialSlotInput` trait with `type Key: Copy + Eq + Hash`, `key() -> Self::Key`, and `material_slot_candidate() -> MaterialSlotCandidate`;
  - `MaterialSlotAppended<K> { key: K, slot: MaterialSlotId, pipeline_compatibility: PipelineCompatibility, resource_compatibility: ResourceCompatibility }`;
  - `MaterialSlotAppend<K> { Appended(MaterialSlotAppended<K>), DroppedLimit }`, or an equivalent self-documenting enum, for row-limit overflow;
  - a shared `append_material_slot(...) -> MaterialSlotAppend<T::Key>` helper that appends `candidate.values` through `FrameMaterialTableBuilder` and returns either the assigned slot plus compatibility values or an explicit limit drop.
- `MaterialSlotInput` is the shared contract for append-time SDF fill/border inputs, text-run inputs, and panel-shape inputs. These inputs are temporary CPU values created while building current-frame records. They normally borrow the resolved base `StandardMaterial` asset plus any local override data and compute `MaterialSlotCandidate` directly. They must not store owned `StandardMaterial` values or `Cow<StandardMaterial>` in retained state. `From<&StandardMaterial>` exists only for no-override cases.
- Material-slot append helpers must return named structs/enums, not bare tuples. `MaterialSlotAppended<K>` is the required success shape so call sites cannot misorder `MaterialSlotId`, `PipelineCompatibility`, and `ResourceCompatibility`. Overflow or row-limit drops must be explicit variants, not `Option`, invalid slot ids, or panics.
- Do not defer duplication cleanup indefinitely. Phase 3 must use `MaterialSlotInput` for panel SDF fill/border material entries and extract a shared SDF paint-entry helper if fill/border routing repeats the same material append and compatibility folding. Phase 6 must use the same contract for the analytic bridge and extract a shared path-material helper if text and panel-shape routing repeat the same append/key/record setup. Higher-level helpers are allowed only when they remove repeated concrete code; the material projection/append contract above is mandatory from Phase 2.
- The shared material contract must not grow into a renderer-wide batching trait: SDF record building, text glyph/path data, panel-shape path packing, bounds, upload lifecycles, and batch resources remain renderer-specific unless a later implementation phase proves a smaller concrete helper removes duplication.
- Material projection produces `MaterialSlotValues` for the table row plus `PipelineCompatibility` and `ResourceCompatibility` for batch selection. If compatibility changes, the record goes to a different batch; if only values change, the record stays in the same batch and reads a different row value.
- Define one shared path from compatibility keys back into batch render material assets. `ResourceCompatibility` must not only key batches; SDF and path batch-material creation must copy or apply the texture handles, sampler/image resources, UV-channel requirements, and other bind-group-affecting fields into the `StandardMaterial` half of `SdfExtendedMaterial` / `PathExtendedMaterial`. Add a small shared helper such as `apply_resource_compatibility_to_standard_material(...)` only if it removes duplication without hiding renderer-specific policy.
- Add debug/test assertions that all records in a batch agree on non-table resource/pipeline properties: texture set, alpha mode, lighting/unlit mode, `double_sided`, `cull_mode`, shadow/prepass policy, and any shader-def-driving field.
- Add extract/prepare/rebind architecture:
  - `MaterialTablePlugin` owns the schedule wiring;
  - `extract_frame_material_table` carries the current frame's rows into the render world;
  - main-world `ensure_material_table_buffer_handle` creates/replaces the `ShaderBuffer` asset handle when the current frame exceeds table capacity;
  - register/unregister batch material handles synchronously when `SdfBatchResources` / `PathBatchResources` material handles are created, grown/replaced, or despawned; purge stale handles before rewriting table-buffer handles;
  - all batch reconcile/growth and table-capacity updates run before `rebind_registered_material_table_buffers`;
  - add a named `BatchResourcesReady` boundary before `rebind_registered_material_table_buffers`; all fill/text/shape batch material creation, growth, replacement, registration, and unregistration must finish before this boundary;
  - add a named `MaterialTableUpdatedToCurrent` set; after this set runs, no table capacity changes and no registered material handle creation/growth/replacement are allowed before extraction;
  - main-world `rebind_registered_material_table_buffers` runs in `MaterialTableUpdatedToCurrent` after `BatchResourcesReady` and last in `PostUpdate` before extraction, rewriting registered `Assets<PathExtendedMaterial>` and fill-material assets so extracted material assets already point at the current table buffer handle;
  - render-world `prepare_material_table_buffer` uploads the extracted `MaterialSlotValues` array into the already-matched table buffer asset before Bevy prepares `ShaderBuffer` assets and before material bind-group preparation;
  - render-world material prepare must only see material assets whose table-buffer handle was rewritten before extraction.
- Add explicit rebind-order tests: grow the table after multiple registered `SdfExtendedMaterial` and `PathExtendedMaterial` handles exist, purge stale registered handles, and assert every extracted render-world material points at the current table-buffer handle before material bind-group preparation.
- Compute a maximum row count from render-device storage-buffer limits and expose high-water warnings. If the current frame would exceed the limit, `append_material_slot(...)` returns `DroppedLimit`; producers skip the entire affected render record when any required material entry cannot append. Multi-entry SDF records must preflight or roll back so a fill cannot be emitted after its required border row failed. Never encode `INVALID` for a required material entry.
- Add a measurement harness in this phase before any renderer consumes the table:
  - build synthetic SDF, text, and panel-shape material-entry sets at small, medium, and stress sizes;
  - measure table build time, row count, upload bytes, buffer capacity growth, and allocation count;
  - include scalar/vector material animation, full topology churn, hidden/clipped removal, and mixed text/SDF/shape cases;
  - compare row count to live rendered material-entry count;
  - define pass/fail thresholds before Phase 3 consumes the table: steady-state allocation count, max build time for small/medium/stress cases on the local benchmark profile, max upload bytes/row counts for configured scenes, and the threshold that stops implementation and amends the appendix durable-slot alternative.
- Define binding layout constants together in one Rust location and one mirrored WGSL location, with comments beside each constant explaining the buffer/resource it binds:
  - existing analytic path bindings stay grouped as `100..=105`;
  - `MATERIAL_TABLE_BINDING = 106` is the shared `MaterialSlotValues` table binding used by both `SdfExtension` and `PathExtension`;
  - batched SDF bindings use `SDF_RENDER_RECORDS_BINDING` and `SDF_MESH_BINDING`;
  - keep these constants adjacent in source and WGSL so future readers can see the full material/batch binding layout without hunting across modules;
  - tests assert Rust/WGSL parity, no duplicate binding numbers within the relevant material group, and that `PathExtension` and `SdfExtension` expose the same material-table binding and compatible shader visibility.
- Add a batch-material registry API. It can have no registrants in this phase; fill/SDF batches register in Phase 3, analytic `PathExtendedMaterial` registers in Phase 6, and text/panel-shape producers migrate in Phases 8 and 9.
- Add a `StandardMaterialUniform` shader-size/static-layout assertion so Bevy material uniform drift fails loudly.
- Gate unused helper APIs with a narrow `#[cfg_attr(not(test), expect(dead_code, reason = "..."))]` until Phase 3 consumes them.

**Files:**
- `crates/bevy_diegetic/src/render/material_table.rs` (new) — `MaterialSlotId`, `SdfPaintMaterial`, `INVALID_GPU_MATERIAL_SLOT`, typed ordinal newtypes needed across APIs, `FrameMaterialTable`, `FrameMaterialTableBuilder`, `FrameMaterialTableBuild`, `MaterialSlotValues`, `MaterialSlotAppend`, `MaterialTablePlugin`, `MaterialTableUpdatedToCurrent`, main-world buffer handle/rebind systems, render-world upload system, binding constants, registry, layout assertion, tests, and measurement harness.
- `crates/bevy_diegetic/src/render/panel_text/relationship.rs` — reference pattern for Bevy source traversal; relationships do not own material slots.
- `crates/bevy_diegetic/src/render/batch_key.rs` — `PipelineCompatibility` and `ResourceCompatibility`; do not add scalar/vector material values to these keys.
- `crates/bevy_diegetic/src/render/mod.rs` — module wiring and schedule registration.
- `crates/bevy_diegetic/src/render/analytic_paths/material.rs` — reference for material storage binding patterns and material prepare hazards.
- `crates/bevy_diegetic/src/render/analytic_paths/batching.rs` and `crates/bevy_diegetic/src/render/panel_text/batching.rs` — reference for padded buffer/growth discipline.

**Constraints from prior phases:** Phase 1 made `ResolvedSdfSurface` available for future fill/SDF routing and moved draw-order limit warnings off the old per-quad path. This phase intentionally chooses the frame-built dense table as the active implementation and leaves the durable slot allocator in the appendix only.

**Acceptance gate:** `cargo build -p bevy_diegetic`, `cargo +nightly fmt --all -- --check`, `cargo clippy -p bevy_diegetic --all-targets`, and `cargo nextest run -p bevy_diegetic` pass. Unit tests cover frame-local `MaterialSlotId` construction rejecting `u32::MAX`, `SdfPaintMaterial::NotAuthored` being the only CPU-side state that emits `u32::MAX`, `INVALID_GPU_MATERIAL_SLOT` never aliasing slot `0`, deterministic dense row assignment from one shared `FrameMaterialTableBuild`, no deduplication by scalar/vector-equal material value, exact row count for live rendered material entries across mixed SDF/text/shape frames, hidden/clipped/missing/removed sources producing no rows, scalar/vector `MaterialSlotValues` projection parity against Bevy `StandardMaterialUniform` conversion, non-table splitter assertions, binding-constant parity, row-limit overflow returning an explicit drop outcome and skipping whole affected records instead of producing invalid required entries, buffer capacity growth/rebind ordering with both SDF and path batch materials registered, `ResourceCompatibility` fields copied into created batch material assets, and the `StandardMaterialUniform` source-conversion assertion. Measurement output records table build time, row count, upload bytes, capacity, allocation count, and pass/fail threshold status for small/medium/stress cases. `rg -n "SharedMaterialAssignments|SharedMaterialTableEpoch|MaterialSlotAllocator" crates/bevy_diegetic/src` has no production hits.

### Phase 3 — Batched SDF Fill Path Using The Frame Table · status: todo

#### Work Order

**Goal:** A private/test-routed batched SDF path renders panel fill/border surfaces from `SdfRenderRecord` storage buffers and the frame material table, matching the current panel SDF path including prepass/shadow, OIT, clip, and ordering behavior.

**Spec:**
- Add `render/fill_batch.rs` for the new SDF/fill batch implementation. This is the first consumer of `FrameMaterialTable`. Keep `panel_geometry.rs` responsible for surface resolution, interaction mesh behavior, and the private old-path parity adapter only.
- SDF producers build material rows and records together for the current frame. Authored fill and border roles use the shared `MaterialSlotInput` / `append_material_slot(...)` path to append `MaterialSlotValues` through `FrameMaterialTableBuilder`; the SDF record builder stores the returned frame-local `MaterialSlotId` in `SdfRenderRecord`. `SdfPaintMaterial::NotAuthored` roles skip projection and table append.
- Define structural SDF material-source identity rather than SDF surface entities:
  - `SdfMaterialSourceKey { panel: Entity, command_index: CommandIndex, role: SdfMaterialRole }`;
  - `SdfMaterialRole { Fill, Border }`;
  - `SdfMaterialSlotInput<'a> { key: SdfMaterialSourceKey, base_material: &'a StandardMaterial, color_override: Option<Color> }`;
  - `impl MaterialSlotInput for SdfMaterialSlotInput<'_>`.
- Do not create entities for SDF fill or border roles. Future widgets may materialize semantic widget entities, and panel shapes/text runs are source entities, but a panel background fill or border is a render role inside a resolved SDF command. Widget/preset output can still produce SDF roles that use `SdfMaterialSourceKey`; the widget entity is not the fill or border identity.
- Define `SdfRenderRecord` as a `ShaderType` with a `SHADER_SIZE` assertion. It must hold only geometry, ordering, and material indices:
  - `transform: Mat4`;
  - `half_size: Vec2`;
  - `mesh_half_size: Vec2`;
  - `corner_radii: Vec4`;
  - `border_widths: Vec4`;
  - `clip_rect: Vec4`;
  - `fill_material: u32`, encoded from `SdfPaintMaterial` with `INVALID_GPU_MATERIAL_SLOT` for `NotAuthored`;
  - `border_material: u32`, encoded from `SdfPaintMaterial` with `INVALID_GPU_MATERIAL_SLOT` for `NotAuthored`;
  - flags indicating absent fill/border roles so the shader can skip table reads for missing roles;
  - `depth_nudge: f32`;
  - `oit_depth_offset: f32`;
  - any flags required by the shader.
- `SdfRenderRecord` must not contain fill color, border color, metallic, roughness, reflectance, emissive, or `ior`; those values come from `material_table`. Alpha used for normal color composition comes from the table. Prepass/shadow variants must either keep the table binding available or use explicitly denormalized prepass-only alpha/shadow flags recorded from the same material source.
- Define fill-only/border-only/not-authored semantics explicitly. CPU retained records use `SdfPaintMaterial` for SDF fill and border material fields; `SdfRenderRecord::from_resolved(...)` converts `SdfPaintMaterial::NotAuthored` and padded records to `INVALID_GPU_MATERIAL_SLOT`. Slot counts must equal the live fill/border material-entry count: fill-only uses `fill_material = Authored(...)` and `border_material = NotAuthored`, border-only uses `fill_material = NotAuthored` and `border_material = Authored(...)`, no-border allocates no border slot, and zero-alpha fill still follows the current shadow/prepass policy without inventing a visible material entry.
- Add CPU retained records that also carry source identity, full `DrawCommandDepth`, batch key, and enough data to detect routing changes. Keep the public CPU records typed; `SdfRenderRecord` is the private GPU mirror built by `SdfRenderRecord::from_resolved(...)`.
- Use CPU-side wrapper/enums such as `SdfHalfSize`, `MeshHalfSize`, `LocalClipRect`, and `SdfPaintMask` instead of passing unlabelled `Vec2`/`Vec4`/`u32` fields through SDF record-builder APIs. `SdfHalfSize` is the local rounded-rectangle SDF form half-size expressed in world units before transform; do not name it `WorldHalfSize`. `SdfPaintMask` records fill/border role-presence bits derived from `SdfPaintMaterial` so WGSL can skip table reads before checking material slot ids; do not use the generic name `FillFlags`.
- Define fill/SDF batch keys:
  - `SdfBatchKey` is the map key for one `SdfBatchResources` entry;
  - it carries sort/order fields directly, including `panel_entity`, `z_level`, render view/layers as needed, and `contiguous_drawn_run: ContiguousDrawnRun` if required to preserve interleaving;
  - it carries `pipeline_compatibility: PipelineCompatibility` and `resource_compatibility: ResourceCompatibility` directly;
  - it carries SDF-specific batch splitters directly only when required by the SDF shader or pass routing, with doc comments explaining the reason;
  - OIT routing may omit `panel_entity` only behind tests; the first private implementation should keep batch routing panel-scoped.
- Define `ContiguousDrawnRun` if SDF or path batching needs to split otherwise-compatible records across intervening draws. Its doc comment must include this exact example: panel order is `background`, `text`, `border`; `background` and `border` are both SDF-compatible, but they must not merge into one SDF batch because that would draw `background`, `border`, `text`.
- Preserve cross-batch draw-order interleaving. Compatibility-key batching must split into maximal compatible runs in `DrawCommandDepth::ordinal_index()` order, or use `ContiguousDrawnRun` on `SdfBatchKey`. A command sequence such as A-compatible, B-compatible, A-compatible must not render as one A draw and one B draw.
- Define a fill texture-set key constructor instead of reusing `InternedMaterialKey` unchanged. It must include texture handles and pipeline/bind-group splitters, but exclude all scalar/vector PBR fields. A future `StandardMaterial` scalar must not silently enter the key.
- Split draw compatibility from material values with direct `pipeline_compatibility: PipelineCompatibility` and `resource_compatibility: ResourceCompatibility` fields on `SdfBatchKey`. `PipelineCompatibility` includes shared material-driven pipeline/specialization requirements. `ResourceCompatibility` includes material texture handles and bind-group resource requirements. Scalar/vector PBR values must be unrepresentable in all compatibility key types.
- `SdfBatchKey` must not contain `BaseMaterialId`, `VisualMaterialInterner`, `InternedMaterialKey`, `VisualBatchKey`, or any scalar/vector PBR value.
- For fills, `resolve_material` output and element colors should populate material-table slots, not batch-key material values.
- Build material rows directly from each live SDF record role while traversing current draw order. Scalar/vector material edits, including base-color alpha edits, update current-frame table rows without changing `SdfBatchKey`. Texture, alpha mode, `double_sided`, `cull_mode`, lighting/unlit, shadow/prepass, or other compatibility changes update `pipeline_compatibility` or `resource_compatibility` and move the record to the correct batch when required.
- Treat `DiegeticPanel` material inputs, element material/color inputs, and `SurfaceShadow` changes as first-class fill invalidation inputs even when `ComputedDiegeticPanel` and layout did not change. Scalar/vector material changes refresh table rows; shadow/prepass policy changes re-key or reroute the affected batch.
- Treat panel visibility, render layers, and removals as first-class invalidation inputs. Fill routing must react to `Visibility`, `RenderLayers`, and panel-removal changes even when layout and `ComputedDiegeticPanel` did not change.
- Upload SDF records in CPU draw order: sort by `DrawCommandDepth::ordinal_index()` and then by command index inside the batch. One draw cannot rely on Bevy transparent-item sorting between records, so record order is the in-batch composition order for overlapping translucent fills.
- Implement `SdfBatchResources` lifecycle like text `PathBatchResources`:
  - `records: Handle<ShaderBuffer>`;
  - inert capacity mesh;
  - material;
  - capacity;
  - padded same-capacity `set_data`;
  - growth that creates new buffers/mesh and rewrites material handles.
- Define `SdfExtendedMaterial = ExtendedMaterial<StandardMaterial, SdfExtension>`, following the current `PathExtendedMaterial` pattern. `SdfExtension` owns the SDF render-record buffer, material-table buffer binding, and any batch-level uniforms/pipeline switches. It must not store per-surface color, border color, geometry, clip, or corner radii in material uniforms; those belong in `SdfRenderRecord` and `MaterialSlotValues`.
- Migrate away from the current forced SDF render-material policy. `SdfMaterialSlotInput` projects scalar/vector PBR values from the resolved source material into `MaterialSlotValues`, including base-color alpha. Before the handle cascade migration, the resolved source may still come from the current owned `StandardMaterial` panel/element fields; after Phase 7, it comes from `SdfMaterial` handle resolution. `PipelineCompatibility` carries authored source alpha mode, `double_sided`, `cull_mode`, lighting/unlit policy, and other pipeline facts that must agree within one SDF batch. Construct each `SdfExtendedMaterial` batch asset from its `SdfBatchKey` compatibility values so Bevy specialization, culling, material flags, and material resources reflect the authored policy for that batch.
- Build SDF batch materials from the exact compatibility fields: convert `BatchAlphaMode` back to the authored `AlphaMode` or an explicitly documented pipeline-safe equivalent, copy `double_sided`, copy `cull_mode`, and copy lighting/unlit policy from `PipelineCompatibility`. Do not rebuild those fields from `Sidedness` unless the authored source was actually a `Sidedness` override.
- Preserve SDF-friendly defaults without hiding user control. The default SDF source material may use `AlphaMode::Blend`, `double_sided = true`, and `cull_mode = None`, but authored panel/element material sources can override those fields. Before Phase 7 these sources are the current owned `StandardMaterial` fields; after Phase 7 they are `SdfMaterial` / element material handles. Tests must cover both the default behavior and authored overrides.
- Register fill batch materials with the material-table registry so table growth rewrites their table-buffer handles in the ordered rebind pass.
- Add batch bounds in this phase. Compute bounds from transformed `mesh_half_size` corners, including AA padding/clip behavior, write `Aabb`, set batch entity translation, attach `NoAutoAabb`, and schedule the update between Bevy bounds calculation and visibility checks.
- State the transform convention explicitly. `SdfRenderRecord::transform` is world-space and is the only transform used by the shader. The batch entity `Transform` / `GlobalTransform` exists only for visibility and transparent sort placement. The `Aabb` is local to that batch entity and is derived by subtracting the chosen batch center from the world-space union.
- Add a fill transform/update schedule contract matching text batching:
  - clear and rebuild the frame material table once for the frame;
  - resolve/route SDF records and append their material rows before `TransformSystems::Propagate`;
  - handle `MaterialSlotAppend::DroppedLimit` by skipping the entire affected SDF record if any authored fill/border entry required by that record cannot append;
  - update SDF record world transforms, transparent sort centers, batch entity placement, and hand-written local `Aabb` from panel `GlobalTransform` only after transform propagation;
  - update hand-written `Aabb` before `VisibilitySystems::CheckVisibility`;
  - create/grow/register `SdfBatchResources` materials in `BatchResourcesReady`;
  - commit/upload fill buffers only after transform, bounds, visibility invalidation, and registration updates have run.
- Add vertex-pulled SDF shader path:
  - subtract `mesh[instance_index].first_vertex_index` before deriving local vertex index;
  - derive record index and quad corner from local index;
  - collapse capacity-tail records before material-table reads;
  - transform corners from `record.mesh_half_size`;
  - apply `depth_nudge` only when `OIT_ENABLED` is absent;
  - pass local SDF coordinates and record index to fragment logic.
- Add a shared WGSL material-table helper named `pbr_input_from_material_table` for the SDF fill shader now and analytic path shader later. It should populate Bevy PBR material fields from repo-owned `MaterialSlotValues` and call existing PBR lighting functions rather than forking lighting. The helper must guard reads in this order: role-present flag, `id != INVALID_GPU_MATERIAL_SLOT`, and `id < arrayLength(&material_table)`. It returns a collapsed/transparent material without reading material fields for absent, sentinel, or out-of-bounds ids. Stale-record safety in the active plan comes from rebuilding records and table rows together for the frame, plus sentinel/out-of-bounds guards for padded records.
- Require all SDF and analytic WGSL material-table reads to go through this helper. Add shader source tests or tripwires that reject direct `material_table[...]` reads outside the helper, including capacity-tail and not-authored role paths.
- Preserve current SDF fragment behavior:
  - rounded rectangle distance;
  - per-side borders;
  - border/fill composition using the fill and border material slot fields;
  - clip discard;
  - subpixel inflation;
  - `SDF_AA_PADDING` behavior;
  - OIT offset via `record.oit_depth_offset`;
  - border-only and zero-alpha prepass/shadow behavior.
- Preserve the current prepass/shadow semantics from `sdf_panel.wgsl`: `fill_alpha > 0.001` is the fill-present threshold for border-only prepass behavior, border-only surfaces discard the transparent interior, and nonzero borders are inflated to at least a 1px screen-space footprint in the prepass so shadow casters remain readable. If the batched prepass reads the material table, the prepass material group must include the table binding and guard absent/sentinel/out-of-bounds material ids before any table read. If the batched prepass uses denormalized fields instead, `SdfRenderRecord` must include doc-commented prepass-only fill alpha and border-present bits derived from the same `SdfPaintMaterial` slots, and tests must compare those fields with the projected table values.
- Define the batched SDF prepass/shadow material-group strategy explicitly. Batched SDF prepass/shadow pipelines must keep the material group and table binding available anywhere alpha/material data affects clipping, depth, or shadow behavior. If Bevy attempts a stripped material-group variant, specialize/guard it like the analytic path and reject or disable only cases proven visually equivalent. Cover `SurfaceShadow::On`, border-only surfaces, zero-alpha fills, and depth-only shadow/prepass pipeline creation.
- Define `sdf_batch_alpha_mode` as the SDF batch compatibility rule for authored source alpha mode. The default SDF material can use `AlphaMode::Blend` because SDF computes per-fragment alpha itself, but authored alpha mode remains a real splitter stored as `BatchAlphaMode`. The definition must state how table-row base-color alpha affects transparent fill composition, how `fill_alpha > 0.001` affects border-only prepass/shadow routing, and how `AlphaMode::Opaque`, `Mask`, `Blend`, `Premultiplied`, `Add`, `Multiply`, and `AlphaToCoverage` route through Bevy phases for SDF batches. If Bevy's depth/prepass routing strips the material bind group for a given authored mode, keep the authored mode in `PipelineCompatibility` and map only the GPU material to a pixel-equivalent, material-group-retaining mode, following the current text `Opaque -> Mask(0.0)` pattern. Such a mapping requires a focused parity test before use.
- Keep the old `sdf_panel.wgsl` entry contract only as the private parity oracle while panel SDFs migrate. The production batched path uses `SdfExtendedMaterial`, `SdfExtension`, and `SdfRenderRecord`.
- Add a dedicated shader hash/tripwire for the new fill shader plus shared material-table WGSL helper. Do not refresh `EXPECTED_SHADER_FNV1A` unless `analytic_path.wgsl` changes.
- Add automated screenshot or pixel-diff validation before the old route is deleted. The private batched path must be compared against the old SDF route for clipped AA edges, overlapping translucent fills, OIT and non-OIT panels, border-only and zero-alpha shadow/prepass behavior, scalar PBR differences, and cross-batch interleaving.
- Keep production routing on the old path in this phase, but do not expose a public alternate route/config. The old path exists only as a parity oracle while the private/test batched path is proven.

**Files:**
- `crates/bevy_diegetic/src/render/fill_batch.rs` (new) — `SdfRenderRecord`, typed CPU retained records, `SdfRenderRecord::from_resolved(...)`, SDF fill/border `SdfPaintMaterial` fields, material refresh/re-key systems, not-authored material fields encoded as `INVALID_GPU_MATERIAL_SLOT` in GPU records, batch store, `SdfBatchResources`, padded buffers, transform propagation follow-up, visibility/render-layer invalidation, in-batch draw ordering, bounds/visibility metadata, batch lifecycle, tests.
- `crates/bevy_diegetic/src/render/panel_geometry.rs` — feed `ResolvedSdfSurface` into private/test batch builder while production behavior remains unchanged.
- `crates/bevy_diegetic/src/render/batch_key.rs` — fill texture-set key constructor or equivalent batch-key support; `PipelineCompatibility` and `ResourceCompatibility` typed splitters; no `VisualBatchKey`/`BaseMaterialId` use in new SDF keys.
- `crates/bevy_diegetic/src/render/material.rs` — support resolving table material values without placing scalar values in the fill batch key.
- `crates/bevy_diegetic/src/render/draw_order.rs` — add `fill_batch_depth_bias(z_level)` or `sdf_batch_depth_bias(z_level)` below same-level line/text lanes.
- `crates/bevy_diegetic/src/render/sdf_material.rs` or `crates/bevy_diegetic/src/render/fill_batch.rs` — add final batched `SdfExtendedMaterial` and `SdfExtension`; keep them separate in meaning from transition-only `LegacySdfExtendedMaterial`.
- `crates/bevy_diegetic/src/shaders/sdf_panel_vertex_pull.wgsl` (new) — vertex-pulled SDF stage.
- `crates/bevy_diegetic/src/shaders/sdf_material_table.wgsl` or similar (new) — frame table helper.
- `crates/bevy_diegetic/src/shaders/sdf_panel.wgsl` — share or mirror SDF fragment helpers as needed.
- `crates/bevy_diegetic/src/lib.rs` — embedded shader registration.
- `crates/bevy_diegetic/src/text/slug/glyph/coverage_probe.rs` or a new probe module — dedicated fill shader/material-table hash.

**Constraints from prior phases:** Phase 1 provides `ResolvedSdfSurface` and moved draw-order limit warnings out of the old path. Phase 2 provides `FrameMaterialTable`, `FrameMaterialTableBuilder`, frame-local `MaterialSlotId`, `MaterialSlotValues`, padded table buffer, binding constants, registry, measurement output, and main-world ordered rebind before extraction.

**Acceptance gate:** `cargo build -p bevy_diegetic`, `cargo +nightly fmt --all -- --check`, `cargo clippy -p bevy_diegetic --all-targets`, and `cargo nextest run -p bevy_diegetic` pass. Tests prove: SDF producers append frame material rows while building current records through the single frame table builder; `SdfBatchKey` cannot contain `BaseMaterialId`, `VisualMaterialInterner`, `InternedMaterialKey`, `VisualBatchKey`, or scalar PBR fields; batched records match old resolved geometry/depth/material inputs except for the deliberate new ability to honor authored alpha mode, sidedness, and culling; pre-propagation resolved SDF data is panel-local and post-propagation systems produce world transforms, sort centers, batch entity placement, and bounds; fill material-only changes update table rows without batch splits when resource/pipeline splitters are unchanged; base-color alpha edits update table rows; source `AlphaMode`, `double_sided`, and `cull_mode` differences split SDF batches and configure the batch render material from compatibility values; `ResourceCompatibility` texture/sampler fields are applied to the created `SdfExtendedMaterial` assets; `cull_mode = None`, `Some(Face::Back)`, and `Some(Face::Front)` remain distinguishable; shadow/prepass/texture/lighting compatibility changes re-key batches when required; interleaved compatibility keys preserve command order by splitting contiguous draw-order segments; two fills differing only in scalar PBR values share one batch; texture differences split; fill-only, border-only, zero-alpha fill, no-border, and not-authored cases append exactly the live material rows and avoid table reads for `SdfPaintMaterial::NotAuthored`; row-limit overflow skips whole affected SDF records and never emits a required-role sentinel; slot `0` can be live while not-authored/padded records use `INVALID_GPU_MATERIAL_SLOT` and perform no table read; padding/sentinel/out-of-bounds records do not read live material rows; shader source tests reject direct material-table reads outside the helper; `SurfaceShadow::On`, border-only, zero-alpha fill, transparent fill, depth-only shadow/prepass behavior, table-bound prepass variants, `sdf_batch_alpha_mode`, and prepass material-group creation match old path or explicitly documented authored-policy behavior; toggling panel `Visibility` or `RenderLayers` without layout changes removes or re-keys records that frame; moving/rotating panels update SDF record transforms, sort centers, and bounds after transform propagation with no double-applied transform; CPU record upload order follows `DrawCommandDepth::ordinal_index()` then command index; overlapping translucent fills in one batch and across interleaved compatible segments compose like the old path; OIT and non-OIT two-panel overlap cases match old `DrawCommandDepth` behavior when union-center screen sort differs from individual surfaces; inert-mesh batch bounds participate in visibility correctly; automated old-vs-batched screenshots or pixel diffs pass for the required panel SDF scenes.

### Phase 4 — Switch SDF Production Routing And Delete The Old Quad Path · status: todo

#### Work Order

**Goal:** Default SDF panel fill/border surfaces render through the frame-table batched path in production, and the old per-surface `PanelSdfMesh` / `LegacySdfExtendedMaterial` route is deleted in the same phase.

**Spec:**
- Replace production `reconcile_sdf_quads` routing with the SDF batch store from Phase 3.
- Delete the old product path, not just hide it:
  - remove `PanelSdfMesh`;
  - remove `PanelSdfSurface` if no remaining test projection needs it;
  - remove `BuiltSdfQuad`;
  - remove `build_sdf_quad_from_resolved` if it is only old-path glue;
  - remove `spawn_sdf_quad`;
  - remove per-surface `LegacySdfExtendedMaterial` asset churn for panel chrome;
  - remove `LegacySdfExtendedMaterial` and `LegacySdfExtension` entirely after panel SDFs migrate. Phase 0 removed the callout SDF route; if implementation discovers another real user, stop and amend this plan before preserving the old material path.
- Keep `PanelInteractionMesh` behavior unchanged. This plan batches visual SDF surfaces, not the invisible picking/interaction quad.
- Preserve or port every old-path test that encoded behavior:
  - text toggles update SDF OIT depth offset while identity holds;
  - overlapping backings order identically on sorted and OIT paths;
  - color/material-only changes do not create new render entities;
  - clip padding keeps AA at clipped edges;
  - hidden/despawned panels emit zero records.
- If any explicit `SurfaceShadow::On` panel behavior cannot be preserved by the batched shader in this phase, do not keep a public alternate route. Fix the batched prepass/shadow path or keep the phase incomplete.
- Update `PanelGeometryPerfStats`:
  - add `sdf_batches`;
  - add `sdf_records`;
  - add `sdf_uploads`;
  - remove or rename `sdf_quads` once no old `PanelSdfMesh` entities remain.
- Update diagnostics constants/public perf docs and examples that display SDF surface counts.
- Keep the Phase 3 batch bounds implementation active in production: hand-written `Aabb`, batch entity translation, and `NoAutoAabb` must cover all records even though the mesh is inert.
- Add a production-system assertion or test proving SDF batching is the only panel-chrome visual route. `LegacySdfExtendedMaterial` and `LegacySdfExtension` must have no production or probe-side users after this phase. If another real user is discovered during implementation, stop and amend this plan before continuing.

**Files:**
- `crates/bevy_diegetic/src/render/panel_geometry.rs` — switch production route and delete old SDF quad renderer.
- `crates/bevy_diegetic/src/render/fill_batch.rs` — production batch store, bounds, upload, and lifecycle systems.
- `crates/bevy_diegetic/src/render/sdf_material.rs` — remove `LegacySdfExtendedMaterial` / `LegacySdfExtension` after panel migration; keep final batched `SdfExtendedMaterial` / `SdfExtension`.
- `crates/bevy_diegetic/src/panel/perf.rs` — SDF batch/record/upload counters.
- `crates/bevy_diegetic/src/panel/constants.rs` — diagnostics paths for new counters and removal/rename of old quad counter.
- `crates/bevy_diegetic/src/render/mod.rs` — schedule production systems in the right sets.
- `crates/bevy_diegetic/examples/diegetic_text_stress.rs`, `crates/bevy_diegetic/examples/units.rs`, `crates/bevy_diegetic/examples/panel_draw_order.rs` — update stats/visual validation expectations as needed.

**Constraints from prior phases:** Phase 0 removed the callout/standalone SDF primitive route. Phase 3 proved the batched panel SDF path against the old path using the frame material table, including batch bounds, transform updates after propagation, visibility/render-layer invalidation, prepass/shadow material-group behavior, in-batch draw order, and OIT/non-OIT overlap cases. The old path may still exist only as internal parity scaffold before this phase; this phase removes it before completion. Phase 2's table registry and ordered rebind are active for `SdfExtendedMaterial`.

**Acceptance gate:** `cargo build -p bevy_diegetic`, `cargo build --workspace --all-features --examples`, `cargo +nightly fmt --all -- --check`, `cargo clippy -p bevy_diegetic --all-targets`, and `cargo nextest run -p bevy_diegetic` pass. `rg -n "PanelSdfMesh|spawn_sdf_quad|PanelSdfSurface|BuiltSdfQuad|build_sdf_quad_from_resolved|reconcile_sdf_quads|LegacySdfExtendedMaterial|LegacySdfExtension|sdf_quads|legacy_sdf|old_sdf|sdf_route" crates/bevy_diegetic/src` finds no old panel-chrome renderer, old stats, or route switch. Callout examples, if retained, compile/render through analytic path / panel-shape rendering, not `SdfExtendedMaterial`. `diegetic_text_stress` reports SDF batches/records/uploads. `units` and `panel_draw_order` remain visually correct.

### Phase 5 — Material-Table Validation Example And SDF Stats · status: todo

#### Work Order

**Goal:** A runnable example and tests prove SDF/fill batching stays stable under animated per-element material values and establish the mixed-content batch-stat readout that later phases extend to text and panel shapes.

**Spec:**
- Continue from the existing `crates/bevy_diegetic/examples/batch_validation.rs` scene rather than adding a second batching example.
- Keep the example structured as labeled validation panels. The scene already contains SDF/fill, text, and panel-shape content; in this phase the SDF/fill counters become authoritative and later phases replace placeholder/pending rows for text, panel shapes, draw counts, and material-table counts with observed values.
- The example already ships a `material table` stats section whose rows display the `MATERIAL_TABLE_PENDING` placeholder (`pending`): `sdf batches`, `sdf records`, `sdf uploads`, `table rows`, `table bytes`, `table capacity`. This phase wires each row to its observed counter from the global material-table stats below and removes the `MATERIAL_TABLE_PENDING` placeholder constant. Leaving any row on `pending` fails this phase's acceptance.
- The SDF panels should include:
  - multiple fills/borders that use different `StandardMaterial` sources but differ only by scalar/vector values such as base color, emissive, metallic, perceptual roughness, or reflectance, and therefore share one compatible batch;
  - at least one otherwise similar fill/border whose material differs by a splitter such as `AlphaMode`, sidedness, culling, texture resource, lighting mode, or shadow/prepass policy, and therefore creates a separate batch;
  - one fill whose source material carries real textures (at minimum `base_color_texture` for a visible image, and ideally a second channel such as `emissive_texture` to exercise non-base-color sampling), proving the textures sample across the fill's element-local box UV (`0..1`) per the "Texture-backed material sampling" principle while the texture handles also split it into its own batch (reason "texture resource splits"). SDF already samples all channels through `pbr_input_from_standard_material`; this phase only adds the visible-result assertion;
  - animated scalar/vector material changes that keep batch count stable after initial allocation.
- Each validation panel must display or log its expected and observed counts:
  - batch count;
  - record count;
  - material slot count;
  - upload count;
  - short reason for the expected split/share, e.g. "scalar/vector materials share", "alpha mode splits", or "texture resource splits".
- Display or log global material-table stats:
  - SDF batch count;
  - SDF record count;
  - SDF upload count;
  - material-table current-frame rows;
  - material-table upload bytes;
  - material-table capacity.
- Tests should assert:
  - animating per-element color over multiple frames keeps live slot count equal to live material-entry count;
  - table capacity stays stable once no new elements are added;
  - batch count does not change for scalar/vector material animation;
  - independent `StandardMaterial` sources that differ only by scalar/vector table values share the same SDF batch while keeping distinct material slots;
  - otherwise matching SDF materials with different alpha mode, `double_sided`, `cull_mode`, texture resources, lighting mode, or shadow/prepass policy split into separate batches;
  - `cull_mode = None`, `Some(Face::Back)`, and `Some(Face::Front)` are represented as distinct compatibility values when authored that way;
  - record count follows live rendered surfaces;
  - removing a panel removes both fill and border rows from the next frame-built table;
  - respawning a panel creates current-frame rows without relying on durable slot reuse.
- Keep example UI/log text focused on measured values and expected split reasons, not implementation explanation.
- Keep the example on the shared Fairy Dust instrumentation helpers (`diegetic_stats_sections_panel`, `diegetic_stats_sections_tree`, `diegetic_stats_panel`, `diegetic_stats_tree`, `fps_stats_panel`, `gpu_meter_panel`) instead of reintroducing copied screen-panel styling. Use named stats sections for `batch_validation` so authored counts, observed renderer counts, and material-table counters can grow independently while `diegetic_text_stress` keeps the flat row wrapper.

**Files:**
- `crates/bevy_diegetic/examples/batch_validation.rs` — validation panels, per-panel expected/observed batch readouts, and animated material-table fill batching example.
- `crates/fairy_dust/src/screen_panels/performance.rs` — update shared instrumentation helpers only when the example needs a reusable panel shape also useful to `diegetic_text_stress`.
- `crates/bevy_diegetic/src/render/material_table.rs` — stats accessors/tests if Phase 2 accessors need production polish.
- `crates/bevy_diegetic/src/render/fill_batch.rs` — churn tests and exposed counters.
- `crates/bevy_diegetic/src/panel/perf.rs` — any additional stats required by the example.

**Constraints from prior phases:** Phase 4 has deleted the old SDF quad path. SDF/fill surfaces now use `FrameMaterialTable` and expose SDF batch/record/upload counters. Phase 2 provides table statistics accessors and the frame-built table measurement harness.

**Acceptance gate:** `cargo build --workspace --all-features --examples`, `cargo +nightly fmt --all -- --check`, `cargo clippy -p bevy_diegetic --all-targets`, and `cargo nextest run -p bevy_diegetic` pass. `batch_validation` runs and shows stable batch count plus flat material-table capacity under material-value animation after initial growth.

### Phase 6 — Prepare Analytic Paths For Material Slots · status: todo

#### Work Order

**Goal:** Analytic-path GPU records, shaders, and materials can read shared material-table slots, while existing text and panel-shape producers still render unchanged.

**Spec:**
- Keep text and panel-shape analytic rendering on the existing shared `PathExtendedMaterial` / `PathExtension`. This phase changes the record layout, material-table binding, and batch-key semantics; it does not create separate `TextExtension` or `ShapeExtension` types.
- Rename analytic-path GPU records and indices while changing their material fields:
  - `PathRecord` becomes `PackedPathRecord`, meaning one packed analytic path's bounds and curve-band ranges;
  - `PathInstanceRecord` becomes `PathQuadRecord`, meaning one quad that places a packed analytic path in a batch;
  - `RunRecord` becomes `PathRenderRecord`, meaning transform, material slot, render mode, AA, and depth state for one text run or one panel-shape primitive;
  - `atlas_index` becomes `packed_path_index`;
  - `run_index` becomes `render_index`.
- Apply the same analytic-record rename to current documentation under `docs/bevy_diegetic/**`. Plan/current docs must use `PackedPathRecord`, `PathQuadRecord`, `PathRenderRecord`, `packed_path_index`, and `render_index`. Historical/as-built docs may preserve old names only when explicitly describing pre-rename history, and must add a short parenthetical mapping to the new name on first use in that section.
- Replace old `RunRecord::fill_color: Vec4` with `PathRenderRecord::material: MaterialSlotId` or an equivalent bare slot-id field.
- Re-assert `PackedPathRecord::SHADER_SIZE`, `PathQuadRecord::SHADER_SIZE`, and `PathRenderRecord::SHADER_SIZE` after the layout change.
- Update `analytic_path_vertex_pull.wgsl` and `analytic_path.wgsl` mirror structs for the new run record layout.
- Switch the analytic-path fragment path from per-run `fill_color` to the shared `pbr_input_from_material_table` helper introduced for SDF/fill batches.
- Add the shared material-table binding to `PathExtendedMaterial` and register analytic-path batch materials with the material-table registry so table growth rewrites their table-buffer handles in the same ordered rebind pass used by SDF batches.
- Preserve the current analytic prepass guard: vertex-pulled analytic batches keep prepass disabled unless this phase also implements and proves a material-group-retaining prepass. Extend stripped-group guard/tests to cover bindings 104, 105, and 106; no depth-only stripped analytic pipeline may use vertex-pull/table buffers.
- Replace analytic batch-key use of `BaseMaterialId`/`VisualMaterialInterner` with `PathBatchKey`, backed by resource-only `PipelineCompatibility` and `ResourceCompatibility` fields, before producer migration. If compatibility code is needed in this phase, name it `LegacyResourceCompatibility` and make scalar PBR fields unrepresentable in it.
- `PathBatchKey` is the final shared analytic-path batch key for both text and panel-shape path draws. It replaces the current generic analytic-path `BatchKey` name and either replaces `ShapeBatchKey` or makes any remaining shape-specific key a narrow wrapper whose reason is documented.
- Define `PathBatchKey` concretely:
  - it is the map key for one `PathBatchResources` entry;
  - it carries sort/order fields directly, including `z_level`, render layers/view scope, shadow/pass participation, and `contiguous_drawn_run: ContiguousDrawnRun` if required to preserve text/shape interleaving;
  - it carries `pipeline_compatibility: PipelineCompatibility` and `resource_compatibility: ResourceCompatibility` directly;
  - it carries analytic-path-specific batch splitters directly only when required by the path shader, vertex-pull route, atlas/resource buffers, AA mode, or render mode, with doc comments explaining the reason.
- Define `TextRunMaterialSourceKey { run: Entity }` for text material projection. This key identifies the authored text source before frame-table row allocation; it is not a `MaterialSlotId` value.
- Do not introduce `PanelShapeMaterialSourceKey { shape: Entity }` in this phase before panel-shape source entities exist. The Phase 6 panel-shape bridge must use a temporary key based on the current `PanelShapeSourceKey` / `PanelShapePrimitiveKey` or `PanelShapeRenderKey` identity, with scalar/vector material values removed from that key. Phase 9 replaces this bridge key with `PanelShapeMaterialSourceKey { shape: Entity }` when source entities are materialized.
- Define append-time analytic material input construction rules. `TextRunMaterialSlotInput<'a>` and `PanelShapeMaterialSlotInput<'a>` implement the private `MaterialSlotInput` contract from Phase 2:
  - borrow the appropriate base `StandardMaterial` from the current producer source in this bridge phase. Text uses the current renderer-resolved panel text material fallback and prepared text color; panel shapes use the current element/panel/default material and resolved primitive color. The handle cascade added in Phase 7 replaces these source lookups later without changing the table/record contract;
  - compute `MaterialSlotCandidate` from an effective scalar/vector material value set: start with the borrowed base `StandardMaterial`, apply only the authored scalar overrides for that producer, then project every table-supported PBR field into `MaterialSlotValues`;
  - do not clone the base material just to apply overrides unless implementation proves that is simpler and still not retained;
  - preserve current `TextStyle` builder semantics on top of the resolved base material: `with_color` supplies the effective `base_color` value, while `with_alpha_mode`, `with_lighting`, `with_sidedness`, and `with_unlit` feed compatibility/policy fields rather than scalar table values;
  - for the compile-enabling text bridge, use the current renderer-resolved text color source (`PreparedPanelText.fill_color`) as the `base_color` override so existing `TextStyle::with_color` behavior is preserved; do not treat color as a special shader path after projection;
  - for panel shapes, use the resolved primitive color as the `base_color` override and project the rest of the resolved material normally;
  - preserve scalar/vector PBR fields such as metallic, roughness, reflectance, emissive, transmission, thickness, attenuation, clearcoat, anisotropy, `ior`, and any future supported `StandardMaterial` value fields;
  - move texture handles, alpha/pipeline mode, sidedness, lighting/unlit mode, and shadow/prepass splitters into `ResourceCompatibility` / `PipelineCompatibility`;
  - never store authored color only or base material only in the table; every row is a full `MaterialSlotValues` projection for that rendered material role.
- Add a concrete producer-facing frame-table API in `render/analytic_paths/batching.rs`:
  - it accepts the resolved material source;
  - it appends a `MaterialSlotValues` row through `FrameMaterialTableBuilder` while building the current frame's `PathRenderRecord`;
  - it returns the frame-local `MaterialSlotId` immediately and never fabricates ids outside the builder;
  - hidden, clipped, missing-glyph, panel-removal, and shape-removal cases simply do not append rows or records for that frame.
- This phase must update every producer call site that constructs `RunRecord`, including text, panel-shape, and analytic probe paths, so the tree compiles after `RunRecord::fill_color` is removed and the type is renamed to `PathRenderRecord`. Because `MaterialSlotId` is frame-local, every live text and panel-shape render record must append or refresh its material row and rewrite its `PathRenderRecord` material id each frame, even when geometry, placement, and atlas data are unchanged. Changed-only producer gates may still skip geometry/atlas work, but they must not skip the per-frame material row/id refresh.
- Preserve dirtiness separation with assertions and tests at concrete producer/update boundaries, not with placeholder wrapper types. Material-value edits update only frame material table rows unless the slot id changes. Placement/depth/AA edits update only `PathRenderRecord` data. Path geometry edits update `PathQuadRecord` / packed-path atlas data. Add producer-local assertions in text and panel-shape update paths so material-only refreshes cannot mark path quad/atlas data dirty, placement-only updates cannot rebuild packed geometry, and geometry edits are the only path that rebuilds packed path data. Add named structs for these buckets only if implementation needs them, and review their full field shape before adding them.
- Add explicit slot-id-only dirty tests: hiding/removing an earlier material entry may renumber later frame-local rows, and unchanged later text/shape records must rewrite only their `PathRenderRecord.material` ids without rebuilding glyph/path geometry, path quads, or atlas data.
- Add explicit material-slot refresh APIs keyed by source identity, backed by the shared `append_material_slot(...)` helper and exposed through producer-specific wrappers where that keeps call sites clear. Scalar material edits refresh table rows even when text layout, shape geometry, z-level, and cascade did not change, and they must not set path-quad, path-render, or atlas dirty flags when slot id and splitters are unchanged.
- Text and panel-shape batch keys should retain only true splitters: texture/bind-group requirements, alpha/pipeline mode, lighting mode, sidedness, shadow participation, z-level, and render layers. Scalar material values must be table data, not key data.
- Refresh `EXPECTED_SHADER_FNV1A` in `coverage_probe.rs` because this phase edits `analytic_path.wgsl`.

**Files:**
- `crates/bevy_diegetic/src/render/analytic_paths/packing.rs` — `PackedPathRecord`, `PathQuadRecord`, `PathRenderRecord`, `packed_path_index`, `render_index`, material slot id field, typed CPU pieces if they live here, and size assertions.
- `crates/bevy_diegetic/src/render/analytic_paths/analytic_path_vertex_pull.wgsl` — path quad/render record mirror update.
- `crates/bevy_diegetic/src/render/analytic_paths/analytic_path.wgsl` — material-table PBR path.
- `crates/bevy_diegetic/src/render/analytic_paths/material.rs` — shared material-table binding, material-group/prepass compatibility, registered buffer handle.
- `crates/bevy_diegetic/src/render/analytic_paths/batching.rs` — `PathBatchStore` material-table registration, `PathBatchKey`, resource-only analytic batch keys, frame-table row API, source-identity material refresh path, and dirty-buffer separation.
- `crates/bevy_diegetic/src/render/panel_text/batching.rs` — compile-enabling bridge call-site changes to construct `PathRenderRecord` with `MaterialSlotId` while preserving current text behavior.
- `crates/bevy_diegetic/src/render/panel_shapes/batching.rs` — compile-enabling bridge call-site changes to construct `PathRenderRecord` with `MaterialSlotId` while preserving current panel-shape behavior.
- `crates/bevy_diegetic/src/render/analytic_line_probe.rs` — migrate probe run/material construction through the same frame-table material-source rules, or retire/delete the probe in this phase.
- `crates/bevy_diegetic/src/render/material_table.rs` — typed text/panel-shape source key support, `FrameMaterialTableBuilder`, and registry support for `PathExtendedMaterial`.
- `crates/bevy_diegetic/src/render/batch_key.rs` — analytic `PipelineCompatibility` and `ResourceCompatibility` support and removal of scalar material values from migrated analytic batch keys.
- `crates/bevy_diegetic/src/text/slug/glyph/coverage_probe.rs` — refresh analytic shader hash.
- `docs/bevy_diegetic/**` — analytic-record rename sweep and first-use mappings for any historical sections that intentionally retain old names.

**Constraints from prior phases:** Phase 2 provides the frame-built table, frame-local `MaterialSlotId`, registry, padded table buffer, main-world rebind before extraction, ordered render-world upload, and measurement output. Phase 3 provides the shared WGSL material-table PBR helper. Phase 4 proves production SDF/fill rendering on the frame table and deletes the old SDF quad path. Phase 5 proves table stability under animated material values.

**Acceptance gate:** `cargo build -p bevy_diegetic`, `cargo +nightly fmt --all -- --check`, `cargo clippy -p bevy_diegetic --all-targets`, and `cargo nextest run -p bevy_diegetic` pass, including the refreshed analytic shader FNV tripwire. Current text/panel-shape examples render unchanged through the frame-table API. Tests prove analytic batch keys no longer contain scalar PBR values; every old `RunRecord` producer compiles through `PathRenderRecord` after `fill_color` removal; analytic producers append rows only through the shared frame table builder; every live text/shape record rewrites its frame-local material slot id each frame; topology churn that renumbers rows rewrites only material ids for unchanged later records; material-source construction preserves non-white text/shape colors together with non-default scalar/vector PBR values; `ResourceCompatibility` fields are applied to created `PathExtendedMaterial` assets; stripped prepass/material-group guards still cover vertex-pull bindings 104, 105, and 106; material-only edits update table rows without rewriting path quad buffers when compatibility and geometry are unchanged; material-only text/shape refresh does not dirty path quad buffers or atlas data; source material changes without layout/geometry changes still refresh current-frame table rows. `rg -n "RunRecord|PathInstanceRecord|\\bPathRecord\\b|atlas_index|run_index" docs/bevy_diegetic` returns only explicitly historical passages that include a first-use mapping to `PathRenderRecord`, `PathQuadRecord`, `PackedPathRecord`, `packed_path_index`, or `render_index`; this plan's own Phase 6 rename map is allowed while the plan is active.

### Phase 7 — Generalize Cascade And Move Material Authoring To Handles · status: todo

#### Work Order

**Goal:** Material selection for SDF surfaces, text, and panel shapes resolves through the existing cascade model using `Handle<StandardMaterial>`, so later producer phases consume a single material-source model.

**Spec:**
- Generalize the cascade implementation from `Copy` attributes to `Clone` attributes:
  - change `CascadeProperty` from `Copy + PartialEq` to `Clone + PartialEq`;
  - change `Override<A>`, `Resolved<A>`, and `CascadeDefault<A>` to derive/require `Clone`, not `Copy`;
  - update `resolve`, `resolve_walk`, propagation, `cascade_default(...)`, typed readers, tests, and cascade macros to clone only when resolving or writing dirty nodes;
  - document that cascade attributes must remain cheap to clone; `Handle<StandardMaterial>` is allowed, owned `StandardMaterial` is not;
  - keep existing `Copy` attributes (`TextAlpha`, `FontUnit`, `Lighting`, `Sidedness`, `AntiAlias`, `HairlineFade`) behaviorally unchanged.
- Add material cascade attributes whose values are `Handle<StandardMaterial>`:
  - `SdfMaterial` for SDF backgrounds/borders and other element surfaces;
  - `TextMaterial` for text runs;
  - `ShapeMaterial` for panel-shape primitives.
- Document that these are cascade/source material handle attributes, not render material asset types. In particular, `SdfMaterial` must not be confused with final batched `SdfExtendedMaterial = ExtendedMaterial<StandardMaterial, SdfExtension>` or migration-only `LegacySdfExtendedMaterial = ExtendedMaterial<StandardMaterial, LegacySdfExtension>`.
- Add a seeded-default cascade path for non-`Default` attributes before adding material-handle cascades. The current `CascadePlugin<A>` calls `init_resource::<CascadeDefault<A>>()`, which is fine for existing value defaults but wrong for material handles because an invalid/default handle would hide setup bugs.
  - Keep the existing default-initialized path for current attributes whose `CascadeDefault<A>: Default` is real.
  - Add an explicit seeded-default path for material attributes, for example `CascadePlugin::<A>::expect_seeded_default()` or a similarly small API, that registers the types and propagation system but requires `CascadeDefault<A>` to be inserted before propagation runs.
  - `RenderPlugin` setup, not `HeadlessLayoutPlugin`, owns material-handle cascade registration. Headless layout must continue to work without `Assets<StandardMaterial>`.
  - `RenderPlugin` must ensure `Assets<StandardMaterial>` is initialized, create one default material asset from `default_panel_material()`, and insert `CascadeDefault<SdfMaterial>`, `CascadeDefault<TextMaterial>`, and `CascadeDefault<ShapeMaterial>` from that handle before registering material cascade propagation.
  - `DiegeticUiPlugin` gets material defaults through `RenderPlugin`; tests that use render systems without `DefaultPlugins` must add `AssetPlugin`, initialize `StandardMaterial` assets, and seed or install the material cascade defaults explicitly.
  - Do not implement `Default` for `SdfMaterial`, `TextMaterial`, or `ShapeMaterial` with an invalid or empty handle.
  - Headless layout paths that do not render may avoid material cascades, but any code path that resolves material handles must prove the defaults are seeded.
- Migrate material authoring APIs to handles:
  - `DiegeticPanelBuilder::material`, `DiegeticPanelBuilder::text_material`, and new `DiegeticPanelBuilder::shape_material` take `Handle<StandardMaterial>`;
  - `El::material` takes `Handle<StandardMaterial>` for element-local surface material;
  - `TextStyle::with_material` takes `Handle<StandardMaterial>` for text-run local material;
  - `PanelLine::material`, `LineStyle::material`, and `PanelCircle::material` take `Handle<StandardMaterial>` for shape-local material, with `PanelShape` forwarding only if it keeps call sites simpler.
- Make Phase 7 a deliberate breaking API migration from owned `StandardMaterial` storage to `Handle<StandardMaterial>` storage. Replace the current owned-material signatures instead of keeping parallel owned and handle builder APIs:
  - `DiegeticPanelBuilder::material(StandardMaterial)` becomes `DiegeticPanelBuilder::material(Handle<StandardMaterial>)`;
  - `DiegeticPanelBuilder::text_material(StandardMaterial)` becomes `DiegeticPanelBuilder::text_material(Handle<StandardMaterial>)`;
  - `El::material(StandardMaterial)` becomes `El::material(Handle<StandardMaterial>)`;
  - `TextStyle::with_material(Handle<StandardMaterial>)` is added as the text-run local override.
  Do not leave deprecated owned-material builder methods in the main API. If implementation later needs an owned-material convenience, it must be a clearly separate helper with an asset-registration context such as `Assets<StandardMaterial>` or a command/helper that creates the asset once and returns/uses a handle. A pure data builder must not create a new asset every frame or hide asset insertion.
- Update layout change classification for handle-based materials. The current `LayoutTree::classify_change` treats element material add/remove/change as layout-affecting because owned `StandardMaterial` has no tight layout-vs-render comparator. After `El::material` stores `Handle<StandardMaterial>`, element material handle add/remove/change must classify as `VisualOnly`, and identical handles must classify as `Identical`. Add tests that replace the current conservative owned-material test.
- Add `TextStyle::with_material(Handle<StandardMaterial>)` as a render/material field, not a measurement field:
  - include the handle in `PartialEq` so exact style equality is honest;
  - exclude it from `hash_layout(...)` and `layout_eq_excluding_visuals(...)`;
  - exclude it from geometry/atlas gating once text material slots own scalar/vector material values;
  - add tests proving a text material handle change is visual/render-only and does not affect text measurement/layout cache keys.
- Preserve scalar override semantics without making color a special batching path:
  - SDF/background layout colors override `base_color` after resolving `SdfMaterial`;
  - `TextStyle::with_color` overrides `base_color` after resolving `TextMaterial`;
  - `PanelLine::color`, `LineStyle::color`, and `PanelCircle::color` override `base_color` after resolving `ShapeMaterial`.
  These overrides are applied before `MaterialSlotValues` projection, alongside the resolved material's other scalar/vector PBR fields. The table row is still the generic material value payload, not a color-only override.
- Add handle-aware material source resolution helpers, for example `resolve_surface_material_handle`, `resolve_text_material_handle`, and `resolve_panel_shape_material_handle`, plus projection helpers that read the current `Assets<StandardMaterial>` value behind the handle.
- Update the already-production SDF batch producer to use the new handle source model:
  - `ResolvedSdfSurface` and `SdfMaterialSlotInput` read from `SdfMaterial` / element material handles instead of retained owned `StandardMaterial` values;
  - scalar/vector material asset edits refresh current-frame SDF table rows even when layout and geometry are unchanged;
  - handle swaps refresh compatibility and re-key batches when texture, alpha mode, `double_sided`, `cull_mode`, lighting, or other splitters change;
  - missing handles skip the affected current-frame SDF entry or use an explicit seeded fallback, and never reuse stale table values.
- Material asset edits and handle swaps are distinct:
  - editing an existing `StandardMaterial` asset refreshes affected table rows but does not change cascade resolution or stable material slot identity;
  - swapping a handle is a cascade value change and may re-key batches if pipeline/resource compatibility changes;
  - missing or unloaded material handles skip the affected current-frame row or use an explicit fallback value; they must not reuse stale scalar values.
- Existing user-facing builder semantics should remain easy: examples show the normal Bevy pattern of creating a `Handle<StandardMaterial>` from `Assets<StandardMaterial>` and passing that handle to panel/text/shape builders.

**Files:**
- `crates/bevy_diegetic/src/cascade/resolved.rs`, `crates/bevy_diegetic/src/cascade/plugin.rs`, `crates/bevy_diegetic/src/cascade/attributes.rs`, and cascade tests — generalize cascade attributes from `Copy` to `Clone` and add material-handle cascade attributes.
- `crates/bevy_diegetic/src/panel/builder.rs` and `crates/bevy_diegetic/src/panel/diegetic_panel.rs` — migrate panel material fields/builders to `Handle<StandardMaterial>` and add `shape_material`.
- `crates/bevy_diegetic/src/layout/builder.rs`, `crates/bevy_diegetic/src/layout/element.rs`, and `crates/bevy_diegetic/src/layout/text_props.rs` — migrate `El::material`, element material storage, and `TextStyle::with_material` to handles.
- `crates/bevy_diegetic/src/layout/line.rs` and `crates/bevy_diegetic/src/layout/draw.rs` — add shape-local material handles and preserve color override semantics.
- `crates/bevy_diegetic/src/render/material.rs`, `crates/bevy_diegetic/src/render/material_table.rs`, `crates/bevy_diegetic/src/render/fill_batch.rs`, and `crates/bevy_diegetic/src/render/panel_geometry.rs` — add handle-resolution/projection helpers, missing-handle behavior, and SDF producer migration from owned material values to handle-resolved sources.
- Existing examples and tests that construct materials — update to create `Handle<StandardMaterial>` through `Assets<StandardMaterial>`.

**Constraints from prior phases:** Phase 4 made SDF fills/borders production on the frame table, and Phase 6 has the analytic bridge and shared material-table projection paths in place, but text and shape producers still preserve current behavior through bridge adapters. This phase changes the material-source API and cascade model for SDF, text, and shapes before text and shape producers consume material slots directly.

**Acceptance gate:** `cargo build -p bevy_diegetic`, `cargo +nightly fmt --all -- --check`, `cargo clippy -p bevy_diegetic --all-targets`, and `cargo nextest run -p bevy_diegetic` pass. Tests prove existing cascade attributes still resolve the same values after the `Clone` migration; the current `Copy` attributes still work through the default-initialized cascade path; `SdfMaterial`, `TextMaterial`, and `ShapeMaterial` cannot silently use invalid/default handles; `RenderPlugin` seeds default material handles before material cascade propagation; `HeadlessLayoutPlugin` still runs without material assets or material cascades; `Handle<StandardMaterial>` cascade attributes resolve local override, parent/panel override, then default; shared handle edits refresh current-frame material table rows for SDF, text, and shape sources; handle swaps update compatibility and re-key when needed; missing handles do not reuse stale table values; production SDF fill/border rows use `SdfMaterial` / element material handle resolution; panel/text/shape builders accept handles; element material handle changes classify as `VisualOnly`; text material handles are excluded from measurement/layout cache keys; and owned `StandardMaterial` values are not the canonical stored material source for new material ladders.

### Phase 8 — Migrate Text Producers Onto Shared Material Slots · status: todo

#### Work Order

**Goal:** Text analytic-path batches allocate/update shared material slots directly and no longer depend on text-side material-value interning.

**Spec:**
- Update `render/panel_text/batching.rs` so each live text run appends a material row and stores the returned `MaterialSlotId`.
- A text color or scalar PBR edit updates the table row without changing the batch key or moving the run to a different batch when texture/pipeline splitters are unchanged.
- Support individual text material sources as a first-class case: two text runs on the same panel may use different `StandardMaterial` inputs and still share one text batch when only scalar/vector table values differ; a text run with a different alpha mode, texture resource, lighting mode, sidedness, or shadow/prepass policy must split into a separate batch.
- Sample material textures on text runs per the "Texture-backed material sampling" principle. When the resolved text material carries any sampled texture channel (`base_color_texture`, `emissive_texture`, `metallic_roughness_texture`, `normal_map_texture`, `occlusion_texture`, etc.), the shared analytic path shader samples it at the per-glyph run-local box UV (`0..1` across the run's resolved layout box); the base-color sample is multiplied into the table-driven base color before glyph coverage so the run's glyphs stencil the image, while the other channels feed `apply_pbr_lighting` as usual. Add the run-local box UV to the per-glyph instance record alongside the existing atlas UV and fill color, computed at reconcile from each glyph quad's position within the run bounds; it is render-record data, never a material-table value or batch-key field. The shader reads the run's `uv_transform` from its material-table row and composes it with that box UV (`final_uv = uv_transform * box_uv`) before sampling, so tiling/offset/rotation are per-record table data, not batch splitters. Texture presence per channel and any added mesh-attribute requirement (tangents for `normal_map_texture`) remain `ResourceCompatibility` / pipeline splitters, so a texture-backed run forms its own text batch; untextured runs and scalar/vector table values are unaffected.
- Preserve and test the current builder contract:
  - `DiegeticPanelBuilder::text_material(Handle<StandardMaterial>)` remains the panel-level default text material;
  - `TextStyle::with_material(Handle<StandardMaterial>)` adds the text-run local material override above the panel-level text material;
  - `TextStyle::with_color`, `with_alpha_mode`, `with_lighting`, `with_sidedness`, and `with_unlit` keep working as per-run overrides on the resolved text material;
  - `TextStyle::with_color` overrides only `base_color`, preserving scalar PBR fields from the resolved `StandardMaterial`;
  - alpha mode, lighting, sidedness, unlit, texture, and other pipeline/resource splitters still re-key batches when they differ.
- Wire the text-run material rung from Phase 7 into `TextRunMaterialSlotInput`. The investigation found panel-level `text_material` and `TextStyle` scalar/policy overrides, but no implemented text-run material field in `TextStyle`, `ElementContent::Text`, or the public `LayoutBuilder::text*` APIs. Keep this as a text-specific API rather than reusing `El::material`, because current `El::material` is documented and used for element surfaces, backgrounds, borders, and panel-shape material resolution. Material-handle-only changes must classify as visual/render-only wherever the current layout diff system allows it.
- Use `TextRunMaterialSourceKey { run: Entity }` for text refresh and cleanup paths, mirroring `PanelShapeMaterialSourceKey { shape: Entity }`.
- Text producer migration must use `TextRunMaterialSlotInput` from Phase 6 so authored text color becomes the effective `base_color` table value while all other scalar/vector PBR fields from the resolved text material are preserved.
- Add or wire the text material-slot refresh system from Phase 6 so source material changes without text/layout changes still update table rows.
- Revise text dirtiness gates for the material-table model. Current `TextStyle::gating_eq` treats color as a material-baked rebuild input; after this phase, `TextStyle::with_color` and `TextStyle::with_material` are material-row inputs. Update `gating_eq` and its tests so scalar/vector material changes refresh table rows and records without rebuilding glyph/path geometry or path quad buffers. Keep true geometry and atlas inputs such as font, size, weight, slant, spacing, wrap, alignment, anchor, and font features in the geometry gate.
- Hidden panel/run routing, clipped-zero text, missing glyph/self-heal behavior, despawn cleanup, and respawn cases must produce exactly the current frame's live rows and records.
- Preserve dirtiness separation from Phase 6: material-only text edits must not rewrite path/quad buffers when compatibility and geometry are unchanged; placement-only edits must not change material values.
- Keep texture/bind-group requirements, alpha/pipeline mode, lighting mode, sidedness, shadow participation, z-level, and render layers in batch keys. Scalar material values must be table data, not key data.

**Files:**
- `crates/bevy_diegetic/src/render/panel_text/batching.rs` — text run frame-table row construction, dirty flags, and tests.
- `crates/bevy_diegetic/src/layout/text_props.rs`, `crates/bevy_diegetic/src/layout/builder.rs`, and `crates/bevy_diegetic/src/layout/element.rs` — use `TextStyle::with_material(Handle<StandardMaterial>)` from Phase 7 and preserve existing text-style override semantics without treating text material-only changes as layout-affecting.
- `crates/bevy_diegetic/src/render/analytic_paths/batching.rs` — producer-facing slot APIs and any text-specific bridge removal.
- `crates/bevy_diegetic/src/render/material_table.rs` — text material row tests if not complete from Phase 6.
- `crates/bevy_diegetic/src/render/batch_key.rs` — ensure text batch keys exclude scalar material values and retain only true splitters.
- `crates/bevy_diegetic/examples/batch_validation.rs` — update the text validation panel with expected/observed text batch count, text run count, and material slot count. The panel must use the public builders: one run inherits `DiegeticPanelBuilder::text_material`, one run uses a text-style color override on that base material, two runs use `TextStyle::with_material(Handle<StandardMaterial>)` with distinct scalar/vector material sources and therefore share a batch, and another run differs by `AlphaMode` or another splitter and therefore creates a second batch. Add a texture-backed run whose `TextStyle::with_material` source carries real textures (at minimum `base_color_texture`): assert the textures sample across the run (box UV `0..1`) and that the run splits into its own batch (reason "texture resource splits"). This is the row authored as a placeholder in earlier example work; this phase makes it real.

**Constraints from prior phases:** Phase 6 changed analytic shaders and `PathRenderRecord` to read frame-local `MaterialSlotId`, registered `PathExtendedMaterial` with the material-table rebind path, replaced analytic scalar-value batch keys with resource-only keys, and added frame-table row construction. Phase 7 generalized the cascade and made `Handle<StandardMaterial>` the canonical material source.

**Acceptance gate:** `cargo build -p bevy_diegetic`, `cargo +nightly fmt --all -- --check`, `cargo clippy -p bevy_diegetic --all-targets`, and `cargo nextest run -p bevy_diegetic` pass. Tests prove text material-only edits update table rows without batch-key changes or path/quad rewrites; `TextStyle::gating_eq` no longer treats color or material handle changes as path-geometry rebuild inputs after material slots own those values; hidden/despawned/clipped/missing-glyph/self-heal/respawn cases produce exactly the current frame's live text rows and records; the existing recolor behavior currently covered by `fill_color_edit_stays_in_batch_as_a_record_write` survives as a table-row update rather than a path-specific color channel; non-white text colors combine correctly with non-default scalar/vector PBR values from the base text material; scalar/vector-only differences such as emissive, metallic, perceptual roughness, reflectance, transmission, thickness, attenuation, clearcoat, anisotropy, and `ior` also stay in table rows and do not split batches; `DiegeticPanelBuilder::text_material` remains the default base material source; `TextStyle::with_material(Handle<StandardMaterial>)` overrides that base source for one run; text-style color/alpha/lighting/sidedness/unlit builder methods still override the resolved base material as they do today; two text runs with distinct scalar/vector material handles share one batch while retaining distinct rows; a text run whose material differs by `AlphaMode` or another splitter creates a separate batch. Current text examples render unchanged, including non-default scalar/vector PBR values sourced from the frame table. `batch_validation` displays the text validation panel with expected and observed counts.

### Phase 9 — Migrate Panel-Shape Producers Onto Shared Material Slots · status: todo

#### Work Order

**Goal:** Panel-shape analytic-path batches build frame-table material rows directly and scalar/vector-only shape edits no longer rebuild or re-key shape geometry.

**Spec:**
- Add a panel-shape source entity model that mirrors panel text: one `PanelShape` entity represents one authored/resolved shape source, and that source may expand to one or many analytic primitives. A plain line may have one primitive; a double-headed arrow may have a shaft plus two cap primitives but still has one `PanelShape` entity.
- Materialize panel-shape source entities from the current resolved shape identity, not from primitives. The migration bridge must map each current `ResolvedPanelShape::source_key` / command source to one stable `PanelShape` entity under the owning panel and reuse that entity across frames while the authored source remains present.
- Add Bevy relationships for shape ownership, analogous to `TextRunOf` / `PanelTextRuns`: `PanelShapeOf` points from a shape entity to its owning panel, and `PanelShapes` is the panel-side relationship target. If implementation proves element-level ownership is required for cleanup or authoring, add an element/source owner component on the shape entity rather than making individual primitives into entities.
- Define `PanelShapeMaterialSourceKey { shape: Entity }` as the panel-shape analog to `TextRunMaterialSourceKey { run: Entity }`. This key identifies the authored material source before frame-table row allocation; it is not a `MaterialSlotId` and must not use slot terminology.
- Keep material source identity separate from render/atlas identity. `PanelShapeMaterialSourceKey { shape }` selects material values for the shape source. Path packing, atlas membership, cleanup, and transform updates still need a render identity for each merged path/primitive group, derived from the shape entity plus a stable group/primitive ordinal or the existing `PanelShapePrimitiveKey`. Do not reuse the material-source key as the atlas key.
- Keep panel-shape primitives as render data under the source entity, not Bevy entities. Primitive ordinals remain local render/packing data used to build analytic path geometry and records.
- Update `render/panel_shapes/batching.rs` so each current-frame panel-shape render record appends a material row and stores the frame-local `MaterialSlotId`.
- Panel-shape producer migration must replace the Phase 6 temporary bridge key with `PanelShapeMaterialSourceKey { shape }` and keep `PanelShapeMaterialSlotInput` as the append-time input so authored shape color overrides the base material color while scalar/vector PBR fields are preserved.
- Add a shape material source ladder before relying on frame-table rows:
  - shape-local material handle override, exposed through builders such as `PanelLine::material`, `LineStyle::material`, and `PanelCircle::material`, plus any enum forwarding needed on `PanelShape`;
  - panel-level shape material default, exposed as `DiegeticPanelBuilder::shape_material(Handle<StandardMaterial>)` and stored on `DiegeticPanel` separately from the existing surface `material`;
  - global/default shape material behavior, using `default_panel_material()` for the first implementation unless a later implementation proves a separate default resource is simpler.
- Resolve shape materials with the same cascade semantics as text materials: shape-local material override, else panel shape material, else global/default material.
- Preserve current shape color builder semantics on top of the resolved base material: `PanelLine::color`, `LineStyle::color`, and `PanelCircle::color` override only `base_color`, while scalar/vector PBR fields come from the resolved `StandardMaterial`.
- Support individual panel-shape material sources as a first-class case: two `PanelShape` entities may use different `StandardMaterial` inputs and still share one panel-shape batch when only scalar/vector table values differ; a shape with a different alpha mode, texture resource, lighting mode, sidedness, or shadow/prepass policy must split into a separate batch.
- Sample material textures on panel shapes per the "Texture-backed material sampling" principle. When the resolved shape material carries any sampled texture channel, the shared analytic path shader samples it at the shape-local box UV (`0..1` across the shape silhouette's resolved bounds); the base-color sample is multiplied into the table-driven base color before stroke coverage so the stroke stencils the image, while other channels feed `apply_pbr_lighting`. Carry the shape-local box UV in `PathRenderRecord` as render data, never a material-table value or batch-key field. The shader reads the shape's `uv_transform` from its material-table row and composes it with that box UV (`final_uv = uv_transform * box_uv`) before sampling, matching text. This reuses the same shared-shader sampling added for text in Phase 8. Texture presence per channel and any added mesh-attribute requirement remain `ResourceCompatibility` / pipeline splitters, so a texture-backed shape forms its own batch.
- Do not let grouping by path geometry imply material equality. A merged analytic path may contain multiple compatible primitives, but one `PathRenderRecord` can read only one material slot. The first implementation should keep one material source per merged render record; if a later public API allows intentionally different materials inside one authored shape, add a reviewed material-source ordinal at that time.
- Rename or replace `LineMergeKey` with `PanelShapeMergeKey` unless an implementation-local old name remains only inside temporary bridge code. Current code includes `LineMergeKey::color` because one merged path record currently owns one color; after material slots, color and other scalar/vector material values move to `MaterialSlotValues` and must leave the merge key. The merge key must preserve the current ability to merge compatible cross-line/guide primitives where tests require one merged path render entry. Do not make `shape: Entity` an unconditional merge-key field if that would split currently mergeable silhouettes. The planned key contains non-material silhouette/order fields such as `clip`, owner bounds, layering, and a stable merge-group identity; include material-source identity only when it is required because one resulting `PathRenderRecord` can reference only one `MaterialSlotId`. Scalar/vector-only shape edits update the table row without changing the merge key, batch key, or atlas unless the silhouette changes.
- Add a per-record diff/update path that compares outline/silhouette separately from material slot updates. Whole-panel `remove_panel` + `upsert_panel` may remain only for topology/shape changes; it must not be the only path for material-only edits.
- Add or wire the panel-shape material-slot refresh system from Phase 6 so source material changes without shape geometry changes still update table rows.
- Preserve panel-shape lifecycle behavior: relationship cleanup, hidden routing, removed shape cleanup, atlas rebuild only for silhouette/path changes, and current-frame row construction for despawn/respawn.
- Preserve dirtiness separation from Phase 6: material-only shape edits must not rewrite path/instance buffers or rebuild the atlas when the slot id and silhouette are unchanged.
- Keep texture/bind-group requirements, alpha/pipeline mode, lighting mode, sidedness, shadow participation, z-level, and render layers in batch keys. Scalar material values must be table data, not key data.

**Files:**
- `crates/bevy_diegetic/src/render/panel_shapes/relationship.rs` (new) — `PanelShape`, `PanelShapeOf`, and `PanelShapes`, mirroring the text relationship pattern while keeping primitives as render data.
- `crates/bevy_diegetic/src/render/panel_shapes/batching.rs` — panel-shape frame-table row construction, `PanelShapeMaterialSourceKey`, `PanelShapeMergeKey` update that preserves existing compatible primitive merging, per-record material-vs-silhouette diff path, dirty flags, atlas rebuild guards, relationship cleanup, and tests.
- `crates/bevy_diegetic/src/layout/line.rs`, `crates/bevy_diegetic/src/layout/draw.rs`, `crates/bevy_diegetic/src/panel/builder.rs`, and `crates/bevy_diegetic/src/panel/diegetic_panel.rs` — use `PanelLine::material`, `LineStyle::material`, `PanelCircle::material`, any needed `PanelShape` forwarding, and `DiegeticPanelBuilder::shape_material(Handle<StandardMaterial>)` from Phase 7, preserving existing per-shape color builders as base-color overrides.
- `crates/bevy_diegetic/src/render/analytic_paths/batching.rs` — producer-facing slot APIs and any shape-specific bridge removal.
- `crates/bevy_diegetic/src/render/material_table.rs` — panel-shape material row tests if not complete from Phase 6.
- `crates/bevy_diegetic/src/render/batch_key.rs` — ensure panel-shape batch keys exclude scalar material values and retain only true splitters.
- `crates/bevy_diegetic/examples/batch_validation.rs` — update the panel-shape validation panel with expected/observed shape batch count, path render count, and material slot count. The panel must include the public shape builders for global/default material behavior, panel-level shape material default, shape-local material override, scalar/vector material variation that batches, and at least one material splitter that creates another batch. Include a texture-backed shape whose source material carries real textures (at minimum `base_color_texture`): assert the textures sample across the shape (box UV `0..1`) and that it splits into its own batch (reason "texture resource splits").

**Constraints from prior phases:** Phase 6 changed analytic shaders and `PathRenderRecord` to read frame-local `MaterialSlotId`, registered `PathExtendedMaterial` with material-table rebind, and added frame-table row construction. Phase 7 generalized the cascade and made `Handle<StandardMaterial>` the canonical material source. Phase 8 proves text producer migration on analytic paths.

**Acceptance gate:** `cargo build -p bevy_diegetic`, `cargo +nightly fmt --all -- --check`, `cargo clippy -p bevy_diegetic --all-targets`, and `cargo nextest run -p bevy_diegetic` pass. Tests prove shape source entities are reused across frames for unchanged `ResolvedPanelShape` sources; material-only shape edits do not despawn/recreate `PanelShape` entities; material source identity is not used as the atlas/render key; same-silhouette/different-material shape primitives do not collapse into one material row; same paint source color edits stay grouped; existing merged ruler/guide parts still share one path render entry where current tests require merging; `LineMergeKey::color` or its equivalent is removed from active grouping after material slots own scalar/vector values; panel-shape scalar/vector edits update table rows without batch-key changes, atlas rebuilds, or path-quad rewrites when silhouette and compatibility are unchanged; non-white shape colors combine correctly with non-default scalar/vector PBR values from the base shape material; global/default, panel-level, and shape-local material sources all route through `PanelShapeMaterialSlotInput`; scalar/vector distinct `StandardMaterial` sources batch while retaining distinct rows; `AlphaMode` or another splitter creates a separate shape batch; hidden/despawned/removed/respawned line, circle, and cap primitive cases produce exactly the current frame's live rows and records. Current panel-shape examples render unchanged. `batch_validation` displays the panel-shape validation panel with expected and observed counts.

### Phase 10 — Remove Analytic-Path Material Interners And Tighten Stats · status: todo

#### Work Order

**Goal:** The frame material table is the only material-value storage path for SDF fills, text, and panel shapes, and the old analytic material interners/bridge code are gone.

**Spec:**
- Remove any migration-only frame-table helper code made unnecessary by direct text and panel-shape producer ownership.
- Delete analytic-path `VisualMaterialInterner` production and probe use.
- Delete `VisualBatchKey` production and probe use. Any remaining compatibility key must be `PathBatchKey` or another scalar-free key with direct `PipelineCompatibility` and `ResourceCompatibility` fields.
- Delete scalar-value `BaseMaterialId` batching. If a `BaseMaterialId` type remains for an unrelated non-table concept, document that role in `batch_key.rs` and prove it does not encode scalar material identity.
- Ensure stats report the useful production values:
  - material-table current-frame rows, upload bytes, and capacity;
  - SDF batches/records/uploads;
  - text batches/runs/uploads;
  - panel-shape batches/path-renders/uploads.
- Finalize `batch_validation` as the mixed-material validation example for the whole plan. It must show multiple panels with SDF fills/borders, text runs, and panel-shape primitives. Each panel must display expected and observed batch counts, record/run counts, material slot counts, and the reason for each expected split or merge. At minimum:
  - one text panel uses `DiegeticPanelBuilder::text_material` as the default text material and proves `TextStyle::with_color` overrides only the glyph color while preserving scalar PBR values from that material;
  - one text panel has two text runs using `TextStyle::with_material(Handle<StandardMaterial>)` with separate material sources that differ only by scalar/vector values such as base color, emissive, or roughness and therefore share one text batch;
  - the same text panel has another text run with a different `AlphaMode` or another pipeline/resource splitter and therefore creates a separate text batch;
  - equivalent scalar-share and splitter-split panels exist for SDF roles and panel-shape primitives.
- Update tests and examples that used old interner counters, old run-color assumptions, or old path-material adapter assumptions.
- Run `rg` checks to prove there is no remaining accidental scalar-material batching path:
  - no old analytic `VisualMaterialInterner` producer or probe use;
  - no old `VisualBatchKey` producer or probe use;
  - no scalar-value `BaseMaterialId` batching;
  - no `RunRecord::fill_color`;
  - no old `BatchPathMaterialInput` / `batch_path_material` production or probe adapter unless the hit is in a deliberate removal test;
  - no production `RunRecord`, `PathInstanceRecord`, `PathRecord`, `atlas_index`, or `run_index` names left in analytic-path GPU record APIs after the Phase 6 rename, except in migration notes/tests explicitly checking removal;
  - no SDF-local material interner;
  - no fill/text/panel-shape batch key field that stores scalar PBR values.

**Files:**
- `crates/bevy_diegetic/src/render/batch_key.rs` — delete `VisualMaterialInterner` and `VisualBatchKey`; delete or narrow `BaseMaterialId` to a documented non-scalar-material role.
- `crates/bevy_diegetic/src/render/analytic_paths/batching.rs` — remove migration/interner compatibility code.
- `crates/bevy_diegetic/src/render/analytic_line_probe.rs` — remove old probe-side run-color/material construction or prove the probe uses the frame table.
- `crates/bevy_diegetic/src/render/panel_text/batching.rs` — final text cleanup and tests.
- `crates/bevy_diegetic/src/render/panel_shapes/batching.rs` — final panel-shape cleanup and tests.
- `crates/bevy_diegetic/src/render/material_table.rs` — final stats and assertions.
- `crates/bevy_diegetic/src/panel/perf.rs` and `crates/bevy_diegetic/src/panel/constants.rs` — updated production counters/docs.
- `crates/bevy_diegetic/examples/diegetic_text_stress.rs`, `crates/bevy_diegetic/examples/batch_validation.rs`, and any text/panel-shape examples with material stats — update display/log expectations.

**Constraints from prior phases:** Phase 4 removed the old panel-chrome SDF quad route. Phase 8 migrated text producers to frame-table material rows. Phase 9 migrated panel-shape producers to frame-table material rows. No remaining production path should require value-keyed material interning for scalar/vector material fields.

**Acceptance gate:** `cargo build -p bevy_diegetic`, `cargo build --workspace --all-features --examples`, `cargo +nightly fmt --all -- --check`, `cargo clippy -p bevy_diegetic --all-targets`, and `cargo nextest run -p bevy_diegetic` pass. `rg -n "RunRecord|PathInstanceRecord|PathRecord|atlas_index|run_index|fill_color: Vec4|BatchPathMaterialInput|batch_path_material|VisualMaterialInterner|VisualBatchKey|SDF-local material interner" crates/bevy_diegetic/src` has no production-path or probe-side hits except deliberate references to `PackedPathRecord`, `PathQuadRecord`, `PathRenderRecord`, `packed_path_index`, `render_index`, or explicit removal tests. Text, panel-shape, and SDF examples show stable batch counts under scalar/vector material animation and accurate material-table stats. `batch_validation` visibly proves scalar/vector `StandardMaterial` variation batches for SDF, text, and panel shapes; pipeline/resource splitter variation creates separate batches; and the displayed expected/observed per-panel counts match automated assertions.

## Appendix A — Durable Slot Table Alternative Not In The Active Plan

This appendix records the more complex alternative so it does not leak into the implementation phases by accident. The current plan does not implement it.

The alternative is a persistent `SharedMaterialTable` with durable `MaterialSlotId` rows, `MaterialSlotKey -> MaterialSlotId` mappings, owner membership, tombstone rows, a two-frame retirement queue, and an epoch-tagged assignment snapshot such as `SharedMaterialAssignments`. That model exists to protect stale GPU records when physical rows can be reused while older extracted records may still reference the table.

Do not implement this alternative unless Phase 2 measurement shows the frame-built dense table is too expensive or cannot satisfy a concrete correctness constraint. If that happens, write a new plan amendment before coding it. The amendment must state the measured failure, the exact threshold that failed, and why the durable allocator is less complex overall than the frame-built table for that measured case.

Names reserved for this appendix-only alternative:

- `SharedMaterialTable`
- `SharedMaterialAssignments`
- `SharedMaterialTableEpoch`
- `MaterialSlotAllocator`
- two-frame delayed reuse / retirement queue
- tombstone rows
- durable owner membership maps

## Appendix B — Follow-On Feature: Reverse Text

Not part of the active plan. Recorded here so the design is settled before anyone implements it.

`Sidedness` now exposes three cull states: `BothSides` (cull none), `FrontOnly` (cull back), and `BackOnly` (cull front). `BackOnly` draws a glyph run that is visible only from behind the panel — on a transparent diegetic panel that is a back-face label viewable through the glass. The glyph geometry is unchanged, so a `BackOnly` run reads **mirror-reversed** when viewed from behind, exactly like writing on the back of a window.

Reverse Text is the separate feature that flips glyph handedness so a back-facing run reads correctly. It is orthogonal to `Sidedness`: cull state decides *which face is visible*, reverse decides *which way round the letters are*. The two compose — a correct-reading label on both faces of a transparent panel is authored as two runs, a normal `FrontOnly` run plus a reversed `BackOnly` run over the same box.

Design constraints when it is implemented:

- Flip in glyph UV / atlas-sample space (negate the per-glyph horizontal texture coordinate), not by a geometry negative-scale. A negative scale flips triangle winding, which collides with `cull_mode` and would make `BackOnly` cull the wrong face. UV-flip keeps reverse and cull independent.
- Reverse is per-glyph render data, not pipeline or resource state, so it rides in the per-glyph instance record alongside the box UV and does **not** split a batch. A reversed run and a normal run that are otherwise compatible still batch together.
- Mirror the run's box UV horizontally as well so any texture-backed base color (the "Texture-backed material sampling" principle) stays aligned with the flipped glyphs.
