//! Demonstrates the simplest usage

use bevy::prelude::*;
use bevy_brp_extras::BrpExtrasPlugin;
use bevy_lagrange::InputControl;
use bevy_lagrange::LagrangePlugin;
use bevy_lagrange::OrbitCam;
use bevy_lagrange::TrackpadInput;
use bevy_window_manager::WindowManagerPlugin;

// camera
const CAMERA_PITCH_DEGREES: f32 = -45.0;
const CAMERA_TRANSLATION: Vec3 = Vec3::new(0.0, 1.5, 5.0);
const SWAPPED_AXIS: [Vec3; 3] = [Vec3::X, Vec3::Z, Vec3::Y];

// cube
const CUBE_COLOR: Color = Color::srgb(0.8, 0.7, 0.6);
const CUBE_SIZE: f32 = 1.0;
const CUBE_TRANSLATION: Vec3 = Vec3::new(0.0, 0.5, 0.0);

// scene
const GROUND_COLOR: Color = Color::srgb(0.3, 0.5, 0.3);
const GROUND_SIZE: f32 = 5.0;
const LIGHT_TRANSLATION: Vec3 = Vec3::new(4.0, 8.0, 4.0);
const WORLD_ROTATION_DEGREES: f32 = 90.0;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(LagrangePlugin)
        .add_plugins(BrpExtrasPlugin::default())
        .add_plugins(WindowManagerPlugin)
        .add_systems(Startup, setup)
        .run();
}

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // Rotation to mimic a rotated world.
    let rotate =
        Transform::from_rotation(Quat::from_rotation_x(WORLD_ROTATION_DEGREES.to_radians()));
    // Ground
    commands.spawn((
        Mesh3d(meshes.add(Plane3d::default().mesh().size(GROUND_SIZE, GROUND_SIZE))),
        MeshMaterial3d(materials.add(GROUND_COLOR)),
        rotate,
    ));
    // Cube
    commands.spawn((
        Mesh3d(meshes.add(Cuboid::new(CUBE_SIZE, CUBE_SIZE, CUBE_SIZE))),
        MeshMaterial3d(materials.add(CUBE_COLOR)),
        rotate * Transform::from_translation(CUBE_TRANSLATION),
    ));
    // Light
    commands.spawn((
        PointLight {
            shadows_enabled: true,
            ..default()
        },
        rotate * Transform::from_translation(LIGHT_TRANSLATION),
    ));
    // Camera
    // Swaps the axis of the camera to use Z as up instead of Y as up which is the default.
    let camera = OrbitCam {
        axis: SWAPPED_AXIS,
        pitch: Some(CAMERA_PITCH_DEGREES.to_radians()),
        input_control: Some(InputControl {
            trackpad: Some(TrackpadInput::blender_default()),
            ..default()
        }),
        ..default()
    };
    commands.spawn((Transform::from_translation(CAMERA_TRANSLATION), camera));
}
