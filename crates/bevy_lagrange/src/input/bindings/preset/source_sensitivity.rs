//! Source-level sensitivity setters for built-in orbit-camera presets.

/// Sets mouse-backed input sensitivity on an orbit-camera preset.
pub trait MouseSensitivity {
    /// Sensitivity value accepted by this preset.
    type Sensitivity;

    /// Returns this preset with mouse-backed sensitivity replaced.
    #[must_use]
    fn mouse_sensitivity(self, sensitivity: Self::Sensitivity) -> Self;
}

/// Sets Bevy pixel-scroll sensitivity on an orbit-camera preset.
pub trait SmoothScrollSensitivity {
    /// Sensitivity value accepted by this preset.
    type Sensitivity;

    /// Returns this preset with smooth-scroll sensitivity replaced.
    #[must_use]
    fn smooth_scroll_sensitivity(self, sensitivity: Self::Sensitivity) -> Self;
}

/// Sets gamepad-backed input sensitivity on an orbit-camera preset.
pub trait GamepadSensitivity {
    /// Sensitivity value accepted by this preset.
    type Sensitivity;

    /// Returns this preset with gamepad-backed sensitivity replaced.
    #[must_use]
    fn gamepad_sensitivity(self, sensitivity: Self::Sensitivity) -> Self;
}
