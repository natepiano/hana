//! Layout engine for diegetic UI panels.
//!
//! This module provides a Clay-inspired layout algorithm implemented in pure Rust
//! with no global state, no unsafe code, and full thread safety.
//!
//! # Retained-mode vs immediate-mode
//!
//! Clay is immediate-mode: the `Clay` object is both builder and engine — you call
//! `clay.begin()`, build the tree with `clay.with(...)`, and get layout results in one shot.
//! The tree is ephemeral and rebuilt from scratch every frame.
//!
//! This engine is retained-mode: tree construction and layout computation are separate steps.
//! [`LayoutBuilder`] produces a [`LayoutTree`] that persists across frames, and
//! [`LayoutEngine`] recomputes positions only when the tree changes. This means:
//!
//! - The tree is built once and **reused** — not rebuilt every frame.
//! - Layout can be recomputed without rebuilding the tree (e.g., panel resized but content
//!   unchanged).
//! - Construction and computation are independently testable.
//!
//! The tradeoff is an extra [`LayoutBuilder`] step at construction time, but you pay it once,
//! not every frame.
//!
//! Retained mode is the natural fit for Bevy. The entire ECS is built around doing nothing
//! unless something changed — `Changed<T>`, `Added<T>`, `Res::is_changed()`, observers.
//! An immediate-mode layout engine would fight the framework by recomputing unconditionally
//! every frame. Retained mode lets Bevy's change detection skip layout entirely on frames
//! where the tree hasn't been touched.
//!
//! # Architecture
//!
//! - [`types`] — Core layout types: `Sizing`, `Direction`, `Padding`, `BoundingBox`, etc.
//! - [`element`] — Arena-based element tree ([`LayoutTree`], [`Element`]). [`Element`] is the
//!   canonical storage format; users construct elements via [`El`] instead.
//! - [`builder`] — [`El`] is the ergonomic fluent builder that converts into [`Element`].
//!   [`LayoutBuilder`] manages parent-child nesting via a closure API — no open/close pairs.
//! - [`engine`] — Two-pass layout computation (BFS sizing, DFS positioning).
//! - [`render`] — Render commands output by the engine.

mod builder;
#[cfg(test)]
mod clay_parity_tests;
mod constants;
mod element;
mod engine;
mod font_features;
mod geometry;
#[cfg(test)]
mod layout_tests;
mod render;
mod shape_cache;
mod sizing;
mod text_props;
mod units;

pub use builder::El;
pub use builder::LayoutBuilder;
pub use element::LayoutTree;
pub use engine::LayoutEngine;
pub use engine::LayoutResult;
pub use engine::MeasureTextFn;
pub use font_features::FontFeatureFlags;
pub use font_features::FontFeatures;
pub use geometry::Border;
pub use geometry::BoundingBox;
pub use geometry::CornerRadius;
pub use render::RectangleSource;
pub use render::RenderCommand;
pub use render::RenderCommandKind;
pub use shape_cache::LineMetricsSnapshot;
pub use shape_cache::ShapedGlyph;
pub use shape_cache::ShapedTextCache;
pub use shape_cache::ShapedTextRun;
pub use sizing::AlignX;
pub use sizing::AlignY;
pub use sizing::Direction;
pub use sizing::Padding;
pub use sizing::Sizing;
pub use text_props::FontSlant;
pub use text_props::FontWeight;
pub use text_props::ForLayout;
pub use text_props::ForStandalone;
pub use text_props::GlyphLoadingPolicy;
pub use text_props::GlyphRenderMode;
pub use text_props::GlyphShadowMode;
pub use text_props::GlyphSidedness;
pub use text_props::LayoutTextStyle;
pub use text_props::TextAlign;
pub use text_props::TextDimensions;
pub use text_props::TextMeasure;
pub use text_props::TextProps;
pub use text_props::TextWrap;
pub use text_props::WorldTextStyle;
pub use units::Anchor;
pub use units::Dimension;
pub use units::DimensionMatch;
pub use units::HasUnit;
pub use units::In;
pub use units::InvalidSize;
pub use units::Mm;
pub use units::PanelSize;
pub use units::PaperSize;
pub use units::Pt;
pub use units::Px;
pub use units::Unit;
pub use units::UnitConfig;

/// Sets the root element's width sizing to `GROW`.
///
/// Screen-space percent sizing uses this crate-internal facade so wider
/// callers do not need direct access to nested `element` internals.
pub(crate) fn set_root_grow_width(tree: &mut LayoutTree) { tree.set_root_grow_width(); }

/// Sets the root element's height sizing to `GROW`.
///
/// See [`set_root_grow_width`] for the rationale behind this facade.
pub(crate) fn set_root_grow_height(tree: &mut LayoutTree) { tree.set_root_grow_height(); }

/// Sets the root element's width sizing to `FIT { min, max }`.
pub(crate) fn set_root_fit_width(tree: &mut LayoutTree, min: Dimension, max: Dimension) {
    tree.set_root_fit_width(min, max);
}

/// Sets the root element's height sizing to `FIT { min, max }`.
pub(crate) fn set_root_fit_height(tree: &mut LayoutTree, min: Dimension, max: Dimension) {
    tree.set_root_fit_height(min, max);
}

