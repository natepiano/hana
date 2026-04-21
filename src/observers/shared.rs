use bevy::prelude::*;
use bevy_kana::Position;

use crate::fit;
use crate::orbit_cam::ForceUpdate;
use crate::orbit_cam::OrbitCam;
use crate::projection;

/// Parameters for an instant orbital snap.
pub(super) struct SnapOrbit {
    pub(super) focus:  Position,
    pub(super) yaw:    Option<f32>,
    pub(super) pitch:  Option<f32>,
    pub(super) radius: f32,
}

/// Parameters for a fit calculation request.
pub(super) struct FitRequest<'a> {
    pub(super) context:    &'a str,
    pub(super) target:     Entity,
    pub(super) yaw:        f32,
    pub(super) pitch:      f32,
    pub(super) margin:     f32,
    pub(super) projection: &'a Projection,
    pub(super) camera:     &'a Camera,
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

/// Shared fit preparation used by both `ZoomToFit` and `AnimateToFit` observers.
/// Extracts target mesh vertices and computes the fit solution for the requested
/// camera orientation.
pub(super) fn prepare_fit_for_target(
    req: &FitRequest,
    mesh_query: &Query<&Mesh3d>,
    children_query: &Query<&Children>,
    global_transform_query: &Query<&GlobalTransform>,
    meshes: &Assets<Mesh>,
) -> Option<fit::FitSolution> {
    let context = req.context;
    let target = req.target;
    let Some((vertices, geometric_center)) = projection::extract_mesh_vertices(
        target,
        children_query,
        mesh_query,
        global_transform_query,
        meshes,
    ) else {
        warn!("{context}: Failed to extract mesh vertices for entity {target:?}");
        return None;
    };

    let Ok(fit) = fit::calculate_fit(
        &vertices,
        geometric_center,
        req.yaw,
        req.pitch,
        req.margin,
        req.projection,
        req.camera,
    )
    .inspect_err(|error| {
        warn!("{context}: Failed to calculate fit for entity {target:?}: {error}");
    }) else {
        return None;
    };

    Some(fit)
}
