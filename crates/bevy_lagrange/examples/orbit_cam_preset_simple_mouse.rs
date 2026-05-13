//! Demonstrates the mouse-oriented `OrbitCamPreset::SimpleMouse` input mode.

mod common;

use bevy_lagrange::OrbitCamPreset;

fn main() {
    fairy_dust::sprinkle_example()
        .with_orbit_cam_bundle(common::configure_camera, OrbitCamPreset::SimpleMouse)
        .with_camera_control_panel()
        .add_systems(bevy::prelude::Startup, common::spawn_scene)
        .run();
}
