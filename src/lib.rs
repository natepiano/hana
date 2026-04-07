#![warn(missing_docs)]
#![doc = include_str!("../README.md")]

use bevy::camera::CameraUpdateSystems;
use bevy::prelude::*;
use bevy::transform::TransformSystems;
#[cfg(feature = "bevy_egui")]
use bevy_egui::EguiPreUpdateSet;

use crate::input::MouseKeyTracker;
use crate::touch::TouchTracker;

#[allow(
    clippy::used_underscore_binding,
    reason = "false positive on enum variant fields"
)]
mod animation;
mod components;
mod constants;
#[cfg(feature = "bevy_egui")]
mod egui;
mod events;
mod fit;
#[cfg(feature = "fit_overlay")]
mod fit_overlay;
mod input;
mod observers;
mod orbit_cam;
mod support;
mod touch;
mod traits;
#[allow(
    clippy::used_underscore_binding,
    reason = "false positive on enum variant fields"
)]
mod types;
mod util;

pub use animation::CameraMove;
pub use animation::CameraMoveList;
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
pub use orbit_cam::OrbitCam;
pub use orbit_cam::OrbitCamSystemSet;
pub use touch::TouchInput;
pub use types::ActiveCameraData;
pub use types::ButtonZoomAxis;
pub use types::FocusBoundsShape;
pub use types::ForceUpdate;
pub use types::InitializationState;
pub use types::InputControl;
pub use types::TimeSource;
pub use types::TrackpadBehavior;
pub use types::TrackpadInput;
pub use types::UpsideDownPolicy;
pub use types::ZoomDirection;

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
        app.init_resource::<ActiveCameraData>()
            .init_resource::<MouseKeyTracker>()
            .init_resource::<TouchTracker>()
            .add_systems(
                PostUpdate,
                (
                    (
                        orbit_cam::active_viewport_data
                            .run_if(|active_cam: Res<ActiveCameraData>| !active_cam.manual),
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

        app.add_observer(observers::on_camera_move_list_added)
            .add_observer(observers::restore_camera_state)
            .add_observer(observers::on_zoom_to_fit)
            .add_observer(observers::on_play_animation)
            .add_observer(observers::on_set_fit_target)
            .add_observer(observers::on_animate_to_fit)
            .add_observer(observers::on_look_at)
            .add_observer(observers::on_look_at_and_zoom_to_fit)
            .add_systems(Update, animation::process_camera_move_list);

        #[cfg(feature = "fit_overlay")]
        app.add_plugins(fit_overlay::ZoomOverlayPlugin);
    }
}
