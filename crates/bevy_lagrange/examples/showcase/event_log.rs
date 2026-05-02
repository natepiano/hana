use super::*;

#[derive(Component)]
pub(crate) struct EventLogNode;

#[derive(Component)]
pub(crate) struct EventLogHint;

#[derive(Component)]
pub(crate) struct EventLogToggleHint;

/// Marker resource: when present, the next `AnimationEnd` enables the event log.
#[derive(Resource)]
pub(super) struct EnableLogOnAnimationEnd;

struct PendingLogEntry {
    text:  String,
    color: Color,
}

#[derive(Default, PartialEq, Eq)]
enum EventLogState {
    #[default]
    Disabled,
    Enabled,
}

#[derive(Resource, Default)]
pub(crate) struct EventLog {
    state:   EventLogState,
    pending: Vec<PendingLogEntry>,
}

pub(crate) fn spawn_ui(commands: &mut Commands, camera: Entity) {
    // Event log scroll container (right edge, scrollable, hidden until enabled)
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(UI_SCREEN_PADDING_PIXELS),
            right: Val::Px(UI_SCREEN_PADDING_PIXELS),
            width: Val::Px(EVENT_LOG_WIDTH),
            bottom: Val::Px(EVENT_LOG_PANEL_BOTTOM_PIXELS),
            flex_direction: FlexDirection::Column,
            overflow: Overflow::scroll_y(),
            ..default()
        },
        Visibility::Hidden,
        Pickable::IGNORE,
        EventLogNode,
        UiTargetCamera(camera),
    ));

    // Log toggle hint (bottom-right, always visible once initial animation completes)
    commands.spawn((
        Text::new("'L' toggle log off and on"),
        TextFont {
            font_size: UI_FONT_SIZE,
            ..default()
        },
        TextColor(Color::srgba(0.7, 0.7, 0.7, 0.7)),
        TextLayout::new_with_justify(Justify::Left),
        Node {
            position_type: PositionType::Absolute,
            bottom: Val::Px(UI_SCREEN_PADDING_PIXELS),
            right: Val::Px(UI_SCREEN_PADDING_PIXELS),
            width: Val::Px(EVENT_LOG_WIDTH),
            ..default()
        },
        Visibility::Hidden,
        EventLogToggleHint,
        UiTargetCamera(camera),
    ));

    // Log scroll/clear hints (bottom-right, hidden until log enabled)
    commands.spawn((
        Text::new("Up/Down scroll log\n'C' clear log"),
        TextFont {
            font_size: UI_FONT_SIZE,
            ..default()
        },
        TextColor(Color::srgba(0.7, 0.7, 0.7, 0.7)),
        TextLayout::new_with_justify(Justify::Left),
        Node {
            position_type: PositionType::Absolute,
            bottom: Val::Px(EVENT_LOG_HINT_BOTTOM_PIXELS),
            right: Val::Px(UI_SCREEN_PADDING_PIXELS),
            width: Val::Px(EVENT_LOG_WIDTH),
            ..default()
        },
        Visibility::Hidden,
        EventLogHint,
        UiTargetCamera(camera),
    ));
}

/// Enables the event log when the initial `AnimateToFit` animation completes.
pub(crate) fn enable_log_on_initial_fit(
    _animation_end: On<AnimationEnd>,
    mut commands: Commands,
    marker: Option<Res<EnableLogOnAnimationEnd>>,
    mut log: ResMut<EventLog>,
    mut container_query: Query<
        &mut Visibility,
        (
            With<EventLogNode>,
            Without<EventLogHint>,
            Without<EventLogToggleHint>,
        ),
    >,
    mut hint_query: Query<
        &mut Visibility,
        (
            With<EventLogHint>,
            Without<EventLogNode>,
            Without<EventLogToggleHint>,
        ),
    >,
    mut toggle_hint_query: Query<
        &mut Visibility,
        (
            With<EventLogToggleHint>,
            Without<EventLogNode>,
            Without<EventLogHint>,
        ),
    >,
) {
    if marker.is_none() {
        return;
    }
    commands.remove_resource::<EnableLogOnAnimationEnd>();
    log.state = EventLogState::Enabled;
    for mut visibility in &mut container_query {
        *visibility = Visibility::Inherited;
    }
    for mut visibility in &mut hint_query {
        *visibility = Visibility::Inherited;
    }
    for mut visibility in &mut toggle_hint_query {
        *visibility = Visibility::Inherited;
    }
}

pub(crate) fn toggle_event_log(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut commands: Commands,
    mut log: ResMut<EventLog>,
    mut container_query: Query<
        (Entity, &mut Visibility, &mut ScrollPosition),
        (With<EventLogNode>, Without<EventLogHint>),
    >,
    children_query: Query<&Children>,
    mut hint_query: Query<&mut Visibility, (With<EventLogHint>, Without<EventLogNode>)>,
) {
    if !keyboard.just_pressed(KeyCode::KeyL) {
        return;
    }

    log.state = match log.state {
        EventLogState::Disabled => EventLogState::Enabled,
        EventLogState::Enabled => EventLogState::Disabled,
    };

    if log.state == EventLogState::Enabled {
        for (_, mut visibility, _) in &mut container_query {
            *visibility = Visibility::Inherited;
        }
        for mut visibility in &mut hint_query {
            *visibility = Visibility::Inherited;
        }
    } else {
        // Clear log entries and hide.
        for (entity, mut visibility, mut scroll) in &mut container_query {
            if let Ok(children) = children_query.get(entity) {
                for child in children.iter() {
                    commands.entity(child).despawn();
                }
            }
            scroll.y = 0.0;
            *visibility = Visibility::Hidden;
        }
        for mut visibility in &mut hint_query {
            *visibility = Visibility::Hidden;
        }
        log.pending.clear();
    }
}

impl EventLog {
    pub(crate) fn push(&mut self, text: String) {
        if self.state == EventLogState::Disabled {
            return;
        }
        self.pending.push(PendingLogEntry {
            text,
            color: EVENT_LOG_COLOR,
        });
    }

    fn push_error(&mut self, text: String) {
        if self.state == EventLogState::Disabled {
            return;
        }
        self.pending.push(PendingLogEntry {
            text,
            color: EVENT_LOG_ERROR_COLOR,
        });
    }

    fn separator(&mut self) {
        if self.state == EventLogState::Disabled {
            return;
        }
        self.pending.push(PendingLogEntry {
            text:  EVENT_LOG_SEPARATOR.into(),
            color: EVENT_LOG_COLOR,
        });
    }
}

fn fmt_vec3(vector: Vec3) -> String {
    format!("({:.1}, {:.1}, {:.1})", vector.x, vector.y, vector.z)
}

pub(crate) fn log_animation_begin(event: On<AnimationBegin>, mut log: ResMut<EventLog>) {
    log.push(format!("AnimationBegin\n  source={:?}", event.source));
}

pub(crate) fn log_animation_end(event: On<AnimationEnd>, mut log: ResMut<EventLog>) {
    log.push(format!("AnimationEnd\n  source={:?}", event.source));
    if event.source != AnimationSource::ZoomToFit {
        log.separator();
    }
}

pub(crate) fn log_camera_move_start(event: On<CameraMoveBegin>, mut log: ResMut<EventLog>) {
    log.push(format!(
        "CameraMoveBegin\n  translation={}\n  focus={}\n  duration={:.0}ms\n  easing={:?}",
        fmt_vec3(event.camera_move.translation()),
        fmt_vec3(event.camera_move.focus()),
        event.camera_move.duration_ms(),
        event.camera_move.easing(),
    ));
}

pub(crate) fn log_camera_move_end(_camera_move_end: On<CameraMoveEnd>, mut log: ResMut<EventLog>) {
    log.push("CameraMoveEnd".into());
}

pub(crate) fn log_zoom_begin(event: On<ZoomBegin>, mut log: ResMut<EventLog>) {
    log.push(format!(
        "ZoomBegin\n  margin={:.2}\n  duration={:.0}ms\n  easing={:?}",
        event.margin,
        event.duration.as_secs_f32() * 1000.0,
        event.easing,
    ));
}

pub(crate) fn log_zoom_end(_zoom_end: On<ZoomEnd>, mut log: ResMut<EventLog>) {
    log.push("ZoomEnd".into());
    log.separator();
}

pub(crate) fn log_animation_cancelled(event: On<AnimationCancelled>, mut log: ResMut<EventLog>) {
    log.push_error(format!(
        "AnimationCancelled\n  source={:?}\n  move_translation={}\n  move_focus={}",
        event.source,
        fmt_vec3(event.camera_move.translation()),
        fmt_vec3(event.camera_move.focus()),
    ));
}

pub(crate) fn log_zoom_cancelled(_zoom_cancelled: On<ZoomCancelled>, mut log: ResMut<EventLog>) {
    log.push_error("ZoomCancelled".into());
}

pub(crate) fn log_animation_rejected(event: On<AnimationRejected>, mut log: ResMut<EventLog>) {
    log.push_error(format!("AnimationRejected\n  source={:?}", event.source));
}

/// Spawns pending log entries as child `Text` nodes inside the scroll container
/// and auto-scrolls to the bottom.
pub(crate) fn update_event_log_text(
    mut commands: Commands,
    mut log: ResMut<EventLog>,
    container_query: Query<(Entity, &ComputedNode), With<EventLogNode>>,
    mut scroll_query: Query<&mut ScrollPosition, With<EventLogNode>>,
) {
    if log.pending.is_empty() {
        return;
    }

    let Ok((container, computed)) = container_query.single() else {
        return;
    };

    for entry in log.pending.drain(..) {
        commands.entity(container).with_child((
            Text::new(entry.text),
            TextFont {
                font_size: EVENT_LOG_FONT_SIZE,
                ..default()
            },
            TextColor(entry.color),
        ));
    }

    // Auto-scroll to bottom.
    if let Ok(mut scroll) = scroll_query.single_mut() {
        let content_height = computed.content_size().y;
        let container_height = computed.size().y;
        let max_scroll =
            (content_height - container_height).max(0.0) * computed.inverse_scale_factor();
        scroll.y = EVENT_LOG_SCROLL_SPEED.mul_add(4.0, max_scroll);
    }
}

/// Scrolls the event log with Up/Down arrow keys, clears with `C`.
pub(crate) fn scroll_event_log(
    mut commands: Commands,
    keyboard: Res<ButtonInput<KeyCode>>,
    mut scroll_query: Query<(Entity, &mut ScrollPosition, &ComputedNode), With<EventLogNode>>,
    children_query: Query<&Children>,
) {
    let Ok((container, mut scroll, computed)) = scroll_query.single_mut() else {
        return;
    };

    if keyboard.just_pressed(KeyCode::KeyC) {
        if let Ok(children) = children_query.get(container) {
            for child in children.iter() {
                commands.entity(child).despawn();
            }
        }
        scroll.y = 0.0;
        return;
    }

    let delta_y = if keyboard.pressed(KeyCode::ArrowDown) {
        EVENT_LOG_SCROLL_SPEED
    } else if keyboard.pressed(KeyCode::ArrowUp) {
        -EVENT_LOG_SCROLL_SPEED
    } else {
        return;
    };

    let max_scroll =
        (computed.content_size().y - computed.size().y).max(0.0) * computed.inverse_scale_factor();
    scroll.y = (scroll.y + delta_y).clamp(0.0, max_scroll);
}
