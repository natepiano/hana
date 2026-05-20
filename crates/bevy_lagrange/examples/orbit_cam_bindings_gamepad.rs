//! Demonstrates gamepad user input through `OrbitCamBindings`.
//!
//! Synthetic tests that drive the trigger bindings must set both the analog
//! button value and the digital pressed state. The current public gamepad
//! selection policy is `Active`, which routes any active gamepad through the
//! camera input route; selected-device routing is future API work.

mod common;

use bevy::input::gamepad::Gamepad;
use bevy::prelude::*;
use bevy_lagrange::CameraInputGamepadSelectionPolicy;
use bevy_lagrange::CameraInputRoutingConfig;
use bevy_lagrange::NoPositionFallback;
use bevy_lagrange::OrbitCamBindings;
use bevy_lagrange::OrbitCamBindingsError;
use bevy_lagrange::OrbitCamHeldBinding;
use bevy_lagrange::OrbitCamInputBinding;
use bevy_lagrange::OrbitCamInputMode;

const ACTIVE_GAMEPAD_STATUS: &str = "Gamepad: active policy uses any connected controller";
const DISCONNECTED_GAMEPAD_STATUS: &str = "Gamepad: none detected";
const STATUS_TEXT_COLOR: Color = Color::srgb(0.9, 0.9, 1.0);
const STATUS_TEXT_INSET_PIXELS: f32 = 12.0;

#[derive(Component)]
struct GamepadStatus;

fn gamepad_bindings() -> Result<OrbitCamBindings, OrbitCamBindingsError> {
    let right_stick =
        OrbitCamInputBinding::gamepad_axes_2d(GamepadAxis::RightStickX, GamepadAxis::RightStickY);
    let left_stick =
        OrbitCamInputBinding::gamepad_axes_2d(GamepadAxis::LeftStickX, GamepadAxis::LeftStickY);
    let triggers = OrbitCamInputBinding::bidirectional_gamepad_buttons(
        GamepadButton::RightTrigger2,
        GamepadButton::LeftTrigger2,
    );

    OrbitCamBindings::builder()
        .orbit(right_stick)
        .pan(OrbitCamHeldBinding::new(
            left_stick,
            GamepadButton::LeftTrigger,
        ))
        .zoom(triggers)
        .gamepad(CameraInputGamepadSelectionPolicy::Active)
        .build()
}

fn spawn_status(mut commands: Commands) {
    commands.spawn((
        Text::new(DISCONNECTED_GAMEPAD_STATUS),
        TextColor(STATUS_TEXT_COLOR),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(STATUS_TEXT_INSET_PIXELS),
            left: Val::Px(STATUS_TEXT_INSET_PIXELS),
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
        DISCONNECTED_GAMEPAD_STATUS
    } else {
        ACTIVE_GAMEPAD_STATUS
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
        .with_orbit_cam(
            common::configure_camera,
            OrbitCamInputMode::Bindings(bindings),
        )
        .with_camera_control_panel()
        .add_systems(Startup, (common::spawn_scene, spawn_status))
        .add_systems(Update, update_status)
        .run();
}
