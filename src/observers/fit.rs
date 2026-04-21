use std::collections::VecDeque;
use std::time::Duration;

use bevy::prelude::*;

use super::shared;
use super::shared::FitRequest;
use super::shared::SnapOrbit;
use crate::animation::CameraMove;
use crate::components::CurrentFitTarget;
use crate::events::AnimateToFit;
use crate::events::AnimationBegin;
use crate::events::AnimationEnd;
use crate::events::AnimationSource;
use crate::events::PlayAnimation;
use crate::events::SetFitTarget;
use crate::events::ZoomBegin;
use crate::events::ZoomContext;
use crate::events::ZoomEnd;
use crate::events::ZoomToFit;
use crate::orbit_cam::OrbitCam;

/// Observer for `ZoomToFit` event - frames a target entity in the camera view.
/// When duration is `Duration::ZERO`, snaps instantly.
/// When duration is greater than zero, animates smoothly via [`PlayAnimation`]
/// with a [`ZoomContext`] so that `on_play_animation` handles all conflict
/// resolution and zoom lifecycle events in one place.
/// Requires target entity to have a `Mesh3d` (direct or on descendants).
pub(super) fn on_zoom_to_fit(
    zoom: On<ZoomToFit>,
    mut commands: Commands,
    mut camera_query: Query<(&mut OrbitCam, &Projection, &Camera)>,
    mesh_query: Query<&Mesh3d>,
    children_query: Query<&Children>,
    global_transform_query: Query<&GlobalTransform>,
    meshes: Res<Assets<Mesh>>,
) {
    let camera = zoom.camera;
    let target = zoom.target;
    let margin = zoom.margin;
    let duration = zoom.duration;
    let easing = zoom.easing;

    let Ok((mut orbit_cam, projection, camera_component)) = camera_query.get_mut(camera) else {
        return;
    };

    debug!(
        "ZoomToFit: yaw={:.3} pitch={:.3} current_focus={:.1?} current_radius={:.1} duration_ms={:.0}",
        orbit_cam.target_yaw,
        orbit_cam.target_pitch,
        orbit_cam.target_focus,
        orbit_cam.target_radius,
        duration.as_secs_f32() * 1000.0,
    );

    let Some(fit) = shared::prepare_fit_for_target(
        &FitRequest {
            context: "ZoomToFit",
            target,
            yaw: orbit_cam.target_yaw,
            pitch: orbit_cam.target_pitch,
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

    if duration > Duration::ZERO {
        // Animated path: use `ToOrbit` to pass orbital params directly, avoiding
        // gimbal lock from atan2 decomposition at extreme pitch angles.
        let camera_moves = VecDeque::from([CameraMove::ToOrbit {
            focus: *fit.focus,
            yaw: orbit_cam.target_yaw,
            pitch: orbit_cam.target_pitch,
            radius: fit.radius,
            duration,
            easing,
        }]);

        let ctx = ZoomContext {
            target,
            margin,
            duration,
            easing,
        };

        // `on_play_animation` handles conflict resolution, `ZoomBegin`, and
        // `ZoomAnimationMarker` insertion — all in one place after acceptance.
        commands.trigger(PlayAnimation::new(camera, camera_moves).zoom_context(ctx));
    } else {
        shared::snap_to_orbit(
            &mut commands,
            &mut orbit_cam,
            SnapOrbit {
                focus:  fit.focus,
                yaw:    None,
                pitch:  None,
                radius: fit.radius,
            },
            |commands| {
                commands.trigger(ZoomBegin {
                    camera,
                    target,
                    margin,
                    duration,
                    easing,
                });
                commands.trigger(ZoomEnd {
                    camera,
                    target,
                    margin,
                    duration: Duration::ZERO,
                    easing,
                });
            },
        );
    }

    // Route fit target updates through a single lifecycle owner.
    commands.trigger(SetFitTarget::new(camera, target));
}

/// Observer for `SetFitTarget` event - sets the target entity for fit debug overlay.
pub(super) fn on_set_fit_target(set_target: On<SetFitTarget>, mut commands: Commands) {
    commands
        .entity(set_target.camera)
        .insert(CurrentFitTarget(set_target.target));
}

/// Observer for `AnimateToFit` event - animates the camera to a specific orientation
/// while fitting a target entity in view.
pub(super) fn on_animate_to_fit(
    event: On<AnimateToFit>,
    mut commands: Commands,
    mut camera_query: Query<(&mut OrbitCam, &Projection, &Camera)>,
    mesh_query: Query<&Mesh3d>,
    children_query: Query<&Children>,
    global_transform_query: Query<&GlobalTransform>,
    meshes: Res<Assets<Mesh>>,
) {
    let camera = event.camera;
    let target = event.target;
    let yaw = event.yaw;
    let pitch = event.pitch;
    let margin = event.margin;
    let duration = event.duration;
    let easing = event.easing;

    let Ok((mut orbit_cam, projection, camera_component)) = camera_query.get_mut(camera) else {
        return;
    };

    let Some(fit) = shared::prepare_fit_for_target(
        &FitRequest {
            context: "AnimateToFit",
            target,
            yaw,
            pitch,
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

    if duration > Duration::ZERO {
        let camera_moves = VecDeque::from([CameraMove::ToOrbit {
            focus: *fit.focus,
            yaw,
            pitch,
            radius: fit.radius,
            duration,
            easing,
        }]);
        commands.trigger(
            PlayAnimation::new(camera, camera_moves).source(AnimationSource::AnimateToFit),
        );
    } else {
        shared::snap_to_orbit(
            &mut commands,
            &mut orbit_cam,
            SnapOrbit {
                focus:  fit.focus,
                yaw:    Some(yaw),
                pitch:  Some(pitch),
                radius: fit.radius,
            },
            |commands| {
                let source = AnimationSource::AnimateToFit;
                commands.trigger(AnimationBegin { camera, source });
                commands.trigger(AnimationEnd { camera, source });
            },
        );
    }
    // Route fit target updates through a single lifecycle owner.
    commands.trigger(SetFitTarget::new(camera, target));
}
