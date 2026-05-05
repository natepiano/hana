//! Reproduces the `ZoomToFit` edge-on degenerate case.
//!
//! Camera starts at pitch=0 looking straight ahead. Click the ground plane
//! to trigger `ZoomToFit` — without the degenerate extent fix the radius
//! blows up to ~425m instead of converging to ~15m.

mod constants;

use bevy::picking::mesh_picking::MeshPickingPlugin;
use bevy::prelude::*;
use bevy_brp_extras::BrpExtrasPlugin;
use bevy_lagrange::InputControl;
use bevy_lagrange::LagrangePlugin;
use bevy_lagrange::OrbitCam;
use bevy_lagrange::TrackpadBehavior;
use bevy_lagrange::TrackpadInput;
use bevy_lagrange::ZoomToFit;
use bevy_window_manager::WindowManagerPlugin;

use crate::constants::CAMERA_FOCUS;
use crate::constants::CAMERA_PITCH;
use crate::constants::CAMERA_RADIUS;
use crate::constants::CAMERA_TRACKPAD_SENSITIVITY;
use crate::constants::CAMERA_TRANSLATION;
use crate::constants::CAMERA_YAW;
use crate::constants::CUBE_COLOR;
use crate::constants::CUBE_TRANSLATION;
use crate::constants::GROUND_COLOR;
use crate::constants::GROUND_SIZE;
use crate::constants::LIGHT_TRANSLATION;
use crate::constants::ZOOM_DURATION;
use crate::constants::ZOOM_MARGIN_MESH;
use crate::constants::ZOOM_MARGIN_SCENE;

#[derive(Resource)]
struct SceneBounds(Entity);

fn main() {
    App::new()
        .add_plugins((
            DefaultPlugins,
            LagrangePlugin,
            BrpExtrasPlugin::default(),
            MeshPickingPlugin,
            WindowManagerPlugin,
        ))
        .add_systems(Startup, setup)
        .run();
}

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // Ground plane
    let ground = commands
        .spawn((
            Mesh3d(meshes.add(Plane3d::default().mesh().size(GROUND_SIZE, GROUND_SIZE))),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: GROUND_COLOR,
                alpha_mode: AlphaMode::Blend,
                double_sided: true,
                cull_mode: None,
                ..default()
            })),
        ))
        .observe(on_ground_clicked)
        .id();

    commands.insert_resource(SceneBounds(ground));

    // Cube
    commands
        .spawn((
            Mesh3d(meshes.add(Cuboid::default())),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: CUBE_COLOR,
                ..default()
            })),
            Transform::from_translation(CUBE_TRANSLATION),
        ))
        .observe(on_mesh_clicked);

    // Light
    commands.spawn((
        DirectionalLight {
            shadows_enabled: true,
            ..default()
        },
        Transform::from_translation(LIGHT_TRANSLATION).looking_at(Vec3::ZERO, Vec3::Y),
    ));

    // Camera — pitch=0 reproduces the edge-on degenerate case when `ZoomToFit`
    // targets the ground plane.
    commands.spawn((
        OrbitCam {
            pitch: Some(CAMERA_PITCH),
            yaw: Some(CAMERA_YAW),
            radius: Some(CAMERA_RADIUS),
            focus: CAMERA_FOCUS,
            button_orbit: MouseButton::Middle,
            button_pan: MouseButton::Middle,
            modifier_pan: Some(KeyCode::ShiftLeft),
            input_control: Some(InputControl {
                trackpad: Some(TrackpadInput {
                    behavior:    TrackpadBehavior::blender_default(),
                    sensitivity: CAMERA_TRACKPAD_SENSITIVITY,
                }),
                ..default()
            }),
            ..default()
        },
        Transform::from_translation(CAMERA_TRANSLATION).looking_at(CAMERA_FOCUS, Vec3::Y),
    ));
}

fn on_mesh_clicked(click: On<Pointer<Click>>, mut commands: Commands) {
    if click.button != PointerButton::Primary {
        return;
    }
    let camera = click.hit.camera;
    commands.trigger(
        ZoomToFit::new(camera, click.entity)
            .margin(ZOOM_MARGIN_MESH)
            .duration(ZOOM_DURATION),
    );
}

fn on_ground_clicked(click: On<Pointer<Click>>, mut commands: Commands, scene: Res<SceneBounds>) {
    if click.button != PointerButton::Primary {
        return;
    }
    let camera = click.hit.camera;
    commands.trigger(
        ZoomToFit::new(camera, scene.0)
            .margin(ZOOM_MARGIN_SCENE)
            .duration(ZOOM_DURATION),
    );
}
