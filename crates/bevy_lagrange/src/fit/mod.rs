//! Camera fit: solve a zoom and focus that frame a target's bounds in the
//! viewport, the projection geometry the solve relies on, and the optional
//! debug overlay.

mod projection;
mod solve;

#[cfg(feature = "fit_overlay")]
mod overlay;

use bevy::prelude::*;
#[cfg(feature = "fit_overlay")]
pub use overlay::FitOverlay;
#[cfg(feature = "fit_overlay")]
use overlay::FitOverlayPlugin;
#[cfg(feature = "fit_overlay")]
pub use overlay::FitTargetOverlayConfig;
pub(crate) use projection::extract_mesh_vertices;
#[cfg(feature = "fit_overlay")]
pub(crate) use solve::Edge;
pub(crate) use solve::FitSolution;
pub(crate) use solve::calculate_fit;

/// Registers camera-fit infrastructure. The core fit solve is pure logic with
/// nothing to register; the optional debug overlay is added when the
/// `fit_overlay` feature is enabled.
pub(crate) struct FitPlugin;

impl Plugin for FitPlugin {
    fn build(&self, app: &mut App) {
        #[cfg(feature = "fit_overlay")]
        app.add_plugins(FitOverlayPlugin);

        #[cfg(not(feature = "fit_overlay"))]
        let _ = app;
    }
}
