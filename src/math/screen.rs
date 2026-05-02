use std::ops::Add;
use std::ops::AddAssign;
use std::ops::Deref;
use std::ops::Div;
use std::ops::DivAssign;
use std::ops::Mul;
use std::ops::MulAssign;
use std::ops::Neg;
use std::ops::Sub;
use std::ops::SubAssign;

use bevy::math::Vec2;
use bevy::reflect::Reflect;

/// Pixel-space coordinates on screen.
///
/// Wraps `Vec2` to distinguish screen coordinates from other 2D
/// quantities. Useful for UI layout, cursor tracking, and any
/// computation in pixel space.
///
/// # Examples
///
/// ```
/// use bevy_kana::ScreenPosition;
///
/// let cursor = ScreenPosition::new(640.0, 480.0);
/// let offset = ScreenPosition::new(10.0, -5.0);
/// let moved = cursor + offset;
/// assert_eq!(moved.x, 650.0);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Default, Reflect)]
pub struct ScreenPosition(pub Vec2);

impl ScreenPosition {
    /// Creates a new screen position from `x` and `y` pixel coordinates.
    #[must_use]
    pub const fn new(x: f32, y: f32) -> Self { Self(Vec2::new(x, y)) }

    /// Consumes `self` and returns the inner `Vec2`.
    #[must_use]
    pub const fn into_inner(self) -> Vec2 { self.0 }
}

impl Deref for ScreenPosition {
    type Target = Vec2;

    fn deref(&self) -> &Vec2 { &self.0 }
}

impl From<Vec2> for ScreenPosition {
    fn from(value: Vec2) -> Self { Self(value) }
}

impl From<ScreenPosition> for Vec2 {
    fn from(value: ScreenPosition) -> Self { value.0 }
}

impl Add for ScreenPosition {
    type Output = Self;

    fn add(self, rhs: Self) -> Self { Self(self.0 + rhs.0) }
}

impl AddAssign for ScreenPosition {
    fn add_assign(&mut self, rhs: Self) { self.0 += rhs.0; }
}

impl Sub for ScreenPosition {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self { Self(self.0 - rhs.0) }
}

impl SubAssign for ScreenPosition {
    fn sub_assign(&mut self, rhs: Self) { self.0 -= rhs.0; }
}

impl Mul<f32> for ScreenPosition {
    type Output = Self;

    fn mul(self, rhs: f32) -> Self { Self(self.0 * rhs) }
}

impl MulAssign<f32> for ScreenPosition {
    fn mul_assign(&mut self, rhs: f32) { self.0 *= rhs; }
}

impl Div<f32> for ScreenPosition {
    type Output = Self;

    fn div(self, rhs: f32) -> Self { Self(self.0 / rhs) }
}

impl DivAssign<f32> for ScreenPosition {
    fn div_assign(&mut self, rhs: f32) { self.0 /= rhs; }
}

impl Neg for ScreenPosition {
    type Output = Self;

    fn neg(self) -> Self { Self(-self.0) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_returns_self() {
        let cursor_position = ScreenPosition(Vec2::new(100.0, 200.0));
        let offset_position = ScreenPosition(Vec2::new(10.0, 20.0));
        let result = cursor_position + offset_position;
        assert_eq!(result.into_inner(), Vec2::new(110.0, 220.0));
    }

    #[test]
    fn deref_provides_vec2_access() {
        let screen_position = ScreenPosition(Vec2::new(100.0, 200.0));
        assert!((screen_position.x - 100.0).abs() < f32::EPSILON);
        assert!((screen_position.y - 200.0).abs() < f32::EPSILON);
    }

    #[test]
    fn from_into_roundtrip() {
        let vec2 = Vec2::new(100.0, 200.0);
        let screen_position = ScreenPosition::from(vec2);
        let round_tripped_vec2: Vec2 = screen_position.into();
        assert_eq!(vec2, round_tripped_vec2);
    }
}
