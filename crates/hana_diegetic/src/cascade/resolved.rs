//! Diegetic cascade attribute value types and their root defaults.

use core::mem::size_of;

use bevy::asset::Handle;
use bevy::log::warn_once;
use bevy::pbr::StandardMaterial;
use bevy::prelude::*;

use super::constants::CASCADE_ATTRIBUTE_BYTES;
use crate::layout::GlyphShadowMode;
use crate::layout::Lighting;
use crate::layout::ShadowCasting;
use crate::layout::Sidedness;
use crate::layout::Unit;
use crate::render::AntiAlias;
use crate::render::HairlineFade;

macro_rules! cascade_attribute {
    // Joins an already-declared value type (one whose own name is the
    // attribute, e.g. `AntiAlias`) to the cascade instead of minting a
    // wrapper struct. The type must derive `Clone`, `PartialEq`, `Debug`, and
    // `Reflect`.
    (existing $name:ty, default = $default:expr) => {
        impl $crate::cascade::resolved::CascadeRoot for $name {
            fn root_default() -> Self { $default }
        }
    };

    ($(#[$meta:meta])* $name:ident($value:ty), default = $default:expr, eq) => {
        $(#[$meta])*
        #[derive(Clone, Copy, PartialEq, Eq, Debug, Reflect)]
        pub struct $name(pub $value);

        impl $crate::cascade::resolved::CascadeRoot for $name {
            fn root_default() -> Self { $name($default) }
        }
    };

    ($(#[$meta:meta])* $name:ident($value:ty), default = $default:expr) => {
        $(#[$meta])*
        #[derive(Clone, Copy, PartialEq, Debug, Reflect)]
        pub struct $name(pub $value);

        impl $crate::cascade::resolved::CascadeRoot for $name {
            fn root_default() -> Self { $name($default) }
        }
    };
}

cascade_attribute!(
    /// Text alpha-mode cascade attribute.
    TextAlpha(AlphaMode),
    default = AlphaMode::Blend,
    eq
);
cascade_attribute!(
    /// Font-unit cascade attribute.
    FontUnit(Unit),
    default = Unit::Meters
);
cascade_attribute!(
    /// HDR text coverage-bias cascade attribute.
    ///
    /// `0.0` leaves analytic glyph coverage unchanged. Positive values make
    /// fractional glyph-edge pixels more opaque, which can compensate for dark
    /// text looking too thin when an HDR camera renders into a float target.
    /// Negative values make fractional edges thinner. Use this for text that
    /// degrades under HDR, especially dark text on light backgrounds; avoid
    /// applying it broadly to light text on dark backgrounds unless that scene
    /// has been tuned, because the same compensation can make those glyphs look
    /// heavier.
    HdrTextCoverageBias(f32),
    default = 0.0
);

const HDR_TEXT_COVERAGE_BIAS_MIN: f32 = -4.0;
const HDR_TEXT_COVERAGE_BIAS_MAX: f32 = 4.0;

impl HdrTextCoverageBias {
    /// No HDR coverage compensation; the shader uses analytic text coverage
    /// unchanged.
    pub(crate) const NO_BIAS: Self = Self(0.0);

    /// Value sent to `PathRenderRecord::text_coverage_bias`.
    ///
    /// The public authored value is intentionally plain `f32` so it can be
    /// tuned live, including through reflection. The shader path clamps it to a
    /// bounded signed transfer and treats non-finite input as no compensation.
    #[must_use]
    pub(crate) fn shader_value(self) -> f32 {
        if self.0.is_finite() {
            self.0
                .clamp(HDR_TEXT_COVERAGE_BIAS_MIN, HDR_TEXT_COVERAGE_BIAS_MAX)
        } else {
            warn_once!(
                "HdrTextCoverageBias value {} is not finite; rendering text without HDR coverage compensation",
                self.0
            );
            0.0
        }
    }
}

/// Source-material handle cascade for SDF backgrounds, borders, and element surfaces.
///
/// `SdfMaterial` is authored source-material identity. It is not the batched
/// `SdfExtendedMaterial` render asset and not the migration-only
/// `LegacySdfExtendedMaterial` render asset.
#[derive(Clone, PartialEq, Eq, Debug, Reflect)]
pub struct SdfMaterial(pub Handle<StandardMaterial>);

impl CascadeRoot for SdfMaterial {
    fn root_default() -> Self { Self(Handle::default()) }
}

const _: () = assert!(size_of::<SdfMaterial>() <= CASCADE_ATTRIBUTE_BYTES);

/// Source-material handle cascade for text runs.
///
/// `TextMaterial` resolves the authored `StandardMaterial` handle before
/// analytic text projection. It is not a Bevy render material asset type.
#[derive(Clone, PartialEq, Eq, Debug, Reflect)]
pub struct TextMaterial(pub Handle<StandardMaterial>);

impl CascadeRoot for TextMaterial {
    fn root_default() -> Self { Self(Handle::default()) }
}

const _: () = assert!(size_of::<TextMaterial>() <= CASCADE_ATTRIBUTE_BYTES);

/// Source-material handle cascade for panel-shape primitives.
///
/// `ShapeMaterial` resolves the authored `StandardMaterial` handle before
/// analytic panel-shape projection. It is not a Bevy render material asset type.
#[derive(Clone, PartialEq, Eq, Debug, Reflect)]
pub struct ShapeMaterial(pub Handle<StandardMaterial>);

impl CascadeRoot for ShapeMaterial {
    fn root_default() -> Self { Self(Handle::default()) }
}

const _: () = assert!(size_of::<ShapeMaterial>() <= CASCADE_ATTRIBUTE_BYTES);

// Lighting cascade attribute. Global default is `Lit` (world text); the
// screen-panel construction bridge overrides it to `Unlit`. Consumed by both
// glyph runs and panel lines.
cascade_attribute!(existing Lighting, default = Lighting::Lit);
// Diegetic shadow-casting cascade attribute. Global default follows Bevy mesh
// behavior: rendered content casts shadows unless a local override opts out.
cascade_attribute!(existing ShadowCasting, default = ShadowCasting::On);
// Glyph-shadow silhouette cascade attribute. Text casts its glyph silhouette
// when shadow casting is enabled unless a local override opts out.
cascade_attribute!(existing GlyphShadowMode, default = GlyphShadowMode::Cast);
// Sidedness cascade attribute. Global default is `BothSides` (world text);
// the screen-panel construction bridge overrides it to `FrontOnly`. Consumed by
// both glyph runs and panel lines.
cascade_attribute!(existing Sidedness, default = Sidedness::BothSides);
// Anti-alias mode cascade attribute. The `AntiAlias` resource is the
// authored global; `sync_anti_alias` mirrors it into
// `CascadeDefault<AntiAlias>` as the cascade root default.
cascade_attribute!(existing AntiAlias, default = AntiAlias::Both);
// Hairline-fade cascade attribute. `HairlineWidth::fade` is the authored
// global; `sync_hairline_fade` mirrors it into `CascadeDefault<HairlineFade>`
// as the cascade root default.
cascade_attribute!(existing HairlineFade, default = HairlineFade::Full);

pub(crate) trait CascadeRoot: bevy_kana::CascadeAttribute {
    fn root_default() -> Self;
}
