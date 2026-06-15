# Element batching

> **Status: IMPLEMENTATION PLAN — phased, delegate-ready.** Convert per-fill SDF
> draws into a batched vertex-pulled path backed by one shared GPU material table
> (`material-as-data`: per-element color and every numeric PBR factor live in a
> table of `StandardMaterialUniform` entries, indexed by a per-record
> `material_index`; the batch key carries no material values). Fills are the first
> adopter; text and lines migrate onto the same table last.

## Delegation Context
<!-- Shared across all phases. /plan:delegate prepends this to every dispatch. -->

- **Project:** `bevy_diegetic` — in-world diegetic UI panel renderer for Bevy with SDF geometry and analytic (Slug) text rendering.
- **Stack:** Rust 2024 edition + Bevy 0.19.0-rc.2; wgpu 29; Slug vertex-pulled text; OIT (`StableTransparency`) for translucent world panels.
- **Layout:**
  - `render/` — `batch_key.rs`, `draw_order.rs`, `panel_geometry.rs`, `sdf_material.rs`, `material.rs`, `constants.rs`, `mod.rs`
  - `render/analytic_paths/` — `packing.rs` (`RunRecord`), `batching.rs` (`BatchGpu`), `analytic_path.wgsl`, `analytic_path_vertex_pull.wgsl`
  - `render/panel_text/batching.rs`, `render/panel_lines/batching.rs`
  - `shaders/sdf_panel.wgsl`
  - `layout/render.rs` (`RenderCommand`/`RenderCommandKind`), `layout/text_props.rs` (`DrawZIndex`)
  - `text/slug/glyph/coverage_probe.rs` (`EXPECTED_SHADER_FNV1A`)
  - `examples/`
- **Key files:**
  - `render/batch_key.rs` — `BaseMaterialId` newtype (`:32`; derives `Clone, Copy, Debug, Eq, Hash, PartialEq` — NOT `#[repr(transparent)]`, NOT `ShaderType`); `VisualBatchKey` (`:123`; 6 fields `base_material, alpha, lighting, sidedness, shadow, layers`); `InternedMaterialKey` (`:142`; hashes `base_color, emissive, metallic, perceptual_roughness, reflectance` + texture asset IDs + pipeline flags); `VisualMaterialInterner` (`:194`), `intern_base_material` (`:203`, mints `materials.len()` as the next id), `base_material` reverse lookup (`:216`).
  - `render/draw_order.rs` — `DrawOrderProjection::from_commands` (`:141`), `depth_for(cmd_index)` (`:168`), `level_occupancy()` (`:173`); `DrawCommandDepth` (`:53`; `ordinal :54`, `z_level :55`, `screen_depth_bias :56`, `oit_depth_offset :57`) with `ordinal_index()` (`:127`), `depth_bias()` (`:133`), `oit_depth_offset()` (`:136`); `HierarchicalDrawKey` (`:69`); `text_batch_depth_bias` (`:200`), `line_batch_depth_bias` (`:205`).
  - `render/panel_geometry.rs` — `PanelSdfSurface` (`:45`; `command_index :47`, `draw_depth: DrawCommandDepth :49`); `reconcile_sdf_quads` (`:223`) builds its reuse map from `command_index` (`:271`); `gather_surfaces` (`:387`) reads `draw_order.depth_for(cmd_index)` (`:404`); `spawn_sdf_quad` (`:612`); per-fill screen bias write `base.depth_bias = surface.draw_depth.depth_bias().get()` (`:507`); overflow guard `per_level_band_overflows` (`:316`) / `oit_total_overflows` (`:325`) inside `reconcile_sdf_quads` + tests (`:740`, `:748`).
  - `render/material.rs` — `resolve_material` (`:46`) folds the per-element layout color into `StandardMaterial.base_color`; called from `panel_geometry.rs:506`.
  - `render/sdf_material.rs` — `SdfPanelUniform` (`:31`; `half_size :33`, `mesh_half_size :37`, `corner_radii :39`, `border_widths :41`, `sdf_kind :46`, `sdf_params :48`, `clip_rect :56`, `oit_depth_offset :61`); forced `double_sided = true`/`cull_mode = None` (`:161–162`), forced `AlphaMode::Blend` (`:164`); `:165` reads the per-fill alpha OUT of `base_color.a` into the `fill_alpha` uniform (shadow prepass) — `base_color.a` is already the alpha source.
  - `render/analytic_paths/packing.rs` — `RunRecord` (`:165`; `transform`, `fill_color`, `render_mode`, `depth_nudge :173`, `oit_depth_offset :175`, `aa_flags`); `RunRecord::SHADER_SIZE` static-assert == 96 (`:188`).
  - `render/analytic_paths/batching.rs` — `BatchGpu` (`:78`; `run_table :82`, `mesh :84`, `material :86`, `capacity :88`).
  - `render/analytic_paths/analytic_path_vertex_pull.wgsl` — `local_index = vertex_index − mesh[instance_index].first_vertex_index` (`:72`); non-OIT depth-shift `#ifndef OIT_ENABLED` (`:108`).
  - `render/analytic_paths/analytic_path.wgsl` — `pbr_input_from_standard_material(in, is_front)` (`:967`), `apply_pbr_lighting(pbr_input)` (`:979`).
  - `shaders/sdf_panel.wgsl` — `pbr_input_from_standard_material(in, is_front)` (`:394`), `apply_pbr_lighting(pbr_input)` (`:461`).
  - `render/panel_text/batching.rs` — `update_panel_text_batches` (`:139`, main-world `PostUpdate`); `batch_material` (`:762`) writes `text_batch_depth_bias(key.z_level)` (`:774`); `commit_batch_buffers` (`:449`); `grow_batch_assets` (`:685`); `RunRecord.depth_nudge` write (`:317`).
  - `render/panel_lines/batching.rs` — `PanelLineBatchStore` (`:191`) with a per-store `VisualMaterialInterner` (`:194`).
  - `render/constants.rs` — `DRAW_LEVEL_GEOMETRY_LANES = 64` (`:8`), `DRAW_LEVEL_STRIDE = 65` (`:10`), `DRAW_LEVEL_TEXT_SUBLANE = 64` (`:12`), `OIT_DEPTH_STEP = 1e-6` (`:43`), `OIT_FOCUS_DEPTH = 0.001` (`:45`).
  - `layout/render.rs` — `RenderCommand.z_index: DrawZIndex` (`:26`); `RenderCommandKind` (`:68`; `Rectangle :70`, `Text :77`, `Border :84`, `Lines :96`, `ScissorStart`/`ScissorEnd`); `draw_step()` (`:110`).
  - `layout/text_props.rs` — `DrawZIndex` newtype (`:170`), default `DrawZIndex(0)`.
  - `text/slug/glyph/coverage_probe.rs` — `EXPECTED_SHADER_FNV1A` (~`:871`), hashes **only** `analytic_path.wgsl`.
  - bevy_pbr `pbr_material.rs` (0.19.0-rc.2, `~/rust/bevy-0.19.0-rc.2/crates/bevy_pbr/src/pbr_material.rs`) — `StandardMaterial` declares `#[data(0, StandardMaterialUniform, binding_array(10))]` (`:23`); `pub ior: f32` (`:338`); `StandardMaterialUniform` is a public `ShaderType` (`:1012`); `AsBindGroupShaderType<StandardMaterialUniform> for StandardMaterial` → `as_bind_group_shader_type(&self, images: &RenderAssets<GpuImage>) -> StandardMaterialUniform` (`:1066`).
- **Build:** `cargo build -p bevy_diegetic` (full: `cargo build --workspace --all-features --examples`).
- **Test:** `cargo nextest run -p bevy_diegetic` — **never `cargo test`**.
- **Lint:** `cargo clippy -p bevy_diegetic --all-targets` (no new warnings); `cargo +nightly fmt`.
- **Style:** `zsh ~/.claude/scripts/load-rust-style.sh --project-root /Users/natemccoy/rust/bevy_diegetic_gpu_meter` — obey `[non-negotiable]` rules + forbidden-words list; no rationale-justifying comments; state mechanisms literally.
- **Invariants:**
  - **Material-as-data.** The batch key carries NO scalar/vector PBR values. Color, alpha, metallic, perceptual_roughness, reflectance, emissive, and `ior` live only in `material_table[material_index]`. Two elements differing only in those values share one batch and one draw. A new upstream `StandardMaterial` field rides in `StandardMaterialUniform` automatically — no record, key, or binding change.
  - **Per-element slot allocation (no leak, no cleanup).** Each live element owns one `material_table` slot keyed by its reconcile identity `(panel_entity, command_index)`, NOT by material value. A material change overwrites that one slot in place (the record's `material_index` does not change, no other slot is touched). Reconcile removal frees the slot to a free-list for reuse. Table size tracks live element count (scene-bounded) — no append-only growth, no value deduplication, no scan/interval/threshold cleanup pass. Two identical materials get two slots; dedup never affected batching (the key holds no material values) so it is dropped.
  - **Sorted/OIT parity.** `DrawOrderProjection` depth orders any two commands identically on the sorted screen `depth_bias` axis and the OIT per-fragment `oit_depth_offset` axis. Preserved by construction; the existing `draw_order.rs` parity tests must stay green.
  - **Cross-panel screen anchoring.** A cross-panel screen batch is one `Transparent3d` item, sortable at one distance only — so the **screen** batch key includes `panel_entity` (`(view, panel_entity, z_level, VisualBatchKey)`). The **OIT** key omits it (`(view, z_level, VisualBatchKey)`) because per-fragment OIT sort uses the per-record `oit_depth_offset`.
  - **OIT focus-depth budget.** Per-panel ordinal span × `OIT_DEPTH_STEP (1e-6)` must stay inside the focus-depth budget (`OIT_FOCUS_DEPTH / OIT_DEPTH_STEP ≈ 1000`). Past it, ordering degrades to OIT-list insertion order — never a step inversion.
  - **Per-level 64-lane screen-band ceiling.** Each z-level owns `DRAW_LEVEL_GEOMETRY_LANES = 64` geometry sub-lanes (`DRAW_LEVEL_STRIDE = 65`; line lane 63, text lane 64). Batching fills into one draw does NOT relax this — each fill still needs a distinct screen sub-lane; >64 fills at one z-level overflows the band.
  - **ShaderBuffer rebind hazard.** `ShaderBuffer::set_data` with a changed byte length re-creates the wgpu buffer; existing bind groups do not follow. Pad every record/table buffer to a power-of-two capacity; on growth allocate a new buffer + inert mesh and rewrite the material's storage-buffer handle (the `BatchGpu`/`grow_batch_assets` discipline).
  - **Reconcile identity.** The retained fill store keys on `(panel_entity, command_index)`. The reuse signature stores the whole `DrawCommandDepth`, not a scalar ordinal — a `text_anchor` move changes `oit_depth_offset` while identity holds, and a scalar would miss the buffer update. A z-index or material move re-keys/rewrites the buffer record; it never respawns an entity.
  - **Build green each phase.** `cargo build && cargo +nightly fmt` + `cargo nextest run` pass before the next phase starts. Inert helpers are gated `#[cfg_attr(not(test), expect(dead_code, reason = "…"))]`, not deleted, until a consumer lands.
  - **Texture boundary.** The table varies every scalar/vector value per element but CANNOT vary the sampled textures inside one draw (textures are bind-group resources). The texture set therefore stays in the batch key: fills/text differing in any sampled texture split into separate batches. Acceptable — diegetic fills are solid/rounded rects with no image texture and text shares one glyph atlas.

## Phases

### Phase 1 — Shared material table foundation (inert) · status: todo

#### Work Order

**Goal:** A `SharedMaterialTable` render resource exists — a per-element
identity-keyed slot allocator plus the GPU `material_table` storage buffer of
`StandardMaterialUniform` entries — and `BaseMaterialId` serializes to a bare
`u32` at the GPU boundary. Nothing reads the table yet; the build is green and
the allocator is unit-tested.

**Spec:**

- **`BaseMaterialId` becomes GPU-serializable** (`batch_key.rs:32`). Add
  `#[repr(transparent)]` + a `ShaderType` derive (or a trivial manual `ShaderType`
  impl) so it serializes as a bare `u32` at the GPU boundary while staying a
  distinct compiler-checked type on the CPU. Keep the existing
  `Clone, Copy, Debug, Eq, Hash, PartialEq` derives. This is the shared slot type
  fills, text, and lines all carry.
- **`SharedMaterialTable` resource** (new file, e.g. `render/material_table.rs`,
  wired via `render/mod.rs`). It owns:
  - **A per-element slot allocator** — the dense-`u32` minting machinery from
    `VisualMaterialInterner` (`batch_key.rs:194/203`) repurposed: a map keyed by
    `(panel_entity, command_index, MaterialRole)` → `BaseMaterialId` slot, plus a
    free-list of released slots. `MaterialRole ∈ {Fill, Border}` — a fill+border
    element is one surface with one `command_index` but needs TWO table slots (the
    `material_index` and `border_material_index` of its `FillRecord`), so a slot is
    identified by role, not by `command_index` alone.
    - `alloc(panel_entity, command_index, role) -> BaseMaterialId` — reuse a
      free-list slot if any, else mint `next` (`materials.len()`).
    - `set(slot, StandardMaterial)` — overwrite that slot's stored
      `StandardMaterial` in place (no new slot, no other slot touched). Allocation
      and `set` run **main-world** during reconcile; the stored value is the
      `StandardMaterial` itself, NOT the converted uniform (the conversion needs
      render-world `RenderAssets<GpuImage>` — see the owning system below).
    - `free(panel_entity, command_index)` — return BOTH of the element's slots
      (Fill and Border) to the free-list.
    - **No value deduplication** (two identical materials get two slots). **No
      scan/interval/threshold cleanup.** Table size = live element count ×
      occupied roles.
  - **The GPU buffer** — `material_table: array<StandardMaterialUniform>` in a
    `Handle<ShaderBuffer>`, capacity padded to `live.next_power_of_two().max(1)`.
- **World boundary (Open decision 1, made concrete).** Slot allocation, `set`,
  and `free` run **main-world** during reconcile (`PanelChildSystems::Build` in
  `PostUpdate`), where the `StandardMaterial` values exist but
  `RenderAssets<GpuImage>` does not. An **Extract** step carries the per-slot
  `StandardMaterial` values and the identity/free-list deltas into the render
  world. A **render-world prepare system** owns the GPU buffer: it converts each
  live slot's `StandardMaterial` through
  `StandardMaterial::as_bind_group_shader_type(&self, images: &RenderAssets<GpuImage>)`
  (the `AsBindGroupShaderType<StandardMaterialUniform>` impl, bevy_pbr
  `pbr_material.rs:1066`) — where `RenderAssets<GpuImage>` resolves the
  texture-handle fields to bind-array indices/flags — and uploads the
  `StandardMaterialUniform` values into the buffer. Reuse this uniform type and
  conversion; do NOT define a parallel material struct. Bind the buffer at a fixed
  group/slot reserved identically for fills, text, and lines.
- **Batch registry + ordered rebind.** Provide an API for a batch material to
  register its handle with the table. On growth (capacity change) the owning
  system reallocates the buffer and rewrites **every** registered batch material's
  table-buffer handle in ONE ordered pass — before any batch material is
  re-prepared and before the draw stream runs — so no batch reads a stale table
  within a frame (honors the ShaderBuffer rebind hazard). The grow trigger is
  **global**: table size is the total live slot count across all panels and all
  element kinds, so a text-run change can grow the buffer and force a fill-batch
  handle rewrite the same frame — which is why the rewrite pass must touch every
  registered batch of every kind, not just the kind that grew. Registrants are
  empty in Phase 1; the fill batch registers in Phase 3, text/lines in Phase 5.
- **Version pin.** Add a `StandardMaterialUniform` `SHADER_SIZE` static-assert so a
  bevy minor upgrade that reorders/resizes the uniform fails the build rather than
  silently mis-reading the table.
- **Inert.** Gate any not-yet-called helper
  `#[cfg_attr(not(test), expect(dead_code, reason = "consumed by the fill batch in Phase 3"))]`.

**Files:**
- `render/batch_key.rs` — `BaseMaterialId` `#[repr(transparent)]` + `ShaderType`.
- `render/material_table.rs` (new) — `SharedMaterialTable` resource, allocator,
  GPU buffer, owning system, batch registry, `SHADER_SIZE` assert.
- `render/mod.rs` — wire `mod material_table;` + register the resource/system.

**Constraints from prior phases:** none (first phase).

**Acceptance gate:** `cargo build -p bevy_diegetic` clean, `cargo +nightly fmt`,
`cargo clippy -p bevy_diegetic --all-targets` no new warnings,
`cargo nextest run -p bevy_diegetic` green. New unit tests: `alloc`/`free`/reuse
keeps table size == live count; repeated `set` on one slot never grows the table;
`free` then `alloc` returns the freed slot; the `StandardMaterialUniform`
`SHADER_SIZE` assert compiles. Nothing else references the table yet.

### Phase 2 — Re-home the overflow guard to a shared panel-draw-order limits system · status: todo

#### Work Order

**Goal:** The per-level band + OIT-budget overflow guard runs from a shared system
keyed on each panel's `DrawOrderProjection`, independent of the per-quad SDF path
(which Phase 3 removes) and any future fill feature flag. Behavior is unchanged
for current panels — the same `warn_once!` messages fire for text-only,
line-only, and fill panels.

**Spec:**

- Move the guard out of `reconcile_sdf_quads` (`panel_geometry.rs:237`) into a
  shared changed-panel system, e.g.
  `render::draw_order_limits::warn_panel_draw_order_limits(panel_entity, &DrawOrderProjection)`,
  emitting the same `warn_once!` messages.
- Keyed on each panel's `DrawOrderProjection`. It reads `level_occupancy()`
  (`draw_order.rs:173`) and checks the **smaller of two ceilings**: (1) the
  per-level band capacity — draw commands at a single z-level must stay below
  `DRAW_LEVEL_GEOMETRY_LANES (64)` (`per_level_band_overflows`); (2) the OIT budget
  (`oit_total_overflows`). It counts the **full** command stream (geometry + the
  fixed line lane 63 + the fixed text lane 64), not only fills, so text-only and
  line-only panels keep warning regardless of the fill path.
- Move with the guard: the predicates
  `per_level_band_overflows`/`oit_total_overflows` AND their dependencies
  `per_level_band_capacity()` (`panel_geometry.rs:312`) and `oit_depth_budget()`
  (`:318`), plus the two tests `per_level_band_overflows_at_screen_band_capacity`
  (`panel_geometry.rs:740`) and `oit_total_overflows_at_depth_budget` (`:748`).
  Leaving the capacity/budget helpers in `panel_geometry.rs` orphans them once
  Phase 3 strips the per-quad path.

**Files:**
- `render/panel_geometry.rs` — remove the guard call + helpers from
  `reconcile_sdf_quads`.
- `render/draw_order_limits.rs` (new) — the shared system + `per_level_band_overflows`/`oit_total_overflows` + `per_level_band_capacity()`/`oit_depth_budget()` + their tests.
- `render/mod.rs` — wire the module + schedule the system on changed panels.

**Constraints from prior phases:** Phase 1 added the inert `SharedMaterialTable`
(not used here). The guard's inputs (`DrawOrderProjection::level_occupancy()`, the
band/OIT constants) are unchanged.

**Acceptance gate:** `cargo build -p bevy_diegetic` clean, `cargo +nightly fmt`,
`cargo clippy -p bevy_diegetic --all-targets` no new warnings,
`cargo nextest run -p bevy_diegetic` green including the moved tests. The overflow
warning still fires at the smaller of the per-level band-capacity and OIT-budget
ceilings for a current (non-batched-fill) panel.

### Phase 3 — Batched fill path (fills join the vertex-pulled path) · status: todo

#### Work Order

**Goal:** Fills render from a per-batch `FillRecord` storage buffer through one
draw per `(view, panel_entity, z_level, VisualBatchKey)` (screen) /
`(view, z_level, VisualBatchKey)` (OIT), reading material from the shared table.
Per-fill SDF quad entities are gone. Render output matches the current per-fill
path for panels with no per-element material variety; two fills differing only in
material share one batch.

**Spec:**

- **`FillRecord`** (shader struct; derives `ShaderType` + `SHADER_SIZE`
  static-assert). Holds only per-fill geometry, ordering, and material indices —
  never material values:
  - `transform: Mat4` — quad-local to world.
  - `size: Vec2`, `half_size: Vec2`, `mesh_half_size: Vec2` — bounds, matching
    `SdfPanelUniform::half_size`/`mesh_half_size` (`sdf_material.rs:33/37`).
  - `corner_radii: Vec4`, `border_widths: Vec4`, `clip_rect: Vec4`
    (`sdf_material.rs:39/41/56`).
  - `sdf_kind: u32`, `sdf_params: Vec4` (`sdf_material.rs:46/48`).
  - `material_index: BaseMaterialId` — slot into `material_table` for the fill.
  - `border_material_index: BaseMaterialId` — slot for the border's material (a
    fill and its border draw from the same table without a separate border-color
    field; no-border fills reuse the same index with zero border width).
  - `depth_nudge: f32` — `DrawCommandDepth::depth_bias().get()` (non-OIT screen).
  - `oit_depth_offset: f32` — `DrawCommandDepth::oit_depth_offset().get()` (OIT).
  - **No** color/alpha/metallic/roughness/reflectance/emissive/`ior` fields — those
    are read from `material_table[material_index]`. `FillRecord` lands near 192
    bytes (std430 vec4 padding on `corner_radii`/`sdf_params`; reorderable toward
    ~176 by grouping the `Vec4`s); keep the `SHADER_SIZE` static-assert on the
    final layout.
  - **Padding records:** zero `mesh_half_size`, `material_index =
    border_material_index = 0`. The fill shader early-outs on zero `mesh_half_size`
    **before any `material_table` read**, so a padding record's index never reaches
    the PBR path — no sentinel slot is reserved (the allocator mints index 0 for
    the first real material).
- **CPU retained record** also stores `panel_entity`, `command_index`, the whole
  `DrawCommandDepth`, and the resolved `material_index` (not shader fields), so
  reconcile can detect a material change.
- **`FillBatchGpu` lifecycle**, mirroring the text `BatchGpu`
  (`analytic_paths/batching.rs:78`): `records: Handle<ShaderBuffer>`,
  `mesh: Handle<Mesh>`, `material`, `capacity: u32`. `capacity =
  live_records.next_power_of_two().max(1)`. Same-capacity edits `set_data` (like
  `commit_batch_buffers`, `panel_text/batching.rs:449`); on growth allocate a new
  buffer + inert capacity-sized quad mesh, insert the new `Mesh3d`, and rewrite the
  material's storage-buffer handle (like `grow_batch_assets`,
  `panel_text/batching.rs:685`). **Register the fill batch material with
  `SharedMaterialTable`** (Phase 1 registry) so a table grow rewrites its handle in
  the ordered pass.
- **Vertex pull.** The fill mesh is an inert capacity-sized quad mesh; the vertex
  shader subtracts `mesh[instance_index].first_vertex_index` from `vertex_index`
  for a local index, then `record = local_index / vertices_per_quad`, matching
  `analytic_path_vertex_pull.wgsl:72`.
- **Gather.** Iterate the `RenderCommand` stream; for every
  `RenderCommandKind::Rectangle` (`render.rs:70`) and `Border` (`:84`) call
  `draw_order.depth_for(cmd_index)` as `gather_surfaces` does
  (`panel_geometry.rs:387/404`) and write the returned `DrawCommandDepth` into the
  record. For each record, allocate/look up the element's TWO table slots from
  `SharedMaterialTable` — `(panel_entity, command_index, Fill)` for `material_index`
  and `(panel_entity, command_index, Border)` for `border_material_index` — and
  `set` each role's current `StandardMaterial` (a no-border fill still owns a Border
  slot, pointed at by `border_material_index` with a zero border width).
- **Batch key.** `VisualBatchKey`'s 6 fields resolve for fills as:
  - `base_material` — the fill key's material component is **the texture set +
    pipeline flags ONLY** (`base_color_texture`, `emissive_texture`,
    `metallic_roughness_texture`, `normal_map_texture`, `occlusion_texture`), with
    **every** scalar/vector PBR value excluded. **Required code change:** today
    `InternedMaterialKey` (`batch_key.rs:142`) hashes the scalar PBR fields and
    `resolve_material` (`render/material.rs:46`) folds the per-element layout color
    into the interned `StandardMaterial` (`panel_geometry.rs:506`) — reused as-is,
    fills would split by color. Define a **distinct fill texture-set key
    constructor** (a positive set: texture bindings + pipeline flags), NOT
    `InternedMaterialKey` minus the scalars, so a future `StandardMaterial` scalar
    field cannot silently re-enter the key. For fills, `resolve_material` must NOT
    fold `effective_color` (or any per-element numeric factor) into the interned
    `StandardMaterial`.
  - `alpha` — fills force `AlphaMode::Blend` (`sdf_material.rs:164`); constant,
    never splits. `base_color.a` is already the per-fill alpha source
    (`sdf_material.rs:165` reads it OUT into the separate `fill_alpha` uniform for
    the shadow prepass), so it lands in the element's table entry directly — an
    alpha-only difference is a table write, not a batch split.
  - `sidedness` — fills force `double_sided = true`/`cull_mode = None`
    (`sdf_material.rs:161–162`); constant, never splits.
  - `lighting` (`unlit`), `shadow` (`VisualShadow`), `layers` (`BatchRenderLayers`)
    — real splitters (pipeline / prepass / view-routing); not expressible as table
    data.
  - **OIT key:** `(view, z_level, VisualBatchKey)`. **Screen key:**
    `(view, panel_entity, z_level, VisualBatchKey)` (cross-panel anchoring).
- **Screen sort.** Each batch's live records are CPU-sorted by
  `DrawCommandDepth::ordinal_index()` (`draw_order.rs:127`) then `command_index`;
  the index buffer visits quads in record order, so a higher ordinal composites
  later (matching the `ScreenDepthBias` rule). A new
  `fill_batch_depth_bias(z_level)` helper beside `text_batch_depth_bias`/
  `line_batch_depth_bias` (`draw_order.rs:200/205`) returns the base geometry lane
  for the z-level, written onto the batch material's `depth_bias` (the
  `Transparent3d` sort key). `fill_batch_depth_bias(z_level)` returns the **base**
  of that z-level's geometry lanes (below the line lane 63 and text lane 64), so
  the whole fill batch sorts under same-level lines and text by construction;
  intra-batch order is the record (ordinal) sort, and the per-level 64-lane ceiling
  still bounds the geometry count (invariant + the Phase 2 guard). The per-record
  `depth_nudge` shifts shader clip-space depth, not CPU submission order.
- **Shared shader PBR-from-table function.** Add a diegetic-owned WGSL function
  `pbr_input_from_material_table` (its own importable module) that reads
  `let m = material_table[material_index];`, populates the `PbrInput.material`
  fields (base color, metallic, roughness, reflectance, emissive, `ior`, flags)
  from `m` exactly as `pbr_input_from_standard_material` copies them out of the
  per-draw uniform today, applies SDF coverage to alpha and the border composite as
  the current shaders do, then calls the existing `apply_pbr_lighting(pbr_input)`
  unchanged (`bevy_pbr::pbr_functions`). The fill fragment shader uses this
  function **instead of** `pbr_input_from_standard_material(in, is_front)`
  (`sdf_panel.wgsl:394`). Apply `depth_nudge` only when `OIT_ENABLED` is absent
  (`analytic_path_vertex_pull.wgsl:108`). **Define this function shared from the
  start** — text/lines reuse it unchanged in Phase 5, so the PBR-from-table path
  cannot drift between element kinds. Texture samples still come from the batch's
  bind group (why the texture set stays in the key).
- **Reconcile.** Identity `(panel_entity, command_index)`; reuse signature stores
  the whole `DrawCommandDepth`. A material-value change (color/alpha/metallic/
  roughness/reflectance/emissive/`ior`) overwrites the element's table slot in
  place — `material_index` does not change, the batch does not split, the record
  does not move. A `z_level` or `VisualBatchKey` (texture set, `unlit`, shadow,
  layers) change removes the record from the old batch and inserts into the new
  one. A bounds/radii/border/clip change rewrites the record fields and marks the
  batch bounds dirty. Reconcile removal `free`s BOTH of the element's table slots
  (Fill and Border). No per-fill render entity exists, so a z-index/material move
  rewrites buffers (and may despawn a now-empty batch entity), never an individual
  fill entity.
- **FNV tripwire.** The new fill shader is separate from `analytic_path.wgsl`
  (the only file `EXPECTED_SHADER_FNV1A` hashes). If `analytic_path.wgsl` is
  untouched, no refresh; verify `EXPECTED_SHADER_FNV1A` still matches before
  relying on no-refresh.

**Files:**
- `render/panel_geometry.rs` — remove the per-fill `spawn_sdf_quad` path; gather
  fills into records; `reconcile_sdf_quads` becomes the fill-batch reconcile.
- `render/fill_batch.rs` (new) — `FillRecord` packing + `SHADER_SIZE` assert,
  `FillBatchGpu` lifecycle, batch build/sort, registry registration.
- `render/batch_key.rs` — the fill texture-set key constructor.
- `render/material.rs` — `resolve_material` must not fold per-element color for
  fills.
- `render/draw_order.rs` — `fill_batch_depth_bias(z_level)`.
- new fill fragment/vertex WGSL + the shared `pbr_input_from_material_table` WGSL
  module; `render/mod.rs` wiring.

**Constraints from prior phases:** Phase 1 built `SharedMaterialTable` (per-element
identity-keyed allocator + GPU `material_table` buffer + batch registry + ordered
rebind), made `BaseMaterialId` `#[repr(transparent)]` + `ShaderType`, and pinned
`StandardMaterialUniform` `SHADER_SIZE`. Phase 2 moved the overflow guard out of
`reconcile_sdf_quads` into `warn_panel_draw_order_limits`, so removing the per-quad
SDF path here leaves the guard intact.

**Acceptance gate:** `cargo build -p bevy_diegetic` clean, `cargo +nightly fmt`,
`cargo clippy -p bevy_diegetic --all-targets` no new warnings,
`cargo nextest run -p bevy_diegetic` green. Tests: (1) **render-equivalence** — for
a representative no-variety panel the batched fill records' `depth_nudge`/
`oit_depth_offset` + geometry match the pre-batch per-fill material values; (2)
**batch collapse** — two/three overlapping fills differing only in material land in
ONE batch and the CPU sort writes them in ordinal order; (3) **screen + OIT
ordering** — a `z=+1` fill renders above default text; a `z=−1` fill below default
fills, on both views; (4) **reconcile** — a material-only change rewrites the
element's table slot and does not split/move the batch; a `text_anchor` toggle
updates the stored `oit_depth_offset`. Behavior: a panel of many differently
materialed fills emits one draw per `(view, panel, z_level, key)`.

### Phase 4 — Animated-color validation (example + statistics + tests) · status: todo

#### Work Order

**Goal:** A runnable example animates per-element fill color every frame and
surfaces live batch statistics (draw/batch counts) and table statistics (live
slots, free slots, capacity), proving memory stays flat under sustained animation
and batching is unaffected. Tests assert the same.

**Spec:**

- **Example** (`examples/element_batching.rs` or similar): one panel with many
  fills whose `base_color` animates per frame; an on-screen/log readout of the
  batch count and the table `(live, free, capacity)` numbers, updating live.
- **Statistics accessors** on `SharedMaterialTable` (live slot count, free-list
  length, buffer capacity) if not already exposed in Phase 1.
- **Tests:** animating per-element color over N frames keeps the table live-slot
  count == element count and capacity stable (no growth); the batch count is
  unchanged by color animation (color is not a batch-key field). A capacity
  assertion stays as a loud backstop.

**Files:**
- `examples/element_batching.rs` (new).
- tests near `render/fill_batch.rs` / `render/material_table.rs`.

**Constraints from prior phases:** Phase 3 batched fills read per-element material
from the Phase 1 shared table via the identity-keyed allocator; a color change is
one slot overwrite. The statistics this phase reads come from the Phase 1
allocator.

**Acceptance gate:** `cargo build --workspace --all-features --examples` clean;
the example runs and shows flat table memory + stable batch count under sustained
color animation; `cargo nextest run -p bevy_diegetic` green including the
flat-memory + stable-batch-count tests.

### Phase 5 — Migrate text and lines onto the shared table · status: todo

#### Work Order

**Goal:** Text and line batches drop their per-batch `StandardMaterial` and
per-record `fill_color` and read material from the one shared `material_table`,
gaining per-element metallic/roughness/emissive/reflectance/`ior`. Current
text/line content renders unchanged; the `diegetic_text_stress` single-batch
invariant holds.

**Spec:**

- **`RunRecord`** (`analytic_paths/packing.rs:165`) replaces `fill_color: Vec4`
  with `material_index: u32` (a `BaseMaterialId`; 12-byte reduction per record).
  Re-assert the new (smaller) `RunRecord::SHADER_SIZE` (`:188`).
- The text/line fragment path switches to the shared `pbr_input_from_material_table`
  function (Phase 3) instead of `pbr_input_from_standard_material` + the per-run
  `fill_color` override (`analytic_path.wgsl:967`).
- Replace the per-store `VisualMaterialInterner` in `analytic_paths/batching.rs`
  and `panel_lines/batching.rs` (`:194`) with the shared `SharedMaterialTable`.
  Text/line batch materials **register** with it (Phase 1 registry). Each run
  allocates a per-element slot keyed by its reconcile identity and `set`s its
  current material; the table is the one shared buffer across fills, text, and
  lines.
- **FNV refresh required here.** This phase edits `analytic_path.wgsl` (the text
  shader now reads the table), which `EXPECTED_SHADER_FNV1A` hashes
  (`coverage_probe.rs ~:871`). After editing, run the test, read the printed hash,
  paste it into `EXPECTED_SHADER_FNV1A` in this same commit.

**Files:**
- `render/analytic_paths/packing.rs` — `RunRecord` `fill_color` → `material_index`,
  `SHADER_SIZE` re-assert.
- `render/analytic_paths/analytic_path.wgsl` — read the shared table via
  `pbr_input_from_material_table`.
- `render/analytic_paths/batching.rs`, `render/panel_lines/batching.rs`,
  `render/panel_text/batching.rs` — interner → shared table; register batch
  materials.
- `text/slug/glyph/coverage_probe.rs` — refresh `EXPECTED_SHADER_FNV1A`.

**Constraints from prior phases:** Phase 1 built the shared table + registry +
ordered rebind and the `BaseMaterialId` GPU type. Phase 3 proved the table, the
per-element allocator, the batch registration, and defined the shared
`pbr_input_from_material_table` WGSL function — text/lines reuse all of it; the
only new work is swapping the record's color field for the index and applying the
shared shader change.

**Acceptance gate:** `cargo build -p bevy_diegetic` clean, `cargo +nightly fmt`,
`cargo clippy -p bevy_diegetic --all-targets` no new warnings,
`cargo nextest run -p bevy_diegetic` green including the refreshed FNV tripwire and
the `diegetic_text_stress` single-batch invariant. Current text/line content
renders unchanged; a text/line run authored with a non-default metallic/roughness/
emissive/reflectance/`ior` renders with that PBR variety from one shared table.
