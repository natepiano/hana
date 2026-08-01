//! Camera animation: the kind-agnostic move queue and state machine
//! ([`CameraMoveList`], [`CameraMove`]), the lifecycle events both fire, and the
//! observers that resolve conflicts and stash/restore camera state.

mod constants;
mod events;
mod lifecycle;
mod queue;

use bevy::prelude::*;
pub use events::AnimationBegin;
pub use events::AnimationEnd;
pub use events::AnimationReason;
pub use events::AnimationRejected;
pub use events::AnimationSource;
pub use events::CameraMoveBegin;
pub use events::CameraMoveEnd;
pub use events::PlayAnimation;
pub use lifecycle::AnimationConflictPolicy;
// Internals reached only from sibling-domain tests (`fit/look`, `input/lifecycle`);
// the queue system and markers are used in non-test builds via `queue::` directly.
#[cfg(test)]
pub(crate) use queue::AnimationSourceMarker;
pub use queue::CameraInputInterruptBehavior;
pub use queue::CameraMove;
pub use queue::CameraMoveList;
#[cfg(test)]
pub(crate) use queue::ZoomAnimationMarker;
pub(crate) use queue::orbital_parameters_from_offset;
#[cfg(test)]
pub(crate) use queue::process_orbit_camera_move_list;

/// Registers the camera-animation domain: the move-queue system that drives
/// in-flight animations and the observers that begin them, resolve conflicts,
/// and stash/restore camera state.
pub(crate) struct AnimationPlugin;

impl Plugin for AnimationPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(lifecycle::on_camera_move_list_added)
            .add_observer(lifecycle::on_play_animation)
            .add_observer(lifecycle::restore_camera_state);
    }
}

pub(crate) fn add_orbit_cam_animation_systems(app: &mut App) {
    app.add_systems(Update, queue::process_orbit_camera_move_list);
}

pub(crate) fn add_free_cam_animation_systems(app: &mut App) {
    app.add_systems(Update, queue::process_free_camera_move_list);
}
