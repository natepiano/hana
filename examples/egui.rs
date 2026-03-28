//! Demonstrates `BlockOnEguiFocus` for per-camera egui input blocking.
//!
//! Split-screen with two viewports. Each side has a floating egui window.
//! The left camera has `BlockOnEguiFocus` — hovering over its egui window
//! prevents orbiting. The right camera has no blocking — orbiting works
//! even while hovering over its egui window.

use bevy::camera::Viewport;
use bevy::prelude::*;
use bevy::window::WindowResized;
use bevy_brp_extras::BrpExtrasPlugin;
use bevy_egui::EguiContexts;
use bevy_egui::EguiPlugin;
use bevy_egui::EguiPrimaryContextPass;
use bevy_egui::egui;
use bevy_lagrange::BlockOnEguiFocus;
use bevy_lagrange::LagrangePlugin;
use bevy_lagrange::OrbitCam;
use bevy_lagrange::TrackpadBehavior;

fn main() {
    let mut app = App::new();
    app.add_plugins(DefaultPlugins)
        .add_plugins(EguiPlugin::default())
        .add_plugins(LagrangePlugin)
        .add_plugins(BrpExtrasPlugin::default())
        .add_systems(Startup, setup)
        .add_systems(Update, set_camera_viewports)
        .add_systems(EguiPrimaryContextPass, ui_system);

    app.run();
}

fn orbit_cam_default() -> OrbitCam {
    OrbitCam {
        trackpad_behavior: TrackpadBehavior::BlenderLike {
            modifier_pan:  Some(KeyCode::ShiftLeft),
            modifier_zoom: Some(KeyCode::ControlLeft),
        },
        trackpad_pinch_to_zoom_enabled: true,
        ..default()
    }
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

    // Left camera — blocked by egui focus (opt-in)
    commands.spawn((
        Transform::from_translation(Vec3::new(0.0, 1.5, 5.0)),
        orbit_cam_default(),
        BlockOnEguiFocus,
        LeftCamera,
    ));

    // Right camera — no blocking, always receives input
    commands.spawn((
        Transform::from_translation(Vec3::new(2.0, 2.0, 5.0)),
        Camera {
            order: 1,
            clear_color: ClearColorConfig::None,
            ..default()
        },
        orbit_cam_default(),
        RightCamera,
    ));
}

#[derive(Component)]
struct LeftCamera;

#[derive(Component)]
struct RightCamera;

#[allow(clippy::unwrap_used)]
fn set_camera_viewports(
    windows: Query<&Window>,
    mut resize_events: MessageReader<WindowResized>,
    mut left_camera: Query<&mut Camera, (With<LeftCamera>, Without<RightCamera>)>,
    mut right_camera: Query<&mut Camera, (With<RightCamera>, Without<LeftCamera>)>,
) {
    for resize_event in resize_events.read() {
        let window = windows.get(resize_event.window).unwrap();
        let half_width = window.resolution.physical_width() / 2;
        let height = window.resolution.physical_height();

        let mut left = left_camera.single_mut().unwrap();
        left.viewport = Some(Viewport {
            physical_position: UVec2::ZERO,
            physical_size: UVec2::new(half_width, height),
            ..default()
        });

        let mut right = right_camera.single_mut().unwrap();
        right.viewport = Some(Viewport {
            physical_position: UVec2::new(half_width, 0),
            physical_size: UVec2::new(half_width, height),
            ..default()
        });
    }
}

#[allow(clippy::cast_precision_loss)]
fn ui_system(mut contexts: EguiContexts, windows: Query<&Window>) -> Result {
    let ctx = contexts.ctx_mut()?;
    let window_width = windows.single().ok().map_or(1280.0, |w| w.width() as f32);
    let half = window_width / 2.0;

    // Floating window pinned to the left viewport
    egui::Window::new("Left — Blocked")
        .fixed_pos(egui::pos2(20.0, 20.0))
        .resizable(false)
        .show(ctx, |ui| {
            ui.label("BlockOnEguiFocus");
            ui.separator();
            ui.label("Hover here and try to orbit.");
            ui.label("The left camera won't respond.");
        });

    // Floating window pinned to the right viewport
    egui::Window::new("Right — Not Blocked")
        .fixed_pos(egui::pos2(half + 20.0, 20.0))
        .resizable(false)
        .show(ctx, |ui| {
            ui.label("No blocking component");
            ui.separator();
            ui.label("Hover here and try to orbit.");
            ui.label("The right camera still responds.");
        });

    Ok(())
}
