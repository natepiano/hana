#![warn(missing_docs)]
#![doc = include_str!("../README.md")]

mod animation;
mod components;
mod constants;
mod enhanced_input;
mod events;
mod fit;
#[cfg(feature = "fit_overlay")]
mod fit_overlay;
mod input;
mod observers;
mod orbit_cam;
mod orbital_math;
mod projection;
mod system_sets;
mod touch;

pub use animation::CameraMove;
pub use animation::CameraMoveList;
use bevy::camera::CameraUpdateSystems;
use bevy::input::gestures::PinchGesture;
use bevy::input::touch::Touches;
use bevy::prelude::*;
use bevy::transform::TransformSystems;
pub use components::AnimationConflictPolicy;
pub use components::CameraInputInterruptBehavior;
pub use components::CurrentFitTarget;
#[cfg(feature = "fit_overlay")]
pub use components::FitOverlay;
use enhanced_input::LagrangeEnhancedInputPlugin;
pub use events::AnimateToFit;
pub use events::AnimationBegin;
pub use events::AnimationEnd;
pub use events::AnimationReason;
pub use events::AnimationRejected;
pub use events::AnimationSource;
pub use events::CameraMoveBegin;
pub use events::CameraMoveEnd;
pub use events::FitAnchor;
pub use events::LookAt;
pub use events::LookAtAndZoomToFit;
pub use events::PlayAnimation;
pub use events::SetFitTarget;
pub use events::ZoomBegin;
pub use events::ZoomContext;
pub use events::ZoomEnd;
pub use events::ZoomReason;
pub use events::ZoomToFit;
#[cfg(feature = "fit_overlay")]
pub use fit_overlay::FitTargetOverlayConfig;
#[cfg(feature = "fit_overlay")]
use fit_overlay::ZoomOverlayPlugin;
pub use input::ActionBindingDescriptor;
pub use input::ActionBindingEntry;
pub use input::ActionBindingSet;
pub use input::AxisResponse;
pub use input::BindingEngagement;
pub use input::BindingGates;
pub use input::BindingRoutePolicy;
pub use input::CameraInputDisabled;
pub use input::CameraInputGamepadSelectionPolicy;
pub use input::CameraInputMetricKind;
pub use input::CameraInputMetricsMissing;
pub use input::CameraInputRouting;
pub use input::CameraInputRoutingConfig;
pub use input::CameraInputSurfaceMetrics;
pub use input::CameraInteractionSources;
pub use input::CameraMotion;
pub use input::CameraSemanticAction;
pub use input::CoarseZoomDelta;
pub use input::ControlSpeed;
pub use input::Damping;
pub use input::GamepadSensitivity;
pub use input::HeldActionBindingEntry;
pub use input::HeldCameraAction;
pub use input::ImpulseCameraAction;
pub use input::InputAxisTransform;
pub use input::InputBindingModifiers;
pub use input::InputBindingScale;
pub use input::InputDeadZone;
pub use input::InputDeltaScale;
pub use input::InputGain;
pub use input::ManualInputSource;
pub use input::MouseSensitivity;
pub use input::NoPositionFallback;
pub use input::OrbitCamBindingGate;
pub use input::OrbitCamBindingWithInputGain;
pub use input::OrbitCamBindings;
pub use input::OrbitCamBindingsBuilder;
pub use input::OrbitCamBindingsDescriptor;
pub use input::OrbitCamBindingsError;
pub use input::OrbitCamBlenderLikeKeyboardPreset;
pub use input::OrbitCamBlenderLikePreset;
pub use input::OrbitCamButtonDragZoom;
pub use input::OrbitCamButtonDragZoomAxis;
pub use input::OrbitCamControlRow;
pub use input::OrbitCamControlSummary;
pub use input::OrbitCamGamepadPreset;
pub use input::OrbitCamGamepadPresetBuilder;
pub use input::OrbitCamGateInput;
pub use input::OrbitCamGatePolarity;
pub use input::OrbitCamHeldBinding;
pub use input::OrbitCamInput;
use input::OrbitCamInputAdapterPlugin;
pub use input::OrbitCamInputBinding;
pub use input::OrbitCamInputContext;
use input::OrbitCamInputLifecyclePlugin;
pub use input::OrbitCamInputMode;
use input::OrbitCamInputModesPlugin;
pub use input::OrbitCamInteractionEnded;
pub use input::OrbitCamInteractionKind;
pub use input::OrbitCamInteractionSourcesChanged;
pub use input::OrbitCamInteractionSpeedChanged;
pub use input::OrbitCamInteractionStarted;
pub use input::OrbitCamInteractionState;
pub use input::OrbitCamKeyboardPreset;
pub use input::OrbitCamManualInput;
pub use input::OrbitCamManualInputWriter;
pub use input::OrbitCamMouseDrag;
pub use input::OrbitCamMouseWheelZoom;
pub use input::OrbitCamOrbitAction;
pub use input::OrbitCamOrbitActionBindings;
pub use input::OrbitCamOrbitBinding;
pub use input::OrbitCamPanAction;
pub use input::OrbitCamPanActionBindings;
pub use input::OrbitCamPanBinding;
pub use input::OrbitCamPinchZoom;
pub use input::OrbitCamPreset;
pub use input::OrbitCamPresetKind;
pub use input::OrbitCamReportingDebounce;
use input::OrbitCamRoutingPlugin;
pub use input::OrbitCamScalePolicy;
pub use input::OrbitCamSensitivity;
pub use input::OrbitCamSimpleMouseKeyboardPreset;
pub use input::OrbitCamSimpleMousePreset;
pub use input::OrbitCamSlowMode;
pub use input::OrbitCamSlowModeState;
pub use input::OrbitCamTouchBinding;
pub use input::OrbitCamTouchBindingConfig;
pub use input::OrbitCamTrackpadScroll;
pub use input::OrbitCamZoomBinding;
pub use input::OrbitCamZoomCoarseAction;
pub use input::OrbitCamZoomCoarseActionBindings;
pub use input::OrbitCamZoomSmoothAction;
pub use input::OrbitCamZoomSmoothActionBindings;
pub use input::OrbitDelta;
pub use input::PanDelta;
pub use input::PinchGestureZoom;
pub use input::ResolvedOrbitCamInputRoute;
pub use input::Sensitivity;
pub use input::SmoothScrollSensitivity;
pub use input::SmoothZoomDelta;
pub use input::ZoomDirection;
pub use input::ZoomInversion;
pub use input::describe_orbit_cam_controls;
pub use input::validate_bindings;
use observers::ObserverPlugin;
pub use orbit_cam::FocusBoundsShape;
pub use orbit_cam::InitializationState;
pub use orbit_cam::OrbitCam;
pub use orbit_cam::OrbitCamSystemSet;
#[doc(hidden)]
pub use orbit_cam::OrbitCamUpdateRequest;
pub use orbit_cam::TimeSource;
pub use orbit_cam::UpsideDownPolicy;
use system_sets::LagrangeSystemSetsPlugin;
use system_sets::OrbitCamInputInternalSet;
pub use system_sets::OrbitCamInputPhase;
use touch::TouchTracker;

/// Bevy plugin that contains the systems for controlling `OrbitCam` components.
/// # Example
/// ```no_run
/// # use bevy::prelude::*;
/// # use bevy_lagrange::{LagrangePlugin, OrbitCam};
/// fn main() {
///     App::new()
///         .add_plugins(DefaultPlugins)
///         .add_plugins(LagrangePlugin)
///         .run();
/// }
/// ```
pub struct LagrangePlugin;

impl Plugin for LagrangePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            LagrangeEnhancedInputPlugin,
            LagrangeSystemSetsPlugin,
            OrbitCamInputModesPlugin,
            OrbitCamRoutingPlugin,
            OrbitCamInputAdapterPlugin,
            OrbitCamInputLifecyclePlugin,
        ));

        app.init_resource::<TouchTracker>()
            .init_resource::<Touches>()
            .add_message::<PinchGesture>()
            .add_systems(
                PreUpdate,
                touch::touch_tracker
                    .in_set(OrbitCamInputPhase::PreInput)
                    .before(OrbitCamInputInternalSet::AdapterInjection),
            )
            .add_systems(
                PostUpdate,
                orbit_cam::orbit_cam
                    .in_set(OrbitCamSystemSet)
                    .before(TransformSystems::Propagate)
                    .before(CameraUpdateSystems),
            );

        app.add_plugins(ObserverPlugin)
            .add_systems(Update, animation::process_camera_move_list);

        #[cfg(feature = "fit_overlay")]
        app.add_plugins(ZoomOverlayPlugin);
    }
}
