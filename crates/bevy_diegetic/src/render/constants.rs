//! Rendering constants for diegetic panels.

use bevy::asset::uuid_handle;
use bevy::prelude::*;

// Draw-order projection
/// Screen-sort position reserved for the shared SDF surface batch within each
/// authored z-index band.
pub(crate) const SDF_SURFACE_BATCH_SORT_ANCHOR: i32 = 0;
/// First command sort position inside each authored z-index band.
pub(crate) const FIRST_COMMAND_SORT_OFFSET: i32 = SDF_SURFACE_BATCH_SORT_ANCHOR + 1;
/// Number of command sort positions reserved inside each authored z-index band.
pub(crate) const COMMAND_SORT_OFFSET_CAPACITY: i32 = 64;
/// Screen-sort position reserved for the shared panel-shape batch within each
/// authored z-index band.
pub(crate) const PANEL_SHAPE_BATCH_SORT_ANCHOR: i32 =
    FIRST_COMMAND_SORT_OFFSET + COMMAND_SORT_OFFSET_CAPACITY - 1;
/// Screen-sort position reserved for the shared text batch within each authored
/// z-index band.
pub(crate) const TEXT_BATCH_SORT_ANCHOR: i32 = PANEL_SHAPE_BATCH_SORT_ANCHOR + 1;
/// Width of one authored z-index band in screen-sort positions.
pub(crate) const DRAW_Z_INDEX_BAND_WIDTH: i32 = TEXT_BATCH_SORT_ANCHOR + 1;
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
