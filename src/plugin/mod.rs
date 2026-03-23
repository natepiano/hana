//! Bevy plugin for diegetic UI panels.
//!
//! Provides [`DiegeticUiPlugin`], which adds layout computation and optional
//! gizmo debug rendering for [`DiegeticPanel`] entities.

mod components;
mod config;
mod systems;

use bevy::prelude::*;
pub use components::ComputedDiegeticPanel;
pub use components::DiegeticPanel;
pub use components::DiegeticTextMeasurer;
pub use components::HueOffset;
pub use config::AtlasConfig;
pub use config::RasterQuality;
pub use systems::DiegeticPerfStats;
pub use systems::ShowTextGizmos;
pub use systems::compute_panel_layouts;
use systems::render_panel_gizmos;

use crate::layout::ForLayout;
use crate::layout::ForStandalone;
use crate::layout::TextProps;
use crate::render::ShapedTextCache;
use crate::render::TextRenderPlugin;
use crate::text::FontRegistry;
use crate::text::MsdfAtlas;
use crate::text::create_parley_measurer;

/// Creates the empty GPU `Image` for the MSDF atlas at startup.
///
/// The atlas starts with no glyphs — they are rasterized on demand. But
/// the `Image` handle must exist before any text extraction system runs
/// so that materials can reference it.
fn init_atlas_image(mut atlas: ResMut<MsdfAtlas>, mut images: ResMut<Assets<Image>>) {
    atlas.upload_to_gpu(&mut images);
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
/// Uses [`RasterQuality::High`] (64px) with 100 glyphs per page.
///
/// # Custom atlas configuration
///
/// Use [`with_atlas`](Self::with_atlas) to tune rasterization quality
/// and per-page glyph budget. This returns a builder that implements
/// [`Plugin`], so you can chain configuration and pass it directly to
/// `add_plugins`:
///
/// ```ignore
/// use bevy_diegetic::{DiegeticUiPlugin, RasterQuality};
///
/// App::new().add_plugins(
///     DiegeticUiPlugin::with_atlas()
///         .quality(RasterQuality::Low)
///         .glyphs_per_page(50)
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
    /// Call `.quality()` and/or `.glyphs_per_page()` to override
    /// defaults, then pass the result to `add_plugins`. Only the
    /// settings you override are changed — the rest use sensible
    /// defaults ([`RasterQuality::High`], 100 glyphs per page).
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
    /// )
    /// ```
    #[must_use]
    pub const fn with_atlas() -> DiegeticUiPluginConfigured {
        DiegeticUiPluginConfigured {
            config: AtlasConfig::new(),
        }
    }
}

impl Plugin for DiegeticUiPlugin {
    fn build(&self, app: &mut App) { build_plugin(app, None); }
}

/// Configured variant of [`DiegeticUiPlugin`] with custom atlas settings.
///
/// Created by [`DiegeticUiPlugin::with_atlas`]. Implements [`Plugin`]
/// so it can be passed directly to `add_plugins`.
pub struct DiegeticUiPluginConfigured {
    config: AtlasConfig,
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
}

impl Plugin for DiegeticUiPluginConfigured {
    fn build(&self, app: &mut App) { build_plugin(app, Some(&self.config)); }
}

/// Shared plugin build logic for both [`DiegeticUiPlugin`] and
/// [`DiegeticUiPluginConfigured`].
fn build_plugin(app: &mut App, config: Option<&AtlasConfig>) {
    // Initialize font registry and wire up parley-backed text measurement.
    let registry = FontRegistry::new();
    let measurer = DiegeticTextMeasurer {
        measure_fn: create_parley_measurer(registry.font_context(), registry.family_names()),
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
        app.insert_resource(MsdfAtlas::with_config(page_size, canonical_size));
    }

    app.insert_resource(registry)
        .insert_resource(measurer)
        .add_plugins(LayoutPlugin)
        .init_resource::<ShowTextGizmos>()
        .register_type::<TextProps<ForLayout>>()
        .register_type::<TextProps<ForStandalone>>()
        .add_plugins(TextRenderPlugin)
        .init_gizmo_group::<DiegeticPanelGizmoGroup>()
        .add_systems(Startup, init_atlas_image)
        .add_systems(Update, render_panel_gizmos.after(compute_panel_layouts));

    #[cfg(feature = "typography_overlay")]
    app.add_systems(Update, crate::debug::build_typography_overlay);
}
