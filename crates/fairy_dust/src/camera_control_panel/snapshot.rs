//! Snapshot resolution: derives a `CameraGuidanceSnapshot` (panel-ready rows
//! and labels) from the `CameraGuidance` config plus the camera's input-mode
//! components, and supplies the label helpers used by the layout pass.

use bevy::prelude::*;
use bevy_lagrange::CameraInteractionSources;
use bevy_lagrange::OrbitCamInputMode;
use bevy_lagrange::OrbitCamInteractionKind;
use bevy_lagrange::OrbitCamPreset;
use bevy_lagrange::describe_orbit_cam_controls;

use super::guidance::CameraGuidance;
use super::guidance::CameraGuidanceContent;
use super::guidance::CameraGuidanceRow;
use super::guidance::SourceVisibility;
use super::preset_switch::PRESET_SWITCH_HINT;

#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub(super) struct CameraGuidanceSnapshot {
    pub(super) camera_label:       String,
    pub(super) mode_label:         String,
    pub(super) mode_value:         String,
    pub(super) rows:               Vec<CameraGuidanceRow>,
    pub(super) source_visibility:  SourceVisibility,
    /// Hint row shown under the source line when the camera is in `Preset`
    /// mode, naming the key that cycles presets.
    pub(super) preset_switch_hint: Option<String>,
}

pub(super) fn resolve_guidance_snapshot(
    name: Option<&Name>,
    guidance: Option<&CameraGuidance>,
    mode: Option<&OrbitCamInputMode>,
) -> CameraGuidanceSnapshot {
    let name_label = name.map(|n| n.as_str().to_string());
    let title_label = guidance.and_then(|g| g.title.clone());
    let source_visibility = guidance.map_or(SourceVisibility::Visible, |g| g.source_visibility);
    let explicit = guidance.and_then(|g| match &g.content {
        CameraGuidanceContent::Auto => None,
        CameraGuidanceContent::Rows(rows) => Some((g, rows.clone())),
    });

    if let Some((guidance, rows)) = explicit {
        let (mode_label, mode_value) = resolve_mode_labels(mode);
        return CameraGuidanceSnapshot {
            camera_label: name_label
                .or(title_label)
                .unwrap_or_else(|| "OrbitCam".to_string()),
            mode_label: guidance.mode_label.clone().unwrap_or(mode_label),
            mode_value: guidance.mode_value.clone().unwrap_or(mode_value),
            rows,
            source_visibility,
            preset_switch_hint: preset_switch_hint(mode),
        };
    }

    let summary = mode.map_or_else(
        || describe_orbit_cam_controls(&OrbitCamInputMode::default()),
        describe_orbit_cam_controls,
    );
    CameraGuidanceSnapshot {
        camera_label: name_label.or(title_label).unwrap_or(summary.camera_label),
        mode_label: summary.mode_label,
        mode_value: summary.mode_value,
        rows: summary.rows.into_iter().map(Into::into).collect(),
        source_visibility,
        preset_switch_hint: preset_switch_hint(mode),
    }
}

/// The preset switcher only acts on `Preset` cameras, so the hint is shown for
/// `Preset` mode (and the default, which is a preset) and hidden otherwise.
fn preset_switch_hint(mode: Option<&OrbitCamInputMode>) -> Option<String> {
    match mode {
        None | Some(OrbitCamInputMode::Preset(_)) => Some(PRESET_SWITCH_HINT.to_string()),
        _ => None,
    }
}

fn resolve_mode_labels(mode: Option<&OrbitCamInputMode>) -> (String, String) {
    let Some(mode) = mode else {
        let preset = OrbitCamPreset::default();
        return ("Preset".to_string(), preset_mode_value(preset).to_string());
    };
    match mode {
        OrbitCamInputMode::Preset(preset) => {
            ("Preset".to_string(), preset_mode_value(*preset).to_string())
        },
        OrbitCamInputMode::Bindings(_) => ("Bindings".to_string(), "Custom".to_string()),
        OrbitCamInputMode::Manual => ("Input".to_string(), "Manual".to_string()),
        _ => ("Input".to_string(), "Custom".to_string()),
    }
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
