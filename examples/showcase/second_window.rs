use super::*;

#[derive(Resource)]
pub(super) struct SecondWindowEntities {
    pub(super) window: Entity,
    pub(super) camera: Entity,
}

#[derive(Component)]
struct SecondWindowCamera;

#[derive(Component)]
pub(super) struct WindowLabel(Timer);

pub(super) fn all_cameras(
    scene: &SceneEntities,
    second: Option<&SecondWindowEntities>,
) -> Vec<Entity> {
    let mut cameras = vec![scene.camera];
    if let Some(second_window) = second {
        cameras.push(second_window.camera);
    }
    cameras
}

/// Returns the camera entity whose window is currently focused.
pub(super) fn focused_camera(
    scene: &SceneEntities,
    second: Option<&SecondWindowEntities>,
    windows: &Query<&Window>,
) -> Entity {
    if let Some(second_window) = second
        && let Ok(window) = windows.get(second_window.window)
        && window.focused
    {
        second_window.camera
    } else {
        // Primary window is the default fallback
        scene.camera
    }
}

pub(super) fn toggle_second_window(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut commands: Commands,
    scene: Res<SceneEntities>,
    second: Option<Res<SecondWindowEntities>>,
    easing: Res<ActiveEasing>,
    camera_query: Query<&OrbitCam>,
    mut log: ResMut<event_log::EventLog>,
) {
    if !keyboard.just_pressed(KeyCode::KeyW) {
        return;
    }

    if let Some(second_window) = second {
        commands.entity(second_window.window).despawn();
        commands.entity(second_window.camera).despawn();
        commands.remove_resource::<SecondWindowEntities>();
        log.push("Window 2: closed".into());
        return;
    }

    let window = commands
        .spawn((
            Window {
                title: "extras - window 2".into(),
                ..default()
            },
            ManagedWindow {
                name: "window_2".into(),
            },
        ))
        .id();

    // Clone settings from primary camera
    let Ok(primary) = camera_query.get(scene.camera) else {
        return;
    };
    let mut second_camera = *primary;
    second_camera.yaw = Some(CAMERA_START_YAW);
    second_camera.pitch = Some(CAMERA_START_PITCH);

    let camera = commands
        .spawn((
            second_camera,
            RenderTarget::Window(WindowRef::Entity(window)),
            SecondWindowCamera,
        ))
        .id();

    commands.trigger(
        AnimateToFit::new(camera, scene.scene_bounds)
            .yaw(CAMERA_START_YAW)
            .pitch(CAMERA_START_PITCH)
            .margin(ZOOM_MARGIN_SCENE)
            .easing(easing.0),
    );

    // `Window 2` label centered in the second window, auto-despawns after a couple seconds.
    commands.spawn((
        Text::new("Window 2"),
        TextFont {
            font_size: PAUSED_OVERLAY_FONT_SIZE,
            ..default()
        },
        TextColor(Color::srgba(1.0, 1.0, 1.0, 0.4)),
        TextLayout::new_with_justify(Justify::Center),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Percent(PAUSED_OVERLAY_TOP_PERCENT),
            width: Val::Percent(100.0),
            ..default()
        },
        UiTargetCamera(camera),
        WindowLabel(Timer::from_seconds(
            WINDOW_LABEL_DURATION_SECS,
            TimerMode::Once,
        )),
    ));

    commands.insert_resource(SecondWindowEntities { window, camera });
    log.push("Window 2: opened".into());
}

pub(super) fn log_window_focus(
    second: Option<Res<SecondWindowEntities>>,
    windows: Query<(Entity, &Window)>,
    mut log: ResMut<event_log::EventLog>,
    mut last_focused: Local<Option<Entity>>,
) {
    let Some(second_window) = second else {
        *last_focused = None;
        return;
    };

    let current_focused = windows
        .iter()
        .find(|(_, window)| window.focused)
        .map(|(entity, _)| entity);

    if current_focused != *last_focused {
        if let Some(focused) = current_focused {
            let label = if focused == second_window.window {
                "Window 2"
            } else {
                "Window 1"
            };
            log.push(format!("{label} focused"));
        }
        *last_focused = current_focused;
    }
}

pub(super) fn cleanup_second_window(
    mut commands: Commands,
    second: Option<Res<SecondWindowEntities>>,
    windows: Query<(), With<Window>>,
    closing: Query<(), With<ClosingWindow>>,
    mut log: ResMut<event_log::EventLog>,
) {
    let Some(second_window) = second else {
        return;
    };
    // Detect both already-despawned windows (`is_err`) and windows marked for close this
    // frame (`ClosingWindow`). Catching `ClosingWindow` ensures the camera is despawned
    // in the same command flush as the window, preventing `camera_system` from hitting a
    // stale `RenderTarget`.
    if windows.get(second_window.window).is_err() || closing.get(second_window.window).is_ok() {
        commands.entity(second_window.camera).despawn();
        commands.remove_resource::<SecondWindowEntities>();
        log.push("Window 2: closed".into());
    }
}

pub(super) fn despawn_window_labels(
    mut commands: Commands,
    time: Res<Time>,
    mut query: Query<(Entity, &mut WindowLabel)>,
) {
    for (entity, mut label) in &mut query {
        label.0.tick(time.delta());
        if label.0.just_finished() {
            commands.entity(entity).despawn();
        }
    }
}
