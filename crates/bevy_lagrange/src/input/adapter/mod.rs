//! Enhanced-input adapter: installs per-camera action entities and resolves them into
//! [`OrbitCamInput`] each frame.
//!
//! The work is split into three pipeline stages, each in its own submodule, scheduled in
//! distinct sets of [`OrbitCamInputInternalSet`]:
//!
//! - [`install`] (`Installation` set) — clears stale installations, builds the action and binding
//!   entities for each camera, and gates the input context per camera.
//! - [`inject`] (`AdapterInjection` set) — each frame translates raw `bevy::input` resources (wheel
//!   / pinch / touch / button-drag) into custom inputs consumed by adapter actions.
//! - [`resolve`] (`ActionResolution` set) — reads the resulting action state and writes the
//!   per-camera [`OrbitCamInput`].
//!
//! Only [`OrbitCamInputAdapterPlugin`] is exposed outside this module.

mod inject;
mod install;
mod resolve;

use bevy::prelude::*;
use bevy_enhanced_input::prelude::InputConditionAppExt;
use install::OrbitCamAdapterDiagnostics;
use install::OrbitCamBindingGateCondition;

use crate::system_sets::OrbitCamInputInternalSet;

pub(crate) struct OrbitCamInputAdapterPlugin;

impl Plugin for OrbitCamInputAdapterPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<OrbitCamAdapterDiagnostics>()
            .add_input_condition::<install::TrackpadBindingCondition>()
            .add_input_condition::<OrbitCamBindingGateCondition>()
            .add_systems(
                PreUpdate,
                (
                    install::clear_replaced_or_manual_installations,
                    install::install_enhanced_input_entities,
                    install::apply_context_gating,
                )
                    .chain()
                    .in_set(OrbitCamInputInternalSet::Installation),
            )
            .add_systems(
                PreUpdate,
                inject::inject_adapter_actions.in_set(OrbitCamInputInternalSet::AdapterInjection),
            )
            .add_systems(
                PreUpdate,
                resolve::resolve_actions_into_orbit_cam_input
                    .in_set(OrbitCamInputInternalSet::ActionResolution),
            );
    }
}

#[cfg(test)]
mod tests {
    use core::time::Duration;

    use bevy::camera::RenderTarget;
    use bevy::input::gamepad::Gamepad;
    use bevy::input::gestures::PinchGesture;
    use bevy::input::mouse::AccumulatedMouseMotion;
    use bevy::input::mouse::AccumulatedMouseScroll;
    use bevy::input::mouse::MouseScrollUnit;
    use bevy::prelude::*;
    use bevy::window::WindowRef;
    use bevy_enhanced_input::prelude::Binding;
    use bevy_enhanced_input::prelude::BindingOf;
    use bevy_enhanced_input::prelude::DeadZone;
    use bevy_enhanced_input::prelude::DeltaScale;
    use bevy_enhanced_input::prelude::Scale;

    use super::OrbitCamInputAdapterPlugin;
    use super::inject::OrbitCamTouchAdapterOverride;
    use super::install::OrbitCamBindingGateCondition;
    use super::install::OrbitCamInputActionEntities;
    use super::install::TrackpadBindingCondition;
    use crate::constants::PINCH_GESTURE_AMPLIFICATION;
    use crate::constants::PIXEL_SCROLL_SCALE;
    use crate::enhanced_input::LagrangeEnhancedInputPlugin;
    use crate::input::CameraInputDisabled;
    use crate::input::CameraInputGamepadSelectionPolicy;
    use crate::input::CameraInputRoutingConfig;
    use crate::input::CameraInteractionSources;
    use crate::input::ControlSpeed;
    use crate::input::OrbitCamBindings;
    use crate::input::OrbitCamHeldBinding;
    use crate::input::OrbitCamInput;
    use crate::input::OrbitCamInputBinding;
    use crate::input::OrbitCamInputContext;
    use crate::input::OrbitCamInputMode;
    use crate::input::OrbitCamPinchZoom;
    use crate::input::OrbitCamPreset;
    use crate::input::OrbitCamTouchBinding;
    use crate::input::OrbitCamTrackpadScroll;
    use crate::input::OrbitDelta;
    use crate::input::modes;
    use crate::input::modes::OrbitCamInputModesPlugin;
    use crate::input::routing::OrbitCamRoutingPlugin;
    use crate::orbit_cam::OrbitCam;
    use crate::system_sets::LagrangeSystemSetsPlugin;
    use crate::touch::OneFingerGestures;
    use crate::touch::TouchGestures;
    use crate::touch::TouchTracker;

    fn test_app() -> App {
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            LagrangeEnhancedInputPlugin,
            LagrangeSystemSetsPlugin,
            OrbitCamInputModesPlugin,
            OrbitCamRoutingPlugin,
            OrbitCamInputAdapterPlugin,
        ));
        app.init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<ButtonInput<MouseButton>>()
            .init_resource::<AccumulatedMouseMotion>()
            .init_resource::<AccumulatedMouseScroll>()
            .init_resource::<TouchTracker>()
            .add_message::<PinchGesture>();
        app.finish();
        app
    }

    fn spawn_camera(world: &mut World, components: impl Bundle) -> Entity {
        world
            .spawn((
                OrbitCam::default(),
                OrbitCamInput::default(),
                Camera::default(),
                RenderTarget::Window(WindowRef::Primary),
                components,
            ))
            .id()
    }

    fn route_to(app: &mut App, camera: Entity) {
        app.insert_resource(CameraInputRoutingConfig::explicit(camera));
    }

    type TestResult = Result<(), &'static str>;

    fn camera_input(app: &App, camera: Entity) -> Result<&OrbitCamInput, &'static str> {
        app.world()
            .get::<OrbitCamInput>(camera)
            .ok_or("camera should have OrbitCamInput")
    }

    fn assert_f32_close(actual: f32, expected: f32) {
        assert!((actual - expected).abs() <= f32::EPSILON);
    }

    #[test]
    fn installer_replaces_placeholder_with_action_entities() {
        let mut app = test_app();
        let camera = spawn_camera(
            app.world_mut(),
            OrbitCamInputMode::Preset(OrbitCamPreset::SimpleMouse),
        );
        route_to(&mut app, camera);

        app.update();

        assert!(app.world().get::<OrbitCamInputContext>(camera).is_some());
        assert!(
            app.world()
                .get::<OrbitCamInputActionEntities>(camera)
                .is_some()
        );
        assert!(modes::installed_input_entities(app.world(), camera).len() > 1);
        assert!(!modes::input_installation_has_placeholder(
            app.world(),
            camera
        ));
    }

    #[test]
    fn installer_binds_adapter_actions_to_custom_inputs() -> TestResult {
        let mut app = test_app();
        let camera = spawn_camera(
            app.world_mut(),
            OrbitCamInputMode::Preset(OrbitCamPreset::SimpleMouse),
        );
        route_to(&mut app, camera);

        app.update();

        let actions = *app
            .world()
            .get::<OrbitCamInputActionEntities>(camera)
            .ok_or("adapter actions should be installed")?;
        let custom_bound_actions = modes::installed_input_entities(app.world(), camera)
            .into_iter()
            .filter_map(|entity| {
                let binding = app.world().get::<Binding>(entity)?;
                let binding_of = app.world().get::<BindingOf>(entity)?;
                matches!(binding, Binding::Custom(_)).then_some(**binding_of)
            })
            .collect::<Vec<_>>();
        let trackpad_custom_bindings = modes::installed_input_entities(app.world(), camera)
            .into_iter()
            .filter(|entity| {
                app.world()
                    .get::<Binding>(*entity)
                    .is_some_and(|binding| matches!(binding, Binding::Custom(_)))
            })
            .filter(|entity| {
                app.world()
                    .get::<TrackpadBindingCondition>(*entity)
                    .is_some()
            })
            .count();

        assert_eq!(custom_bound_actions.len(), 5);
        assert!(custom_bound_actions.contains(&actions.adapter_orbit));
        assert!(custom_bound_actions.contains(&actions.adapter_pan));
        assert!(custom_bound_actions.contains(&actions.adapter_zoom_coarse));
        assert!(custom_bound_actions.contains(&actions.adapter_zoom_smooth));
        assert_eq!(trackpad_custom_bindings, 1);
        Ok(())
    }

    #[test]
    fn mouse_drag_action_resolves_to_orbit_input() -> TestResult {
        let mut app = test_app();
        let camera = spawn_camera(
            app.world_mut(),
            OrbitCamInputMode::Preset(OrbitCamPreset::SimpleMouse),
        );
        route_to(&mut app, camera);
        app.world_mut()
            .resource_mut::<ButtonInput<MouseButton>>()
            .press(MouseButton::Left);
        app.world_mut()
            .resource_mut::<AccumulatedMouseMotion>()
            .delta = Vec2::new(5.0, -2.0);

        app.update();

        let input = camera_input(&app, camera)?;
        assert_eq!(input.orbit(), OrbitDelta::from(Vec2::new(5.0, -2.0)));
        assert!(input.has_orbit());
        assert!(input.sources().contains(CameraInteractionSources::MOUSE));
        Ok(())
    }

    #[test]
    fn blender_like_shift_middle_mouse_resolves_to_pan_only() -> TestResult {
        let mut app = test_app();
        let camera = spawn_camera(
            app.world_mut(),
            OrbitCamInputMode::Preset(OrbitCamPreset::BlenderLike),
        );
        route_to(&mut app, camera);
        app.world_mut()
            .resource_mut::<ButtonInput<MouseButton>>()
            .press(MouseButton::Middle);
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::ShiftLeft);
        app.world_mut()
            .resource_mut::<AccumulatedMouseMotion>()
            .delta = Vec2::new(5.0, -2.0);

        app.update();

        let input = camera_input(&app, camera)?;
        assert!(!input.has_orbit());
        assert_eq!(input.pan().pixels(), Vec2::new(5.0, -2.0));
        assert!(input.sources().contains(CameraInteractionSources::MOUSE));
        Ok(())
    }

    #[test]
    fn wheel_line_adapter_resolves_to_coarse_zoom() -> TestResult {
        let mut app = test_app();
        let camera = spawn_camera(
            app.world_mut(),
            OrbitCamInputMode::Preset(OrbitCamPreset::SimpleMouse),
        );
        route_to(&mut app, camera);
        *app.world_mut().resource_mut::<AccumulatedMouseScroll>() = AccumulatedMouseScroll {
            unit:  MouseScrollUnit::Line,
            delta: Vec2::new(0.0, 3.0),
        };

        app.update();

        let input = camera_input(&app, camera)?;
        assert_f32_close(input.zoom_coarse().amount(), 3.0);
        assert!(input.has_zoom());
        assert!(input.sources().contains(CameraInteractionSources::WHEEL));
        Ok(())
    }

    #[test]
    fn blender_like_trackpad_shift_resolves_to_pan_only() -> TestResult {
        let mut app = test_app();
        let camera = spawn_camera(
            app.world_mut(),
            OrbitCamInputMode::Preset(OrbitCamPreset::BlenderLike),
        );
        route_to(&mut app, camera);
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::ShiftLeft);
        *app.world_mut().resource_mut::<AccumulatedMouseScroll>() = AccumulatedMouseScroll {
            unit:  MouseScrollUnit::Pixel,
            delta: Vec2::new(4.0, 6.0),
        };

        app.update();

        let input = camera_input(&app, camera)?;
        assert!(!input.has_orbit());
        assert_eq!(input.pan().pixels(), Vec2::new(4.0, 6.0));
        assert!(
            input
                .sources()
                .contains(CameraInteractionSources::SMOOTH_SCROLL)
        );
        Ok(())
    }

    #[test]
    fn routed_second_blender_like_trackpad_resolves_to_orbit_only() -> TestResult {
        let mut app = test_app();
        let primary = spawn_camera(
            app.world_mut(),
            OrbitCamInputMode::Preset(OrbitCamPreset::BlenderLike),
        );
        let second = spawn_camera(
            app.world_mut(),
            OrbitCamInputMode::Preset(OrbitCamPreset::BlenderLike),
        );
        route_to(&mut app, second);
        *app.world_mut().resource_mut::<AccumulatedMouseScroll>() = AccumulatedMouseScroll {
            unit:  MouseScrollUnit::Pixel,
            delta: Vec2::new(4.0, 6.0),
        };

        app.update();

        assert!(!camera_input(&app, primary)?.has_input());
        let second_input = camera_input(&app, second)?;
        assert_eq!(second_input.orbit(), OrbitDelta::from(Vec2::new(4.0, 6.0)));
        assert!(
            second_input
                .sources()
                .contains(CameraInteractionSources::SMOOTH_SCROLL)
        );
        Ok(())
    }

    #[test]
    fn blender_like_trackpad_control_resolves_to_zoom_only() -> TestResult {
        let mut app = test_app();
        let camera = spawn_camera(
            app.world_mut(),
            OrbitCamInputMode::Preset(OrbitCamPreset::BlenderLike),
        );
        route_to(&mut app, camera);
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::ControlLeft);
        *app.world_mut().resource_mut::<AccumulatedMouseScroll>() = AccumulatedMouseScroll {
            unit:  MouseScrollUnit::Pixel,
            delta: Vec2::new(4.0, 6.0),
        };

        app.update();

        let input = camera_input(&app, camera)?;
        assert!(!input.has_orbit());
        assert!(!input.has_pan());
        assert_f32_close(input.zoom_smooth().amount(), 6.0 * PIXEL_SCROLL_SCALE);
        assert!(
            input
                .sources()
                .contains(CameraInteractionSources::SMOOTH_SCROLL)
        );
        Ok(())
    }

    #[test]
    fn pixel_scroll_adapter_resolves_to_smooth_zoom() -> TestResult {
        let mut app = test_app();
        let bindings = OrbitCamBindings::builder()
            .zoom(OrbitCamTrackpadScroll::default())
            .build()
            .map_err(|_| "bindings should validate")?;
        let camera = spawn_camera(app.world_mut(), OrbitCamInputMode::Bindings(bindings));
        route_to(&mut app, camera);
        *app.world_mut().resource_mut::<AccumulatedMouseScroll>() = AccumulatedMouseScroll {
            unit:  MouseScrollUnit::Pixel,
            delta: Vec2::new(0.0, 20.0),
        };

        app.update();

        let input = camera_input(&app, camera)?;
        assert_f32_close(input.zoom_smooth().amount(), 20.0 * PIXEL_SCROLL_SCALE);
        assert!(
            input
                .sources()
                .contains(CameraInteractionSources::SMOOTH_SCROLL)
        );
        Ok(())
    }

    #[test]
    fn pinch_adapter_resolves_to_smooth_zoom() -> TestResult {
        let mut app = test_app();
        let camera = spawn_camera(
            app.world_mut(),
            OrbitCamInputMode::Preset(OrbitCamPreset::SimpleMouse),
        );
        route_to(&mut app, camera);
        app.world_mut().write_message(PinchGesture(2.0));

        app.update();

        let input = camera_input(&app, camera)?;
        assert_f32_close(
            input.zoom_smooth().amount(),
            2.0 * PINCH_GESTURE_AMPLIFICATION,
        );
        assert!(input.sources().contains(CameraInteractionSources::PINCH));
        Ok(())
    }

    #[test]
    fn pinch_adapter_is_suppressed_by_routed_held_action() -> TestResult {
        let mut app = test_app();
        let camera = spawn_camera(
            app.world_mut(),
            OrbitCamInputMode::Preset(OrbitCamPreset::SimpleMouse),
        );
        route_to(&mut app, camera);
        app.world_mut()
            .resource_mut::<ButtonInput<MouseButton>>()
            .press(MouseButton::Left);
        app.world_mut().write_message(PinchGesture(2.0));

        app.update();

        let input = camera_input(&app, camera)?;
        assert_f32_close(input.zoom_smooth().amount(), 0.0);
        assert!(!input.sources().contains(CameraInteractionSources::PINCH));
        Ok(())
    }

    #[test]
    fn blender_like_shift_modifier_suppresses_pinch() -> TestResult {
        let mut app = test_app();
        let camera = spawn_camera(
            app.world_mut(),
            OrbitCamInputMode::Preset(OrbitCamPreset::BlenderLike),
        );
        route_to(&mut app, camera);
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::ShiftLeft);
        app.world_mut().write_message(PinchGesture(2.0));

        app.update();

        let input = camera_input(&app, camera)?;
        assert_f32_close(input.zoom_smooth().amount(), 0.0);
        assert!(!input.sources().contains(CameraInteractionSources::PINCH));
        Ok(())
    }

    #[test]
    fn blender_like_control_modifier_suppresses_pinch() -> TestResult {
        let mut app = test_app();
        let camera = spawn_camera(
            app.world_mut(),
            OrbitCamInputMode::Preset(OrbitCamPreset::BlenderLike),
        );
        route_to(&mut app, camera);
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::ControlLeft);
        app.world_mut().write_message(PinchGesture(2.0));

        app.update();

        let input = camera_input(&app, camera)?;
        assert_f32_close(input.zoom_smooth().amount(), 0.0);
        assert!(!input.sources().contains(CameraInteractionSources::PINCH));
        Ok(())
    }

    #[test]
    fn non_routed_held_action_does_not_suppress_routed_pinch() -> TestResult {
        let mut app = test_app();
        let bindings = OrbitCamBindings::builder()
            .zoom(OrbitCamPinchZoom)
            .build()
            .map_err(|_| "bindings should validate")?;
        let routed = spawn_camera(app.world_mut(), OrbitCamInputMode::Bindings(bindings));
        spawn_camera(
            app.world_mut(),
            OrbitCamInputMode::Preset(OrbitCamPreset::SimpleMouse),
        );
        route_to(&mut app, routed);
        app.world_mut()
            .resource_mut::<ButtonInput<MouseButton>>()
            .press(MouseButton::Left);
        app.world_mut().write_message(PinchGesture(2.0));

        app.update();

        let input = camera_input(&app, routed)?;
        assert_f32_close(
            input.zoom_smooth().amount(),
            2.0 * PINCH_GESTURE_AMPLIFICATION,
        );
        assert!(input.sources().contains(CameraInteractionSources::PINCH));
        Ok(())
    }

    #[test]
    fn touch_adapter_resolves_to_orbit_input() -> TestResult {
        let mut app = test_app();
        let bindings = OrbitCamBindings::builder()
            .touch(Some(OrbitCamTouchBinding::OneFingerOrbit))
            .build()
            .map_err(|_| "bindings should validate")?;
        let camera = spawn_camera(app.world_mut(), OrbitCamInputMode::Bindings(bindings));
        route_to(&mut app, camera);
        app.insert_resource(OrbitCamTouchAdapterOverride(TouchGestures::OneFinger(
            OneFingerGestures {
                motion: Vec2::new(7.0, 8.0),
            },
        )));

        app.update();

        let input = camera_input(&app, camera)?;
        assert_eq!(input.orbit(), OrbitDelta::from(Vec2::new(7.0, 8.0)));
        assert!(input.sources().contains(CameraInteractionSources::TOUCH));
        Ok(())
    }

    #[test]
    fn keyboard_binding_resolves_to_smooth_zoom() -> TestResult {
        let mut app = test_app();
        let bindings = OrbitCamBindings::builder()
            .zoom(OrbitCamHeldBinding::new(KeyCode::Equal, KeyCode::ShiftLeft))
            .build()
            .map_err(|_| "bindings should validate")?;
        let camera = spawn_camera(app.world_mut(), OrbitCamInputMode::Bindings(bindings));
        route_to(&mut app, camera);
        {
            let mut keyboard = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
            keyboard.press(KeyCode::Equal);
            keyboard.press(KeyCode::ShiftLeft);
        }

        app.update();

        let input = camera_input(&app, camera)?;
        assert_f32_close(input.zoom_smooth().amount(), 1.0);
        assert!(input.sources().contains(CameraInteractionSources::KEYBOARD));
        Ok(())
    }

    #[test]
    fn gamepad_binding_resolves_to_orbit_input() -> TestResult {
        let mut app = test_app();
        let bindings = OrbitCamBindings::builder()
            .orbit(OrbitCamHeldBinding::new(
                GamepadAxis::LeftStickX,
                GamepadButton::LeftTrigger2,
            ))
            .gamepad(CameraInputGamepadSelectionPolicy::Active)
            .build()
            .map_err(|_| "bindings should validate")?;
        let camera = spawn_camera(app.world_mut(), OrbitCamInputMode::Bindings(bindings));
        route_to(&mut app, camera);
        let mut gamepad = Gamepad::default();
        gamepad.analog_mut().set(GamepadAxis::LeftStickX, 0.75);
        gamepad.analog_mut().set(GamepadButton::LeftTrigger2, 1.0);
        gamepad.digital_mut().press(GamepadButton::LeftTrigger2);
        app.world_mut().spawn(gamepad);

        app.update();

        let input = camera_input(&app, camera)?;
        assert_eq!(input.orbit().pixels(), Vec2::new(0.75, 0.0));
        assert!(input.sources().contains(CameraInteractionSources::GAMEPAD));
        Ok(())
    }

    #[test]
    fn cardinal_keyboard_binding_resolves_to_orbit_input() -> TestResult {
        let mut app = test_app();
        let bindings = OrbitCamBindings::builder()
            .orbit(OrbitCamInputBinding::cardinal_keys(
                KeyCode::ArrowUp,
                KeyCode::ArrowRight,
                KeyCode::ArrowDown,
                KeyCode::ArrowLeft,
            ))
            .build()
            .map_err(|_| "bindings should validate")?;
        let camera = spawn_camera(app.world_mut(), OrbitCamInputMode::Bindings(bindings));
        route_to(&mut app, camera);
        {
            let mut keyboard = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
            keyboard.press(KeyCode::ArrowRight);
            keyboard.press(KeyCode::ArrowUp);
        }

        app.update();

        let input = camera_input(&app, camera)?;
        assert_eq!(input.orbit().pixels(), Vec2::ONE);
        assert!(input.sources().contains(CameraInteractionSources::KEYBOARD));
        Ok(())
    }

    #[test]
    fn bidirectional_keyboard_binding_resolves_to_smooth_zoom() -> TestResult {
        let mut app = test_app();
        let bindings = OrbitCamBindings::builder()
            .zoom(OrbitCamInputBinding::bidirectional_keys(
                KeyCode::Equal,
                KeyCode::Minus,
            ))
            .build()
            .map_err(|_| "bindings should validate")?;
        let camera = spawn_camera(app.world_mut(), OrbitCamInputMode::Bindings(bindings));
        route_to(&mut app, camera);
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Minus);

        app.update();

        let input = camera_input(&app, camera)?;
        assert_f32_close(input.zoom_smooth().amount(), -1.0);
        assert!(input.sources().contains(CameraInteractionSources::KEYBOARD));
        Ok(())
    }

    #[test]
    fn gamepad_axes2d_binding_resolves_to_orbit_input() -> TestResult {
        let mut app = test_app();
        let bindings = OrbitCamBindings::builder()
            .orbit(OrbitCamInputBinding::gamepad_axes_2d(
                GamepadAxis::RightStickX,
                GamepadAxis::RightStickY,
            ))
            .gamepad(CameraInputGamepadSelectionPolicy::Active)
            .build()
            .map_err(|_| "bindings should validate")?;
        let camera = spawn_camera(app.world_mut(), OrbitCamInputMode::Bindings(bindings));
        route_to(&mut app, camera);
        let mut gamepad = Gamepad::default();
        gamepad.analog_mut().set(GamepadAxis::RightStickX, 0.5);
        gamepad.analog_mut().set(GamepadAxis::RightStickY, -0.25);
        app.world_mut().spawn(gamepad);

        app.update();

        let input = camera_input(&app, camera)?;
        assert_eq!(input.orbit().pixels(), Vec2::new(0.5, -0.25));
        assert!(input.sources().contains(CameraInteractionSources::GAMEPAD));
        Ok(())
    }

    #[test]
    fn bidirectional_gamepad_buttons_resolve_to_smooth_zoom() -> TestResult {
        let mut app = test_app();
        let bindings = OrbitCamBindings::builder()
            .zoom(OrbitCamInputBinding::bidirectional_gamepad_buttons(
                GamepadButton::RightTrigger2,
                GamepadButton::LeftTrigger2,
            ))
            .gamepad(CameraInputGamepadSelectionPolicy::Active)
            .build()
            .map_err(|_| "bindings should validate")?;
        let camera = spawn_camera(app.world_mut(), OrbitCamInputMode::Bindings(bindings));
        route_to(&mut app, camera);
        let mut gamepad = Gamepad::default();
        gamepad.analog_mut().set(GamepadButton::LeftTrigger2, 0.4);
        gamepad.digital_mut().press(GamepadButton::LeftTrigger2);
        app.world_mut().spawn(gamepad);

        app.update();

        let input = camera_input(&app, camera)?;
        assert_f32_close(input.zoom_smooth().amount(), -0.4);
        assert!(input.sources().contains(CameraInteractionSources::GAMEPAD));
        Ok(())
    }

    #[test]
    fn gamepad_preset_requires_no_position_route() -> TestResult {
        let mut app = test_app();
        let camera = spawn_camera(
            app.world_mut(),
            OrbitCamInputMode::Preset(OrbitCamPreset::Gamepad),
        );
        let mut gamepad = Gamepad::default();
        gamepad.analog_mut().set(GamepadAxis::RightStickX, 1.0);
        app.world_mut().spawn(gamepad);

        app.update();

        assert!(!camera_input(&app, camera)?.has_input());
        Ok(())
    }

    #[test]
    fn gamepad_preset_installs_modifiers_and_gate_conditions() {
        let mut app = test_app();
        let camera = spawn_camera(
            app.world_mut(),
            OrbitCamInputMode::Preset(OrbitCamPreset::Gamepad),
        );
        route_to(&mut app, camera);

        app.update();

        let installed = modes::installed_input_entities(app.world(), camera);
        assert!(
            installed
                .iter()
                .any(|entity| app.world().get::<DeadZone>(*entity).is_some())
        );
        assert!(
            installed
                .iter()
                .any(|entity| app.world().get::<Scale>(*entity).is_some())
        );
        assert!(
            installed
                .iter()
                .any(|entity| app.world().get::<DeltaScale>(*entity).is_some())
        );
        assert!(installed.iter().any(|entity| {
            app.world()
                .get::<OrbitCamBindingGateCondition>(*entity)
                .is_some()
        }));
    }

    /// Drives the gamepad preset's gated motion-split path with a real gamepad.
    /// The `InputReader` resolves a `GamepadButton` gate through its analog
    /// channel (`gamepad.get(button)`), so the gate button must have its analog
    /// value set — a bare `digital_mut().press` reads as `0.0` and never
    /// engages the gate. Right-stick orbit resolves `Normal`; adding the
    /// RightTrigger gate must flip the resolved speed to `Slow` while orbit
    /// stays active (fast is blocked by the same trigger, slow requires it).
    #[test]
    fn gamepad_preset_slow_orbit_gate_resolves_speed() -> TestResult {
        let mut app = test_app();
        let camera = spawn_camera(
            app.world_mut(),
            OrbitCamInputMode::Preset(OrbitCamPreset::Gamepad),
        );
        route_to(&mut app, camera);
        let gamepad_entity = app.world_mut().spawn(Gamepad::default()).id();
        app.update();

        {
            let mut gamepad = app
                .world_mut()
                .get_mut::<Gamepad>(gamepad_entity)
                .ok_or("gamepad should exist")?;
            gamepad.analog_mut().set(GamepadAxis::RightStickX, 1.0);
        }
        app.world_mut()
            .resource_mut::<Time>()
            .advance_by(Duration::from_millis(16));
        app.update();
        assert!(camera_input(&app, camera)?.has_orbit());
        assert_eq!(
            camera_input(&app, camera)?.orbit_speed(),
            ControlSpeed::Normal
        );

        {
            let mut gamepad = app
                .world_mut()
                .get_mut::<Gamepad>(gamepad_entity)
                .ok_or("gamepad should exist")?;
            gamepad.analog_mut().set(GamepadButton::RightTrigger, 1.0);
            gamepad.digital_mut().press(GamepadButton::RightTrigger);
        }
        // The gate is a separate action the binding condition reads, so it
        // settles a frame after the trigger registers; keep the stick deflected
        // and advance time each frame so orbit stays active.
        for _ in 0..2 {
            {
                let mut gamepad = app
                    .world_mut()
                    .get_mut::<Gamepad>(gamepad_entity)
                    .ok_or("gamepad should exist")?;
                gamepad.analog_mut().set(GamepadAxis::RightStickX, 1.0);
            }
            app.world_mut()
                .resource_mut::<Time>()
                .advance_by(Duration::from_millis(16));
            app.update();
        }

        assert!(camera_input(&app, camera)?.has_orbit());
        assert_eq!(
            camera_input(&app, camera)?.orbit_speed(),
            ControlSpeed::Slow
        );
        Ok(())
    }

    /// Drives the gated motion-split path with a keyboard gate (gamepad digital
    /// buttons don't propagate through this harness). Fast orbit is blocked by
    /// Shift; the slow variant requires Shift and is tagged `Slow`, so pressing
    /// Shift must flip the resolved speed without changing the source set.
    #[test]
    fn keyboard_gated_slow_orbit_resolves_speed() -> TestResult {
        let bindings = OrbitCamBindings::builder()
            .orbit(
                OrbitCamHeldBinding::new(Binding::mouse_motion(), MouseButton::Left)
                    .with_blocked_gate(KeyCode::ShiftLeft),
            )
            .orbit(
                OrbitCamHeldBinding::new(Binding::mouse_motion(), MouseButton::Left)
                    .with_required_gate(KeyCode::ShiftLeft)
                    .speed(ControlSpeed::Slow),
            )
            .build()
            .map_err(|_| "bindings should build")?;

        let mut app = test_app();
        let camera = spawn_camera(app.world_mut(), OrbitCamInputMode::Bindings(bindings));
        route_to(&mut app, camera);
        app.world_mut()
            .resource_mut::<ButtonInput<MouseButton>>()
            .press(MouseButton::Left);
        app.world_mut()
            .resource_mut::<AccumulatedMouseMotion>()
            .delta = Vec2::new(4.0, -3.0);
        app.update();
        assert!(camera_input(&app, camera)?.has_orbit());
        assert_eq!(
            camera_input(&app, camera)?.orbit_speed(),
            ControlSpeed::Normal
        );

        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::ShiftLeft);
        // The gate is a separate action the binding condition reads, so it
        // settles a frame after the key registers; re-assert motion each frame.
        for _ in 0..2 {
            app.world_mut()
                .resource_mut::<AccumulatedMouseMotion>()
                .delta = Vec2::new(4.0, -3.0);
            app.update();
        }
        assert!(camera_input(&app, camera)?.has_orbit());
        assert_eq!(
            camera_input(&app, camera)?.orbit_speed(),
            ControlSpeed::Slow
        );
        Ok(())
    }

    #[test]
    fn gamepad_gate_conditions_despawn_when_mode_changes() {
        let mut app = test_app();
        let camera = spawn_camera(
            app.world_mut(),
            OrbitCamInputMode::Preset(OrbitCamPreset::Gamepad),
        );
        route_to(&mut app, camera);
        app.update();

        assert!(
            modes::installed_input_entities(app.world(), camera)
                .iter()
                .any(|entity| app
                    .world()
                    .get::<OrbitCamBindingGateCondition>(*entity)
                    .is_some())
        );

        app.world_mut()
            .entity_mut(camera)
            .insert(OrbitCamInputMode::Preset(OrbitCamPreset::SimpleMouse));
        app.update();

        assert!(
            !modes::installed_input_entities(app.world(), camera)
                .iter()
                .any(|entity| app
                    .world()
                    .get::<OrbitCamBindingGateCondition>(*entity)
                    .is_some())
        );
    }

    #[test]
    fn manual_mode_bypasses_action_resolution() -> TestResult {
        let mut app = test_app();
        let camera = spawn_camera(app.world_mut(), OrbitCamInputMode::Manual);
        route_to(&mut app, camera);
        *app.world_mut().resource_mut::<AccumulatedMouseScroll>() = AccumulatedMouseScroll {
            unit:  MouseScrollUnit::Line,
            delta: Vec2::new(0.0, 3.0),
        };

        app.update();

        assert!(app.world().get::<OrbitCamInputContext>(camera).is_none());
        assert!(!camera_input(&app, camera)?.has_input());
        Ok(())
    }

    #[test]
    fn gated_camera_clears_previous_action_input() -> TestResult {
        let mut app = test_app();
        let camera = spawn_camera(
            app.world_mut(),
            OrbitCamInputMode::Preset(OrbitCamPreset::SimpleMouse),
        );
        route_to(&mut app, camera);
        *app.world_mut().resource_mut::<AccumulatedMouseScroll>() = AccumulatedMouseScroll {
            unit:  MouseScrollUnit::Line,
            delta: Vec2::new(0.0, 3.0),
        };
        app.update();
        assert!(camera_input(&app, camera)?.has_zoom());

        app.world_mut()
            .entity_mut(camera)
            .insert(CameraInputDisabled);
        *app.world_mut().resource_mut::<AccumulatedMouseScroll>() = AccumulatedMouseScroll {
            unit:  MouseScrollUnit::Line,
            delta: Vec2::new(0.0, 3.0),
        };
        app.update();

        assert!(!camera_input(&app, camera)?.has_input());
        Ok(())
    }
}
