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
//! # Quick Start
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

#[cfg(feature = "typography_overlay")]
mod debug;
mod layout;
mod plugin;
mod render;
mod text;

// Layout types.
// Debug overlay.
#[cfg(feature = "typography_overlay")]
pub use debug::TypographyOverlay;
pub use layout::AlignX;
pub use layout::AlignY;
pub use layout::Border;
pub use layout::BoundingBox;
pub use layout::Direction;
// Layout tree.
pub use layout::El;
pub use layout::FontSlant;
pub use layout::FontWeight;
pub use layout::ForLayout;
pub use layout::ForStandalone;
pub use layout::GlyphRenderMode;
pub use layout::GlyphShadowMode;
pub use layout::LayoutBuilder;
pub use layout::LayoutTree;
/// Function signature for custom text measurement. Takes a text string and
/// a [`TextMeasure`] describing the font configuration, returns
/// [`TextDimensions`]. See [`DiegeticTextMeasurer`] and the `side_by_side`
/// example for usage.
pub use layout::MeasureTextFn;
pub use layout::Padding;
pub use layout::Sizing;
pub use layout::TextAlign;
pub use layout::TextAnchor;
pub use layout::TextConfig;
/// Measured width and height of a text string, returned by [`MeasureTextFn`].
pub use layout::TextDimensions;
/// Font configuration passed to [`MeasureTextFn`]: font ID, size, weight,
/// slant, line height, letter/word spacing. See the `side_by_side` example
/// for a real-world custom measurer that bridges clay-layout to our
/// parley-backed measurement via this type.
pub use layout::TextMeasure;
pub use layout::TextProps;
pub use layout::TextStyle;
pub use layout::TextWrap;
// Bevy plugin.
pub use plugin::AtlasConfig;
pub use plugin::ComputedDiegeticPanel;
pub use plugin::DiegeticPanel;
pub use plugin::DiegeticPanelGizmoGroup;
pub use plugin::DiegeticPerfStats;
pub use plugin::DiegeticTextMeasurer;
pub use plugin::DiegeticUiPlugin;
pub use plugin::DiegeticUiPluginConfigured;
pub use plugin::HueOffset;
pub use plugin::LayoutPlugin;
pub use plugin::RasterQuality;
pub use plugin::ShowTextGizmos;
// Render.
pub use render::WorldText;
// Text.
pub use text::Font;
pub use text::FontId;
pub use text::FontMetrics;
pub use text::FontRegistry;
#[cfg(feature = "typography_overlay")]
pub use text::GlyphBounds;
#[cfg(feature = "typography_overlay")]
pub use text::GlyphTypographyMetrics;
pub use text::MsdfAtlas;
