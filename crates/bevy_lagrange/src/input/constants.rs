use std::time::Duration;

// action names
pub(super) const ORBIT_ACTION_NAME: &str = "OrbitCamOrbitAction";
pub(super) const PAN_ACTION_NAME: &str = "OrbitCamPanAction";
pub(super) const ZOOM_COARSE_ACTION_NAME: &str = "OrbitCamZoomCoarseAction";
pub(super) const ZOOM_SMOOTH_ACTION_NAME: &str = "OrbitCamZoomSmoothAction";

// camera labels
pub(super) const ORBIT_CAM_CAMERA_LABEL: &str = "OrbitCam";

// debounce durations
pub(super) const DEFAULT_REPORTING_DEBOUNCE: Duration = Duration::from_millis(100);

// mode labels
pub(super) const INPUT_MODE_LABEL: &str = "Input";
pub(super) const PRESET_MODE_LABEL: &str = "Preset";

// mode values
pub(super) const CUSTOM_BINDINGS_MODE_VALUE: &str = "custom bindings";
pub(super) const MANUAL_MODE_VALUE: &str = "manual input";

// row labels
pub(super) const APP_AUTHORED_INPUT_ROW_LABEL: &str = "app-authored input";
pub(super) const CUSTOM_INPUT_ROW_LABEL: &str = "custom input";
pub(super) const GAMEPAD_BINDINGS_ROW_LABEL: &str = "gamepad bindings";
pub(super) const ONE_FINGER_TOUCH_ROW_LABEL: &str = "one finger touch";
pub(super) const TWO_FINGER_TOUCH_ROW_LABEL: &str = "two finger touch";

// source-stem labels
pub(super) const GAMEPAD_BINDING_SOURCE_LABEL: &str = "gamepad binding";
pub(super) const INPUT_BINDING_SOURCE_LABEL: &str = "input binding";
pub(super) const KEYBOARD_BINDING_SOURCE_LABEL: &str = "keyboard binding";
pub(super) const MANUAL_INPUT_SOURCE_LABEL: &str = "manual input";
pub(super) const MOUSE_BINDING_SOURCE_LABEL: &str = "mouse binding";
pub(super) const MOUSE_DESCRIPTOR_LABEL: &str = "mouse";
pub(super) const PINCH_SOURCE_LABEL: &str = "pinch";
pub(super) const TOUCH_SOURCE_LABEL: &str = "touch";
pub(super) const TRACKPAD_SOURCE_LABEL: &str = "smooth-scroll";
pub(super) const WHEEL_SOURCE_LABEL: &str = "wheel";

// split zoom-direction row labels — each bidirectional zoom source shows one
// row per direction, labeled by the physical gesture that drives it.
pub(super) const WHEEL_ZOOM_IN_LABEL: &str = "wheel ↑";
pub(super) const WHEEL_ZOOM_OUT_LABEL: &str = "wheel ↓";
pub(super) const PINCH_ZOOM_IN_LABEL: &str = "pinch out";
pub(super) const PINCH_ZOOM_OUT_LABEL: &str = "pinch in";
pub(super) const SMOOTH_SCROLL_ZOOM_IN_LABEL: &str = "scroll ↑";
pub(super) const SMOOTH_SCROLL_ZOOM_OUT_LABEL: &str = "scroll ↓";

// shared test constants
#[cfg(test)]
pub(super) const CUSTOM_SLOW_SCALE: f32 = 0.25;
#[cfg(test)]
pub(super) const DISABLED_INPUT_GAIN: f32 = super::InputGain::DISABLED.0;
#[cfg(test)]
pub(super) const INVALID_SOURCE_INPUT_GAIN: f32 = -1.0;
#[cfg(test)]
pub(super) const PINCH_INPUT_GAIN: f32 = 0.5;
#[cfg(test)]
pub(super) const WHEEL_INPUT_GAIN: f32 = 0.25;
