//! Demonstrates app-authored camera intent through `OrbitCamManualInputWriter`.

mod scene_setup;

use bevy::prelude::*;
use bevy_lagrange::CameraInteractionSources;
use bevy_lagrange::ManualInputSource;
use bevy_lagrange::OrbitCamInputMode;
use bevy_lagrange::OrbitCamInputPhase;
use bevy_lagrange::OrbitCamInteractionKind;
use bevy_lagrange::OrbitCamManualInputWriter;
use fairy_dust::CameraGuidance;
use fairy_dust::CameraGuidanceRow;

const KEYBOARD_ZOOM_LABEL: &str = "+ / -";
const MANUAL_ZOOM_LABEL: &str = "M";
const ORBIT_LABEL: &str = "Arrows";
const ORBIT_PIXELS: f32 = 8.0;
const PAN_LABEL: &str = "WASD";
const PAN_PIXELS: f32 = 6.0;
const ZOOM_AMOUNT: f32 = 0.08;

#[derive(Component)]
struct ManualCamera;

fn manual_guidance() -> CameraGuidance {
    CameraGuidance::custom([
        CameraGuidanceRow::new(OrbitCamInteractionKind::Orbit, ORBIT_LABEL)
            .with_camera_interaction_sources(CameraInteractionSources::KEYBOARD),
        CameraGuidanceRow::new(OrbitCamInteractionKind::Pan, PAN_LABEL)
            .with_camera_interaction_sources(CameraInteractionSources::KEYBOARD),
        CameraGuidanceRow::new(OrbitCamInteractionKind::Zoom, KEYBOARD_ZOOM_LABEL)
            .with_camera_interaction_sources(CameraInteractionSources::KEYBOARD),
        CameraGuidanceRow::new(OrbitCamInteractionKind::Zoom, MANUAL_ZOOM_LABEL)
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

        let orbit = signed_vec2(
            &keys,
            (KeyCode::ArrowRight, KeyCode::ArrowLeft),
            (KeyCode::ArrowUp, KeyCode::ArrowDown),
            ORBIT_PIXELS,
        );

        let pan = signed_vec2(
            &keys,
            (KeyCode::KeyD, KeyCode::KeyA),
            (KeyCode::KeyW, KeyCode::KeyS),
            PAN_PIXELS,
        );

        let zoom = signed_axis(&keys, KeyCode::Equal, KeyCode::Minus, ZOOM_AMOUNT);

        if orbit.is_none() && pan.is_none() && zoom.is_none() {
            continue;
        }

        let Ok(mut input) = writer.get_mut(camera, ManualInputSource::observed_keyboard()) else {
            continue;
        };
        if let Some(orbit) = orbit {
            input.orbit_pixels(orbit);
        }
        if let Some(pan) = pan {
            input.pan_pixels(pan);
        }
        if let Some(zoom) = zoom {
            input.zoom_smooth_amount(zoom);
        }
    }
}

fn signed_axis(
    keys: &ButtonInput<KeyCode>,
    positive: KeyCode,
    negative: KeyCode,
    amount: f32,
) -> Option<f32> {
    match (keys.pressed(positive), keys.pressed(negative)) {
        (true, false) => Some(amount),
        (false, true) => Some(-amount),
        (true, true) => Some(0.0),
        (false, false) => None,
    }
}

fn signed_vec2(
    keys: &ButtonInput<KeyCode>,
    x_axis: (KeyCode, KeyCode),
    y_axis: (KeyCode, KeyCode),
    amount: f32,
) -> Option<Vec2> {
    let x = signed_axis(keys, x_axis.0, x_axis.1, amount);
    let y = signed_axis(keys, y_axis.0, y_axis.1, amount);
    match (x, y) {
        (None, None) => None,
        _ => Some(Vec2::new(x.unwrap_or(0.0), y.unwrap_or(0.0))),
    }
}

fn main() {
    fairy_dust::sprinkle_example()
        .with_orbit_cam(
            scene_setup::configure_camera,
            (OrbitCamInputMode::Manual, ManualCamera, manual_guidance()),
        )
        .with_camera_control_panel()
        .add_systems(Startup, scene_setup::spawn_scene)
        .add_systems(
            PreUpdate,
            write_manual_input.in_set(OrbitCamInputPhase::WriteManual),
        )
        .run();
}
