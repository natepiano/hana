use super::*;

#[derive(Component)]
pub(crate) struct CameraInputInterruptBehaviorLabel;

#[derive(Component)]
pub(crate) struct AnimationConflictPolicyLabel;

#[derive(Component)]
pub(crate) struct PausedOverlay;

pub(crate) fn spawn_ui(commands: &mut Commands, camera: Entity) {
    // Interrupt behavior hint (bottom-left)
    commands.spawn((
        Text::new(animation_controls::interrupt_behavior_hint_text(
            CameraInputInterruptBehavior::Ignore,
        )),
        TextFont {
            font_size: FontSize::Px(UI_FONT_SIZE),
            ..default()
        },
        TextColor(HINT_TEXT_COLOR),
        Node {
            position_type: PositionType::Absolute,
            bottom: Val::Px(UI_SCREEN_PADDING_PIXELS),
            left: Val::Px(UI_SCREEN_PADDING_PIXELS),
            ..default()
        },
        CameraInputInterruptBehaviorLabel,
        UiTargetCamera(camera),
    ));

    // Conflict policy hint (bottom-left, above interrupt behavior)
    commands.spawn((
        Text::new(animation_controls::conflict_policy_hint_text(
            AnimationConflictPolicy::LastWins,
        )),
        TextFont {
            font_size: FontSize::Px(UI_FONT_SIZE),
            ..default()
        },
        TextColor(HINT_TEXT_COLOR),
        Node {
            position_type: PositionType::Absolute,
            bottom: Val::Px(CONFLICT_POLICY_HINT_BOTTOM_PIXELS),
            left: Val::Px(UI_SCREEN_PADDING_PIXELS),
            ..default()
        },
        AnimationConflictPolicyLabel,
        UiTargetCamera(camera),
    ));

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
