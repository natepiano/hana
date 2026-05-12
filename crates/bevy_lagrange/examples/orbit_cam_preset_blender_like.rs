//! Demonstrates the editor-oriented `OrbitCamPreset::BlenderLike` input mode.

mod common;

use bevy_lagrange::OrbitCamPreset;
use fairy_dust::CameraGuidance;

fn main() {
    fairy_dust::sprinkle_example()
        .with_orbit_cam_bundle(
            common::configure_camera,
            (
                OrbitCamPreset::BlenderLike,
                CameraGuidance::for_preset(OrbitCamPreset::BlenderLike),
            ),
        )
        .with_camera_guidance_panel()
        .add_systems(bevy::prelude::Startup, common::spawn_scene)
        .run();
}
