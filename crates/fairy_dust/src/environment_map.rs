//! Capability: image-based lighting from a bundled environment map.
//!
//! Inserts an [`EnvironmentMapLight`] on the orbit camera. Both the panel
//! shader and the text/analytic shader call `apply_pbr_lighting`, which samples
//! the bound environment map, so panel backgrounds and glyphs gain diffuse
//! ambient fill plus specular reflection from the `pisa` cathedral HDRI.
//!
//! Specular reflection only reads as a sharp glint on a metallic, low-roughness
//! surface; the default panel material is a rough dielectric, so the dominant
//! effect here is the diffuse ambient term lifting the scene.
//!
//! The cubemaps are embedded (not loaded from a runtime `assets/` dir) so the
//! capability resolves from any example crate regardless of its asset root.
//!
//! Gated behind the `SprinkleBuilder<WithOrbitCam>` typestate — see
//! [`crate::SprinkleBuilder::with_environment_map`].

use bevy::asset::embedded_asset;
use bevy::prelude::*;

use crate::orbit_cam::FairyDustOrbitCam;

const DIFFUSE_MAP: &str = "embedded://fairy_dust/environment_maps/pisa_diffuse_rgb9e5_zstd.ktx2";
const SPECULAR_MAP: &str = "embedded://fairy_dust/environment_maps/pisa_specular_rgb9e5_zstd.ktx2";

/// Scales the environment map's contribution to lighting. Matches the value
/// used by the `hana` editor camera.
const ENV_LIGHT_INTENSITY: f32 = 2000.0;

pub(crate) fn install(app: &mut App) {
    embedded_asset!(app, "environment_maps/pisa_diffuse_rgb9e5_zstd.ktx2");
    embedded_asset!(app, "environment_maps/pisa_specular_rgb9e5_zstd.ktx2");
    app.add_observer(insert_environment_map);
}

fn insert_environment_map(
    trigger: On<Add, FairyDustOrbitCam>,
    asset_server: Res<AssetServer>,
    mut commands: Commands,
) {
    commands.entity(trigger.entity).insert(EnvironmentMapLight {
        diffuse_map: asset_server.load(DIFFUSE_MAP),
        specular_map: asset_server.load(SPECULAR_MAP),
        intensity: ENV_LIGHT_INTENSITY,
        ..default()
    });
}
