//! Layout units, dimensions, and anchors.

use bevy::prelude::Reflect;

use super::constants::MIN_CUSTOM_MPU;

/// Physical unit for interpreting numeric dimensions.
///
/// Used by [`UnitConfig`](crate::UnitConfig) to define what "1.0" means for
/// layout dimensions and font sizes, and by
/// [`WorldTextStyle::with_unit`](crate::layout::TextProps::with_unit) to set
/// per-entity size units.
///
/// `Custom(f32)` is an escape hatch for any unit not covered by the named
/// variants — the value is meters per unit.
///
/// # Examples
///
/// ```ignore
/// Unit::Meters          // 1 unit = 1 meter (Bevy default)
/// Unit::Millimeters     // 1 unit = 1mm
/// Unit::Points          // 1 unit = 1 typographic point (1/72 inch)
/// Unit::Pixels          // 1 unit = 1 logical pixel on screen
/// Unit::Inches          // 1 unit = 1 inch
/// Unit::Custom(0.01)    // 1 unit = 1 centimeter
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq, Reflect)]
pub enum Unit {
    /// 1 unit = 1 meter. Bevy's default world-space convention.
    #[default]
    Meters,
    /// 1 unit = 1 millimeter (0.001 m).
    Millimeters,
    /// 1 unit = 1 typographic point (1/72 inch ≈ 0.000353 m).
    Points,
    /// 1 unit = 1 logical pixel on screen.
    ///
    /// In the layout engine, pixels map 1:1 with typographic points
    /// (`to_points()` returns 1.0). The actual screen-pixel behavior
    /// is provided by the camera system: screen-space panels
    /// use an orthographic camera where 1 world unit = 1 pixel;
    /// world-space panels can use per-frame `Transform` scaling.
    Pixels,
    /// 1 unit = 1 inch (0.0254 m).
    Inches,
    /// 1 unit = the given number of meters.
    Custom(f32),
}

impl Unit {
    /// Returns the conversion factor from this unit to meters.
    ///
    /// [`Unit::Custom`] values below [`Unit::Points`] (0.000353 m) are clamped.
    #[must_use]
    pub const fn meters_per_unit(self) -> f32 {
        match self {
            Self::Meters => 1.0,
            Self::Millimeters => 0.001,
            Self::Points | Self::Pixels => 0.0254 / 72.0,
            Self::Inches => 0.0254,
            Self::Custom(mpu) => {
                if mpu < MIN_CUSTOM_MPU {
                    MIN_CUSTOM_MPU
                } else {
                    mpu
                }
            },
        }
    }

    /// Returns the multiplier to convert a value in this unit to typographic points.
    #[must_use]
    pub const fn to_points(self) -> f32 { self.meters_per_unit() / Self::Points.meters_per_unit() }
}

/// A dimension with an optional unit.
///
/// Carries both the numeric value and the unit it's expressed in.
/// Created implicitly via `From` impls on [`Pt`](crate::Pt),
/// [`Mm`](crate::Mm), [`In`](crate::In), and bare `f32`.
///
/// - `Pt(24.0).into()` → value 24.0, unit Points
/// - `Mm(6.0).into()` → value 6.0, unit Millimeters
/// - `In(0.24).into()` → value 0.24, unit Inches
/// - `(24.0_f32).into()` → value 24.0, unit None (contextual default)
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Dimension {
    /// The numeric size value.
    pub value: f32,
    /// The unit the value is expressed in. `None` = contextual default.
    pub unit:  Option<Unit>,
}

impl Dimension {
    /// Resolves this dimension to points.
    ///
    /// If the dimension carries an explicit unit, converts using that
    /// unit's `to_points()`. Otherwise multiplies by `default_scale`
    /// (typically `layout_to_pts` or `font_to_pts`).
    #[must_use]
    pub fn to_points(self, default_scale: f32) -> f32 {
        match self.unit {
            Some(unit) => self.value * unit.to_points(),
            None => self.value * default_scale,
        }
    }

    /// Converts this dimension to world meters.
    ///
    /// Dimensions with an explicit unit convert via `unit.meters_per_unit()`.
    /// Dimensions without a unit (bare `f32`) use `default_meters_per_unit`
    /// (typically the panel's layout unit conversion factor).
    #[must_use]
    pub fn to_meters(self, default_meters_per_unit: f32) -> f32 {
        match self.unit {
            Some(unit) => self.value * unit.meters_per_unit(),
            None => self.value * default_meters_per_unit,
        }
    }
}

impl From<f32> for Dimension {
    fn from(value: f32) -> Self { Self { value, unit: None } }
}

/// Anchor point for standalone text positioning.
///
/// Determines which point of the text block's bounding box is placed
/// at the entity's [`Transform`] position.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Reflect)]
pub enum Anchor {
    /// Top-left corner at the transform position.
    TopLeft,
    /// Top-center at the transform position.
    TopCenter,
    /// Top-right corner at the transform position.
    TopRight,
    /// Center-left at the transform position.
    CenterLeft,
    /// Center of the text block at the transform position.
    #[default]
    Center,
    /// Center-right at the transform position.
    CenterRight,
    /// Bottom-left corner at the transform position.
    BottomLeft,
    /// Bottom-center at the transform position.
    BottomCenter,
    /// Bottom-right corner at the transform position.
    BottomRight,
}

impl Anchor {
    /// Returns the offset from the top-left corner as a fraction of (width, height).
    ///
    /// For `TopLeft` this is (0, 0). For `Center` it's (0.5, 0.5).
    /// Multiply by the actual width/height to get the offset in whatever units.
    #[must_use]
    pub const fn offset_fraction(self) -> (f32, f32) {
        let x = match self {
            Self::TopLeft | Self::CenterLeft | Self::BottomLeft => 0.0,
            Self::TopCenter | Self::Center | Self::BottomCenter => 0.5,
            Self::TopRight | Self::CenterRight | Self::BottomRight => 1.0,
        };
        let y = match self {
            Self::TopLeft | Self::TopCenter | Self::TopRight => 0.0,
            Self::CenterLeft | Self::Center | Self::CenterRight => 0.5,
            Self::BottomLeft | Self::BottomCenter | Self::BottomRight => 1.0,
        };
        (x, y)
    }

    /// Returns the anchor offset for a bounding box of the given size.
    #[must_use]
    pub fn offset(self, width: f32, height: f32) -> (f32, f32) {
        let (fx, fy) = self.offset_fraction();
        (width * fx, height * fy)
    }
}
