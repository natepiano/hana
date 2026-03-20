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

mod layout;
mod plugin;
mod render;
mod text;

// Layout types.
pub use layout::AlignX;
pub use layout::AlignY;
pub use layout::Border;
pub use layout::BoundingBox;
// Layout engine.
pub use layout::ComputedLayout;
pub use layout::Culling;
pub use layout::Direction;
// Layout tree.
pub use layout::El;
pub use layout::ElementColors;
pub use layout::FontSlant;
pub use layout::FontWeight;
pub use layout::ForLayout;
pub use layout::ForStandalone;
pub use layout::LayoutBuilder;
pub use layout::LayoutEngine;
pub use layout::LayoutResult;
pub use layout::LayoutTree;
pub use layout::MeasureTextFn;
pub use layout::Padding;
// Render commands.
pub use layout::RenderCommand;
pub use layout::RenderCommandKind;
pub use layout::Sizing;
pub use layout::TextAlign;
pub use layout::TextAnchor;
pub use layout::TextConfig;
pub use layout::TextDimensions;
pub use layout::TextMeasure;
pub use layout::TextProps;
pub use layout::TextStyle;
pub use layout::TextWrap;
// Bevy plugin.
pub use plugin::ComputedDiegeticPanel;
pub use plugin::DiegeticPanel;
pub use plugin::DiegeticPanelGizmoGroup;
pub use plugin::DiegeticPerfStats;
pub use plugin::DiegeticTextMeasurer;
pub use plugin::DiegeticUiPlugin;
pub use plugin::ShowTextGizmos;
pub use render::GlyphQuadData;
pub use render::MsdfTextMaterial;
pub use render::ShapedTextCache;
pub use render::TextRenderPlugin;
pub use render::TextShapingContext;
// Render.
pub use render::WorldText;
pub use render::build_glyph_mesh;
pub use render::shape_text_to_quads;
// Text.
pub use text::EMBEDDED_FONT;
pub use text::FontId;
pub use text::FontRegistry;
pub use text::GlyphKey;
pub use text::GlyphMetrics;
pub use text::MsdfAtlas;
pub use text::MsdfBitmap;
pub use text::create_parley_measurer;
pub use text::rasterize_glyph;
