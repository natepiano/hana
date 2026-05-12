//! Demonstrates keyboard user input through `OrbitCamBindings`.

mod common;

use bevy::prelude::*;
use bevy_lagrange::BindingRecipe;
use bevy_lagrange::BindingRoutePolicy;
use bevy_lagrange::CameraInputRoutingConfig;
use bevy_lagrange::CameraInteractionSources;
use bevy_lagrange::HeldActionBindingEntry;
use bevy_lagrange::NoPositionFallback;
use bevy_lagrange::OrbitCamBindings;
use bevy_lagrange::OrbitCamBindingsError;
use bevy_lagrange::OrbitCamInteractionKind;
use bevy_lagrange::OrbitCamOrbitAction;
use bevy_lagrange::OrbitCamPanAction;
use bevy_lagrange::OrbitCamWheelBinding;
use bevy_lagrange::OrbitCamZoomSmoothAction;
use fairy_dust::CameraGuidance;
use fairy_dust::CameraGuidanceRow;

fn keyboard_bindings() -> Result<OrbitCamBindings, OrbitCamBindingsError> {
    let orbit_keys = BindingRecipe::CardinalKeys(
        KeyCode::ArrowUp,
        KeyCode::ArrowRight,
        KeyCode::ArrowDown,
        KeyCode::ArrowLeft,
    );
    let pan_keys =
        BindingRecipe::CardinalKeys(KeyCode::KeyW, KeyCode::KeyD, KeyCode::KeyS, KeyCode::KeyA);
    let zoom_keys = BindingRecipe::BidirectionalKeys(KeyCode::Equal, KeyCode::Minus);

    OrbitCamBindings::builder()
        .held_orbit_binding(
            HeldActionBindingEntry::<OrbitCamOrbitAction>::from_enhanced_input_pair(
                orbit_keys,
                orbit_keys,
                CameraInteractionSources::KEYBOARD,
                BindingRoutePolicy::NoPosition,
            )?,
        )
        .held_pan_binding(
            HeldActionBindingEntry::<OrbitCamPanAction>::from_enhanced_input_pair(
                pan_keys,
                pan_keys,
                CameraInteractionSources::KEYBOARD,
                BindingRoutePolicy::NoPosition,
            )?,
        )
        .held_smooth_zoom_binding(
            HeldActionBindingEntry::<OrbitCamZoomSmoothAction>::from_enhanced_input_pair(
                zoom_keys,
                zoom_keys,
                CameraInteractionSources::KEYBOARD,
                BindingRoutePolicy::NoPosition,
            )?,
        )
        .wheel(OrbitCamWheelBinding::Disabled)
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
