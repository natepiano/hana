#![warn(missing_docs)]
#![doc = include_str!("../README.md")]

mod animation;
mod components;
mod constants;
#[cfg(feature = "bevy_egui")]
mod egui;
mod enhanced_input;
mod events;
mod fit;
#[cfg(feature = "fit_overlay")]
mod fit_overlay;
pub mod input;
mod observers;
mod orbit_cam;
mod orbital_math;
mod projection;
mod system_sets;
mod touch;

pub use animation::CameraMove;
pub use animation::CameraMoveList;
use bevy::camera::CameraUpdateSystems;
use bevy::prelude::*;
use bevy::transform::TransformSystems;
#[cfg(feature = "bevy_egui")]
use bevy_egui::EguiPreUpdateSet;
pub use components::AnimationConflictPolicy;
pub use components::CameraInputInterruptBehavior;
pub use components::CurrentFitTarget;
#[cfg(feature = "fit_overlay")]
pub use components::FitOverlay;
#[cfg(feature = "bevy_egui")]
pub use egui::BlockOnEguiFocus;
#[cfg(feature = "bevy_egui")]
pub use egui::EguiFocusIncludesHover;
#[cfg(feature = "bevy_egui")]
pub use egui::EguiWantsFocus;
use enhanced_input::LagrangeEnhancedInputPlugin;
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
#[cfg(feature = "fit_overlay")]
pub use fit_overlay::FitTargetOverlayConfig;
#[cfg(feature = "fit_overlay")]
use fit_overlay::ZoomOverlayPlugin;
pub use input::ButtonZoomAxis;
pub use input::CameraInputDisabled;
pub use input::CameraInputMetricKind;
pub use input::CameraInputMetricsMissing;
pub use input::CameraInputSurfaceMetrics;
pub use input::CameraInteractionSources;
pub use input::CoarseZoomDelta;
pub use input::InputControl;
#[cfg(feature = "reflect-input-modes")]
use input::LagrangeInputTypesPlugin;
pub use input::ManualInputSource;
use input::MouseKeyTracker;
pub use input::OrbitCamInput;
pub use input::OrbitCamInputContext;
pub use input::OrbitCamInteractionEnded;
pub use input::OrbitCamInteractionKind;
pub use input::OrbitCamInteractionSourcesChanged;
pub use input::OrbitCamInteractionStarted;
pub use input::OrbitCamInteractionState;
pub use input::OrbitCamManualInput;
pub use input::OrbitCamManualInputWriter;
pub use input::OrbitDelta;
pub use input::PanDelta;
pub use input::SmoothZoomDelta;
pub use input::TrackpadBehavior;
pub use input::TrackpadInput;
pub use input::ZoomDirection;
use observers::ObserverPlugin;
pub use orbit_cam::ActiveCameraData;
pub use orbit_cam::CameraInputDetection;
pub use orbit_cam::FocusBoundsShape;
pub use orbit_cam::ForceUpdate;
pub use orbit_cam::InitializationState;
pub use orbit_cam::OrbitCam;
pub use orbit_cam::OrbitCamSystemSet;
pub use orbit_cam::TimeSource;
pub use orbit_cam::UpsideDownPolicy;
use system_sets::LagrangeSystemSetsPlugin;
pub use system_sets::OrbitCamInputPhase;
pub use touch::TouchInput;
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
        app.add_plugins((LagrangeEnhancedInputPlugin, LagrangeSystemSetsPlugin));

        #[cfg(feature = "reflect-input-modes")]
        app.add_plugins(LagrangeInputTypesPlugin);

        app.init_resource::<ActiveCameraData>()
            .init_resource::<MouseKeyTracker>()
            .init_resource::<TouchTracker>()
            .add_systems(
                PostUpdate,
                (
                    (
                        orbit_cam::active_viewport_data.run_if(
                            |active_camera: Res<ActiveCameraData>| {
                                active_camera.detection == CameraInputDetection::Automatic
                            },
                        ),
                        input::mouse_key_tracker,
                        touch::touch_tracker,
                    ),
                    orbit_cam::orbit_cam,
                )
                    .chain()
                    .in_set(OrbitCamSystemSet)
                    .before(TransformSystems::Propagate)
                    .before(CameraUpdateSystems),
            );

        #[cfg(feature = "bevy_egui")]
        {
            app.init_resource::<EguiWantsFocus>()
                .init_resource::<EguiFocusIncludesHover>()
                .add_systems(
                    PostUpdate,
                    egui::check_egui_wants_focus
                        .after(EguiPreUpdateSet::InitContexts)
                        .before(OrbitCamSystemSet),
                );
        }

        app.add_plugins(ObserverPlugin)
            .add_systems(Update, animation::process_camera_move_list);

        #[cfg(feature = "fit_overlay")]
        app.add_plugins(ZoomOverlayPlugin);
    }
}
