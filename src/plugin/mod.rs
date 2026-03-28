//! Bevy plugin for diegetic UI panels.
//!
//! Provides [`DiegeticUiPlugin`], which adds layout computation and optional
//! gizmo debug rendering for [`DiegeticPanel`] entities.

mod components;
mod config;
mod diagnostics;
mod screen_space;
mod systems;

use bevy::asset::AssetLoadFailedEvent;
use bevy::camera::Camera3d;
use bevy::core_pipeline::oit::OrderIndependentTransparencySettings;
use bevy::prelude::*;
use bevy::render::view::Msaa;
pub use components::ComputedDiegeticPanel;
pub use components::DiegeticPanel;
pub use components::DiegeticPanelBuilder;
pub use components::DiegeticTextMeasurer;
pub use components::HueOffset;
pub use components::RenderMode;
pub use components::ScreenSpace;
pub use components::SurfaceShadow;
pub use config::AtlasConfig;
pub use config::GlyphWorkerThreads;
pub use config::In;
pub use config::Mm;
pub use config::PanelSize;
pub use config::PaperSize;
pub use config::Pt;
pub use config::Px;
pub use config::RasterQuality;
pub use config::UnitConfig;
use diagnostics::install as install_perf_diagnostics;
use screen_space::cleanup_screen_space_cameras;
use screen_space::propagate_screen_space_render_layers;
use screen_space::setup_screen_space_cameras;
pub use systems::DiegeticPerfStats;
pub use systems::ShowTextGizmos;
use systems::compute_panel_layouts;
use systems::render_debug_gizmos;
use systems::render_layout_gizmos;

use crate::layout::ForLayout;
use crate::layout::ForStandalone;
use crate::layout::TextProps;
pub use crate::layout::Unit;
use crate::render::PanelGeometryPlugin;
use crate::render::PanelRttPlugin;
use crate::render::ShapedTextCache;
use crate::render::TextRenderPlugin;
use crate::text;
use crate::text::Font;
use crate::text::FontId;
use crate::text::FontLoadFailed;
use crate::text::FontLoader;
use crate::text::FontRegistered;
use crate::text::FontRegistry;
use crate::text::FontSource;
use crate::text::MsdfAtlas;

/// Ensures all `Camera3d` entities have OIT enabled for correct
/// transparent panel rendering. Disables MSAA on cameras where OIT is
/// added (OIT requires MSAA off).
#[allow(clippy::type_complexity)]
fn ensure_oit_on_cameras(
    cameras: Query<
        (Entity, Option<&Msaa>),
        (
            With<Camera3d>,
            Without<OrderIndependentTransparencySettings>,
        ),
    >,
    mut commands: Commands,
) {
    for (entity, msaa) in &cameras {
        // Disable MSAA if it's enabled — OIT panics with MSAA > 1.
        if msaa.is_some_and(|m| m.samples() > 1) {
            commands.entity(entity).insert(Msaa::Off);
        }
        commands
            .entity(entity)
            .insert(OrderIndependentTransparencySettings::default());
    }
}

/// Enables perspective-scaled line widths on panel debug gizmos.
fn configure_panel_gizmos(mut config_store: ResMut<bevy::prelude::GizmoConfigStore>) {
    let (config, _) = config_store.config_mut::<DiegeticPanelGizmoGroup>();
    config.line.perspective = true;
}

/// Creates the empty GPU `Image` for the MSDF atlas at startup and
/// fires [`FontRegistered`] for the embedded default font.
fn init_atlas_and_embedded_font(
    mut atlas: ResMut<MsdfAtlas>,
    mut images: ResMut<Assets<Image>>,
    mut commands: Commands,
) {
    atlas.upload_to_gpu(&mut images);
    // Fire FontRegistered for the embedded font so observers see it.
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
            // Skip if already registered (e.g., embedded font).
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

/// Gizmo group for diegetic panel debug wireframes.
///
/// Enable or disable via Bevy's [`GizmoConfigStore`].
///
/// **Note:** This API is provisional. Once panels render real geometry
/// (Phase 4), debug visualization will likely move to a per-panel debug
/// mode rather than a separate gizmo group.
#[derive(Default, Reflect, GizmoConfigGroup)]
pub struct DiegeticPanelGizmoGroup;

/// Layout-only plugin for diegetic UI panels.
///
/// Registers the layout computation system, text measurement, and shaped
/// text cache — but no rendering, no gizmos, no GPU. Suitable for headless
/// apps and benchmarks.
///
/// Use [`DiegeticUiPlugin`] instead for full rendering support.
pub struct LayoutPlugin;

impl Plugin for LayoutPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<Self>() {
            install_perf_diagnostics(app);
            app.init_resource::<ShapedTextCache>()
                .init_resource::<DiegeticPerfStats>()
                .add_systems(Update, compute_panel_layouts);
        }
    }
}

/// Plugin that adds diegetic UI panel support to a Bevy app.
///
/// Includes [`LayoutPlugin`] plus MSDF text rendering, font
/// registration, atlas management, and gizmo debug wireframes.
///
/// # Quick start (defaults)
///
/// ```ignore
/// App::new().add_plugins(DiegeticUiPlugin)
/// ```
///
/// Uses [`RasterQuality::High`] (64px) with 100 glyphs per page and
/// [`GlyphWorkerThreads::Auto`] glyph raster workers.
///
/// # Custom atlas configuration
///
/// Use [`with_atlas`](Self::with_atlas) to tune rasterization quality,
/// per-page glyph budget, and glyph raster worker count. This returns a
/// builder that implements [`Plugin`], so you can chain configuration and
/// pass it directly to `add_plugins`:
///
/// ```ignore
/// use bevy_diegetic::{DiegeticUiPlugin, GlyphWorkerThreads, RasterQuality};
///
/// App::new().add_plugins(
///     DiegeticUiPlugin::with_atlas()
///         .quality(RasterQuality::Low)
///         .glyphs_per_page(50)
///         .glyph_worker_threads(GlyphWorkerThreads::Fixed(4))
/// )
/// ```
///
/// When a custom config is provided, the plugin logs the estimated
/// memory per atlas page so you can tune for your target platform.
/// Values outside the safe range are clamped with a warning.
pub struct DiegeticUiPlugin;

impl DiegeticUiPlugin {
    /// Returns an atlas configuration builder that implements [`Plugin`].
    ///
    /// Call `.quality()`, `.glyphs_per_page()`, and/or
    /// `.glyph_worker_threads()` to override defaults, then pass the
    /// result to `add_plugins`. Only the settings you override are
    /// changed — the rest use sensible defaults
    /// ([`RasterQuality::High`], 100 glyphs per page,
    /// [`GlyphWorkerThreads::Auto`]).
    ///
    /// The builder logs an `info!` summary of the computed atlas page
    /// size and estimated memory at startup. Out-of-range values are
    /// clamped with a `warn!`.
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Sharp text, small atlas pages:
    /// App::new().add_plugins(
    ///     DiegeticUiPlugin::with_atlas()
    ///         .quality(RasterQuality::High)
    ///         .glyphs_per_page(50)
    ///         .glyph_worker_threads(GlyphWorkerThreads::Fixed(6))
    /// )
    /// ```
    #[must_use]
    pub const fn with_atlas() -> DiegeticUiPluginConfigured {
        DiegeticUiPluginConfigured {
            config:      AtlasConfig::new(),
            unit_config: None,
        }
    }
}

impl Plugin for DiegeticUiPlugin {
    fn build(&self, app: &mut App) { build_plugin(app, None, None); }
}

/// Configured variant of [`DiegeticUiPlugin`] with custom atlas settings.
///
/// Created by [`DiegeticUiPlugin::with_atlas`]. Implements [`Plugin`]
/// so it can be passed directly to `add_plugins`.
pub struct DiegeticUiPluginConfigured {
    config:      AtlasConfig,
    unit_config: Option<UnitConfig>,
}

impl DiegeticUiPluginConfigured {
    /// Sets the rasterization quality.
    ///
    /// Controls the canonical pixel size used for MSDF generation.
    /// Higher quality = sharper text at extreme zoom, but more memory
    /// per glyph.
    ///
    /// | Variant    | Pixels | Notes                                |
    /// |------------|--------|--------------------------------------|
    /// | `Low`      | 16     | Retro/pixel-art, minimal memory      |
    /// | `Medium`   | 32     | Sharp at normal viewing distances     |
    /// | `High`     | 64     | Sharp at extreme zoom (default)       |
    /// | `Extreme`  | 128    | Maximum fidelity, 16x memory vs `Medium` |
    /// | `Custom`   | 8–256  | Clamped to safe range                |
    #[must_use]
    pub const fn quality(mut self, quality: RasterQuality) -> Self {
        self.config.quality = quality;
        self
    }

    /// Sets the target number of glyphs per atlas page.
    ///
    /// This is an **estimate** — actual capacity depends on the font
    /// and character mix. When a page fills, a new page is allocated
    /// automatically. Smaller values reduce per-page memory but may
    /// increase the number of draw calls for text-heavy apps.
    /// Clamped to 10–2000.
    #[must_use]
    pub const fn glyphs_per_page(mut self, count: u16) -> Self {
        self.config.glyphs_per_page = count;
        self
    }

    /// Sets the async glyph raster worker policy.
    ///
    /// This applies to the shared MSDF text pipeline used by both
    /// diegetic panels and [`WorldText`](crate::WorldText).
    ///
    /// [`GlyphWorkerThreads::Auto`] uses the crate's default heuristic,
    /// currently up to 6 worker threads, clamped to the machine's
    /// available parallelism. [`GlyphWorkerThreads::Fixed`] requests an
    /// explicit worker count, which is clamped to
    /// `1..=available_parallelism()` with a warning.
    #[must_use]
    pub const fn glyph_worker_threads(mut self, workers: GlyphWorkerThreads) -> Self {
        self.config.glyph_worker_threads = workers;
        self
    }

    /// Sets the global [`UnitConfig`] for layout dimensions and font sizes.
    ///
    /// Default: layout in [`Unit::Meters`], fonts in [`Unit::Points`].
    #[must_use]
    pub const fn unit_config(mut self, config: UnitConfig) -> Self {
        self.unit_config = Some(config);
        self
    }
}

impl Plugin for DiegeticUiPluginConfigured {
    fn build(&self, app: &mut App) { build_plugin(app, Some(&self.config), self.unit_config); }
}

/// Shared plugin build logic for both [`DiegeticUiPlugin`] and
/// [`DiegeticUiPluginConfigured`].
fn build_plugin(app: &mut App, config: Option<&AtlasConfig>, unit_config: Option<UnitConfig>) {
    install_perf_diagnostics(app);
    // Initialize font registry and wire up parley-backed text measurement.
    let registry = FontRegistry::new();
    let measurer = DiegeticTextMeasurer {
        measure_fn: text::create_parley_measurer(registry.font_context(), registry.family_names()),
    };

    // Initialize MSDF atlas — glyphs are rasterized on demand.
    // Skip if the user pre-inserted a custom atlas resource directly.
    if !app.world().contains_resource::<MsdfAtlas>() {
        let cfg = config.copied().unwrap_or_default();

        // Only log when the user explicitly configured the atlas.
        if config.is_some() {
            cfg.log_and_clamp();
        }

        let page_size = cfg.page_size();
        let canonical_size = cfg.canonical_size();
        let glyph_worker_threads = cfg.clamped_glyph_worker_threads();
        app.insert_resource(MsdfAtlas::with_config(
            page_size,
            canonical_size,
            glyph_worker_threads,
        ));
    }

    app.insert_resource(unit_config.unwrap_or_default())
        .insert_resource(registry)
        .insert_resource(measurer)
        .init_asset::<Font>()
        .init_asset_loader::<FontLoader>()
        .add_plugins(LayoutPlugin)
        .init_resource::<ShowTextGizmos>()
        .register_type::<TextProps<ForLayout>>()
        .register_type::<TextProps<ForStandalone>>()
        .register_type::<Unit>()
        .register_type::<UnitConfig>()
        .add_plugins(TextRenderPlugin)
        .add_plugins(PanelGeometryPlugin)
        .add_plugins(PanelRttPlugin)
        .init_gizmo_group::<DiegeticPanelGizmoGroup>()
        .add_systems(
            Startup,
            (init_atlas_and_embedded_font, configure_panel_gizmos),
        )
        .add_systems(PostUpdate, (consume_loaded_fonts, watch_font_failures))
        .add_systems(
            Update,
            (
                ensure_oit_on_cameras,
                setup_screen_space_cameras.after(compute_panel_layouts),
                render_layout_gizmos.after(compute_panel_layouts),
                render_debug_gizmos.after(compute_panel_layouts),
            ),
        )
        .add_systems(
            PostUpdate,
            (
                propagate_screen_space_render_layers,
                cleanup_screen_space_cameras,
            ),
        );

    #[cfg(feature = "typography_overlay")]
    {
        app.add_observer(crate::debug::on_overlay_added);
        app.add_observer(crate::debug::on_overlay_removed);
        app.add_systems(Update, crate::debug::build_typography_overlay);
        app.add_systems(
            PostUpdate,
            crate::debug::emit_typography_overlay_ready
                .after(bevy::camera::visibility::VisibilitySystems::CalculateBounds),
        );
    }
}
