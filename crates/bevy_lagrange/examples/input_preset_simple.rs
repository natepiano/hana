//! Demonstrates the mouse-oriented `OrbitCamPreset::SimpleMouse` input mode.

mod scene_setup;

use bevy_lagrange::OrbitCamInputMode;
use bevy_lagrange::OrbitCamPreset;

fn main() {
    fairy_dust::sprinkle_example()
        .with_orbit_cam(
            scene_setup::configure_camera,
            OrbitCamInputMode::Preset(OrbitCamPreset::SimpleMouse),
        )
        .with_camera_control_panel()
        .add_systems(bevy::prelude::Startup, scene_setup::spawn_scene)
        .run();
}
