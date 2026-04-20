//! Top-level text plugin that wires font loading, MSDF atlas initialization,
//! and parley-backed measurement into the Bevy app.

use bevy::asset::AssetLoadFailedEvent;
use bevy::prelude::*;

use super::MsdfAtlas;
use super::atlas_config::AtlasConfig;
use super::font::Font;
use super::font_loader::FontLoader;
use super::font_registry::FontId;
use super::font_registry::FontLoadFailed;
use super::font_registry::FontRegistered;
use super::font_registry::FontRegistry;
use super::font_registry::FontSource;
use super::measurer;
use super::measurer::DiegeticTextMeasurer;

pub(crate) struct TextPlugin;

impl Plugin for TextPlugin {
    fn build(&self, app: &mut App) {
        // Preserve the font-parse-failure gate: if the embedded font fails
        // to parse, skip text setup entirely (the plugin stack is disabled).
        let Some(registry) = FontRegistry::new() else {
            warn!("bevy_diegetic: embedded font failed to parse — text plugin disabled");
            return;
        };

        let measurer = DiegeticTextMeasurer {
            measure_fn: measurer::create_parley_measurer(
                registry.font_context(),
                registry.family_names(),
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

        app.insert_resource(registry)
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
    mut registry: ResMut<FontRegistry>,
    mut commands: Commands,
) {
    for event in events.read() {
        if let AssetEvent::Added { id } = event
            && let Some(font) = font_assets.get(*id)
        {
            if registry.font_id_by_name(font.name()).is_some() {
                continue;
            }
            if let Some(font_id) = registry.register_font(font.name(), font.data()) {
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
