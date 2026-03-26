//! Camera extras: zoom-to-fit, queued animations, and debug visualization.
//!
//! Enabled via the `extras_debug` feature flag. All public types are re-exported
//! at the crate root.

pub mod animation;
pub mod components;
pub mod events;
pub mod fit;
mod observers;
mod support;
#[cfg(feature = "extras_debug")]
pub mod visualization;

use bevy::prelude::*;

/// Registers extras observers, systems, and optional visualization.
pub fn build_extras(app: &mut App) {
    app.add_observer(observers::on_camera_move_list_added)
        .add_observer(observers::restore_camera_state)
        .add_observer(observers::on_zoom_to_fit)
        .add_observer(observers::on_play_animation)
        .add_observer(observers::on_set_fit_target)
        .add_observer(observers::on_animate_to_fit)
        .add_observer(observers::on_look_at)
        .add_observer(observers::on_look_at_and_zoom_to_fit)
        .add_systems(Update, animation::process_camera_move_list);

    #[cfg(feature = "extras_debug")]
    app.add_plugins(visualization::VisualizationPlugin);
}
