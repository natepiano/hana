//! Camera input API.
//!
//! # Quick Start
//!
//! `OrbitCam` defaults to [`OrbitCamPreset::SimpleMouse`]. Insert
//! [`OrbitCamPreset::BlenderLike`] for editor-style navigation, insert
//! [`OrbitCamBindings`] when your app owns a keymap or gamepad mapping, or
//! insert [`OrbitCamManual`] when your app computes camera intent itself.
//!
//! App-authored manual camera input should write through
//! [`OrbitCamManualInputWriter`] in [`OrbitCamInputPhase::WriteManual`].
//! Preset and custom binding input is finalized before the controller runs and
//! emits [`OrbitCamInteractionStarted`], [`OrbitCamInteractionEnded`], and
//! [`OrbitCamInteractionSourcesChanged`] with source attribution.
//!
//! Surface metrics are derived into the resolved input route each frame.
//! An explicit [`CameraInputSurfaceMetrics`] component overrides only the
//! fields it provides, which is useful for render-to-texture and editor-panel
//! cameras whose logical input surface differs from the rendered camera view.
//! Mouse-like and keyboard held interactions keep their owner while held.
//! Gamepad and touch source attribution is reported today; selected-gamepad
//! and touch-owner latching are future routing policy work.
//!
//! [`OrbitCamInputPhase::WriteManual`]: crate::OrbitCamInputPhase::WriteManual

mod actions;
mod adapter;
mod bindings;
mod constants;
mod context;
mod control_summary;
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
pub(super) use adapter::OrbitCamInputAdapterPlugin;
pub use bindings::ActionBindingDescriptor;
pub use bindings::ActionBindingEntry;
pub use bindings::ActionBindingSet;
pub use bindings::BindingEngagement;
pub use bindings::BindingRoutePolicy;
pub use bindings::CameraInputGamepadSelectionPolicy;
pub use bindings::HeldActionBindingEntry;
pub(super) use bindings::InputBindingDescriptor;
pub(super) use bindings::InputBindingTransform;
pub use bindings::OrbitCamBindings;
pub use bindings::OrbitCamBindingsBuilder;
pub use bindings::OrbitCamBindingsDescriptor;
pub use bindings::OrbitCamBindingsError;
pub use bindings::OrbitCamButtonDragZoom;
pub use bindings::OrbitCamButtonDragZoomAxis;
pub use bindings::OrbitCamHeldBinding;
pub use bindings::OrbitCamInputBinding;
pub use bindings::OrbitCamMouseDrag;
pub use bindings::OrbitCamMouseWheelZoom;
pub use bindings::OrbitCamOrbitActionBindings;
pub use bindings::OrbitCamOrbitBinding;
pub use bindings::OrbitCamPanActionBindings;
pub use bindings::OrbitCamPanBinding;
pub use bindings::OrbitCamPinchZoom;
pub use bindings::OrbitCamPreset;
pub use bindings::OrbitCamTouchBinding;
pub use bindings::OrbitCamTrackpadScroll;
pub use bindings::OrbitCamZoomBinding;
pub use bindings::OrbitCamZoomCoarseActionBindings;
pub use bindings::OrbitCamZoomSmoothActionBindings;
pub use bindings::PinchGestureZoom;
pub use bindings::WheelZoomPolarity;
pub use bindings::ZoomDirection;
pub(super) use bindings::mod_keys_pressed;
pub use bindings::validate_bindings;
pub use context::OrbitCamInputContext;
pub use control_summary::OrbitCamControlRow;
pub use control_summary::OrbitCamControlSummary;
pub use control_summary::describe_orbit_cam_controls;
pub use disabled::CameraInputDisabled;
pub use events::CameraInputMetricsMissing;
pub use events::OrbitCamInteractionEnded;
pub use events::OrbitCamInteractionKind;
pub use events::OrbitCamInteractionSourcesChanged;
pub use events::OrbitCamInteractionStarted;
pub use intent::CameraMotion;
pub use intent::CoarseZoomDelta;
pub use intent::OrbitCamInput;
pub use intent::OrbitDelta;
pub use intent::PanDelta;
pub use intent::SmoothZoomDelta;
pub(super) use lifecycle::OrbitCamInputLifecyclePlugin;
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
pub(super) use modes::OrbitCamInputModeReplaced;
pub(super) use modes::OrbitCamInputModesPlugin;
pub use modes::OrbitCamManual;
pub use routing::CameraInputRouting;
pub use routing::CameraInputRoutingConfig;
pub(super) use routing::CameraInputSourceLatches;
pub use routing::NoPositionFallback;
pub(super) use routing::OrbitCamInputBlockers;
pub(super) use routing::OrbitCamInputContextGated;
pub(super) use routing::OrbitCamRoutingPlugin;
pub use routing::ResolvedOrbitCamInputRoute;
pub use sources::CameraInteractionSources;
pub use sources::ManualInputSource;
pub use state::OrbitCamInteractionState;
