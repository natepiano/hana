/// Clamps a value between two optional bounds. If both `min` and `max` are `None`,
/// returns the value unchanged.
pub(crate) const fn clamp_optional(value: f32, min: Option<f32>, max: Option<f32>) -> f32 {
    let mut v = value;
    if let Some(min) = min
        && v < min
    {
        v = min;
    }
    if let Some(max) = max
        && v > max
    {
        v = max;
    }
    v
}
