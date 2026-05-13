//! Demonstrates app-authored camera intent through `OrbitCamManualInputWriter`.

mod common;

use bevy::prelude::*;
use bevy_lagrange::CameraInteractionSources;
use bevy_lagrange::ManualInputSource;
use bevy_lagrange::OrbitCamInputPhase;
use bevy_lagrange::OrbitCamInteractionKind;
use bevy_lagrange::OrbitCamManual;
use bevy_lagrange::OrbitCamManualInputWriter;
use fairy_dust::CameraGuidance;
use fairy_dust::CameraGuidanceRow;

const ORBIT_PIXELS: f32 = 8.0;
const PAN_PIXELS: f32 = 6.0;
const ZOOM_AMOUNT: f32 = 0.08;

#[derive(Component)]
struct ManualCamera;

fn manual_guidance() -> CameraGuidance {
    CameraGuidance::custom([
        CameraGuidanceRow::new(OrbitCamInteractionKind::Orbit, "Arrows")
            .with_camera_interaction_sources(CameraInteractionSources::KEYBOARD),
        CameraGuidanceRow::new(OrbitCamInteractionKind::Pan, "WASD")
            .with_camera_interaction_sources(CameraInteractionSources::KEYBOARD),
        CameraGuidanceRow::new(OrbitCamInteractionKind::Zoom, "+ / -")
            .with_camera_interaction_sources(CameraInteractionSources::KEYBOARD),
        CameraGuidanceRow::new(OrbitCamInteractionKind::Zoom, "M")
            .with_camera_interaction_sources(CameraInteractionSources::MANUAL),
    ])
}

fn write_manual_input(
    keys: Res<ButtonInput<KeyCode>>,
    cameras: Query<Entity, With<ManualCamera>>,
    mut writer: OrbitCamManualInputWriter,
) {
    for camera in &cameras {
        if keys.pressed(KeyCode::KeyM)
            && let Ok(mut input) = writer.get_mut(camera, ManualInputSource::manual())
        {
            input.zoom_active();
        }

        let (orbit, has_orbit) = signed_vec2(
            (
                keys.pressed(KeyCode::ArrowRight),
                keys.pressed(KeyCode::ArrowLeft),
            ),
            (
                keys.pressed(KeyCode::ArrowUp),
                keys.pressed(KeyCode::ArrowDown),
            ),
            ORBIT_PIXELS,
        );

        let (pan, has_pan) = signed_vec2(
            (keys.pressed(KeyCode::KeyD), keys.pressed(KeyCode::KeyA)),
            (keys.pressed(KeyCode::KeyW), keys.pressed(KeyCode::KeyS)),
            PAN_PIXELS,
        );

        let (zoom, has_zoom) = signed_axis(
            keys.pressed(KeyCode::Equal),
            keys.pressed(KeyCode::Minus),
            ZOOM_AMOUNT,
        );

        if !has_orbit && !has_pan && !has_zoom {
            continue;
        }

        let Ok(mut input) = writer.get_mut(camera, ManualInputSource::observed_keyboard()) else {
            continue;
        };
        if has_orbit {
            input.orbit_pixels(orbit);
        }
        if has_pan {
            input.pan_pixels(pan);
        }
        if has_zoom {
            input.zoom_smooth_amount(zoom);
        }
    }
}

const fn signed_axis(positive: bool, negative: bool, amount: f32) -> (f32, bool) {
    match (positive, negative) {
        (true, false) => (amount, true),
        (false, true) => (-amount, true),
        (true, true) => (0.0, true),
        (false, false) => (0.0, false),
    }
}

const fn signed_vec2(x_axis: (bool, bool), y_axis: (bool, bool), amount: f32) -> (Vec2, bool) {
    let (x, x_active) = signed_axis(x_axis.0, x_axis.1, amount);
    let (y, y_active) = signed_axis(y_axis.0, y_axis.1, amount);
    (Vec2::new(x, y), x_active || y_active)
}

fn main() {
    fairy_dust::sprinkle_example()
        .with_orbit_cam_bundle(
            common::configure_camera,
            (OrbitCamManual, ManualCamera, manual_guidance()),
        )
        .with_camera_guidance_panel()
        .add_systems(Startup, common::spawn_scene)
        .add_systems(
            PreUpdate,
            write_manual_input.in_set(OrbitCamInputPhase::WriteManual),
        )
        .run();
}
