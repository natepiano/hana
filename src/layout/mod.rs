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
mod element;
mod engine;
#[cfg(test)]
mod layout_tests;
mod render;
mod types;

pub use builder::El;
pub use builder::LayoutBuilder;
pub use element::LayoutTree;
pub use engine::LayoutEngine;
pub use engine::LayoutResult;
pub use engine::MeasureTextFn;
pub use render::RenderCommandKind;
pub use types::AlignX;
pub use types::AlignY;
pub use types::Border;
pub use types::BoundingBox;
pub use types::Direction;
pub use types::FontSlant;
pub use types::FontWeight;
pub use types::ForLayout;
pub use types::ForStandalone;
pub use types::Padding;
pub use types::Sizing;
pub use types::TextAlign;
pub use types::TextAnchor;
pub use types::TextConfig;
pub use types::TextDimensions;
pub use types::TextMeasure;
pub use types::TextProps;
pub use types::TextStyle;
pub use types::TextWrap;
