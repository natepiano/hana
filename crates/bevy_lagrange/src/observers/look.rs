use std::time::Duration;

use bevy::prelude::*;
use bevy_kana::Displacement;
use bevy_kana::Position;

use super::constants::LOOK_AT_AND_ZOOM_TO_FIT_CONTEXT;
use super::constants::LOOK_AT_AND_ZOOM_TO_FIT_LOOK_FRACTION;
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

    let camera_position = camera_transform.translation();
    let target_position = target_transform.translation();

    if duration > Duration::ZERO {
        commands.trigger(
            PlayAnimation::new(
                camera,
                [CameraMove::ToPosition {
                    translation: camera_position,
                    focus: target_position,
                    duration,
                    easing,
                }],
            )
            .source(AnimationSource::LookAt),
        );
    } else {
        let (yaw, pitch, radius) =
            animation::orbital_params_from_offset(Displacement(camera_position - target_position));
        snap_orbit::snap_to_orbit(
            &mut commands,
            &mut orbit_cam,
            SnapOrbit {
                focus: Position(target_position),
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

/// Observer for `LookAtAndZoomToFit` event — first rotates the camera in place to
/// face the target, then frames the target from that look direction.
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

    let camera_position = camera_transform.translation();

    let Ok(target_global_transform) = global_transform_query.get(target) else {
        warn!("LookAtAndZoomToFit: target {target:?} has no GlobalTransform");
        return;
    };
    let target_position = target_global_transform.translation();
    let (yaw, pitch, _) =
        animation::orbital_params_from_offset(Displacement(camera_position - target_position));

    let Some(fit) = fit_request::prepare_fit_for_target(
        &FitRequest {
            context: LOOK_AT_AND_ZOOM_TO_FIT_CONTEXT,
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
        let look_duration = duration.mul_f32(LOOK_AT_AND_ZOOM_TO_FIT_LOOK_FRACTION);
        let fit_duration = duration.saturating_sub(look_duration);
        commands.trigger(
            PlayAnimation::new(
                camera,
                [
                    CameraMove::ToPosition {
                        translation: camera_position,
                        focus: target_position,
                        duration: look_duration,
                        easing,
                    },
                    CameraMove::ToOrbit {
                        focus: *fit.focus,
                        yaw,
                        pitch,
                        radius: fit.radius,
                        duration: fit_duration,
                        easing,
                    },
                ],
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::animation::AnimationSourceMarker;
    use crate::animation::CameraMoveList;
    use crate::animation::ZoomAnimationMarker;
    use crate::components::CurrentFitTarget;
    use crate::events::ZoomBegin;
    use crate::events::ZoomEnd;
    use crate::observers::ObserverPlugin;

    type TestResult = Result<(), &'static str>;

    const TEST_DURATION: Duration = Duration::from_secs(1);
    const TEST_MARGIN: f32 = 0.15;
    const TEST_CAMERA_POSITION: Vec3 = Vec3::new(0.0, 1.5, 3.0);
    const TEST_TARGET_POSITION: Vec3 = Vec3::new(3.5, 0.5, 0.0);
    const EPSILON: f32 = 0.000_001;

    #[derive(Resource, Default)]
    struct ZoomEventCounts {
        begin: usize,
        end:   usize,
    }

    fn count_zoom_begin(_: On<ZoomBegin>, mut counts: ResMut<ZoomEventCounts>) {
        counts.begin += 1;
    }

    fn count_zoom_end(_: On<ZoomEnd>, mut counts: ResMut<ZoomEventCounts>) { counts.end += 1; }

    fn assert_f32_close(actual: f32, expected: f32) {
        assert!((actual - expected).abs() <= EPSILON);
    }

    fn test_app() -> App {
        let mut app = App::new();
        app.init_resource::<Assets<Mesh>>()
            .init_resource::<ZoomEventCounts>()
            .add_plugins(ObserverPlugin)
            .add_observer(count_zoom_begin)
            .add_observer(count_zoom_end);
        app
    }

    fn spawn_target(app: &mut App) -> Entity {
        let mesh = app
            .world_mut()
            .resource_mut::<Assets<Mesh>>()
            .add(Cuboid::new(1.0, 1.0, 1.0));
        app.world_mut()
            .spawn((
                Mesh3d(mesh),
                Transform::from_translation(TEST_TARGET_POSITION),
                GlobalTransform::from(Transform::from_translation(TEST_TARGET_POSITION)),
            ))
            .id()
    }

    fn spawn_camera(app: &mut App) -> Entity {
        app.world_mut()
            .spawn((
                OrbitCam::default(),
                Projection::Perspective(PerspectiveProjection::default()),
                Camera::default(),
                Transform::from_translation(TEST_CAMERA_POSITION),
                GlobalTransform::from(Transform::from_translation(TEST_CAMERA_POSITION)),
            ))
            .id()
    }

    #[test]
    fn look_at_and_zoom_to_fit_queues_look_then_fit_without_zoom_events() -> TestResult {
        let mut app = test_app();
        let target = spawn_target(&mut app);
        let camera = spawn_camera(&mut app);

        app.world_mut().entity_mut(camera).trigger(|_| {
            LookAtAndZoomToFit::new(camera, target)
                .margin(TEST_MARGIN)
                .duration(TEST_DURATION)
        });
        app.update();

        let Some(queue) = app.world().get::<CameraMoveList>(camera) else {
            return Err("LookAtAndZoomToFit should create a camera move queue");
        };
        let mut moves = queue.camera_moves.iter();
        let Some(look_move) = moves.next() else {
            return Err("LookAtAndZoomToFit should queue a look-at move");
        };
        let Some(fit_move) = moves.next() else {
            return Err("LookAtAndZoomToFit should queue a fit move");
        };
        assert!(moves.next().is_none());

        match look_move {
            CameraMove::ToPosition {
                translation,
                focus,
                duration,
                ..
            } => {
                assert_eq!(*translation, TEST_CAMERA_POSITION);
                assert_eq!(*focus, TEST_TARGET_POSITION);
                assert_eq!(
                    *duration,
                    TEST_DURATION.mul_f32(LOOK_AT_AND_ZOOM_TO_FIT_LOOK_FRACTION)
                );
            },
            CameraMove::ToOrbit { .. } => {
                return Err("first LookAtAndZoomToFit move should look at the target");
            },
        }

        let (expected_yaw, expected_pitch, _) = animation::orbital_params_from_offset(
            Displacement(TEST_CAMERA_POSITION - TEST_TARGET_POSITION),
        );
        match fit_move {
            CameraMove::ToOrbit {
                yaw,
                pitch,
                duration,
                ..
            } => {
                assert_f32_close(*yaw, expected_yaw);
                assert_f32_close(*pitch, expected_pitch);
                assert_eq!(
                    *duration,
                    TEST_DURATION.saturating_sub(
                        TEST_DURATION.mul_f32(LOOK_AT_AND_ZOOM_TO_FIT_LOOK_FRACTION),
                    )
                );
            },
            CameraMove::ToPosition { .. } => {
                return Err("second LookAtAndZoomToFit move should fit from the look direction");
            },
        }

        assert_eq!(
            app.world()
                .get::<AnimationSourceMarker>(camera)
                .map(|source| source.0),
            Some(AnimationSource::LookAtAndZoomToFit)
        );
        assert!(app.world().get::<ZoomAnimationMarker>(camera).is_none());
        let Some(current_target) = app.world().get::<CurrentFitTarget>(camera) else {
            return Err("LookAtAndZoomToFit should update the current fit target");
        };
        assert_eq!(current_target.0, target);

        let counts = app.world().resource::<ZoomEventCounts>();
        assert_eq!(counts.begin, 0);
        assert_eq!(counts.end, 0);

        Ok(())
    }
}
