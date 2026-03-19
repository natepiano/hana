//! Font loading, text measurement, and MSDF atlas generation.
//!
//! [`FontRegistry`] manages font loading via parley's `FontContext`.
//! The registry embeds `JetBrains Mono` as the default font and provides
//! a [`MeasureTextFn`](crate::MeasureTextFn) backed by parley's layout engine.
//!
//! [`MsdfAtlas`] packs rasterized MSDF glyph bitmaps into a texture atlas
//! for GPU rendering.

mod atlas;
mod font_registry;
mod measurer;
mod msdf_rasterizer;

pub use atlas::GlyphKey;
pub use atlas::GlyphMetrics;
pub use atlas::MsdfAtlas;
pub use font_registry::EMBEDDED_FONT;
pub use font_registry::FontId;
pub use font_registry::FontRegistry;
pub use measurer::create_parley_measurer;
pub use msdf_rasterizer::DEFAULT_CANONICAL_SIZE;
pub use msdf_rasterizer::MsdfBitmap;
pub use msdf_rasterizer::rasterize_glyph;
