use super::*;

#[derive(Component)]
pub(super) struct CameraInputInterruptBehaviorLabel;

#[derive(Component)]
pub(super) struct AnimationConflictPolicyLabel;

#[derive(Component)]
pub(super) struct PausedOverlay;

pub(super) fn spawn_ui(commands: &mut Commands, camera: Entity) {
    // Instructions
    commands.spawn((
        Text::new("Click a mesh to zoom-to-fit\nClick the ground to zoom back out\n\nPress:\n'Esc' pause / unpause\n'P' toggle projection\n'D' debug overlay\n'H' Home w/animate fit to scene\n'A' animate camera\n'F' look at hovered mesh\n'G' look at + zoom-to-fit hovered mesh\n'R' randomize easing\n'E' reset to 'CubicOut' easing\n'I' toggle interrupt behavior\n'Q' cycle conflict policy\n'W' toggle second window"),
        TextFont {
            font_size: UI_FONT_SIZE,
            ..default()
        },
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(UI_SCREEN_PADDING_PX),
            left: Val::Px(UI_SCREEN_PADDING_PX),
            ..default()
        },
        UiTargetCamera(camera),
    ));

    // Interrupt behavior hint (bottom-left)
    commands.spawn((
        Text::new(animation_controls::interrupt_behavior_hint_text(
            CameraInputInterruptBehavior::Ignore,
        )),
        TextFont {
            font_size: UI_FONT_SIZE,
            ..default()
        },
        TextColor(Color::srgba(0.7, 0.7, 0.7, 0.7)),
        Node {
            position_type: PositionType::Absolute,
            bottom: Val::Px(UI_SCREEN_PADDING_PX),
            left: Val::Px(UI_SCREEN_PADDING_PX),
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
            font_size: UI_FONT_SIZE,
            ..default()
        },
        TextColor(Color::srgba(0.7, 0.7, 0.7, 0.7)),
        Node {
            position_type: PositionType::Absolute,
            bottom: Val::Px(CONFLICT_POLICY_HINT_BOTTOM_PX),
            left: Val::Px(UI_SCREEN_PADDING_PX),
            ..default()
        },
        AnimationConflictPolicyLabel,
        UiTargetCamera(camera),
    ));

    event_log::spawn_ui(commands, camera);

    // Paused overlay (centered, hidden until Esc)
    commands.spawn((
        Text::new("PAUSED"),
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
        Visibility::Hidden,
        PausedOverlay,
        UiTargetCamera(camera),
    ));
}

pub(super) fn toggle_pause(
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
