//! Bevy plugin for diegetic UI panels.
//!
//! Provides [`DiegeticUiPlugin`], which adds layout computation and optional
//! gizmo debug rendering for [`DiegeticPanel`] entities.

#[allow(
    clippy::used_underscore_binding,
    reason = "false positive from derive-generated code for `PanelMode::Screen` variant fields"
)]
mod components;
mod config;
mod constants;
mod diagnostics;
mod runtime;
mod screen_space;
mod systems;

use bevy::prelude::*;
pub use components::ComputedDiegeticPanel;
pub use components::DiegeticPanel;
pub use components::DiegeticPanelBuilder;
pub use components::DiegeticTextMeasurer;
pub use components::HueOffset;
pub use components::PanelMode;
pub use components::RenderMode;
pub use components::ScreenDimension;
pub use components::ScreenPosition;
pub use components::SurfaceShadow;
pub use config::AtlasConfig;
pub use config::DimensionMatch;
pub use config::GlyphWorkerThreads;
pub use config::HasUnit;
pub use config::In;
pub use config::InvalidSize;
pub use config::Mm;
pub use config::PanelSize;
pub use config::PaperSize;
pub use config::Pt;
pub use config::Px;
pub use config::RasterQuality;
pub use config::UnitConfig;
pub use systems::DiegeticPanelGizmoGroup;
pub use systems::DiegeticPerfStats;
pub use systems::ShowTextGizmos;

pub use crate::layout::Unit;
use crate::render::ShapedTextCache;

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
            app.add_plugins(diagnostics::DiagnosticsPlugin);
            app.init_resource::<ShapedTextCache>()
                .init_resource::<DiegeticPerfStats>()
                .add_systems(Update, systems::compute_panel_layouts);
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
    fn build(&self, app: &mut App) {
        app.add_plugins(runtime::DiegeticRuntimePlugin::new(None, None));
    }
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
    fn build(&self, app: &mut App) {
        app.add_plugins(runtime::DiegeticRuntimePlugin::new(
            Some(self.config),
            self.unit_config,
        ));
    }
}
