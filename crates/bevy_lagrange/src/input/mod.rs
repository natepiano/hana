//! Camera input API.
//!
//! # Quick Start
//!
//! App-authored manual camera input should write [`OrbitCamInput`] through
//! [`OrbitCamManualInputWriter`] in [`OrbitCamInputPhase::WriteManual`].
//!
//! [`OrbitCamInputPhase::WriteManual`]: crate::OrbitCamInputPhase::WriteManual

mod actions;
mod adapter;
mod bindings;
mod context;
mod disabled;
mod events;
mod intent;
mod lifecycle;
mod manual;
mod metrics;
mod modes;
mod routing;
mod sources;
mod state;

pub use actions::CameraSemanticAction;
pub use actions::HeldCameraAction;
pub use actions::ImpulseCameraAction;
pub use actions::OrbitCamOrbitAction;
pub use actions::OrbitCamPanAction;
pub use actions::OrbitCamZoomCoarseAction;
pub use actions::OrbitCamZoomSmoothAction;
pub(crate) use adapter::OrbitCamInputAdapterPlugin;
pub use bindings::ActionBindingDescriptor;
pub use bindings::ActionBindingEntry;
pub use bindings::ActionBindingSet;
pub use bindings::BindingEngagement;
pub use bindings::BindingRecipe;
pub use bindings::BindingRoutePolicy;
pub use bindings::CameraInputGamepadSelectionPolicy;
pub use bindings::HeldActionBindingEntry;
pub use bindings::OrbitCamBindings;
pub use bindings::OrbitCamBindingsBuilder;
pub use bindings::OrbitCamBindingsDescriptor;
pub use bindings::OrbitCamBindingsError;
pub use bindings::OrbitCamBindingsWheelSet;
pub use bindings::OrbitCamBindingsWheelUnset;
pub use bindings::OrbitCamBlenderLikeWheelBinding;
pub use bindings::OrbitCamButtonDragZoomAxis;
pub use bindings::OrbitCamButtonDragZoomBinding;
pub use bindings::OrbitCamOrbitActionBindings;
pub use bindings::OrbitCamPanActionBindings;
pub use bindings::OrbitCamPinchBinding;
pub use bindings::OrbitCamPreset;
pub use bindings::OrbitCamTouchBinding;
pub use bindings::OrbitCamWheelBinding;
pub use bindings::OrbitCamWheelModifier;
pub use bindings::OrbitCamZoomCoarseActionBindings;
pub use bindings::OrbitCamZoomSmoothActionBindings;
pub use bindings::ZoomDirection;
pub use bindings::validate_bindings;
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
pub(crate) use lifecycle::OrbitCamInputLifecyclePlugin;
pub use manual::OrbitCamManualInput;
pub use manual::OrbitCamManualInputWriter;
pub use metrics::CameraInputMetricKind;
pub use metrics::CameraInputSurfaceMetrics;
#[cfg(feature = "reflect-input-modes")]
pub use modes::OrbitCamInputMode;
#[cfg(feature = "reflect-input-modes")]
pub use modes::OrbitCamInputModeApplied;
#[cfg(feature = "reflect-input-modes")]
pub use modes::OrbitCamInputModeApplyState;
#[cfg(feature = "reflect-input-modes")]
pub use modes::OrbitCamInputModeApplyStatus;
#[cfg(feature = "reflect-input-modes")]
pub use modes::OrbitCamInputModeDescriptor;
#[cfg(feature = "reflect-input-modes")]
pub use modes::OrbitCamInputModeRejected;
pub(crate) use modes::OrbitCamInputModeReplaced;
pub(crate) use modes::OrbitCamInputModesPlugin;
pub use modes::OrbitCamManual;
pub use routing::CameraInputRouting;
pub use routing::CameraInputRoutingConfig;
pub(crate) use routing::CameraInputSourceLatches;
pub use routing::NoPositionFallback;
pub(crate) use routing::OrbitCamInputBlockers;
pub(crate) use routing::OrbitCamInputContextGated;
pub(crate) use routing::OrbitCamRoutingPlugin;
pub(crate) use routing::ResolvedOrbitCamInputRoute;
pub use sources::CameraInteractionSources;
pub use sources::ManualInputSource;
pub use state::OrbitCamInteractionState;
