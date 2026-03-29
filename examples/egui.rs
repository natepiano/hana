//! Demonstrates `BlockOnEguiFocus` — prevents camera input while hovering egui.
//!
//! Controls:
//!   T — Toggle `BlockOnEguiFocus` on/off
//!
//! With blocking ON: hover the panel and try to orbit — camera won't respond.
//! With blocking OFF: orbit works even while hovering the panel.

use bevy::prelude::*;
use bevy_brp_extras::BrpExtrasPlugin;
use bevy_egui::EguiContexts;
use bevy_egui::EguiPlugin;
use bevy_egui::EguiPrimaryContextPass;
use bevy_egui::egui;
use bevy_lagrange::BlockOnEguiFocus;
use bevy_lagrange::LagrangePlugin;
use bevy_lagrange::OrbitCam;
use bevy_lagrange::TrackpadBehavior;
use bevy_window_manager::WindowManagerPlugin;

fn main() {
    let mut app = App::new();
    app.add_plugins(DefaultPlugins)
        .add_plugins(EguiPlugin::default())
        .add_plugins(LagrangePlugin)
        .add_plugins(BrpExtrasPlugin::default())
        .add_plugins(WindowManagerPlugin)
        .add_systems(Startup, setup)
        .add_systems(Update, toggle_blocking)
        .add_systems(EguiPrimaryContextPass, ui_system);

    app.run();
}

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // Ground
    commands.spawn((
        Mesh3d(meshes.add(Plane3d::default().mesh().size(5.0, 5.0))),
        MeshMaterial3d(materials.add(Color::srgb(0.3, 0.5, 0.3))),
    ));
    // Cube
    commands.spawn((
        Mesh3d(meshes.add(Cuboid::new(1.0, 1.0, 1.0))),
        MeshMaterial3d(materials.add(Color::srgb(0.8, 0.7, 0.6))),
        Transform::from_xyz(0.0, 0.5, 0.0),
    ));
    // Light
    commands.spawn((
        PointLight {
            shadows_enabled: true,
            ..default()
        },
        Transform::from_xyz(4.0, 8.0, 4.0),
    ));
    // Camera — starts with blocking enabled
    commands.spawn((
        Transform::from_translation(Vec3::new(0.0, 1.5, 5.0)),
        OrbitCam {
            trackpad_behavior: TrackpadBehavior::BlenderLike {
                modifier_pan:  Some(KeyCode::ShiftLeft),
                modifier_zoom: Some(KeyCode::ControlLeft),
            },
            trackpad_pinch_to_zoom_enabled: true,
            ..default()
        },
        BlockOnEguiFocus,
    ));
}

fn toggle_blocking(
    keys: Res<ButtonInput<KeyCode>>,
    mut commands: Commands,
    query: Query<(Entity, Option<&BlockOnEguiFocus>), With<OrbitCam>>,
) {
    if keys.just_pressed(KeyCode::KeyT) {
        for (entity, blocking) in &query {
            if blocking.is_some() {
                commands.entity(entity).remove::<BlockOnEguiFocus>();
                info!("BlockOnEguiFocus OFF");
            } else {
                commands.entity(entity).insert(BlockOnEguiFocus);
                info!("BlockOnEguiFocus ON");
            }
        }
    }
}

fn ui_system(
    mut contexts: EguiContexts,
    query: Query<Option<&BlockOnEguiFocus>, With<OrbitCam>>,
) -> Result {
    let blocking = query.iter().next().is_some_and(|b| b.is_some());
    let status = if blocking { "ON" } else { "OFF" };

    egui::SidePanel::left("panel")
        .resizable(true)
        .show(contexts.ctx_mut()?, |ui| {
            ui.heading(format!("BlockOnEguiFocus: {status}"));
            ui.separator();
            ui.label("Press T to toggle blocking.");
            ui.separator();
            ui.label("Hover here and try to orbit.");
        });
    Ok(())
}
