//! `OrbitCam` component, systems, and helpers.

mod controller;

use bevy::prelude::*;
pub(crate) use controller::orbit_cam;

use super::constants::DEFAULT_INPUT_SENSITIVITY;
use super::constants::DEFAULT_ORBIT_ANGLE;
use super::constants::DEFAULT_ORBIT_SMOOTHNESS;
use super::constants::DEFAULT_PAN_SMOOTHNESS;
use super::constants::DEFAULT_TARGET_RADIUS;
use super::constants::DEFAULT_ZOOM_LOWER_LIMIT;
use super::constants::DEFAULT_ZOOM_SMOOTHNESS;
use super::input::OrbitCamInput;
use super::input::OrbitCamInputContext;
use super::input::OrbitCamPreset;

/// Base system set to allow ordering of `OrbitCam`.
#[derive(SystemSet, Debug, Hash, PartialEq, Eq, Clone)]
pub struct OrbitCamSystemSet;

/// The shape to restrict the camera's focus inside.
#[derive(Clone, PartialEq, Debug, Reflect, Copy)]
pub enum FocusBoundsShape {
    /// Limit the camera's focus to a sphere centered on `focus_bounds_origin`.
    Sphere(Sphere),
    /// Limit the camera's focus to a cuboid centered on `focus_bounds_origin`.
    Cuboid(Cuboid),
}

impl From<Sphere> for FocusBoundsShape {
    fn from(value: Sphere) -> Self { Self::Sphere(value) }
}

impl From<Cuboid> for FocusBoundsShape {
    fn from(value: Cuboid) -> Self { Self::Cuboid(value) }
}

/// Whether the camera is allowed to orbit past the poles into an upside-down orientation.
#[derive(Clone, PartialEq, Eq, Debug, Reflect, Copy, Default)]
pub enum UpsideDownPolicy {
    /// Camera may orbit upside down.
    Allow,
    /// Camera pitch is clamped to prevent going upside down.
    #[default]
    Prevent,
}

/// Whether `OrbitCam` has been initialized from the camera's current transform.
#[derive(Clone, PartialEq, Eq, Debug, Reflect, Copy, Default)]
pub enum InitializationState {
    /// Initialization has not yet occurred.
    #[default]
    Pending,
    /// Initialization is complete.
    Complete,
}

/// One-shot controller request for recalculating the camera transform.
#[doc(hidden)]
#[derive(Clone, PartialEq, Eq, Debug, Reflect, Copy, Default)]
pub enum OrbitCamUpdateRequest {
    /// No forced update was requested.
    #[default]
    None,
    /// Force one transform update on the next controller pass.
    ForceUpdate,
}

/// Which time source drives camera smoothing.
#[derive(Clone, PartialEq, Eq, Debug, Reflect, Copy, Default)]
pub enum TimeSource {
    /// Use Bevy's virtual time (respects pause).
    #[default]
    Virtual,
    /// Use real (wall-clock) time (ignores pause).
    Real,
}

/// Internal per-camera state used to keep orbit direction stable during a drag.
#[derive(Component, Default, Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) struct OrbitDragState {
    orientation: CameraOrientation,
    orbit_drag:  DragActivity,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum DragActivity {
    Active,
    #[default]
    Idle,
}

impl From<bool> for DragActivity {
    fn from(active: bool) -> Self { if active { Self::Active } else { Self::Idle } }
}

/// Whether the camera was latched as upside down when orbit dragging started.
#[derive(Clone, PartialEq, Eq, Debug, Copy, Default)]
pub(crate) enum CameraOrientation {
    #[default]
    Normal,
    UpsideDown,
}

const fn clamp_optional(value: f32, min: Option<f32>, max: Option<f32>) -> f32 {
    let mut clamped_value = value;
    if let Some(min) = min
        && clamped_value < min
    {
        clamped_value = min;
    }
    if let Some(max) = max
        && clamped_value > max
    {
        clamped_value = max;
    }
    clamped_value
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
#[require(
    Camera3d,
    OrbitDragState,
    OrbitCamInput,
    OrbitCamInputContext,
    OrbitCamPreset
)]
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
    /// automatically, but you can also update it directly for programmatic camera motion.
    /// Defaults to `Vec3::ZERO`.
    pub target_focus:        Vec3,
    /// The target yaw value. The camera will smoothly transition to this value. Updated
    /// automatically, but you can also update it directly for programmatic camera motion.
    /// Defaults to `0.0`.
    pub target_yaw:          f32,
    /// The target pitch value. The camera will smoothly transition to this value Updated
    /// automatically, but you can also update it directly for programmatic camera motion.
    /// Defaults to `0.0`.
    pub target_pitch:        f32,
    /// The target radius value. The camera will smoothly transition to this value. Updated
    /// automatically, but you can also update it directly for programmatic camera motion.
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
    /// Whether to allow the camera to go upside down.
    /// Defaults to `UpsideDownPolicy::Prevent`.
    pub upside_down_policy:  UpsideDownPolicy,
    /// Whether `OrbitCam` has been initialized with the initial config.
    /// Set to `InitializationState::Complete` if you want the camera to smoothly animate to its
    /// initial position.
    /// Defaults to `InitializationState::Pending`.
    pub initialization:      InitializationState,
    /// One-shot update request used by [`OrbitCam::force_update`].
    #[doc(hidden)]
    pub update_request:      OrbitCamUpdateRequest,
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
            orbit_sensitivity:   DEFAULT_INPUT_SENSITIVITY,
            orbit_smoothness:    DEFAULT_ORBIT_SMOOTHNESS,
            pan_sensitivity:     DEFAULT_INPUT_SENSITIVITY,
            pan_smoothness:      DEFAULT_PAN_SMOOTHNESS,
            zoom_sensitivity:    DEFAULT_INPUT_SENSITIVITY,
            zoom_smoothness:     DEFAULT_ZOOM_SMOOTHNESS,
            yaw:                 None,
            pitch:               None,
            target_yaw:          DEFAULT_ORBIT_ANGLE,
            target_pitch:        DEFAULT_ORBIT_ANGLE,
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
            update_request:      OrbitCamUpdateRequest::None,
            axis:                [Vec3::X, Vec3::Y, Vec3::Z],
            time_source:         TimeSource::Virtual,
        }
    }
}

impl OrbitCam {
    /// Requests one transform update on the next controller pass.
    ///
    /// Use this after mutating current camera state or projection state directly,
    /// when no target-value change would otherwise make the controller recalculate
    /// the transform.
    pub const fn force_update(&mut self) {
        self.update_request = OrbitCamUpdateRequest::ForceUpdate;
    }

    pub(crate) fn consume_update_request(&mut self) -> OrbitCamUpdateRequest {
        core::mem::take(&mut self.update_request)
    }

    pub(super) const fn clamp_yaw(&self, yaw: f32) -> f32 {
        clamp_optional(yaw, self.yaw_lower_limit, self.yaw_upper_limit)
    }

    pub(super) const fn clamp_pitch(&self, pitch: f32) -> f32 {
        clamp_optional(pitch, self.pitch_lower_limit, self.pitch_upper_limit)
    }

    pub(super) const fn clamp_zoom(&self, zoom: f32) -> f32 {
        clamp_optional(zoom, Some(self.zoom_lower_limit), self.zoom_upper_limit)
    }

    pub(super) fn clamp_focus(&self, focus: Vec3) -> Vec3 {
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
