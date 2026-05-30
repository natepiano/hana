use super::*;

pub(crate) fn toggle_debug_overlay(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut commands: Commands,
    scene: Res<SceneEntities>,
    visualization_query: Query<(), With<FitOverlay>>,
) {
    if !keyboard.just_pressed(KeyCode::KeyD) {
        return;
    }

    let camera = scene.camera;
    if visualization_query.get(camera).is_ok() {
        commands.entity(camera).remove::<FitOverlay>();
    } else {
        commands.entity(camera).insert(FitOverlay);
    }
}

pub(crate) fn animate_camera(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut commands: Commands,
    scene: Res<SceneEntities>,
    easing: Res<ActiveEasing>,
    camera_query: Query<&OrbitCam>,
) {
    if !keyboard.just_pressed(KeyCode::KeyA) {
        return;
    }

    let camera = scene.camera;
    let Ok(orbit_camera) = camera_query.get(camera) else {
        return;
    };

    let easing_function = easing.0;
    let yaw = orbit_camera.target_yaw;
    let pitch = orbit_camera.target_pitch;
    let radius = orbit_camera.target_radius;
    let focus = orbit_camera.target_focus;

    let camera_moves = [
        CameraMove::ToOrbit {
            focus,
            yaw: yaw + QUARTER_TURN_RADIANS,
            pitch,
            radius,
            duration: Duration::from_millis(ORBIT_MOVE_DURATION_MILLIS),
            easing: easing_function,
        },
        CameraMove::ToOrbit {
            focus,
            yaw: QUARTER_TURN_RADIANS.mul_add(SECOND_ORBIT_MOVE_QUARTER_TURNS, yaw),
            pitch,
            radius,
            duration: Duration::from_millis(ORBIT_MOVE_DURATION_MILLIS),
            easing: easing_function,
        },
        CameraMove::ToOrbit {
            focus,
            yaw: QUARTER_TURN_RADIANS.mul_add(THIRD_ORBIT_MOVE_QUARTER_TURNS, yaw),
            pitch,
            radius,
            duration: Duration::from_millis(ORBIT_MOVE_DURATION_MILLIS),
            easing: easing_function,
        },
        CameraMove::ToOrbit {
            focus,
            yaw: QUARTER_TURN_RADIANS.mul_add(FOURTH_ORBIT_MOVE_QUARTER_TURNS, yaw),
            pitch,
            radius,
            duration: Duration::from_millis(ORBIT_MOVE_DURATION_MILLIS),
            easing: easing_function,
        },
    ];

    commands.trigger(PlayAnimation::new(camera, camera_moves));
}

pub(crate) fn randomize_easing(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut easing: ResMut<ActiveEasing>,
    time: Res<Time>,
    mut log: ResMut<event_log::EventLog>,
) {
    if keyboard.just_pressed(KeyCode::KeyR) {
        let index = (time.elapsed_secs() * SECONDS_TO_MILLIS).to_usize() % ALL_EASINGS.len();
        easing.0 = ALL_EASINGS[index];
        log.push(format!("Easing: {:#?}", easing.0));
    }
    if keyboard.just_pressed(KeyCode::KeyE) {
        easing.0 = EaseFunction::CubicOut;
        log.push(EVENT_LOG_EASING_RESET.into());
    }
}

#[derive(Default, PartialEq, Eq)]
pub(crate) enum DeferRefit {
    #[default]
    Continue,
    WaitOneFrame,
}

#[derive(PartialEq, Eq)]
enum ProjectionLog {
    Unwritten,
    Written,
}

/// Toggles between perspective and orthographic projection, then re-fits the scene.
///
/// The fit is deferred one frame via `defer_refit` because `OrbitCam` needs to
/// process the projection change (syncing radius ↔ orthographic scale) before the
/// fit calculation can produce correct results.
pub(crate) fn toggle_projection(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut commands: Commands,
    scene: Res<SceneEntities>,
    active_easing: Res<ActiveEasing>,
    mut camera_query: Query<(&mut Projection, &mut OrbitCam)>,
    mut log: ResMut<event_log::EventLog>,
    mut defer_refit: Local<DeferRefit>,
) {
    // Deferred fit: projection was changed last frame, `OrbitCam` has now synced.
    if *defer_refit == DeferRefit::WaitOneFrame {
        *defer_refit = DeferRefit::Continue;
        commands.trigger(
            AnimateToFit::new(scene.camera, scene.scene_bounds)
                .yaw(CAMERA_START_YAW)
                .pitch(CAMERA_START_PITCH)
                .margin(ZOOM_MARGIN_SCENE)
                .duration(Duration::from_millis(ANIMATE_FIT_DURATION_MILLIS))
                .easing(active_easing.0),
        );
        return;
    }

    if !keyboard.just_pressed(KeyCode::KeyP) {
        return;
    }
    let Ok((mut projection, mut orbit_camera)) = camera_query.get_mut(scene.camera) else {
        return;
    };
    let mut projection_log = ProjectionLog::Unwritten;
    match *projection {
        Projection::Perspective(_) => {
            *projection = Projection::from(OrthographicProjection {
                scaling_mode: ScalingMode::FixedVertical {
                    viewport_height: ORTHOGRAPHIC_VIEWPORT_HEIGHT,
                },
                far: ORTHOGRAPHIC_FAR_PLANE,
                ..OrthographicProjection::default_3d()
            });
            log.push(PROJECTION_LOG_ORTHOGRAPHIC.into());
            projection_log = ProjectionLog::Written;
        },
        Projection::Orthographic(_) => {
            *projection = Projection::Perspective(PerspectiveProjection::default());
            log.push(PROJECTION_LOG_PERSPECTIVE.into());
            projection_log = ProjectionLog::Written;
        },
        Projection::Custom(_) => {},
    }
    orbit_camera.force_update();
    if projection_log == ProjectionLog::Written {
        *defer_refit = DeferRefit::WaitOneFrame;
    }
}

pub(crate) fn interrupt_behavior_hint_text(behavior: CameraInputInterruptBehavior) -> String {
    match behavior {
        CameraInputInterruptBehavior::Ignore => {
            "CameraInputInterruptBehavior::Ignore - camera input during animation is ignored".into()
        },
        CameraInputInterruptBehavior::Cancel => {
            "CameraInputInterruptBehavior::Cancel - camera input during animation will cancel it".into()
        },
        CameraInputInterruptBehavior::Complete => {
            "CameraInputInterruptBehavior::Complete - camera input during animation will jump to final position"
                .into()
        },
    }
}

pub(crate) fn toggle_interrupt_behavior(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut commands: Commands,
    scene: Res<SceneEntities>,
    mut behavior_query: Query<&mut CameraInputInterruptBehavior>,
    mut hint_query: Query<&mut Text, With<ui::CameraInputInterruptBehaviorLabel>>,
    mut log: ResMut<event_log::EventLog>,
) {
    if !keyboard.just_pressed(KeyCode::KeyI) {
        return;
    }

    let new_behavior =
        behavior_query
            .get(scene.camera)
            .map_or(
                CameraInputInterruptBehavior::Ignore,
                |behavior| match *behavior {
                    CameraInputInterruptBehavior::Ignore => CameraInputInterruptBehavior::Cancel,
                    CameraInputInterruptBehavior::Cancel => CameraInputInterruptBehavior::Complete,
                    CameraInputInterruptBehavior::Complete => CameraInputInterruptBehavior::Ignore,
                },
            );

    if let Ok(mut behavior) = behavior_query.get_mut(scene.camera) {
        *behavior = new_behavior;
    } else {
        commands.entity(scene.camera).insert(new_behavior);
    }

    for mut text in &mut hint_query {
        **text = interrupt_behavior_hint_text(new_behavior);
    }
    log.push(format!("CameraInputInterruptBehavior: {new_behavior:?}"));
}

pub(crate) fn conflict_policy_hint_text(policy: AnimationConflictPolicy) -> String {
    match policy {
        AnimationConflictPolicy::LastWins => {
            "AnimationConflictPolicy::LastWins - new animation cancels current one".into()
        },
        AnimationConflictPolicy::FirstWins => {
            "AnimationConflictPolicy::FirstWins - new animation is rejected while one is playing"
                .into()
        },
    }
}

pub(crate) fn toggle_animation_conflict_policy(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut commands: Commands,
    scene: Res<SceneEntities>,
    mut policy_query: Query<&mut AnimationConflictPolicy>,
    mut hint_query: Query<&mut Text, With<ui::AnimationConflictPolicyLabel>>,
    mut log: ResMut<event_log::EventLog>,
) {
    if !keyboard.just_pressed(KeyCode::KeyQ) {
        return;
    }

    let new_policy =
        policy_query
            .get(scene.camera)
            .map_or(AnimationConflictPolicy::FirstWins, |policy| match *policy {
                AnimationConflictPolicy::LastWins => AnimationConflictPolicy::FirstWins,
                AnimationConflictPolicy::FirstWins => AnimationConflictPolicy::LastWins,
            });

    if let Ok(mut policy) = policy_query.get_mut(scene.camera) {
        *policy = new_policy;
    } else {
        commands.entity(scene.camera).insert(new_policy);
    }

    for mut text in &mut hint_query {
        **text = conflict_policy_hint_text(new_policy);
    }
    log.push(format!("AnimationConflictPolicy: {new_policy:?}"));
}
