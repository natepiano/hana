//! Demonstrates how to pause time without affecting the camera

use bevy::prelude::*;
use bevy_brp_extras::BrpExtrasPlugin;
use bevy_lagrange::InputControl;
use bevy_lagrange::LagrangePlugin;
use bevy_lagrange::OrbitCam;
use bevy_lagrange::TimeSource;
use bevy_lagrange::TrackpadInput;
use bevy_window_manager::WindowManagerPlugin;

// camera
const CAMERA_TRANSLATION: Vec3 = Vec3::new(0.0, 1.5, 5.0);

// cube
const CUBE_COLOR: Color = Color::srgb(0.8, 0.7, 0.6);
const CUBE_ROTATION_SPEED: f32 = 1.0;
const CUBE_SIZE: f32 = 1.0;
const CUBE_TRANSLATION: Vec3 = Vec3::new(0.0, 0.5, 0.0);

// scene
const GROUND_COLOR: Color = Color::srgb(0.3, 0.5, 0.3);
const GROUND_SIZE: f32 = 5.0;
const LIGHT_TRANSLATION: Vec3 = Vec3::new(4.0, 8.0, 4.0);

// ui
const PAUSE_HELP_TEXT: &str = "Press Space to pause the 'game'";

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(LagrangePlugin)
        .add_plugins(BrpExtrasPlugin::default())
        .add_plugins(WindowManagerPlugin)
        .add_systems(Startup, setup)
        .add_systems(Update, (pause_game_system, cube_rotator_system))
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
    commands.spawn((
        Mesh3d(meshes.add(Cuboid::new(CUBE_SIZE, CUBE_SIZE, CUBE_SIZE))),
        MeshMaterial3d(materials.add(CUBE_COLOR)),
        Transform::from_translation(CUBE_TRANSLATION),
        Cube,
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
            time_source: TimeSource::Real,
            input_control: Some(InputControl {
                trackpad: Some(TrackpadInput::blender_default()),
                ..default()
            }),
            ..default()
        },
    ));
    // Help text
    commands.spawn(Text::new(PAUSE_HELP_TEXT));
}

// Pauses the game (i.e. virtual time)
fn pause_game_system(key_input: Res<ButtonInput<KeyCode>>, mut time: ResMut<Time<Virtual>>) {
    if key_input.just_pressed(KeyCode::Space) {
        if time.is_paused() {
            time.unpause();
        } else {
            time.pause();
        }
    }
}

// Rotates the cube so you can see the effect of pausing time
// Note the default time for the Update schedule is `Time<Virtual>`
fn cube_rotator_system(time: Res<Time>, mut query: Query<&mut Transform, With<Cube>>) {
    for mut transform in &mut query {
        transform.rotate_y(CUBE_ROTATION_SPEED * time.delta_secs());
    }
}
