//! Camera input API.
//!
//! # Quick Start
//!
//! `OrbitCam` defaults to
//! `OrbitCamInputMode::with_preset(OrbitCamPreset::simple_mouse())`. Insert
//! `OrbitCamInputMode::with_preset(OrbitCamPreset::blender_like())` for editor-style
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
mod axis_response;
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
mod touch;

pub use actions::CameraSemanticAction;
pub use actions::HeldCameraAction;
pub use actions::ImpulseCameraAction;
pub use actions::OrbitCamOrbitAction;
pub use actions::OrbitCamPanAction;
pub use actions::OrbitCamZoomCoarseAction;
pub use actions::OrbitCamZoomSmoothAction;
pub(super) use adapter::OrbitCamInputAdapterPlugin;
pub use axis_response::AxisResponse;
pub use axis_response::Damping;
pub use axis_response::Sensitivity;
use bevy::input::gestures::PinchGesture;
use bevy::input::touch::Touches;
use bevy::prelude::*;
use bevy_enhanced_input::prelude::EnhancedInputPlugin;
pub use bindings::ActionBindingDescriptor;
pub use bindings::ActionBindingEntry;
pub use bindings::ActionBindingSet;
pub use bindings::BindingEngagement;
pub use bindings::BindingGates;
pub use bindings::BindingRoutePolicy;
pub use bindings::CameraInputGamepadSelectionPolicy;
pub use bindings::GamepadInputGain;
pub use bindings::HeldActionBindingEntry;
pub use bindings::InputAxisTransform;
pub(super) use bindings::InputBindingDescriptor;
pub use bindings::InputBindingModifiers;
pub use bindings::InputBindingScale;
pub use bindings::InputDeadZone;
pub use bindings::InputDeltaScale;
pub use bindings::InputGain;
pub use bindings::MouseInputGain;
pub use bindings::OrbitCamBindingGate;
pub use bindings::OrbitCamBindingWithInputGain;
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
pub use bindings::OrbitCamInputGain;
pub use bindings::OrbitCamKeyboardPreset;
pub use bindings::OrbitCamMouseDrag;
pub use bindings::OrbitCamMouseWheelZoom;
pub use bindings::OrbitCamOrbitActionBindings;
pub use bindings::OrbitCamOrbitBinding;
pub use bindings::OrbitCamPanActionBindings;
pub use bindings::OrbitCamPanBinding;
pub use bindings::OrbitCamPinchZoom;
pub use bindings::OrbitCamPreset;
pub use bindings::OrbitCamPresetKind;
pub use bindings::OrbitCamScalePolicy;
pub use bindings::OrbitCamSimpleMouseKeyboardPreset;
pub use bindings::OrbitCamSimpleMousePreset;
pub use bindings::OrbitCamSlowMode;
pub use bindings::OrbitCamTouchBinding;
pub use bindings::OrbitCamTouchBindingConfig;
pub use bindings::OrbitCamTrackpadScroll;
pub use bindings::OrbitCamZoomBinding;
pub use bindings::OrbitCamZoomCoarseActionBindings;
pub use bindings::OrbitCamZoomSmoothActionBindings;
pub use bindings::PinchGestureZoom;
pub use bindings::SmoothScrollInputGain;
pub use bindings::ZoomInversion;
pub(super) use bindings::mod_keys_pressed;
pub use bindings::validate_bindings;
pub use context::FlyCamInputContext;
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
pub use routing::OrbitCamSlowModeState;
pub use routing::ResolvedOrbitCamInputRoute;
pub use sources::CameraInteractionSources;
pub use sources::ManualInputSource;
pub(crate) use touch::TouchGestures;
pub(crate) use touch::TouchTracker;

use crate::system_sets::OrbitCamInputInternalSet;
use crate::system_sets::OrbitCamInputPhase;

/// Registers shared camera input infrastructure used by every camera kind.
///
/// Each camera kind registers its own enhanced-input context in its own
/// plugin; this owns the enhanced-input core, the bevy touch/gesture inputs,
/// and the touch tracker that feeds the input adapter.
pub(super) struct InputPlugin;

impl Plugin for InputPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<EnhancedInputPlugin>() {
            app.add_plugins(EnhancedInputPlugin);
        }
        app.init_resource::<TouchTracker>()
            .init_resource::<Touches>()
            .add_message::<PinchGesture>()
            .add_systems(
                PreUpdate,
                touch::touch_tracker
                    .in_set(OrbitCamInputPhase::PreInput)
                    .before(OrbitCamInputInternalSet::AdapterInjection),
            );
    }
}
