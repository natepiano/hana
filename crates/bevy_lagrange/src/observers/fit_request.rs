use bevy::prelude::*;

use crate::fit;
use crate::projection;

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

/// Shared fit preparation used by both `ZoomToFit` and `AnimateToFit` observers.
/// Extracts target mesh vertices and computes the fit solution for the requested
/// camera orientation.
pub(super) fn prepare_fit_for_target(
    request: &FitRequest,
    mesh_query: &Query<&Mesh3d>,
    children_query: &Query<&Children>,
    global_transform_query: &Query<&GlobalTransform>,
    meshes: &Assets<Mesh>,
) -> Option<fit::FitSolution> {
    let context = request.context;
    let target = request.target;
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
        request.yaw,
        request.pitch,
        request.margin,
        request.projection,
        request.camera,
    )
    .inspect_err(|error| {
        warn!("{context}: Failed to calculate fit for entity {target:?}: {error}");
    }) else {
        return None;
    };

    Some(fit)
}
