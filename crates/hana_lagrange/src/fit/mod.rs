//! Camera fit: framing a target's bounds in the viewport. Holds the fit-family
//! triggers (`ZoomToFit`, `AnimateToFit`, `LookAt`, `LookAtAndZoomToFit`,
//! `SetFitTarget`) with the observers that drive them, the projection geometry
//! the solve relies on, and the optional debug overlay.

mod camera_pose;
mod constants;
mod geometry;
mod target;
mod triggers;

#[cfg(feature = "fit_overlay")]
mod overlay;

use bevy::prelude::*;
pub use geometry::FitAnchor;
#[cfg(feature = "fit_overlay")]
pub use overlay::FitOverlay;
#[cfg(feature = "fit_overlay")]
use overlay::FitOverlayPlugin;
#[cfg(feature = "fit_overlay")]
pub use overlay::FitTargetOverlayConfig;
pub use target::CurrentFitTarget;
pub use target::SetFitTarget;
pub use triggers::AnimateToFit;
pub use triggers::LookAt;
pub use triggers::LookAtAndZoomToFit;
pub use triggers::ZoomBegin;
pub use triggers::ZoomContext;
pub use triggers::ZoomEnd;
pub use triggers::ZoomReason;
pub use triggers::ZoomToFit;
pub(crate) use triggers::on_free_cam_animate_to_fit;
pub(crate) use triggers::on_free_cam_look_at;
pub(crate) use triggers::on_free_cam_look_at_and_zoom_to_fit;
pub(crate) use triggers::on_free_cam_zoom_to_fit;
pub(crate) use triggers::on_orbit_cam_animate_to_fit;
pub(crate) use triggers::on_orbit_cam_look_at;
pub(crate) use triggers::on_orbit_cam_look_at_and_zoom_to_fit;
pub(crate) use triggers::on_orbit_cam_zoom_to_fit;

/// Registers the camera-fit domain's shared target lifecycle, plus the optional
/// debug overlay when the `fit_overlay` feature is enabled. Per-camera fit,
/// zoom, and look observers are registered by each [`CameraKind`](crate::CameraKind).
/// The core fit solve is pure logic with nothing to register.
pub(crate) struct FitPlugin;

impl Plugin for FitPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(target::on_set_fit_target);

        #[cfg(feature = "fit_overlay")]
        app.add_plugins(FitOverlayPlugin);
    }
}
