use super::*;

#[derive(Component)]
pub(crate) struct PausedOverlay;

pub(crate) fn spawn_ui(commands: &mut Commands, camera: Entity) {
    policy_panel::spawn_policy_panel(commands);

    event_log::spawn_log_panel(commands);

    // Paused overlay (centered, hidden until Esc)
    commands.spawn((
        Text::new(PAUSED_TEXT),
        TextFont {
            font_size: FontSize::Px(PAUSED_OVERLAY_FONT_SIZE),
            ..default()
        },
        TextColor(OVERLAY_TEXT_COLOR),
        TextLayout::justify(Justify::Center),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Percent(PAUSED_OVERLAY_TOP_PERCENT),
            width: Val::Percent(FULL_WIDTH_PERCENT),
            ..default()
        },
        Visibility::Hidden,
        PausedOverlay,
        UiTargetCamera(camera),
    ));
}

pub(crate) fn toggle_pause(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut time: ResMut<Time<Virtual>>,
    mut overlay: Query<&mut Visibility, With<PausedOverlay>>,
) {
    if !keyboard.just_pressed(KeyCode::Escape) {
        return;
    }
    if time.is_paused() {
        time.unpause();
        for mut visibility in &mut overlay {
            *visibility = Visibility::Hidden;
        }
    } else {
        time.pause();
        for mut visibility in &mut overlay {
            *visibility = Visibility::Inherited;
        }
    }
}
