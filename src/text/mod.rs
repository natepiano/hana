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
mod atlas_config;
mod constants;
mod font;
mod font_loader;
mod font_registry;
mod measurer;
#[cfg(test)]
mod msdf_parity_tests;
mod msdf_rasterizer;
#[cfg(test)]
mod msdf_rasterizer_tests;
#[cfg(test)]
mod parley_measurer_tests;
mod text_plugin;

pub use atlas::GlyphKey;
pub use atlas::GlyphLookup;
pub use atlas::GlyphMetrics;
pub use atlas::MsdfAtlas;
pub use atlas_config::AtlasConfig;
pub use atlas_config::GlyphWorkerThreads;
pub use atlas_config::RasterQuality;
pub use constants::EMBEDDED_FONT;
pub use font::Font;
pub use font::FontMetrics;
#[cfg(feature = "typography_overlay")]
pub use font::GlyphBounds;
#[cfg(feature = "typography_overlay")]
pub use font::GlyphTypographyMetrics;
pub use font_registry::FontId;
pub use font_registry::FontLoadFailed;
pub use font_registry::FontRegistered;
pub use font_registry::FontRegistry;
pub use font_registry::FontSource;
pub use measurer::DiegeticTextMeasurer;
pub use measurer::create_parley_measurer;
pub(crate) use text_plugin::TextPlugin;
