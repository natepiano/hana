//! Demonstrates the bevy_egui feature which allows bevy_lagrange to ignore input events in
//! egui windows

use bevy::prelude::*;
use bevy_egui::egui;
use bevy_egui::EguiContexts;
use bevy_egui::EguiPlugin;
use bevy_egui::EguiPrimaryContextPass;
use bevy_lagrange::LagrangePlugin;
use bevy_lagrange::PanOrbitCamera;

fn main() {
    let mut app = App::new();
    app.add_plugins(DefaultPlugins)
        .add_plugins(EguiPlugin::default())
        .add_plugins(LagrangePlugin)
        .add_systems(Startup, setup)
        .add_systems(EguiPrimaryContextPass, ui_example_system);

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
    // Camera
    commands.spawn((
        Transform::from_translation(Vec3::new(0.0, 1.5, 5.0)),
        PanOrbitCamera::default(),
    ));
}

fn ui_example_system(mut contexts: EguiContexts) -> Result {
    egui::SidePanel::left("left_panel")
        .resizable(true)
        .show(contexts.ctx_mut()?, |ui| {
            ui.label("Left resizeable panel");
        });

    egui::Window::new("Movable Window").show(contexts.ctx_mut()?, |ui| {
        ui.label("Hello world");
    });

    egui::Window::new("Immovable Window")
        .movable(false)
        .show(contexts.ctx_mut()?, |ui| {
            ui.label("Hello world");
        });
    Ok(())
}
