//! Demonstrates keyboard user input through `OrbitCamBindings`.

mod common;

use bevy::prelude::*;
use bevy_lagrange::CameraInputRoutingConfig;
use bevy_lagrange::CameraInteractionSources;
use bevy_lagrange::NoPositionFallback;
use bevy_lagrange::OrbitCamBindings;
use bevy_lagrange::OrbitCamBindingsError;
use bevy_lagrange::OrbitCamInputBinding;
use bevy_lagrange::OrbitCamInteractionKind;
use fairy_dust::CameraGuidance;
use fairy_dust::CameraGuidanceRow;

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

fn keyboard_guidance() -> CameraGuidance {
    CameraGuidance::custom([
        CameraGuidanceRow::new(OrbitCamInteractionKind::Orbit, "Arrows -> Orbit")
            .when_sources(CameraInteractionSources::KEYBOARD),
        CameraGuidanceRow::new(OrbitCamInteractionKind::Pan, "WASD -> Pan")
            .when_sources(CameraInteractionSources::KEYBOARD),
        CameraGuidanceRow::new(OrbitCamInteractionKind::Zoom, "+ / - -> Zoom")
            .when_sources(CameraInteractionSources::KEYBOARD),
    ])
    .with_title("Keyboard Bindings")
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
        .with_orbit_cam_bundle(common::configure_camera, (bindings, keyboard_guidance()))
        .with_camera_guidance_panel()
        .add_systems(Startup, common::spawn_scene)
        .run();
}
