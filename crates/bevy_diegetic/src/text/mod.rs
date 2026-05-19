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
//! [`GlyphAtlas`] packs rasterized MSDF glyph bitmaps into a texture atlas
//! for GPU rendering.

mod atlas;
mod atlas_config;
mod atlas_slot;
mod bitmap_dims;
mod constants;
mod font;
mod font_loader;
mod font_registry;
mod gpu_rasterizer;
mod measurer;
mod msdf_rasterizer;

pub use atlas::GlyphAtlas;
pub use atlas::GlyphKey;
pub use atlas::GlyphLookup;
pub use atlas::GlyphMetrics;
pub use atlas::GpuAtlasRegion;
pub use atlas_config::AtlasConfig;
pub use atlas_config::AtlasConfigError;
pub use atlas_config::GlyphWorkerThreads;
pub use atlas_config::RasterBackend;
pub use atlas_config::RasterQuality;
pub use atlas_slot::AtlasPreference;
pub use atlas_slot::AtlasSlot;
pub use atlas_slot::AtlasSwapCompleted;
pub use atlas_slot::AtlasSwapStarted;
use bevy::asset::AssetLoadFailedEvent;
use bevy::prelude::*;
pub(crate) use constants::DEFAULT_FAMILY;
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
pub use gpu_rasterizer::GpuEnqueueResult;
pub use gpu_rasterizer::GpuGlyphBudget;
pub use gpu_rasterizer::GpuRasterizerPlugin;
pub use gpu_rasterizer::enqueue_gpu_glyph;
pub use measurer::DiegeticTextMeasurer;
pub use measurer::create_parley_measurer;
pub use msdf_rasterizer::DistanceField;

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
        // the user pre-inserted a custom `GlyphAtlas` directly.
        let config = app
            .world()
            .get_resource::<AtlasConfig>()
            .copied()
            .unwrap_or_default();

        if !app.world().contains_resource::<AtlasSlot>() {
            // Only log when the user explicitly configured the atlas.
            if app.world().contains_resource::<AtlasConfig>() {
                config.log_and_clamp();
            }
            // Reject unsupported (backend, distance_field) combinations
            // up front so the GPU dispatcher never queues a glyph it
            // cannot rasterize.
            if let Err(err) = config.validate() {
                warn!("AtlasConfig: {err}; downgrading backend to Cpu",);
            }
            let page_size = config.page_size();
            let canonical_size = config.canonical_size();
            let glyph_worker_threads = config.clamped_glyph_worker_threads();
            let mut atlas = GlyphAtlas::with_config(
                page_size,
                canonical_size,
                glyph_worker_threads,
                config.distance_field,
                None,
            );
            atlas.set_backend(config.backend);
            app.insert_resource(AtlasSlot::Single(atlas));
        }
        if !app.world().contains_resource::<AtlasPreference>() {
            app.insert_resource(AtlasPreference {
                distance_field: config.distance_field,
                quality:        config.quality,
                backend:        config.backend,
            });
        }

        app.insert_resource(font_registry)
            .insert_resource(measurer)
            .init_asset::<Font>()
            .init_asset_loader::<FontLoader>()
            .add_systems(Startup, init_atlas_and_embedded_font)
            .add_systems(
                PostUpdate,
                (drive_atlas_swap, consume_loaded_fonts, watch_font_failures),
            );
    }
}

/// Creates the empty GPU `Image` for the active glyph atlas at startup
/// and fires [`FontRegistered`] for the embedded default font.
fn init_atlas_and_embedded_font(
    mut atlas: ResMut<AtlasSlot>,
    mut images: ResMut<Assets<Image>>,
    mut commands: Commands,
) {
    atlas.active_mut().upload_to_gpu(&mut images);
    commands.trigger(FontRegistered {
        id:     FontId::MONOSPACE,
        name:   DEFAULT_FAMILY.to_string(),
        source: FontSource::Embedded,
    });
}

/// Drives the parallel-atlas swap state machine.
///
/// The driver reads [`AtlasPreference`] each `PostUpdate` tick. When
/// the preference's `(distance_field, canonical_size)` tuple differs
/// from the active atlas, it builds a fresh `pending` atlas at the
/// new tuple, marks visible text entities pending via the
/// `AtlasSwapStarted` event, and waits for the text-shaping pass to
/// queue their glyphs onto pending. Once pending's in-flight raster
/// jobs drain, the slot completes the swap and fires
/// `AtlasSwapCompleted` to trigger material rebuilds.
///
/// If the user flips the preference mid-swap (e.g. picks a third
/// quality while a swap is in flight), the in-flight pending is
/// abandoned (its workers' channel drops, results vanish harmlessly)
/// and a new pending starts at the new target.
#[allow(
    clippy::needless_pass_by_value,
    reason = "Bevy system functions take resources by value"
)]
fn drive_atlas_swap(
    preference: Res<AtlasPreference>,
    mut slot: ResMut<AtlasSlot>,
    config: Option<Res<AtlasConfig>>,
    mut frames_in_swap: Local<u32>,
    mut commands: Commands,
) {
    let active_mode = slot.distance_field();
    let active_canonical = slot.active().canonical_size();
    let active_backend = slot.active().backend();
    let target_mode = preference.distance_field;
    let target_canonical = preference.quality.pixel_size();
    let target_backend = preference.backend;
    let active_matches_target = target_mode == active_mode
        && target_canonical == active_canonical
        && target_backend == active_backend;

    match (&mut *slot, active_matches_target) {
        // Steady state, matching preferences: nothing to do.
        (AtlasSlot::Single(_), true) => {
            *frames_in_swap = 0;
        },

        // Steady state, mismatched preference: start a swap.
        (AtlasSlot::Single(_), false) => {
            let active = match std::mem::take(&mut *slot) {
                AtlasSlot::Single(a) => a,
                AtlasSlot::Swapping { active, .. } => active,
            };
            let target_cfg = target_config(
                config.as_deref(),
                target_mode,
                preference.quality,
                target_backend,
            );
            let mut pending = GlyphAtlas::with_config(
                target_cfg.page_size(),
                target_cfg.canonical_size(),
                target_cfg.clamped_glyph_worker_threads(),
                target_cfg.distance_field,
                Some(active.worker_pool()),
            );
            pending.set_backend(target_cfg.backend);
            if let Some(dispatcher) = active.gpu_dispatcher_handle() {
                pending.set_gpu_dispatcher(dispatcher);
            }
            *slot = AtlasSlot::Swapping { active, pending };
            *frames_in_swap = 0;
            commands.trigger(AtlasSwapStarted);
        },

        // Mid-swap, both preferences now match active: abandon pending
        // and return to Single(active).
        (AtlasSlot::Swapping { .. }, true) => {
            let taken = std::mem::take(&mut *slot);
            if let AtlasSlot::Swapping { active, .. } = taken {
                *slot = AtlasSlot::Single(active);
            }
            *frames_in_swap = 0;
            commands.trigger(AtlasSwapCompleted);
        },

        // Mid-swap, preference still differs from active.
        (AtlasSlot::Swapping { pending, .. }, false) => {
            let pending_matches_target = pending.distance_field() == target_mode
                && pending.canonical_size() == target_canonical
                && pending.backend() == target_backend;
            if pending_matches_target {
                // Wait at least two frames after swap start: one for
                // the `AtlasSwapStarted` observer to mark visible text
                // entities with `PendingGlyphs`, and one for the
                // text-shaping pass to enqueue their glyphs onto
                // pending. Then complete the swap when pending has no
                // in-flight raster jobs left.
                *frames_in_swap = frames_in_swap.saturating_add(1);
                if *frames_in_swap >= 2 && pending.in_flight_count() == 0 {
                    slot.complete_swap();
                    *frames_in_swap = 0;
                    commands.trigger(AtlasSwapCompleted);
                }
            } else {
                // User flipped to a third target mid-swap. Restart
                // with a new pending at the new target.
                let taken = std::mem::take(&mut *slot);
                let active = match taken {
                    AtlasSlot::Single(a) | AtlasSlot::Swapping { active: a, .. } => a,
                };
                let target_cfg = target_config(
                    config.as_deref(),
                    target_mode,
                    preference.quality,
                    target_backend,
                );
                let mut pending = GlyphAtlas::with_config(
                    target_cfg.page_size(),
                    target_cfg.canonical_size(),
                    target_cfg.clamped_glyph_worker_threads(),
                    target_cfg.distance_field,
                    Some(active.worker_pool()),
                );
                pending.set_backend(target_cfg.backend);
                if let Some(dispatcher) = active.gpu_dispatcher_handle() {
                    pending.set_gpu_dispatcher(dispatcher);
                }
                *slot = AtlasSlot::Swapping { active, pending };
                *frames_in_swap = 0;
                commands.trigger(AtlasSwapStarted);
            }
        },
    }
}

/// Builds an `AtlasConfig` that overlays the requested mode, quality,
/// and backend onto the user's existing config (or defaults if none).
fn target_config(
    config: Option<&AtlasConfig>,
    mode: DistanceField,
    quality: RasterQuality,
    backend: RasterBackend,
) -> AtlasConfig {
    let base = config.copied().unwrap_or_default();
    AtlasConfig {
        quality,
        glyphs_per_page: base.glyphs_per_page,
        glyph_worker_threads: base.glyph_worker_threads,
        distance_field: mode,
        backend,
    }
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
