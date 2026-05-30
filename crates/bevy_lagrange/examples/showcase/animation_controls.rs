use super::*;

pub(crate) fn toggle_debug_overlay(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut commands: Commands,
    scene: Res<SceneEntities>,
    second: Option<Res<second_window::SecondWindowEntities>>,
    visualization_query: Query<(), With<FitOverlay>>,
    windows: Query<&Window>,
) {
    if !keyboard.just_pressed(KeyCode::KeyD) {
        return;
    }

    let camera = second_window::focused_camera(&scene, second.as_deref(), &windows);
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
    second: Option<Res<second_window::SecondWindowEntities>>,
    easing: Res<ActiveEasing>,
    camera_query: Query<&OrbitCam>,
    windows: Query<&Window>,
) {
    if !keyboard.just_pressed(KeyCode::KeyA) {
        return;
    }

    let camera = second_window::focused_camera(&scene, second.as_deref(), &windows);
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

pub(crate) fn animate_fit_to_scene(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut commands: Commands,
    scene: Res<SceneEntities>,
    home: Option<Res<CameraHomeEntity>>,
    second: Option<Res<second_window::SecondWindowEntities>>,
    easing: Res<ActiveEasing>,
    windows: Query<&Window>,
) {
    if !keyboard.just_pressed(KeyCode::KeyH) {
        return;
    }

    let camera = second_window::focused_camera(&scene, second.as_deref(), &windows);
    if camera == scene.camera {
        return;
    }
    let target = home.as_deref().map_or(scene.scene_bounds, |home| home.0);
    commands.trigger(
        AnimateToFit::new(camera, target)
            .yaw(CAMERA_START_YAW)
            .pitch(CAMERA_START_PITCH)
            .margin(ZOOM_MARGIN_SCENE)
            .duration(Duration::from_millis(ANIMATE_FIT_DURATION_MILLIS))
            .easing(easing.0),
    );
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
    second: Option<Res<second_window::SecondWindowEntities>>,
    active_easing: Res<ActiveEasing>,
    mut camera_query: Query<(&mut Projection, &mut OrbitCam)>,
    mut log: ResMut<event_log::EventLog>,
    mut defer_refit: Local<DeferRefit>,
) {
    // Deferred fit: projection was changed last frame, `OrbitCam` has now synced.
    if *defer_refit == DeferRefit::WaitOneFrame {
        *defer_refit = DeferRefit::Continue;
        for camera in second_window::all_cameras(&scene, second.as_deref()) {
            commands.trigger(
                AnimateToFit::new(camera, scene.scene_bounds)
                    .yaw(CAMERA_START_YAW)
                    .pitch(CAMERA_START_PITCH)
                    .margin(ZOOM_MARGIN_SCENE)
                    .duration(Duration::from_millis(ANIMATE_FIT_DURATION_MILLIS))
                    .easing(active_easing.0),
            );
        }
        return;
    }

    if !keyboard.just_pressed(KeyCode::KeyP) {
        return;
    }
    let mut projection_log = ProjectionLog::Unwritten;
    for camera in second_window::all_cameras(&scene, second.as_deref()) {
        let Ok((mut projection, mut orbit_camera)) = camera_query.get_mut(camera) else {
            continue;
        };
        match *projection {
            Projection::Perspective(_) => {
                *projection = Projection::from(OrthographicProjection {
                    scaling_mode: ScalingMode::FixedVertical {
                        viewport_height: ORTHOGRAPHIC_VIEWPORT_HEIGHT,
                    },
                    far: ORTHOGRAPHIC_FAR_PLANE,
                    ..OrthographicProjection::default_3d()
                });
                if projection_log == ProjectionLog::Unwritten {
                    log.push(PROJECTION_LOG_ORTHOGRAPHIC.into());
                    projection_log = ProjectionLog::Written;
                }
            },
            Projection::Orthographic(_) => {
                *projection = Projection::Perspective(PerspectiveProjection::default());
                if projection_log == ProjectionLog::Unwritten {
                    log.push(PROJECTION_LOG_PERSPECTIVE.into());
                    projection_log = ProjectionLog::Written;
                }
            },
            Projection::Custom(_) => {},
        }
        orbit_camera.force_update();
    }
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
    second: Option<Res<second_window::SecondWindowEntities>>,
    mut behavior_query: Query<&mut CameraInputInterruptBehavior>,
    mut hint_query: Query<&mut Text, With<ui::CameraInputInterruptBehaviorLabel>>,
    mut log: ResMut<event_log::EventLog>,
) {
    if !keyboard.just_pressed(KeyCode::KeyI) {
        return;
    }

    // Determine what the new behavior should be based on the primary camera.
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

    for camera in second_window::all_cameras(&scene, second.as_deref()) {
        if let Ok(mut behavior) = behavior_query.get_mut(camera) {
            *behavior = new_behavior;
        } else {
            commands.entity(camera).insert(new_behavior);
        }
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
    second: Option<Res<second_window::SecondWindowEntities>>,
    mut policy_query: Query<&mut AnimationConflictPolicy>,
    mut hint_query: Query<&mut Text, With<ui::AnimationConflictPolicyLabel>>,
    mut log: ResMut<event_log::EventLog>,
) {
    if !keyboard.just_pressed(KeyCode::KeyQ) {
        return;
    }

    // Determine what the new policy should be based on the primary camera.
    let new_policy =
        policy_query
            .get(scene.camera)
            .map_or(AnimationConflictPolicy::FirstWins, |policy| match *policy {
                AnimationConflictPolicy::LastWins => AnimationConflictPolicy::FirstWins,
                AnimationConflictPolicy::FirstWins => AnimationConflictPolicy::LastWins,
            });

    for camera in second_window::all_cameras(&scene, second.as_deref()) {
        if let Ok(mut policy) = policy_query.get_mut(camera) {
            *policy = new_policy;
        } else {
            commands.entity(camera).insert(new_policy);
        }
    }

    for mut text in &mut hint_query {
        **text = conflict_policy_hint_text(new_policy);
    }
    log.push(format!("AnimationConflictPolicy: {new_policy:?}"));
}
