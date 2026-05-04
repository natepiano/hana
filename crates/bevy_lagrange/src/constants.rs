// Animation constants
/// Tolerance for detecting external camera input during animations.
/// Values within this threshold are considered unchanged (accounts for floating point noise).
pub(crate) const EXTERNAL_INPUT_TOLERANCE: f32 = 1e-6;
/// Conversion factor from seconds to milliseconds.
pub(crate) const MILLIS_PER_SECOND: f32 = 1000.0;

// Fit constants
/// Maximum centering iterations per candidate radius.
pub(crate) const CENTERING_MAX_ITERATIONS: usize = 10;
/// Normalized screen-space center offset tolerance.
pub(crate) const CENTERING_TOLERANCE: f32 = 0.0001;
/// Default fit margin applied by event constructors.
pub(crate) const DEFAULT_FIT_MARGIN: f32 = 0.1;
/// Minimum screen-space extent before treating a dimension as degenerate (edge-on).
/// Below this threshold the dimension is ignored for fit purposes.
pub(crate) const DEGENERATE_EXTENT_THRESHOLD: f32 = 1e-6;
/// Initial best-guess radius as a multiple of the object radius (2x).
pub(crate) const INITIAL_RADIUS_MULTIPLIER: f32 = 2.0;
/// Maximum allowed margin value.
pub(crate) const MAX_MARGIN: f32 = 0.9999;
/// Maximum binary search iterations.
pub(crate) const MAX_ITERATIONS: usize = 200;
/// Maximum search radius as a multiple of the object radius (100x).
pub(crate) const MAX_RADIUS_MULTIPLIER: f32 = 100.0;
/// Minimum allowed margin value.
pub(crate) const MIN_MARGIN: f32 = 0.0;
/// Minimum search radius as a fraction of the object radius (0.1x).
pub(crate) const MIN_RADIUS_MULTIPLIER: f32 = 0.1;
/// Convergence tolerance (0.1% of search range).
pub(crate) const TOLERANCE: f32 = 0.001;

// Input constants
/// Conversion factor from mouse drag delta to scroll-equivalent zoom input.
pub(crate) const BUTTON_ZOOM_SCALE: f32 = 0.03;
/// Amplification factor for trackpad pinch gesture input.
pub(crate) const PINCH_GESTURE_AMPLIFICATION: f32 = 10.0;
/// Scale factor for converting pixel-based scroll events to zoom input.
pub(crate) const PIXEL_SCROLL_SCALE: f32 = 0.005;

// Orbit constants
/// Approximate-equality threshold for floating-point convergence checks.
pub(crate) const EPSILON: f32 = 0.001;
/// Minimum orbit radius when camera and focus coincide.
pub(crate) const MIN_ORBIT_RADIUS: f32 = 0.05;
/// Fraction of current radius applied per scroll unit.
pub(crate) const SCROLL_ZOOM_FACTOR: f32 = 0.2;
/// Exponent that shapes the smoothing response curve.
pub(crate) const SMOOTHNESS_EXPONENT: i32 = 7;
/// Conversion factor from two-finger touch pinch to zoom input.
pub(crate) const TOUCH_PINCH_SCALE: f32 = 0.015;

// Orbit defaults
/// Default smoothing factor for orbit motion.
pub(crate) const DEFAULT_ORBIT_SMOOTHNESS: f32 = 0.1;
/// Default smoothing factor for pan motion.
pub(crate) const DEFAULT_PAN_SMOOTHNESS: f32 = 0.02;
/// Default orbital radius.
pub(crate) const DEFAULT_TARGET_RADIUS: f32 = 1.0;
/// Default lower limit on zoom (radius or orthographic scale).
pub(crate) const DEFAULT_ZOOM_LOWER_LIMIT: f32 = 1e-7;
/// Default smoothing factor for zoom motion.
pub(crate) const DEFAULT_ZOOM_SMOOTHNESS: f32 = 0.1;

// Projection constants
/// Perspective near plane as a fraction of the current orbit radius.
pub(crate) const PERSPECTIVE_NEAR_RADIUS_FACTOR: f32 = 0.001;
/// Absolute minimum perspective near plane.
pub(crate) const PERSPECTIVE_NEAR_MIN: f32 = 1e-6;
/// Minimum depth for a point to be considered in front of the camera.
/// Points at or below this depth are treated as behind the camera in perspective projection.
pub(crate) const MIN_VISIBLE_DEPTH: f32 = 0.1;
