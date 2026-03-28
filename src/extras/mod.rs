//! Camera extras: zoom-to-fit, queued animations, and debug visualization.
//!
//! Enabled via the `zoom_overlay` feature flag. All public types are re-exported
//! at the crate root.

mod animation;
mod components;
mod events;
mod fit;
mod observers;
mod support;
#[cfg(feature = "zoom_overlay")]
mod visualization;

pub use animation::CameraMove;
pub use animation::CameraMoveList;
use bevy::prelude::*;
pub use components::AnimationConflictPolicy;
pub use components::CameraInputInterruptBehavior;
pub use components::CurrentFitTarget;
pub use components::FitVisualization;
pub use events::AnimateToFit;
pub use events::AnimationBegin;
pub use events::AnimationCancelled;
pub use events::AnimationEnd;
pub use events::AnimationRejected;
pub use events::AnimationSource;
pub use events::CameraMoveBegin;
pub use events::CameraMoveEnd;
pub use events::LookAt;
pub use events::LookAtAndZoomToFit;
pub use events::PlayAnimation;
pub use events::SetFitTarget;
pub use events::ZoomBegin;
pub use events::ZoomCancelled;
pub use events::ZoomContext;
pub use events::ZoomEnd;
pub use events::ZoomToFit;
#[cfg(feature = "zoom_overlay")]
pub use visualization::FitTargetVisualizationConfig;

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

    #[cfg(feature = "zoom_overlay")]
    app.add_plugins(visualization::VisualizationPlugin);
}
