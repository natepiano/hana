# Tangent-Bitangent-Normal For Analytic Materials

## Purpose

This document records the deferred design for tangent-bitangent-normal
(`TBN`) support on diegetic analytic surfaces: SDF fills/borders, slug text,
and panel shapes. It replaces the old tangent-basis appendix in
`as-built/material-table-batching.md`.

The feature is not part of the active batching plan. Build it only after the
main SDF/text/shape material-table path is stable and there is a concrete
material need for normal maps or parallax/relief mapping on analytic surfaces.

TBN is a material and lighting feature. It is not expected to fix glyph edge
shimmer, small-text antialiasing, HDR text degradation, or coverage instability
while text moves across screen pixels. Those belong to the analytic coverage
and presentation path.

## What TBN Enables

`TBN` is the local surface frame used by tangent-space material maps:

- `T` is the surface tangent, normally the direction of increasing material U.
- `B` is the bitangent, normally the direction of increasing material V.
- `N` is the world-space surface normal.

For diegetic analytic surfaces, synthesizing this frame enables:

- `normal_map_texture`: sample tangent-space normals and perturb the surface
  normal before `apply_pbr_lighting`.
- `depth_map`: run parallax or relief mapping by offsetting material UVs before
  sampling the other texture channels.
- future material-driven bevel, engraving, and stamped-surface looks where a
  texture affects lighting instead of changing glyph or shape geometry.

## Current State

Text and shapes deliberately strip tangent-dependent maps today:

- Text: `strip_tangent_dependent_maps` in
  `crates/bevy_diegetic/src/render/panel_text/batching.rs` clears
  `normal_map_texture` and `depth_map`.
- Shapes mirror that behavior in
  `crates/bevy_diegetic/src/render/panel_shapes/batching.rs`.

The strip is correct until this document is implemented. Without a TBN, those
maps would sample with an undefined tangent basis and produce wrong lighting.

SDF fills route through their own batched SDF shader path and need the same
capability if fills/borders should support normal/depth maps. Do not implement
TBN for text/shapes only and then claim full `StandardMaterial` parity across
all diegetic render families.

## Existing Shader Path

The shared analytic shader path is:

- `crates/bevy_diegetic/src/render/analytic_paths/analytic_path.wgsl`
- `crates/bevy_diegetic/src/shaders/sdf_material_table.wgsl`
- `crates/bevy_diegetic/src/render/material_table.wgsl`

`pbr_input_from_material_table` currently:

1. Reads the record's `MaterialSlotValues`.
2. Computes sampled UVs with `compute_material_sampled_uv(box_uv,
   uv_transform)`.
3. Calls Bevy's `pbr_input_from_standard_material` with those UVs.
4. Applies scalar/vector table values back onto the sampled `PbrInput`.

Bevy's stock normal/parallax path is gated behind `VERTEX_TANGENTS`. It expects
`VertexOutput.world_tangent` to exist and uses helpers like
`calculate_tbn_mikktspace`, `apply_normal_mapping`, and `parallaxed_uv`.
Diegetic vertex-pulled batch records do not carry mesh tangents, so the stock
mesh path cannot be used as-is. We need a repo-owned synthesized TBN path.

## Design

The surfaces are planar, so the TBN frame is straightforward:

- `N`: panel face normal in world space, corrected for front/back rendering and
  double-sided material semantics.
- `T`: panel-local +X transformed to world space, aligned with increasing
  material U after the element's box UV and any horizontal box-UV flip.
- `B`: panel-local -Y or +Y transformed to world space, aligned with increasing
  material V. The sign must match the existing box-UV convention: `(0, 0)` is
  top-left and `(1, 1)` is bottom-right.

The key design constraint is consistency with material UVs. If reverse text or
box-UV flipping changes the material U direction, the tangent handedness must
change with it. If `uv_transform` rotates or mirrors material UVs, the
normal-map frame should either incorporate that transform's linear part or
explicitly document the first implementation's limits. The preferred
implementation applies the 2x2 UV-transform basis to `T`/`B` so rotated material
textures and normal maps stay aligned.

## Material Classification

The Phase 2 classification is decided, but some fields are physically deferred
until this feature lands:

- `parallax_depth_scale`: table value.
- `max_parallax_layer_count`: table value.
- `max_relief_mapping_search_steps`: table value.
- `parallax_mapping_method`: `PipelineCompatibility` split, because it selects
  the shader method.
- `depth_map`: `ResourceCompatibility` texture splitter. The texture is already
  exposed by the `StandardMaterial` half of the batch material; batching across
  many distinct height maps is the texture-array problem in Appendix B of
  `as-built/material-table-batching.md`.
- `normal_map_texture`, `normal_map_channel`, and `flip_normal_map_y` remain
  resource/compatibility facts, not table values.

When this feature lands, add the parallax scalar fields to Rust
`MaterialSlotValues`, the WGSL mirror, projection tests, and the field-approval
destructure. Until then, dropping those scalars is correct because no diegetic
shader reads them.

## Implementation Outline

1. Add a small shared WGSL helper module for synthesized planar TBN.
   It should take world normal, world tangent, world bitangent or enough record
   data to derive them, plus material flags and `is_front`.

2. Extend the record data only if the shader cannot derive the basis from
   existing transforms:
   - `PathRenderRecord` has the record `transform`, and `PathQuadRecord` has
     box-UV data and flip state.
   - `SdfRenderRecord` has the surface `transform`.
   - Prefer deriving `T`, `B`, and `N` from those transforms in WGSL before
     adding per-record storage fields.

3. Update `pbr_input_from_material_table` or add a sibling helper that can:
   - compute final material UV from `box_uv` and `uv_transform`;
   - optionally parallax-displace that UV before texture sampling;
   - sample normal maps with the synthesized TBN;
   - then apply scalar/vector table values exactly as today.

4. Remove `normal_map_texture` and `depth_map` stripping only after the shader
   path works for the relevant render family. Text and shapes should stop
   stripping together because they share the analytic path shader. SDF stripping
   or support must be decided separately.

5. Add `parallax_mapping_method` to pipeline compatibility and wire relief vs.
   occlusion method selection. Port Bevy's `parallax_mapping.wgsl` behavior
   rather than inventing a new algorithm.

6. Keep texture handles out of the material table. Normal and depth map handles
   remain resource compatibility fields copied into the `StandardMaterial` half
   of the batch material by `apply_resource_compatibility_to_standard_material`.

## Validation

Add tests and runtime examples that prove:

- Text and panel shapes no longer strip `normal_map_texture` / `depth_map` only
  after TBN support is active.
- A flat normal map produces the same lighting as no normal map.
- A visible normal map changes lighting consistently as the panel moves and
  rotates.
- Reversed or horizontally flipped text keeps normal-map orientation aligned
  with the visible material texture.
- `uv_transform` rotation/scale is honored by both base-color texture sampling
  and normal/depth sampling.
- Two records that differ only in parallax scalar values share a batch and read
  different material table rows.
- Two records that differ by `parallax_mapping_method`, texture handles, normal
  map channel, or flip-normal-map-y split as compatibility requires.
- SDF fill, text, and shape behavior are each covered if the feature claims
  support for all three render families.

Runtime validation should include a small panel with:

- one text run with a normal map;
- one panel shape with the same normal map;
- one SDF fill with the same normal map if SDF support is included;
- a rotated `uv_transform`;
- a grazing light angle so orientation mistakes are obvious.

## Risks

- Bevy's stock path is `VERTEX_TANGENTS`-driven. Calling into it without real
  `world_tangent` data is not enough; this feature needs an explicit synthesized
  TBN path.
- `uv_transform` can rotate or mirror UVs. If TBN ignores that transform, normal
  maps will visually rotate away from base-color textures.
- Reverse text and box-UV flips change handedness. Normal maps must follow the
  material UVs, not the unflipped geometry.
- Parallax changes the sampled UV before all other texture reads. Applying it
  after base-color or normal sampling is wrong.
- This does not reduce texture batch splitting. Many distinct normal/depth maps
  still need the texture-array extension from Appendix B of
  `as-built/material-table-batching.md`.

## Non-Goals

- Do not use this work to tune text antialiasing, HDR compensation, or coverage
  shimmer.
- Do not introduce a second text rendering mechanism.
- Do not move texture handles into `MaterialSlotValues`.
- Do not implement a durable material table as part of this feature.
- Do not add per-record TBN storage unless deriving it from existing transforms
  is insufficient or measurably too expensive.
