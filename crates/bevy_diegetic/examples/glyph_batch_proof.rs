//! Glyph-instancing Step-1 proof (`docs/bevy_diegetic/glyph_instancing.md`).
//!
//! One ordinary world label renders through the per-run path and seeds the
//! shared glyph atlas; the crate's `batch_proof` scaffolding then spawns one
//! hand-built batch entity beside it whose glyphs are expanded entirely by
//! the vertex-pulling shader from hand-written records. The two stacked
//! copies under the source label must match it glyph for glyph.
//!
//! Shortcuts:
//! - `G` — force a capacity growth (same-frame mesh swap; captures frames N / N+1 / N+2 to
//!   `/private/tmp/glyph_batch_proof/`)
//! - `I` — toggle the glyph-index debug staircase
//! - `O` — toggle OIT on the orbit camera
//! - `L` — swing the directional light around the scene

use bevy::core_pipeline::oit::OrderIndependentTransparencySettings;
use bevy::prelude::*;
use bevy_diegetic::DiegeticText;
use bevy_diegetic::GlyphBatchProofPlugin;
use bevy_diegetic::force_capacity_growth;
use bevy_diegetic::toggle_debug_index;
use bevy_lagrange::OrbitCam;
use bevy_lagrange::OrbitCamPreset;
use fairy_dust::CameraHomeTarget;

const PROOF_TEXT: &str = "Glyph Pull";
const PROOF_TEXT_SIZE: f32 = 0.3;
const SOURCE_Y: f32 = 1.7;
const DISPLAY_Z: f32 = 2.0;
const SOURCE_COLOR: Color = Color::srgba(1.0, 0.38, 0.20, 1.0);
const GROUND_SIZE: f32 = 5.4;
const GROUND_COLOR: Color = Color::srgb(0.08, 0.08, 0.08);
const HOME_PITCH: f32 = 0.055;
const HOME_YAW: f32 = 0.0;
const LIGHT_AIM: Vec3 = Vec3::new(0.0, 1.0, DISPLAY_Z);
const KEY_LIGHT_POS: Vec3 = Vec3::new(0.0, 5.0, DISPLAY_Z + 12.0);
const LIGHT_SWING_RADIANS: f32 = 0.5;

fn main() {
    fairy_dust::sprinkle_example()
        .with_brp_extras()
        .with_save_window_position()
        .with_studio_lighting()
        .aim_at(LIGHT_AIM)
        .key_light_pos(KEY_LIGHT_POS)
        .with_ground_plane()
        .size(GROUND_SIZE)
        .color(GROUND_COLOR)
        .with_orbit_cam_preset(|_| {}, OrbitCamPreset::BlenderLike)
        .with_stable_transparency()
        .with_camera_home()
        .pitch(HOME_PITCH)
        .yaw(HOME_YAW)
        .add_plugins(GlyphBatchProofPlugin)
        .add_systems(Startup, setup)
        .with_shortcut(KeyCode::KeyG, force_capacity_growth)
        .with_shortcut(KeyCode::KeyI, toggle_debug_index)
        .with_shortcut(KeyCode::KeyO, toggle_oit)
        .with_shortcut(KeyCode::KeyL, swing_light)
        .run();
}

/// Spawns the source label: it renders through the per-run path (the
/// coexistence half of the proof) and its prepared run + atlas records feed
/// the hand-built batch.
fn setup(mut commands: Commands) {
    commands.spawn((
        Name::new("Source label"),
        CameraHomeTarget,
        DiegeticText::world(PROOF_TEXT)
            .size(PROOF_TEXT_SIZE)
            .color(SOURCE_COLOR)
            .transform(Transform::from_xyz(0.0, SOURCE_Y, DISPLAY_Z))
            .build(),
    ));
}

/// `O`: toggles OIT on the orbit camera, re-specializing the text pipelines
/// both ways.
fn toggle_oit(
    cameras: Query<(Entity, Has<OrderIndependentTransparencySettings>), With<OrbitCam>>,
    mut commands: Commands,
) {
    for (camera, oit_on) in &cameras {
        if oit_on {
            commands
                .entity(camera)
                .remove::<OrderIndependentTransparencySettings>();
            info!("glyph batch proof: OIT off");
        } else {
            commands
                .entity(camera)
                .insert(OrderIndependentTransparencySettings::default());
            info!("glyph batch proof: OIT on");
        }
    }
}

/// `L`: swings every directional light around the aim point so shading
/// response to a moved light is visible on the batch glyphs.
fn swing_light(mut lights: Query<&mut Transform, With<DirectionalLight>>) {
    for mut transform in &mut lights {
        transform.rotate_around(LIGHT_AIM, Quat::from_rotation_y(LIGHT_SWING_RADIANS));
    }
}
