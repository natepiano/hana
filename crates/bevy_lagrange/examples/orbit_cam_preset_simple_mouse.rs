//! Demonstrates the mouse-oriented `OrbitCamPreset::SimpleMouse` input mode.

mod common;

use bevy_lagrange::OrbitCamPreset;
use fairy_dust::CameraGuidance;

fn main() {
    fairy_dust::sprinkle_example()
        .with_orbit_cam_bundle(
            common::configure_camera,
            (OrbitCamPreset::SimpleMouse, CameraGuidance::auto()),
        )
        .with_camera_guidance_panel()
        .add_systems(bevy::prelude::Startup, common::spawn_scene)
        .run();
}
