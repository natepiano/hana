# Image Batching

## Goal

Route diegetic image leaves through the same kind of batched renderer used by
SDF surfaces, text, and panel shapes.

The current image path is intentionally simple but no longer good enough:
`RenderCommandKind::Image` and `RenderCommandKind::PrecomposeLdr` create or
reuse ordinary child mesh entities. That makes image layers, shadow policy, and
draw-order depth state special-case reconciliation work instead of normal batch
routing.

The target model is:

```text
layout command -> image render record -> image batch key -> image batch entity
```

Images should still draw in `DrawSortTier::Surface`, because they occupy the
same conceptual layer as fills, borders, and precomposed surfaces. They should
gain their own batch family because their shader inputs are texture quads, not
SDF surfaces.

## Current State

`RenderCommandKind::Image` is a textured quad:

```rust
Image {
    handle: Handle<Image>,
    tint: Color,
}
```

`RenderCommandKind::PrecomposeLdr` is also image-like at render time: it draws
the cached precompose render target as one textured quad.

Today both commands return `DrawSortTier::Surface`, but neither returns a
`DrawBatchFamily`; `draw_batch_family()` returns `None` for images and
precompose commands. The renderer therefore routes them through
`reconcile_panel_image_children`.

That path keeps one child entity per image command with:

```rust
PanelImageChild {
    element_idx,
    draw_depth,
    handle,
    tint,
    bounds,
    shadow_casting,
}
```

The child entity owns a rectangle mesh and a `StandardMaterial` whose
`base_color_texture` is the image handle and whose `base_color` is the tint.
This has three concrete problems:

- Image children are still ordinary entities, so render layers and shadow
  policy have to be synchronized during reconciliation.
- Shadow policy is managed by inserting/removing `NotShadowCaster` on each
  image child instead of routing through a batch key.
- A reused child can return early unless every render-affecting input is in
  `PanelImageChild`, so every new policy becomes another cache field.

## Immediate Fix

Before batching, the current child-entity path has already received the
correctness fix:

- Query the owning panel's effective `RenderLayers`.
- Query the owning panel's resolved `ShadowCasting`.
- Use those values when spawning an image child.
- Store the effective shadow policy in `PanelImageChild`.
- Keep the effective layer as the child entity's `RenderLayers` component.
- Compare cached shadow policy when reusing an image child.
- Update the child entity's `RenderLayers` and `NotShadowCaster` state even
  when image handle, tint, bounds, and draw depth are unchanged.

This is a correctness fix, not the batching project. Image children must never
hard-code layer 0 or drift from the panel shadow cascade.

## Batching Model

Add an image batch family:

```rust
DrawBatchFamily::Image
```

Then route:

```rust
RenderCommandKind::Image        -> DrawBatchFamily::Image
RenderCommandKind::PrecomposeLdr -> DrawBatchFamily::Image
```

Both stay in:

```rust
DrawSortTier::Surface
```

because their relative order against fills, borders, text, and panel shapes is
still decided by `DrawOrderKey` / `DrawCommandDepth`.

## Batch Key

An image batch can only combine records that agree on GPU state shared by one
draw. The first useful batch key should include:

```rust
ImageBatchKey {
    texture: Handle<Image>,
    layers: BatchRenderLayers,
    shadow: VisualShadow,
    screen_depth_bias: ScreenDepthBias,
    alpha_mode: BatchAlphaMode,
}
```

Notes:

- `texture` stays in the key because ordinary sampled texture resources cannot
  vary per record inside one non-bindless draw.
- `layers` stays in the key because a batch entity has one `RenderLayers`
  component.
- `shadow` stays in the key because a batch entity either carries
  `NotShadowCaster` or it does not.
- `screen_depth_bias` stays in the key because Bevy's hardware depth bias is a
  material/draw property.
- `DrawOrderIndex` still belongs in each record, not in the batch key; it feeds
  `ClipDepthNudge` and `OitDepthOffset` just like the other batched render
  families.

This means the first batching pass batches repeated use of the same image
texture under the same shared render state. It does not magically batch every
unrelated texture into one draw.

## Render Record

Add an image render record that carries only per-image values:

```rust
ImageRenderRecord {
    bounds,
    tint,
    uv_rect,
    clip_depth_nudge,
    oit_depth_offset,
}
```

The initial `uv_rect` can be `0..1` for ordinary images and precompose outputs.
It becomes important once atlas-backed images exist.

The record should be the source of per-image tint. Do not use
`StandardMaterial::base_color` for tint if several records share one batch,
because `base_color` is a material-wide uniform. The image shader should sample
the shared texture and multiply by the record's tint.

## Material And Shader

Use a dedicated image batch material, for example:

```rust
ImageExtendedMaterial = ExtendedMaterial<StandardMaterial, ImageExtension>
```

`StandardMaterial` owns the shared image texture and shared pipeline state.
`ImageExtension` owns image-record buffer binding and shader plumbing.

The shader path should:

- draw one quad per image record,
- map the record's panel bounds to world/screen position,
- sample the batch texture using `uv_rect`,
- multiply by record tint,
- apply `ClipDepthNudge` in the vertex path for non-OIT,
- apply `OitDepthOffset` in the OIT path,
- preserve the same alpha/depth behavior as the current child-entity image
  material.

## Precompose

`PrecomposeLdr` should feed the same image batch path:

```text
precompose cache image handle -> ImageBatchKey.texture
precompose command bounds     -> ImageRenderRecord.bounds
Color::WHITE                  -> ImageRenderRecord.tint
```

Different precompose render targets usually have different texture handles, so
they will usually form separate image batches. That is still better than a
separate child entity path because ordering, layers, shadow policy, and depth
state use the same batch machinery as the rest of diegetic rendering.

## Arbitrary Texture Batching

Batching arbitrary distinct images into one draw requires one of these:

- an atlas, where many logical images share one physical texture and differ by
  `uv_rect`;
- bindless or texture-array support, where each record carries a texture index;
- a render-target atlas for precompose outputs.

The first image batching pass should not pretend ordinary distinct
`Handle<Image>` values can share one draw. It should build the batch family and
make repeated textures batch correctly. Atlas or bindless support can then
extend the record format without changing the public `b.image(...)` authoring
API.

## Implementation Plan

1. Fix current image child render layers and shadow casting. Done in the
   entity path.
2. Add `DrawBatchFamily::Image` and route `Image` / `PrecomposeLdr` to it.
3. Add `ImageBatchKey`, `ImageRenderRecord`, and `ImageBatchStore`.
4. Build image batch entities from image commands instead of spawning
   `PanelImageChild` children.
5. Add `ImageExtendedMaterial` / `ImageExtension` and WGSL record sampling.
6. Remove image command handling from `reconcile_panel_image_children` once the
   batch path is complete.
7. Keep precompose cache generation, but route the final precompose image
   through the image batch store.
8. Add optional atlas support as a later pass.

## Tests

Add focused tests for:

- image commands return `DrawBatchFamily::Image`;
- precompose commands return `DrawBatchFamily::Image`;
- repeated use of the same `Handle<Image>` and compatible shared state produces
  one image batch;
- different image handles split batches;
- different `RenderLayers` split batches;
- different `VisualShadow` values split batches;
- different `ScreenDepthBias` values split batches;
- record tint can differ inside one batch;
- record `DrawOrderIndex` changes update per-record depth values without
  changing the texture batch key;
- precompose output routes into the image batch path;
- the old hard-coded layer 0 behavior is gone;
- reused image children update `NotShadowCaster` when resolved
  `ShadowCasting` changes.

## Non-Goals

- Do not add atlas packing in the first batching pass.
- Do not add bindless texture indexing in the first batching pass.
- Do not change `b.image(el, handle, tint)` authoring.
- Do not make image intrinsic sizing part of this project.
- Do not route SDF, text, or panel-shape material texture sampling through this
  path; those are material-table features for their own render families

## Not to forget
./docs/bevy_diegetic/batching-diagram.md will need to be updated with the as-built as part of the implementation

## Resolved Design Decisions

These answers were resolved in review before implementation.

### 1. Generic `BatchStore<K, R>`: copy-adapt image first, extract after

`SdfBatchStore`, `PathBatchStore`, and `PanelShapeBatchStore` are three
independent near-copies (HashMap-by-key + upsert/remove/retain/take-empty + the
same reconcile/commit/bounds systems). Image would be a fourth.

Decision: **copy-adapt the image store first** (it is the simplest — single
texture per key, tint in-record, no material-table append), then **phase a
generic `BatchStore<K, R>` extraction into this project** as a dedicated later
step that refactors all four families onto the shared abstraction. The generic is
in scope for this project's completeness, not deferred to a follow-on — but it is
designed against four concrete stores, not three.

Scope note: image is the **last** render family that needs batching. Every
`RenderCommandKind` then routes through a `DrawBatchFamily`
(`Rectangle`/`Border` -> SdfSurface, `PanelShapes` -> PanelShape, `Text` -> Text,
`Image`/`PrecomposeLdr` -> Image). The only remaining unbatched commands,
`ScissorStart`/`ScissorEnd`, are clip-state control, not draws.

### 2. Opaque->Mask(0.0) shadow remap: shared helper

SDF (`sdf_batch_alpha_mode`, `fill_batch.rs:906`) and text
(`panel_text/batching.rs`) each independently map `Opaque + VisualShadow::Cast ->
Mask(0.0)` so a shadow-casting opaque batch keeps a maskable pipeline whose
material bind group survives the depth-only shadow pass and whose shader can still
discard by coverage.

Decision: **extract one shared helper** — `batch_shadow_alpha_mode(authored,
VisualShadow) -> BatchAlphaMode` (or similarly named) in `render/batch_key.rs`,
which already owns the shared `BatchAlphaMode` / `VisualShadow` key fragments. The
image family uses it, and the existing SDF and text call sites migrate onto it so
the three copies collapse to one. Images then respect their texture's alpha when
casting shadows instead of casting a solid quad.

### 3. Precompose color space: sRGB render target, sampled as sRGB

The precompose offscreen target (`precompose_image(pixel_size)`,
`render/precompose.rs`) is LDR. The old child path samples it through stock PBR
`base_color_texture`, which hardware-linearizes an sRGB texture on read. The new
image batch shader samples the texture directly, so the target format and the
shader's sampling must agree.

Decision: **use an sRGB render-target format (`Rgba8UnormSrgb`) and sample it as
an sRGB texture.** The offscreen pass encodes linear->sRGB on write; the batch
shader's hardware sampler decodes sRGB->linear on read. This round-trips exactly,
matches stock PBR `base_color_texture` behavior (so precompose output is identical
to today with no shader-side gamma math), and 8-bit sRGB has enough precision for
LDR. Do **not** use a linear `Rgba8Unorm` target, which diverges from the texture
convention and invites accidental double-conversion.

### 4. Image + border ordering via ClipDepthNudge

A border that bites into image size must composite **on top** of the image.

Decision: **the image record feeds `ClipDepthNudge` (and `OitDepthOffset`) from
its `DrawOrderIndex` exactly like SDF and path records do.** A border drawn after
the image carries a higher `DrawOrderIndex`, so its `ClipDepthNudge` pushes it
forward and it wins the depth test / composites over the image. No new machinery —
the image family reuses the shared per-record depth plumbing
(`render/draw_order.rs`), which is why `DrawOrderIndex` stays per-record and out
of `ImageBatchKey`.

## Collapsing Duplication (Generics + Traits)

This is the deferred half of decision #1: after image ships as a fourth
copy-adapted family, collapse the four near-identical families onto shared
generic machinery. Scoped here so the later phase has a target, not a blank page.

### What is duplicated today (per family)

Each of `SdfBatchStore`, `PathBatchStore`, `PanelShapeBatchStore` (and the new
`ImageBatchStore`) repeats the same shape:

- **Store resource**: `HashMap<Key, Batch>` + `record_index: HashMap<RecordKey,
  Key>`, with `upsert_record` / `remove_record` / `retain_records` /
  `take_empty_batches`.
- **Per-key batch container** (`SdfBatch` etc.): `Vec<Record>`, batch `entity`,
  `gpu: Option<Resources>`, record-upload / bounds `Dirty` flags,
  `first_draw_order_index`, and `upsert_record` / `remove_record` /
  `sort_records` / `world_bounds`.
- **Six PostUpdate systems**: build records from commands, update world
  transforms, reconcile batch entities + grow GPU buffers, update bounds, commit
  GPU buffers, register materials.

Only narrow pieces are already shared: the key fragments in `batch_key.rs`
(`BatchAlphaMode`, `BatchRenderLayers`, `VisualShadow`, compatibility newtypes),
the `Dirty` flag (`render/dirty.rs`), and the draw-order depth types
(`render/draw_order.rs`).

### Proposed collapse

A `BatchFamily` trait carrying the associated types and the per-family logic that
genuinely differs, with a generic `BatchStore<F: BatchFamily>` and generic systems
parameterized over `F`:

- `type Key: Eq + Hash` — the batch key (`SdfBatchKey`, `ImageBatchKey`, ...).
- `type Record` — the CPU record; `type GpuRecord: ShaderType` — the packed
  buffer row.
- `type Resources` — the per-batch GPU handles (record buffer, mesh, material
  handle).
- family hooks: build records from a command + resolved state, pack a
  `GpuRecord`, and produce the batch material.

The generic then owns store bookkeeping (`upsert`/`remove`/`retain`/
`take_empty`), the batch container, and the six systems as `system::<F>`
instantiations. Per-family code shrinks to the key/record definitions plus the
handful of hooks that actually differ (e.g. SDF's fill/border material-table
append vs. image's single in-record tint).

### Sequencing

Do this **after** all four concrete families exist, so the trait is designed
against four real shapes, not extrapolated from three. Migrate one family at a
time onto the generic. The public `b.image(...)` authoring API and the batch-key
contents do not change.

**Required agenda item — SDF/text shadow-alpha rule (from PD-1).** SDF remaps
`(Opaque, Cast) -> Mask(0.0)` (shadow-gated, `fill_batch.rs:906`); text remaps
`Opaque -> Mask(0.0)` unconditionally (`batching.rs:1169`) because opaque text
loses its material bind group in the camera depth/normal prepass, not just the
shadow pass; images are always `Blend` and need neither. When designing the trait,
this phase MUST explicitly decide how the family hook models opaque-remap: one
shared hook the families parameterize (would force SDF onto text's unconditional
rule — needs a prepass-strip parity test proving SDF still renders correctly), or
a per-family hook that keeps the two rules distinct with a documented reason. Do
not silently carry the divergence forward — resolve it here.

## Team Review Corrections (cycle 1, auto-recorded)

Determined corrections against the shipped code (`fill_batch.rs`,
`batch_key.rs`, `draw_order.rs`, `precompose.rs`, `panel_text/reconcile.rs`,
`panel_text/batching.rs`, `analytic_paths/material.rs`). These have a single
correct outcome and are folded into the spec above when it is compiled to a
phased plan. Consensus noted where multiple lenses agreed.

### Batch key must key on the hashable rank, not the f32 bias
`ImageBatchKey.screen_depth_bias: ScreenDepthBias` will not compile: `ScreenDepthBias`
is `f32`-backed, `PartialEq` only, no `Eq + Hash` (`draw_order.rs:70`), and the
store is `HashMap<ImageBatchKey, _>`. Mirror `SdfBatchKey`: carry `z_index:
DrawZIndex` + `z_index_rank: DrawZIndexRank` (both `Eq + Hash`) and derive the
material `depth_bias = z_index_rank.screen_depth_bias().get()` (`fill_batch.rs:881`).
The "different `ScreenDepthBias` splits batches" test becomes "different
`DrawZIndexRank`". Consensus: correctness + risk + architecture (critical).

### Render record needs a per-record world transform and panel record-key
`ImageRenderRecord { bounds, tint, uv_rect, ... }` carries only panel-local
bounds. One texture used by two panels forms one batch holding records from both,
and a batch entity gets no transform propagation — a moving panel would leave its
image behind. Mirror SDF's CPU/GPU split: CPU `ResolvedImageRecord { record_key:
{ panel, command_index }, bounds, tint, uv_rect, draw_depth, transform: Mat4 }`
and GPU `ImageRenderRecord: ShaderType { transform, size, uv_rect, tint,
clip_depth_nudge, oit_depth_offset }`, plus an `update_image_batch_world_transforms`
system after `TransformSystems::Propagate` (`fill_batch.rs:1288-1316`). The
`record_key` doubles as the store membership index. Consensus: correctness +
architecture (critical). Adds a required system to the plan.

### Vertex-pulled image material must declare prepass/shadow/deferred shaders
Image draws one quad per record over an inert all-zero mesh, so every pipeline
(main, camera prepass, shadow) needs a custom vertex shader that pulls geometry
from the record buffer, or it rasterizes a degenerate mesh. Declare all entry
points like `SdfExtension` (`fill_batch.rs:807-813`). Correct image-shadow
alpha comes from a **prepass fragment shader that samples the texture alpha and
`discard`s** (as `sdf_panel.wgsl:312-342`), NOT from the alpha-mode helper.
Consensus: correctness + risk + architecture (critical).

### Image material must NOT strip the material bind group
SDF strips `MATERIAL_BIND_GROUP_INDEX` in depth passes because it samples nothing
through it. Image's visible output IS the sampled texture, and its record buffer
lives in that group — stripping deletes both. Keep a populated material group:
texture on the `StandardMaterial` half (`base_color_texture`), `ImageExtension`
binds only the record storage buffer, no stripping logic. Consensus: risk +
architecture (important).

### Replicate the ShaderBuffer growth guard (do not inherit)
The buffer-rebind hazard is guarded in SDF by (a) uploading a fixed-capacity
payload so `set_data` byte length never changes, (b) allocating a NEW buffer on
growth and re-pointing the material bind group, (c) capacity
`record_count().max(1).next_power_of_two()` (never zero). The image store copy
must reproduce all three explicitly. Risk lens (important).

### Image router rebuilds every frame; do not copy the reconcile's `Changed<>` trigger
Model the router on `route_sdf_batch_records` (full rebuild per frame, read
effective `RenderLayers`/`Resolved<ShadowCasting>` from the panel query), not the
change-filtered `reconcile_panel_image_children`. `layers` and `shadow` are in the
key, so a bare layer/shadow change must re-route the record to a new batch key.
Add a test that flips only `RenderLayers`/`Resolved<ShadowCasting>` and asserts
the batch key moves. Risk lens (important).

### Sequencing: no double-draw window; add missing numbered steps
- Land the batch draw path and the child-path removal **atomically** (or gate the
  child path off `draw_batch_family(&kind).is_none()`), else both draw every image
  → doubled alpha and shadow casters between steps 5 and 6. Risk lens (important).
- The 8-step plan is missing explicit steps for: the shared shadow-alpha helper
  extraction (decision #2); the generic `BatchStore` extraction (decision #1,
  one step per family migration); the `batching-diagram.md` update. Split step 4
  (six systems) and step 5 (material type + plugin + specialization + WGSL) into
  finer units. Implementation-quality lens (important).

### Step 6 is a deletion list, not an edit
Once `Image` + `PrecomposeLdr` both route through the batch, the whole entity path
is dead: `PanelImageChild`, `ReusableImageChild`, `ImageVisuals`, `ImageGeometry`,
`reconcile_panel_image_children`, `collect_panel_image_commands`,
`reconcile_existing_image`, `apply_image_shadow_casting`, `build_image_visuals`,
and the three image tests + helpers (`reconcile.rs:1444-1668`). Also fix the
`DiegeticPerfStats::reconcile_ms` reset/accumulate coupling that
`reconcile_panel_text_children` owns (`reconcile.rs:181-184`). Implementation-quality
lens (important).

### Fully-clipped cull must survive
Today an image with an empty `effective_clip` emits no child (`reconcile.rs:686`).
The batch path must emit no `ImageRenderRecord` when the clip is empty, or
fully-scissored images start drawing. Partial image clipping stays unsupported (as
today) unless a `clip_rect` is added to the record. Correctness lens (minor).

### Tint is linear `Vec4`, multiplied post-decode; `alpha_mode` is currently constant
Store `ImageRenderRecord.tint` as linear `Vec4` and multiply after the hardware
sRGB decode of the sampled texture (matching SDF's `linear_color`,
`fill_batch.rs:1940`). Both `Image` and `PrecomposeLdr` are hard-`Blend` today,
so `ImageBatchKey.alpha_mode` is a constant with no authoring source — either drop
it from the key or document images as always-`Blend`. Correctness lens (minor).

### Test coverage gaps (mirror the shipped suites)
Add tests the shipped families pin and the doc omits: buffer growth/rebind keeps
capacity stable and re-points the material; batch entity/buffer survives a
same-key per-record update; batch world-bounds correctness; cross-panel same-texture
sharing places each record at its own panel transform; shadow-casting image with an
alpha hole casts a holed shadow (not a solid quad); bare `RenderLayers`/`ShadowCasting`
change re-keys the record; border-over-image compositing (see decision #4 revision);
precompose visual parity; generic-store before/after parity gating the extraction.
Move the two old-path bullets ("hard-coded layer 0 gone", "reused child updates
`NotShadowCaster`") under Immediate Fix — step 6 deletes the entities they assert
on. Implementation-quality + correctness lenses (important).

### Generic-collapse seams: unifies per-record families, not all four
The `BatchFamily`/`BatchStore<F>` sketch overstates unification. The three shipped
stores diverge on membership granularity (SDF per-record; Path per-run→many-quads;
Shape per-panel), dirty granularity (SDF 2 flags; Path/Shape composite trackers +
atlas), GPU grow policy (SDF one capacity; Path/Shape two + shared atlas upload),
and build `SystemParam`s (SDF/Path/Shape append to the frame material table; image
appends nothing). The generic needs `type Member`, `type Dirty`, and `grow`/`build`
hooks as explicit seams. Image + SDF unify cleanly (both per-record); Path + Shape
are the maximally-similar pair — prove the generic by collapsing Path+Shape first,
then Sdf+Image, expecting two store templates, not one. `uv_rect` forward-compat
covers the atlas route only; bindless still needs a per-record texture index and
removing `texture` from the key. Architecture lens (important, changeability).

### `DrawBatchFamily::Image` variant + routing
Confirm plan step 2: `DrawBatchFamily` has only `SdfSurface`/`PanelShape`/`Text`
(`layout/render.rs:69`) and `draw_batch_family()` returns `None` for
`Image`/`PrecomposeLdr` (`layout/render.rs:143`); both gain the new `Image`
variant and route to it.

## Team Review Corrections (cycle 2, auto-recorded)

Cycle-2 verification sharpened or corrected several cycle-1 items and added new
determined findings.

### Cutover is atomic; the child-path gate creates a NO-DRAW window if landed early
Refutes the cycle-1 "(or gate the child path)" as a standalone alternative. The
gate condition is "child draws only while `draw_batch_family(kind).is_none()`", so
the moment plan step 2 flips `draw_batch_family(Image)=Some`, the child path goes
silent — if the batch path is not yet live, images stop drawing entirely. Build
the store/router/material against `RenderCommandKind::Image`/`PrecomposeLdr`
**directly, without flipping `draw_batch_family`**, then land the flip + the
`collect_panel_image_commands` gate + batch activation as **one atomic commit**.
The router-gate model is `panel_shapes/batching.rs:825` (the only site that
consults `draw_batch_family` today), not the SDF router. Add a regression test:
with the flip on, `collect_panel_image_commands` yields zero `PanelImageChild` and
the store holds exactly one record — proving a single draw source.

### `ImageBatchKey` must OMIT `contiguous_drawn_run`
Do not blindly mirror `SdfBatchKey`. `contiguous_drawn_run` (`fill_batch.rs:392`,
assigned by `assign_contiguous_runs`) is a depth-buffer-regime splitter that
matters only because SDF is frequently Opaque. Images are always Blend and order
entirely via per-record `oit_depth_offset` (which works across batch boundaries),
so copying this field would over-split image batches for no benefit.

### Copy `sort_records` — intra-batch order depends on it (OIT is off by default)
OIT is opt-in (`transparency.rs`: only with a `Camera3d` carrying
`StableTransparency`). With OIT off, records in one batch composite in submission
order, correct only because SDF sorts records by `draw_order_index` before upload
(`sort_records`, `fill_batch.rs:597`). The image store must copy
`sort_records` + `refresh_first_draw_order_index`. Test: two overlapping
same-texture records at different `DrawOrderIndex` composite in draw order with OIT
disabled. (Cross-texture overlapping order is OIT-only and pre-existing — do not
claim the batch path guarantees it OIT-off.)

### Preserve the precompose `entry(...)?` skip; never synthesize a default texture handle
`PrecomposeLdr` today emits no child when the cache entry is absent
(`reconcile.rs:697`). If a record ever fell back to `Handle::<Image>::default()`,
every not-yet-ready precompose across all panels would collide on
`ImageBatchKey.texture == default` and merge into one bogus batch. Keep the
`entry(...)?` skip in the router.

### No-strip is structural, not a fragile invariant
`ImageExtension` always carries a `#[storage]` record-buffer entry, so its
`MATERIAL_BIND_GROUP_INDEX` layout is never empty and Bevy never strips it —
regardless of whether `base_color_texture` is `Some`. State the binding decision
as a structural fact; no runtime `Some(texture)` invariant needs guarding.

### `reconcile_ms` deletion is the safe direction — fix stale comments only
Deleting `reconcile_panel_image_children` leaves text's `reconcile_ms` **assignment**
(`reconcile.rs:322`) as the sole writer — an assignment, not `+=`, so no
accumulate-onto-stale bug. Step-10 sub-tasks: delete the image `mul_add`; leave
text's line as-is; delete/rewrite the three stale cross-referencing comments
(`reconcile.rs:183`, `559`, `mod.rs:108`); decide whether the image route system
re-adds its cost to `reconcile_ms` or accept the metric narrowing (document it).
`record_modified_materials` (`reconcile.rs:1449`) is test-only and goes with the
image test module.

### Growth-guard test precedent is `panel_text/batching.rs:2220`, not panel_shapes
`panel_shapes/batching.rs` has NO buffer-growth test. The
constant-payload-length-across-growth invariant is pinned by
`commit_payloads_keep_a_constant_length_between_growths` in
`panel_text/batching.rs:2220`. (The four cited `fill_batch.rs` tests all exist and
are correctly located.)

### Transform update marks BOTH dirty flags
`update_sdf_batch_world_transforms` marks `record_upload` AND `bounds_update` dirty
(`fill_batch.rs:1312`); the image `update_image_batch_world_transforms` copy must
mark both.

### Generic collapse: SDF+Image is the true nearest pair; expect ~3 templates
Corrects the cycle-1 "prove on Path+Shape first" sequencing. Path (text) and Shape
already **share every type** (`PathBatchKey`, records, `PathExtendedMaterial`, the
dirty trackers — `panel_shapes/batching.rs:60`) yet ship **separate stores** with
unrelated membership APIs (Path per-run `upsert_run`; Shape per-panel
`upsert_panel`) — evidence the store does not unify them, not a warm-up. Realistic
count: **three store templates** — per-record (SDF+Image), per-run (Path), per-panel+atlas
(Shape). Prove the generic on **SDF+Image first**. Additional seams cycle-1 missed:
- **Material-table registration/rebind is opt-in.** SDF/text/Shape register their
  batch material each frame (`register_sdf_batch_materials::<T>`, `fill_batch.rs:1180`)
  and get `extension.material_table` rebound (`material_table.rs:859`). Image has
  no `material_table` binding and must skip both — a seam separate from the
  "appends rows" build hook.
- **System topology differs**, not just SystemParams: append families run before
  `TransformSystems::Propagate`; SDF (and Image) need a separate post-Propagate
  transform system because cross-panel per-record membership forces per-record
  transform lookup; Shape folds one transform at build. Add seams for
  before/after-Propagate gating and transform-update strategy.
- **Atlas is Shape-only** (`PanelShapeBatchStore.atlas`), plus Shape-only per-record
  `outline` — not a shared "Path/Shape" trait, a Shape-specific store extension.
- **`world_bounds` is a bespoke per-family hook**, not covered by `build`.

## Corrected End-to-End Step Order

Replaces the original 8-step Implementation Plan (which had no total order, bundled
steps 4/5, and omitted the two critical corrections and the sequencing fixes).
`[Z-RANK]` = key on `DrawZIndex`+`DrawZIndexRank`; `[XFORM]` = per-record world
transform system.

1. **(shipped)** Immediate Fix in the entity path. Its two old-path assertions
   ("layer 0 gone", "reused child updates `NotShadowCaster`") stay under Immediate
   Fix — step 10 deletes the code they test.
2. Add `ImageBatchKey` `[Z-RANK]` (`texture`, `layers`, `shadow`, `z_index`,
   `z_index_rank`; derive `depth_bias`; OMIT `contiguous_drawn_run` and, if chosen,
   `alpha_mode`). Add CPU `ResolvedImageRecord` (`record_key {panel, command_index}`,
   bounds, linear-`Vec4` tint, uv_rect, draw_depth, `transform: Mat4`) and GPU
   `ImageRenderRecord: ShaderType`.
3. Add `ImageBatchStore` from `SdfBatchStore` — HashMap-by-key + `record_index` +
   upsert/remove/retain/take-empty + `sort_records`. Replicate all three growth
   guards explicitly.
4. Add `route_image_batch_records` (full rebuild per frame, read effective
   layers/shadow from the panel query; filter model `panel_shapes:825`; preserve
   the empty-clip cull and the precompose `entry(...)?` skip). Does NOT flip
   `draw_batch_family`.
5. Add `reconcile_image_batch_entities` + GPU buffer grow.
6. `[XFORM]` Add `update_image_batch_world_transforms` after `TransformSystems::Propagate`
   (mark both dirty flags), then `update_image_batch_bounds`.
7. Add `ImageExtendedMaterial`/`ImageExtension` + WGSL, split: (7a) material type +
   plugin registration; (7b) specialization declaring main/prepass/shadow vertex
   entry points + prepass fragment that samples texture alpha and `discard`s, no
   material-group strip; (7c) WGSL vertex-pull + `uv_rect` sample + post-decode
   linear-tint multiply + `ClipDepthNudge`/`OitDepthOffset`.
8. Route precompose: handle → key texture, `Color::WHITE` tint; keep existing
   `Bgra8UnormSrgb` target (PD-2).
9. **Atomic cutover:** flip `draw_batch_family(Image/PrecomposeLdr)=Some(Image)` +
   add the `collect_panel_image_commands` gate + activate — one commit.
10. **Deletion:** remove the dead entity path (types/systems/tests listed above) +
    the image `reconcile_ms` accumulate + stale comments.
11. **Border-over-image ordering (PD-3, in scope).** Route the biting/clipping
    border into the transparent phase (or a concrete in-front depth-test offset)
    so its `oit_depth_offset`/screen bias — driven from the same
    `ClipDepthNudge`/draw-order machinery as the image record — composites it over
    the coplanar `Blend` image. Scope the phase change to the biting border only;
    leave the normal border's opaque-push behavior unchanged. Add a before/after
    regression test (border pixels over image pixels in the overlap region); this
    test fails on `main` and becomes a fixed-behavior gate here.
12. Update `batching-diagram.md`.
13. **Later phase (out of image scope):** the SDF+text shadow-alpha rule decision
    (PD-1) and the generic `BatchStore<F>` — SDF+Image first, expecting ~3
    templates, one migration per step, each gated by a before/after parity test.

## Proposed User Decisions

These three revise decisions you already resolved (#2/#3/#4). Surfaced for your
approval rather than changed silently. Status: `proposed`.

### PD-1 (revises Decision #2) — shared shadow-alpha helper does not belong in the image project
SDF and text do NOT apply the same rule. SDF gates on shadow (`(Opaque, Cast) ->
Mask(0.0)`, `fill_batch.rs:906`); text remaps `Opaque -> Mask(0.0)`
**unconditionally** (`batching.rs:1169`), because opaque text loses its material
bind group in the camera depth/normal prepass, not just the shadow pass.

Cycle-2 sharpening (correctness + risk lenses):
- Images are hard-`Blend` (`reconcile.rs:815`); the `Opaque -> …` arm never runs,
  so the helper delivers **zero behavioral value to images** — its only effect in
  this project would be refactoring two *other* families.
- Option (a) "parameterize on needs-bind-group-in-prepass" is not cleanly
  implementable: prepass survival is a per-camera *runtime* property (a downstream
  `Camera3d` with TAA/SSAO/deferred enables one; the crate configures none today).
  A correct unified helper would have to adopt text's **unconditional** remap,
  which changes SDF's non-casting-opaque behavior and needs its own validation.

Recommendation: **remove the shared shadow-alpha helper from the image project**
(inert for always-Blend images). Severity: important.

**RESOLVED (user):** helper removed from the image project; keep SDF's
shadow-gated rule and text's unconditional rule as two distinct rules for now — do
not force SDF onto text's unconditional remap. **The SDF/text shadow-alpha
divergence is deferred to the generics/traits phase and MUST be addressed there**
(see "Collapsing Duplication" → Sequencing): decide, against all four real
families, whether the two rules unify behind one trait hook or stay deliberately
distinct with a documented reason. Not a blocker for image batching.

### PD-2 (revises Decision #3) — RESOLVED: precompose is already sRGB; no format change
Status: `resolved` (auto-recorded — no user choice remains). The shipped target is
already `TextureFormat::Bgra8UnormSrgb` with a linear sampler (`precompose.rs:417`),
sampled through stock PBR `base_color_texture` (hardware sRGB decode). The
round-trip decision #3 wants already holds; switching to `Rgba8UnormSrgb` is
gratuitous AND swaps channel order away from the swapchain-preferred format (a
potential regression). Decision: **keep `Bgra8UnormSrgb`, sample via
`base_color_texture`, no hand-rolled gamma; no precompose change in this project.**
Confirmed by all four lenses across both cycles.

### PD-3 (revises Decision #4) — image+border ordering is a pre-existing gap, not a batching concern
A `Blend` image renders in the transparent/OIT pass, ordered by `oit_depth_offset`,
not `clip_depth_nudge`; and an opaque border is pushed AWAY from the camera by
`OPAQUE_FILL_DEPTH_PUSH_LAYERS` (`fill_batch.rs:89`) so coplanar text wins — which
puts it behind a coplanar transparent image, the opposite of the requirement. So
"no new machinery, border just wins via ClipDepthNudge" is wrong.

Cycle-2 sharpening (correctness lens): the current child-entity image already
renders `Blend` at `TEXT_Z_OFFSET` and never touches `clip_depth_nudge`
(`reconcile.rs:815-823`); the border is the same pushed-away opaque SDF today. So
**border-over-image already fails on `main`** — the batch migration inherits the
behavior, it does not regress it. The real fix is a phase/depth-write change (draw
the biting border in the transparent phase, or give it a concrete depth-test
offset), not `ClipDepthNudge`.

Recommendation: **preserve today's behavior in the batch project** (Blend image,
per-record `oit_depth_offset`/screen bias) and file border-over-image as a separate
correctness ticket; do not gate the batch work on a test that fails on `main`.
Severity: important.

**RESOLVED (user): expand scope now.** Fix border-over-image as part of this
project rather than deferring. A border that bites into image size must render
**on top of** the coplanar image. Because the image is `Blend` (transparent/OIT
pass) and the biting border is opaque SDF pushed AWAY from the camera by
`OPAQUE_FILL_DEPTH_PUSH_LAYERS` (`fill_batch.rs:89`), `ClipDepthNudge` alone cannot
achieve this. The fix is a phase/depth change to the biting border:
- **Approach:** route the biting border into the transparent phase alongside the
  image (or give it a concrete depth-test offset in front of the image), so their
  order is resolved by `oit_depth_offset`/screen bias rather than by the
  opaque-push that currently sinks it behind.
- **Ordering:** the border's `oit_depth_offset` (or screen depth bias) must place
  it in front of the image at equal world depth; drive it from the same
  `ClipDepthNudge`/draw-order machinery the image records use, so a border that
  clips into the image's footprint composites over it.
- **Test:** add a before/after parity/regression test — image + biting border,
  assert border pixels composite over image pixels in the overlap region. This
  test currently fails on `main`; it becomes a fixed-behavior gate for this
  project (not an inherited-failure gate).
- **Watch:** this touches the opaque-vs-transparent split for borders; verify no
  regression to the normal (non-biting) border case, which should keep its current
  opaque-push behavior. Scope the phase change to the biting/clipping border only.
