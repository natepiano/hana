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
  path; those are material-table features for their own render families.
