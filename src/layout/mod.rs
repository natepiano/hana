//! Layout engine for diegetic UI panels.
//!
//! This module provides a Clay-inspired layout algorithm implemented in pure Rust
//! with no global state, no unsafe code, and full thread safety.
//!
//! # Architecture
//!
//! - [`types`] — Core layout types: `Sizing`, `Direction`, `Padding`, `BoundingBox`, etc.
//! - [`element`] — Element tree representation (`LayoutTree`, `Element`).
//! - [`builder`] — Ergonomic closure-based API for constructing trees.
//! - [`engine`] — The layout computation engine.
//! - [`render`] — Render commands output by the engine.

mod builder;
mod element;
mod engine;
mod render;
mod types;

pub use builder::El;
pub use builder::LayoutBuilder;
pub use element::Element;
pub use element::ElementContent;
pub use element::LayoutTree;
pub use engine::ComputedLayout;
pub use engine::LayoutEngine;
pub use engine::LayoutResult;
pub use engine::MeasureTextFn;
pub use render::RenderCommand;
pub use render::RenderCommandKind;
pub use types::AlignX;
pub use types::AlignY;
pub use types::BackgroundColor;
pub use types::Border;
pub use types::BoundingBox;
pub use types::Direction;
pub use types::Padding;
pub use types::Sizing;
pub use types::TextConfig;
pub use types::TextDimensions;
