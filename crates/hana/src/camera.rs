use bevy::prelude::*;
use bevy_inspector_egui::{prelude::*, quick::ResourceInspectorPlugin};
use bevy_panorbit_camera::{PanOrbitCamera, PanOrbitCameraPlugin, TrackpadBehavior};

use crate::action::{just_pressed, toggle_active, Action};

pub struct CameraPlugin;

#[derive(Component, Debug)]
struct PrimaryCamera;

impl Plugin for CameraPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<CameraConfig>()
            .init_resource::<CameraConfig>()
            .add_plugins(
                ResourceInspectorPlugin::<CameraConfig>::default()
                    .run_if(toggle_active(false, Action::CameraInspect)),
            )
            .add_plugins(PanOrbitCameraPlugin)
            .add_systems(Startup, setup)
            .add_systems(Update, home_camera.run_if(just_pressed(Action::CameraHome)));
    }
}

fn setup(mut commands: Commands, camera_config: Res<CameraConfig>) {
    commands.spawn((
        PanOrbitCamera {
            allow_upside_down: true,
            button_orbit: MouseButton::Middle,
            button_pan: MouseButton::Middle,
            modifier_pan: Some(KeyCode::ShiftLeft),
            trackpad_sensitivity: 1.5,
            trackpad_behavior: TrackpadBehavior::BlenderLike {
                modifier_pan: Some(KeyCode::ShiftLeft),
                modifier_zoom: Some(KeyCode::ControlLeft),
            },
            trackpad_pinch_to_zoom_enabled: true,
            ..default()
        },
        camera_config.home_transform.clone(), // Transform::from_xyz(0., 1.5, 10.).looking_at(Vec3::ZERO, Vec3::Y),
        PrimaryCamera,
    ));
}

/// return camera to the home position
fn home_camera(
    camera_config: Res<CameraConfig>,
    mut camera_transform: Query<&mut Transform, With<PrimaryCamera>>,
) {
    if let Ok(mut transform) = camera_transform.get_single_mut() {
        *transform = camera_config.home_transform.clone();
    }
}

#[derive(Resource, Reflect, InspectorOptions, Debug, PartialEq, Clone, Copy)]
#[reflect(Resource, InspectorOptions)]
pub struct CameraConfig {
    pub home_transform: Transform,
}

impl Default for CameraConfig {
    fn default() -> Self {
        Self {
            home_transform: Transform::from_xyz(0.0, 2.5, 10.).looking_at(Vec3::ZERO, Vec3::Y),
        }
    }
}
