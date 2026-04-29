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
mod msdf_rasterizer;

pub use atlas::GlyphKey;
pub use atlas::GlyphLookup;
pub use atlas::GlyphMetrics;
pub use atlas::MsdfAtlas;
pub use atlas_config::AtlasConfig;
pub use atlas_config::GlyphWorkerThreads;
pub use atlas_config::RasterQuality;
use bevy::asset::AssetLoadFailedEvent;
use bevy::prelude::*;
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

use self::font_loader::FontLoader;

pub(crate) struct TextPlugin;

impl Plugin for TextPlugin {
    fn build(&self, app: &mut App) {
        // Preserve the font-parse-failure gate: if the embedded font fails
        // to parse, skip text setup entirely (the plugin stack is disabled).
        let Some(font_registry) = FontRegistry::new() else {
            warn!("bevy_diegetic: embedded font failed to parse — text plugin disabled");
            return;
        };

        let measurer = DiegeticTextMeasurer {
            measure_fn: measurer::create_parley_measurer(
                font_registry.font_context(),
                font_registry.family_names(),
            ),
        };

        // Initialize the MSDF atlas from the configured `AtlasConfig`, unless
        // the user pre-inserted a custom `MsdfAtlas` directly.
        let config = app
            .world()
            .get_resource::<AtlasConfig>()
            .copied()
            .unwrap_or_default();

        if !app.world().contains_resource::<MsdfAtlas>() {
            // Only log when the user explicitly configured the atlas.
            if app.world().contains_resource::<AtlasConfig>() {
                config.log_and_clamp();
            }
            let page_size = config.page_size();
            let canonical_size = config.canonical_size();
            let glyph_worker_threads = config.clamped_glyph_worker_threads();
            app.insert_resource(MsdfAtlas::with_config(
                page_size,
                canonical_size,
                glyph_worker_threads,
            ));
        }

        app.insert_resource(font_registry)
            .insert_resource(measurer)
            .init_asset::<Font>()
            .init_asset_loader::<FontLoader>()
            .add_systems(Startup, init_atlas_and_embedded_font)
            .add_systems(PostUpdate, (consume_loaded_fonts, watch_font_failures));
    }
}

/// Creates the empty GPU `Image` for the MSDF atlas at startup and fires
/// [`FontRegistered`] for the embedded default font.
fn init_atlas_and_embedded_font(
    mut atlas: ResMut<MsdfAtlas>,
    mut images: ResMut<Assets<Image>>,
    mut commands: Commands,
) {
    atlas.upload_to_gpu(&mut images);
    commands.trigger(FontRegistered {
        id:     FontId::MONOSPACE,
        name:   "JetBrains Mono".to_string(),
        source: FontSource::Embedded,
    });
}

/// Watches for newly loaded [`Font`] assets and registers them with
/// [`FontRegistry`]. Fires [`FontRegistered`] for each successful
/// registration.
fn consume_loaded_fonts(
    mut events: MessageReader<AssetEvent<Font>>,
    font_assets: Res<Assets<Font>>,
    mut font_registry: ResMut<FontRegistry>,
    mut commands: Commands,
) {
    for event in events.read() {
        if let AssetEvent::Added { id } = event
            && let Some(font) = font_assets.get(*id)
        {
            if font_registry.font_id_by_name(font.name()).is_some() {
                continue;
            }
            if let Some(font_id) = font_registry.register_font(font.name(), font.data()) {
                commands.trigger(FontRegistered {
                    id:     font_id,
                    name:   (*font.name()).to_string(),
                    source: FontSource::Loaded,
                });
            }
        }
    }
}

/// Watches for failed [`Font`] asset loads and fires [`FontLoadFailed`].
fn watch_font_failures(
    mut failures: MessageReader<AssetLoadFailedEvent<Font>>,
    mut commands: Commands,
) {
    for event in failures.read() {
        commands.trigger(FontLoadFailed {
            path:  event.path.to_string(),
            error: event.error.to_string(),
        });
    }
}
