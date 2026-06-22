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
pub use input::GamepadSensitivity;
pub use input::HeldActionBindingEntry;
pub use input::HeldCameraAction;
pub use input::ImpulseCameraAction;
pub use input::InputAxisTransform;
pub use input::InputBindingModifiers;
pub use input::InputBindingScale;
pub use input::InputDeadZone;
pub use input::InputDeltaScale;
pub use input::InputSensitivity;
pub use input::ManualInputSource;
pub use input::MouseSensitivity;
pub use input::NoPositionFallback;
pub use input::OrbitCamBindingGate;
pub use input::OrbitCamBindingWithSensitivity;
pub use input::OrbitCamBindings;
pub use input::OrbitCamBindingsBuilder;
pub use input::OrbitCamBindingsDescriptor;
pub use input::OrbitCamBindingsError;
pub use input::OrbitCamBlenderLikeKeyboardPreset;
#[cfg(feature = "reflect-input-modes")]
pub use input::OrbitCamBlenderLikeKeyboardPresetDraft;
pub use input::OrbitCamBlenderLikePreset;
#[cfg(feature = "reflect-input-modes")]
pub use input::OrbitCamBlenderLikePresetDraft;
pub use input::OrbitCamButtonDragZoom;
pub use input::OrbitCamButtonDragZoomAxis;
pub use input::OrbitCamControlRow;
pub use input::OrbitCamControlSummary;
pub use input::OrbitCamGamepadPreset;
pub use input::OrbitCamGamepadPresetBuilder;
#[cfg(feature = "reflect-input-modes")]
pub use input::OrbitCamGamepadPresetDraft;
pub use input::OrbitCamGateInput;
pub use input::OrbitCamGatePolarity;
pub use input::OrbitCamHeldBinding;
pub use input::OrbitCamInput;
use input::OrbitCamInputAdapterPlugin;
pub use input::OrbitCamInputBinding;
pub use input::OrbitCamInputContext;
use input::OrbitCamInputLifecyclePlugin;
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
pub use input::OrbitCamInputModeDraft;
#[cfg(feature = "reflect-input-modes")]
pub use input::OrbitCamInputModeRejected;
use input::OrbitCamInputModesPlugin;
pub use input::OrbitCamInteractionEnded;
pub use input::OrbitCamInteractionKind;
pub use input::OrbitCamInteractionSourcesChanged;
pub use input::OrbitCamInteractionSpeedChanged;
pub use input::OrbitCamInteractionStarted;
pub use input::OrbitCamInteractionState;
pub use input::OrbitCamKeyboardPreset;
#[cfg(feature = "reflect-input-modes")]
pub use input::OrbitCamKeyboardPresetDraft;
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
#[cfg(feature = "reflect-input-modes")]
pub use input::OrbitCamPresetDraft;
pub use input::OrbitCamPresetKind;
pub use input::OrbitCamReportingDebounce;
use input::OrbitCamRoutingPlugin;
pub use input::OrbitCamScalePolicy;
pub use input::OrbitCamSensitivity;
#[cfg(feature = "reflect-input-modes")]
pub use input::OrbitCamSensitivityDraft;
pub use input::OrbitCamSimpleMouseKeyboardPreset;
#[cfg(feature = "reflect-input-modes")]
pub use input::OrbitCamSimpleMouseKeyboardPresetDraft;
pub use input::OrbitCamSimpleMousePreset;
#[cfg(feature = "reflect-input-modes")]
pub use input::OrbitCamSimpleMousePresetDraft;
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

        #[cfg(feature = "reflect-input-modes")]
        app.register_type::<ActionBindingDescriptor>()
            .register_type::<BindingEngagement>()
            .register_type::<BindingGates>()
            .register_type::<BindingRoutePolicy>()
            .register_type::<CameraInputGamepadSelectionPolicy>()
            .register_type::<CameraInteractionSources>()
            .register_type::<ControlSpeed>()
            .register_type::<InputAxisTransform>()
            .register_type::<InputBindingModifiers>()
            .register_type::<InputBindingScale>()
            .register_type::<InputDeadZone>()
            .register_type::<InputDeltaScale>()
            .register_type::<InputSensitivity>()
            .register_type::<OrbitCamBindingGate>()
            .register_type::<OrbitCamBindingWithSensitivity<OrbitCamButtonDragZoom>>()
            .register_type::<OrbitCamBindingWithSensitivity<OrbitCamMouseWheelZoom>>()
            .register_type::<OrbitCamBindingWithSensitivity<OrbitCamPinchZoom>>()
            .register_type::<OrbitCamBindingWithSensitivity<OrbitCamTrackpadScroll>>()
            .register_type::<OrbitCamBindings>()
            .register_type::<OrbitCamBindingsDescriptor>()
            .register_type::<OrbitCamBlenderLikeKeyboardPreset>()
            .register_type::<OrbitCamBlenderLikeKeyboardPresetDraft>()
            .register_type::<OrbitCamBlenderLikePreset>()
            .register_type::<OrbitCamBlenderLikePresetDraft>()
            .register_type::<OrbitCamButtonDragZoom>()
            .register_type::<OrbitCamButtonDragZoomAxis>()
            .register_type::<OrbitCamGamepadPreset>()
            .register_type::<OrbitCamGamepadPresetDraft>()
            .register_type::<OrbitCamGateInput>()
            .register_type::<OrbitCamGatePolarity>()
            .register_type::<OrbitCamHeldBinding>()
            .register_type::<OrbitCamInputBinding>()
            .register_type::<OrbitCamInputModeDraft>()
            .register_type::<OrbitCamKeyboardPreset>()
            .register_type::<OrbitCamKeyboardPresetDraft>()
            .register_type::<OrbitCamMouseDrag>()
            .register_type::<OrbitCamMouseWheelZoom>()
            .register_type::<OrbitCamOrbitBinding>()
            .register_type::<OrbitCamPanBinding>()
            .register_type::<OrbitCamPinchZoom>()
            .register_type::<OrbitCamPreset>()
            .register_type::<OrbitCamPresetDraft>()
            .register_type::<OrbitCamPresetKind>()
            .register_type::<OrbitCamScalePolicy>()
            .register_type::<OrbitCamSensitivity>()
            .register_type::<OrbitCamSensitivityDraft>()
            .register_type::<OrbitCamSimpleMouseKeyboardPreset>()
            .register_type::<OrbitCamSimpleMouseKeyboardPresetDraft>()
            .register_type::<OrbitCamSimpleMousePreset>()
            .register_type::<OrbitCamSimpleMousePresetDraft>()
            .register_type::<OrbitCamSlowMode>()
            .register_type::<OrbitCamTouchBinding>()
            .register_type::<OrbitCamTouchBindingConfig>()
            .register_type::<OrbitCamTrackpadScroll>()
            .register_type::<OrbitCamZoomBinding>()
            .register_type::<ZoomInversion>();

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

#[cfg(test)]
mod tests {
    #[cfg(feature = "reflect-input-modes")]
    use std::any::TypeId;
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
            |event: On<AnimationEnd>, mut counts: ResMut<AnimationEventCounts>| {
                if matches!(event.reason, AnimationReason::Cancelled { .. }) {
                    counts.cancelled += 1;
                }
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
                OrbitCamInputMode::Manual,
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

    #[cfg(feature = "reflect-input-modes")]
    fn type_is_registered<T: 'static>(app: &App) -> bool {
        let registry = app.world().resource::<AppTypeRegistry>().read();
        registry.get(TypeId::of::<T>()).is_some()
    }

    #[cfg(feature = "reflect-input-modes")]
    #[test]
    fn reflect_input_mode_types_are_registered() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, LagrangePlugin));

        assert!(type_is_registered::<ActionBindingDescriptor>(&app));
        assert!(type_is_registered::<BindingEngagement>(&app));
        assert!(type_is_registered::<BindingGates>(&app));
        assert!(type_is_registered::<BindingRoutePolicy>(&app));
        assert!(type_is_registered::<CameraInputGamepadSelectionPolicy>(
            &app
        ));
        assert!(type_is_registered::<CameraInteractionSources>(&app));
        assert!(type_is_registered::<ControlSpeed>(&app));
        assert!(type_is_registered::<InputAxisTransform>(&app));
        assert!(type_is_registered::<InputBindingModifiers>(&app));
        assert!(type_is_registered::<InputBindingScale>(&app));
        assert!(type_is_registered::<InputDeadZone>(&app));
        assert!(type_is_registered::<InputDeltaScale>(&app));
        assert!(type_is_registered::<InputSensitivity>(&app));
        assert!(type_is_registered::<OrbitCamBindingGate>(&app));
        assert!(type_is_registered::<
            OrbitCamBindingWithSensitivity<OrbitCamButtonDragZoom>,
        >(&app));
        assert!(type_is_registered::<
            OrbitCamBindingWithSensitivity<OrbitCamMouseWheelZoom>,
        >(&app));
        assert!(type_is_registered::<
            OrbitCamBindingWithSensitivity<OrbitCamPinchZoom>,
        >(&app));
        assert!(type_is_registered::<
            OrbitCamBindingWithSensitivity<OrbitCamTrackpadScroll>,
        >(&app));
        assert!(type_is_registered::<OrbitCamBindings>(&app));
        assert!(type_is_registered::<OrbitCamBindingsDescriptor>(&app));
        assert!(type_is_registered::<OrbitCamBlenderLikeKeyboardPreset>(
            &app
        ));
        assert!(type_is_registered::<OrbitCamBlenderLikeKeyboardPresetDraft>(&app));
        assert!(type_is_registered::<OrbitCamBlenderLikePreset>(&app));
        assert!(type_is_registered::<OrbitCamBlenderLikePresetDraft>(&app));
        assert!(type_is_registered::<OrbitCamButtonDragZoom>(&app));
        assert!(type_is_registered::<OrbitCamButtonDragZoomAxis>(&app));
        assert!(type_is_registered::<OrbitCamGamepadPreset>(&app));
        assert!(type_is_registered::<OrbitCamGamepadPresetDraft>(&app));
        assert!(type_is_registered::<OrbitCamGateInput>(&app));
        assert!(type_is_registered::<OrbitCamGatePolarity>(&app));
        assert!(type_is_registered::<OrbitCamHeldBinding>(&app));
        assert!(type_is_registered::<OrbitCamInputBinding>(&app));
        assert!(type_is_registered::<OrbitCamInputMode>(&app));
        assert!(type_is_registered::<OrbitCamInputModeDescriptor>(&app));
        assert!(type_is_registered::<OrbitCamInputModeDraft>(&app));
        assert!(type_is_registered::<OrbitCamKeyboardPreset>(&app));
        assert!(type_is_registered::<OrbitCamKeyboardPresetDraft>(&app));
        assert!(type_is_registered::<OrbitCamMouseDrag>(&app));
        assert!(type_is_registered::<OrbitCamMouseWheelZoom>(&app));
        assert!(type_is_registered::<OrbitCamOrbitBinding>(&app));
        assert!(type_is_registered::<OrbitCamPanBinding>(&app));
        assert!(type_is_registered::<OrbitCamPinchZoom>(&app));
        assert!(type_is_registered::<OrbitCamPreset>(&app));
        assert!(type_is_registered::<OrbitCamPresetDraft>(&app));
        assert!(type_is_registered::<OrbitCamPresetKind>(&app));
        assert!(type_is_registered::<OrbitCamScalePolicy>(&app));
        assert!(type_is_registered::<OrbitCamSensitivity>(&app));
        assert!(type_is_registered::<OrbitCamSensitivityDraft>(&app));
        assert!(type_is_registered::<OrbitCamSimpleMouseKeyboardPreset>(
            &app
        ));
        assert!(type_is_registered::<OrbitCamSimpleMouseKeyboardPresetDraft>(&app));
        assert!(type_is_registered::<OrbitCamSimpleMousePreset>(&app));
        assert!(type_is_registered::<OrbitCamSimpleMousePresetDraft>(&app));
        assert!(type_is_registered::<OrbitCamSlowMode>(&app));
        assert!(type_is_registered::<OrbitCamTouchBinding>(&app));
        assert!(type_is_registered::<OrbitCamTouchBindingConfig>(&app));
        assert!(type_is_registered::<OrbitCamTrackpadScroll>(&app));
        assert!(type_is_registered::<OrbitCamZoomBinding>(&app));
        assert!(type_is_registered::<ZoomInversion>(&app));
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
