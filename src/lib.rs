#![warn(missing_docs)]
#![allow(clippy::used_underscore_binding)]
#![doc = include_str!("../README.md")]

use std::f32::consts::PI;

use bevy::camera::CameraUpdateSystems;
use bevy::camera::RenderTarget;
use bevy::input::gestures::PinchGesture;
use bevy::input::mouse::MouseWheel;
use bevy::prelude::*;
use bevy::transform::TransformSystems;
use bevy::window::PrimaryWindow;
use bevy::window::WindowRef;
#[cfg(feature = "bevy_egui")]
use bevy_egui::EguiPreUpdateSet;
use touch::touch_tracker;

#[cfg(feature = "bevy_egui")]
pub use crate::egui::BlockOnEguiFocus;
#[cfg(feature = "bevy_egui")]
pub use crate::egui::EguiFocusIncludesHover;
#[cfg(feature = "bevy_egui")]
pub use crate::egui::EguiWantsFocus;
use crate::input::button_zoom_just_pressed;
use crate::input::mouse_key_tracker;
use crate::input::MouseKeyTracker;
pub use crate::touch::TouchControls;
use crate::touch::TouchGestures;
use crate::touch::TouchTracker;
use crate::traits::OptionalClamp;

#[cfg(feature = "fit_overlay")]
mod animation;
#[cfg(feature = "fit_overlay")]
mod components;
#[cfg(feature = "bevy_egui")]
mod egui;
#[cfg(feature = "fit_overlay")]
mod events;
#[cfg(feature = "fit_overlay")]
mod fit;
#[cfg(feature = "fit_overlay")]
mod fit_overlay;
mod input;
#[cfg(feature = "fit_overlay")]
mod observers;
#[cfg(feature = "fit_overlay")]
mod support;
mod touch;
mod traits;
mod util;

#[cfg(feature = "fit_overlay")]
pub use animation::CameraMove;
#[cfg(feature = "fit_overlay")]
pub use animation::CameraMoveList;
#[cfg(feature = "fit_overlay")]
pub use components::AnimationConflictPolicy;
#[cfg(feature = "fit_overlay")]
pub use components::CameraInputInterruptBehavior;
#[cfg(feature = "fit_overlay")]
pub use components::CurrentFitTarget;
#[cfg(feature = "fit_overlay")]
pub use components::FitOverlay;
#[cfg(feature = "fit_overlay")]
pub use events::AnimateToFit;
#[cfg(feature = "fit_overlay")]
pub use events::AnimationBegin;
#[cfg(feature = "fit_overlay")]
pub use events::AnimationCancelled;
#[cfg(feature = "fit_overlay")]
pub use events::AnimationEnd;
#[cfg(feature = "fit_overlay")]
pub use events::AnimationRejected;
#[cfg(feature = "fit_overlay")]
pub use events::AnimationSource;
#[cfg(feature = "fit_overlay")]
pub use events::CameraMoveBegin;
#[cfg(feature = "fit_overlay")]
pub use events::CameraMoveEnd;
#[cfg(feature = "fit_overlay")]
pub use events::LookAt;
#[cfg(feature = "fit_overlay")]
pub use events::LookAtAndZoomToFit;
#[cfg(feature = "fit_overlay")]
pub use events::PlayAnimation;
#[cfg(feature = "fit_overlay")]
pub use events::SetFitTarget;
#[cfg(feature = "fit_overlay")]
pub use events::ZoomBegin;
#[cfg(feature = "fit_overlay")]
pub use events::ZoomCancelled;
#[cfg(feature = "fit_overlay")]
pub use events::ZoomContext;
#[cfg(feature = "fit_overlay")]
pub use events::ZoomEnd;
#[cfg(feature = "fit_overlay")]
pub use events::ZoomToFit;
#[cfg(feature = "fit_overlay")]
pub use fit_overlay::FitTargetOverlayConfig;

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
                        active_viewport_data
                            .run_if(|active_cam: Res<ActiveCameraData>| !active_cam.manual),
                        mouse_key_tracker,
                        touch_tracker,
                    ),
                    orbit_cam,
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

        #[cfg(feature = "fit_overlay")]
        {
            app.add_observer(observers::on_camera_move_list_added)
                .add_observer(observers::restore_camera_state)
                .add_observer(observers::on_zoom_to_fit)
                .add_observer(observers::on_play_animation)
                .add_observer(observers::on_set_fit_target)
                .add_observer(observers::on_animate_to_fit)
                .add_observer(observers::on_look_at)
                .add_observer(observers::on_look_at_and_zoom_to_fit)
                .add_systems(Update, animation::process_camera_move_list);
            app.add_plugins(fit_overlay::ZoomOverlayPlugin);
        }
    }
}

/// Base system set to allow ordering of `OrbitCam`
#[derive(SystemSet, Debug, Hash, PartialEq, Eq, Clone)]
pub struct OrbitCamSystemSet;

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
#[allow(clippy::struct_excessive_bools)]
#[require(Camera3d)]
pub struct OrbitCam {
    /// The point to orbit around, and what the camera looks at. Updated automatically.
    /// If you want to change the focus programmatically after initialization, set `target_focus`
    /// instead.
    /// Defaults to `Vec3::ZERO`.
    pub focus:                          Vec3,
    /// The radius of the orbit, or the distance from the `focus` point.
    /// For orthographic projection, this is ignored, and the projection's `scale` is used instead.
    /// If set to `None`, it will be calculated from the camera's current position during
    /// initialization.
    /// Automatically updated.
    /// Defaults to `None`.
    pub radius:                         Option<f32>,
    /// Rotation in radians around the global Y axis (longitudinal). Updated automatically.
    /// If both `yaw` and `pitch` are `0.0`, then the camera will be looking forward, i.e. in
    /// the `Vec3::NEG_Z` direction, with up being `Vec3::Y`.
    /// If set to `None`, it will be calculated from the camera's current position during
    /// initialization.
    /// You should not update this after initialization - use `target_yaw` instead.
    /// Defaults to `None`.
    pub yaw:                            Option<f32>,
    /// Rotation in radians around the local X axis (latitudinal). Updated automatically.
    /// If both `yaw` and `pitch` are `0.0`, then the camera will be looking forward, i.e. in
    /// the `Vec3::NEG_Z` direction, with up being `Vec3::Y`.
    /// If set to `None`, it will be calculated from the camera's current position during
    /// initialization.
    /// You should not update this after initialization - use `target_pitch` instead.
    /// Defaults to `None`.
    pub pitch:                          Option<f32>,
    /// The target focus point. The camera will smoothly transition to this value. Updated
    /// automatically, but you can also update it manually to control the camera independently of
    /// the mouse controls, e.g. with the keyboard.
    /// Defaults to `Vec3::ZERO`.
    pub target_focus:                   Vec3,
    /// The target yaw value. The camera will smoothly transition to this value. Updated
    /// automatically, but you can also update it manually to control the camera independently of
    /// the mouse controls, e.g. with the keyboard.
    /// Defaults to `0.0`.
    pub target_yaw:                     f32,
    /// The target pitch value. The camera will smoothly transition to this value Updated
    /// automatically, but you can also update it manually to control the camera independently of
    /// the mouse controls, e.g. with the keyboard.
    /// Defaults to `0.0`.
    pub target_pitch:                   f32,
    /// The target radius value. The camera will smoothly transition to this value. Updated
    /// automatically, but you can also update it manually to control the camera independently of
    /// the mouse controls, e.g. with the keyboard.
    /// Defaults to `1.0`.
    pub target_radius:                  f32,
    /// Upper limit on the `yaw` value, in radians. Use this to restrict the maximum rotation
    /// around the global Y axis.
    /// Defaults to `None`.
    pub yaw_upper_limit:                Option<f32>,
    /// Lower limit on the `yaw` value, in radians. Use this to restrict the maximum rotation
    /// around the global Y axis.
    /// Defaults to `None`.
    pub yaw_lower_limit:                Option<f32>,
    /// Upper limit on the `pitch` value, in radians. Use this to restrict the maximum rotation
    /// around the local X axis.
    /// Defaults to `None`.
    pub pitch_upper_limit:              Option<f32>,
    /// Lower limit on the `pitch` value, in radians. Use this to restrict the maximum rotation
    /// around the local X axis.
    /// Defaults to `None`.
    pub pitch_lower_limit:              Option<f32>,
    /// The origin for a shape to restrict the cameras `focus` position.
    /// Defaults to `Vec3::ZERO`.
    pub focus_bounds_origin:            Vec3,
    /// The shape (Sphere or Cuboid) that the `focus` is restricted by. Centered on the
    /// `focus_bounds_origin`.
    /// Defaults to `None`.
    pub focus_bounds_shape:             Option<FocusBoundsShape>,
    /// Upper limit on the zoom. This applies to `radius`, in the case of using a perspective
    /// camera, or the projection's scale in the case of using an orthographic camera.
    /// Defaults to `None`.
    pub zoom_upper_limit:               Option<f32>,
    /// Lower limit on the zoom. This applies to `radius`, in the case of using a perspective
    /// camera, or the projection's scale in the case of using an orthographic camera.
    /// Should always be >0 otherwise you'll get stuck at 0.
    /// Defaults to `0.05`.
    pub zoom_lower_limit:               f32,
    /// The sensitivity of the orbiting motion. A value of `0.0` disables orbiting.
    /// Defaults to `1.0`.
    pub orbit_sensitivity:              f32,
    /// How much smoothing is applied to the orbit motion. A value of `0.0` disables smoothing,
    /// so there's a 1:1 mapping of input to camera position. A value of `1.0` is infinite
    /// smoothing.
    /// Defaults to `0.8`.
    pub orbit_smoothness:               f32,
    /// The sensitivity of the panning motion. A value of `0.0` disables panning.
    /// Defaults to `1.0`.
    pub pan_sensitivity:                f32,
    /// How much smoothing is applied to the panning motion. A value of `0.0` disables smoothing,
    /// so there's a 1:1 mapping of input to camera position. A value of `1.0` is infinite
    /// smoothing.
    /// Defaults to `0.6`.
    pub pan_smoothness:                 f32,
    /// The sensitivity of moving the camera closer or further way using the scroll wheel.
    /// A value of `0.0` disables zooming.
    /// Defaults to `1.0`.
    pub zoom_sensitivity:               f32,
    /// How much smoothing is applied to the zoom motion. A value of `0.0` disables smoothing,
    /// so there's a 1:1 mapping of input to camera position. A value of `1.0` is infinite
    /// smoothing.
    /// Defaults to `0.8`.
    /// Note that this setting does not apply to pixel-based scroll events, as they are typically
    /// already smooth. It only applies to line-based scroll events.
    pub zoom_smoothness:                f32,
    /// Button used to orbit the camera.
    /// Defaults to `Button::Left`.
    pub button_orbit:                   MouseButton,
    /// Button used to pan the camera.
    /// Defaults to `Button::Right`.
    pub button_pan:                     MouseButton,
    /// Button used to zoom the camera, by holding it down and moving the mouse forward and back.
    /// Defaults to `None`.
    pub button_zoom:                    Option<MouseButton>,
    /// Which axis should zoom the camera when using `button_zoom`.
    /// Defaults to `ButtonZoomAxis::Y`.
    pub button_zoom_axis:               ButtonZoomAxis,
    /// Key that must be pressed for `button_orbit` to work.
    /// Defaults to `None` (no modifier).
    pub modifier_orbit:                 Option<KeyCode>,
    /// Key that must be pressed for `button_pan` to work.
    /// Defaults to `None` (no modifier).
    pub modifier_pan:                   Option<KeyCode>,
    /// Whether touch controls are enabled.
    /// Defaults to `true`.
    pub touch_enabled:                  bool,
    /// The control scheme for touch inputs.
    /// Defaults to `TouchControls::OneFingerOrbit`.
    pub touch_controls:                 TouchControls,
    /// The behavior for trackpad inputs.
    /// Defaults to `TrackpadBehavior::DefaultZoom`.
    /// To enable orbit behavior similar to Blender, change this to
    /// `TrackpadBehavior::BlenderLike`. For `BlenderLike` panning, add `ShiftLeft` to the
    /// `modifier_pan` field. For `BlenderLike` zooming, add `ControlLeft` in `modifier_zoom`
    /// field.
    pub trackpad_behavior:              TrackpadBehavior,
    /// Whether to enable pinch-to-zoom functionality on trackpads.
    /// Defaults to `false`.
    pub trackpad_pinch_to_zoom_enabled: bool,
    /// The sensitivity of trackpad gestures when using `BlenderLike` behavior. A value of `0.0`
    /// effectively disables trackpad orbit/pan functionality. This applies to both orbit and pan.
    /// operations when using a trackpad with the `BlenderLike` behavior mode.
    /// Defaults to `1.0`.
    pub trackpad_sensitivity:           f32,
    /// Whether to reverse the zoom direction. This applies to the button-based zoom `button_zoom`
    /// as well. If you want button zoom to remain the same, set `button_zoom_reverse` to `true`.
    /// Defaults to `false`.
    pub reversed_zoom:                  bool,
    /// Whether the zoom direction when using `button_zoom` is reversed.
    /// Defaults to `false`.
    pub reversed_button_zoom:           bool,
    /// Whether the camera is currently upside down. Updated automatically.
    /// This is used to determine which way to orbit, because it's more intuitive to reverse the
    /// orbit direction when upside down.
    /// Should not be set manually unless you know what you're doing.
    /// Defaults to `false` (but will be updated immediately).
    pub is_upside_down:                 bool,
    /// Whether to allow the camera to go upside down.
    /// Defaults to `false`.
    pub allow_upside_down:              bool,
    /// If `false`, disable control of the camera. Defaults to `true`.
    pub enabled:                        bool,
    /// Whether `OrbitCam` has been initialized with the initial config.
    /// Set to `true` if you want the camera to smoothly animate to its initial position.
    /// Defaults to `false`.
    pub initialized:                    bool,
    /// Whether to update the camera's transform regardless of whether there are any changes/input.
    /// Set this to `true` if you want to modify values directly.
    /// This will be automatically set back to `false` after one frame.
    /// Defaults to `false`.
    pub force_update:                   bool,
    /// Axis order definition. This can be used to e.g. define a different default
    /// up direction. The default up is Y, but if you want the camera rotated.
    /// The axis can be switched.
    /// Defaults to `[Vec3::X, Vec3::Y, Vec3::Z]`.
    pub axis:                           [Vec3; 3],
    /// Use real time instead of virtual time. Set this to `true` if you want to pause virtual
    /// time without affecting the camera, for example in a game.
    /// Defaults to `false`.
    pub use_real_time:                  bool,
}

impl Default for OrbitCam {
    fn default() -> Self {
        Self {
            focus:                          Vec3::ZERO,
            target_focus:                   Vec3::ZERO,
            radius:                         None,
            is_upside_down:                 false,
            allow_upside_down:              false,
            orbit_sensitivity:              1.0,
            orbit_smoothness:               0.1,
            pan_sensitivity:                1.0,
            pan_smoothness:                 0.02,
            zoom_sensitivity:               1.0,
            zoom_smoothness:                0.1,
            button_orbit:                   MouseButton::Left,
            button_pan:                     MouseButton::Right,
            button_zoom:                    None,
            button_zoom_axis:               ButtonZoomAxis::Y,
            reversed_button_zoom:           false,
            modifier_orbit:                 None,
            modifier_pan:                   None,
            touch_enabled:                  true,
            touch_controls:                 TouchControls::OneFingerOrbit,
            trackpad_behavior:              TrackpadBehavior::Default,
            trackpad_pinch_to_zoom_enabled: false,
            trackpad_sensitivity:           1.0,
            reversed_zoom:                  false,
            enabled:                        true,
            yaw:                            None,
            pitch:                          None,
            target_yaw:                     0.0,
            target_pitch:                   0.0,
            target_radius:                  1.0,
            initialized:                    false,
            yaw_upper_limit:                None,
            yaw_lower_limit:                None,
            pitch_upper_limit:              None,
            pitch_lower_limit:              None,
            focus_bounds_origin:            Vec3::ZERO,
            focus_bounds_shape:             None,
            zoom_upper_limit:               None,
            zoom_lower_limit:               0.05,
            force_update:                   false,
            axis:                           [Vec3::X, Vec3::Y, Vec3::Z],
            use_real_time:                  false,
        }
    }
}

impl OrbitCam {
    fn clamp_yaw(&self, yaw: f32) -> f32 {
        yaw.clamp_optional(self.yaw_lower_limit, self.yaw_upper_limit)
    }

    fn clamp_pitch(&self, pitch: f32) -> f32 {
        pitch.clamp_optional(self.pitch_lower_limit, self.pitch_upper_limit)
    }

    fn clamp_zoom(&self, zoom: f32) -> f32 {
        zoom.clamp_optional(Some(self.zoom_lower_limit), self.zoom_upper_limit)
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

/// Tracks which `OrbitCam` is active (should handle input events).
///
/// Also stores the window and viewport dimensions, which are used for scaling mouse motion.
/// `LagrangePlugin` manages this resource automatically, in order to support multiple
/// viewports/windows. However, if this doesn't work for you, you can take over and manage it
/// yourself, e.g. when you want to control a camera that is rendering to a texture.
#[derive(Resource, Default, Debug, PartialEq)]
pub struct ActiveCameraData {
    /// ID of the entity with `OrbitCam` that will handle user input. In other words, this
    /// is the camera that will move when you orbit/pan/zoom.
    pub entity:        Option<Entity>,
    /// The viewport size. This is only used to scale the panning mouse motion. I recommend setting
    /// this to the actual render target dimensions (e.g. the image or viewport), and changing
    /// `OrbitCam::pan_sensitivity` to adjust the sensitivity if required.
    pub viewport_size: Option<Vec2>,
    /// The size of the window. This is only used to scale the orbit mouse motion. I recommend
    /// setting this to actual dimensions of the window that you want to control the camera from,
    /// and changing `OrbitCam::orbit_sensitivity` to adjust the sensitivity if required.
    pub window_size:   Option<Vec2>,
    /// Indicates to `LagrangePlugin` that it should not update/overwrite this resource.
    /// If you are manually updating this resource you should set this to `true`.
    /// Note that setting this to `true` will effectively break multiple viewport/window support
    /// unless you manually reimplement it.
    pub manual:        bool,
}

/// The shape to restrict the camera's focus inside.
#[derive(Clone, PartialEq, Debug, Reflect, Copy)]
pub enum FocusBoundsShape {
    /// Limit the camera's focus to a sphere centered on `focus_bounds_origin`.
    Sphere(Sphere),
    /// Limit the camera's focus to a cuboid centered on `focus_bounds_origin`.
    Cuboid(Cuboid),
}

/// The shape to restrict the camera's focus inside.
#[derive(Clone, PartialEq, Eq, Debug, Reflect, Copy)]
pub enum ButtonZoomAxis {
    /// Zoom by moving the mouse along the x-axis.
    X,
    /// Zoom by moving the mouse along the y-axis.
    Y,
    /// Zoom by moving the mouse along either the x-axis or the y-axis.
    XY,
}

impl From<Sphere> for FocusBoundsShape {
    fn from(value: Sphere) -> Self { Self::Sphere(value) }
}

impl From<Cuboid> for FocusBoundsShape {
    fn from(value: Cuboid) -> Self { Self::Cuboid(value) }
}

/// Allows for changing the `TrackpadBehavior` from default to the way it works in Blender.
///
/// In Blender the trackpad orbits when scrolling. If you hold down the `ShiftLeft`, it Pans and
/// holding down `ControlLeft` will Zoom.
#[derive(Clone, PartialEq, Eq, Debug, Reflect, Copy)]
pub enum TrackpadBehavior {
    /// Default touchpad behavior. I.e., no special gesture support, scrolling on the touchpad
    /// (vertically) will zoom, as it does with a mouse.
    Default,
    /// Blender-like touchpad behavior. Scrolling on the touchpad will orbit, and you can pinch to
    /// zoom. Optionally you can pan, or switch scroll to zoom, by holding down a modifier.
    BlenderLike {
        /// Modifier key that enables panning while scrolling
        modifier_pan: Option<KeyCode>,

        /// Modifier key that enables panning while scrolling
        modifier_zoom: Option<KeyCode>,
    },
}

impl TrackpadBehavior {
    /// Creates a `BlenderLike` variant with default modifiers (Shift for pan, Ctrl for zoom)
    #[must_use]
    pub const fn blender_default() -> Self {
        Self::BlenderLike {
            modifier_pan:  Some(KeyCode::ShiftLeft),
            modifier_zoom: Some(KeyCode::ControlLeft),
        }
    }
}

/// Gather data about the active viewport, i.e. the viewport the user is interacting with.
/// Enables multiple viewports/windows.
fn active_viewport_data(
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
    #[cfg(feature = "bevy_egui")] block_on_egui_query: Query<&crate::egui::BlockOnEguiFocus>,
) {
    let mut new_resource = ActiveCameraData::default();
    let mut max_cam_order = 0;

    let mut has_input = false;
    for (entity, camera, target, pan_orbit) in &orbit_cameras {
        let input_just_activated = input::orbit_just_pressed(pan_orbit, &mouse_input, &key_input)
            || input::pan_just_pressed(pan_orbit, &mouse_input, &key_input)
            || !pinch_events.is_empty()
            || !scroll_events.is_empty()
            || button_zoom_just_pressed(pan_orbit, &mouse_input)
            || (touches.iter_just_pressed().count() > 0
                && touches.iter_just_pressed().count() == touches.iter().count());

        if input_just_activated {
            has_input = true;
            #[allow(unused_mut, unused_assignments)]
            let mut should_get_input = true;
            #[cfg(feature = "bevy_egui")]
            {
                if block_on_egui_query.contains(entity) {
                    should_get_input = !egui_wants_focus.prev && !egui_wants_focus.curr;
                }
            }
            if should_get_input {
                // First check if cursor is in the same window as this camera
                if let RenderTarget::Window(win_ref) = target {
                    let Some(window) = (match win_ref {
                        WindowRef::Primary => primary_windows.single().ok(),
                        WindowRef::Entity(entity) => other_windows.get(*entity).ok(),
                    }) else {
                        // Window does not exist - maybe it was closed and the camera not cleaned up
                        continue;
                    };

                    // Is the cursor/touch in this window?
                    // Note: there's a bug in winit that causes `window.cursor_position()` to return
                    // a `Some` value even if the cursor is not in this window, in very specific
                    // cases. See: https://github.com/natepiano/bevy_lagrange/issues/22
                    if let Some(input_position) = window.cursor_position().or_else(|| {
                        touches
                            .iter_just_pressed()
                            .collect::<Vec<_>>()
                            .first()
                            .map(|touch| touch.position())
                    }) {
                        // Now check if cursor is within this camera's viewport
                        if let Some(Rect { min, max }) = camera.logical_viewport_rect() {
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
                            if cursor_in_vp && camera.order >= max_cam_order {
                                new_resource = ActiveCameraData {
                                    entity:        Some(entity),
                                    viewport_size: camera.logical_viewport_size(),
                                    window_size:   Some(Vec2::new(window.width(), window.height())),
                                    manual:        false,
                                };
                                max_cam_order = camera.order;
                            }
                        }
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

    pan_orbit.initialized = true;
}

/// Collects mouse, keyboard, and touch input into a single `CameraInput`.
fn collect_camera_input(
    entity: Entity,
    pan_orbit: &OrbitCam,
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
    if pan_orbit.enabled && active_cam.entity == Some(entity) {
        let zoom_direction = if pan_orbit.reversed_zoom { -1.0 } else { 1.0 };

        orbit = mouse_key_tracker.orbit * pan_orbit.orbit_sensitivity;
        pan = mouse_key_tracker.pan * pan_orbit.pan_sensitivity;
        scroll_line = mouse_key_tracker.scroll_line * zoom_direction * pan_orbit.zoom_sensitivity;
        scroll_pixel = mouse_key_tracker.scroll_pixel * zoom_direction * pan_orbit.zoom_sensitivity;
        orbit_button_changed = mouse_key_tracker.orbit_button_changed;

        if pan_orbit.touch_enabled {
            let (touch_orbit, touch_pan, touch_zoom_pixel) = match pan_orbit.touch_controls {
                TouchControls::OneFingerOrbit => match touch_tracker.get_touch_gestures() {
                    TouchGestures::None => (Vec2::ZERO, Vec2::ZERO, 0.0),
                    TouchGestures::OneFinger(one_finger_gestures) => {
                        (one_finger_gestures.motion, Vec2::ZERO, 0.0)
                    },
                    TouchGestures::TwoFinger(two_finger_gestures) => (
                        Vec2::ZERO,
                        two_finger_gestures.motion,
                        two_finger_gestures.pinch * 0.015,
                    ),
                },
                TouchControls::TwoFingerOrbit => match touch_tracker.get_touch_gestures() {
                    TouchGestures::None => (Vec2::ZERO, Vec2::ZERO, 0.0),
                    TouchGestures::OneFinger(one_finger_gestures) => {
                        (Vec2::ZERO, one_finger_gestures.motion, 0.0)
                    },
                    TouchGestures::TwoFinger(two_finger_gestures) => (
                        two_finger_gestures.motion,
                        Vec2::ZERO,
                        two_finger_gestures.pinch * 0.015,
                    ),
                },
            };

            orbit += touch_orbit * pan_orbit.orbit_sensitivity;
            pan += touch_pan * pan_orbit.pan_sensitivity;
            scroll_pixel += touch_zoom_pixel * zoom_direction * pan_orbit.zoom_sensitivity;
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
fn apply_orbit_input(orbit: Vec2, pan_orbit: &mut OrbitCam, window_size: Option<Vec2>) -> bool {
    if orbit.length_squared() > 0.0 {
        // Use window size for rotation otherwise the sensitivity is far too high for small
        // viewports
        if let Some(win_size) = window_size {
            let delta_x = {
                let delta = orbit.x / win_size.x * PI * 2.0;
                if pan_orbit.is_upside_down {
                    -delta
                } else {
                    delta
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
        let line_delta = -scroll_line * pan_orbit.target_radius * 0.2;
        let pixel_delta = -scroll_pixel * pan_orbit.target_radius * 0.2;

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
    has_moved: bool,
) {
    let (Some(yaw), Some(pitch), Some(radius)) = (pan_orbit.yaw, pan_orbit.pitch, pan_orbit.radius)
    else {
        return;
    };

    #[allow(clippy::float_cmp)]
    if !has_moved
        // For smoothed values, we must check whether current value is different from target
        // value. If we only checked whether the values were non-zero this frame, then
        // the camera would instantly stop moving as soon as you stopped moving it, instead
        // of smoothly stopping
        && pan_orbit.target_yaw == yaw
        && pan_orbit.target_pitch == pitch
        && pan_orbit.target_radius == radius
        && pan_orbit.target_focus == pan_orbit.focus
        && !pan_orbit.force_update
    {
        return;
    }

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
    let new_focus = util::lerp_and_snap_vec3(
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
    pan_orbit.focus = new_focus;
    pan_orbit.force_update = false;
}

// ============================================================================
// Main camera system
// ============================================================================

/// Main system for processing input and converting to transformations
fn orbit_cam(
    active_cam: Res<ActiveCameraData>,
    mouse_key_tracker: Res<MouseKeyTracker>,
    touch_tracker: Res<TouchTracker>,
    mut orbit_cameras: Query<(Entity, &mut OrbitCam, &mut Transform, &mut Projection)>,
    time_real: Res<Time<Real>>,
    time_virt: Res<Time<Virtual>>,
) {
    for (entity, mut pan_orbit, mut transform, mut projection) in &mut orbit_cameras {
        if !pan_orbit.initialized {
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
            pan_orbit.is_upside_down = transform.up().dot(world_up) < 0.0;
        }

        let mut has_moved = apply_orbit_input(input.orbit, &mut pan_orbit, active_cam.window_size);
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
        if !pan_orbit.allow_upside_down {
            pan_orbit.target_pitch = pan_orbit.target_pitch.clamp(-PI / 2.0, PI / 2.0);
        }

        let delta = if pan_orbit.use_real_time {
            time_real.delta_secs()
        } else {
            time_virt.delta_secs()
        };

        smooth_and_update_transform(
            &mut pan_orbit,
            &mut transform,
            &mut projection,
            delta,
            has_moved,
        );
    }
}
