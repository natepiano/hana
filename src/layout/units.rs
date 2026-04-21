//! Layout units, dimensions, anchors, and typed dimensional wrappers.

use bevy::prelude::Reflect;

use super::constants::MIN_CUSTOM_MPU;
use super::constants::PIXELS_PER_INCH;

/// Physical unit for interpreting numeric dimensions.
///
/// Used by [`CascadeDefaults`](crate::CascadeDefaults) to define what "1.0"
/// means for layout dimensions and font sizes, and by
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
            Self::Points => 0.0254 / 72.0,
            // 96 DPI convention: 1 point = 96/72 pixels ≈ 1.333 pixels.
            Self::Pixels => 0.0254 / PIXELS_PER_INCH,
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
/// Created implicitly via `From` impls on [`Pt`], [`Mm`], [`In`], and bare `f32`.
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
/// at the entity's [`Transform`](bevy::prelude::Transform) position.
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

// ── Dimensional newtypes ─────────────────────────────────────────────────────

/// A value in typographic points (1/72 inch ≈ 0.353 mm).
#[derive(Clone, Copy, Debug)]
pub struct Pt(pub f32);

/// A value in millimeters.
#[derive(Clone, Copy, Debug)]
pub struct Mm(pub f32);

/// A value in inches.
#[derive(Clone, Copy, Debug)]
pub struct In(pub f32);

/// A value in logical pixels.
///
/// On screen-space panels, pixels map 1:1 to
/// on-screen logical pixels. On world-space panels, the system resolves
/// pixel dimensions per-frame using the active camera's projection.
#[derive(Clone, Copy, Debug)]
pub struct Px(pub f32);

impl From<Pt> for f32 {
    fn from(v: Pt) -> Self { v.0 * Unit::Points.meters_per_unit() }
}

impl From<Mm> for f32 {
    fn from(v: Mm) -> Self { v.0 * Unit::Millimeters.meters_per_unit() }
}

impl From<In> for f32 {
    fn from(v: In) -> Self { v.0 * Unit::Inches.meters_per_unit() }
}

impl From<Pt> for Dimension {
    fn from(v: Pt) -> Self {
        Self {
            value: v.0,
            unit:  Some(Unit::Points),
        }
    }
}

impl From<Mm> for Dimension {
    fn from(v: Mm) -> Self {
        Self {
            value: v.0,
            unit:  Some(Unit::Millimeters),
        }
    }
}

impl From<In> for Dimension {
    fn from(v: In) -> Self {
        Self {
            value: v.0,
            unit:  Some(Unit::Inches),
        }
    }
}

impl From<Px> for Dimension {
    fn from(v: Px) -> Self {
        Self {
            value: v.0,
            unit:  Some(Unit::Pixels),
        }
    }
}

// ── DimensionMatch trait ────────────────────────────────────────────────────

/// Marker trait for values accepted by `.size(w, h)` methods.
///
/// The `.size(...)` APIs on
/// [`DiegeticPanelBuilder`](crate::DiegeticPanelBuilder)
/// and [`El`](crate::El) require both arguments to have the same concrete type:
///
/// - `size(100.0, 50.0)` works
/// - `size(Mm(210.0), Mm(297.0))` works
/// - `size(Mm(210.0), In(11.0))` is a compile error
///
/// Use `.width(...)` and `.height(...)` separately when you intentionally want
/// different unit types on each axis.
pub trait DimensionMatch: Into<Dimension> {}

impl DimensionMatch for f32 {}
impl DimensionMatch for Dimension {}
impl DimensionMatch for Pt {}
impl DimensionMatch for Mm {}
impl DimensionMatch for In {}
impl DimensionMatch for Px {}

// ── HasUnit trait ────────────────────────────────────────────────────────────

/// Trait for types that carry a compile-time unit association.
///
/// Implemented by the dimensional newtypes ([`Mm`], [`In`], [`Pt`], [`Px`]).
/// Used by the generic [`PanelSize`] implementation for same-unit tuples:
/// `impl<U: HasUnit> PanelSize for (U, U)`.
pub trait HasUnit {
    /// The [`Unit`] this type represents.
    const UNIT: Unit;

    /// Returns the raw numeric value.
    fn value(self) -> f32;
}

impl HasUnit for Mm {
    const UNIT: Unit = Unit::Millimeters;

    fn value(self) -> f32 { self.0 }
}

impl HasUnit for In {
    const UNIT: Unit = Unit::Inches;

    fn value(self) -> f32 { self.0 }
}

impl HasUnit for Pt {
    const UNIT: Unit = Unit::Points;

    fn value(self) -> f32 { self.0 }
}

impl HasUnit for Px {
    const UNIT: Unit = Unit::Pixels;

    fn value(self) -> f32 { self.0 }
}

// ── Paper sizes ──────────────────────────────────────────────────────────────

/// Standard paper and card sizes.
///
/// Each variant stores its dimensions in millimeters internally and
/// converts to meters (or any [`Unit`]) on request. Implements
/// [`PanelSize`] so it can be passed directly to
/// [`DiegeticPanelBuilder::size`](crate::DiegeticPanelBuilder::size).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Reflect)]
pub enum PaperSize {
    // ── ISO A series ─────────────────────────────────────────────────
    /// A0 — 841 × 1189 mm.
    A0,
    /// A1 — 594 × 841 mm.
    A1,
    /// A2 — 420 × 594 mm.
    A2,
    /// A3 — 297 × 420 mm.
    A3,
    /// A4 — 210 × 297 mm.
    A4,
    /// A5 — 148 × 210 mm.
    A5,
    /// A6 — 105 × 148 mm.
    A6,
    /// A7 — 74 × 105 mm.
    A7,
    /// A8 — 52 × 74 mm.
    A8,

    // ── ISO B series ─────────────────────────────────────────────────
    /// B0 — 1000 × 1414 mm.
    B0,
    /// B1 — 707 × 1000 mm.
    B1,
    /// B2 — 500 × 707 mm.
    B2,
    /// B3 — 353 × 500 mm.
    B3,
    /// B4 — 250 × 353 mm.
    B4,
    /// B5 — 176 × 250 mm.
    B5,

    // ── North American ───────────────────────────────────────────────
    /// US Letter — 8.5 × 11 inches (215.9 × 279.4 mm).
    USLetter,
    /// US Legal — 8.5 × 14 inches (215.9 × 355.6 mm).
    USLegal,
    /// US Ledger / Tabloid — 11 × 17 inches (279.4 × 431.8 mm).
    USLedger,
    /// US Executive — 7.25 × 10.5 inches (184.2 × 266.7 mm).
    USExecutive,

    // ── Cards ────────────────────────────────────────────────────────
    /// US Business Card — 3.5 × 2 inches (88.9 × 50.8 mm).
    BusinessCard,
    /// Index Card 3 × 5 inches (76.2 × 127.0 mm).
    IndexCard3x5,
    /// Index Card 4 × 6 inches (101.6 × 152.4 mm).
    IndexCard4x6,
    /// Index Card 5 × 8 inches (127.0 × 203.2 mm).
    IndexCard5x8,

    // ── Photo ────────────────────────────────────────────────────────
    /// Photo 4 × 6 inches (101.6 × 152.4 mm).
    Photo4x6,
    /// Photo 5 × 7 inches (127.0 × 177.8 mm).
    Photo5x7,
    /// Photo 8 × 10 inches (203.2 × 254.0 mm).
    Photo8x10,

    // ── Poster ───────────────────────────────────────────────────────
    /// Poster 18 × 24 inches (457.2 × 609.6 mm).
    Poster18x24,
    /// Poster 24 × 36 inches (609.6 × 914.4 mm).
    Poster24x36,
}

impl PaperSize {
    /// Width in millimeters (shorter dimension).
    #[must_use]
    pub const fn width_mm(self) -> f32 {
        match self {
            Self::A0 => 841.0,
            Self::A1 => 594.0,
            Self::A2 => 420.0,
            Self::A3 => 297.0,
            Self::A4 => 210.0,
            Self::A5 => 148.0,
            Self::A6 => 105.0,
            Self::A7 => 74.0,
            Self::A8 => 52.0,
            Self::B0 => 1000.0,
            Self::B1 => 707.0,
            Self::B2 => 500.0,
            Self::B3 => 353.0,
            Self::B4 => 250.0,
            Self::B5 => 176.0,
            Self::USLetter | Self::USLegal => 215.9,
            Self::USLedger => 279.4,
            Self::USExecutive => 184.15,
            Self::BusinessCard => 88.9,
            Self::IndexCard3x5 => 76.2,
            Self::IndexCard4x6 | Self::Photo4x6 => 101.6,
            Self::IndexCard5x8 | Self::Photo5x7 => 127.0,
            Self::Photo8x10 => 203.2,
            Self::Poster18x24 => 457.2,
            Self::Poster24x36 => 609.6,
        }
    }

    /// Height in millimeters (longer dimension).
    #[must_use]
    pub const fn height_mm(self) -> f32 {
        match self {
            Self::A0 => 1189.0,
            Self::A1 => 841.0,
            Self::A2 => 594.0,
            Self::A3 => 420.0,
            Self::A4 => 297.0,
            Self::A5 => 210.0,
            Self::A6 => 148.0,
            Self::A7 => 105.0,
            Self::A8 => 74.0,
            Self::B0 => 1414.0,
            Self::B1 => 1000.0,
            Self::B2 => 707.0,
            Self::B3 => 500.0,
            Self::B4 => 353.0,
            Self::B5 => 250.0,
            Self::USLetter => 279.4,
            Self::USLegal => 355.6,
            Self::USLedger => 431.8,
            Self::USExecutive => 266.7,
            Self::BusinessCard => 50.8,
            Self::IndexCard3x5 => 127.0,
            Self::IndexCard4x6 | Self::Photo4x6 => 152.4,
            Self::IndexCard5x8 => 203.2,
            Self::Photo5x7 => 177.8,
            Self::Photo8x10 => 254.0,
            Self::Poster18x24 => 609.6,
            Self::Poster24x36 => 914.4,
        }
    }

    /// The natural unit for this paper size.
    ///
    /// ISO and metric sizes return [`Unit::Millimeters`].
    /// North American, card, photo, and poster sizes return [`Unit::Inches`].
    #[must_use]
    pub const fn native_unit(self) -> Unit {
        match self {
            // ISO series — metric
            Self::A0
            | Self::A1
            | Self::A2
            | Self::A3
            | Self::A4
            | Self::A5
            | Self::A6
            | Self::A7
            | Self::A8
            | Self::B0
            | Self::B1
            | Self::B2
            | Self::B3
            | Self::B4
            | Self::B5 => Unit::Millimeters,

            // North American, cards, photos, posters — imperial
            Self::USLetter
            | Self::USLegal
            | Self::USLedger
            | Self::USExecutive
            | Self::BusinessCard
            | Self::IndexCard3x5
            | Self::IndexCard4x6
            | Self::IndexCard5x8
            | Self::Photo4x6
            | Self::Photo5x7
            | Self::Photo8x10
            | Self::Poster18x24
            | Self::Poster24x36 => Unit::Inches,
        }
    }

    /// Width in the given unit.
    ///
    /// Conversions involve floating-point arithmetic and may accumulate
    /// rounding error. For maximum precision, use [`width_mm`](Self::width_mm)
    /// directly for millimeter values.
    #[must_use]
    pub fn width_as<U: HasUnit>(&self) -> f32 {
        self.width_mm() * Unit::Millimeters.meters_per_unit() / U::UNIT.meters_per_unit()
    }

    /// Height in the given unit.
    ///
    /// Conversions involve floating-point arithmetic and may accumulate
    /// rounding error. For maximum precision, use [`height_mm`](Self::height_mm)
    /// directly for millimeter values.
    #[must_use]
    pub fn height_as<U: HasUnit>(&self) -> f32 {
        self.height_mm() * Unit::Millimeters.meters_per_unit() / U::UNIT.meters_per_unit()
    }

    /// Width in meters.
    #[deprecated(
        since = "0.1.0",
        note = "use `width_as::<Mm>()` or the native unit accessor"
    )]
    #[must_use]
    pub const fn width(self) -> f32 { self.width_mm() * 0.001 }

    /// Height in meters.
    #[deprecated(
        since = "0.1.0",
        note = "use `height_as::<Mm>()` or the native unit accessor"
    )]
    #[must_use]
    pub const fn height(self) -> f32 { self.height_mm() * 0.001 }

    /// Width in the given unit.
    #[deprecated(since = "0.1.0", note = "use `width_as::<U>()` instead")]
    #[must_use]
    pub fn width_in(self, unit: Unit) -> f32 {
        self.width_mm() * Unit::Millimeters.meters_per_unit() / unit.meters_per_unit()
    }

    /// Height in the given unit.
    #[deprecated(since = "0.1.0", note = "use `height_as::<U>()` instead")]
    #[must_use]
    pub fn height_in(self, unit: Unit) -> f32 {
        self.height_mm() * Unit::Millimeters.meters_per_unit() / unit.meters_per_unit()
    }

    /// Returns `(width, height)` in portrait orientation (taller than wide).
    ///
    /// If the paper is already portrait, returns as-is. If landscape
    /// (wider than tall), swaps width and height. Returns [`Mm`]
    /// newtypes so the result works directly with
    /// [`DiegeticPanelBuilder::size`](crate::DiegeticPanelBuilder::size).
    #[must_use]
    pub fn portrait(self) -> (Mm, Mm) {
        let (w, h) = (self.width_mm(), self.height_mm());
        if w > h {
            (Mm(h), Mm(w))
        } else {
            (Mm(w), Mm(h))
        }
    }

    /// Returns `(width, height)` in landscape orientation (wider than tall).
    ///
    /// If the paper is already landscape, returns as-is. If portrait
    /// (taller than wide), swaps width and height. Returns [`Mm`]
    /// newtypes so the result works directly with
    /// [`DiegeticPanelBuilder::size`](crate::DiegeticPanelBuilder::size).
    #[must_use]
    pub fn landscape(self) -> (Mm, Mm) {
        let (w, h) = (self.width_mm(), self.height_mm());
        if h > w {
            (Mm(h), Mm(w))
        } else {
            (Mm(w), Mm(h))
        }
    }
}

// ── PanelSize trait ──────────────────────────────────────────────────────────

/// Trait for types that can provide panel dimensions with their unit.
///
/// Returns `(width, height, unit)` where width and height are in the
/// returned unit's coordinate space.
///
/// Implemented by:
/// - [`PaperSize`] — standard paper/card sizes in their natural unit
/// - Same-unit tuples like `(Mm(210.0), Mm(297.0))` — compiler rejects mixed units
/// - Bare `(f32, f32)` — defaults to [`Unit::Meters`]
pub trait PanelSize {
    /// Returns `(width, height, unit)`.
    fn dimensions(self) -> (f32, f32, Unit);
}

impl PanelSize for PaperSize {
    fn dimensions(self) -> (f32, f32, Unit) {
        let unit = self.native_unit();
        match unit {
            Unit::Millimeters => (self.width_mm(), self.height_mm(), unit),
            Unit::Inches => {
                let w = self.width_mm() * Unit::Millimeters.meters_per_unit()
                    / Unit::Inches.meters_per_unit();
                let h = self.height_mm() * Unit::Millimeters.meters_per_unit()
                    / Unit::Inches.meters_per_unit();
                (w, h, unit)
            },
            _ => (self.width_mm(), self.height_mm(), Unit::Millimeters),
        }
    }
}

/// Same-unit tuples: `(Mm(210.0), Mm(297.0))`, `(In(8.5), In(11.0))`, etc.
/// Mixed-unit tuples like `(Mm(210.0), In(11.0))` are a compile error.
impl<U: HasUnit> PanelSize for (U, U) {
    fn dimensions(self) -> (f32, f32, Unit) { (self.0.value(), self.1.value(), U::UNIT) }
}

/// Bare `(f32, f32)` tuples default to [`Unit::Meters`] (Bevy's world space).
impl PanelSize for (f32, f32) {
    fn dimensions(self) -> (f32, f32, Unit) { (self.0, self.1, Unit::Meters) }
}

// ── Error types ──────────────────────────────────────────────────────────────

/// Error returned when panel dimensions are zero or negative.
///
/// Emitted by
/// [`DiegeticPanelBuilder::build`](crate::DiegeticPanelBuilder) when
/// both axes are fixed-size and at least one is `<= 0.0`. Dynamic axes
/// ([`Fit`](crate::Fit) / [`Percent`](crate::Percent) /
/// [`Grow`](crate::Grow)) start at `0.0` and resolve later; they do not
/// trip this check.
///
/// # Why a runtime check rather than a compile-time one?
///
/// The rest of the sizing API catches invalid states at compile time via
/// [`PanelSizing`](crate::PanelSizing) and
/// [`CompatibleUnits`](crate::CompatibleUnits). Zero and negative sizes
/// are the single remaining runtime check, and that's a deliberate
/// tradeoff:
///
/// - **Floats can't carry type-level non-zero proofs.** Rust has no const generics for `f32`, so
///   there is no `NonZero<f32>` to build a `NonZeroDimension` wrapper on top of. Any "no zero"
///   enforcement requires a fallible constructor like `Px::new(600.0) -> Option<Px>` — that's still
///   a runtime check, just relocated earlier in the pipeline.
/// - **`Px(0.0)` is useful.** It's a legitimate "no minimum" sentinel in
///   [`GrowRange`](crate::GrowRange) / [`FitRange`](crate::FitRange) and a "no cap" sentinel in
///   [`GrowMax`](crate::GrowMax) / [`FitMax`](crate::FitMax). Banning `Px(0.0)` at construction
///   forces a parallel type for "zero-ok" arguments or awkward `Unlimited` enum variants, which is
///   worse ergonomics than one `Result` at `.build()`.
/// - **`const fn` panic-on-zero** would catch only literal zero in source; any runtime-computed
///   value (config file, user input, arithmetic) still needs the same check, so it doesn't
///   eliminate the runtime path.
///
/// If you want the check to fail earlier (e.g., at the point you read a
/// user-supplied value), validate the float yourself before handing it
/// to [`Px`](crate::Px) / [`Mm`](crate::Mm) / [`Pt`](crate::Pt) /
/// [`In`](crate::layout::In).
#[derive(Debug)]
pub struct InvalidSize {
    /// The invalid width value.
    pub width:  f32,
    /// The invalid height value.
    pub height: f32,
}

impl core::fmt::Display for InvalidSize {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "panel dimensions must be positive, got {}×{}",
            self.width, self.height
        )
    }
}

impl core::error::Error for InvalidSize {}
