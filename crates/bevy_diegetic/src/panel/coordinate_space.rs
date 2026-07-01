//! Panel coordinate space and companion effects.

#![allow(
    clippy::used_underscore_binding,
    reason = "false positive on enum variant fields"
)]

use bevy::camera::visibility::RenderLayers;
use bevy::prelude::*;
use bevy::window::WindowRef;

use crate::layout::Dimension;
use crate::layout::ShadowCasting;
use crate::layout::Sizing;

/// Where a screen-space panel is placed within the window.
#[derive(Clone, Copy, Debug, Default, PartialEq, Reflect)]
pub enum ScreenPosition {
    /// Pin to the window edge/corner that matches the panel's
    /// [`Anchor`](crate::Anchor). `Anchor::TopLeft` pins to the window's
    /// top-left corner, `Anchor::Center` pins to the window's center, etc.
    #[default]
    Screen,
    /// Place at an explicit pixel position (top-left origin, y-down).
    /// The panel's [`Anchor`](crate::Anchor) determines which point of the
    /// panel sits at this position.
    At(Vec2),
}

/// Compatibility adapter for the old panel-surface shadow API.
///
/// New code should author [`ShadowCasting`] so fills, borders, panel shapes,
/// text, and images share one cascade.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Reflect)]
pub enum SurfaceShadow {
    /// The panel surface does not cast shadows.
    #[default]
    Off,
    /// The panel surface participates in shadow casting.
    On,
}

impl From<SurfaceShadow> for ShadowCasting {
    fn from(value: SurfaceShadow) -> Self {
        match value {
            SurfaceShadow::Off => Self::Off,
            SurfaceShadow::On => Self::On,
        }
    }
}

impl From<ShadowCasting> for SurfaceShadow {
    fn from(value: ShadowCasting) -> Self {
        match value {
            ShadowCasting::Off => Self::Off,
            ShadowCasting::On => Self::On,
        }
    }
}

/// Whether the panel is placed in 3D world space or rendered as a 2D screen
/// overlay.
///
/// `World` panels are positioned and scaled in 3D space.
/// `Screen` panels render via an orthographic overlay camera.
#[derive(Clone, Debug, Reflect)]
pub enum CoordinateSpace {
    /// Panel is placed in 3D world space.
    World {
        /// Panel width, expressed with the layout engine's [`Sizing`] enum.
        /// `Fixed` is a physical value in the panel's layout unit;
        /// `Fit { min, max }` shrink-wraps content (bounded by `max`).
        /// `Grow` / `Percent` are screen-only and rejected by the world
        /// builder at compile time.
        #[reflect(ignore)]
        width:  Sizing,
        /// Panel height, same semantics as `width`.
        #[reflect(ignore)]
        height: Sizing,
    },
    /// Panel renders as a 2D screen overlay.
    Screen {
        /// Where to place the panel within the window.
        position:      ScreenPosition,
        /// Panel width, expressed with the layout engine's [`Sizing`] enum.
        /// `Fixed` is a pixel value; `Percent(f)` is a fraction of the
        /// window; `Fit { min, max }` grows to content (bounded by `max` if
        /// set); `Grow { min, max }` fills the window clamped to `[min, max]`.
        #[reflect(ignore)]
        width:         Sizing,
        /// Panel height, same semantics as `width`.
        #[reflect(ignore)]
        height:        Sizing,
        /// Camera render order. Higher orders render on top. Default: `1`.
        camera_order:  isize,
        /// Render layers for isolation from the scene camera.
        /// Default: `RenderLayers::layer(31)`.
        render_layers: RenderLayers,
        /// Window this panel renders into. Defaults to `WindowRef::Primary`.
        /// Use `WindowRef::Entity(..)` to pin a panel to a specific window.
        window:        WindowRef,
    },
}

impl Default for CoordinateSpace {
    fn default() -> Self {
        Self::World {
            width:  Sizing::Fixed(Dimension {
                value: 0.0,
                unit:  None,
            }),
            height: Sizing::Fixed(Dimension {
                value: 0.0,
                unit:  None,
            }),
        }
    }
}

impl CoordinateSpace {
    /// Returns `true` if this is a screen-space panel.
    #[must_use]
    pub const fn is_screen(&self) -> bool { matches!(self, Self::Screen { .. }) }
}
