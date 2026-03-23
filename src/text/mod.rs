//! Font loading, text measurement, and MSDF atlas generation.
//!
//! [`FontRegistry`] manages font loading via parley's `FontContext`.
//! The registry embeds `JetBrains Mono` as the default font and provides
//! a [`MeasureTextFn`](crate::MeasureTextFn) backed by parley's layout engine.
//!
//! [`Font`] provides access to font-level typographic metrics via
//! [`Font::metrics`], which returns a [`FontMetrics`] struct scaled to
//! any requested font size.
//!
//! [`MsdfAtlas`] packs rasterized MSDF glyph bitmaps into a texture atlas
//! for GPU rendering.

mod atlas;
mod font;
mod font_registry;
mod measurer;
#[cfg(test)]
mod msdf_parity_tests;
mod msdf_rasterizer;
#[cfg(test)]
mod msdf_rasterizer_tests;
#[cfg(test)]
mod parley_measurer_tests;

pub(super) use atlas::GlyphKey;
pub(super) use atlas::MsdfAtlas;
pub use font::Font;
pub use font::FontMetrics;
#[cfg(feature = "typography_overlay")]
pub use font::GlyphBounds;
#[cfg(feature = "typography_overlay")]
pub use font::GlyphTypographyMetrics;
pub(super) use font_registry::EMBEDDED_FONT;
pub use font_registry::FontId;
pub use font_registry::FontRegistry;
pub(super) use measurer::create_parley_measurer;
pub(super) use msdf_rasterizer::DEFAULT_CANONICAL_SIZE;
pub(super) use msdf_rasterizer::DEFAULT_GLYPH_PADDING;
pub(super) use msdf_rasterizer::DEFAULT_SDF_RANGE;
