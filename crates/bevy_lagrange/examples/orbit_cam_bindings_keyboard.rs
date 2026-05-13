//! Demonstrates keyboard user input through `OrbitCamBindings`.

mod common;

use bevy::prelude::*;
use bevy_lagrange::CameraInputRoutingConfig;
use bevy_lagrange::NoPositionFallback;
use bevy_lagrange::OrbitCamBindings;
use bevy_lagrange::OrbitCamBindingsError;
use bevy_lagrange::OrbitCamInputBinding;

fn keyboard_bindings() -> Result<OrbitCamBindings, OrbitCamBindingsError> {
    let orbit_keys = OrbitCamInputBinding::cardinal_keys(
        KeyCode::ArrowUp,
        KeyCode::ArrowRight,
        KeyCode::ArrowDown,
        KeyCode::ArrowLeft,
    );
    let pan_keys = OrbitCamInputBinding::cardinal_keys(
        KeyCode::KeyW,
        KeyCode::KeyD,
        KeyCode::KeyS,
        KeyCode::KeyA,
    );
    let zoom_keys = OrbitCamInputBinding::bidirectional_keys(KeyCode::Equal, KeyCode::Minus);

    OrbitCamBindings::builder()
        .orbit(orbit_keys)
        .pan(pan_keys)
        .zoom(zoom_keys)
        .build()
}

fn main() {
    let Ok(bindings) = keyboard_bindings() else {
        error!("keyboard camera bindings failed to validate");
        return;
    };

    fairy_dust::sprinkle_example()
        .insert_resource(
            CameraInputRoutingConfig::cursor_hit_test()
                .with_no_position_fallback(NoPositionFallback::OnlyEligibleCamera),
        )
        .with_orbit_cam_bundle(common::configure_camera, bindings)
        .with_camera_control_panel()
        .add_systems(Startup, common::spawn_scene)
        .run();
}
