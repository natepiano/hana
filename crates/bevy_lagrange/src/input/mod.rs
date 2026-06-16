//! Camera input API.
//!
//! # Quick Start
//!
//! `OrbitCam` defaults to
//! [`OrbitCamInputMode::Preset(OrbitCamPreset::SimpleMouse)`]. Insert
//! [`OrbitCamInputMode::Preset(OrbitCamPreset::BlenderLike)`] for editor-style
//! navigation, [`OrbitCamInputMode::Bindings`] when your app owns a keymap or
//! gamepad mapping, or [`OrbitCamInputMode::Manual`] when your app computes
//! camera intent itself.
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
mod interaction_state;
mod lifecycle;
mod manual;
mod metrics;
mod modes;
mod routing;
mod sources;

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
pub use bindings::BindingGates;
pub use bindings::BindingRoutePolicy;
pub use bindings::CameraInputGamepadSelectionPolicy;
pub use bindings::HeldActionBindingEntry;
pub use bindings::InputAxisTransform;
pub(super) use bindings::InputBindingDescriptor;
pub use bindings::InputBindingModifiers;
pub use bindings::InputBindingScale;
pub use bindings::InputDeadZone;
pub use bindings::InputDeltaScale;
pub use bindings::OrbitCamBindingGate;
pub use bindings::OrbitCamBindings;
pub use bindings::OrbitCamBindingsBuilder;
pub use bindings::OrbitCamBindingsDescriptor;
pub use bindings::OrbitCamBindingsError;
pub use bindings::OrbitCamBlenderLikeKeyboardPreset;
pub use bindings::OrbitCamBlenderLikePreset;
pub use bindings::OrbitCamButtonDragZoom;
pub use bindings::OrbitCamButtonDragZoomAxis;
pub use bindings::OrbitCamGamepadPreset;
pub use bindings::OrbitCamGamepadPresetBuilder;
pub use bindings::OrbitCamGateInput;
pub use bindings::OrbitCamGatePolarity;
pub use bindings::OrbitCamHeldBinding;
pub use bindings::OrbitCamInputBinding;
pub use bindings::OrbitCamKeyboardPreset;
pub use bindings::OrbitCamMouseDrag;
pub use bindings::OrbitCamMouseWheelZoom;
pub use bindings::OrbitCamOrbitActionBindings;
pub use bindings::OrbitCamOrbitBinding;
pub use bindings::OrbitCamPanActionBindings;
pub use bindings::OrbitCamPanBinding;
pub use bindings::OrbitCamPinchZoom;
pub use bindings::OrbitCamPreset;
pub use bindings::OrbitCamScalePolicy;
pub use bindings::OrbitCamSimpleMouseKeyboardPreset;
pub use bindings::OrbitCamSimpleMousePreset;
pub use bindings::OrbitCamSlowMode;
pub use bindings::OrbitCamTouchBinding;
pub use bindings::OrbitCamTrackpadScroll;
pub use bindings::OrbitCamZoomBinding;
pub use bindings::OrbitCamZoomCoarseActionBindings;
pub use bindings::OrbitCamZoomSmoothActionBindings;
pub use bindings::PinchGestureZoom;
pub use bindings::ZoomInversion;
pub(super) use bindings::mod_keys_pressed;
pub use bindings::validate_bindings;
pub use context::OrbitCamInputContext;
pub use control_summary::ControlSpeed;
pub use control_summary::OrbitCamControlRow;
pub use control_summary::OrbitCamControlSummary;
pub use control_summary::ZoomDirection;
pub use control_summary::describe_orbit_cam_controls;
pub use disabled::CameraInputDisabled;
pub use events::CameraInputMetricsMissing;
pub use events::OrbitCamInteractionEnded;
pub use events::OrbitCamInteractionKind;
pub use events::OrbitCamInteractionSourcesChanged;
pub use events::OrbitCamInteractionSpeedChanged;
pub use events::OrbitCamInteractionStarted;
pub use intent::CameraMotion;
pub use intent::CoarseZoomDelta;
pub use intent::OrbitCamInput;
pub use intent::OrbitDelta;
pub use intent::PanDelta;
pub use intent::SmoothZoomDelta;
pub use interaction_state::OrbitCamInteractionState;
pub(super) use lifecycle::OrbitCamInputLifecyclePlugin;
pub use lifecycle::OrbitCamReportingDebounce;
pub use manual::OrbitCamManualInput;
pub use manual::OrbitCamManualInputWriter;
pub use metrics::CameraInputMetricKind;
pub use metrics::CameraInputSurfaceMetrics;
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
pub use modes::OrbitCamInputModeDraft;
#[cfg(feature = "reflect-input-modes")]
pub use modes::OrbitCamInputModeRejected;
pub(super) use modes::OrbitCamInputModeReplaced;
pub(super) use modes::OrbitCamInputModesPlugin;
pub(crate) use modes::OrbitCamManual;
pub(crate) use modes::OrbitCamResolvedBindings;
pub use routing::CameraInputRouting;
pub use routing::CameraInputRoutingConfig;
pub(super) use routing::CameraInputSourceLatches;
pub use routing::NoPositionFallback;
pub(super) use routing::OrbitCamInputBlockers;
pub(super) use routing::OrbitCamInputContextGated;
pub(super) use routing::OrbitCamRoutingPlugin;
pub(crate) use routing::OrbitCamSlowModeLatches;
pub use routing::ResolvedOrbitCamInputRoute;
pub use sources::CameraInteractionSources;
pub use sources::ManualInputSource;
