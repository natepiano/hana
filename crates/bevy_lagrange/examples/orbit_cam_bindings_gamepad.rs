//! Demonstrates gamepad user input through `OrbitCamBindings`.
//!
//! Synthetic tests that drive the trigger bindings must set both the analog
//! button value and the digital pressed state. The current public gamepad
//! selection policy is `Active`, which routes any active gamepad through the
//! camera input route; selected-device routing is future API work.

mod common;

use bevy::input::gamepad::Gamepad;
use bevy::prelude::*;
use bevy_lagrange::BindingRecipe;
use bevy_lagrange::BindingRoutePolicy;
use bevy_lagrange::CameraInputGamepadSelectionPolicy;
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

#[derive(Component)]
struct GamepadStatus;

fn gamepad_bindings() -> Result<OrbitCamBindings, OrbitCamBindingsError> {
    let right_stick =
        BindingRecipe::GamepadAxes2d(GamepadAxis::RightStickX, GamepadAxis::RightStickY);
    let left_stick = BindingRecipe::GamepadAxes2d(GamepadAxis::LeftStickX, GamepadAxis::LeftStickY);
    let triggers = BindingRecipe::BidirectionalGamepadButtons(
        GamepadButton::RightTrigger2,
        GamepadButton::LeftTrigger2,
    );

    OrbitCamBindings::builder()
        .held_orbit_binding(
            HeldActionBindingEntry::<OrbitCamOrbitAction>::from_enhanced_input_pair(
                right_stick,
                right_stick,
                CameraInteractionSources::GAMEPAD,
                BindingRoutePolicy::NoPosition,
            )?,
        )
        .held_pan_binding(
            HeldActionBindingEntry::<OrbitCamPanAction>::from_enhanced_input_pair(
                left_stick,
                BindingRecipe::GamepadButton(GamepadButton::LeftTrigger),
                CameraInteractionSources::GAMEPAD,
                BindingRoutePolicy::NoPosition,
            )?,
        )
        .held_smooth_zoom_binding(
            HeldActionBindingEntry::<OrbitCamZoomSmoothAction>::from_enhanced_input_pair(
                triggers,
                triggers,
                CameraInteractionSources::GAMEPAD,
                BindingRoutePolicy::NoPosition,
            )?,
        )
        .gamepad(CameraInputGamepadSelectionPolicy::Active)
        .wheel(OrbitCamWheelBinding::Disabled)
        .build()
}

fn gamepad_guidance() -> CameraGuidance {
    CameraGuidance::custom([
        CameraGuidanceRow::new(OrbitCamInteractionKind::Orbit, "Right stick -> Orbit")
            .when_sources(CameraInteractionSources::GAMEPAD),
        CameraGuidanceRow::new(OrbitCamInteractionKind::Pan, "Left stick + L1 -> Pan")
            .when_sources(CameraInteractionSources::GAMEPAD),
        CameraGuidanceRow::new(OrbitCamInteractionKind::Zoom, "Triggers -> Zoom")
            .when_sources(CameraInteractionSources::GAMEPAD),
    ])
    .with_title("Gamepad Bindings")
}

fn spawn_status(mut commands: Commands) {
    commands.spawn((
        Text::new("Gamepad: none detected"),
        TextColor(Color::srgb(0.9, 0.9, 1.0)),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(12.0),
            left: Val::Px(12.0),
            ..default()
        },
        GamepadStatus,
    ));
}

fn update_status(
    gamepads: Query<(), With<Gamepad>>,
    mut labels: Query<&mut Text, With<GamepadStatus>>,
) {
    let status = if gamepads.is_empty() {
        "Gamepad: none detected"
    } else {
        "Gamepad: active policy uses any connected controller"
    };
    for mut label in &mut labels {
        **label = status.into();
    }
}

fn main() {
    let Ok(bindings) = gamepad_bindings() else {
        error!("gamepad camera bindings failed to validate");
        return;
    };

    fairy_dust::sprinkle_example()
        .insert_resource(
            CameraInputRoutingConfig::cursor_hit_test()
                .with_no_position_fallback(NoPositionFallback::OnlyEligibleCamera),
        )
        .with_orbit_cam_bundle(common::configure_camera, (bindings, gamepad_guidance()))
        .with_camera_guidance_panel()
        .add_systems(Startup, (common::spawn_scene, spawn_status))
        .add_systems(Update, update_status)
        .run();
}
