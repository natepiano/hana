//! Public guidance configuration types: `CameraGuidance`, `CameraGuidanceRow`.

use bevy::prelude::*;
use bevy_diegetic::Anchor;
use bevy_lagrange::CameraInteractionSources;
use bevy_lagrange::OrbitCamControlRow;
use bevy_lagrange::OrbitCamControlSummary;
use bevy_lagrange::OrbitCamInteractionKind;
use bevy_lagrange::OrbitCamPreset;
use bevy_lagrange::describe_orbit_cam_controls;

/// Whether active source labels are rendered in the guidance panel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceVisibility {
    /// Source labels are rendered.
    Visible,
    /// Source labels are hidden.
    Hidden,
}

/// Data-driven camera control metadata shown by [`SprinkleBuilder`](crate::SprinkleBuilder)
/// examples.
#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct CameraGuidance {
    pub(super) anchor:            Anchor,
    pub(super) title:             Option<String>,
    pub(super) mode_label:        Option<String>,
    pub(super) mode_value:        Option<String>,
    pub(super) content:           CameraGuidanceContent,
    pub(super) source_visibility: SourceVisibility,
}

impl Default for CameraGuidance {
    fn default() -> Self { Self::auto() }
}

impl CameraGuidance {
    /// Builds guidance rows from the camera's actual input-mode components.
    #[must_use]
    pub const fn auto() -> Self {
        Self {
            anchor:            Anchor::BottomRight,
            title:             None,
            mode_label:        None,
            mode_value:        None,
            content:           CameraGuidanceContent::Auto,
            source_visibility: SourceVisibility::Visible,
        }
    }

    /// Builds guidance rows for a built-in orbit-camera preset.
    #[must_use]
    pub fn for_preset(preset: OrbitCamPreset) -> Self {
        Self::from_summary(describe_orbit_cam_controls(Some(&preset), None, None))
    }

    /// Builds custom camera guidance rows.
    #[must_use]
    pub fn custom(rows: impl IntoIterator<Item = CameraGuidanceRow>) -> Self {
        Self {
            anchor:            Anchor::BottomRight,
            title:             None,
            mode_label:        None,
            mode_value:        None,
            content:           CameraGuidanceContent::Rows(rows.into_iter().collect()),
            source_visibility: SourceVisibility::Visible,
        }
    }

    /// Sets the panel screen anchor.
    #[must_use]
    pub const fn with_anchor(mut self, anchor: Anchor) -> Self {
        self.anchor = anchor;
        self
    }

    /// Replaces the panel title.
    #[must_use]
    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// Controls whether active source labels are rendered.
    #[must_use]
    pub const fn with_source_visibility(mut self, source_visibility: SourceVisibility) -> Self {
        self.source_visibility = source_visibility;
        self
    }

    /// Returns explicitly configured rows.
    ///
    /// Auto guidance is resolved when the panel binds to a camera.
    #[must_use]
    pub fn rows(&self) -> &[CameraGuidanceRow] {
        match &self.content {
            CameraGuidanceContent::Auto => &[],
            CameraGuidanceContent::Rows(rows) => rows,
        }
    }

    fn from_summary(summary: OrbitCamControlSummary) -> Self {
        Self {
            anchor:            Anchor::BottomRight,
            title:             Some(summary.camera_label),
            mode_label:        Some(summary.mode_label),
            mode_value:        Some(summary.mode_value),
            content:           CameraGuidanceContent::Rows(
                summary.rows.into_iter().map(Into::into).collect(),
            ),
            source_visibility: SourceVisibility::Visible,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum CameraGuidanceContent {
    Auto,
    Rows(Vec<CameraGuidanceRow>),
}

/// A single camera guidance row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CameraGuidanceRow {
    kind:                       OrbitCamInteractionKind,
    label:                      String,
    camera_interaction_sources: CameraInteractionSources,
}

impl CameraGuidanceRow {
    /// Creates a row for an interaction kind.
    #[must_use]
    pub fn new(kind: OrbitCamInteractionKind, label: impl Into<String>) -> Self {
        Self {
            kind,
            label: label.into(),
            camera_interaction_sources: CameraInteractionSources::NONE,
        }
    }

    /// Highlights this row only when the active sources intersect `sources`.
    #[must_use]
    pub const fn with_camera_interaction_sources(
        mut self,
        camera_interaction_sources: CameraInteractionSources,
    ) -> Self {
        self.camera_interaction_sources = camera_interaction_sources;
        self
    }

    /// Returns the interaction kind matched by this row.
    #[must_use]
    pub const fn kind(&self) -> OrbitCamInteractionKind { self.kind }

    /// Returns this row's camera-interaction source metadata.
    #[must_use]
    pub const fn camera_interaction_sources(&self) -> CameraInteractionSources {
        self.camera_interaction_sources
    }

    /// Returns the display label.
    #[must_use]
    pub fn label(&self) -> &str { &self.label }
}

impl From<OrbitCamControlRow> for CameraGuidanceRow {
    fn from(row: OrbitCamControlRow) -> Self {
        Self::new(row.kind, row.label)
            .with_camera_interaction_sources(row.camera_interaction_sources)
    }
}
