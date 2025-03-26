mod action;
mod basic_viz;
mod camera;
mod error;
mod error_handling;
mod oscillating_gizmo;
mod splash;

use bevy::prelude::*;
use hana_async::AsyncRuntimePlugin;
use hana_viz::HanaVizPlugin;

use crate::action::ActionPlugin;
use crate::basic_viz::BasicVizPlugin;
use crate::camera::CameraPlugin;
use crate::error_handling::ErrorHandlingPlugin;
use crate::oscillating_gizmo::OscillatingGizmoPlugin;
use crate::splash::SplashPlugin;

fn main() {
    trace!("Starting Hana visualization management system");

    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins((
            ActionPlugin,
            AsyncRuntimePlugin,
            BasicVizPlugin,
            CameraPlugin,
            ErrorHandlingPlugin,
            HanaVizPlugin,
            OscillatingGizmoPlugin,
            SplashPlugin,
        ))
        .add_systems(Startup, setup)
        .run();
}

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let cuboid = Cuboid {
        half_size: Vec3::new(1.0, 1.0, 1.0) / 2.0,
    };
    let mesh = meshes.add(Mesh::from(cuboid));
    let material = StandardMaterial::default();
    let material_handle = materials.add(material);

    let transform = Transform::from_xyz(2.0, 0.0, 0.0);
    commands
        .spawn(Mesh3d(mesh.clone()))
        .insert(transform)
        .insert(MeshMaterial3d(material_handle.clone()));
}
