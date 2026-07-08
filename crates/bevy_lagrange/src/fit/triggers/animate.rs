use std::collections::VecDeque;
use std::time::Duration;

use bevy::math::curve::easing::EaseFunction;
use bevy::prelude::*;

use super::request;
use super::request::FitRequest;
use crate::CameraBasis;
use crate::animation::AnimationBegin;
use crate::animation::AnimationEnd;
use crate::animation::AnimationReason;
use crate::animation::AnimationSource;
use crate::animation::CameraMove;
use crate::animation::PlayAnimation;
use crate::fit::camera_pose;
use crate::fit::camera_pose::FreeCamFitPose;
use crate::fit::camera_pose::SnapOrbit;
use crate::fit::constants::ANIMATE_TO_FIT_CONTEXT;
use crate::fit::constants::DEFAULT_ANIMATE_TO_FIT_PITCH;
use crate::fit::constants::DEFAULT_ANIMATE_TO_FIT_YAW;
use crate::fit::constants::DEFAULT_FIT_MARGIN;
use crate::fit::geometry::FitAnchor;
use crate::fit::target::SetFitTarget;
use crate::free_cam::FreeCam;
use crate::operation::LookAngles;
use crate::operation::Roll;
use crate::orbit_cam::OrbitCam;

/// Animates the camera to a caller-specified orientation while framing a target entity.
#[derive(EntityEvent, Reflect)]
#[reflect(Event, FromReflect)]
pub struct AnimateToFit {
    /// The camera entity.
    #[event_target]
    pub(crate) camera:    Entity,
    /// The entity to frame.
    pub(crate) target:    Entity,
    /// Final yaw in radians.
    pub(crate) yaw:       f32,
    /// Final pitch in radians.
    pub(crate) pitch:     f32,
    /// Fraction of screen to leave as margin.
    pub(crate) margin:    f32,
    /// Screen-space anchor used after the target has been fitted.
    pub(crate) anchor:    FitAnchor,
    /// Pixel offset from the selected anchor, using positive x right and positive y down.
    pub(crate) offset_px: Vec2,
    /// Animation duration (`ZERO` for instant).
    pub(crate) duration:  Duration,
    /// Easing curve for the animation.
    pub(crate) easing:    EaseFunction,
}

impl AnimateToFit {
    /// Creates a new `AnimateToFit` with default parameters.
    #[must_use]
    pub const fn new(camera: Entity, target: Entity) -> Self {
        Self {
            camera,
            target,
            yaw: DEFAULT_ANIMATE_TO_FIT_YAW,
            pitch: DEFAULT_ANIMATE_TO_FIT_PITCH,
            margin: DEFAULT_FIT_MARGIN,
            anchor: FitAnchor::Center,
            offset_px: Vec2::ZERO,
            duration: Duration::ZERO,
            easing: EaseFunction::CubicOut,
        }
    }

    /// Sets the target yaw.
    #[must_use]
    pub const fn yaw(mut self, yaw: f32) -> Self {
        self.yaw = yaw;
        self
    }

    /// Sets the target pitch.
    #[must_use]
    pub const fn pitch(mut self, pitch: f32) -> Self {
        self.pitch = pitch;
        self
    }

    /// Sets the margin.
    #[must_use]
    pub const fn margin(mut self, margin: f32) -> Self {
        self.margin = margin;
        self
    }

    /// Sets which fitted bounds point should land on the matching viewport point.
    #[must_use]
    pub const fn anchor(mut self, anchor: FitAnchor) -> Self {
        self.anchor = anchor;
        self
    }

    /// Sets a pixel offset from the selected anchor.
    ///
    /// Positive x moves the fitted bounds right. Positive y moves them down,
    /// matching Bevy's screen-space coordinate convention.
    #[must_use]
    pub const fn offset_px(mut self, offset_px: Vec2) -> Self {
        self.offset_px = offset_px;
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

/// Observer for `AnimateToFit` event - animates the camera to a specific orientation
/// while fitting a target entity in view.
pub(crate) fn on_orbit_cam_animate_to_fit(
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
    let anchor = event.anchor;
    let offset_px = event.offset_px;

    let Ok((mut orbit_cam, projection, camera_component)) = camera_query.get_mut(camera) else {
        return;
    };

    let Some(fit) = request::prepare_fit_for_target(
        &FitRequest {
            context: ANIMATE_TO_FIT_CONTEXT,
            target,
            yaw,
            pitch,
            margin,
            anchor,
            offset_px,
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
        let camera_moves = VecDeque::from([CameraMove::ToOrbitalLookAt {
            target: *fit.focus,
            yaw,
            pitch,
            radius: fit.radius,
            roll: None,
            duration,
            easing,
        }]);
        commands.trigger(
            PlayAnimation::new(camera, camera_moves)
                .source(AnimationSource::AnimateToFit)
                .target(target),
        );
    } else {
        camera_pose::snap_to_orbit(
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
                commands.trigger(AnimationBegin {
                    camera,
                    source,
                    target: Some(target),
                });
                commands.trigger(AnimationEnd {
                    camera,
                    source,
                    target: Some(target),
                    reason: AnimationReason::Completed,
                });
            },
        );
    }
    // Route fit target updates through a single lifecycle owner.
    commands.trigger(SetFitTarget::new(camera, target));
}

/// Observer for `AnimateToFit` on `FreeCam`: applies the shared fit solution as
/// free-flight position, look, and roll state.
pub(crate) fn on_free_cam_animate_to_fit(
    event: On<AnimateToFit>,
    mut commands: Commands,
    mut camera_query: Query<(&mut FreeCam, &mut Projection, &Camera, &CameraBasis)>,
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
    let anchor = event.anchor;
    let offset_px = event.offset_px;

    let Ok((mut free_cam, mut projection, camera_component, basis)) = camera_query.get_mut(camera)
    else {
        return;
    };

    let Some(fit) = request::prepare_fit_for_target(
        &FitRequest {
            context: ANIMATE_TO_FIT_CONTEXT,
            target,
            yaw,
            pitch,
            margin,
            anchor,
            offset_px,
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

    let target_pose = FreeCamFitPose::from_fit(
        fit,
        &projection,
        *basis,
        LookAngles { yaw, pitch },
        Roll::default(),
    );
    camera_pose::sync_free_cam_projection(&mut projection, fit);
    if duration > Duration::ZERO {
        let camera_moves = VecDeque::from([CameraMove::ToOrbitalLookAt {
            target: *fit.focus,
            yaw,
            pitch,
            radius: fit.radius,
            roll: Some(Roll::default()),
            duration,
            easing,
        }]);
        commands.trigger(
            PlayAnimation::new(camera, camera_moves)
                .source(AnimationSource::AnimateToFit)
                .target(target),
        );
    } else {
        camera_pose::apply_free_cam_pose(&mut free_cam, target_pose);
        commands.trigger(AnimationBegin {
            camera,
            source: AnimationSource::AnimateToFit,
            target: Some(target),
        });
        commands.trigger(AnimationEnd {
            camera,
            source: AnimationSource::AnimateToFit,
            target: Some(target),
            reason: AnimationReason::Completed,
        });
    }
    commands.trigger(SetFitTarget::new(camera, target));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CurrentFitTarget;
    use crate::Initialization;
    use crate::Position;
    use crate::animation::AnimationPlugin;
    use crate::animation::CameraMoveList;

    const CAMERA_POSITION: Vec3 = Vec3::new(0.0, 0.0, 8.0);
    const FIT_PITCH: f32 = 0.25;
    const FIT_YAW: f32 = 0.5;
    const TEST_DURATION: Duration = Duration::from_millis(800);
    const TARGET_POSITION: Vec3 = Vec3::ZERO;
    const EPSILON: f32 = 0.000_001;

    type TestResult = Result<(), &'static str>;

    #[derive(Resource, Default)]
    struct AnimationEventCounts {
        begin: usize,
        end:   usize,
    }

    fn count_animation_begin(_: On<AnimationBegin>, mut counts: ResMut<AnimationEventCounts>) {
        counts.begin += 1;
    }

    fn count_animation_end(_: On<AnimationEnd>, mut counts: ResMut<AnimationEventCounts>) {
        counts.end += 1;
    }

    fn assert_f32_close(actual: f32, expected: f32) {
        assert!((actual - expected).abs() <= EPSILON);
    }

    fn test_app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<Assets<Mesh>>()
            .init_resource::<AnimationEventCounts>()
            .add_observer(on_free_cam_animate_to_fit)
            .add_observer(crate::fit::target::on_set_fit_target)
            .add_plugins(AnimationPlugin)
            .add_observer(count_animation_begin)
            .add_observer(count_animation_end);
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
                Transform::from_translation(TARGET_POSITION),
                GlobalTransform::from(Transform::from_translation(TARGET_POSITION)),
            ))
            .id()
    }

    fn spawn_free_camera(app: &mut App) -> Entity {
        app.world_mut()
            .spawn((
                FreeCam::from_pose(
                    Position(CAMERA_POSITION),
                    LookAngles::default(),
                    Roll::default(),
                ),
                Projection::Perspective(PerspectiveProjection::default()),
                Camera::default(),
                CameraBasis::Y_UP,
                Transform::from_translation(CAMERA_POSITION),
            ))
            .id()
    }

    #[test]
    fn instant_free_cam_animate_to_fit_writes_free_pose() -> TestResult {
        let mut app = test_app();
        let target = spawn_target(&mut app);
        let camera = spawn_free_camera(&mut app);

        app.world_mut().entity_mut(camera).trigger(|_| {
            AnimateToFit::new(camera, target)
                .yaw(FIT_YAW)
                .pitch(FIT_PITCH)
        });
        app.update();

        let Some(free_cam) = app.world().get::<FreeCam>(camera) else {
            return Err("FreeCam should still be present after AnimateToFit");
        };
        assert_f32_close(free_cam.look.current().yaw, FIT_YAW);
        assert_f32_close(free_cam.look.current().pitch, FIT_PITCH);
        assert_f32_close(free_cam.roll.current().0, 0.0);
        assert_ne!(free_cam.translate.current(), Position(CAMERA_POSITION));
        assert_eq!(free_cam.initialization, Initialization::Active);

        let Some(current_target) = app.world().get::<CurrentFitTarget>(camera) else {
            return Err("AnimateToFit should update CurrentFitTarget for FreeCam");
        };
        assert_eq!(current_target.0, target);

        let counts = app.world().resource::<AnimationEventCounts>();
        assert_eq!(counts.begin, 1);
        assert_eq!(counts.end, 1);

        Ok(())
    }

    #[test]
    fn timed_free_cam_animate_to_fit_starts_free_camera_animation() -> TestResult {
        let mut app = test_app();
        let target = spawn_target(&mut app);
        let camera = spawn_free_camera(&mut app);

        app.world_mut().entity_mut(camera).trigger(|_| {
            AnimateToFit::new(camera, target)
                .yaw(FIT_YAW)
                .pitch(FIT_PITCH)
                .duration(TEST_DURATION)
        });
        app.update();

        let Some(queue) = app.world().get::<CameraMoveList>(camera) else {
            return Err("timed FreeCam AnimateToFit should insert CameraMoveList");
        };
        let Some(CameraMove::ToOrbitalLookAt {
            yaw, pitch, roll, ..
        }) = queue.camera_moves.front()
        else {
            return Err("timed FreeCam AnimateToFit should queue a ToOrbitalLookAt move");
        };
        assert_f32_close(*yaw, FIT_YAW);
        assert_f32_close(*pitch, FIT_PITCH);
        assert_eq!(*roll, Some(Roll::default()));

        let counts = app.world().resource::<AnimationEventCounts>();
        assert_eq!(counts.begin, 1);
        assert_eq!(counts.end, 0);

        Ok(())
    }
}
