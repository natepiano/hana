use bevy_diegetic::DiegeticText;

use super::*;

#[derive(Component)]
pub(crate) struct PausedOverlay;

pub(crate) fn spawn_ui(commands: &mut Commands) {
    policy_panel::spawn_policy_panel(commands);

    event_log::spawn_log_panel(commands);

    // Paused overlay: a screen-space text label centered on the window, hidden
    // until Esc toggles the pause state.
    let overlay = DiegeticText::screen(PAUSED_TEXT)
        .size(PAUSED_OVERLAY_FONT_SIZE)
        .color(OVERLAY_TEXT_COLOR)
        .anchor(Anchor::Center)
        .spawn(commands);
    commands
        .entity(overlay)
        .insert((PausedOverlay, Visibility::Hidden));
}

pub(crate) fn toggle_pause(
    mut time: ResMut<Time<Virtual>>,
    mut overlay: Query<&mut Visibility, With<PausedOverlay>>,
) {
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
