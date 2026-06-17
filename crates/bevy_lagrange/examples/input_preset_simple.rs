//! Minimal `OrbitCam::simple_mouse` preset example.

use bevy::prelude::*;
use bevy_lagrange::LagrangePlugin;
use bevy_lagrange::OrbitCam;

fn main() {
    App::new()
        .add_plugins((DefaultPlugins, LagrangePlugin))
        .add_systems(Startup, spawn_camera)
        .run();
}

fn spawn_camera(mut commands: Commands) {
    commands.spawn((Transform::from_xyz(0.0, 1.5, 5.0), OrbitCam::simple_mouse()));
}
