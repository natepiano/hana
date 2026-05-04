//! Demonstrates common configuration options,
//! and how to modify them at runtime
//!
//! Controls:
//!   Orbit: Middle click
//!   Pan: Shift + Middle click
//!   Zoom: Mousewheel OR Right click + move mouse up/down

use std::f32::consts::TAU;

use bevy::prelude::*;
use bevy_brp_extras::BrpExtrasPlugin;
use bevy_lagrange::InputControl;
use bevy_lagrange::LagrangePlugin;
use bevy_lagrange::OrbitCam;
use bevy_lagrange::TouchInput;
use bevy_lagrange::TrackpadInput;
use bevy_lagrange::UpsideDownPolicy;
use bevy_lagrange::ZoomDirection;
use bevy_window_manager::WindowManagerPlugin;

// camera
const CAMERA_FOCUS: Vec3 = Vec3::new(0.0, 1.0, 0.0);
const CAMERA_ORBIT_SENSITIVITY: f32 = 1.5;
const CAMERA_PAN_SENSITIVITY: f32 = 0.5;
const CAMERA_PITCH: f32 = TAU / 8.0;
const CAMERA_PITCH_LIMIT: f32 = TAU / 3.0;
const CAMERA_RADIUS: f32 = 5.0;
const CAMERA_YAW: f32 = TAU / 8.0;
const CAMERA_YAW_LIMIT: f32 = TAU / 4.0;
const CAMERA_ZOOM_LOWER_LIMIT: f32 = 1.0;
const CAMERA_ZOOM_SENSITIVITY: f32 = 0.5;
const CAMERA_ZOOM_UPPER_LIMIT: f32 = 5.0;

// cube
const CUBE_COLOR: Color = Color::srgb(0.8, 0.7, 0.6);
const CUBE_SIZE: f32 = 1.0;
const CUBE_TRANSLATION: Vec3 = Vec3::new(0.0, 0.5, 0.0);

// scene
const GROUND_COLOR: Color = Color::srgb(0.3, 0.5, 0.3);
const GROUND_SIZE: f32 = 5.0;
const LIGHT_TRANSLATION: Vec3 = Vec3::new(4.0, 8.0, 4.0);

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(LagrangePlugin)
        .add_plugins(BrpExtrasPlugin::default())
        .add_plugins(WindowManagerPlugin)
        .add_systems(Startup, setup)
        .add_systems(Update, toggle_camera_controls_system)
        .run();
}

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // Ground
    commands.spawn((
        Mesh3d(meshes.add(Plane3d::default().mesh().size(GROUND_SIZE, GROUND_SIZE))),
        MeshMaterial3d(materials.add(GROUND_COLOR)),
    ));
    // Cube
    commands.spawn((
        Mesh3d(meshes.add(Cuboid::new(CUBE_SIZE, CUBE_SIZE, CUBE_SIZE))),
        MeshMaterial3d(materials.add(CUBE_COLOR)),
        Transform::from_translation(CUBE_TRANSLATION),
    ));
    // Light
    commands.spawn((
        PointLight {
            shadows_enabled: true,
            ..default()
        },
        Transform::from_translation(LIGHT_TRANSLATION),
    ));
    // Camera
    commands.spawn((
        // Note we're setting the initial position below with yaw, pitch, and radius, hence
        // we don't set transform on the camera.
        OrbitCam {
            // Set focal point (what the camera should look at)
            focus: CAMERA_FOCUS,
            // Set the starting position, relative to focus (overrides camera's transform).
            yaw: Some(CAMERA_YAW),
            pitch: Some(CAMERA_PITCH),
            radius: Some(CAMERA_RADIUS),
            // Set limits on rotation and zoom
            yaw_upper_limit: Some(CAMERA_YAW_LIMIT),
            yaw_lower_limit: Some(-CAMERA_YAW_LIMIT),
            pitch_upper_limit: Some(CAMERA_PITCH_LIMIT),
            pitch_lower_limit: Some(-CAMERA_PITCH_LIMIT),
            zoom_upper_limit: Some(CAMERA_ZOOM_UPPER_LIMIT),
            zoom_lower_limit: CAMERA_ZOOM_LOWER_LIMIT,
            // Adjust sensitivity of controls
            orbit_sensitivity: CAMERA_ORBIT_SENSITIVITY,
            pan_sensitivity: CAMERA_PAN_SENSITIVITY,
            zoom_sensitivity: CAMERA_ZOOM_SENSITIVITY,
            // Allow the camera to go upside down
            upside_down_policy: UpsideDownPolicy::Allow,
            // Change the controls (these match Blender)
            button_orbit: MouseButton::Middle,
            button_pan: MouseButton::Middle,
            modifier_pan: Some(KeyCode::ShiftLeft),
            // Also enable zooming by holding right click and moving the mouse
            button_zoom: Some(MouseButton::Right),
            // Optionally configure button zoom to use left-right mouse movement
            // button_zoom_axis: ButtonZoomAxis::X,
            input_control: Some(InputControl {
                // Use alternate touch controls
                touch:    Some(TouchInput::TwoFingerOrbit),
                trackpad: Some(TrackpadInput::blender_default()),
                // Reverse the zoom direction
                zoom:     ZoomDirection::Reversed,
            }),
            ..default()
        },
    ));
}

// This is how you can change config at runtime.
// Press 'T' to toggle the camera controls.
fn toggle_camera_controls_system(
    key_input: Res<ButtonInput<KeyCode>>,
    mut saved_input_control: Local<Option<InputControl>>,
    mut orbit_cam_query: Query<&mut OrbitCam>,
) {
    if key_input.just_pressed(KeyCode::KeyT) {
        for mut orbit_cam in &mut orbit_cam_query {
            if let Some(input_control) = orbit_cam.input_control {
                *saved_input_control = Some(input_control);
                orbit_cam.input_control = None;
            } else {
                orbit_cam.input_control = saved_input_control
                    .take()
                    .or_else(|| Some(InputControl::default()));
            }
        }
    }
}
