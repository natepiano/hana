//! Rendering constants for diegetic panels.

use bevy::asset::uuid_handle;
use bevy::prelude::*;

// layer ordering
/// Screen sort lane used by the batched SDF fill. Lower sort ordinals draw
/// first and stay behind later lanes: fill backgrounds use lane 0,
/// per-command geometry uses lanes 1..=64, and text uses lane 65.
pub(crate) const DRAW_LEVEL_FILL_SUBLANE: i32 = 0;
/// Number of per-command geometry screen sort lanes inside one z-level band.
pub(crate) const DRAW_LEVEL_GEOMETRY_LANES: i32 = 64;
/// First screen sort lane available to panel-owned geometry commands.
pub(crate) const DRAW_LEVEL_GEOMETRY_START_SUBLANE: i32 = DRAW_LEVEL_FILL_SUBLANE + 1;
/// Number of screen sort lanes reserved for each z-level.
pub(crate) const DRAW_LEVEL_STRIDE: i32 = DRAW_LEVEL_TEXT_SUBLANE + 1;
/// Screen sort lane used by batched text inside each z-level band.
pub(crate) const DRAW_LEVEL_TEXT_SUBLANE: i32 =
    DRAW_LEVEL_GEOMETRY_START_SUBLANE + DRAW_LEVEL_GEOMETRY_LANES;
/// Per-command depth bias for Geometry mode sort ordering.
///
/// Bevy packs this through `i32` into `DepthBiasState.constant`.
/// Controls the `Transparent3d` sort key so back-to-front submission
/// order matches the painter's order. Also wins the depth test for
/// coplanar fragments.
pub(crate) const LAYER_DEPTH_BIAS: f32 = 1.0;
/// Per-command OIT depth offset for coplanar fragment ordering.
///
/// Added to `position.z` in the fragment shader before `oit_draw`
/// stores the fragment. Pipeline `depth_bias` does NOT affect
/// `in.position.z`, so we apply this offset manually.
/// Reverse-Z: positive = closer to camera = composited in front.
///
/// Calibration: `bevy_lagrange` syncs the perspective near plane to
/// `radius × 0.001`, so a fragment at the camera's focus distance has
/// `position.z = near / d ≈ 0.001`. A 64-ordinal offset span must stay well
/// below that or the offset drives `position.z` non-positive and
/// `pack_24bit_depth_8bit_alpha` in `oit_draw` saturates it to the
/// cleared-background depth, where bevy's resolve pass drops every
/// fragment whose alpha < 1.0. At `1e-6`, 64 steps total `6.4e-5`
/// (6.4% of the focus depth) and one step spans ~17 quanta of the
/// 24-bit OIT depth packing, so adjacent ordinals stay distinct.
///
/// Panels much farther than the camera focus shrink `position.z` below
/// the 64-step budget (z = near/d crosses `6.4e-5` at ~15.6× the orbit
/// radius). The `OIT_MIN_DEPTH` floor in `sdf_panel.wgsl` and
/// `analytic_path.wgsl` keeps those fragments
/// storable; past the bound their coplanar ordering collapses to OIT-list
/// insertion order instead of going invisible.
pub(crate) const OIT_DEPTH_STEP: f32 = 0.000_001;
/// Nominal OIT `position.z` for fragments at the camera focus distance.
pub(crate) const OIT_FOCUS_DEPTH: f32 = 0.001;

// material defaults
/// Default metallic value for panel surfaces. Non-metallic (dielectric).
pub(super) const DEFAULT_METALLIC: f32 = 0.0;
/// Default reflectance for panel surfaces. Very low specular to avoid
/// washing out colors under bright lights.
pub(super) const DEFAULT_REFLECTANCE: f32 = 0.02;
/// Default roughness for panel surfaces. Matte paper-like appearance.
pub(super) const DEFAULT_ROUGHNESS: f32 = 0.95;

// sdf rendering
/// World-space padding added to each SDF quad mesh beyond the SDF boundary.
/// Gives the exterior anti-aliasing ramp room to render — without this, the
/// mesh edge coincides with the SDF boundary and the AA fade-out is clipped.
pub(crate) const SDF_AA_PADDING: f32 = 0.001;
/// `SdfPanelUniform::sdf_kind` value for rounded-rectangle panel surfaces.
pub(crate) const SDF_KIND_ROUNDED_RECT: u32 = 0;
/// `SdfPanelUniform::sdf_params` value for rounded-rectangle panel surfaces.
pub(crate) const SDF_ROUNDED_RECT_PARAMS: Vec4 = Vec4::ZERO;
/// Internal-asset handle for the `sdf_stroke.wgsl` shader.
pub(super) const SDF_STROKE_SHADER_HANDLE: Handle<Shader> =
    uuid_handle!("536f3741-5418-4d7a-a0b2-8cfb1d30e8a1");

// text rendering
/// Fixed panel-local Z for text and image meshes.
///
/// Layering is handled by `StandardMaterial::depth_bias`, so panel-local
/// geometry stays coplanar.
pub(super) const TEXT_Z_OFFSET: f32 = 0.0;
/// Default clip rect for unclipped text: effectively infinite panel-local
/// bounds so the shader clip test becomes a no-op.
pub(super) const UNCLIPPED_TEXT_CLIP_RECT: Vec4 = Vec4::new(-1e6, -1e6, 1e6, 1e6);
