//! Demonstrates multiple viewports in a single window and multiple windows,
//! each with an independent `OrbitCam`.
//!
//! The primary window has a full-size view and a minimap overlay in the
//! top-right corner. A second OS window shows a separate camera angle.

use bevy::camera::RenderTarget;
use bevy::camera::Viewport;
use bevy::prelude::*;
use bevy::window::ClosingWindow;
use bevy::window::WindowRef;
use bevy::window::WindowResized;
use bevy_brp_extras::BrpExtrasPlugin;
use bevy_lagrange::InputControl;
use bevy_lagrange::LagrangePlugin;
use bevy_lagrange::OrbitCam;
use bevy_lagrange::TrackpadInput;
use bevy_window_manager::ManagedWindow;
use bevy_window_manager::WindowManagerPlugin;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(LagrangePlugin)
        .add_plugins(BrpExtrasPlugin::default())
        .add_plugins(WindowManagerPlugin)
        .add_systems(Startup, setup)
        .add_systems(
            Update,
            (cleanup_cameras_on_window_close, set_camera_viewports),
        )
        .run();
}

fn pan_orbit_default() -> OrbitCam {
    OrbitCam {
        input_control: Some(InputControl {
            trackpad: Some(TrackpadInput::blender_default()),
            ..default()
        }),
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

    // --- Primary window: main camera ---
    commands.spawn((
        Transform::from_translation(Vec3::new(0.0, 0.5, 5.0)),
        pan_orbit_default(),
    ));

    // --- Primary window: minimap viewport overlay ---
    commands.spawn((
        Transform::from_translation(Vec3::new(1.0, 1.5, 4.0)),
        Camera {
            order: 1,
            clear_color: ClearColorConfig::None,
            ..default()
        },
        pan_orbit_default(),
        MinimapCamera,
    ));

    // --- Second OS window ---
    let second_window = commands
        .spawn((
            Window {
                title: "Second window".to_owned(),
                ..default()
            },
            ManagedWindow {
                name: "second_window".into(),
            },
        ))
        .id();

    commands.spawn((
        Transform::from_translation(Vec3::new(5.0, 1.5, 7.0)),
        Camera::default(),
        RenderTarget::Window(WindowRef::Entity(second_window)),
        pan_orbit_default(),
    ));
}

#[derive(Component)]
struct MinimapCamera;

/// Despawns cameras whose render-target window is marked `ClosingWindow`.
/// Prevents `camera_system` from panicking on a stale `RenderTarget`.
fn cleanup_cameras_on_window_close(
    mut commands: Commands,
    closing: Query<Entity, With<ClosingWindow>>,
    cameras: Query<(Entity, &RenderTarget)>,
) {
    for (cam_entity, target) in &cameras {
        if let RenderTarget::Window(WindowRef::Entity(window)) = target
            && closing.get(*window).is_ok()
        {
            commands.entity(cam_entity).despawn();
        }
    }
}

fn set_camera_viewports(
    windows: Query<&Window>,
    mut resize_events: MessageReader<WindowResized>,
    mut right_camera: Single<&mut Camera, With<MinimapCamera>>,
) {
    for resize_event in resize_events.read() {
        let Ok(window) = windows.get(resize_event.window) else {
            continue;
        };
        let size = window.resolution.physical_width() / 5;
        right_camera.viewport = Some(Viewport {
            physical_position: UVec2::new(window.resolution.physical_width() - size, 0),
            physical_size: UVec2::new(size, size),
            ..default()
        });
    }
}
