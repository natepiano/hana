//! Demonstrates usage with an orthographic camera

use bevy::camera::ScalingMode;
use bevy::prelude::*;
use bevy_brp_extras::BrpExtrasPlugin;
use bevy_lagrange::ForceUpdate;
use bevy_lagrange::LagrangePlugin;
use bevy_lagrange::OrbitCam;
use bevy_lagrange::OrbitCamSystemSet;
use bevy_window_manager::WindowManagerPlugin;

// camera
const CAMERA_TRANSLATION: Vec3 = Vec3::new(0.0, 1.5, 6.0);
const ORTHOGRAPHIC_VIEWPORT_HEIGHT: f32 = 1.0;

// cube
const CUBE_COLOR: Color = Color::srgb(0.8, 0.7, 0.6);
const CUBE_SIZE: f32 = 1.0;
const CUBE_TRANSLATION: Vec3 = Vec3::new(0.0, 0.5, 0.0);

// scene
const GROUND_COLOR: Color = Color::srgb(0.3, 0.5, 0.3);
const GROUND_SIZE: f32 = 5.0;
const LIGHT_TRANSLATION: Vec3 = Vec3::new(4.0, 8.0, 4.0);

// ui
const PROJECTION_HELP_TEXT: &str = "Press R to switch projection";

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(LagrangePlugin)
        .add_plugins(BrpExtrasPlugin::default())
        .add_plugins(WindowManagerPlugin)
        .add_systems(Startup, setup)
        .add_systems(Update, switch_projection.before(OrbitCamSystemSet))
        .run();
}

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // help
    commands.spawn(Text::new(PROJECTION_HELP_TEXT));
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
        Projection::from(OrthographicProjection {
            scaling_mode: ScalingMode::FixedVertical {
                viewport_height: ORTHOGRAPHIC_VIEWPORT_HEIGHT,
            },
            ..OrthographicProjection::default_3d()
        }),
        OrbitCam::default(),
    ));
}

fn switch_projection(
    mut next_projection: Local<Projection>,
    key_input: Res<ButtonInput<KeyCode>>,
    mut camera_query: Query<(&mut OrbitCam, &mut Projection)>,
) {
    if key_input.just_pressed(KeyCode::KeyR) {
        let Ok((mut camera, mut projection)) = camera_query.single_mut() else {
            return;
        };
        std::mem::swap(&mut *next_projection, &mut *projection);
        camera.force_update = ForceUpdate::Pending;
    }
}
