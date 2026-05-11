//! Camera input API.
//!
//! # Quick Start
//!
//! `OrbitCam` still uses the legacy raw-input fields until the input cutover phase.
//! The types in this module define the additive semantic input surface that later
//! phases connect to enhanced input.
//!
//! App-authored manual camera input should write [`OrbitCamInput`] through
//! [`OrbitCamManualInputWriter`] in [`OrbitCamInputPhase::WriteManual`].
//!
//! [`OrbitCamInputPhase::WriteManual`]: crate::OrbitCamInputPhase::WriteManual

mod context;
mod disabled;
mod events;
mod intent;
mod legacy;
mod manual;
mod metrics;
mod sources;
mod state;

#[cfg(feature = "reflect-input-modes")]
use bevy::prelude::*;
pub use context::OrbitCamInputContext;
pub use disabled::CameraInputDisabled;
pub use events::CameraInputMetricsMissing;
pub use events::OrbitCamInteractionEnded;
pub use events::OrbitCamInteractionKind;
pub use events::OrbitCamInteractionSourcesChanged;
pub use events::OrbitCamInteractionStarted;
pub use intent::CoarseZoomDelta;
pub use intent::OrbitCamInput;
pub use intent::OrbitDelta;
pub use intent::PanDelta;
pub use intent::SmoothZoomDelta;
pub use legacy::ButtonZoomAxis;
pub use legacy::InputControl;
pub(crate) use legacy::MouseKeyTracker;
pub(crate) use legacy::OrbitButtonChange;
pub use legacy::TrackpadBehavior;
pub use legacy::TrackpadInput;
pub use legacy::ZoomDirection;
pub(crate) use legacy::button_zoom_just_pressed;
pub(crate) use legacy::mouse_key_tracker;
pub(crate) use legacy::orbit_just_pressed;
pub(crate) use legacy::pan_just_pressed;
pub use manual::OrbitCamManualInput;
pub use manual::OrbitCamManualInputWriter;
pub use metrics::CameraInputMetricKind;
pub use metrics::CameraInputSurfaceMetrics;
pub use sources::CameraInteractionSources;
pub use sources::ManualInputSource;
pub use state::OrbitCamInteractionState;

#[cfg(feature = "reflect-input-modes")]
pub(crate) struct LagrangeInputTypesPlugin;

#[cfg(feature = "reflect-input-modes")]
impl Plugin for LagrangeInputTypesPlugin {
    fn build(&self, app: &mut App) { register_input_reflection(app); }
}

#[cfg(feature = "reflect-input-modes")]
fn register_input_reflection(app: &mut App) {
    app.register_type::<CameraInputDisabled>()
        .register_type::<CameraInputMetricKind>()
        .register_type::<CameraInputMetricsMissing>()
        .register_type::<CameraInputSurfaceMetrics>()
        .register_type::<CameraInteractionSources>()
        .register_type::<CoarseZoomDelta>()
        .register_type::<OrbitCamInput>()
        .register_type::<OrbitCamInputContext>()
        .register_type::<OrbitCamInteractionEnded>()
        .register_type::<OrbitCamInteractionKind>()
        .register_type::<OrbitCamInteractionSourcesChanged>()
        .register_type::<OrbitCamInteractionStarted>()
        .register_type::<OrbitCamInteractionState>()
        .register_type::<OrbitDelta>()
        .register_type::<PanDelta>()
        .register_type::<SmoothZoomDelta>();
}
