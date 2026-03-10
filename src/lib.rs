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

// Layout types.
pub use layout::AlignX;
pub use layout::AlignY;
pub use layout::Border;
pub use layout::BoundingBox;
pub use layout::Direction;
pub use layout::Padding;
pub use layout::Sizing;
pub use layout::TextConfig;
pub use layout::TextDimensions;

// Layout tree.
pub use layout::El;
pub use layout::Element;
pub use layout::ElementContent;
pub use layout::LayoutBuilder;
pub use layout::LayoutTree;

// Layout engine.
pub use layout::ComputedLayout;
pub use layout::LayoutEngine;
pub use layout::LayoutResult;
pub use layout::MeasureTextFn;

// Render commands.
pub use layout::RenderCommand;
pub use layout::RenderCommandKind;

// Bevy plugin.
pub use plugin::ComputedDiegeticPanel;
pub use plugin::DiegeticPanel;
pub use plugin::DiegeticPanelGizmoGroup;
pub use plugin::DiegeticTextMeasurer;
pub use plugin::DiegeticUiPlugin;
