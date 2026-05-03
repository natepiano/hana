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

use bevy::math::Quat;
use bevy::math::Vec3;
use bevy::reflect::Reflect;

/// Generates a semantic newtype wrapper around a math primitive.
///
/// Semantic types wrap an inner type with no invariant — their purpose is
/// to prevent accidental mixing of values that share the same underlying
/// type but carry different meaning (e.g., `Position` vs `Velocity`).
///
/// All arithmetic operations return `Self`, and `Deref` provides transparent
/// access to the inner type's methods and fields.
///
/// # Generated API
///
/// - `Deref<Target = InnerType>` for transparent field and method access
/// - `From<InnerType>` and `Into<InnerType>` conversions
/// - `into_inner(self) -> InnerType`
/// - `Add`, `Sub`, `Mul<f32>`, `Div<f32>`, `Neg` (all return `Self`)
/// - `AddAssign`, `SubAssign`, `MulAssign<f32>`, `DivAssign<f32>`
/// - `Add<InnerType>`, `Sub<InnerType>` for mixing with raw Bevy values
/// - `distance`, `distance_squared`, `lerp` accepting `impl Into<Self>`
macro_rules! semantic_newtype {
    (
        $(#[$meta:meta])*
        $name:ident, $inner:ty
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Default, Reflect)]
        pub struct $name(pub $inner);

        impl $name {
            /// Creates a new value from components.
            pub const fn new(x: f32, y: f32, z: f32) -> Self {
                Self(<$inner>::new(x, y, z))
            }

            /// Consumes `self` and returns the inner value.
            ///
            /// Use this when you need to pass the raw type to a Bevy API.
            pub const fn into_inner(self) -> $inner {
                self.0
            }

            /// Euclidean distance between two values.
            pub fn distance(self, other: impl Into<Self>) -> f32 {
                self.0.distance(other.into().0)
            }

            /// Squared euclidean distance (avoids a square root).
            pub fn distance_squared(self, other: impl Into<Self>) -> f32 {
                self.0.distance_squared(other.into().0)
            }

            /// Linear interpolation between two values.
            #[must_use]
            pub fn lerp(self, other: impl Into<Self>, t: f32) -> Self {
                Self(self.0.lerp(other.into().0, t))
            }
        }

        impl Deref for $name {
            type Target = $inner;

            fn deref(&self) -> &$inner {
                &self.0
            }
        }

        impl From<$inner> for $name {
            fn from(value: $inner) -> Self {
                Self(value)
            }
        }

        impl From<$name> for $inner {
            fn from(value: $name) -> Self {
                value.0
            }
        }

        impl Add for $name {
            type Output = Self;

            fn add(self, rhs: Self) -> Self {
                Self(self.0 + rhs.0)
            }
        }

        impl AddAssign for $name {
            fn add_assign(&mut self, rhs: Self) {
                self.0 += rhs.0;
            }
        }

        impl Sub for $name {
            type Output = Self;

            fn sub(self, rhs: Self) -> Self {
                Self(self.0 - rhs.0)
            }
        }

        impl SubAssign for $name {
            fn sub_assign(&mut self, rhs: Self) {
                self.0 -= rhs.0;
            }
        }

        impl Mul<f32> for $name {
            type Output = Self;

            fn mul(self, rhs: f32) -> Self {
                Self(self.0 * rhs)
            }
        }

        impl MulAssign<f32> for $name {
            fn mul_assign(&mut self, rhs: f32) {
                self.0 *= rhs;
            }
        }

        impl Div<f32> for $name {
            type Output = Self;

            fn div(self, rhs: f32) -> Self {
                Self(self.0 / rhs)
            }
        }

        impl DivAssign<f32> for $name {
            fn div_assign(&mut self, rhs: f32) {
                self.0 /= rhs;
            }
        }

        impl Neg for $name {
            type Output = Self;

            fn neg(self) -> Self {
                Self(-self.0)
            }
        }

        // Cross-type arithmetic with raw inner type.
        // Allows natural mixing with raw `Vec3` values from Bevy APIs.

        impl Add<$inner> for $name {
            type Output = Self;

            fn add(self, rhs: $inner) -> Self {
                Self(self.0 + rhs)
            }
        }

        impl AddAssign<$inner> for $name {
            fn add_assign(&mut self, rhs: $inner) {
                self.0 += rhs;
            }
        }

        impl Sub<$inner> for $name {
            type Output = Self;

            fn sub(self, rhs: $inner) -> Self {
                Self(self.0 - rhs)
            }
        }

        impl SubAssign<$inner> for $name {
            fn sub_assign(&mut self, rhs: $inner) {
                self.0 -= rhs;
            }
        }
    };
}

semantic_newtype!(
    /// A point in 3D space.
    ///
    /// Wraps `Vec3` to distinguish spatial positions from other vector
    /// quantities like velocity or displacement. All arithmetic operations
    /// return `Position`, and `Deref` provides transparent access to
    /// `Vec3` fields and methods.
    ///
    /// # Examples
    ///
    /// ```
    /// use bevy::math::Vec3;
    /// use bevy_kana::Position;
    ///
    /// let start_position = Position(Vec3::new(1.0, 0.0, 0.0));
    /// let end_position = Position(Vec3::new(3.0, 0.0, 0.0));
    ///
    /// // Centroid of two points
    /// let midpoint = (start_position + end_position) / 2.0;
    /// assert_eq!(midpoint.into_inner(), Vec3::new(2.0, 0.0, 0.0));
    /// ```
    Position, Vec3
);

semantic_newtype!(
    /// A delta or offset in 3D space.
    ///
    /// Wraps `Vec3` to distinguish spatial offsets from other vector
    /// quantities like position or velocity. Use `Displacement` to
    /// represent the difference between two points, a movement delta,
    /// or any directional distance.
    ///
    /// # Examples
    ///
    /// ```
    /// use bevy::math::Vec3;
    /// use bevy_kana::Displacement;
    ///
    /// let step = Displacement(Vec3::new(0.0, 0.0, -1.0));
    /// let double_step = step + step;
    /// assert_eq!(double_step.into_inner(), Vec3::new(0.0, 0.0, -2.0));
    /// ```
    Displacement, Vec3
);

semantic_newtype!(
    /// Rate of position change in 3D space.
    ///
    /// Wraps `Vec3` to distinguish velocity from other vector quantities.
    /// Scaling a `Velocity` by a time delta (e.g., `velocity * time_delta`)
    /// gives a per-frame displacement while preserving the `Velocity` type
    /// for the scaled result.
    ///
    /// # Examples
    ///
    /// ```
    /// use bevy::math::Vec3;
    /// use bevy_kana::Velocity;
    ///
    /// let velocity = Velocity(Vec3::new(10.0, 0.0, 0.0));
    /// let time_delta = 0.016;
    /// let frame_velocity = velocity * time_delta;
    /// assert!((frame_velocity.x - 0.16).abs() < 1e-6);
    /// ```
    Velocity, Vec3
);

/// A rotation in 3D space.
///
/// Wraps `Quat` with semantic meaning. Unlike the semantic `Vec3` types,
/// `Orientation` has custom arithmetic:
///
/// - `Orientation * Orientation → Orientation` (rotation composition)
/// - `Orientation * Vec3 → Vec3` (rotate a vector)
///
/// Other `Quat` methods are available through `Deref`.
///
/// # Examples
///
/// ```
/// use bevy::math::Quat;
/// use bevy::math::Vec3;
/// use bevy_kana::Orientation;
///
/// let orientation = Orientation::from(Quat::from_rotation_y(std::f32::consts::FRAC_PI_2));
/// let rotated = orientation * Vec3::X;
/// assert!((rotated - Vec3::NEG_Z).length() < 1e-6);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Default, Reflect)]
pub struct Orientation(pub Quat);

impl Orientation {
    /// Consumes `self` and returns the inner `Quat`.
    #[must_use]
    pub const fn into_inner(self) -> Quat { self.0 }

    /// Returns the inverse rotation.
    #[must_use]
    pub fn inverse(self) -> Self { Self(self.0.inverse()) }

    /// Spherical linear interpolation between `self` and `other`.
    #[must_use]
    pub fn slerp(self, other: Self, t: f32) -> Self { Self(self.0.slerp(other.0, t)) }

    /// Linear interpolation between `self` and `other`.
    ///
    /// Faster than [`Orientation::slerp`] but less accurate for large
    /// angular differences.
    #[must_use]
    pub fn lerp(self, other: Self, t: f32) -> Self { Self(self.0.lerp(other.0, t)) }
}

impl Deref for Orientation {
    type Target = Quat;

    fn deref(&self) -> &Quat { &self.0 }
}

impl From<Quat> for Orientation {
    fn from(value: Quat) -> Self { Self(value) }
}

impl From<Orientation> for Quat {
    fn from(value: Orientation) -> Self { value.0 }
}

/// Rotation composition: applying `rhs` then `self`.
impl Mul for Orientation {
    type Output = Self;

    fn mul(self, rhs: Self) -> Self { Self(self.0 * rhs.0) }
}

impl MulAssign for Orientation {
    fn mul_assign(&mut self, rhs: Self) { self.0 = self.0 * rhs.0; }
}

/// Rotates a vector by this orientation.
impl Mul<Vec3> for Orientation {
    type Output = Vec3;

    fn mul(self, rhs: Vec3) -> Vec3 { self.0 * rhs }
}

#[cfg(test)]
mod tests {
    use std::f32::consts::FRAC_PI_2;

    use super::*;

    #[test]
    fn displacement_add_returns_self() {
        let left_displacement = Displacement(Vec3::new(1.0, 0.0, 0.0));
        let right_displacement = Displacement(Vec3::new(0.0, 1.0, 0.0));
        let result = left_displacement + right_displacement;
        assert_eq!(result.into_inner(), Vec3::new(1.0, 1.0, 0.0));
    }

    #[test]
    fn displacement_from_into_roundtrip() {
        let vec3 = Vec3::new(1.0, 2.0, 3.0);
        let displacement = Displacement::from(vec3);
        let round_tripped_vec3: Vec3 = displacement.into();
        assert_eq!(vec3, round_tripped_vec3);
    }

    #[test]
    fn orientation_deref_provides_quat_access() {
        let orientation = Orientation::from(Quat::IDENTITY);
        assert!((orientation.w - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn orientation_from_into_roundtrip() {
        let quat = Quat::from_rotation_y(FRAC_PI_2);
        let orientation = Orientation::from(quat);
        let round_tripped_quat: Quat = orientation.into();
        assert_eq!(quat, round_tripped_quat);
    }

    #[test]
    fn orientation_inverse_undoes_rotation() {
        let orientation = Orientation::from(Quat::from_rotation_y(FRAC_PI_2));
        let inverse_orientation = orientation.inverse();
        let composed = orientation * inverse_orientation;
        let result = composed * Vec3::X;
        assert!((result - Vec3::X).length() < 1e-6);
    }

    #[test]
    fn orientation_rotate_vector() {
        let orientation = Orientation::from(Quat::from_rotation_y(FRAC_PI_2));
        let result = orientation * Vec3::X;
        assert!((result - Vec3::NEG_Z).length() < 1e-6);
    }

    #[test]
    fn orientation_rotation_composition() {
        let first_orientation = Orientation::from(Quat::from_rotation_y(FRAC_PI_2));
        let second_orientation = Orientation::from(Quat::from_rotation_y(FRAC_PI_2));
        let composed = first_orientation * second_orientation;
        let result = composed * Vec3::X;
        assert!((result - Vec3::NEG_X).length() < 1e-5);
    }

    #[test]
    fn orientation_slerp_halfway() {
        let start_orientation = Orientation::from(Quat::IDENTITY);
        let end_orientation = Orientation::from(Quat::from_rotation_y(FRAC_PI_2));
        let midpoint_orientation = start_orientation.slerp(end_orientation, 0.5);
        let result = midpoint_orientation * Vec3::X;
        let angle = result.angle_between(Vec3::X);
        assert!((angle - std::f32::consts::FRAC_PI_4).abs() < 1e-5);
    }

    #[test]
    fn position_add_assign() {
        let mut position = Position(Vec3::new(1.0, 0.0, 0.0));
        position += Position(Vec3::new(0.0, 1.0, 0.0));
        assert_eq!(position.into_inner(), Vec3::new(1.0, 1.0, 0.0));
    }

    #[test]
    fn position_add_returns_self() {
        let left_position = Position(Vec3::new(1.0, 0.0, 0.0));
        let right_position = Position(Vec3::new(0.0, 1.0, 0.0));
        let result = left_position + right_position;
        assert_eq!(result.into_inner(), Vec3::new(1.0, 1.0, 0.0));
    }

    #[test]
    fn position_deref_provides_vec3_access() {
        let position = Position(Vec3::new(1.0, 2.0, 3.0));
        assert!((position.x - 1.0).abs() < f32::EPSILON);
        assert!((position.length() - Vec3::new(1.0, 2.0, 3.0).length()).abs() < f32::EPSILON);
    }

    #[test]
    fn position_from_into_roundtrip() {
        let vec3 = Vec3::new(1.0, 2.0, 3.0);
        let position = Position::from(vec3);
        let round_tripped_vec3: Vec3 = position.into();
        assert_eq!(vec3, round_tripped_vec3);
    }

    #[test]
    fn position_neg() {
        let position = Position(Vec3::new(1.0, -2.0, 3.0));
        let result = -position;
        assert_eq!(result.into_inner(), Vec3::new(-1.0, 2.0, -3.0));
    }

    #[test]
    fn position_scalar_div() {
        let position = Position(Vec3::new(2.0, 4.0, 6.0));
        let result = position / 2.0;
        assert_eq!(result.into_inner(), Vec3::new(1.0, 2.0, 3.0));
    }

    #[test]
    fn position_scalar_mul() {
        let position = Position(Vec3::new(1.0, 2.0, 3.0));
        let result = position * 2.0;
        assert_eq!(result.into_inner(), Vec3::new(2.0, 4.0, 6.0));
    }

    #[test]
    fn velocity_add_combines_velocities() {
        let left_velocity = Velocity(Vec3::new(1.0, 0.0, 0.0));
        let right_velocity = Velocity(Vec3::new(0.0, 1.0, 0.0));
        let combined = left_velocity + right_velocity;
        assert_eq!(combined.into_inner(), Vec3::new(1.0, 1.0, 0.0));
    }

    #[test]
    fn velocity_scalar_mul_for_time_delta() {
        let velocity = Velocity(Vec3::new(10.0, 0.0, 0.0));
        let frame_velocity = velocity * 0.016;
        assert!((frame_velocity.x - 0.16).abs() < 1e-6);
    }
}
