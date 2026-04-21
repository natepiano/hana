//! Panel rendering modes and companion effects.

#![allow(
    clippy::used_underscore_binding,
    reason = "false positive on enum variant fields"
)]

use bevy::camera::visibility::RenderLayers;
use bevy::prelude::*;

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

/// How the panel's visual content is rendered to the screen.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Reflect)]
pub enum RenderMode {
    /// Render-to-texture: all content composited into an offscreen texture,
    /// displayed as a single textured quad. Fixed resolution, one draw call.
    ///
    /// Text is rasterized to the intermediate texture and resampled on
    /// display, which causes visible softness compared to [`Geometry`] mode.
    /// Use only when a single draw call is required or when the panel is
    /// viewed at a distance where per-glyph MSDF meshes are unnecessary.
    Texture,
    /// Direct 3D geometry: backgrounds, borders, and text rendered as
    /// separate meshes in the scene. Infinite resolution, multiple draw
    /// calls. Layer ordering uses `depth_bias` on the transparent sort key.
    #[default]
    Geometry,
}

/// Whether the panel's surface geometry casts 3D shadows.
///
/// "Surface" means backgrounds, borders, and the RTT display quad — the
/// structural parts of the panel. Text shadow casting is controlled
/// independently per text element via `GlyphShadowMode`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Reflect)]
pub enum SurfaceShadow {
    /// Surface geometry does not cast shadows (default).
    #[default]
    Off,
    /// Surface geometry participates in shadow casting.
    On,
}

/// Whether the panel lives in 3D world space or as a 2D screen overlay.
///
/// `World` panels are positioned and scaled in 3D space.
/// `Screen` panels render via an orthographic overlay camera.
#[derive(Clone, Debug, Reflect)]
pub enum PanelMode {
    /// Panel lives in 3D world space.
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
    },
}

impl Default for PanelMode {
    fn default() -> Self {
        Self::World {
            width:  Sizing::Fixed(crate::layout::Dimension {
                value: 0.0,
                unit:  None,
            }),
            height: Sizing::Fixed(crate::layout::Dimension {
                value: 0.0,
                unit:  None,
            }),
        }
    }
}

impl PanelMode {
    /// Returns `true` if this is a screen-space panel.
    #[must_use]
    pub const fn is_screen(&self) -> bool { matches!(self, Self::Screen { .. }) }
}

/// Hue rotation applied to all text in a panel, in radians.
///
/// Attach to the same entity as a
/// [`DiegeticPanel`](crate::DiegeticPanel) to rotate the hue of every
/// vertex color in the panel's text mesh. This is a GPU-side effect —
/// changing it does not trigger layout recomputation or mesh rebuilds.
///
/// Individual text elements retain their per-element colors set via
/// `TextConfig::with_color`. This rotation shifts all of them by the
/// same amount. A value of `TAU / 3` (~2.09) shifts reds to greens,
/// greens to blues, etc. A full `TAU` (6.28) cycles back to the original
/// colors.
///
/// See the `text_stress` example for usage.
#[derive(Component, Default, Clone, Copy, Debug, Reflect)]
pub struct HueOffset(pub f32);
