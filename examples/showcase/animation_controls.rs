use super::*;

pub(super) fn toggle_debug_overlay(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut commands: Commands,
    scene: Res<SceneEntities>,
    second: Option<Res<second_window::SecondWindowEntities>>,
    viz_query: Query<(), With<FitOverlay>>,
    windows: Query<&Window>,
) {
    if !keyboard.just_pressed(KeyCode::KeyD) {
        return;
    }

    let camera = second_window::focused_camera(&scene, second.as_deref(), &windows);
    if viz_query.get(camera).is_ok() {
        commands.entity(camera).remove::<FitOverlay>();
    } else {
        commands.entity(camera).insert(FitOverlay);
    }
}

pub(super) fn animate_camera(
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
    let half_pi = PI / 2.0;
    let yaw = orbit_camera.target_yaw;
    let pitch = orbit_camera.target_pitch;
    let radius = orbit_camera.target_radius;
    let focus = orbit_camera.target_focus;

    let camera_moves = [
        CameraMove::ToOrbit {
            focus,
            yaw: yaw + half_pi,
            pitch,
            radius,
            duration: Duration::from_millis(ORBIT_MOVE_DURATION_MS),
            easing: easing_function,
        },
        CameraMove::ToOrbit {
            focus,
            yaw: yaw + half_pi * 2.0,
            pitch,
            radius,
            duration: Duration::from_millis(ORBIT_MOVE_DURATION_MS),
            easing: easing_function,
        },
        CameraMove::ToOrbit {
            focus,
            yaw: yaw + half_pi * 3.0,
            pitch,
            radius,
            duration: Duration::from_millis(ORBIT_MOVE_DURATION_MS),
            easing: easing_function,
        },
        CameraMove::ToOrbit {
            focus,
            yaw: yaw + half_pi * 4.0,
            pitch,
            radius,
            duration: Duration::from_millis(ORBIT_MOVE_DURATION_MS),
            easing: easing_function,
        },
    ];

    commands.trigger(PlayAnimation::new(camera, camera_moves));
}

pub(super) fn randomize_easing(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut easing: ResMut<ActiveEasing>,
    time: Res<Time>,
    mut log: ResMut<event_log::EventLog>,
) {
    if keyboard.just_pressed(KeyCode::KeyR) {
        let index = (time.elapsed_secs() * 1000.0).to_usize() % ALL_EASINGS.len();
        easing.0 = ALL_EASINGS[index];
        log.push(format!("Easing: {:#?}", easing.0));
    }
    if keyboard.just_pressed(KeyCode::KeyE) {
        easing.0 = EaseFunction::CubicOut;
        log.push("Easing: reset to CubicOut".into());
    }
}

pub(super) fn animate_fit_to_scene(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut commands: Commands,
    scene: Res<SceneEntities>,
    second: Option<Res<second_window::SecondWindowEntities>>,
    easing: Res<ActiveEasing>,
    windows: Query<&Window>,
) {
    if !keyboard.just_pressed(KeyCode::KeyH) {
        return;
    }

    let camera = second_window::focused_camera(&scene, second.as_deref(), &windows);
    commands.trigger(
        AnimateToFit::new(camera, scene.scene_bounds)
            .yaw(CAMERA_START_YAW)
            .pitch(CAMERA_START_PITCH)
            .margin(ZOOM_MARGIN_SCENE)
            .duration(Duration::from_millis(ANIMATE_FIT_DURATION_MS))
            .easing(easing.0),
    );
}

/// Toggles between perspective and orthographic projection, then re-fits the scene.
///
/// The fit is deferred one frame via `pending_fit` because `OrbitCam` needs to
/// process the projection change (syncing radius ↔ orthographic scale) before the
/// fit calculation can produce correct results.
pub(super) fn toggle_projection(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut commands: Commands,
    scene: Res<SceneEntities>,
    second: Option<Res<second_window::SecondWindowEntities>>,
    active_easing: Res<ActiveEasing>,
    mut camera_query: Query<(&mut Projection, &mut OrbitCam)>,
    mut log: ResMut<event_log::EventLog>,
    mut pending_fit: Local<bool>,
) {
    // Deferred fit: projection was changed last frame, `OrbitCam` has now synced.
    if *pending_fit {
        *pending_fit = false;
        for camera in second_window::all_cameras(&scene, second.as_deref()) {
            commands.trigger(
                AnimateToFit::new(camera, scene.scene_bounds)
                    .yaw(CAMERA_START_YAW)
                    .pitch(CAMERA_START_PITCH)
                    .margin(ZOOM_MARGIN_SCENE)
                    .duration(Duration::from_millis(ANIMATE_FIT_DURATION_MS))
                    .easing(active_easing.0),
            );
        }
        return;
    }

    if !keyboard.just_pressed(KeyCode::KeyP) {
        return;
    }
    let mut logged = false;
    for camera in second_window::all_cameras(&scene, second.as_deref()) {
        let Ok((mut projection, mut orbit_camera)) = camera_query.get_mut(camera) else {
            continue;
        };
        match *projection {
            Projection::Perspective(_) => {
                *projection = Projection::from(OrthographicProjection {
                    scaling_mode: ScalingMode::FixedVertical {
                        viewport_height: 1.0,
                    },
                    far: 40.0,
                    ..OrthographicProjection::default_3d()
                });
                if !logged {
                    log.push("Projection: Orthographic".into());
                    logged = true;
                }
            },
            Projection::Orthographic(_) => {
                *projection = Projection::Perspective(PerspectiveProjection::default());
                if !logged {
                    log.push("Projection: Perspective".into());
                    logged = true;
                }
            },
            Projection::Custom(_) => {},
        }
        orbit_camera.force_update = ForceUpdate::Pending;
    }
    if logged {
        *pending_fit = true;
    }
}

pub(super) fn interrupt_behavior_hint_text(behavior: CameraInputInterruptBehavior) -> String {
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

pub(super) fn toggle_interrupt_behavior(
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

pub(super) fn conflict_policy_hint_text(policy: AnimationConflictPolicy) -> String {
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

pub(super) fn toggle_animation_conflict_policy(
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
