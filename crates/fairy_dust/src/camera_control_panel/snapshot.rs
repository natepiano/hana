//! Snapshot resolution: derives a `CameraGuidanceSnapshot` (panel-ready rows
//! and labels) from the `CameraGuidance` config plus the camera's input-mode
//! components, and supplies the label helpers used by the layout pass.

use bevy::prelude::*;
use bevy_enhanced_input::prelude::ModKeys;
use bevy_lagrange::CameraInteractionSources;
use bevy_lagrange::ControlSpeed;
use bevy_lagrange::OrbitCamInputMode;
use bevy_lagrange::OrbitCamInteractionKind;
use bevy_lagrange::OrbitCamPreset;
use bevy_lagrange::OrbitCamSlowMode;
use bevy_lagrange::ZoomDirection;
use bevy_lagrange::describe_orbit_cam_controls;

use super::guidance::CameraGuidance;
use super::guidance::CameraGuidanceContent;
use super::guidance::CameraGuidanceRow;

#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub(super) struct CameraGuidanceSnapshot {
    pub(super) camera_label:            String,
    pub(super) mode_label:              String,
    pub(super) mode_value:              String,
    pub(super) slow_mode_binding_label: Option<String>,
    pub(super) rows:                    Vec<CameraGuidanceRow>,
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
        let has_effective_rows = mode_has_effective_rows(mode);
        return CameraGuidanceSnapshot {
            camera_label: name_label
                .or(title_label)
                .unwrap_or_else(|| "OrbitCam".to_string()),
            mode_label: guidance.mode_label.clone().unwrap_or(mode_label),
            mode_value: guidance.mode_value.clone().unwrap_or(mode_value),
            slow_mode_binding_label: slow_mode_binding_label(mode, has_effective_rows),
            rows,
        };
    }

    let summary = mode.map_or_else(
        || describe_orbit_cam_controls(&OrbitCamInputMode::default()),
        describe_orbit_cam_controls,
    );
    CameraGuidanceSnapshot {
        camera_label:            name_label.or(title_label).unwrap_or(summary.camera_label),
        mode_label:              summary.mode_label,
        mode_value:              summary.mode_value,
        slow_mode_binding_label: slow_mode_binding_label(mode, !summary.rows.is_empty()),
        rows:                    summary.rows.into_iter().map(Into::into).collect(),
    }
}

fn mode_has_effective_rows(mode: Option<&OrbitCamInputMode>) -> bool {
    mode.is_some_and(|mode| !describe_orbit_cam_controls(mode).rows.is_empty())
}

fn resolve_mode_labels(mode: Option<&OrbitCamInputMode>) -> (String, String) {
    let Some(mode) = mode else {
        let preset = OrbitCamPreset::default();
        return ("Preset".to_string(), preset.kind().name().to_string());
    };
    match mode {
        OrbitCamInputMode::Preset(preset) => {
            ("Preset".to_string(), preset.kind().name().to_string())
        },
        OrbitCamInputMode::Bindings(_) => ("Bindings".to_string(), "Custom".to_string()),
        OrbitCamInputMode::Manual => ("Input".to_string(), "Manual".to_string()),
        _ => ("Input".to_string(), "Custom".to_string()),
    }
}

fn slow_mode_binding_label(
    mode: Option<&OrbitCamInputMode>,
    has_effective_rows: bool,
) -> Option<String> {
    if !has_effective_rows {
        return None;
    }

    let slow_mode = match mode? {
        OrbitCamInputMode::Preset(preset) => preset.to_bindings().ok()?.slow_mode().cloned(),
        OrbitCamInputMode::Bindings(bindings) => bindings.slow_mode().cloned(),
        _ => None,
    }?;
    Some(slow_mode_binding_hint(&slow_mode).to_string())
}

const fn slow_mode_binding_hint(slow_mode: &OrbitCamSlowMode) -> &'static str {
    match (slow_mode.toggle_key, slow_mode.mod_keys) {
        (KeyCode::KeyS, ModKeys::ALT) => "alt-s",
        _ => "slow",
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
    use bevy_lagrange::InputGain;
    use bevy_lagrange::OrbitCamBindings;
    use bevy_lagrange::OrbitCamBindingsError;
    use bevy_lagrange::OrbitCamBlenderLikePreset;
    use bevy_lagrange::OrbitCamMouseDrag;
    use bevy_lagrange::OrbitCamScalePolicy;
    use bevy_lagrange::OrbitCamSensitivity;

    use super::*;

    const CUSTOM_SLOW_SCALE: f32 = 0.25;

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

    #[test]
    fn blender_like_snapshot_includes_slow_mode_binding_hint() {
        let snapshot = resolve_guidance_snapshot(
            None,
            None,
            Some(&OrbitCamInputMode::with_preset(
                OrbitCamPreset::blender_like(),
            )),
        );

        assert_eq!(snapshot.slow_mode_binding_label.as_deref(), Some("alt-s"));
    }

    #[test]
    fn tuned_blender_like_snapshot_keeps_preset_label_and_slow_mode_hint() {
        let disabled = InputGain::DISABLED.0;
        let preset = OrbitCamBlenderLikePreset::default()
            .mouse_sensitivity(OrbitCamSensitivity::uniform(disabled))
            .smooth_scroll_sensitivity(OrbitCamSensitivity::uniform(disabled));
        let snapshot =
            resolve_guidance_snapshot(None, None, Some(&OrbitCamInputMode::with_preset(preset)));
        let labels = row_labels(&snapshot);

        assert_eq!(snapshot.mode_label, "Preset");
        assert_eq!(snapshot.mode_value, "BlenderLike");
        assert_eq!(snapshot.slow_mode_binding_label.as_deref(), Some("alt-s"));
        assert!(labels.contains(&"pinch out"));
        assert!(labels.contains(&"pinch in"));
        assert!(!labels.contains(&"mmb drag"));
        assert!(!labels.contains(&"smooth-scroll"));
    }

    #[test]
    fn custom_slow_mode_snapshot_omits_hint_when_all_controls_are_disabled()
    -> Result<(), OrbitCamBindingsError> {
        let disabled = InputGain::DISABLED.0;
        let bindings = OrbitCamBindings::builder()
            .slow_mode(OrbitCamSlowMode {
                toggle_key: KeyCode::KeyS,
                mod_keys:   ModKeys::ALT,
                scale:      OrbitCamScalePolicy {
                    normal: InputGain::DEFAULT.0,
                    slow:   CUSTOM_SLOW_SCALE,
                },
            })
            .orbit(OrbitCamMouseDrag::new(MouseButton::Middle).with_sensitivity(disabled))
            .build()?;
        let snapshot =
            resolve_guidance_snapshot(None, None, Some(&OrbitCamInputMode::Bindings(bindings)));

        assert_eq!(snapshot.slow_mode_binding_label, None);
        assert!(snapshot.rows.is_empty());
        Ok(())
    }

    #[test]
    fn simple_mouse_snapshot_omits_slow_mode_hint() {
        let snapshot = resolve_guidance_snapshot(
            None,
            None,
            Some(&OrbitCamInputMode::with_preset(
                OrbitCamPreset::simple_mouse(),
            )),
        );

        assert_eq!(snapshot.slow_mode_binding_label, None);
    }

    fn row_labels(snapshot: &CameraGuidanceSnapshot) -> Vec<&str> {
        snapshot.rows.iter().map(CameraGuidanceRow::label).collect()
    }
}
