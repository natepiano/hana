use bevy::prelude::*;
use bevy_lagrange::OrbitCam;

const CAMERA_FOCUS: Vec3 = Vec3::new(0.0, 0.75, 0.0);
const CAMERA_PITCH: f32 = 0.45;
const CAMERA_RADIUS: f32 = 6.0;
const CAMERA_YAW: f32 = 0.55;

const CUBE_COLOR: Color = Color::srgb(0.8, 0.7, 0.6);
const CUBE_SIZE: f32 = 1.0;
const CUBE_TRANSLATION: Vec3 = Vec3::new(0.0, 0.5, 0.0);
const GROUND_COLOR: Color = Color::srgb(0.28, 0.42, 0.34);
const GROUND_SIZE: f32 = 7.0;
const LIGHT_TRANSLATION: Vec3 = Vec3::new(4.0, 8.0, 4.0);

pub(super) const fn configure_camera(camera: &mut OrbitCam) {
    camera.focus = CAMERA_FOCUS;
    camera.yaw = Some(CAMERA_YAW);
    camera.pitch = Some(CAMERA_PITCH);
    camera.radius = Some(CAMERA_RADIUS);
}

pub(super) fn spawn_scene(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    commands.spawn((
        Mesh3d(meshes.add(Plane3d::default().mesh().size(GROUND_SIZE, GROUND_SIZE))),
        MeshMaterial3d(materials.add(GROUND_COLOR)),
    ));
    commands.spawn((
        Mesh3d(meshes.add(Cuboid::new(CUBE_SIZE, CUBE_SIZE, CUBE_SIZE))),
        MeshMaterial3d(materials.add(CUBE_COLOR)),
        Transform::from_translation(CUBE_TRANSLATION),
    ));
    commands.spawn((
        PointLight {
            shadow_maps_enabled: true,
            ..default()
        },
        Transform::from_translation(LIGHT_TRANSLATION),
    ));
}
