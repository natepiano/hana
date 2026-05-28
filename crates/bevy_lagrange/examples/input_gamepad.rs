//! Demonstrates gamepad user input through `OrbitCamBindings`.
//!
//! Synthetic tests that drive the trigger bindings must set both the analog
//! button value and the digital pressed state. The current public gamepad
//! selection policy is `Active`, which routes any active gamepad through the
//! camera input route; selected-device routing is future API work.

use bevy::input::gamepad::Gamepad;
use bevy::prelude::*;
use bevy_lagrange::CameraInputGamepadSelectionPolicy;
use bevy_lagrange::CameraInputRoutingConfig;
use bevy_lagrange::NoPositionFallback;
use bevy_lagrange::OrbitCam;
use bevy_lagrange::OrbitCamBindings;
use bevy_lagrange::OrbitCamBindingsError;
use bevy_lagrange::OrbitCamHeldBinding;
use bevy_lagrange::OrbitCamInputBinding;
use bevy_lagrange::OrbitCamInputMode;
use fairy_dust::Anchor;
use fairy_dust::CameraHomeTarget;
use fairy_dust::ControlActivation;
use fairy_dust::TitleBar;

// camera
const CAMERA_FOCUS: Vec3 = CUBE_TRANSLATION;
const CAMERA_PITCH: f32 = 0.45;
const CAMERA_RADIUS: f32 = 6.0;
const CAMERA_YAW: f32 = 0.55;
const HOME_MARGIN: f32 = 0.5;

// cube
const CUBE_COLOR: Color = Color::srgb(0.8, 0.7, 0.6);
const CUBE_GROUND_CLEARANCE: f32 = 0.1;
const CUBE_SIZE: f32 = 1.0;
const CUBE_TRANSLATION: Vec3 = Vec3::new(0.0, CUBE_SIZE * 0.5 + CUBE_GROUND_CLEARANCE, 0.0);

// scene and HUD
const GROUND_COLOR: Color = Color::srgb(0.28, 0.42, 0.34);
const GROUND_SIZE: f32 = 7.0;
const ORBIT_CONTROL: &str = "RS Orbit";
const PAN_CONTROL: &str = "LB+LS Pan";
const ZOOM_CONTROL: &str = "LT/RT Zoom";
const GAMEPAD_CONNECTED_CONTROL: &str = "Gamepad Connected";

#[derive(Resource, Default)]
struct GamepadConnection {
    connected: bool,
}

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
        .with_brp_extras()
        .with_save_window_position()
        .with_studio_lighting()
        .with_ground_plane()
        .size(GROUND_SIZE)
        .color(GROUND_COLOR)
        .with_cube()
        .size(CUBE_SIZE)
        .color(CUBE_COLOR)
        .transform(Transform::from_translation(CUBE_TRANSLATION))
        .insert(CameraHomeTarget)
        .with_orbit_cam(configure_camera, OrbitCamInputMode::Bindings(bindings))
        .with_camera_home()
        .yaw(CAMERA_YAW)
        .pitch(CAMERA_PITCH)
        .margin(HOME_MARGIN)
        .with_title_bar(
            TitleBar::new()
                .with_title("Gamepad")
                .with_anchor(Anchor::TopLeft)
                .control(ORBIT_CONTROL)
                .control(PAN_CONTROL)
                .control(ZOOM_CONTROL)
                .control(GAMEPAD_CONNECTED_CONTROL),
        )
        .wire_chip_to_state::<GamepadConnection, _>(GAMEPAD_CONNECTED_CONTROL, |connection| {
            activation_for(connection.connected)
        })
        .init_resource::<GamepadConnection>()
        .with_camera_control_panel()
        .add_systems(Update, update_gamepad_connection)
        .run();
}

fn configure_camera(camera: &mut OrbitCam) {
    camera.focus = CAMERA_FOCUS;
    camera.yaw = Some(CAMERA_YAW);
    camera.pitch = Some(CAMERA_PITCH);
    camera.radius = Some(CAMERA_RADIUS);
}

const fn activation_for(active: bool) -> ControlActivation {
    if active {
        ControlActivation::Active
    } else {
        ControlActivation::Inactive
    }
}

fn update_gamepad_connection(
    gamepads: Query<(), With<Gamepad>>,
    mut connection: ResMut<GamepadConnection>,
) {
    let connected = !gamepads.is_empty();
    if connection.connected != connected {
        connection.connected = connected;
    }
}
