use std::time::Duration;

use bevy::math::curve::easing::EaseFunction;
use bevy::prelude::*;
use bevy_kana::Position;

use super::plan::LookAtPlan;
use super::support;
use crate::CameraBasis;
use crate::animation::AnimationSource;
use crate::fit::camera_pose;
use crate::fit::camera_pose::FreeCamFitPose;
use crate::fit::camera_pose::SnapOrbit;
use crate::free_cam::FreeCam;
use crate::orbit_cam::OrbitCam;

/// Rotates the camera in place to face a target entity.
#[derive(EntityEvent, Reflect)]
#[reflect(Event, FromReflect)]
pub struct LookAt {
    /// The camera entity.
    #[event_target]
    pub camera:   Entity,
    /// The entity to look at.
    pub target:   Entity,
    /// Animation duration (`ZERO` for instant).
    pub duration: Duration,
    /// Easing curve for the animation.
    pub easing:   EaseFunction,
}

impl LookAt {
    /// Creates a new `LookAt` with instant duration and cubic-out easing.
    #[must_use]
    pub const fn new(camera: Entity, target: Entity) -> Self {
        Self {
            camera,
            target,
            duration: Duration::ZERO,
            easing: EaseFunction::CubicOut,
        }
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

/// Observer for `LookAt` event — rotates the camera in place to look at a target entity.
/// The camera stays at its current world position; only the orbit pivot re-anchors.
pub(crate) fn on_orbit_cam_look_at(
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
    let plan = LookAtPlan::from_world_positions(camera_position, target_position);

    if duration > Duration::ZERO {
        support::trigger_timed_animation(
            &mut commands,
            camera,
            target,
            AnimationSource::LookAt,
            [plan.to_look_move(None, duration, easing)],
        );
    } else {
        camera_pose::snap_to_orbit(
            &mut commands,
            &mut orbit_cam,
            SnapOrbit {
                focus:  Position(target_position),
                yaw:    Some(plan.yaw),
                pitch:  Some(plan.pitch),
                radius: plan.radius,
            },
            |commands| {
                support::trigger_completed_animation(
                    commands,
                    camera,
                    target,
                    AnimationSource::LookAt,
                );
            },
        );
    }
}

/// Observer for `LookAt` on `FreeCam`: preserves position and roll while
/// turning the camera to face the target.
pub(crate) fn on_free_cam_look_at(
    event: On<LookAt>,
    mut commands: Commands,
    mut camera_query: Query<(&mut FreeCam, &CameraBasis, &Transform)>,
    global_transform_query: Query<&GlobalTransform>,
) {
    let camera = event.camera;
    let target = event.target;
    let duration = event.duration;
    let easing = event.easing;

    let Ok((mut free_cam, basis, transform)) = camera_query.get_mut(camera) else {
        return;
    };

    let Ok(target_transform) = global_transform_query.get(target) else {
        warn!("LookAt: target {target:?} has no GlobalTransform");
        return;
    };

    let start = FreeCamFitPose::from_free_cam_or_transform(&free_cam, transform, *basis);
    let target_position = target_transform.translation();
    let plan = LookAtPlan::from_free_camera(start.position.0, target_position, *basis);
    let roll = Some(start.roll);

    if duration > Duration::ZERO {
        support::trigger_timed_animation(
            &mut commands,
            camera,
            target,
            AnimationSource::LookAt,
            [plan.to_look_move(roll, duration, easing)],
        );
    } else {
        camera_pose::apply_free_cam_pose(
            &mut free_cam,
            FreeCamFitPose {
                position: start.position,
                look:     plan.look_angles(),
                roll:     start.roll,
            },
        );
        support::trigger_completed_animation(
            &mut commands,
            camera,
            target,
            AnimationSource::LookAt,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CurrentFitTarget;
    use crate::animation::AnimationPlugin;
    use crate::animation::AnimationSourceMarker;
    use crate::animation::CameraMove;
    use crate::animation::CameraMoveList;
    use crate::operation::LookAngles;
    use crate::operation::Roll;

    type TestResult = Result<(), &'static str>;

    const EPSILON: f32 = 0.000_001;
    const TEST_CAMERA_POSITION: Vec3 = Vec3::new(0.0, 1.5, 3.0);
    const TEST_DURATION: Duration = Duration::from_secs(1);
    const TEST_FREE_CAMERA_ROLL: Roll = Roll(0.35);
    const TEST_TARGET_POSITION: Vec3 = Vec3::new(3.5, 0.5, 0.0);

    fn assert_f32_close(actual: f32, expected: f32) {
        assert!((actual - expected).abs() <= EPSILON);
    }

    fn assert_look_close(actual: LookAngles, expected: LookAngles) {
        assert_f32_close(actual.yaw, expected.yaw);
        assert_f32_close(actual.pitch, expected.pitch);
    }

    fn test_app() -> App {
        let mut app = App::new();
        app.add_plugins(AnimationPlugin)
            .add_observer(on_orbit_cam_look_at)
            .add_observer(on_free_cam_look_at);
        app
    }

    fn spawn_target(app: &mut App) -> Entity {
        app.world_mut()
            .spawn((
                Transform::from_translation(TEST_TARGET_POSITION),
                GlobalTransform::from(Transform::from_translation(TEST_TARGET_POSITION)),
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
                Transform::from_translation(TEST_CAMERA_POSITION),
                GlobalTransform::from(Transform::from_translation(TEST_CAMERA_POSITION)),
            ))
            .id()
    }

    #[test]
    fn free_cam_look_at_snaps_look_preserving_position_and_roll() -> TestResult {
        let mut app = test_app();
        let target = spawn_target(&mut app);
        let camera = spawn_free_camera(&mut app);

        app.world_mut()
            .entity_mut(camera)
            .trigger(|_| LookAt::new(camera, target));
        app.update();

        let Some(free_cam) = app.world().get::<FreeCam>(camera) else {
            return Err("FreeCam should still be present after LookAt");
        };
        let expected = LookAtPlan::from_free_camera(
            TEST_CAMERA_POSITION,
            TEST_TARGET_POSITION,
            CameraBasis::Y_UP,
        );
        assert_eq!(free_cam.translate.target().0, TEST_CAMERA_POSITION);
        assert_look_close(free_cam.look.target(), expected.look_angles());
        assert_eq!(free_cam.roll.target(), TEST_FREE_CAMERA_ROLL);
        assert!(app.world().get::<CameraMoveList>(camera).is_none());
        assert!(app.world().get::<CurrentFitTarget>(camera).is_none());

        Ok(())
    }

    #[test]
    fn free_cam_look_at_queues_timed_look_move() -> TestResult {
        let mut app = test_app();
        let target = spawn_target(&mut app);
        let camera = spawn_free_camera(&mut app);

        app.world_mut()
            .entity_mut(camera)
            .trigger(|_| LookAt::new(camera, target).duration(TEST_DURATION));
        app.update();

        let Some(queue) = app.world().get::<CameraMoveList>(camera) else {
            return Err("timed FreeCam LookAt should create a camera move queue");
        };
        let mut moves = queue.camera_moves.iter();
        let Some(look_move) = moves.next() else {
            return Err("timed FreeCam LookAt should queue a look-at move");
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
                assert_eq!(*duration, TEST_DURATION);
            },
            CameraMove::ToOrbitalLookAt { .. } => {
                return Err("timed FreeCam LookAt should queue a look-at move");
            },
        }
        assert_eq!(
            app.world()
                .get::<AnimationSourceMarker>(camera)
                .map(|marker| marker.source),
            Some(AnimationSource::LookAt)
        );

        Ok(())
    }
}
