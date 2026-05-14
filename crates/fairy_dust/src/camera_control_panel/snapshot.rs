//! Snapshot resolution: derives a `CameraGuidanceSnapshot` (panel-ready rows
//! and labels) from the `CameraGuidance` config plus the camera's input-mode
//! components, and supplies the label helpers used by the layout pass.

use bevy::prelude::*;
use bevy_lagrange::CameraInteractionSources;
use bevy_lagrange::OrbitCamBindings;
use bevy_lagrange::OrbitCamControlSummary;
use bevy_lagrange::OrbitCamInteractionKind;
use bevy_lagrange::OrbitCamManual;
use bevy_lagrange::OrbitCamPreset;
use bevy_lagrange::describe_orbit_cam_controls;

use super::config::CameraGuidance;
use super::config::CameraGuidanceContent;
use super::config::CameraGuidanceRow;

#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub(super) struct CameraGuidanceSnapshot {
    pub(super) camera_label: String,
    pub(super) mode_label:   String,
    pub(super) mode_value:   String,
    pub(super) rows:         Vec<CameraGuidanceRow>,
    pub(super) show_sources: bool,
}

pub(super) fn resolve_guidance_snapshot(
    guidance: &CameraGuidance,
    preset: Option<&OrbitCamPreset>,
    bindings: Option<&OrbitCamBindings>,
    manual: Option<&OrbitCamManual>,
) -> CameraGuidanceSnapshot {
    match &guidance.content {
        CameraGuidanceContent::Auto => {
            let summary = describe_orbit_cam_controls(preset, bindings, manual);
            snapshot_from_summary(guidance, summary)
        },
        CameraGuidanceContent::Rows(rows) => {
            let (mode_label, mode_value) = resolve_mode_labels(preset, bindings, manual);
            CameraGuidanceSnapshot {
                camera_label: guidance
                    .title
                    .clone()
                    .unwrap_or_else(|| "OrbitCam".to_string()),
                mode_label:   guidance.mode_label.clone().unwrap_or(mode_label),
                mode_value:   guidance.mode_value.clone().unwrap_or(mode_value),
                rows:         rows.clone(),
                show_sources: guidance.show_sources,
            }
        },
    }
}

fn snapshot_from_summary(
    guidance: &CameraGuidance,
    summary: OrbitCamControlSummary,
) -> CameraGuidanceSnapshot {
    CameraGuidanceSnapshot {
        camera_label: guidance.title.clone().unwrap_or(summary.camera_label),
        mode_label:   summary.mode_label,
        mode_value:   summary.mode_value,
        rows:         summary.rows.into_iter().map(Into::into).collect(),
        show_sources: guidance.show_sources,
    }
}

fn resolve_mode_labels(
    preset: Option<&OrbitCamPreset>,
    bindings: Option<&OrbitCamBindings>,
    manual: Option<&OrbitCamManual>,
) -> (String, String) {
    if manual.is_some() {
        return ("Input".to_string(), "Manual".to_string());
    }
    if bindings.is_some() {
        return ("Bindings".to_string(), "Custom".to_string());
    }
    let preset = preset.copied().unwrap_or_default();
    ("Preset".to_string(), preset_mode_value(preset).to_string())
}

const fn preset_mode_value(preset: OrbitCamPreset) -> &'static str {
    match preset {
        OrbitCamPreset::SimpleMouse => "SimpleMouse",
        OrbitCamPreset::BlenderLike => "BlenderLike",
        _ => "Custom",
    }
}

pub(super) const fn row_active(row: &CameraGuidanceRow, sources: CameraInteractionSources) -> bool {
    if sources.is_empty() {
        return false;
    }
    sources.intersects(row.camera_interaction_sources())
}

pub(super) fn source_label(sources: CameraInteractionSources) -> String {
    let mut labels = Vec::new();
    push_source_label(
        &mut labels,
        sources,
        CameraInteractionSources::MOUSE,
        "button-drag",
    );
    push_source_label(
        &mut labels,
        sources,
        CameraInteractionSources::WHEEL,
        "wheel",
    );
    push_source_label(
        &mut labels,
        sources,
        CameraInteractionSources::SMOOTH_SCROLL,
        "smooth-scroll",
    );
    push_source_label(
        &mut labels,
        sources,
        CameraInteractionSources::PINCH,
        "pinch",
    );
    push_source_label(
        &mut labels,
        sources,
        CameraInteractionSources::TOUCH,
        "touch",
    );
    push_source_label(
        &mut labels,
        sources,
        CameraInteractionSources::KEYBOARD,
        "keyboard",
    );
    push_source_label(
        &mut labels,
        sources,
        CameraInteractionSources::GAMEPAD,
        "gamepad",
    );
    push_source_label(
        &mut labels,
        sources,
        CameraInteractionSources::MANUAL,
        "manual",
    );

    if labels.is_empty() {
        "idle".to_string()
    } else {
        labels.join(" + ")
    }
}

pub(super) const fn kind_label(kind: OrbitCamInteractionKind) -> &'static str {
    match kind {
        OrbitCamInteractionKind::Orbit => "Orbit",
        OrbitCamInteractionKind::Pan => "Pan",
        OrbitCamInteractionKind::Zoom => "Zoom",
        _ => "",
    }
}

fn push_source_label(
    labels: &mut Vec<&'static str>,
    sources: CameraInteractionSources,
    source: CameraInteractionSources,
    label: &'static str,
) {
    if sources.contains(source) {
        labels.push(label);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_label_lists_sources_without_brackets() {
        let sources = CameraInteractionSources::MOUSE.union(CameraInteractionSources::PINCH);

        assert_eq!(source_label(sources), "button-drag + pinch");
        assert_eq!(source_label(CameraInteractionSources::NONE), "idle");
    }
}
