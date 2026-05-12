//! Demonstrates how to keep the camera's focus inside a shape.

use bevy::color::palettes::css::WHITE;
use bevy::prelude::*;
use bevy_brp_extras::BrpExtrasPlugin;
use bevy_lagrange::LagrangePlugin;
use bevy_lagrange::OrbitCam;
use bevy_window_manager::WindowManagerPlugin;

// camera
const CAMERA_TRANSLATION: Vec3 = Vec3::new(0.0, 1.5, 5.0);
const FOCUS_BOUNDS_ORIGIN: Vec3 = Vec3::splat(1.0);
const FOCUS_BOUNDS_SIZE: f32 = 1.0;

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
        .add_systems(Update, show_bounds)
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
        Transform::from_translation(CAMERA_TRANSLATION),
        OrbitCam {
            // Shape can take Cuboid or Sphere
            focus_bounds_shape: Some(
                Cuboid::new(FOCUS_BOUNDS_SIZE, FOCUS_BOUNDS_SIZE, FOCUS_BOUNDS_SIZE).into(),
            ),
            // Move the origin of the shape
            focus_bounds_origin: FOCUS_BOUNDS_ORIGIN,
            ..default()
        },
    ));
}

fn show_bounds(mut gizmos: Gizmos) {
    // Display focus bound shape
    gizmos.cube(Transform::from_translation(FOCUS_BOUNDS_ORIGIN), WHITE);
}
