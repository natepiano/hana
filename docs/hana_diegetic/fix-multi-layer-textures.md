# Fix — classify the multi-layer material texture fields

> **Status: TODO — not started.** Small, mechanical. Discovered 2026-07-03 when
> hana's workspace broke against the `48d11c8` pin.

## Problem

`batch_key.rs` destructures `StandardMaterial` exhaustively on purpose — the
field-approval gate (`render/batch_key.rs:203` and `:300`, "Do not add `..`")
turns any new Bevy field into an E0027 so it gets an explicit classification.

But some `StandardMaterial` fields are feature-gated, and Cargo feature
unification means a *downstream* workspace can switch them on without
hana_diegetic opting in. Any consumer that enables one of these bevy features
breaks hana_diegetic's build:

- `pbr_multi_layer_material_textures` → `clearcoat_channel`,
  `clearcoat_texture`, `clearcoat_roughness_channel`,
  `clearcoat_roughness_texture`, `clearcoat_normal_channel`,
  `clearcoat_normal_texture`
- `pbr_anisotropy_texture` → `anisotropy_channel`, `anisotropy_texture`
- `pbr_specular_textures` → specular / specular-tint texture and channel fields

This happened in practice: hana's workspace carried an unused
`pbr_multi_layer_material_textures` flag and the two `From<&StandardMaterial>`
impls failed with E0027 on the six clearcoat fields. Immediate remedy was
removing the flag from hana, which only works because nothing there uses it —
the underlying exposure is still open for every downstream workspace.

## Fix

1. Enable the three features on hana_diegetic's own bevy dependency
   (`crates/hana_diegetic/Cargo.toml`), so the gated fields exist
   unconditionally for this crate and no downstream flag changes the field set
   it compiles against.
2. Classify the new fields in both destructures (`PipelineCompatibility` and
   `ResourceCompatibility` in `render/batch_key.rs`). Recommended:
   **unsupported** (`_..._unsupported`) — panels are overwhelmingly unlit, and
   unlit materials skip clearcoat/anisotropy/specular entirely, so there is no
   consumer for these as batch-splitting resources. Reclassify as `resource`
   (capture into `ResourceCompatibility`) only if glossy textured panel
   materials ever become a real use.

Scalar clearcoat (`clearcoat`, `clearcoat_perceptual_roughness`) and scalar
anisotropy are already classified as `table` fields — this note is only about
the feature-gated texture/channel tier.

## Cost

Forcing the features on means every hana_diegetic consumer compiles bevy's
multi-layer/anisotropy/specular texture support. That is field + shader-def
plumbing, not per-frame work — the shader paths only activate when a material
binds the textures.
