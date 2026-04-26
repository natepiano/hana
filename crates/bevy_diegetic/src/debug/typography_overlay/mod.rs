//! Typography overlay — renders font-level metric lines and per-glyph
//! bounding boxes as retained gizmos on any [`WorldText`](crate::WorldText)
//! entity.
//!
//! Uses [`ComputedWorldText`](crate::render::ComputedWorldText) data
//! populated by the renderer to ensure exact alignment with the rendered
//! MSDF quads — no independent layout computation.
//!
//! Metric lines are drawn using Bevy's retained [`GizmoAsset`](bevy::prelude::GizmoAsset)
//! (spawned once, not redrawn every frame). Labels are spawned as
//! [`WorldText`](crate::WorldText) children.

mod glyph;
mod labels;
mod lifecycle;
mod metric_lines;
mod pipeline;
mod scaling;

use bevy::color::palettes::css::WHITE;
use bevy::prelude::*;
pub(super) use lifecycle::emit_typography_overlay_ready;
pub(super) use lifecycle::on_overlay_added;
pub(super) use lifecycle::on_overlay_removed;
pub(super) use pipeline::build_typography_overlay;

use super::constants::DEFAULT_LINE_WIDTH;
use crate::layout::GlyphShadowMode;
use crate::panel::SurfaceShadow;

/// Whether per-glyph bounding box annotations are visible.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Reflect)]
pub enum GlyphMetricVisibility {
    /// Glyph bounding boxes and origin dots are drawn.
    Shown,
    /// Glyph-level annotations are suppressed.
    Hidden,
}

/// Attach to a [`WorldText`](crate::WorldText) entity to render typography
/// metric annotations. Built into the library as a debug tool — only
/// available when the `typography_overlay` feature is enabled.
///
/// Metric lines are rendered as retained gizmos (spawned once, not
/// redrawn every frame). Labels are spawned as [`WorldText`](crate::WorldText)
/// children.
///
/// # Example
///
/// ```ignore
/// commands.spawn((
///     WorldText::new("Typography"),
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
            label_size:     6.0,
            extend:         8.0,
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
            SurfaceShadow::On => GlyphShadowMode::Text,
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

/// Fired on the [`WorldText`](crate::WorldText) entity when its
/// [`TypographyOverlay`] and all descendant label text are fully rendered
/// and interactable.
#[derive(EntityEvent)]
pub struct TypographyOverlayReady {
    /// The hidden overlay-bounds entity that is ready to use as a fit target.
    #[event_target]
    pub entity: Entity,
    /// The [`WorldText`](crate::WorldText) entity that owns the overlay.
    pub owner:  Entity,
}

/// Internal marker: overlay labels have been spawned, waiting for their
/// glyphs to finish and transforms to propagate.
#[derive(Component)]
pub(super) struct AwaitingOverlayReady {
    ready_target: Entity,
}
