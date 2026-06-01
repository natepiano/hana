//! Typography overlay — renders font-level metric lines and per-glyph
//! bounding boxes as retained gizmos on any [`TextContent`](crate::TextContent)
//! entity.
//!
//! Uses [`ComputedWorldText`](crate::render::ComputedWorldText) data
//! populated by the renderer to ensure exact alignment with the rendered
//! slug glyphs — no independent layout computation.
//!
//! Metric lines are drawn using Bevy's retained [`GizmoAsset`](bevy::prelude::GizmoAsset)
//! (spawned once, not redrawn every frame). Labels are spawned as
//! [`TextContent`](crate::TextContent) children.

mod constants;
mod glyph;
mod labels;
mod lifecycle;
mod metric_lines;
mod pipeline;
mod scaling;

use bevy::color::palettes::css::WHITE;
use bevy::prelude::*;
use constants::DEFAULT_OVERLAY_EXTEND;
use constants::DEFAULT_OVERLAY_LABEL_SIZE;
pub(super) use lifecycle::on_overlay_added;
pub(super) use lifecycle::on_overlay_removed;
pub(super) use pipeline::build_typography_overlay;

use super::constants::DEFAULT_LINE_WIDTH;
use crate::WorldText;
use crate::layout::GlyphShadowMode;
use crate::layout::TextStyle;
use crate::panel::SurfaceShadow;

/// Spawns a world-space overlay label as a one-element [`WorldText`] panel child
/// of `container`.
///
/// The label's [`TextStyle`] anchor becomes the panel anchor, so the text sits at
/// `transform` exactly as the bare-[`TextContent`](crate::TextContent) spawns did
/// before [`WorldText`] one-element panels replaced the standalone world-text
/// render path (a bare `TextContent` no longer renders on its own).
pub(super) fn spawn_overlay_label(
    commands: &mut Commands,
    container: Entity,
    text: impl Into<String>,
    style: TextStyle,
    transform: Transform,
) {
    let anchor = style.anchor();
    commands.entity(container).with_child(
        WorldText::new(text)
            .style(style)
            .anchor(anchor)
            .transform(transform)
            .build(),
    );
}

/// Whether per-glyph bounding box annotations are visible.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Reflect)]
pub enum GlyphMetricVisibility {
    /// Glyph bounding boxes and origin dots are drawn.
    Shown,
    /// Glyph-level annotations are suppressed.
    Hidden,
}

/// Attach to a [`TextContent`](crate::TextContent) entity to render typography
/// metric annotations.
///
/// Built into the library as a debug tool — only available when the
/// `typography_overlay` feature is enabled.
///
/// Metric lines are rendered as retained gizmos (spawned once, not
/// redrawn every frame). Labels are spawned as [`TextContent`](crate::TextContent)
/// children.
///
/// # Example
///
/// ```ignore
/// commands.spawn((
///     TextContent::new("Typography"),
///     TextStyle::new(48.0),
///     TypographyOverlay::default(),
///     Transform::from_xyz(0.0, 2.0, 0.0),
/// ));
/// ```
#[derive(Component, Clone, Debug, Reflect)]
pub struct TypographyOverlay {
    /// Show font-level metric lines (ascent, descent, cap height, x-height,
    /// baseline, top, bottom).
    pub font_metrics:   GlyphMetricVisibility,
    /// Show per-glyph bounding boxes as gizmo lines (from font bbox).
    pub glyph_metrics:  GlyphMetricVisibility,
    /// Show text labels on the metric lines.
    pub labels:         GlyphMetricVisibility,
    /// Color for overlay lines and labels (includes alpha).
    pub color:          Color,
    /// Gizmo line width in pixels.
    pub line_width:     f32,
    /// Font size for metric labels.
    pub label_size:     f32,
    /// How far annotation lines extend beyond text bounds (in layout units).
    pub extend:         f32,
    /// Whether overlay geometry and labels cast shadows.
    pub surface_shadow: SurfaceShadow,
}

impl Default for TypographyOverlay {
    fn default() -> Self {
        Self {
            font_metrics:   GlyphMetricVisibility::Shown,
            glyph_metrics:  GlyphMetricVisibility::Shown,
            labels:         GlyphMetricVisibility::Shown,
            color:          Color::from(WHITE),
            line_width:     DEFAULT_LINE_WIDTH,
            label_size:     DEFAULT_OVERLAY_LABEL_SIZE,
            extend:         DEFAULT_OVERLAY_EXTEND,
            surface_shadow: SurfaceShadow::Off,
        }
    }
}

impl TypographyOverlay {
    /// Sets whether overlay constituents cast shadows.
    #[must_use]
    pub const fn with_shadow(mut self, surface_shadow: SurfaceShadow) -> Self {
        self.surface_shadow = surface_shadow;
        self
    }

    const fn label_shadow_mode(&self) -> GlyphShadowMode {
        match self.surface_shadow {
            SurfaceShadow::Off => GlyphShadowMode::None,
            SurfaceShadow::On => GlyphShadowMode::Cast,
        }
    }
}

/// Marker for the single container entity that holds all overlay children.
/// Spawned by [`on_overlay_added`] and despawned by [`on_overlay_removed`].
#[derive(Component)]
pub struct OverlayContainer;

/// Hidden mesh entity representing the full overlay extent for fit/home
/// operations.
#[derive(Component)]
pub struct OverlayBoundingBox;
