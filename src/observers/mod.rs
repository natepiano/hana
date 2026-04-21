//! Observers that wire events to camera behavior.

use bevy::prelude::*;

mod animation;
mod fit;
mod look;
mod shared;

/// Registers every observer in the `observers` domain.
pub(crate) struct ObserverPlugin;

impl Plugin for ObserverPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(animation::on_camera_move_list_added)
            .add_observer(animation::on_play_animation)
            .add_observer(animation::restore_camera_state)
            .add_observer(fit::on_animate_to_fit)
            .add_observer(fit::on_set_fit_target)
            .add_observer(fit::on_zoom_to_fit)
            .add_observer(look::on_look_at)
            .add_observer(look::on_look_at_and_zoom_to_fit);
    }
}
