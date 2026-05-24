//! Font loading and text measurement.
//!
//! [`FontRegistry`] manages font loading via parley's `FontContext`.
//! The registry embeds `JetBrains Mono` as the default font and provides
//! a [`MeasureTextFn`](crate::MeasureTextFn) backed by parley's layout engine.
//!
//! [`Font`] provides access to font-level typographic metrics via
//! [`Font::metrics`], which returns a [`FontMetrics`] struct scaled to
//! any requested font size.
//!
//! Glyph rendering is handled by the slug analytic Bézier renderer; this
//! module owns only font infrastructure and its slug shader/material setup.

mod constants;
mod font;
mod font_loader;
mod font_registry;
mod measurer;

use bevy::asset::AssetLoadFailedEvent;
use bevy::pbr::MaterialPlugin;
use bevy::prelude::*;
pub(crate) use constants::DEFAULT_FAMILY;
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
pub(crate) use font_registry::ResolvedFontData;
pub use measurer::DiegeticTextMeasurer;
pub use measurer::create_parley_measurer;

use self::font_loader::FontLoader;
use crate::slug_text_spike::SlugBackend;
use crate::slug_text_spike::SlugTextMaterial;
use crate::slug_text_spike::register_slug_text_shader;

pub(crate) struct TextPlugin;

impl Plugin for TextPlugin {
    fn build(&self, app: &mut App) {
        // Slug text setup travels with the font infrastructure. The shader
        // registration stays in the slug module until its files move under
        // `text/slug/` (Phase 4 of the slug migration).
        register_slug_text_shader(app);
        app.init_resource::<SlugBackend>();
        app.add_plugins(MaterialPlugin::<SlugTextMaterial>::default());

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

        app.insert_resource(font_registry)
            .insert_resource(measurer)
            .init_asset::<Font>()
            .init_asset_loader::<FontLoader>()
            .add_systems(Startup, register_embedded_font)
            .add_systems(PostUpdate, (consume_loaded_fonts, watch_font_failures));
    }
}

/// Fires [`FontRegistered`] for the embedded default font at startup.
fn register_embedded_font(mut commands: Commands) {
    commands.trigger(FontRegistered {
        id:     FontId::MONOSPACE,
        name:   DEFAULT_FAMILY.to_string(),
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
