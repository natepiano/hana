use bevy::prelude::*;
use bevy_kana::Position;

use crate::orbit_cam::ForceUpdate;
use crate::orbit_cam::OrbitCam;

/// Parameters for an instant orbital snap.
pub(super) struct SnapOrbit {
    pub(super) focus:  Position,
    pub(super) yaw:    Option<f32>,
    pub(super) pitch:  Option<f32>,
    pub(super) radius: f32,
}

/// Snaps the camera to an orbital position instantly (no animation) and fires
/// caller-provided lifecycle events via `emit_events`.
pub(super) fn snap_to_orbit(
    commands: &mut Commands,
    orbit_cam: &mut OrbitCam,
    snap: SnapOrbit,
    emit_events: impl FnOnce(&mut Commands),
) {
    orbit_cam.focus = *snap.focus;
    orbit_cam.radius = Some(snap.radius);
    orbit_cam.target_focus = *snap.focus;
    orbit_cam.target_radius = snap.radius;
    if let Some(yaw) = snap.yaw {
        orbit_cam.yaw = Some(yaw);
        orbit_cam.target_yaw = yaw;
    }
    if let Some(pitch) = snap.pitch {
        orbit_cam.pitch = Some(pitch);
        orbit_cam.target_pitch = pitch;
    }
    orbit_cam.force_update = ForceUpdate::Pending;

    emit_events(commands);
}
