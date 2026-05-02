use bevy::window::WindowFocused;

use super::*;

#[derive(Resource)]
pub(crate) struct SecondWindowEntities {
    pub(crate) window: Entity,
    pub(crate) camera: Entity,
}

#[derive(Component)]
struct SecondWindowCamera;

#[derive(Component)]
pub(crate) struct WindowLabel(Timer);

pub(crate) fn all_cameras(
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
pub(crate) fn focused_camera(
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

pub(crate) fn toggle_second_window(
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
        // Despawn only the window — `on_second_window_removed` handles the
        // camera despawn, resource removal, and "closed" log entry.
        commands.entity(second_window.window).despawn();
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

pub(crate) fn log_window_focus(
    second: Option<Res<SecondWindowEntities>>,
    mut focus_events: MessageReader<WindowFocused>,
    mut log: ResMut<event_log::EventLog>,
) {
    let Some(second_window) = second else {
        focus_events.clear();
        return;
    };

    for event in focus_events.read() {
        if !event.focused {
            continue;
        }
        let label = if event.window == second_window.window {
            "Window 2"
        } else {
            "Window 1"
        };
        log.push(format!("{label} focused"));
    }
}

pub(crate) fn on_second_window_removed(
    trigger: On<Remove, Window>,
    mut commands: Commands,
    second: Option<Res<SecondWindowEntities>>,
    mut log: ResMut<event_log::EventLog>,
) {
    let Some(second_window) = second else {
        return;
    };
    if second_window.window != trigger.entity {
        return;
    }

    commands.entity(second_window.camera).despawn();
    commands.remove_resource::<SecondWindowEntities>();
    log.push("Window 2: closed".into());
}

pub(crate) fn despawn_window_labels(
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
