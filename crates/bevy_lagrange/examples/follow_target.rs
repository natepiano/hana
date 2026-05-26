//! Demonstrates how to have the camera follow a target object

use std::f32::consts::TAU;

use bevy::prelude::*;
use bevy_brp_extras::BrpExtrasPlugin;
use bevy_lagrange::LagrangePlugin;
use bevy_lagrange::OrbitCam;
use bevy_window_manager::WindowManagerPlugin;

// animation
const CUBE_ORBIT_DEGREES_PER_SECOND: f32 = 20.0;
const CUBE_ORBIT_RADIUS: f32 = 1.5;

// camera
const CAMERA_PAN_SENSITIVITY: f32 = 0.0;
const CAMERA_PAN_SMOOTHNESS: f32 = 0.0;
const CAMERA_TRANSLATION: Vec3 = Vec3::new(0.0, 1.5, 5.0);

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
        .add_systems(Update, (animate_cube, camera_follow).chain())
        .run();
}

#[derive(Component)]
struct Cube;

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
    commands
        .spawn((
            Mesh3d(meshes.add(Cuboid::new(CUBE_SIZE, CUBE_SIZE, CUBE_SIZE))),
            MeshMaterial3d(materials.add(CUBE_COLOR)),
            Transform::from_translation(CUBE_TRANSLATION),
        ))
        .insert(Cube);
    // Light
    commands.spawn((
        PointLight {
            shadow_maps_enabled: true,
            ..default()
        },
        Transform::from_translation(LIGHT_TRANSLATION),
    ));
    // Camera
    commands.spawn((
        Transform::from_translation(CAMERA_TRANSLATION),
        OrbitCam {
            // Panning the camera changes the focus, and so you most likely want to disable
            // panning when setting the focus manually
            pan_sensitivity: CAMERA_PAN_SENSITIVITY,
            // If you want to fully control the camera's focus, set smoothness to 0 so it
            // immediately snaps to that location. If you want the 'follow' to be smoothed,
            // leave this at default or set it to something between 0 and 1.
            pan_smoothness: CAMERA_PAN_SMOOTHNESS,
            ..default()
        },
    ));
}

/// Move the cube in a circle around the Y axis
fn animate_cube(
    time: Res<Time>,
    mut cube_query: Query<&mut Transform, With<Cube>>,
    mut angle: Local<f32>,
) {
    if let Ok(mut cube_transform) = cube_query.single_mut() {
        // Rotate 20 degrees a second, wrapping around to 0 after a full rotation
        *angle += CUBE_ORBIT_DEGREES_PER_SECOND.to_radians() * time.delta_secs() % TAU;
        // Convert angle to position
        let position = Vec3::new(
            angle.sin() * CUBE_ORBIT_RADIUS,
            CUBE_TRANSLATION.y,
            angle.cos() * CUBE_ORBIT_RADIUS,
        );
        cube_transform.translation = position;
    }
}

/// Set the camera's focus to the cube's position
fn camera_follow(
    mut orbit_cam_query: Query<&mut OrbitCam>,
    cube_query: Query<&Transform, With<Cube>>,
) {
    if let Ok(mut orbit_cam) = orbit_cam_query.single_mut()
        && let Ok(cube_transform) = cube_query.single()
    {
        orbit_cam.target_focus = cube_transform.translation;
    }
}
