//! `bevy_diegetic` — Diegetic UI for Bevy.
//!
//! Provides an in-world UI layout engine inspired by [Clay](https://github.com/nicbarker/clay),
//! implemented in pure Rust with no global state and full thread safety.
//!
//! # Retained-mode layout
//!
//! Clay is immediate-mode: the tree is rebuilt from scratch every frame and layout is computed
//! inline as you build it. `bevy_diegetic` is retained-mode: the [`LayoutTree`] is built once
//! via [`LayoutBuilder`], stored on a component, and the [`LayoutEngine`] only recomputes
//! positions when the tree changes. This is the natural fit for Bevy — the entire ECS is built
//! around doing nothing unless something changed (`Changed<T>`, `Res::is_changed()`,
//! observers). An immediate-mode engine would fight the framework by recomputing
//! unconditionally every frame; retained mode lets Bevy's change detection skip layout
//! entirely on frames where the tree hasn't been touched.
//!
//! # Quick start
//!
//! ```ignore
//! use bevy::prelude::*;
//! use bevy_diegetic::*;
//!
//! App::new()
//!     .add_plugins(DefaultPlugins)
//!     .add_plugins(DiegeticUiPlugin)
//!     .add_systems(Startup, setup)
//!     .run();
//! ```
//!
//! # Configuration
//!
//! Insert [`AtlasConfig`] and/or [`CascadeDefaults`] as resources before
//! adding [`DiegeticUiPlugin`] to override defaults:
//!
//! ```ignore
//! App::new()
//!     .insert_resource(
//!         AtlasConfig::new()
//!             .with_quality(RasterQuality::Low)
//!             .with_glyphs_per_page(50),
//!     )
//!     .insert_resource(CascadeDefaults {
//!         panel_font_unit: Unit::Millimeters,
//!         ..default()
//!     })
//!     .add_plugins(DiegeticUiPlugin);
//! ```

mod callouts;
mod cascade;
mod constants;
#[cfg(feature = "typography_overlay")]
mod debug;
mod layout;
mod panel;
mod render;
mod screen_space;
mod text;

use bevy::asset::embedded_asset;
use bevy::prelude::*;
pub use callouts::ArrowStyle;
pub use callouts::CalloutCap;
pub use callouts::CalloutLine;
pub use cascade::CascadeDefaults;
pub use cascade::CascadeSet;
#[cfg(feature = "typography_overlay")]
pub use debug::GlyphMetricVisibility;
#[cfg(feature = "typography_overlay")]
pub use debug::OverlayBoundingBox;
#[cfg(feature = "typography_overlay")]
pub use debug::TypographyOverlay;
#[cfg(feature = "typography_overlay")]
pub use debug::TypographyOverlayReady;
pub use layout::AlignX;
pub use layout::AlignY;
pub use layout::Anchor;
pub use layout::Border;
pub use layout::BoundingBox;
pub use layout::CornerRadius;
pub use layout::Dimension;
pub use layout::DimensionMatch;
pub use layout::Direction;
pub use layout::El;
pub use layout::FontFeatureFlags;
pub use layout::FontFeatures;
pub use layout::FontSlant;
pub use layout::FontWeight;
pub use layout::ForLayout;
pub use layout::ForStandalone;
pub use layout::GlyphLoadingPolicy;
pub use layout::GlyphRenderMode;
pub use layout::GlyphShadowMode;
pub use layout::GlyphSidedness;
pub use layout::HasUnit;
pub use layout::In;
pub use layout::InvalidSize;
pub use layout::LayoutBuilder;
pub use layout::LayoutTextStyle;
pub use layout::LayoutTree;
/// Function signature for custom text measurement. Takes a text string and
/// a [`TextMeasure`] describing the font configuration, returns
/// [`TextDimensions`]. See [`DiegeticTextMeasurer`] and the `side_by_side`
/// example for usage.
pub use layout::MeasureTextFn;
pub use layout::Mm;
pub use layout::Padding;
pub use layout::PanelSize;
pub use layout::PaperSize;
pub use layout::Pt;
pub use layout::Px;
pub use layout::Sizing;
pub use layout::TextAlign;
/// Measured width and height of a text string, returned by [`MeasureTextFn`].
pub use layout::TextDimensions;
/// Font configuration passed to [`MeasureTextFn`]: font ID, size, weight,
/// slant, line height, letter/word spacing. See the `side_by_side` example
/// for a real-world custom measurer that bridges clay-layout to our
/// parley-backed measurement via this type.
pub use layout::TextMeasure;
pub use layout::TextProps;
pub use layout::TextWrap;
pub use layout::Unit;
pub use layout::WorldTextStyle;
pub use panel::AnyUnit;
pub use panel::AtlasPerfStats;
pub use panel::CompatibleUnits;
pub use panel::ComputedDiegeticPanel;
pub use panel::CoordinateSpace;
pub use panel::DiegeticPanel;
pub use panel::DiegeticPanelBuilder;
pub use panel::DiegeticPanelGizmoGroup;
pub use panel::DiegeticPerfStats;
pub use panel::Fit;
pub use panel::FitMax;
pub use panel::FitRange;
pub use panel::Grow;
pub use panel::GrowMax;
pub use panel::GrowRange;
pub use panel::HeadlessLayoutPlugin;
pub use panel::HueOffset;
pub use panel::Inches;
pub use panel::Millimeters;
pub use panel::PanelSizing;
pub use panel::PanelTextPerfStats;
pub use panel::Percent;
pub use panel::Pixels;
pub use panel::Points;
pub use panel::RenderMode;
pub use panel::ScreenPosition;
pub use panel::ShowTextGizmos;
pub use panel::SurfaceShadow;
pub use render::PanelTextChild;
pub use render::PendingGlyphs;
pub use render::StableTransparency;
pub use render::WorldText;
pub use render::WorldTextReady;
pub use render::default_panel_material;
pub use text::AtlasConfig;
pub use text::DiegeticTextMeasurer;
pub use text::Font;
pub use text::FontId;
pub use text::FontLoadFailed;
pub use text::FontMetrics;
pub use text::FontRegistered;
pub use text::FontRegistry;
pub use text::FontSource;
#[cfg(feature = "typography_overlay")]
pub use text::GlyphBounds;
pub use text::GlyphKey;
pub use text::GlyphMetrics;
#[cfg(feature = "typography_overlay")]
pub use text::GlyphTypographyMetrics;
pub use text::GlyphWorkerThreads;
pub use text::MsdfAtlas;
pub use text::RasterQuality;

/// Bevy plugin that adds diegetic UI panel support.
///
/// Composes layout, rendering, text, callouts, and screen-space overlay
/// support into a single plugin. Insert configuration resources
/// ([`AtlasConfig`], [`CascadeDefaults`]) before adding this plugin —
/// they take effect through the child plugins at build time.
///
/// # Quick start
///
/// ```ignore
/// App::new().add_plugins(DiegeticUiPlugin)
/// ```
///
/// # Custom atlas configuration
///
/// ```ignore
/// App::new()
///     .insert_resource(
///         AtlasConfig::new()
///             .with_quality(RasterQuality::Low)
///             .with_glyphs_per_page(50)
///             .with_glyph_worker_threads(GlyphWorkerThreads::Fixed(4)),
///     )
///     .add_plugins(DiegeticUiPlugin);
/// ```
pub struct DiegeticUiPlugin;

impl Plugin for DiegeticUiPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "shaders/sdf_panel.wgsl");
        embedded_asset!(app, "shaders/msdf_text.wgsl");

        app.init_resource::<CascadeDefaults>();
        app.add_plugins((
            text::TextPlugin,
            panel::PanelPlugin,
            screen_space::ScreenSpacePlugin,
            render::RenderPlugin,
            callouts::CalloutPlugin,
            #[cfg(feature = "typography_overlay")]
            debug::TypographyOverlayPlugin,
        ));
    }
}
