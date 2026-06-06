use std::ops::Add;
use std::ops::AddAssign;
use std::ops::Div;
use std::ops::DivAssign;
use std::ops::Mul;
use std::ops::MulAssign;
use std::ops::Neg;
use std::ops::Sub;
use std::ops::SubAssign;

use bevy::math::Vec3;
use bevy::prelude::Deref;
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
        #[derive(Debug, Clone, Copy, PartialEq, Default, Deref, Reflect)]
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

            fn add(self, right_hand_side: Self) -> Self {
                Self(self.0 + right_hand_side.0)
            }
        }

        impl AddAssign for $name {
            fn add_assign(&mut self, right_hand_side: Self) {
                self.0 += right_hand_side.0;
            }
        }

        impl Sub for $name {
            type Output = Self;

            fn sub(self, right_hand_side: Self) -> Self {
                Self(self.0 - right_hand_side.0)
            }
        }

        impl SubAssign for $name {
            fn sub_assign(&mut self, right_hand_side: Self) {
                self.0 -= right_hand_side.0;
            }
        }

        impl Mul<f32> for $name {
            type Output = Self;

            fn mul(self, right_hand_side: f32) -> Self {
                Self(self.0 * right_hand_side)
            }
        }

        impl MulAssign<f32> for $name {
            fn mul_assign(&mut self, right_hand_side: f32) {
                self.0 *= right_hand_side;
            }
        }

        impl Div<f32> for $name {
            type Output = Self;

            fn div(self, right_hand_side: f32) -> Self {
                Self(self.0 / right_hand_side)
            }
        }

        impl DivAssign<f32> for $name {
            fn div_assign(&mut self, right_hand_side: f32) {
                self.0 /= right_hand_side;
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

            fn add(self, right_hand_side: $inner) -> Self {
                Self(self.0 + right_hand_side)
            }
        }

        impl AddAssign<$inner> for $name {
            fn add_assign(&mut self, right_hand_side: $inner) {
                self.0 += right_hand_side;
            }
        }

        impl Sub<$inner> for $name {
            type Output = Self;

            fn sub(self, right_hand_side: $inner) -> Self {
                Self(self.0 - right_hand_side)
            }
        }

        impl SubAssign<$inner> for $name {
            fn sub_assign(&mut self, right_hand_side: $inner) {
                self.0 -= right_hand_side;
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

#[cfg(test)]
mod tests {
    use super::*;

    // displacement fixtures
    const DISPLACEMENT_LEFT: Displacement = Displacement::new(1.0, 0.0, 0.0);
    const DISPLACEMENT_RIGHT: Displacement = Displacement::new(0.0, 1.0, 0.0);
    const DISPLACEMENT_ROUNDTRIP_VECTOR: Vec3 = Vec3::new(1.0, 2.0, 3.0);

    // position fixtures
    const POSITION: Position = Position::new(POSITION_X, POSITION_Y, POSITION_Z);
    const POSITION_LEFT: Position = Position::new(1.0, 0.0, 0.0);
    const POSITION_NEGATED_INPUT: Position = Position::new(1.0, -2.0, 3.0);
    const POSITION_RIGHT: Position = Position::new(0.0, 1.0, 0.0);
    const POSITION_WITH_SCALE_INPUT: Position = Position::new(2.0, 4.0, 6.0);
    const POSITION_X: f32 = 1.0;
    const POSITION_Y: f32 = 2.0;
    const POSITION_Z: f32 = 3.0;
    const SCALE_FACTOR: f32 = 2.0;

    // velocity fixtures
    const FRAME_TIME_DELTA: f32 = 0.016;
    const LEFT_VELOCITY: Velocity = Velocity::new(1.0, 0.0, 0.0);
    const RIGHT_VELOCITY: Velocity = Velocity::new(0.0, 1.0, 0.0);
    const SCALED_VELOCITY: Velocity = Velocity::new(10.0, 0.0, 0.0);
    const VELOCITY_PER_FRAME_TOLERANCE: f32 = 1e-6;

    #[test]
    fn displacement_add_returns_self() {
        let left_displacement = DISPLACEMENT_LEFT;
        let right_displacement = DISPLACEMENT_RIGHT;
        let result = left_displacement + right_displacement;
        assert_eq!(
            result.into_inner(),
            DISPLACEMENT_LEFT.into_inner() + DISPLACEMENT_RIGHT.into_inner()
        );
    }

    #[test]
    fn displacement_from_into_roundtrip() {
        let vec3 = DISPLACEMENT_ROUNDTRIP_VECTOR;
        let displacement = Displacement::from(vec3);
        let round_tripped_vec3: Vec3 = displacement.into();
        assert_eq!(vec3, round_tripped_vec3);
    }

    #[test]
    fn position_add_assign() {
        let mut position = POSITION_LEFT;
        position += POSITION_RIGHT;
        assert_eq!(
            position.into_inner(),
            POSITION_LEFT.into_inner() + POSITION_RIGHT.into_inner()
        );
    }

    #[test]
    fn position_add_returns_self() {
        let left_position = POSITION_LEFT;
        let right_position = POSITION_RIGHT;
        let result = left_position + right_position;
        assert_eq!(
            result.into_inner(),
            POSITION_LEFT.into_inner() + POSITION_RIGHT.into_inner()
        );
    }

    #[test]
    fn position_deref_provides_vec3_access() {
        let position = POSITION;
        assert!((position.x - POSITION_X).abs() < f32::EPSILON);
        assert!(
            (position.length() - Vec3::new(POSITION_X, POSITION_Y, POSITION_Z).length()).abs()
                < f32::EPSILON
        );
    }

    #[test]
    fn position_from_into_roundtrip() {
        let vec3 = POSITION.into_inner();
        let position = Position::from(vec3);
        let round_tripped_vec3: Vec3 = position.into();
        assert_eq!(vec3, round_tripped_vec3);
    }

    #[test]
    fn position_neg() {
        let position = POSITION_NEGATED_INPUT;
        let result = -position;
        assert_eq!(result.into_inner(), -POSITION_NEGATED_INPUT.into_inner());
    }

    #[test]
    fn position_scalar_div() {
        let position = POSITION_WITH_SCALE_INPUT;
        let result = position / SCALE_FACTOR;
        assert_eq!(result.into_inner(), POSITION.into_inner());
    }

    #[test]
    fn position_scalar_mul() {
        let position = POSITION;
        let result = position * SCALE_FACTOR;
        assert_eq!(result.into_inner(), POSITION.into_inner() * SCALE_FACTOR);
    }

    #[test]
    fn velocity_add_combines_velocities() {
        let left_velocity = LEFT_VELOCITY;
        let right_velocity = RIGHT_VELOCITY;
        let combined = left_velocity + right_velocity;
        assert_eq!(
            combined.into_inner(),
            LEFT_VELOCITY.into_inner() + RIGHT_VELOCITY.into_inner()
        );
    }

    #[test]
    fn velocity_scalar_mul_for_time_delta() {
        let velocity = SCALED_VELOCITY;
        let frame_velocity = velocity * FRAME_TIME_DELTA;
        assert!(
            SCALED_VELOCITY
                .x
                .mul_add(-FRAME_TIME_DELTA, frame_velocity.x)
                .abs()
                < VELOCITY_PER_FRAME_TOLERANCE
        );
    }
}
