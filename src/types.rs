//! Public types, enums, and resources used by `OrbitCam`.

use bevy::prelude::*;

use super::touch::TouchInput;

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

impl From<Sphere> for FocusBoundsShape {
    fn from(value: Sphere) -> Self { Self::Sphere(value) }
}

impl From<Cuboid> for FocusBoundsShape {
    fn from(value: Cuboid) -> Self { Self::Cuboid(value) }
}

/// Which axis controls button-based zoom.
#[derive(Clone, PartialEq, Eq, Debug, Reflect, Copy)]
pub enum ButtonZoomAxis {
    /// Zoom by moving the mouse along the x-axis.
    X,
    /// Zoom by moving the mouse along the y-axis.
    Y,
    /// Zoom by moving the mouse along either the x-axis or the y-axis.
    XY,
}

/// Selects how trackpad input is interpreted.
///
/// In Blender the trackpad orbits when scrolling. If you hold down the `ShiftLeft`, it pans and
/// holding down `ControlLeft` will zoom.
#[derive(Clone, PartialEq, Eq, Debug, Reflect, Copy)]
pub enum TrackpadBehavior {
    /// Trackpad scrolling and pinching both zoom.
    ZoomOnly,
    /// Trackpad scrolling orbits by default, and can pan or zoom with modifiers.
    /// Pinching still zooms.
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

/// Interactive input configuration for `OrbitCam`.
#[derive(Clone, PartialEq, Debug, Reflect, Copy)]
pub struct InputControl {
    /// The control scheme for touch inputs.
    /// Set to `None` to disable touch input entirely.
    pub touch:    Option<TouchInput>,
    /// The trackpad input configuration.
    pub trackpad: Option<TrackpadInput>,
    /// Direction of all zoom input, including scroll, pinch, and button-drag zoom.
    pub zoom:     ZoomDirection,
}

impl Default for InputControl {
    fn default() -> Self {
        Self {
            touch:    Some(TouchInput::OneFingerOrbit),
            trackpad: Some(TrackpadInput::default()),
            zoom:     ZoomDirection::Normal,
        }
    }
}

/// Trackpad-specific input configuration for `OrbitCam`.
#[derive(Clone, PartialEq, Debug, Reflect, Copy)]
pub struct TrackpadInput {
    /// The behavior for trackpad scroll input.
    pub behavior:    TrackpadBehavior,
    /// The sensitivity of trackpad gestures when using `BlenderLike` behavior.
    pub sensitivity: f32,
}

impl Default for TrackpadInput {
    fn default() -> Self {
        Self {
            behavior:    TrackpadBehavior::ZoomOnly,
            sensitivity: 1.0,
        }
    }
}

impl TrackpadInput {
    /// Creates a Blender-like trackpad configuration with default modifiers and sensitivity.
    #[must_use]
    pub const fn blender_default() -> Self {
        Self {
            behavior:    TrackpadBehavior::blender_default(),
            sensitivity: 1.0,
        }
    }
}

/// Direction of scroll/zoom input.
#[derive(Clone, PartialEq, Eq, Debug, Reflect, Copy, Default)]
pub enum ZoomDirection {
    /// Scrolling zooms in the default direction.
    #[default]
    Normal,
    /// Scrolling zooms in the opposite direction.
    Reversed,
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

/// Whether to force a transform update this frame regardless of input.
#[derive(Clone, PartialEq, Eq, Debug, Reflect, Copy, Default)]
pub enum ForceUpdate {
    /// No forced update.
    #[default]
    Idle,
    /// Force a transform update this frame, then return to `Idle`.
    Pending,
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
