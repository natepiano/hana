use bevy::prelude::*;

use super::config::OrbitCamPresetConfig;
use super::source_sensitivity::GamepadSensitivity;
use crate::input::ControlSpeed;
use crate::input::bindings::CameraInputGamepadSelectionPolicy;
use crate::input::bindings::InputDeadZone;
use crate::input::bindings::OrbitCamBindings;
use crate::input::bindings::OrbitCamBindingsBuilder;
use crate::input::bindings::OrbitCamHeldBinding;
use crate::input::bindings::OrbitCamInputBinding;
use crate::input::bindings::OrbitCamSensitivity;
use crate::input::bindings::error::OrbitCamBindingsError;

/// Tunable gamepad preset descriptor.
#[derive(Clone, Copy, Debug, PartialEq, Reflect)]
#[reflect(Default)]
pub struct OrbitCamGamepadPreset {
    sensitivity:      OrbitCamSensitivity,
    orbit_scale:      f32,
    slow_orbit_scale: f32,
    pan_scale:        f32,
    slow_pan_scale:   f32,
    zoom_scale:       f32,
    slow_zoom_scale:  f32,
    stick_dead_zone:  InputDeadZone,
}

impl OrbitCamGamepadPreset {
    const DEFAULT_ORBIT_SCALE: f32 = 1200.0;
    const DEFAULT_PAN_SCALE: f32 = 800.0;
    const DEFAULT_SLOW_ORBIT_SCALE: f32 = 120.0;
    const DEFAULT_SLOW_PAN_SCALE: f32 = 80.0;
    const DEFAULT_SLOW_ZOOM_SCALE: f32 = 0.6;
    const DEFAULT_STICK_DEAD_ZONE_LOWER: f32 = 0.18;
    const DEFAULT_STICK_DEAD_ZONE_UPPER: f32 = 1.0;
    const DEFAULT_ZOOM_SCALE: f32 = 7.0;
    const MAX_DEAD_ZONE: f32 = 1.0;
    const MIN_DEAD_ZONE: f32 = 0.0;
    const MIN_SCALE: f32 = 0.0;

    /// Starts a tuning builder from this preset.
    #[must_use]
    pub const fn customize(self) -> OrbitCamGamepadPresetBuilder {
        OrbitCamGamepadPresetBuilder { preset: self }
    }

    /// Builds the zero-config gamepad preset.
    ///
    /// # Errors
    ///
    /// Returns [`OrbitCamBindingsError`] when generated descriptors fail
    /// validation.
    pub fn build(self) -> Result<OrbitCamBindings, OrbitCamBindingsError> {
        <Self as OrbitCamPresetConfig>::build(self)
    }

    /// Sets source sensitivity for generated gamepad bindings.
    #[must_use]
    pub const fn gamepad_sensitivity(mut self, sensitivity: OrbitCamSensitivity) -> Self {
        self.sensitivity = sensitivity;
        self
    }

    /// Sets the fast orbit scale.
    #[must_use]
    pub const fn orbit_scale(mut self, orbit_scale: f32) -> Self {
        self.orbit_scale = orbit_scale;
        self
    }

    /// Sets the slow orbit scale.
    #[must_use]
    pub const fn slow_orbit_scale(mut self, slow_orbit_scale: f32) -> Self {
        self.slow_orbit_scale = slow_orbit_scale;
        self
    }

    /// Sets the fast pan scale.
    #[must_use]
    pub const fn pan_scale(mut self, pan_scale: f32) -> Self {
        self.pan_scale = pan_scale;
        self
    }

    /// Sets the slow pan scale.
    #[must_use]
    pub const fn slow_pan_scale(mut self, slow_pan_scale: f32) -> Self {
        self.slow_pan_scale = slow_pan_scale;
        self
    }

    /// Sets the fast zoom scale.
    #[must_use]
    pub const fn zoom_scale(mut self, zoom_scale: f32) -> Self {
        self.zoom_scale = zoom_scale;
        self
    }

    /// Sets the slow zoom scale.
    #[must_use]
    pub const fn slow_zoom_scale(mut self, slow_zoom_scale: f32) -> Self {
        self.slow_zoom_scale = slow_zoom_scale;
        self
    }

    /// Sets the axial stick dead-zone thresholds.
    #[must_use]
    pub const fn stick_dead_zone(mut self, stick_dead_zone: InputDeadZone) -> Self {
        self.stick_dead_zone = stick_dead_zone;
        self
    }

    pub(super) fn build_into(
        self,
        builder: OrbitCamBindingsBuilder,
    ) -> Result<OrbitCamBindingsBuilder, OrbitCamBindingsError> {
        self.validate()?;
        Ok(self.add_to(builder))
    }

    fn validate(&self) -> Result<(), OrbitCamBindingsError> {
        self.sensitivity.validate()?;
        Self::validate_scale_pair(self.orbit_scale, self.slow_orbit_scale)?;
        Self::validate_scale_pair(self.pan_scale, self.slow_pan_scale)?;
        Self::validate_scale_pair(self.zoom_scale, self.slow_zoom_scale)?;
        self.validate_stick_dead_zone()
    }

    fn validate_scale_pair(fast: f32, slow: f32) -> Result<(), OrbitCamBindingsError> {
        if !fast.is_finite()
            || !slow.is_finite()
            || fast < Self::MIN_SCALE
            || slow < Self::MIN_SCALE
            || slow > fast
        {
            return Err(OrbitCamBindingsError::InvalidScale);
        }
        Ok(())
    }

    fn validate_stick_dead_zone(&self) -> Result<(), OrbitCamBindingsError> {
        let lower = self.stick_dead_zone.lower_threshold;
        let upper = self.stick_dead_zone.upper_threshold;
        if !lower.is_finite()
            || !upper.is_finite()
            || lower < Self::MIN_DEAD_ZONE
            || upper > Self::MAX_DEAD_ZONE
            || lower >= upper
        {
            return Err(OrbitCamBindingsError::InvalidDeadZone);
        }
        Ok(())
    }

    fn add_to(self, builder: OrbitCamBindingsBuilder) -> OrbitCamBindingsBuilder {
        let orbit_sensitivity = self.sensitivity.orbit_sensitivity().value();
        let pan_sensitivity = self.sensitivity.pan_sensitivity().value();
        let zoom_sensitivity = self.sensitivity.zoom_sensitivity().value();
        let fast_orbit = gamepad_stick(
            GamepadAxis::RightStickX,
            GamepadAxis::RightStickY,
            self.orbit_scale,
            self.stick_dead_zone,
        );
        let slow_orbit = gamepad_stick(
            GamepadAxis::RightStickX,
            GamepadAxis::RightStickY,
            self.slow_orbit_scale,
            self.stick_dead_zone,
        );
        let fast_pan = gamepad_stick(
            GamepadAxis::LeftStickX,
            GamepadAxis::LeftStickY,
            self.pan_scale,
            self.stick_dead_zone,
        );
        let slow_pan = gamepad_stick(
            GamepadAxis::LeftStickX,
            GamepadAxis::LeftStickY,
            self.slow_pan_scale,
            self.stick_dead_zone,
        );

        builder
            .orbit(
                OrbitCamHeldBinding::same(fast_orbit)
                    .with_sensitivity(orbit_sensitivity)
                    .with_blocked_gate(GamepadButton::RightTrigger),
            )
            .orbit(
                OrbitCamHeldBinding::same(slow_orbit)
                    .with_sensitivity(orbit_sensitivity)
                    .with_required_gate(GamepadButton::RightTrigger)
                    .speed(ControlSpeed::Slow),
            )
            .pan(
                OrbitCamHeldBinding::same(fast_pan)
                    .with_sensitivity(pan_sensitivity)
                    .with_blocked_gate(GamepadButton::LeftTrigger),
            )
            .pan(
                OrbitCamHeldBinding::same(slow_pan)
                    .with_sensitivity(pan_sensitivity)
                    .with_required_gate(GamepadButton::LeftTrigger)
                    .speed(ControlSpeed::Slow),
            )
            .zoom(
                OrbitCamHeldBinding::same(gamepad_trigger(
                    GamepadButton::RightTrigger2,
                    self.zoom_scale,
                ))
                .with_sensitivity(zoom_sensitivity)
                .with_blocked_gate(GamepadButton::RightTrigger),
            )
            .zoom(
                OrbitCamHeldBinding::same(gamepad_trigger(
                    GamepadButton::LeftTrigger2,
                    -self.zoom_scale,
                ))
                .with_sensitivity(zoom_sensitivity)
                .with_blocked_gate(GamepadButton::LeftTrigger),
            )
            .zoom(
                OrbitCamHeldBinding::same(gamepad_trigger(
                    GamepadButton::RightTrigger2,
                    self.slow_zoom_scale,
                ))
                .with_sensitivity(zoom_sensitivity)
                .with_required_gate(GamepadButton::RightTrigger)
                .speed(ControlSpeed::Slow),
            )
            .zoom(
                OrbitCamHeldBinding::same(gamepad_trigger(
                    GamepadButton::LeftTrigger2,
                    -self.slow_zoom_scale,
                ))
                .with_sensitivity(zoom_sensitivity)
                .with_required_gate(GamepadButton::LeftTrigger)
                .speed(ControlSpeed::Slow),
            )
            .gamepad(CameraInputGamepadSelectionPolicy::Active)
    }
}

impl Default for OrbitCamGamepadPreset {
    fn default() -> Self {
        Self {
            sensitivity:      OrbitCamSensitivity::default(),
            orbit_scale:      Self::DEFAULT_ORBIT_SCALE,
            slow_orbit_scale: Self::DEFAULT_SLOW_ORBIT_SCALE,
            pan_scale:        Self::DEFAULT_PAN_SCALE,
            slow_pan_scale:   Self::DEFAULT_SLOW_PAN_SCALE,
            zoom_scale:       Self::DEFAULT_ZOOM_SCALE,
            slow_zoom_scale:  Self::DEFAULT_SLOW_ZOOM_SCALE,
            stick_dead_zone:  InputDeadZone::new(
                Self::DEFAULT_STICK_DEAD_ZONE_LOWER,
                Self::DEFAULT_STICK_DEAD_ZONE_UPPER,
            ),
        }
    }
}

impl GamepadSensitivity for OrbitCamGamepadPreset {
    type Sensitivity = OrbitCamSensitivity;

    fn gamepad_sensitivity(self, sensitivity: Self::Sensitivity) -> Self {
        Self::gamepad_sensitivity(self, sensitivity)
    }
}

impl OrbitCamPresetConfig for OrbitCamGamepadPreset {
    fn build(self) -> Result<OrbitCamBindings, OrbitCamBindingsError> {
        self.build_into(OrbitCamBindings::builder())?.build()
    }
}

/// Fluent tuning builder for [`OrbitCamGamepadPreset`].
#[derive(Clone, Copy, Debug, PartialEq, Reflect)]
pub struct OrbitCamGamepadPresetBuilder {
    preset: OrbitCamGamepadPreset,
}

impl OrbitCamGamepadPresetBuilder {
    /// Sets the fast orbit scale.
    #[must_use]
    pub const fn orbit_scale(mut self, orbit_scale: f32) -> Self {
        self.preset.orbit_scale = orbit_scale;
        self
    }

    /// Sets the slow orbit scale.
    #[must_use]
    pub const fn slow_orbit_scale(mut self, slow_orbit_scale: f32) -> Self {
        self.preset.slow_orbit_scale = slow_orbit_scale;
        self
    }

    /// Sets the fast pan scale.
    #[must_use]
    pub const fn pan_scale(mut self, pan_scale: f32) -> Self {
        self.preset.pan_scale = pan_scale;
        self
    }

    /// Sets the slow pan scale.
    #[must_use]
    pub const fn slow_pan_scale(mut self, slow_pan_scale: f32) -> Self {
        self.preset.slow_pan_scale = slow_pan_scale;
        self
    }

    /// Sets the fast zoom scale.
    #[must_use]
    pub const fn zoom_scale(mut self, zoom_scale: f32) -> Self {
        self.preset.zoom_scale = zoom_scale;
        self
    }

    /// Sets the slow zoom scale.
    #[must_use]
    pub const fn slow_zoom_scale(mut self, slow_zoom_scale: f32) -> Self {
        self.preset.slow_zoom_scale = slow_zoom_scale;
        self
    }

    /// Sets the axial stick dead-zone thresholds.
    #[must_use]
    pub const fn stick_dead_zone(mut self, stick_dead_zone: InputDeadZone) -> Self {
        self.preset.stick_dead_zone = stick_dead_zone;
        self
    }

    /// Sets source sensitivity for generated gamepad bindings.
    #[must_use]
    pub const fn gamepad_sensitivity(mut self, sensitivity: OrbitCamSensitivity) -> Self {
        self.preset.sensitivity = sensitivity;
        self
    }

    /// Returns the tuned preset payload.
    #[must_use]
    pub const fn into_preset(self) -> OrbitCamGamepadPreset { self.preset }

    /// Builds tuned gamepad bindings.
    ///
    /// # Errors
    ///
    /// Returns [`OrbitCamBindingsError`] when generated descriptors fail
    /// validation.
    pub fn build(self) -> Result<OrbitCamBindings, OrbitCamBindingsError> {
        self.preset.build_into(OrbitCamBindings::builder())?.build()
    }
}

fn gamepad_stick(
    x_axis: GamepadAxis,
    y_axis: GamepadAxis,
    scale: f32,
    dead_zone: InputDeadZone,
) -> OrbitCamInputBinding {
    OrbitCamInputBinding::gamepad_axes_2d(x_axis, y_axis)
        .with_dead_zone(dead_zone)
        .with_scale(Vec2::splat(scale))
        .with_delta_scale()
}

fn gamepad_trigger(button: GamepadButton, scale: f32) -> OrbitCamInputBinding {
    OrbitCamInputBinding::gamepad_button_axis(button, scale).with_delta_scale()
}
