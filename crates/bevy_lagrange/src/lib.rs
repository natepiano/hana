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
pub use input::ActionBindingDescriptor;
pub use input::ActionBindingEntry;
pub use input::ActionBindingSet;
pub use input::BindingEngagement;
pub use input::BindingRoutePolicy;
pub use input::CameraInputDisabled;
pub use input::CameraInputGamepadSelectionPolicy;
pub use input::CameraInputMetricKind;
pub use input::CameraInputMetricsMissing;
pub use input::CameraInputRouting;
pub use input::CameraInputRoutingConfig;
pub use input::CameraInputSurfaceMetrics;
pub use input::CameraInteractionSources;
pub use input::CameraSemanticAction;
pub use input::CoarseZoomDelta;
pub use input::HeldActionBindingEntry;
pub use input::HeldCameraAction;
pub use input::ImpulseCameraAction;
pub use input::ManualInputSource;
pub use input::NoPositionFallback;
pub use input::OrbitCamBindings;
pub use input::OrbitCamBindingsBuilder;
pub use input::OrbitCamBindingsDescriptor;
pub use input::OrbitCamBindingsError;
pub use input::OrbitCamButtonDragZoom;
pub use input::OrbitCamButtonDragZoomAxis;
pub use input::OrbitCamControlRow;
pub use input::OrbitCamControlSummary;
pub use input::OrbitCamHeldBinding;
pub use input::OrbitCamInput;
use input::OrbitCamInputAdapterPlugin;
pub use input::OrbitCamInputBinding;
pub use input::OrbitCamInputContext;
use input::OrbitCamInputLifecyclePlugin;
#[cfg(feature = "reflect-input-modes")]
pub use input::OrbitCamInputMode;
#[cfg(feature = "reflect-input-modes")]
pub use input::OrbitCamInputModeApplied;
#[cfg(feature = "reflect-input-modes")]
pub use input::OrbitCamInputModeApplyState;
#[cfg(feature = "reflect-input-modes")]
pub use input::OrbitCamInputModeApplyStatus;
#[cfg(feature = "reflect-input-modes")]
pub use input::OrbitCamInputModeDescriptor;
#[cfg(feature = "reflect-input-modes")]
pub use input::OrbitCamInputModeRejected;
use input::OrbitCamInputModesPlugin;
pub use input::OrbitCamInteractionEnded;
pub use input::OrbitCamInteractionKind;
pub use input::OrbitCamInteractionSourcesChanged;
pub use input::OrbitCamInteractionStarted;
pub use input::OrbitCamInteractionState;
pub use input::OrbitCamManual;
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
use input::OrbitCamRoutingPlugin;
pub use input::OrbitCamTouchBinding;
pub use input::OrbitCamTrackpadScroll;
pub use input::OrbitCamZoomBinding;
pub use input::OrbitCamZoomCoarseAction;
pub use input::OrbitCamZoomCoarseActionBindings;
pub use input::OrbitCamZoomSmoothAction;
pub use input::OrbitCamZoomSmoothActionBindings;
pub use input::OrbitDelta;
pub use input::PanDelta;
pub use input::SmoothZoomDelta;
pub use input::ZoomDirection;
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

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::time::Duration;

    use bevy::camera::RenderTarget;
    use bevy::input::mouse::AccumulatedMouseMotion;
    use bevy::input::mouse::AccumulatedMouseScroll;
    use bevy::prelude::*;
    use bevy::window::WindowRef;

    use super::*;

    const INPUT_SURFACE_SIZE: Vec2 = Vec2::new(100.0, 100.0);
    const MANUAL_ORBIT_DELTA: Vec2 = Vec2::new(25.0, 0.0);
    const MOVE_DURATION_MILLIS: u64 = 1_000;
    const ANIMATION_YAW: f32 = 1.0;
    const ANIMATION_RADIUS: f32 = 2.0;

    #[derive(Component)]
    struct ScheduleInvariantCamera;

    #[derive(Resource, Default)]
    struct AnimationEventCounts {
        cancelled: usize,
    }

    type TestResult = Result<(), &'static str>;

    fn test_app() -> App {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, LagrangePlugin))
            .init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<ButtonInput<MouseButton>>()
            .init_resource::<AccumulatedMouseMotion>()
            .init_resource::<AccumulatedMouseScroll>()
            .init_resource::<AnimationEventCounts>()
            .add_systems(
                PreUpdate,
                write_manual_orbit_input.in_set(OrbitCamInputPhase::WriteManual),
            );
        app.finish();
        app
    }

    fn write_manual_orbit_input(
        mut writer: OrbitCamManualInputWriter,
        cameras: Query<Entity, With<ScheduleInvariantCamera>>,
    ) {
        let Ok(camera) = cameras.single() else {
            return;
        };
        let Ok(mut input) = writer.get_mut(camera, ManualInputSource::observed_keyboard()) else {
            return;
        };
        input.orbit_pixels(MANUAL_ORBIT_DELTA);
    }

    fn observe_animation_cancelled(world: &mut World, camera: Entity) {
        world.entity_mut(camera).observe(
            |_event: On<AnimationCancelled>, mut counts: ResMut<AnimationEventCounts>| {
                counts.cancelled += 1;
            },
        );
    }

    fn animation_move() -> CameraMove {
        CameraMove::ToOrbit {
            focus:    Vec3::ZERO,
            yaw:      ANIMATION_YAW,
            pitch:    0.0,
            radius:   ANIMATION_RADIUS,
            duration: Duration::from_millis(MOVE_DURATION_MILLIS),
            easing:   EaseFunction::Linear,
        }
    }

    fn spawn_manual_camera(app: &mut App) -> Entity {
        let camera = app
            .world_mut()
            .spawn((
                ScheduleInvariantCamera,
                OrbitCam {
                    orbit_smoothness: 0.0,
                    ..default()
                },
                OrbitCamManual,
                Camera::default(),
                RenderTarget::Window(WindowRef::Primary),
                Transform::from_xyz(0.0, 0.0, 10.0),
                CameraInputSurfaceMetrics::camera_view_and_input_surface(
                    INPUT_SURFACE_SIZE,
                    INPUT_SURFACE_SIZE,
                ),
                CameraMoveList::new(VecDeque::from([animation_move()])),
                CameraInputInterruptBehavior::Cancel,
            ))
            .id();
        observe_animation_cancelled(app.world_mut(), camera);
        camera
    }

    #[test]
    fn enhanced_input_scheduling_invariant() -> TestResult {
        let mut app = test_app();
        let camera = spawn_manual_camera(&mut app);
        app.insert_resource(CameraInputRoutingConfig::explicit(camera));

        app.update();

        assert_eq!(app.world().resource::<AnimationEventCounts>().cancelled, 1);
        assert!(app.world().get::<CameraMoveList>(camera).is_none());
        let orbit_cam = app
            .world()
            .get::<OrbitCam>(camera)
            .ok_or("camera missing OrbitCam")?;
        assert!(orbit_cam.target_yaw < -1.0);
        Ok(())
    }
}
