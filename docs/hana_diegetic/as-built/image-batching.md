# Image Batching

## What it is

Diegetic `image` and precompose-LDR leaves render through a batched image family (`ImageBatchStore` → one batch entity per GPU-compatible key) instead of one child entity per draw command. Batch-store bookkeeping for all four render families (SDF surfaces, images, text glyph runs, panel shapes) is unified on one generic member-routing container, `BatchStore<K, B>`. The guiding model: a batch *member* is one element's draw contribution; a *batch* groups members by a GPU-compatibility key; a *store* is the member↔batch index. This removes the per-command entity churn of the old image path (`PanelImageChild` spawn/despawn/reconcile) and collapses four hand-rolled copies of the routing dance into a single generic.

## How it works

### Trait stack (`crates/hana_diegetic/src/render/batch_store.rs`)

- `BatchEntry` — `is_empty()`, `entity() -> Option<Entity>`. Implemented by all four batch types; the free fn `take_empty_batches<K, B: BatchEntry>(&mut HashMap<K, B>) -> Vec<Entity>` is the one shared drop-empty helper.
- `Batch: BatchEntry + Default` — `type MemberKey: Copy + Eq + Hash`, `type Payload`, and `insert`/`update`/`remove(member, payload)`. The store never inspects `Payload`; the concrete batch owns GPU records, dirty flags, transform carry-over, and equality.
- `BatchStore<K, B>` — two maps: `batches: HashMap<K, B>` and `member_index: HashMap<B::MemberKey, K>`. `upsert(key, member, payload)` handles same-key update / changed-key move / absent insert; plus `remove`, `retain(&HashSet<MemberKey>)`, `contains`, `key_for`, `member_batch_mut` (the substrate for future per-member update channels), `get`/`get_mut`, `batches`/`batches_mut`, `take_empty_batches`.
- `MemberRecord` — `panel()`, `transform() -> Mat4`, `update_world_transform(&GlobalTransform)`. For records whose world transform is panel-relative (SDF, Image).
- `MemberBatch: Batch` — `records_mut`, `record_upload_mut`, `bounds_update`/`bounds_update_mut`, `world_bounds`. Exposes the per-batch state the two shared post-propagation bodies need but `Batch` keeps payload-opaque. (`batch_entity` was collapsed into the `BatchEntry::entity()` supertrait method.)
- `MemberFamily: 'static` — associated `Key`/`Batch`/`Store: Resource<Mutability = Mutable>`/`Marker: Component` plus `store_mut(&mut Store) -> &mut BatchStore<Key, Batch>`. Only `SdfMemberFamily` and `ImageMemberFamily` implement it.
- Two generic system bodies parameterized by `MemberFamily`: `update_batch_world_transforms::<F>` (post-`Propagate`; per record `update_world_transform`, marks BOTH `record_upload` and `bounds_update` when any composed matrix changed) and `update_batch_bounds::<F>` (writes the batch entity's `Transform`/`GlobalTransform` = world-bounds center and a center-zero `Aabb`).

### Four family instantiations

| Family | Store | Key | Batch | MemberKey |
|---|---|---|---|---|
| SDF | `SdfBatchStore(BatchStore<SdfBatchKey, SdfBatch>)` `fill_batch.rs:742` | `SdfBatchKey` | `SdfBatch` | `SdfRecordKey` |
| Image | `ImageBatchStore(BatchStore<ImageBatchKey, ImageBatch>)` `image_batch.rs:395` | `ImageBatchKey` | `ImageBatch` | `ImageRecordKey` |
| Text | `TextRunBatchStore(BatchStore<PathBatchKey, TextRunBatch>)` `analytic_paths/batching.rs:549` | `PathBatchKey` | `TextRunBatch` | `RunStorageKey` |
| Shape | `ShapeBatchStore { store: BatchStore<PathBatchKey, ShapeBatch>, panel_members, atlas, atlas_dirty }` `panel_shapes/batching.rs:371` | `PathBatchKey` | `ShapeBatch` | `PanelShapeRenderKey` |

SDF and Image are `Resource` newtypes and implement `MemberFamily`. Text's store is a field of the `GlyphCache` resource (reached via `batch_store()`/`batch_store_mut()`), not a `Resource`; its post-`Propagate` transform system `write_batch_run_transforms` stays concrete. `ShapeBatchStore` wraps `BatchStore` behind a per-panel delivery API (`upsert_panel`/`try_refresh_panel`/`remove_panel`) because shapes are re-derived wholesale from each panel's command stream; it applies the panel transform at build time and has no post-`Propagate` system. Technique-layer types shared by text and shape keep `Path*` names (`PathBatchKey`, `PathQuadRecord`, `PathRenderRecord`, `PathAtlas`, `PathExtendedMaterial`).

### Router

`RenderCommandKind::draw_batch_family() -> Option<DrawBatchFamily>` (`layout/render.rs:140`) maps command kinds to families: `Rectangle`/`Border` → `SdfSurface`, `PanelShapes` → `PanelShape`, `Text` → `Text`, `Image`/`PrecomposeLdr` → `Image`. This is the shared gate: each family's route pass emits records only for commands whose `draw_batch_family()` matches (e.g. shape gate at `panel_shapes/batching.rs:865`).

### Image path

`route_image_batch_records` (`image_batch.rs:498`, `PostUpdate`, `.after(precompose::cleanup_retired_precompose_images).before(TransformSystems::Propagate).before(BatchResourcesReady)`) does a full per-frame rebuild: for each visible panel it reads effective `RenderLayers` (default `layer(0)`) and `Resolved<ShadowCasting>` (default `On`), then `collect_panel_image_records` walks `computed.result().commands`:

- `image_record_source`: `Image { handle, tint }` → `(handle, linear_tint(tint))`; `PrecomposeLdr` → `precompose_cache.entry(command.element_idx)` (skip if absent — never synthesize a default handle) → `(entry.image, linear_tint(WHITE))`.
- Skips: `clip::effective_clip(...)?` (empty-clip cull; partial clipping unsupported) and `computed.draw_order().depth_for(command_index)?` (no depth → drop).
- Builds `ImageBatchKey { texture, layers: BatchRenderLayers, shadow: VisualShadow, z_index: DrawZIndex, z_index_rank: DrawZIndexRank }` and `ResolvedImageRecord::new(record_key {panel, command_index}, local_transform, size, tint: Vec4, ImageUvRect::default(), draw_depth)`. Note the two distinct indices: precompose lookup keys on `command.element_idx`; the record key uses the `enumerate()` index.
- Coordinate conversion (`local_transform_from_bounds`/`image_size_from_bounds`): layout points → world units via `panel.points_to_world()` (NOT baked into the panel `GlobalTransform`), anchored and Y-flipped, with `TEXT_Z_OFFSET` in local Z. `size` is the FULL world-unit extent.
- `store.upsert_record(batch_key, record)` then `store.retain_records(&active)`.

`update_batch_world_transforms::<ImageMemberFamily>` (post-`Propagate`) sets `record.transform = panel_global.to_matrix() * local_transform.to_matrix()` (world-absolute per record — required because one texture used by two panels is a single cross-panel batch). `reconcile_image_batch_entities` spawns/despawns one batch entity per key (`DiegeticImageBatch`, `Mesh3d`, `MeshMaterial3d(ImageExtendedMaterial)`, `Visibility::Inherited`, `NoAutoAabb`, `Aabb::default()`, `RenderLayers`, `NotShadowCaster` when `shadow == None`); `update_batch_bounds::<ImageMemberFamily>` writes the manual AABB; `commit_image_batch_buffers` uploads dirty record buffers and fills `DiegeticPerfStats::image_breakdown`.

### GPU / material

`ResolvedImageRecord` (CPU) carries `record_key`, `local_transform`, `size`, `tint: Vec4` (linear), `uv_rect`, `draw_depth: DrawCommandDepth`, `transform: Mat4`. `ImageRenderRecord: ShaderType` (GPU, `SHADER_SIZE == 128`, const-asserted) carries `transform, size, uv_rect: Vec4, tint: Vec4, clip_depth_nudge: f32, oit_depth_offset: f32`. `clip_depth_nudge` is uploaded relative to the batch `first_draw_order_index`; `oit_depth_offset` stays panel-absolute. `ImageBatchResources { records: Handle<ShaderBuffer>, mesh: Handle<Mesh>, material: Handle<ImageExtendedMaterial>, capacity }`.

`ImageExtendedMaterial = ExtendedMaterial<StandardMaterial, ImageExtension>` (`image_material.rs`): the `StandardMaterial` half owns `base_color_texture = key.texture`, `unlit: true`, `double_sided: true`, `cull_mode: None`, `alpha_mode: Blend`, `depth_bias = key.depth_bias()`; `ImageExtension` binds only `#[storage(107, read_only, visibility(vertex, fragment))] records`. `set_image_material_records` re-points binding 107 after growth.

Shader `crates/hana_diegetic/src/shaders/image_panel.wgsl` (embedded as `IMAGE_PANEL_SHADER_PATH = "embedded://hana_diegetic/shaders/image_panel.wgsl"`, registered in `lib.rs`): vertex-pull over an inert `capacity*4`-vertex / `capacity*6`-index mesh; record index = `(vertex_index - mesh[instance_index].first_vertex_index) / 4u`; builds an origin quad `local = signs * record.size * 0.5` and outputs `position_world_to_clip(record.transform * vec4(local, 0, 1))` — it never composes the mesh model matrix. Non-OIT branch adds `clip.z += clip_depth_nudge * CLIP_DEPTH_NUDGE_PER_LAYER * clip.w`; OIT branch offsets `oit_pos.z += record.oit_depth_offset`. Record index reaches the fragment via the UV_1 (`uv_b`) interpolant.

### Images sit outside the material table

SDF/text/shape bind a shared `material_table` and go through `register_*_batch_materials` + rebind each frame; the image family binds none of that and skips both. There is no cascade `StandardMaterial` handle carrying tint — tint is stored in the record (`ResolvedImageRecord.tint`, linear `Vec4`) and is multiplied in-shader after the hardware sRGB texture decode, never via `StandardMaterial::base_color`.

### Depth

Per-band ordering comes from `ScreenDepthBias` derived from `DrawZIndexRank` (`key.depth_bias() = z_index_rank.screen_depth_bias().get()`, fed to `StandardMaterial::depth_bias`). Per-record coplanar ordering comes from `ClipDepthNudge` (non-OIT clip-z nudge) and `OitDepthOffset` (OIT position-z offset), both from `DrawCommandDepth`. `ScreenDepthBias`/`ClipDepthNudge`/`OitDepthOffset` are `f32`, `PartialEq` only — never in a batch key. `DrawZIndex` and `DrawZIndexRank` are `Eq + Hash` and are the hashable key fragments.

## Invariants

- Batch keys must be `Eq + Hash`. Key on `DrawZIndex` + `DrawZIndexRank`, never the `f32` `ScreenDepthBias`. Derive `depth_bias` from `z_index_rank.screen_depth_bias().get()`.
- `DrawOrderIndex` stays per-record, never in a batch key; it drives `ClipDepthNudge`/`OitDepthOffset` and the intra-batch `sort_records` order.
- `ImageBatchKey` omits `contiguous_drawn_run` and alpha mode: images are always `AlphaMode::Blend`. Any change adding an authored image alpha mode must revisit this.
- Records are sorted by draw order before upload (`sort_records`, tiebreak on `command_index`) + `refresh_first_draw_order_index`. OIT is opt-in, so intra-batch composite order depends on this sort.
- Records are uploaded as a fixed-capacity padded payload (`set_data` byte length constant per capacity); on growth allocate a NEW `ShaderBuffer` + inert mesh and re-point the material's binding 107; capacity is `record_count().max(1).next_power_of_two()` (never zero).
- Per-record world transform is written after `TransformSystems::Propagate` and marks BOTH `record_upload` and `bounds_update` when it changes. The router carries the stored `transform` onto the rebuilt record before the equality check so a static image re-upserts clean (no per-frame dirty).
- The image vertex shader treats `record.transform` as the full world transform and must NOT compose the batch entity's model matrix (that entity's `GlobalTransform` is the world-bounds center, for culling only). `size` is the full extent — halve it for corner offsets.
- `ImageExtension` always carries a `#[storage]` entry, so `MATERIAL_BIND_GROUP_INDEX` is never empty and Bevy never strips it — structural, no runtime guard. Do NOT add a stripped-material-group guard or override `enable_prepass()` (those exist for SDF/text because they can be Opaque/Mask and hit the opaque depth-only prepass; images never do).
- Preserve the precompose absent-entry skip (no synthesized `Handle::<Image>::default()`) and the empty-clip cull.
- The `b.image(el, handle, tint)` authoring API is stable; `uv_rect` is forward-compat only (no atlas/bindless yet).
- Precompose offscreen target stays `TextureFormat::Bgra8UnormSrgb`, sampled via `base_color_texture`.
- Batch internals (GPU buffers, growth guard, `sort_records`, dirty flags, family-specific alpha remaps, per-family perf-breakdown push) are per-family and NOT moved into `BatchStore`; only member-routing bookkeeping is generic.
- Per-family alpha remaps stay distinct: SDF `(Opaque, Cast) -> Mask(0.0)` shadow-gated (`sdf_batch_alpha_mode`); text `Opaque -> Mask(0.0)` unconditional (loses its material group in the camera depth/normal prepass); images need neither.
- SDF's clipped-border split + `Blend` reroute (`push_resolved_sdf_surfaces`/`should_split_clipped_border`/`fill_only`/`border_only`/`clip_rect_limits_mesh`/`FillMaterialOverride` in `panel_geometry.rs`; `sdf_record_pipeline_compatibility`/`clipped_border_uses_transparent_phase` at both call sites in `fill_batch.rs`) is upstream of the store and must survive. An SDF element can own one OR two member keys; member-key-granular `retain` handles it. The clipped-border coercion is a distinct alpha stage ordered BEFORE the shadow remap.

## Calibration / gotchas

- `ImageRenderRecord::SHADER_SIZE == 128`, const-asserted — changing the GPU record layout trips this.
- Shader magic numbers: `CLIP_DEPTH_NUDGE_PER_LAYER = 0.0000002`, `IMAGE_ALPHA_DISCARD = 0.001`, `OIT_MIN_DEPTH = 0.000003`, `INVALID_RECORD_INDEX = 0xFFFFFFFF`.
- `OPAQUE_FILL_DEPTH_PUSH_LAYERS = 1.0` (`fill_batch.rs:95`): opaque SDF fill/border is pushed away from the camera by this, which is exactly why a coplanar opaque clipping border landed BEHIND a `Blend` image. The fix routes the clipping-border-only record to `Blend` so it orders by `oit_depth_offset` at equal world depth; a *filled* clipped card splits into an `Opaque` fill-only record (behind the image) and a `Blend` border-only record (in front). The reroute only fires when fill is `NotAuthored`, border is `Authored`, `clip_rect_limits_mesh()` is true, and the record actually takes the opaque push (`opaque_fill_depth_push(...) > 0.0`); an unclipped border's default clip rect equals mesh bounds, so it never misfires.
- **Shadow via prepass discard, not an alpha helper.** Image shadow alpha comes from the camera/shadow prepass fragment sampling texture alpha and `discard`ing (`IMAGE_ALPHA_DISCARD`), matching `sdf_panel.wgsl`'s `fill_alpha_for_prepass`. Because the material is always `Blend` (MAY_DISCARD), its bind group survives on the shadow pipeline and `@binding(107)` is present everywhere the image material compiles.
- **Precompose color space:** offscreen target stays `Bgra8UnormSrgb`, deliberately unchanged; images sample it via `base_color_texture` with tint applied post-decode.
- **`embedded_asset!` path normalization trap (naga/Bevy):** register the shader from a file whose directory is an ancestor of (or equal to) the shader's — the crate root `lib.rs` for `src/shaders/*`. Registering from `src/render/` with a `../shaders/...` path produces an un-normalized `render/../shaders/...` asset path that a clean `embedded://hana_diegetic/shaders/...` load never matches (Bevy's `MemoryAssetReader`/`Dir` keep the literal `..`), so the shader silently fails to resolve and images draw nothing. Invisible to `cargo nextest` under `MinimalPlugins` — code review / on-screen launch is the only gate.
- `specialize` is a no-op `Ok(())`; entry-point names (`vertex`/`fragment`) match Bevy defaults and the `MaterialExtension` `ShaderRef` hooks override the stages. `deferred_*` hooks also return the image path — harmless, a `Blend` material never enters the deferred pass.
- **Schedule-set overlap:** the image router is anchored to the precompose cache systems it reads (`.after(cleanup_retired_precompose_images)`), NOT the whole `PanelChildSystems::Build` set — the panel-shape batch systems are members of both `Build` and `BatchResourcesReady`, so a `Build → BatchResourcesReady` edge triggers `SetsHaveOrderButIntersect` at schedule init. This class of bug is invisible to the per-plugin test schedules; only launching the full app catches it.
- No image example ships under `examples/`; on-screen confirmation piggybacks on `batch_validation` (bottom-right panel draws four tinted cards from one texture → one batch of four records; the info panel has a 4th "image" family column). Precompose LDR draws legitimately count as image batches (the text panel's precompose adds two single-record batches, so the harness shows 3 image batches / 6 records, not 1/4 — the routing invariant is over batches).
- The inert image mesh winding is byte-identical to `inert_sdf_batch_mesh`, so SDF's corner→sign and `box_uv` derivation port unchanged; it carries `ATTRIBUTE_UV_0` (texture sampling) and `ATTRIBUTE_UV_1` (record index varying).

## Why

- **Images bypass the material table** because the table exists to share/rebind a `StandardMaterial` slot across compatible records; images carry no per-record material state except a tint that the shader applies directly, and one texture per key is already the batch identity. Binding the table would add a register/rebind cost and an empty-group strip hazard for zero benefit.
- **Tint stays in the record** (linear `Vec4`, post-decode multiply) rather than `base_color` so many differently-tinted images sharing a texture stay in ONE batch — putting tint in the material would split the batch per tint.
- **The generic `BatchStore` was extracted mid-project, after the image family was already live and after the SDF clipped-border split landed**, not at the end: the image store was first copy-adapted from the SDF store, which proved the routing dance was byte-identical across families; extracting then let SDF, image, text, and shape all migrate onto one container while the divergences (alpha remaps, perf push, transform timing) were already understood and could be recorded as per-family-by-construction rather than forced into a false shared hook.
- **A member = one element's draw contribution** (not one element) so a clipped SDF element that splits into fill + border owns two member keys, and a cross-panel texture forms one batch holding records from several panels — the routing and `retain` are member-granular, which is what makes both the split and cross-panel merge natural.
- **Shape keeps a per-panel wrapper** (`upsert_panel`) because shapes are re-derived wholesale from each panel's resolved command stream — there is no retained per-shape update channel yet. Routing its members through `BatchStore` behind that wrapper gets the generic bookkeeping (and per-member moves) without inventing a retained authoring path; the substrate for one (`member_batch_mut`) is present, and the follow-on design is stubbed at `docs/hana_diegetic/retained-shapes.md`.
- **Only SDF and Image implement `MemberFamily`** because they are the two families with a `Res<Store>` reached by a post-`Propagate` per-record transform system with identical bodies. Text runs the same pattern but reaches its store through `GlyphCache` (different access path); shape applies its transform at build time. Forcing all four through one trait would abstract over access paths that genuinely differ.
