//! Snapshot resolution: derives a `CameraGuidanceSnapshot` (panel-ready rows
//! and labels) from the `CameraGuidance` config plus the camera's input-mode
//! components, and supplies the label helpers used by the layout pass.

use bevy::prelude::*;
use bevy_lagrange::CameraInteractionSources;
use bevy_lagrange::ControlSpeed;
use bevy_lagrange::OrbitCamInputMode;
use bevy_lagrange::OrbitCamInteractionKind;
use bevy_lagrange::OrbitCamPreset;
use bevy_lagrange::ZoomDirection;
use bevy_lagrange::describe_orbit_cam_controls;

use super::guidance::CameraGuidance;
use super::guidance::CameraGuidanceContent;
use super::guidance::CameraGuidanceRow;

#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub(super) struct CameraGuidanceSnapshot {
    pub(super) camera_label: String,
    pub(super) mode_label:   String,
    pub(super) mode_value:   String,
    pub(super) rows:         Vec<CameraGuidanceRow>,
}

pub(super) fn resolve_guidance_snapshot(
    name: Option<&Name>,
    guidance: Option<&CameraGuidance>,
    mode: Option<&OrbitCamInputMode>,
) -> CameraGuidanceSnapshot {
    let name_label = name.map(|n| n.as_str().to_string());
    let title_label = guidance.and_then(|g| g.title.clone());
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
        };
    }

    let summary = mode.map_or_else(
        || describe_orbit_cam_controls(&OrbitCamInputMode::default()),
        describe_orbit_cam_controls,
    );
    CameraGuidanceSnapshot {
        camera_label: name_label.or(title_label).unwrap_or(summary.camera_label),
        mode_label:   summary.mode_label,
        mode_value:   summary.mode_value,
        rows:         summary.rows.into_iter().map(Into::into).collect(),
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
        OrbitCamPreset::Keyboard => "Keyboard",
        OrbitCamPreset::SimpleMouseKeyboard => "SimpleMouseKeyboard",
        OrbitCamPreset::BlenderLikeKeyboard => "BlenderLikeKeyboard",
        OrbitCamPreset::Gamepad => "Gamepad",
        _ => "Custom",
    }
}

pub(super) fn row_active(
    row: &CameraGuidanceRow,
    sources: CameraInteractionSources,
    live_zoom_direction: Option<ZoomDirection>,
) -> bool {
    if sources.is_empty() || !sources.intersects(row.camera_interaction_sources()) {
        return false;
    }
    match row.zoom_direction() {
        // Orbit, pan, and unsplit zoom rows match on source alone.
        None => true,
        // A directional zoom row lights only when the live zoom matches it. Until
        // a direction is known (no zoom engaged yet) fall back to source-only so
        // the row can still light rather than going dark.
        Some(direction) => live_zoom_direction.is_none_or(|live| live == direction),
    }
}

pub(super) const fn action_label(
    kind: OrbitCamInteractionKind,
    direction: Option<ZoomDirection>,
) -> &'static str {
    match (kind, direction) {
        (OrbitCamInteractionKind::Orbit, _) => "Orbit",
        (OrbitCamInteractionKind::Pan, _) => "Pan",
        (OrbitCamInteractionKind::Zoom, Some(ZoomDirection::In)) => "Zoom In",
        (OrbitCamInteractionKind::Zoom, Some(ZoomDirection::Out)) => "Zoom Out",
        (OrbitCamInteractionKind::Zoom, None) => "Zoom",
        _ => "",
    }
}

pub(super) const fn speed_label(speed: ControlSpeed) -> &'static str {
    match speed {
        ControlSpeed::Normal => "Normal",
        ControlSpeed::Slow => "Slow",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn zoom_row(direction: Option<ZoomDirection>) -> CameraGuidanceRow {
        let row = CameraGuidanceRow::new(OrbitCamInteractionKind::Zoom, "rt")
            .with_camera_interaction_sources(CameraInteractionSources::GAMEPAD);
        match direction {
            Some(direction) => row.with_zoom_direction(direction),
            None => row,
        }
    }

    #[test]
    fn directional_zoom_row_lights_only_on_matching_live_direction() {
        let row = zoom_row(Some(ZoomDirection::In));
        assert!(row_active(
            &row,
            CameraInteractionSources::GAMEPAD,
            Some(ZoomDirection::In)
        ));
        assert!(!row_active(
            &row,
            CameraInteractionSources::GAMEPAD,
            Some(ZoomDirection::Out)
        ));
    }

    #[test]
    fn directional_zoom_row_lights_when_live_direction_unknown() {
        let row = zoom_row(Some(ZoomDirection::Out));
        assert!(row_active(&row, CameraInteractionSources::GAMEPAD, None));
    }

    #[test]
    fn non_directional_row_lights_regardless_of_live_direction() {
        let row = zoom_row(None);
        assert!(row_active(
            &row,
            CameraInteractionSources::GAMEPAD,
            Some(ZoomDirection::In)
        ));
        assert!(row_active(
            &row,
            CameraInteractionSources::GAMEPAD,
            Some(ZoomDirection::Out)
        ));
    }

    #[test]
    fn row_inactive_without_source_overlap() {
        let row = zoom_row(Some(ZoomDirection::In));
        assert!(!row_active(
            &row,
            CameraInteractionSources::WHEEL,
            Some(ZoomDirection::In)
        ));
        assert!(!row_active(
            &row,
            CameraInteractionSources::NONE,
            Some(ZoomDirection::In)
        ));
    }
}
