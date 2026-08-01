use std::collections::VecDeque;
use std::time::Duration;

use bevy::math::curve::easing::EaseFunction;
use bevy::prelude::*;

use super::request;
use super::request::FitRequest;
use crate::CameraBasis;
use crate::animation::CameraMove;
use crate::animation::PlayAnimation;
use crate::constants::MILLIS_PER_SECOND;
use crate::fit::camera_pose;
use crate::fit::camera_pose::FreeCamFitPose;
use crate::fit::camera_pose::SnapOrbit;
use crate::fit::constants::DEFAULT_FIT_MARGIN;
use crate::fit::constants::ZOOM_TO_FIT_CONTEXT;
use crate::fit::geometry::FitAnchor;
use crate::fit::target::SetFitTarget;
use crate::free_cam::FreeCam;
use crate::orbit_cam::OrbitCam;

/// Context for a zoom-to-fit operation routed through `PlayAnimation`.
#[derive(Clone, Debug, Reflect)]
pub struct ZoomContext {
    /// The entity being framed.
    pub target:   Entity,
    /// The margin from the triggering `ZoomToFit`.
    pub margin:   f32,
    /// The duration from the triggering `ZoomToFit`.
    pub duration: Duration,
    /// The easing curve from the triggering `ZoomToFit`.
    pub easing:   EaseFunction,
}

/// Frames a target entity in the camera view while preserving the current viewing angle.
#[derive(EntityEvent, Reflect)]
#[reflect(Event, FromReflect)]
pub struct ZoomToFit {
    /// The camera entity to zoom.
    #[event_target]
    pub(crate) camera:    Entity,
    /// The entity to frame.
    pub(crate) target:    Entity,
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

impl ZoomToFit {
    /// Creates a new `ZoomToFit` event with default margin, instant duration, and cubic-out easing.
    #[must_use]
    pub const fn new(camera: Entity, target: Entity) -> Self {
        Self {
            camera,
            target,
            margin: DEFAULT_FIT_MARGIN,
            anchor: FitAnchor::Center,
            offset_px: Vec2::ZERO,
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

/// Emitted when a `ZoomToFit` operation begins.
#[derive(EntityEvent, Reflect)]
#[reflect(Event, FromReflect)]
pub struct ZoomBegin {
    /// The camera that is zooming.
    #[event_target]
    pub camera:   Entity,
    /// The entity being framed.
    pub target:   Entity,
    /// The margin from the triggering `ZoomToFit`.
    pub margin:   f32,
    /// The duration from the triggering `ZoomToFit`.
    pub duration: Duration,
    /// The easing curve from the triggering `ZoomToFit`.
    pub easing:   EaseFunction,
}

/// Emitted when a `ZoomToFit` operation stops, either by completing naturally
/// or by being cancelled. Inspect [`ZoomEnd::reason`] to distinguish.
#[derive(EntityEvent, Reflect)]
#[reflect(Event, FromReflect)]
pub struct ZoomEnd {
    /// The camera that stopped zooming.
    #[event_target]
    pub camera:   Entity,
    /// The entity that was being framed.
    pub target:   Entity,
    /// The margin from the triggering `ZoomToFit`.
    pub margin:   f32,
    /// The duration from the triggering `ZoomToFit`.
    pub duration: Duration,
    /// The easing curve from the triggering `ZoomToFit`.
    pub easing:   EaseFunction,
    /// Why the zoom stopped: completed naturally, or cancelled.
    pub reason:   ZoomReason,
}

/// Why a [`ZoomEnd`] fired.
#[derive(Clone, Copy, Debug, Reflect)]
pub enum ZoomReason {
    /// The zoom-to-fit animation ran to completion.
    Completed,
    /// The zoom-to-fit was interrupted before it could complete.
    Cancelled,
}

/// Observer for `ZoomToFit` event - frames a target entity in the camera view.
/// When duration is `Duration::ZERO`, snaps instantly.
/// When duration is greater than zero, animates smoothly via [`PlayAnimation`]
/// with a [`ZoomContext`] so that `on_play_animation` handles all conflict
/// resolution and zoom lifecycle events in one place.
/// Requires target entity to have a `Mesh3d` (direct or on descendants).
pub(crate) fn on_orbit_cam_zoom_to_fit(
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
    let anchor = zoom.anchor;
    let offset_px = zoom.offset_px;

    let Ok((mut orbit_cam, projection, camera_component)) = camera_query.get_mut(camera) else {
        return;
    };

    debug!(
        "ZoomToFit: yaw={:.3} pitch={:.3} current_focus={:.1?} current_radius={:.1} duration_ms={:.0}",
        orbit_cam.orbit.target().yaw,
        orbit_cam.orbit.target().pitch,
        orbit_cam.pan.target().0,
        orbit_cam.zoom.target().0,
        duration.as_secs_f32() * MILLIS_PER_SECOND,
    );

    let Some(fit) = request::prepare_fit_for_target(
        &FitRequest {
            context: ZOOM_TO_FIT_CONTEXT,
            target,
            yaw: orbit_cam.orbit.target().yaw,
            pitch: orbit_cam.orbit.target().pitch,
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
        // Animated path: use `ToOrbitalLookAt` to pass orbital parameters directly, avoiding
        // gimbal lock from atan2 decomposition at extreme pitch angles.
        let camera_moves = VecDeque::from([CameraMove::ToOrbitalLookAt {
            target: *fit.focus,
            yaw: orbit_cam.orbit.target().yaw,
            pitch: orbit_cam.orbit.target().pitch,
            radius: fit.radius,
            roll: None,
            duration,
            easing,
        }]);

        let zoom_context = ZoomContext {
            target,
            margin,
            duration,
            easing,
        };

        // `on_play_animation` handles conflict resolution, `ZoomBegin`, and
        // `ZoomAnimationMarker` insertion — all in one place after acceptance.
        commands.trigger(
            PlayAnimation::new(camera, camera_moves)
                .zoom_context(zoom_context)
                .target(target),
        );
    } else {
        camera_pose::snap_to_orbit(
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
                    reason: ZoomReason::Completed,
                });
            },
        );
    }

    // Route fit target updates through a single lifecycle owner.
    commands.trigger(SetFitTarget::new(camera, target));
}

/// Observer for `ZoomToFit` on `FreeCam`: frames a target by preserving the
/// current free-flight look/roll and translating to the fitted distance.
pub(crate) fn on_free_cam_zoom_to_fit(
    zoom: On<ZoomToFit>,
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
    let camera = zoom.camera;
    let target = zoom.target;
    let margin = zoom.margin;
    let duration = zoom.duration;
    let easing = zoom.easing;
    let anchor = zoom.anchor;
    let offset_px = zoom.offset_px;

    let Ok((mut free_cam, mut projection, camera_component, basis, transform)) =
        camera_query.get_mut(camera)
    else {
        return;
    };
    let start = FreeCamFitPose::from_free_cam_or_transform(&free_cam, transform, *basis);

    debug!(
        "FreeCam ZoomToFit: yaw={:.3} pitch={:.3} roll={:.3} position={:.1?} duration_ms={:.0}",
        start.look.yaw,
        start.look.pitch,
        start.roll.0,
        start.position.0,
        duration.as_secs_f32() * MILLIS_PER_SECOND,
    );

    let Some(fit) = request::prepare_fit_for_target(
        &FitRequest {
            context: ZOOM_TO_FIT_CONTEXT,
            target,
            yaw: start.look.yaw,
            pitch: start.look.pitch,
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

    let target_pose = FreeCamFitPose::from_fit(fit, &projection, *basis, start.look, start.roll);
    camera_pose::sync_free_cam_projection(&mut projection, fit);

    if duration > Duration::ZERO {
        let zoom_context = ZoomContext {
            target,
            margin,
            duration,
            easing,
        };
        let camera_moves = VecDeque::from([CameraMove::ToOrbitalLookAt {
            target: *fit.focus,
            yaw: start.look.yaw,
            pitch: start.look.pitch,
            radius: fit.radius,
            roll: Some(start.roll),
            duration,
            easing,
        }]);
        commands.trigger(
            PlayAnimation::new(camera, camera_moves)
                .zoom_context(zoom_context)
                .target(target),
        );
    } else {
        camera_pose::apply_free_cam_pose(&mut free_cam, target_pose);
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
            reason: ZoomReason::Completed,
        });
    }

    commands.trigger(SetFitTarget::new(camera, target));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CurrentFitTarget;
    use crate::animation::AnimationBegin;
    use crate::animation::AnimationEnd;
    use crate::animation::AnimationPlugin;
    use crate::animation::CameraMoveList;
    use crate::operation::LookAngles;
    use crate::operation::Position;
    use crate::operation::Roll;

    const CAMERA_POSITION: Vec3 = Vec3::new(0.0, 0.0, 8.0);
    const CAMERA_LOOK: LookAngles = LookAngles {
        yaw:   0.5,
        pitch: -0.25,
    };
    const CAMERA_ROLL: Roll = Roll(0.35);
    const TARGET_POSITION: Vec3 = Vec3::ZERO;
    const TEST_DURATION: Duration = Duration::from_millis(800);
    const EPSILON: f32 = 0.000_001;

    type TestResult = Result<(), &'static str>;

    #[derive(Resource, Default)]
    struct ZoomEventCounts {
        begin: usize,
        end:   usize,
    }

    #[derive(Resource, Default)]
    struct AnimationEventCounts {
        begin: usize,
        end:   usize,
    }

    fn count_zoom_begin(_: On<ZoomBegin>, mut counts: ResMut<ZoomEventCounts>) {
        counts.begin += 1;
    }

    fn count_zoom_end(_: On<ZoomEnd>, mut counts: ResMut<ZoomEventCounts>) { counts.end += 1; }

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
            .init_resource::<ZoomEventCounts>()
            .init_resource::<AnimationEventCounts>()
            .add_observer(on_free_cam_zoom_to_fit)
            .add_observer(crate::fit::target::on_set_fit_target)
            .add_observer(count_zoom_begin)
            .add_observer(count_zoom_end)
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
                FreeCam::from_pose(CAMERA_POSITION, CAMERA_LOOK, CAMERA_ROLL),
                Projection::Perspective(PerspectiveProjection::default()),
                Camera::default(),
                CameraBasis::Y_UP,
                Transform::from_translation(CAMERA_POSITION),
            ))
            .id()
    }

    #[test]
    fn instant_free_cam_zoom_to_fit_preserves_look_and_roll() -> TestResult {
        let mut app = test_app();
        let target = spawn_target(&mut app);
        let camera = spawn_free_camera(&mut app);

        app.world_mut()
            .entity_mut(camera)
            .trigger(|_| ZoomToFit::new(camera, target));
        app.update();

        let Some(free_cam) = app.world().get::<FreeCam>(camera) else {
            return Err("FreeCam should still be present after ZoomToFit");
        };
        assert_f32_close(free_cam.look.current().yaw, CAMERA_LOOK.yaw);
        assert_f32_close(free_cam.look.current().pitch, CAMERA_LOOK.pitch);
        assert_f32_close(free_cam.roll.current().0, CAMERA_ROLL.0);
        assert_ne!(free_cam.translate.current(), Position(CAMERA_POSITION));

        let Some(current_target) = app.world().get::<CurrentFitTarget>(camera) else {
            return Err("ZoomToFit should update CurrentFitTarget for FreeCam");
        };
        assert_eq!(current_target.0, target);

        let zoom_counts = app.world().resource::<ZoomEventCounts>();
        assert_eq!(zoom_counts.begin, 1);
        assert_eq!(zoom_counts.end, 1);
        let animation_counts = app.world().resource::<AnimationEventCounts>();
        assert_eq!(animation_counts.begin, 0);
        assert_eq!(animation_counts.end, 0);

        Ok(())
    }

    #[test]
    fn timed_free_cam_zoom_to_fit_starts_free_camera_animation() -> TestResult {
        let mut app = test_app();
        let target = spawn_target(&mut app);
        let camera = spawn_free_camera(&mut app);

        app.world_mut()
            .entity_mut(camera)
            .trigger(|_| ZoomToFit::new(camera, target).duration(TEST_DURATION));
        app.update();

        let Some(queue) = app.world().get::<CameraMoveList>(camera) else {
            return Err("timed FreeCam ZoomToFit should insert CameraMoveList");
        };
        let Some(CameraMove::ToOrbitalLookAt {
            yaw, pitch, roll, ..
        }) = queue.camera_moves.front()
        else {
            return Err("timed FreeCam ZoomToFit should queue a ToOrbitalLookAt move");
        };
        assert_f32_close(*yaw, CAMERA_LOOK.yaw);
        assert_f32_close(*pitch, CAMERA_LOOK.pitch);
        assert_eq!(*roll, Some(CAMERA_ROLL));

        let current_target = app.world().get::<CurrentFitTarget>(camera);
        assert!(current_target.is_some());
        assert_eq!(current_target.map(|target| target.0), Some(target));

        let zoom_counts = app.world().resource::<ZoomEventCounts>();
        assert_eq!(zoom_counts.begin, 1);
        assert_eq!(zoom_counts.end, 0);
        let animation_counts = app.world().resource::<AnimationEventCounts>();
        assert_eq!(animation_counts.begin, 1);
        assert_eq!(animation_counts.end, 0);

        Ok(())
    }
}
