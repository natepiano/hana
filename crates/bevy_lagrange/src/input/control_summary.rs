use bevy::prelude::*;
use bevy_enhanced_input::prelude::Binding;
use bevy_enhanced_input::prelude::ModKeys;

use super::ActionBindingEntry;
use super::CameraInputGamepadSelectionPolicy;
use super::CameraInteractionSources;
use super::CameraSemanticAction;
use super::HeldActionBindingEntry;
use super::HeldCameraAction;
use super::OrbitCamBindings;
use super::OrbitCamButtonDragZoom;
use super::OrbitCamInteractionKind;
use super::OrbitCamManual;
use super::OrbitCamPreset;
use super::OrbitCamTouchBinding;
use super::OrbitCamTrackpadScroll;
use super::bindings::InputBindingDescriptor;
use super::bindings::InputBindingEntry;

/// Point-in-time display summary of the controls configured for an `OrbitCam`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OrbitCamControlSummary {
    /// Camera type label.
    pub camera_label: String,
    /// Label for the selected input mode category.
    pub mode_label:   String,
    /// Value for the selected input mode category.
    pub mode_value:   String,
    /// Display rows that describe the selected input mode.
    pub rows:         Vec<OrbitCamControlRow>,
}

/// One display row in an [`OrbitCamControlSummary`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OrbitCamControlRow {
    /// Interaction kind controlled by this row.
    pub kind:                       OrbitCamInteractionKind,
    /// Human-readable binding label.
    pub label:                      String,
    /// Sources associated with this binding.
    pub camera_interaction_sources: CameraInteractionSources,
}

/// Describes the effective `OrbitCam` controls from the same precedence used by
/// camera input mode selection: manual input, custom bindings, then preset.
#[must_use]
pub fn describe_orbit_cam_controls(
    preset: Option<&OrbitCamPreset>,
    bindings: Option<&OrbitCamBindings>,
    manual: Option<&OrbitCamManual>,
) -> OrbitCamControlSummary {
    if manual.is_some() {
        return describe_manual_controls();
    }

    if let Some(bindings) = bindings {
        return describe_bindings("Bindings", "Custom", bindings);
    }

    let preset = preset.copied().unwrap_or_default();
    describe_preset(preset)
}

fn describe_preset(preset: OrbitCamPreset) -> OrbitCamControlSummary {
    let mode_value = preset_mode_value(preset);
    match preset.to_bindings() {
        Ok(bindings) => describe_bindings("Preset", mode_value, &bindings),
        Err(_) => OrbitCamControlSummary {
            camera_label: "OrbitCam".to_string(),
            mode_label:   "Preset".to_string(),
            mode_value:   mode_value.to_string(),
            rows:         Vec::new(),
        },
    }
}

fn describe_manual_controls() -> OrbitCamControlSummary {
    OrbitCamControlSummary {
        camera_label: "OrbitCam".to_string(),
        mode_label:   "Input".to_string(),
        mode_value:   "Manual".to_string(),
        rows:         vec![
            control_row(
                OrbitCamInteractionKind::Orbit,
                "App-authored input",
                CameraInteractionSources::MANUAL,
            ),
            control_row(
                OrbitCamInteractionKind::Pan,
                "App-authored input",
                CameraInteractionSources::MANUAL,
            ),
            control_row(
                OrbitCamInteractionKind::Zoom,
                "App-authored input",
                CameraInteractionSources::MANUAL,
            ),
        ],
    }
}

fn describe_bindings(
    mode_label: &str,
    mode_value: &str,
    bindings: &OrbitCamBindings,
) -> OrbitCamControlSummary {
    let mut rows = Vec::new();

    rows.extend(
        bindings
            .orbit()
            .entries()
            .iter()
            .map(|entry| describe_held_entry(entry, OrbitCamInteractionKind::Orbit)),
    );
    rows.extend(
        bindings
            .trackpad_orbit()
            .iter()
            .copied()
            .map(|trackpad| describe_trackpad(trackpad, OrbitCamInteractionKind::Orbit)),
    );

    rows.extend(
        bindings
            .pan()
            .entries()
            .iter()
            .map(|entry| describe_held_entry(entry, OrbitCamInteractionKind::Pan)),
    );
    rows.extend(
        bindings
            .trackpad_pan()
            .iter()
            .copied()
            .map(|trackpad| describe_trackpad(trackpad, OrbitCamInteractionKind::Pan)),
    );

    rows.extend(
        bindings
            .zoom_smooth()
            .entries()
            .iter()
            .map(|entry| describe_held_entry(entry, OrbitCamInteractionKind::Zoom)),
    );
    rows.extend(
        bindings
            .zoom_coarse()
            .entries()
            .iter()
            .map(|entry| describe_action_entry(entry, OrbitCamInteractionKind::Zoom)),
    );

    if bindings.mouse_wheel_zoom().is_some() {
        rows.push(control_row(
            OrbitCamInteractionKind::Zoom,
            "Wheel",
            CameraInteractionSources::WHEEL,
        ));
    }

    rows.extend(
        bindings
            .trackpad_zoom()
            .iter()
            .copied()
            .map(|trackpad| describe_trackpad(trackpad, OrbitCamInteractionKind::Zoom)),
    );

    if let Some(button_drag) = bindings.button_drag_zoom() {
        rows.push(describe_button_drag(button_drag));
    }

    if bindings.pinch_zoom() {
        rows.push(control_row(
            OrbitCamInteractionKind::Zoom,
            "Pinch",
            CameraInteractionSources::PINCH,
        ));
    }

    if let Some(touch) = bindings.touch() {
        rows.extend(describe_touch(touch));
    }

    if bindings.gamepad() == CameraInputGamepadSelectionPolicy::Active
        && !rows.iter().any(|row| {
            row.camera_interaction_sources
                .contains(CameraInteractionSources::GAMEPAD)
        })
    {
        rows.push(control_row(
            OrbitCamInteractionKind::Orbit,
            "Gamepad bindings",
            CameraInteractionSources::GAMEPAD,
        ));
    }

    OrbitCamControlSummary {
        camera_label: "OrbitCam".to_string(),
        mode_label: mode_label.to_string(),
        mode_value: mode_value.to_string(),
        rows,
    }
}

fn describe_held_entry<A: HeldCameraAction>(
    entry: &HeldActionBindingEntry<A>,
    kind: OrbitCamInteractionKind,
) -> OrbitCamControlRow {
    let label = held_binding_stem(entry);
    control_row(kind, label, entry.sources())
}

fn describe_action_entry<A: CameraSemanticAction>(
    entry: &ActionBindingEntry<A>,
    kind: OrbitCamInteractionKind,
) -> OrbitCamControlRow {
    let label = descriptor_stem(entry.binding_descriptor(), entry.sources());
    control_row(kind, label, entry.sources())
}

fn describe_trackpad(
    trackpad: OrbitCamTrackpadScroll,
    kind: OrbitCamInteractionKind,
) -> OrbitCamControlRow {
    control_row(
        kind,
        trackpad_stem(trackpad.mod_keys),
        CameraInteractionSources::SMOOTH_SCROLL,
    )
}

fn describe_button_drag(button_drag: OrbitCamButtonDragZoom) -> OrbitCamControlRow {
    control_row(
        OrbitCamInteractionKind::Zoom,
        format!("{} drag", mouse_button_label(button_drag.button)),
        CameraInteractionSources::MOUSE,
    )
}

fn describe_touch(touch: OrbitCamTouchBinding) -> Vec<OrbitCamControlRow> {
    match touch {
        OrbitCamTouchBinding::OneFingerOrbit => vec![
            touch_row(OrbitCamInteractionKind::Orbit, "One finger touch"),
            touch_row(OrbitCamInteractionKind::Pan, "Two finger touch"),
            touch_row(OrbitCamInteractionKind::Zoom, "Two finger touch"),
        ],
        OrbitCamTouchBinding::TwoFingerOrbit => vec![
            touch_row(OrbitCamInteractionKind::Pan, "One finger touch"),
            touch_row(OrbitCamInteractionKind::Orbit, "Two finger touch"),
            touch_row(OrbitCamInteractionKind::Zoom, "Two finger touch"),
        ],
    }
}

fn touch_row(kind: OrbitCamInteractionKind, label: &'static str) -> OrbitCamControlRow {
    control_row(kind, label, CameraInteractionSources::TOUCH)
}

fn held_binding_stem<A: HeldCameraAction>(entry: &HeldActionBindingEntry<A>) -> String {
    if let Some(label) = mouse_drag_stem(entry.motion_descriptor(), entry.engagement_descriptor()) {
        return label;
    }

    let motion = descriptor_stem(entry.motion_descriptor(), entry.sources());
    if entry.motion_descriptor() == entry.engagement_descriptor() {
        return motion;
    }

    let engagement = descriptor_stem(entry.engagement_descriptor(), entry.sources());
    format!("{motion} + {engagement}")
}

fn mouse_drag_stem(
    motion: &InputBindingDescriptor,
    engagement: &InputBindingDescriptor,
) -> Option<String> {
    let motion_mod_keys = mouse_motion_mod_keys(motion)?;
    let (button, button_mod_keys) = engagement.mouse_button_engagement()?;
    if motion_mod_keys != button_mod_keys {
        return None;
    }

    Some(with_mod_keys(
        button_mod_keys,
        format!("{} drag", mouse_button_label(button)),
    ))
}

fn mouse_motion_mod_keys(descriptor: &InputBindingDescriptor) -> Option<ModKeys> {
    descriptor
        .entries_slice()
        .iter()
        .find_map(|entry| match entry.binding {
            Binding::MouseMotion { mod_keys } => Some(mod_keys),
            Binding::Keyboard { .. }
            | Binding::MouseButton { .. }
            | Binding::MouseWheel { .. }
            | Binding::GamepadButton(_)
            | Binding::GamepadAxis(_)
            | Binding::AnyKey
            | Binding::None => None,
        })
}

fn descriptor_stem(
    descriptor: &InputBindingDescriptor,
    sources: CameraInteractionSources,
) -> String {
    if let Some(label) = keyboard_stem(descriptor.entries_slice()) {
        return label;
    }
    if let Some(label) = gamepad_axis_stem(descriptor.entries_slice()) {
        return label;
    }
    if let Some(label) = gamepad_button_stem(descriptor.entries_slice()) {
        return label;
    }
    if let Some((button, mod_keys)) = descriptor.mouse_button_engagement() {
        return with_mod_keys(mod_keys, mouse_button_label(button));
    }
    if descriptor.entries_slice().iter().any(|entry| {
        matches!(
            entry.binding,
            Binding::MouseWheel { .. } | Binding::MouseMotion { .. }
        )
    }) {
        return "Mouse".to_string();
    }

    source_stem(sources).to_string()
}

fn keyboard_stem(entries: &[InputBindingEntry]) -> Option<String> {
    let keys = entries
        .iter()
        .map(|entry| match entry.binding {
            Binding::Keyboard { key, mod_keys } => Some((key, mod_keys)),
            Binding::MouseButton { .. }
            | Binding::MouseMotion { .. }
            | Binding::MouseWheel { .. }
            | Binding::GamepadButton(_)
            | Binding::GamepadAxis(_)
            | Binding::AnyKey
            | Binding::None => None,
        })
        .collect::<Option<Vec<_>>>()?;

    match keys.as_slice() {
        [] => None,
        [(key, mod_keys)] => Some(with_mod_keys(*mod_keys, key_label(*key))),
        [(positive, _), (negative, _)] => Some(format!(
            "{} / {}",
            key_label(*positive),
            key_label(*negative)
        )),
        keys if is_arrow_keys(keys) => Some("Arrow keys".to_string()),
        keys if is_wasd(keys) => Some("WASD".to_string()),
        _ => Some("Keyboard keys".to_string()),
    }
}

fn gamepad_axis_stem(entries: &[InputBindingEntry]) -> Option<String> {
    let gamepad_axes = entries
        .iter()
        .map(|entry| match entry.binding {
            Binding::GamepadAxis(axis) => Some(axis),
            Binding::Keyboard { .. }
            | Binding::MouseButton { .. }
            | Binding::MouseMotion { .. }
            | Binding::MouseWheel { .. }
            | Binding::GamepadButton(_)
            | Binding::AnyKey
            | Binding::None => None,
        })
        .collect::<Option<Vec<_>>>()?;

    match gamepad_axes.as_slice() {
        [] => None,
        [GamepadAxis::LeftStickX, GamepadAxis::LeftStickY]
        | [GamepadAxis::LeftStickY, GamepadAxis::LeftStickX] => Some("Left stick".to_string()),
        [GamepadAxis::RightStickX, GamepadAxis::RightStickY]
        | [GamepadAxis::RightStickY, GamepadAxis::RightStickX] => Some("Right stick".to_string()),
        [single_axis] => Some(gamepad_axis_label(*single_axis)),
        _ => Some("Gamepad axes".to_string()),
    }
}

fn gamepad_button_stem(entries: &[InputBindingEntry]) -> Option<String> {
    let buttons = entries
        .iter()
        .map(|entry| match entry.binding {
            Binding::GamepadButton(button) => Some(button),
            Binding::Keyboard { .. }
            | Binding::MouseButton { .. }
            | Binding::MouseMotion { .. }
            | Binding::MouseWheel { .. }
            | Binding::GamepadAxis(_)
            | Binding::AnyKey
            | Binding::None => None,
        })
        .collect::<Option<Vec<_>>>()?;

    match buttons.as_slice() {
        [] => None,
        [GamepadButton::RightTrigger2, GamepadButton::LeftTrigger2]
        | [GamepadButton::LeftTrigger2, GamepadButton::RightTrigger2] => {
            Some("RT / LT".to_string())
        },
        [positive, negative] => Some(format!(
            "{} / {}",
            gamepad_button_label(*positive),
            gamepad_button_label(*negative)
        )),
        [button] => Some(gamepad_button_label(*button)),
        _ => Some("Gamepad buttons".to_string()),
    }
}

fn is_arrow_keys(keys: &[(KeyCode, ModKeys)]) -> bool {
    contains_key(keys, KeyCode::ArrowUp)
        && contains_key(keys, KeyCode::ArrowRight)
        && contains_key(keys, KeyCode::ArrowDown)
        && contains_key(keys, KeyCode::ArrowLeft)
}

fn is_wasd(keys: &[(KeyCode, ModKeys)]) -> bool {
    contains_key(keys, KeyCode::KeyW)
        && contains_key(keys, KeyCode::KeyD)
        && contains_key(keys, KeyCode::KeyS)
        && contains_key(keys, KeyCode::KeyA)
}

fn contains_key(keys: &[(KeyCode, ModKeys)], key: KeyCode) -> bool {
    keys.iter().any(|(candidate, _)| *candidate == key)
}

fn trackpad_stem(mod_keys: ModKeys) -> String {
    if mod_keys.is_empty() {
        "Trackpad".to_string()
    } else {
        format!("{}+trackpad", compact_mod_keys(mod_keys))
    }
}

fn with_mod_keys(mod_keys: ModKeys, label: String) -> String {
    if mod_keys.is_empty() {
        label
    } else {
        format!("{}+{label}", compact_mod_keys(mod_keys))
    }
}

fn compact_mod_keys(mod_keys: ModKeys) -> String { mod_keys.to_string().replace(" + ", "+") }

fn key_label(key: KeyCode) -> String {
    match key {
        KeyCode::ArrowUp => "Up".to_string(),
        KeyCode::ArrowRight => "Right".to_string(),
        KeyCode::ArrowDown => "Down".to_string(),
        KeyCode::ArrowLeft => "Left".to_string(),
        KeyCode::Equal => "+".to_string(),
        KeyCode::Minus => "-".to_string(),
        KeyCode::Space => "Space".to_string(),
        KeyCode::KeyA => "A".to_string(),
        KeyCode::KeyD => "D".to_string(),
        KeyCode::KeyH => "H".to_string(),
        KeyCode::KeyM => "M".to_string(),
        KeyCode::KeyS => "S".to_string(),
        KeyCode::KeyW => "W".to_string(),
        _ => debug_name(key, "Key"),
    }
}

fn mouse_button_label(button: MouseButton) -> String {
    match button {
        MouseButton::Left => "Left".to_string(),
        MouseButton::Right => "Right".to_string(),
        MouseButton::Middle => "MMB".to_string(),
        _ => format!("{button:?}"),
    }
}

fn gamepad_axis_label(axis: GamepadAxis) -> String {
    match axis {
        GamepadAxis::LeftStickX => "Left stick X".to_string(),
        GamepadAxis::LeftStickY => "Left stick Y".to_string(),
        GamepadAxis::RightStickX => "Right stick X".to_string(),
        GamepadAxis::RightStickY => "Right stick Y".to_string(),
        _ => debug_name(axis, ""),
    }
}

fn gamepad_button_label(button: GamepadButton) -> String {
    match button {
        GamepadButton::LeftTrigger => "L1".to_string(),
        GamepadButton::RightTrigger => "R1".to_string(),
        GamepadButton::LeftTrigger2 => "LT".to_string(),
        GamepadButton::RightTrigger2 => "RT".to_string(),
        _ => debug_name(button, ""),
    }
}

fn debug_name(value: impl std::fmt::Debug, prefix: &str) -> String {
    let name = format!("{value:?}");
    name.strip_prefix(prefix).unwrap_or(&name).to_string()
}

const fn source_stem(sources: CameraInteractionSources) -> &'static str {
    if sources.contains(CameraInteractionSources::KEYBOARD) {
        "Keyboard binding"
    } else if sources.contains(CameraInteractionSources::GAMEPAD) {
        "Gamepad binding"
    } else if sources.contains(CameraInteractionSources::MOUSE) {
        "Mouse binding"
    } else if sources.contains(CameraInteractionSources::WHEEL) {
        "Wheel"
    } else if sources.contains(CameraInteractionSources::SMOOTH_SCROLL) {
        "Trackpad"
    } else if sources.contains(CameraInteractionSources::PINCH) {
        "Pinch"
    } else if sources.contains(CameraInteractionSources::TOUCH) {
        "Touch"
    } else if sources.contains(CameraInteractionSources::MANUAL) {
        "Manual input"
    } else {
        "Input binding"
    }
}

const fn preset_mode_value(preset: OrbitCamPreset) -> &'static str {
    match preset {
        OrbitCamPreset::SimpleMouse => "SimpleMouse",
        OrbitCamPreset::BlenderLike => "BlenderLike",
    }
}

fn control_row(
    kind: OrbitCamInteractionKind,
    label: impl Into<String>,
    sources: CameraInteractionSources,
) -> OrbitCamControlRow {
    OrbitCamControlRow {
        kind,
        label: label.into(),
        camera_interaction_sources: sources,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::OrbitCamBindingsError;
    use crate::input::OrbitCamInputBinding;
    use crate::input::OrbitCamMouseDrag;

    #[test]
    fn blender_like_summary_reflects_preset_bindings() {
        let summary = describe_orbit_cam_controls(Some(&OrbitCamPreset::BlenderLike), None, None);
        let labels = summary_labels(&summary);

        assert_eq!(summary.camera_label, "OrbitCam");
        assert_eq!(summary.mode_label, "Preset");
        assert_eq!(summary.mode_value, "BlenderLike");
        assert!(labels.contains(&"MMB drag"));
        assert!(labels.contains(&"Trackpad"));
        assert!(labels.contains(&"Shift+MMB drag"));
        assert!(labels.contains(&"Shift+trackpad"));
        assert!(labels.contains(&"Wheel"));
        assert!(labels.contains(&"Ctrl+trackpad"));
        assert!(labels.contains(&"Pinch"));
    }

    #[test]
    fn custom_bindings_override_preset_summary() -> Result<(), OrbitCamBindingsError> {
        let bindings = OrbitCamBindings::builder()
            .orbit(OrbitCamMouseDrag::new(MouseButton::Right))
            .build()?;
        let summary =
            describe_orbit_cam_controls(Some(&OrbitCamPreset::BlenderLike), Some(&bindings), None);
        let labels = summary_labels(&summary);

        assert_eq!(summary.mode_label, "Bindings");
        assert_eq!(summary.mode_value, "Custom");
        assert!(labels.contains(&"Right drag"));
        assert!(!labels.contains(&"MMB drag"));

        Ok(())
    }

    #[test]
    fn keyboard_bindings_show_key_groups() -> Result<(), OrbitCamBindingsError> {
        let orbit_keys = OrbitCamInputBinding::cardinal_keys(
            KeyCode::ArrowUp,
            KeyCode::ArrowRight,
            KeyCode::ArrowDown,
            KeyCode::ArrowLeft,
        );
        let pan_keys = OrbitCamInputBinding::cardinal_keys(
            KeyCode::KeyW,
            KeyCode::KeyD,
            KeyCode::KeyS,
            KeyCode::KeyA,
        );
        let zoom_keys = OrbitCamInputBinding::bidirectional_keys(KeyCode::Equal, KeyCode::Minus);
        let bindings = OrbitCamBindings::builder()
            .orbit(orbit_keys)
            .pan(pan_keys)
            .zoom(zoom_keys)
            .build()?;
        let summary = describe_orbit_cam_controls(None, Some(&bindings), None);
        let labels = summary_labels(&summary);

        assert!(labels.contains(&"Arrow keys"));
        assert!(labels.contains(&"WASD"));
        assert!(labels.contains(&"+ / -"));

        Ok(())
    }

    #[test]
    fn manual_summary_wins_over_other_modes() -> Result<(), OrbitCamBindingsError> {
        let bindings = OrbitCamBindings::builder()
            .orbit(OrbitCamMouseDrag::new(MouseButton::Right))
            .build()?;
        let manual = OrbitCamManual;
        let summary = describe_orbit_cam_controls(
            Some(&OrbitCamPreset::BlenderLike),
            Some(&bindings),
            Some(&manual),
        );
        let labels = summary_labels(&summary);

        assert_eq!(summary.mode_label, "Input");
        assert_eq!(summary.mode_value, "Manual");
        assert!(labels.contains(&"App-authored input"));
        assert!(summary.rows.iter().all(|row| {
            row.camera_interaction_sources
                .contains(CameraInteractionSources::MANUAL)
        }));

        Ok(())
    }

    fn summary_labels(summary: &OrbitCamControlSummary) -> Vec<&str> {
        summary.rows.iter().map(|row| row.label.as_str()).collect()
    }
}
