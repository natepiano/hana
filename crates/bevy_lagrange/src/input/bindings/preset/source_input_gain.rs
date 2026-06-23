//! Source-level input gain setters for built-in orbit-camera presets.

/// Sets mouse-backed input gain on an orbit-camera preset.
pub trait MouseInputGain {
    /// `InputGain` value accepted by this preset.
    type Gain;

    /// Returns this preset with mouse-backed input gain replaced.
    #[must_use]
    fn mouse_input_gain(self, input_gain: Self::Gain) -> Self;
}

/// Sets Bevy pixel-scroll input gain on an orbit-camera preset.
pub trait SmoothScrollInputGain {
    /// `InputGain` value accepted by this preset.
    type Gain;

    /// Returns this preset with smooth-scroll input gain replaced.
    #[must_use]
    fn smooth_scroll_input_gain(self, input_gain: Self::Gain) -> Self;
}

/// Sets gamepad-backed input gain on an orbit-camera preset.
pub trait GamepadInputGain {
    /// `InputGain` value accepted by this preset.
    type Gain;

    /// Returns this preset with gamepad-backed input gain replaced.
    #[must_use]
    fn gamepad_input_gain(self, input_gain: Self::Gain) -> Self;
}
