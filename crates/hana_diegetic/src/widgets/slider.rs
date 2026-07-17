/// Direction in which slider values increase.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SliderDirection {
    /// Values increase from left to right.
    #[default]
    LeftToRight,
    /// Values increase from right to left.
    RightToLeft,
    /// Values increase from bottom to top.
    BottomToTop,
    /// Values increase from top to bottom.
    TopToBottom,
}

/// Validated numeric range for a slider.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SliderRange {
    start: f32,
    end:   f32,
}

impl SliderRange {
    /// Creates a finite slider range whose start is strictly less than its end.
    ///
    /// # Errors
    ///
    /// Returns [`SliderConfigError::NonFiniteRange`] when either endpoint is
    /// non-finite, or [`SliderConfigError::UnorderedRange`] when `start` is not
    /// strictly less than `end`.
    pub fn new(start: f32, end: f32) -> Result<Self, SliderConfigError> {
        if !start.is_finite() || !end.is_finite() {
            return Err(SliderConfigError::NonFiniteRange);
        }
        if start >= end {
            return Err(SliderConfigError::UnorderedRange);
        }
        Ok(Self { start, end })
    }

    /// Returns the inclusive lower endpoint.
    #[must_use]
    pub const fn start(self) -> f32 { self.start }

    /// Returns the inclusive upper endpoint.
    #[must_use]
    pub const fn end(self) -> f32 { self.end }
}

/// Validated step interval for a slider.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SliderStep(f32);

impl SliderStep {
    /// Creates a finite, positive slider step.
    ///
    /// # Errors
    ///
    /// Returns [`SliderConfigError::NonPositiveStep`] when `step` is non-finite,
    /// zero, or negative.
    pub fn new(step: f32) -> Result<Self, SliderConfigError> {
        if !step.is_finite() || step <= 0.0 {
            return Err(SliderConfigError::NonPositiveStep);
        }
        Ok(Self(step))
    }

    /// Returns the step interval.
    #[must_use]
    pub const fn value(self) -> f32 { self.0 }
}

/// Authored configuration for a panel slider.
///
/// Attach it to an element with [`El::slider`](crate::El::slider). Runtime
/// slider state and events are added by the slider-behavior phase.
#[must_use]
#[derive(Clone, Debug, PartialEq)]
pub struct Slider {
    range:         SliderRange,
    initial_value: f32,
    step:          Option<SliderStep>,
    direction:     SliderDirection,
}

impl Slider {
    /// Creates a slider declaration with a finite initial value.
    ///
    /// # Errors
    ///
    /// Returns [`SliderConfigError::NonFiniteValue`] when `initial_value` is
    /// non-finite.
    pub fn new(range: SliderRange, initial_value: f32) -> Result<Self, SliderConfigError> {
        if !initial_value.is_finite() {
            return Err(SliderConfigError::NonFiniteValue);
        }
        Ok(Self {
            range,
            initial_value,
            step: None,
            direction: SliderDirection::default(),
        })
    }

    /// Sets the validated step interval.
    pub const fn step(mut self, step: SliderStep) -> Self {
        self.step = Some(step);
        self
    }

    /// Sets the direction in which values increase.
    pub const fn direction(mut self, direction: SliderDirection) -> Self {
        self.direction = direction;
        self
    }
}

/// Invalid slider authoring configuration.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum SliderConfigError {
    /// A range endpoint was non-finite.
    #[error("slider range endpoints must be finite")]
    NonFiniteRange,
    /// The range start was not strictly less than the range end.
    #[error("slider range start must be less than its end")]
    UnorderedRange,
    /// A slider value was non-finite.
    #[error("slider value must be finite")]
    NonFiniteValue,
    /// A slider step was non-finite, zero, or negative.
    #[error("slider step must be finite and positive")]
    NonPositiveStep,
}

#[cfg(test)]
mod tests {
    use super::Slider;
    use super::SliderConfigError;
    use super::SliderDirection;
    use super::SliderRange;
    use super::SliderStep;

    #[test]
    fn range_rejects_non_finite_endpoints() {
        assert_eq!(
            SliderRange::new(f32::NAN, 1.0),
            Err(SliderConfigError::NonFiniteRange)
        );
        assert_eq!(
            SliderRange::new(0.0, f32::INFINITY),
            Err(SliderConfigError::NonFiniteRange)
        );
    }

    #[test]
    fn range_rejects_unordered_endpoints() {
        assert_eq!(
            SliderRange::new(1.0, 1.0),
            Err(SliderConfigError::UnorderedRange)
        );
        assert_eq!(
            SliderRange::new(2.0, 1.0),
            Err(SliderConfigError::UnorderedRange)
        );
    }

    #[test]
    fn slider_rejects_non_finite_initial_value() {
        let Ok(range) = SliderRange::new(0.0, 1.0) else {
            return;
        };
        assert_eq!(
            Slider::new(range, f32::NAN),
            Err(SliderConfigError::NonFiniteValue)
        );
    }

    #[test]
    fn step_rejects_non_positive_or_non_finite_values() {
        for invalid in [0.0, -1.0, f32::NAN, f32::INFINITY] {
            assert_eq!(
                SliderStep::new(invalid),
                Err(SliderConfigError::NonPositiveStep)
            );
        }
    }

    #[test]
    fn valid_slider_retains_authored_configuration() {
        let Ok(range) = SliderRange::new(-1.0, 2.0) else {
            return;
        };
        let Ok(step) = SliderStep::new(0.25) else {
            return;
        };
        let Ok(slider) = Slider::new(range, 0.5) else {
            return;
        };
        let slider = slider.step(step).direction(SliderDirection::TopToBottom);

        assert_eq!(slider.range, range);
        assert!((slider.initial_value - 0.5).abs() <= f32::EPSILON);
        assert_eq!(slider.step, Some(step));
        assert_eq!(slider.direction, SliderDirection::TopToBottom);
    }

    #[test]
    fn error_messages_are_stable() {
        let messages = [
            (
                SliderConfigError::NonFiniteRange,
                "slider range endpoints must be finite",
            ),
            (
                SliderConfigError::UnorderedRange,
                "slider range start must be less than its end",
            ),
            (
                SliderConfigError::NonFiniteValue,
                "slider value must be finite",
            ),
            (
                SliderConfigError::NonPositiveStep,
                "slider step must be finite and positive",
            ),
        ];

        for (error, message) in messages {
            assert_eq!(error.to_string(), message);
        }
    }
}
