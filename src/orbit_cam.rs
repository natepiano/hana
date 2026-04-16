//! `OrbitCam` component, systems, and helpers.

use std::f32::consts::PI;
use std::f32::consts::TAU;

use bevy::camera::RenderTarget;
use bevy::input::gestures::PinchGesture;
use bevy::input::mouse::MouseWheel;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy::window::WindowRef;

use super::constants::DEFAULT_ORBIT_SMOOTHNESS;
use super::constants::DEFAULT_PAN_SMOOTHNESS;
use super::constants::DEFAULT_TARGET_RADIUS;
use super::constants::DEFAULT_ZOOM_LOWER_LIMIT;
use super::constants::DEFAULT_ZOOM_SMOOTHNESS;
use super::constants::SCROLL_ZOOM_FACTOR;
use super::constants::TOUCH_PINCH_SCALE;
#[cfg(feature = "bevy_egui")]
use super::egui::BlockOnEguiFocus;
#[cfg(feature = "bevy_egui")]
use super::egui::EguiWantsFocus;
use super::input;
use super::input::MouseKeyTracker;
use super::touch::TouchGestures;
use super::touch::TouchInput;
use super::touch::TouchTracker;
use super::traits;
use super::types::ActiveCameraData;
use super::types::ButtonZoomAxis;
use super::types::CameraInputDetection;
use super::types::FocusBoundsShape;
use super::types::ForceUpdate;
use super::types::InitializationState;
use super::types::InputControl;
use super::types::TimeSource;
use super::types::UpsideDownPolicy;
use super::types::ZoomDirection;
use super::util;

/// Base system set to allow ordering of `OrbitCam`
#[derive(SystemSet, Debug, Hash, PartialEq, Eq, Clone)]
pub struct OrbitCamSystemSet;

/// Internal per-camera state used to keep orbit direction stable during a drag.
#[derive(Component, Default, Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) struct OrbitDragState {
    orientation: CameraOrientation,
}

/// Whether the camera was latched as upside down when orbit dragging started.
#[derive(Clone, PartialEq, Eq, Debug, Copy, Default)]
enum CameraOrientation {
    #[default]
    Normal,
    UpsideDown,
}

/// Tags an entity as capable of panning and orbiting.
///
/// Provides a way to configure the camera's behaviour and controls.
/// # Example
/// ```no_run
/// # use bevy::prelude::*;
/// # use bevy_lagrange::{LagrangePlugin, OrbitCam};
/// # fn main() {
/// #     App::new()
/// #         .add_plugins(DefaultPlugins)
/// #         .add_plugins(LagrangePlugin)
/// #         .add_systems(Startup, setup)
/// #         .run();
/// # }
/// fn setup(mut commands: Commands) {
///     commands.spawn((
///         Transform::from_translation(Vec3::new(0.0, 1.5, 5.0)),
///         OrbitCam::default(),
///     ));
/// }
/// ```
#[derive(Component, Reflect, Copy, Clone, Debug, PartialEq)]
#[reflect(Component)]
#[require(Camera3d, OrbitDragState)]
pub struct OrbitCam {
    /// The point to orbit around, and what the camera looks at. Updated automatically.
    /// If you want to change the focus programmatically after initialization, set `target_focus`
    /// instead.
    /// Defaults to `Vec3::ZERO`.
    pub focus:               Vec3,
    /// The radius of the orbit, or the distance from the `focus` point.
    /// For orthographic projection, this is ignored, and the projection's `scale` is used instead.
    /// If set to `None`, it will be calculated from the camera's current position during
    /// initialization.
    /// Automatically updated.
    /// Defaults to `None`.
    pub radius:              Option<f32>,
    /// Rotation in radians around the global Y axis (longitudinal). Updated automatically.
    /// If both `yaw` and `pitch` are `0.0`, then the camera will be looking forward, i.e. in
    /// the `Vec3::NEG_Z` direction, with up being `Vec3::Y`.
    /// If set to `None`, it will be calculated from the camera's current position during
    /// initialization.
    /// You should not update this after initialization - use `target_yaw` instead.
    /// Defaults to `None`.
    pub yaw:                 Option<f32>,
    /// Rotation in radians around the local X axis (latitudinal). Updated automatically.
    /// If both `yaw` and `pitch` are `0.0`, then the camera will be looking forward, i.e. in
    /// the `Vec3::NEG_Z` direction, with up being `Vec3::Y`.
    /// If set to `None`, it will be calculated from the camera's current position during
    /// initialization.
    /// You should not update this after initialization - use `target_pitch` instead.
    /// Defaults to `None`.
    pub pitch:               Option<f32>,
    /// The target focus point. The camera will smoothly transition to this value. Updated
    /// automatically, but you can also update it manually to control the camera independently of
    /// the mouse controls, e.g. with the keyboard.
    /// Defaults to `Vec3::ZERO`.
    pub target_focus:        Vec3,
    /// The target yaw value. The camera will smoothly transition to this value. Updated
    /// automatically, but you can also update it manually to control the camera independently of
    /// the mouse controls, e.g. with the keyboard.
    /// Defaults to `0.0`.
    pub target_yaw:          f32,
    /// The target pitch value. The camera will smoothly transition to this value Updated
    /// automatically, but you can also update it manually to control the camera independently of
    /// the mouse controls, e.g. with the keyboard.
    /// Defaults to `0.0`.
    pub target_pitch:        f32,
    /// The target radius value. The camera will smoothly transition to this value. Updated
    /// automatically, but you can also update it manually to control the camera independently of
    /// the mouse controls, e.g. with the keyboard.
    /// Defaults to `1.0`.
    pub target_radius:       f32,
    /// Upper limit on the `yaw` value, in radians. Use this to restrict the maximum rotation
    /// around the global Y axis.
    /// Defaults to `None`.
    pub yaw_upper_limit:     Option<f32>,
    /// Lower limit on the `yaw` value, in radians. Use this to restrict the maximum rotation
    /// around the global Y axis.
    /// Defaults to `None`.
    pub yaw_lower_limit:     Option<f32>,
    /// Upper limit on the `pitch` value, in radians. Use this to restrict the maximum rotation
    /// around the local X axis.
    /// Defaults to `None`.
    pub pitch_upper_limit:   Option<f32>,
    /// Lower limit on the `pitch` value, in radians. Use this to restrict the maximum rotation
    /// around the local X axis.
    /// Defaults to `None`.
    pub pitch_lower_limit:   Option<f32>,
    /// The origin for a shape to restrict the cameras `focus` position.
    /// Defaults to `Vec3::ZERO`.
    pub focus_bounds_origin: Vec3,
    /// The shape (`Sphere` or `Cuboid`) that the `focus` is restricted by. Centered on the
    /// `focus_bounds_origin`.
    /// Defaults to `None`.
    pub focus_bounds_shape:  Option<FocusBoundsShape>,
    /// Upper limit on the zoom. This applies to `radius`, in the case of using a perspective
    /// camera, or the projection's `scale` in the case of using an orthographic camera.
    /// Defaults to `None`.
    pub zoom_upper_limit:    Option<f32>,
    /// Lower limit on the zoom. This applies to `radius`, in the case of using a perspective
    /// camera, or the projection's `scale` in the case of using an orthographic camera.
    /// Should always be >0 otherwise you'll get stuck at 0.
    /// Defaults to `1e-7`.
    pub zoom_lower_limit:    f32,
    /// The sensitivity of the orbiting motion. A value of `0.0` disables orbiting.
    /// Defaults to `1.0`.
    pub orbit_sensitivity:   f32,
    /// How much smoothing is applied to the orbit motion. A value of `0.0` disables smoothing,
    /// so there's a 1:1 mapping of input to camera position. A value of `1.0` is infinite
    /// smoothing.
    /// Defaults to `0.8`.
    pub orbit_smoothness:    f32,
    /// The sensitivity of the panning motion. A value of `0.0` disables panning.
    /// Defaults to `1.0`.
    pub pan_sensitivity:     f32,
    /// How much smoothing is applied to the panning motion. A value of `0.0` disables smoothing,
    /// so there's a 1:1 mapping of input to camera position. A value of `1.0` is infinite
    /// smoothing.
    /// Defaults to `0.6`.
    pub pan_smoothness:      f32,
    /// The sensitivity of moving the camera closer or further way using the scroll wheel.
    /// A value of `0.0` disables zooming.
    /// Defaults to `1.0`.
    pub zoom_sensitivity:    f32,
    /// How much smoothing is applied to the zoom motion. A value of `0.0` disables smoothing,
    /// so there's a 1:1 mapping of input to camera position. A value of `1.0` is infinite
    /// smoothing.
    /// Defaults to `0.8`.
    /// Note that this setting does not apply to pixel-based scroll events, as they are typically
    /// already smooth. It only applies to line-based scroll events.
    pub zoom_smoothness:     f32,
    /// Button used to orbit the camera.
    /// Defaults to `Button::Left`.
    pub button_orbit:        MouseButton,
    /// Button used to pan the camera.
    /// Defaults to `Button::Right`.
    pub button_pan:          MouseButton,
    /// Button used to zoom the camera, by holding it down and moving the mouse forward and back.
    /// Defaults to `None`.
    pub button_zoom:         Option<MouseButton>,
    /// Which axis should zoom the camera when using `button_zoom`.
    /// Defaults to `ButtonZoomAxis::Y`.
    pub button_zoom_axis:    ButtonZoomAxis,
    /// Key that must be pressed for `button_orbit` to work.
    /// Defaults to `None` (no modifier).
    pub modifier_orbit:      Option<KeyCode>,
    /// Key that must be pressed for `button_pan` to work.
    /// Defaults to `None` (no modifier).
    pub modifier_pan:        Option<KeyCode>,
    /// Interactive input configuration.
    /// Set to `None` to disable all user input for this camera.
    /// Defaults to `Some(InputControl::default())`.
    pub input_control:       Option<InputControl>,
    /// Whether to allow the camera to go upside down.
    /// Defaults to `UpsideDownPolicy::Prevent`.
    pub upside_down_policy:  UpsideDownPolicy,
    /// Whether `OrbitCam` has been initialized with the initial config.
    /// Set to `InitializationState::Complete` if you want the camera to smoothly animate to its
    /// initial position.
    /// Defaults to `InitializationState::Pending`.
    pub initialization:      InitializationState,
    /// Whether to update the camera's transform regardless of whether there are any
    /// changes/input. Set to `ForceUpdate::Pending` if you want to modify values directly.
    /// This will be automatically set back to `ForceUpdate::Idle` after one frame.
    /// Defaults to `ForceUpdate::Idle`.
    pub force_update:        ForceUpdate,
    /// Axis order definition. This can be used to e.g. define a different default
    /// up direction. The default up is Y, but if you want the camera rotated.
    /// The axis can be switched.
    /// Defaults to `[Vec3::X, Vec3::Y, Vec3::Z]`.
    pub axis:                [Vec3; 3],
    /// Which time source drives camera smoothing.
    /// Defaults to `TimeSource::Virtual`.
    pub time_source:         TimeSource,
}

impl Default for OrbitCam {
    fn default() -> Self {
        Self {
            focus:               Vec3::ZERO,
            target_focus:        Vec3::ZERO,
            radius:              None,
            upside_down_policy:  UpsideDownPolicy::Prevent,
            orbit_sensitivity:   1.0,
            orbit_smoothness:    DEFAULT_ORBIT_SMOOTHNESS,
            pan_sensitivity:     1.0,
            pan_smoothness:      DEFAULT_PAN_SMOOTHNESS,
            zoom_sensitivity:    1.0,
            zoom_smoothness:     DEFAULT_ZOOM_SMOOTHNESS,
            button_orbit:        MouseButton::Left,
            button_pan:          MouseButton::Right,
            button_zoom:         None,
            button_zoom_axis:    ButtonZoomAxis::Y,
            modifier_orbit:      None,
            modifier_pan:        None,
            input_control:       Some(InputControl::default()),
            yaw:                 None,
            pitch:               None,
            target_yaw:          0.0,
            target_pitch:        0.0,
            target_radius:       DEFAULT_TARGET_RADIUS,
            initialization:      InitializationState::Pending,
            yaw_upper_limit:     None,
            yaw_lower_limit:     None,
            pitch_upper_limit:   None,
            pitch_lower_limit:   None,
            focus_bounds_origin: Vec3::ZERO,
            focus_bounds_shape:  None,
            zoom_upper_limit:    None,
            zoom_lower_limit:    DEFAULT_ZOOM_LOWER_LIMIT,
            force_update:        ForceUpdate::Idle,
            axis:                [Vec3::X, Vec3::Y, Vec3::Z],
            time_source:         TimeSource::Virtual,
        }
    }
}

impl OrbitCam {
    const fn clamp_yaw(&self, yaw: f32) -> f32 {
        traits::clamp_optional(yaw, self.yaw_lower_limit, self.yaw_upper_limit)
    }

    const fn clamp_pitch(&self, pitch: f32) -> f32 {
        traits::clamp_optional(pitch, self.pitch_lower_limit, self.pitch_upper_limit)
    }

    const fn clamp_zoom(&self, zoom: f32) -> f32 {
        traits::clamp_optional(zoom, Some(self.zoom_lower_limit), self.zoom_upper_limit)
    }

    fn clamp_focus(&self, focus: Vec3) -> Vec3 {
        let Some(shape) = self.focus_bounds_shape else {
            return focus;
        };
        let origin = self.focus_bounds_origin;
        match shape {
            FocusBoundsShape::Cuboid(shape) => shape.closest_point(focus - origin) + origin,
            FocusBoundsShape::Sphere(shape) => shape.closest_point(focus - origin) + origin,
        }
    }
}

// ============================================================================
// Viewport detection
// ============================================================================

/// Gather data about the active viewport, i.e. the viewport the user is interacting with.
/// Enables multiple viewports/windows.
pub(crate) fn active_viewport_data(
    mut active_cam: ResMut<ActiveCameraData>,
    mouse_input: Res<ButtonInput<MouseButton>>,
    key_input: Res<ButtonInput<KeyCode>>,
    pinch_events: MessageReader<PinchGesture>,
    scroll_events: MessageReader<MouseWheel>,
    touches: Res<Touches>,
    primary_windows: Query<&Window, With<PrimaryWindow>>,
    other_windows: Query<&Window, Without<PrimaryWindow>>,
    orbit_cameras: Query<(Entity, &Camera, &RenderTarget, &OrbitCam)>,
    #[cfg(feature = "bevy_egui")] egui_wants_focus: Res<EguiWantsFocus>,
    #[cfg(feature = "bevy_egui")] block_on_egui_query: Query<&BlockOnEguiFocus>,
) {
    let mut new_resource = ActiveCameraData::default();
    let mut max_camera_order = 0;

    let mut has_input = false;
    for (entity, camera, target, pan_orbit) in &orbit_cameras {
        let input_just_activated = input::orbit_just_pressed(pan_orbit, &mouse_input, &key_input)
            || input::pan_just_pressed(pan_orbit, &mouse_input, &key_input)
            || !pinch_events.is_empty()
            || !scroll_events.is_empty()
            || input::button_zoom_just_pressed(pan_orbit, &mouse_input)
            || (touches.iter_just_pressed().count() > 0
                && touches.iter_just_pressed().count() == touches.iter().count());

        if input_just_activated && pan_orbit.input_control.is_some() {
            has_input = true;
            let should_get_input = {
                #[cfg(feature = "bevy_egui")]
                {
                    if block_on_egui_query.contains(entity) {
                        !egui_wants_focus.prev && !egui_wants_focus.curr
                    } else {
                        true
                    }
                }
                #[cfg(not(feature = "bevy_egui"))]
                {
                    true
                }
            };
            if should_get_input && let RenderTarget::Window(win_ref) = target {
                let Some(window) = (match win_ref {
                    WindowRef::Primary => primary_windows.single().ok(),
                    WindowRef::Entity(entity) => other_windows.get(*entity).ok(),
                }) else {
                    // Window does not exist - maybe it was closed and the camera not cleaned up
                    continue;
                };

                // Is the cursor/touch in this window?
                // Note: there's a bug in winit that causes `window.cursor_position()` to
                // return a `Some` value even if the cursor is not in this window, in very
                // specific cases.
                // See: https://github.com/natepiano/bevy_lagrange/issues/22
                if let Some(input_position) = window.cursor_position().or_else(|| {
                    touches
                        .iter_just_pressed()
                        .collect::<Vec<_>>()
                        .first()
                        .map(|touch| touch.position())
                }) && let Some(Rect { min, max }) = camera.logical_viewport_rect()
                {
                    // Window coordinates have Y starting at the bottom, so we need to
                    // reverse the y component before comparing
                    // with the viewport rect
                    let cursor_in_vp = input_position.x > min.x
                        && input_position.x < max.x
                        && input_position.y > min.y
                        && input_position.y < max.y;

                    // Only set if camera order is higher. This may overwrite a previous
                    // value in the case the viewport is
                    // overlapping another viewport.
                    if cursor_in_vp && camera.order >= max_camera_order {
                        new_resource = ActiveCameraData {
                            entity:        Some(entity),
                            viewport_size: camera.logical_viewport_size(),
                            window_size:   Some(Vec2::new(window.width(), window.height())),
                            detection:     CameraInputDetection::Automatic,
                        };
                        max_camera_order = camera.order;
                    }
                }
            }
        }
    }

    if has_input {
        active_cam.set_if_neq(new_resource);
    }
}

// ============================================================================
// pan_orbit_camera helpers
// ============================================================================

/// Aggregated camera input for a single frame.
struct CameraInput {
    orbit:                Vec2,
    pan:                  Vec2,
    scroll_line:          f32,
    scroll_pixel:         f32,
    orbit_button_changed: bool,
}

/// Initializes `OrbitCam` from the camera's current transform, applying all limits.
fn initialize_orbit_cam(
    pan_orbit: &mut OrbitCam,
    transform: &mut Transform,
    projection: &mut Projection,
) {
    let (yaw, pitch, radius) = util::calculate_from_translation_and_focus(
        transform.translation,
        pan_orbit.focus,
        pan_orbit.axis,
    );
    let &mut mut yaw = pan_orbit.yaw.get_or_insert(yaw);
    let &mut mut pitch = pan_orbit.pitch.get_or_insert(pitch);
    let &mut mut radius = pan_orbit.radius.get_or_insert(radius);
    let mut focus = pan_orbit.focus;

    yaw = pan_orbit.clamp_yaw(yaw);
    pitch = pan_orbit.clamp_pitch(pitch);
    radius = pan_orbit.clamp_zoom(radius);
    focus = pan_orbit.clamp_focus(focus);

    pan_orbit.yaw = Some(yaw);
    pan_orbit.pitch = Some(pitch);
    pan_orbit.radius = Some(radius);
    pan_orbit.target_yaw = yaw;
    pan_orbit.target_pitch = pitch;
    pan_orbit.target_radius = radius;
    pan_orbit.target_focus = focus;

    util::update_orbit_transform(
        yaw,
        pitch,
        radius,
        focus,
        transform,
        projection,
        pan_orbit.axis,
    );

    pan_orbit.initialization = InitializationState::Complete;
}

/// Collects mouse, keyboard, and touch input into a single `CameraInput`.
fn collect_camera_input(
    entity: Entity,
    orbit_cam: &OrbitCam,
    active_cam: &ActiveCameraData,
    mouse_key_tracker: &MouseKeyTracker,
    touch_tracker: &TouchTracker,
) -> CameraInput {
    let mut orbit = Vec2::ZERO;
    let mut pan = Vec2::ZERO;
    let mut scroll_line = 0.0;
    let mut scroll_pixel = 0.0;
    let mut orbit_button_changed = false;

    // Only skip getting input if the camera is inactive/disabled — it might still
    // be lerping towards target values when the user is not actively controlling it.
    if let Some(input_control) = orbit_cam.input_control
        && active_cam.entity == Some(entity)
    {
        let zoom_sign = match input_control.zoom {
            ZoomDirection::Normal => 1.0,
            ZoomDirection::Reversed => -1.0,
        };

        orbit = mouse_key_tracker.orbit * orbit_cam.orbit_sensitivity;
        pan = mouse_key_tracker.pan * orbit_cam.pan_sensitivity;
        scroll_line = mouse_key_tracker.scroll_line * zoom_sign * orbit_cam.zoom_sensitivity;
        scroll_pixel = mouse_key_tracker.scroll_pixel * zoom_sign * orbit_cam.zoom_sensitivity;
        orbit_button_changed = mouse_key_tracker.orbit_button_changed;

        if let Some(touch_input) = input_control.touch {
            let (touch_orbit, touch_pan, touch_zoom_pixel) = match touch_input {
                TouchInput::OneFingerOrbit => match touch_tracker.get_touch_gestures() {
                    TouchGestures::None => (Vec2::ZERO, Vec2::ZERO, 0.0),
                    TouchGestures::OneFinger(one_finger_gestures) => {
                        (one_finger_gestures.motion, Vec2::ZERO, 0.0)
                    },
                    TouchGestures::TwoFinger(two_finger_gestures) => (
                        Vec2::ZERO,
                        two_finger_gestures.motion,
                        two_finger_gestures.pinch * TOUCH_PINCH_SCALE,
                    ),
                },
                TouchInput::TwoFingerOrbit => match touch_tracker.get_touch_gestures() {
                    TouchGestures::None => (Vec2::ZERO, Vec2::ZERO, 0.0),
                    TouchGestures::OneFinger(one_finger_gestures) => {
                        (Vec2::ZERO, one_finger_gestures.motion, 0.0)
                    },
                    TouchGestures::TwoFinger(two_finger_gestures) => (
                        two_finger_gestures.motion,
                        Vec2::ZERO,
                        two_finger_gestures.pinch * TOUCH_PINCH_SCALE,
                    ),
                },
            };

            orbit += touch_orbit * orbit_cam.orbit_sensitivity;
            pan += touch_pan * orbit_cam.pan_sensitivity;
            scroll_pixel += touch_zoom_pixel * zoom_sign * orbit_cam.zoom_sensitivity;
        }
    }

    CameraInput {
        orbit,
        pan,
        scroll_line,
        scroll_pixel,
        orbit_button_changed,
    }
}

/// Applies orbit input to target yaw/pitch. Returns `true` if the camera moved.
fn apply_orbit_input(
    orbit: Vec2,
    pan_orbit: &mut OrbitCam,
    drag_state: OrbitDragState,
    window_size: Option<Vec2>,
) -> bool {
    if orbit.length_squared() > 0.0 {
        // Use window size for rotation otherwise the sensitivity is far too high for small
        // viewports
        if let Some(win_size) = window_size {
            let delta_x = {
                let delta = orbit.x / win_size.x * TAU;
                match drag_state.orientation {
                    CameraOrientation::UpsideDown => -delta,
                    CameraOrientation::Normal => delta,
                }
            };
            let delta_y = orbit.y / win_size.y * PI;
            pan_orbit.target_yaw -= delta_x;
            pan_orbit.target_pitch += delta_y;
            return true;
        }
    }
    false
}

/// Applies pan input to target focus. Returns `true` if the camera moved.
fn apply_pan_input(
    mut pan: Vec2,
    pan_orbit: &mut OrbitCam,
    viewport_size: Option<Vec2>,
    transform: &Transform,
    projection: &Projection,
) -> bool {
    if pan.length_squared() > 0.0 {
        // Make panning distance independent of resolution and FOV
        if let Some(vp_size) = viewport_size {
            let mut multiplier = 1.0;
            match *projection {
                Projection::Perspective(ref p) => {
                    pan *= Vec2::new(p.fov * p.aspect_ratio, p.fov) / vp_size;
                    // Make panning proportional to distance away from focus point
                    if let Some(radius) = pan_orbit.radius {
                        multiplier = radius;
                    }
                },
                Projection::Orthographic(ref p) => {
                    pan *= Vec2::new(p.area.width(), p.area.height()) / vp_size;
                },
                Projection::Custom(_) => todo!(),
            }
            // Translate by local axes
            let right = transform.rotation * pan_orbit.axis[0] * -pan.x;
            let up = transform.rotation * pan_orbit.axis[1] * pan.y;
            let translation = (right + up) * multiplier;
            pan_orbit.target_focus += translation;
            return true;
        }
    }
    false
}

/// Applies scroll/zoom input to target radius. Returns `true` if the camera moved.
fn apply_scroll_input(scroll_line: f32, scroll_pixel: f32, pan_orbit: &mut OrbitCam) -> bool {
    if (scroll_line + scroll_pixel).abs() > 0.0 {
        let line_delta = -scroll_line * pan_orbit.target_radius * SCROLL_ZOOM_FACTOR;
        let pixel_delta = -scroll_pixel * pan_orbit.target_radius * SCROLL_ZOOM_FACTOR;

        pan_orbit.target_radius += line_delta + pixel_delta;

        // Pixel-based scrolling is added directly to the current value (already smooth)
        pan_orbit.radius = pan_orbit
            .radius
            .map(|value| pan_orbit.clamp_zoom(value + pixel_delta));

        return true;
    }
    false
}

/// Interpolates current values toward targets and updates the camera transform.
fn smooth_and_update_transform(
    pan_orbit: &mut OrbitCam,
    transform: &mut Transform,
    projection: &mut Projection,
    delta: f32,
) {
    let (Some(yaw), Some(pitch), Some(radius)) = (pan_orbit.yaw, pan_orbit.pitch, pan_orbit.radius)
    else {
        return;
    };

    let new_yaw =
        util::lerp_and_snap_f32(yaw, pan_orbit.target_yaw, pan_orbit.orbit_smoothness, delta);
    let new_pitch = util::lerp_and_snap_f32(
        pitch,
        pan_orbit.target_pitch,
        pan_orbit.orbit_smoothness,
        delta,
    );
    let new_radius = util::lerp_and_snap_f32(
        radius,
        pan_orbit.target_radius,
        pan_orbit.zoom_smoothness,
        delta,
    );
    let new_focus = util::lerp_and_snap_position(
        pan_orbit.focus,
        pan_orbit.target_focus,
        pan_orbit.pan_smoothness,
        delta,
    );

    util::update_orbit_transform(
        new_yaw,
        new_pitch,
        new_radius,
        new_focus,
        transform,
        projection,
        pan_orbit.axis,
    );

    pan_orbit.yaw = Some(new_yaw);
    pan_orbit.pitch = Some(new_pitch);
    pan_orbit.radius = Some(new_radius);
    pan_orbit.focus = *new_focus;
    pan_orbit.force_update = ForceUpdate::Idle;
}

// ============================================================================
// Main camera system
// ============================================================================

/// Main system for processing input and converting to transformations
pub(crate) fn orbit_cam(
    active_cam: Res<ActiveCameraData>,
    mouse_key_tracker: Res<MouseKeyTracker>,
    touch_tracker: Res<TouchTracker>,
    mut orbit_cameras: Query<(
        Entity,
        &mut OrbitCam,
        &mut OrbitDragState,
        &mut Transform,
        &mut Projection,
    )>,
    time_real: Res<Time<Real>>,
    time_virt: Res<Time<Virtual>>,
) {
    for (entity, mut pan_orbit, mut drag_state, mut transform, mut projection) in &mut orbit_cameras
    {
        if pan_orbit.initialization == InitializationState::Pending {
            initialize_orbit_cam(&mut pan_orbit, &mut transform, &mut projection);
        }

        let input = collect_camera_input(
            entity,
            &pan_orbit,
            &active_cam,
            &mouse_key_tracker,
            &touch_tracker,
        );

        // Only check for upside down when orbiting started or ended this frame,
        // so we don't reverse the yaw direction while the user is still dragging
        if input.orbit_button_changed {
            let world_up = pan_orbit.axis[1];
            drag_state.orientation = if transform.up().dot(world_up) < 0.0 {
                CameraOrientation::UpsideDown
            } else {
                CameraOrientation::Normal
            };
        }

        let mut has_moved = apply_orbit_input(
            input.orbit,
            &mut pan_orbit,
            *drag_state,
            active_cam.window_size,
        );
        has_moved |= apply_pan_input(
            input.pan,
            &mut pan_orbit,
            active_cam.viewport_size,
            &transform,
            &projection,
        );
        has_moved |= apply_scroll_input(input.scroll_line, input.scroll_pixel, &mut pan_orbit);

        // Apply constraints
        pan_orbit.target_yaw = pan_orbit.clamp_yaw(pan_orbit.target_yaw);
        pan_orbit.target_pitch = pan_orbit.clamp_pitch(pan_orbit.target_pitch);
        pan_orbit.target_radius = pan_orbit.clamp_zoom(pan_orbit.target_radius);
        pan_orbit.target_focus = pan_orbit.clamp_focus(pan_orbit.target_focus);
        if pan_orbit.upside_down_policy == UpsideDownPolicy::Prevent {
            pan_orbit.target_pitch = pan_orbit.target_pitch.clamp(-PI / 2.0, PI / 2.0);
        }

        let delta = match pan_orbit.time_source {
            TimeSource::Real => time_real.delta_secs(),
            TimeSource::Virtual => time_virt.delta_secs(),
        };

        // Only pass `&mut transform` when something actually changed.
        // Passing it unconditionally triggers Bevy's `DerefMut` change detection,
        // marking `Transform` (and therefore `GlobalTransform`) as changed every
        // frame — even when the camera is idle.
        let (Some(yaw), Some(pitch), Some(radius)) =
            (pan_orbit.yaw, pan_orbit.pitch, pan_orbit.radius)
        else {
            continue;
        };

        #[allow(
            clippy::float_cmp,
            reason = "lerp_and_snap produces bitwise-identical values on convergence"
        )]
        let needs_update = has_moved
            || pan_orbit.force_update != ForceUpdate::Idle
            || pan_orbit.target_yaw != yaw
            || pan_orbit.target_pitch != pitch
            || pan_orbit.target_radius != radius
            || pan_orbit.target_focus != pan_orbit.focus;

        if needs_update {
            smooth_and_update_transform(&mut pan_orbit, &mut transform, &mut projection, delta);
        }
    }
}
