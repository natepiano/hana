use std::time::Duration;

use bevy::prelude::*;
use bevy_kana::Displacement;
use bevy_kana::Position;

use super::fit_request;
use super::fit_request::FitRequest;
use super::snap_orbit;
use super::snap_orbit::SnapOrbit;
use crate::animation;
use crate::animation::CameraMove;
use crate::events::AnimationBegin;
use crate::events::AnimationEnd;
use crate::events::AnimationSource;
use crate::events::LookAt;
use crate::events::LookAtAndZoomToFit;
use crate::events::PlayAnimation;
use crate::events::SetFitTarget;
use crate::orbit_cam::OrbitCam;

/// Observer for `LookAt` event — rotates the camera in place to look at a target entity.
/// The camera stays at its current world position; only the orbit pivot re-anchors.
pub(super) fn on_look_at(
    event: On<LookAt>,
    mut commands: Commands,
    mut camera_query: Query<(&mut OrbitCam, &GlobalTransform)>,
    global_transform_query: Query<&GlobalTransform>,
) {
    let camera = event.camera;
    let target = event.target;
    let duration = event.duration;
    let easing = event.easing;

    let Ok((mut orbit_cam, camera_transform)) = camera_query.get_mut(camera) else {
        return;
    };

    let Ok(target_transform) = global_transform_query.get(target) else {
        warn!("LookAt: target {target:?} has no GlobalTransform");
        return;
    };

    let camera_pos = camera_transform.translation();
    let target_pos = target_transform.translation();

    if duration > Duration::ZERO {
        commands.trigger(
            PlayAnimation::new(
                camera,
                [CameraMove::ToPosition {
                    translation: camera_pos,
                    focus: target_pos,
                    duration,
                    easing,
                }],
            )
            .source(AnimationSource::LookAt),
        );
    } else {
        let (yaw, pitch, radius) =
            animation::orbital_params_from_offset(Displacement(camera_pos - target_pos));
        snap_orbit::snap_to_orbit(
            &mut commands,
            &mut orbit_cam,
            SnapOrbit {
                focus: Position(target_pos),
                yaw: Some(yaw),
                pitch: Some(pitch),
                radius,
            },
            |commands| {
                let source = AnimationSource::LookAt;
                commands.trigger(AnimationBegin { camera, source });
                commands.trigger(AnimationEnd { camera, source });
            },
        );
    }
}

/// Observer for `LookAtAndZoomToFit` event — rotates the camera in place to look at
/// a target entity and adjusts the radius to frame it, all in one fluid motion.
/// The yaw and pitch are back-solved from the camera's current world position.
pub(super) fn on_look_at_and_zoom_to_fit(
    event: On<LookAtAndZoomToFit>,
    mut commands: Commands,
    mut camera_query: Query<(&mut OrbitCam, &Projection, &Camera, &GlobalTransform)>,
    mesh_query: Query<&Mesh3d>,
    children_query: Query<&Children>,
    global_transform_query: Query<&GlobalTransform>,
    meshes: Res<Assets<Mesh>>,
) {
    let camera = event.camera;
    let target = event.target;
    let margin = event.margin;
    let duration = event.duration;
    let easing = event.easing;

    let Ok((mut orbit_cam, projection, camera_component, camera_transform)) =
        camera_query.get_mut(camera)
    else {
        return;
    };

    let camera_pos = camera_transform.translation();

    // Back-solve yaw/pitch from camera's current position relative to the target.
    // We need the target's bounds center for this, so we run the fit calculation
    // with a preliminary yaw/pitch, then refine.
    let Ok(target_gt) = global_transform_query.get(target) else {
        warn!("LookAtAndZoomToFit: target {target:?} has no GlobalTransform");
        return;
    };
    let target_pos = target_gt.translation();
    let (preliminary_yaw, preliminary_pitch, _) =
        animation::orbital_params_from_offset(Displacement(camera_pos - target_pos));

    let Some(fit) = fit_request::prepare_fit_for_target(
        &FitRequest {
            context: "LookAtAndZoomToFit",
            target,
            yaw: preliminary_yaw,
            pitch: preliminary_pitch,
            margin,
            projection,
            camera: camera_component,
        },
        &mesh_query,
        &children_query,
        &global_transform_query,
        &meshes,
    ) else {
        return;
    };

    // Recompute yaw/pitch relative to the fit's focus (bounds center), which may
    // differ slightly from the raw `GlobalTransform` translation.
    let (yaw, pitch, _) =
        animation::orbital_params_from_offset(Displacement(camera_pos - *fit.focus));

    if duration > Duration::ZERO {
        commands.trigger(
            PlayAnimation::new(
                camera,
                [CameraMove::ToOrbit {
                    focus: *fit.focus,
                    yaw,
                    pitch,
                    radius: fit.radius,
                    duration,
                    easing,
                }],
            )
            .source(AnimationSource::LookAtAndZoomToFit),
        );
    } else {
        snap_orbit::snap_to_orbit(
            &mut commands,
            &mut orbit_cam,
            SnapOrbit {
                focus:  fit.focus,
                yaw:    Some(yaw),
                pitch:  Some(pitch),
                radius: fit.radius,
            },
            |commands| {
                let source = AnimationSource::LookAtAndZoomToFit;
                commands.trigger(AnimationBegin { camera, source });
                commands.trigger(AnimationEnd { camera, source });
            },
        );
    }

    commands.trigger(SetFitTarget::new(camera, target));
}
