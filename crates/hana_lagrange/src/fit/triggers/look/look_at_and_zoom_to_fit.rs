use std::time::Duration;

use bevy::math::curve::easing::EaseFunction;
use bevy::prelude::*;

use super::plan::LookAtPlan;
use super::support;
use crate::CameraBasis;
use crate::animation::AnimationSource;
use crate::animation::CameraMove;
use crate::fit::camera_pose;
use crate::fit::camera_pose::FreeCamFitPose;
use crate::fit::camera_pose::SnapOrbit;
use crate::fit::constants::DEFAULT_FIT_MARGIN;
use crate::fit::constants::LOOK_AT_AND_ZOOM_TO_FIT_CONTEXT;
use crate::fit::constants::LOOK_AT_AND_ZOOM_TO_FIT_LOOK_FRACTION;
use crate::fit::geometry::FitAnchor;
use crate::fit::target::SetFitTarget;
use crate::fit::triggers::request;
use crate::fit::triggers::request::FitRequest;
use crate::free_cam::FreeCam;
use crate::orbit_cam::OrbitCam;

/// Rotates the camera to face a target entity and frames it in view.
#[derive(EntityEvent, Reflect)]
#[reflect(Event, FromReflect)]
pub struct LookAtAndZoomToFit {
    /// The camera entity.
    #[event_target]
    pub camera:   Entity,
    /// The entity to frame.
    pub target:   Entity,
    /// Fraction of screen to leave as margin.
    pub margin:   f32,
    /// Animation duration (`ZERO` for instant).
    pub duration: Duration,
    /// Easing curve for the animation.
    pub easing:   EaseFunction,
}

impl LookAtAndZoomToFit {
    /// Creates a new `LookAtAndZoomToFit` with default parameters.
    #[must_use]
    pub const fn new(camera: Entity, target: Entity) -> Self {
        Self {
            camera,
            target,
            margin: DEFAULT_FIT_MARGIN,
            duration: Duration::ZERO,
            easing: EaseFunction::CubicOut,
        }
    }

    /// Sets the margin.
    #[must_use]
    pub const fn margin(mut self, margin: f32) -> Self {
        self.margin = margin;
        self
    }

    /// Sets the animation duration.
    #[must_use]
    pub const fn duration(mut self, duration: Duration) -> Self {
        self.duration = duration;
        self
    }

    /// Sets the easing function.
    #[must_use]
    pub const fn easing(mut self, easing: EaseFunction) -> Self {
        self.easing = easing;
        self
    }
}

/// Observer for `LookAtAndZoomToFit` event — first rotates the camera in place to
/// face the target, then frames the target from that look direction.
pub(crate) fn on_orbit_cam_look_at_and_zoom_to_fit(
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
    let plan = LookAtPlan::from_world_positions(camera_position, target_position);

    let Some(fit) = request::prepare_fit_for_target(
        &FitRequest {
            context: LOOK_AT_AND_ZOOM_TO_FIT_CONTEXT,
            target,
            yaw: plan.yaw,
            pitch: plan.pitch,
            margin,
            anchor: FitAnchor::Center,
            offset_px: Vec2::ZERO,
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
        support::trigger_timed_animation(
            &mut commands,
            camera,
            target,
            AnimationSource::LookAtAndZoomToFit,
            [
                plan.to_look_move(None, look_duration, easing),
                CameraMove::ToOrbitalLookAt {
                    target: *fit.focus,
                    yaw: plan.yaw,
                    pitch: plan.pitch,
                    radius: fit.radius,
                    roll: None,
                    duration: fit_duration,
                    easing,
                },
            ],
        );
    } else {
        camera_pose::snap_to_orbit(
            &mut commands,
            &mut orbit_cam,
            SnapOrbit {
                focus:  fit.focus,
                yaw:    Some(plan.yaw),
                pitch:  Some(plan.pitch),
                radius: fit.radius,
            },
            |commands| {
                support::trigger_completed_animation(
                    commands,
                    camera,
                    target,
                    AnimationSource::LookAtAndZoomToFit,
                );
            },
        );
    }

    commands.trigger(SetFitTarget::new(camera, target));
}

/// Observer for `LookAtAndZoomToFit` on `FreeCam`: turns toward the target,
/// then fits the target from that look direction while preserving roll.
pub(crate) fn on_free_cam_look_at_and_zoom_to_fit(
    event: On<LookAtAndZoomToFit>,
    mut commands: Commands,
    mut camera_query: Query<(
        &mut FreeCam,
        &mut Projection,
        &Camera,
        &CameraBasis,
        &Transform,
    )>,
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

    let Ok((mut free_cam, mut projection, camera_component, basis, transform)) =
        camera_query.get_mut(camera)
    else {
        return;
    };

    let Ok(target_global_transform) = global_transform_query.get(target) else {
        warn!("LookAtAndZoomToFit: target {target:?} has no GlobalTransform");
        return;
    };
    let start = FreeCamFitPose::from_free_cam_or_transform(&free_cam, transform, *basis);
    let target_position = target_global_transform.translation();
    let plan = LookAtPlan::from_free_camera(start.position.0, target_position, *basis);

    let Some(fit) = request::prepare_fit_for_target(
        &FitRequest {
            context: LOOK_AT_AND_ZOOM_TO_FIT_CONTEXT,
            target,
            yaw: plan.yaw,
            pitch: plan.pitch,
            margin,
            anchor: FitAnchor::Center,
            offset_px: Vec2::ZERO,
            projection: &projection,
            camera: camera_component,
        },
        &mesh_query,
        &children_query,
        &global_transform_query,
        &meshes,
    ) else {
        return;
    };

    let target_pose =
        FreeCamFitPose::from_fit(fit, &projection, *basis, plan.look_angles(), start.roll);
    camera_pose::sync_free_cam_projection(&mut projection, fit);
    if duration > Duration::ZERO {
        let look_duration = duration.mul_f32(LOOK_AT_AND_ZOOM_TO_FIT_LOOK_FRACTION);
        let fit_duration = duration.saturating_sub(look_duration);
        let roll = Some(start.roll);
        support::trigger_timed_animation(
            &mut commands,
            camera,
            target,
            AnimationSource::LookAtAndZoomToFit,
            [
                plan.to_look_move(roll, look_duration, easing),
                CameraMove::ToOrbitalLookAt {
                    target: *fit.focus,
                    yaw: plan.yaw,
                    pitch: plan.pitch,
                    radius: fit.radius,
                    roll,
                    duration: fit_duration,
                    easing,
                },
            ],
        );
    } else {
        camera_pose::apply_free_cam_pose(&mut free_cam, target_pose);
        support::trigger_completed_animation(
            &mut commands,
            camera,
            target,
            AnimationSource::LookAtAndZoomToFit,
        );
    }

    commands.trigger(SetFitTarget::new(camera, target));
}

#[cfg(test)]
mod tests {
    use bevy_kana::Displacement;

    use super::*;
    use crate::CurrentFitTarget;
    use crate::animation;
    use crate::animation::AnimationPlugin;
    use crate::animation::AnimationSourceMarker;
    use crate::animation::CameraMoveList;
    use crate::animation::ZoomAnimationMarker;
    use crate::fit::FitPlugin;
    use crate::fit::ZoomBegin;
    use crate::fit::ZoomEnd;
    use crate::fit::triggers::look::look_at;
    use crate::operation::LookAngles;
    use crate::operation::Roll;

    type TestResult = Result<(), &'static str>;

    const EPSILON: f32 = 0.000_001;
    const TEST_CAMERA_POSITION: Vec3 = Vec3::new(0.0, 1.5, 3.0);
    const TEST_DURATION: Duration = Duration::from_secs(1);
    const TEST_FREE_CAMERA_ROLL: Roll = Roll(0.35);
    const TEST_MARGIN: f32 = 0.15;
    const TEST_TARGET_POSITION: Vec3 = Vec3::new(3.5, 0.5, 0.0);

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

    fn assert_look_close(actual: LookAngles, expected: LookAngles) {
        assert_f32_close(actual.yaw, expected.yaw);
        assert_f32_close(actual.pitch, expected.pitch);
    }

    fn test_app() -> App {
        let mut app = App::new();
        app.init_resource::<Assets<Mesh>>()
            .init_resource::<Time>()
            .init_resource::<ZoomEventCounts>()
            .add_plugins((AnimationPlugin, FitPlugin))
            .add_observer(look_at::on_orbit_cam_look_at)
            .add_observer(on_orbit_cam_look_at_and_zoom_to_fit)
            .add_observer(look_at::on_free_cam_look_at)
            .add_observer(on_free_cam_look_at_and_zoom_to_fit)
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

    fn spawn_free_camera(app: &mut App) -> Entity {
        app.world_mut()
            .spawn((
                FreeCam::from_pose(
                    TEST_CAMERA_POSITION,
                    LookAngles {
                        yaw:   0.25,
                        pitch: -0.1,
                    },
                    TEST_FREE_CAMERA_ROLL,
                ),
                CameraBasis::Y_UP,
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
            CameraMove::ToLookAt {
                position,
                target,
                duration,
                ..
            } => {
                assert_eq!(*position, TEST_CAMERA_POSITION);
                assert_eq!(*target, TEST_TARGET_POSITION);
                assert_eq!(
                    *duration,
                    TEST_DURATION.mul_f32(LOOK_AT_AND_ZOOM_TO_FIT_LOOK_FRACTION)
                );
            },
            CameraMove::ToOrbitalLookAt { .. } => {
                return Err("first LookAtAndZoomToFit move should look at the target");
            },
        }

        let (expected_yaw, expected_pitch, _) = animation::orbital_parameters_from_offset(
            Displacement(TEST_CAMERA_POSITION - TEST_TARGET_POSITION),
        );
        match fit_move {
            CameraMove::ToOrbitalLookAt {
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
            CameraMove::ToLookAt { .. } => {
                return Err("second LookAtAndZoomToFit move should fit from the look direction");
            },
        }

        assert_eq!(
            app.world()
                .get::<AnimationSourceMarker>(camera)
                .map(|marker| marker.source),
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

    #[test]
    fn free_cam_look_at_and_zoom_to_fit_snaps_fit_preserving_roll() -> TestResult {
        let mut app = test_app();
        let target = spawn_target(&mut app);
        let camera = spawn_free_camera(&mut app);

        app.world_mut()
            .entity_mut(camera)
            .trigger(|_| LookAtAndZoomToFit::new(camera, target).margin(TEST_MARGIN));
        app.update();

        let Some(free_cam) = app.world().get::<FreeCam>(camera) else {
            return Err("FreeCam should still be present after LookAtAndZoomToFit");
        };
        let expected = LookAtPlan::from_free_camera(
            TEST_CAMERA_POSITION,
            TEST_TARGET_POSITION,
            CameraBasis::Y_UP,
        );
        assert_look_close(free_cam.look.target(), expected.look_angles());
        assert_eq!(free_cam.roll.target(), TEST_FREE_CAMERA_ROLL);
        assert_ne!(free_cam.translate.target().0, TEST_CAMERA_POSITION);
        assert!(app.world().get::<CameraMoveList>(camera).is_none());
        let Some(current_target) = app.world().get::<CurrentFitTarget>(camera) else {
            return Err("LookAtAndZoomToFit should update the current fit target");
        };
        assert_eq!(current_target.0, target);

        Ok(())
    }

    #[test]
    fn free_cam_look_at_and_zoom_to_fit_queues_look_then_fit() -> TestResult {
        let mut app = test_app();
        let target = spawn_target(&mut app);
        let camera = spawn_free_camera(&mut app);

        app.world_mut().entity_mut(camera).trigger(|_| {
            LookAtAndZoomToFit::new(camera, target)
                .margin(TEST_MARGIN)
                .duration(TEST_DURATION)
        });
        app.update();

        let Some(queue) = app.world().get::<CameraMoveList>(camera) else {
            return Err("timed FreeCam LookAtAndZoomToFit should create a camera move queue");
        };
        let mut moves = queue.camera_moves.iter();
        let Some(look_move) = moves.next() else {
            return Err("timed FreeCam LookAtAndZoomToFit should queue a look-at move");
        };
        let Some(fit_move) = moves.next() else {
            return Err("timed FreeCam LookAtAndZoomToFit should queue a fit move");
        };
        assert!(moves.next().is_none());

        match look_move {
            CameraMove::ToLookAt {
                position,
                target,
                roll,
                duration,
                ..
            } => {
                assert_eq!(*position, TEST_CAMERA_POSITION);
                assert_eq!(*target, TEST_TARGET_POSITION);
                assert_eq!(*roll, Some(TEST_FREE_CAMERA_ROLL));
                assert_eq!(
                    *duration,
                    TEST_DURATION.mul_f32(LOOK_AT_AND_ZOOM_TO_FIT_LOOK_FRACTION)
                );
            },
            CameraMove::ToOrbitalLookAt { .. } => {
                return Err("first FreeCam LookAtAndZoomToFit move should look at the target");
            },
        }

        let expected = LookAtPlan::from_free_camera(
            TEST_CAMERA_POSITION,
            TEST_TARGET_POSITION,
            CameraBasis::Y_UP,
        );
        match fit_move {
            CameraMove::ToOrbitalLookAt {
                yaw,
                pitch,
                roll,
                duration,
                ..
            } => {
                assert_f32_close(*yaw, expected.yaw);
                assert_f32_close(*pitch, expected.pitch);
                assert_eq!(*roll, Some(TEST_FREE_CAMERA_ROLL));
                assert_eq!(
                    *duration,
                    TEST_DURATION.saturating_sub(
                        TEST_DURATION.mul_f32(LOOK_AT_AND_ZOOM_TO_FIT_LOOK_FRACTION),
                    )
                );
            },
            CameraMove::ToLookAt { .. } => {
                return Err(
                    "second FreeCam LookAtAndZoomToFit move should fit from the look direction",
                );
            },
        }
        assert_eq!(
            app.world()
                .get::<AnimationSourceMarker>(camera)
                .map(|marker| marker.source),
            Some(AnimationSource::LookAtAndZoomToFit)
        );
        assert!(app.world().get::<ZoomAnimationMarker>(camera).is_none());
        let Some(current_target) = app.world().get::<CurrentFitTarget>(camera) else {
            return Err("LookAtAndZoomToFit should update the current fit target");
        };
        assert_eq!(current_target.0, target);

        Ok(())
    }
}
