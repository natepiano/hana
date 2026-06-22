use bevy::prelude::*;
use bevy_enhanced_input::prelude::Binding;
use bevy_enhanced_input::prelude::ModKeys;

use super::ActionBindingEntry;
use super::CameraInputGamepadSelectionPolicy;
use super::CameraInteractionSources;
use super::CameraSemanticAction;
use super::HeldActionBindingEntry;
use super::HeldCameraAction;
use super::OrbitCamBindingWithSensitivity;
use super::OrbitCamBindings;
use super::OrbitCamButtonDragZoom;
use super::OrbitCamGateInput;
use super::OrbitCamGatePolarity;
use super::OrbitCamInputMode;
use super::OrbitCamInteractionKind;
use super::OrbitCamSlowMode;
use super::OrbitCamTouchBinding;
use super::OrbitCamTouchBindingConfig;
use super::OrbitCamTrackpadScroll;
use super::ZoomInversion;
use super::bindings::BindingGates;
use super::bindings::InputAxisTransform;
use super::bindings::InputBindingDescriptor;
use super::bindings::InputBindingEntry;
use super::constants::APP_AUTHORED_INPUT_ROW_LABEL;
use super::constants::CUSTOM_BINDINGS_MODE_VALUE;
use super::constants::CUSTOM_INPUT_ROW_LABEL;
use super::constants::GAMEPAD_BINDING_SOURCE_LABEL;
use super::constants::GAMEPAD_BINDINGS_ROW_LABEL;
use super::constants::INPUT_BINDING_SOURCE_LABEL;
use super::constants::INPUT_MODE_LABEL;
use super::constants::KEYBOARD_BINDING_SOURCE_LABEL;
use super::constants::MANUAL_INPUT_SOURCE_LABEL;
use super::constants::MANUAL_MODE_VALUE;
use super::constants::MOUSE_BINDING_SOURCE_LABEL;
use super::constants::MOUSE_DESCRIPTOR_LABEL;
use super::constants::ONE_FINGER_TOUCH_ROW_LABEL;
use super::constants::ORBIT_CAM_CAMERA_LABEL;
use super::constants::PINCH_SOURCE_LABEL;
use super::constants::PINCH_ZOOM_IN_LABEL;
use super::constants::PINCH_ZOOM_OUT_LABEL;
use super::constants::PRESET_MODE_LABEL;
use super::constants::SMOOTH_SCROLL_ZOOM_IN_LABEL;
use super::constants::SMOOTH_SCROLL_ZOOM_OUT_LABEL;
use super::constants::TOUCH_SOURCE_LABEL;
use super::constants::TRACKPAD_SOURCE_LABEL;
use super::constants::TWO_FINGER_TOUCH_ROW_LABEL;
use super::constants::WHEEL_SOURCE_LABEL;
use super::constants::WHEEL_ZOOM_IN_LABEL;
use super::constants::WHEEL_ZOOM_OUT_LABEL;

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
    /// Whether this binding is the normal or the slow (precise) variant.
    pub speed:                      ControlSpeed,
    /// The zoom direction this row drives, or `None` for rows that are not
    /// direction-specific: orbit, pan, or a bidirectional zoom kept as one row.
    pub zoom_direction:             Option<ZoomDirection>,
}

/// The direction a zoom affordance drives the camera.
///
/// Every zoom row from a built-in preset carries one of these so the control
/// panel can highlight only the engaged direction — pressing the zoom-in
/// trigger lights only the zoom-in row.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Reflect)]
pub enum ZoomDirection {
    /// Moves the camera toward its target.
    In,
    /// Moves the camera away from its target.
    Out,
}

/// Distinguishes a normal binding from its slow, precise counterpart — for the
/// gamepad preset, the slow variant is the one gated behind `rb`/`lb`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Reflect)]
pub enum ControlSpeed {
    /// Default speed binding.
    #[default]
    Normal,
    /// Slow, precise binding engaged while a modifier gate is held.
    Slow,
}

impl OrbitCamControlRow {
    /// Overrides the binding speed variant.
    #[must_use]
    pub const fn with_speed(mut self, speed: ControlSpeed) -> Self {
        self.speed = speed;
        self
    }

    /// Tags this row with the zoom direction it drives.
    #[must_use]
    pub const fn with_zoom_direction(mut self, zoom_direction: ZoomDirection) -> Self {
        self.zoom_direction = Some(zoom_direction);
        self
    }
}

/// Describes the effective `OrbitCam` controls for an input mode.
#[must_use]
pub fn describe_orbit_cam_controls(mode: &OrbitCamInputMode) -> OrbitCamControlSummary {
    match mode {
        OrbitCamInputMode::Preset(preset) => match preset.to_bindings() {
            Ok(bindings) => describe_bindings(PRESET_MODE_LABEL, preset.kind().name(), &bindings),
            Err(_) => OrbitCamControlSummary {
                camera_label: ORBIT_CAM_CAMERA_LABEL.to_string(),
                mode_label:   PRESET_MODE_LABEL.to_string(),
                mode_value:   preset.kind().name().to_string(),
                rows:         Vec::new(),
            },
        },
        OrbitCamInputMode::Bindings(bindings) => {
            describe_bindings(INPUT_MODE_LABEL, CUSTOM_BINDINGS_MODE_VALUE, bindings)
        },
        OrbitCamInputMode::Manual => describe_manual_controls(),
    }
}

pub(crate) fn effective_slow_mode(bindings: &OrbitCamBindings) -> Option<&OrbitCamSlowMode> {
    let slow_mode = bindings.slow_mode()?;
    (!effective_control_rows(bindings).is_empty()).then_some(slow_mode)
}

fn describe_manual_controls() -> OrbitCamControlSummary {
    OrbitCamControlSummary {
        camera_label: ORBIT_CAM_CAMERA_LABEL.to_string(),
        mode_label:   INPUT_MODE_LABEL.to_string(),
        mode_value:   MANUAL_MODE_VALUE.to_string(),
        rows:         vec![
            control_row(
                OrbitCamInteractionKind::Orbit,
                APP_AUTHORED_INPUT_ROW_LABEL,
                CameraInteractionSources::MANUAL,
            ),
            control_row(
                OrbitCamInteractionKind::Pan,
                APP_AUTHORED_INPUT_ROW_LABEL,
                CameraInteractionSources::MANUAL,
            ),
            control_row(
                OrbitCamInteractionKind::Zoom,
                APP_AUTHORED_INPUT_ROW_LABEL,
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
    OrbitCamControlSummary {
        camera_label: ORBIT_CAM_CAMERA_LABEL.to_string(),
        mode_label:   mode_label.to_string(),
        mode_value:   mode_value.to_string(),
        rows:         effective_control_rows(bindings),
    }
}

fn effective_control_rows(bindings: &OrbitCamBindings) -> Vec<OrbitCamControlRow> {
    let mut rows = Vec::new();

    rows.extend(
        bindings
            .orbit()
            .enabled_entries()
            .map(|entry| describe_held_entry(entry, OrbitCamInteractionKind::Orbit)),
    );
    rows.extend(
        bindings
            .enabled_trackpad_orbit()
            .map(|(_, trackpad)| trackpad)
            .map(|trackpad| describe_trackpad(trackpad, OrbitCamInteractionKind::Orbit)),
    );

    rows.extend(
        bindings
            .pan()
            .enabled_entries()
            .map(|entry| describe_held_entry(entry, OrbitCamInteractionKind::Pan)),
    );
    rows.extend(
        bindings
            .enabled_trackpad_pan()
            .map(|(_, trackpad)| trackpad)
            .map(|trackpad| describe_trackpad(trackpad, OrbitCamInteractionKind::Pan)),
    );

    // Every zoom source shows one row per direction so zoom in and zoom out read
    // separately. `inversion_sign` flips the synthesized in/out tags for sources
    // routed through the inverting zoom path (wheel, pinch, smooth-scroll); the
    // native trigger/key bindings derive their direction from their own scale.
    let inversion_sign = zoom_inversion_sign(bindings.zoom_inversion());

    for entry in bindings.zoom_smooth().enabled_entries() {
        rows.extend(describe_zoom_held_entry(entry));
    }
    rows.extend(
        bindings
            .zoom_coarse()
            .enabled_entries()
            .map(|entry| describe_action_entry(entry, OrbitCamInteractionKind::Zoom)),
    );

    if bindings.enabled_mouse_wheel_zoom().is_some() {
        push_zoom_pair(
            &mut rows,
            WHEEL_ZOOM_IN_LABEL,
            WHEEL_ZOOM_OUT_LABEL,
            CameraInteractionSources::WHEEL,
            inversion_sign,
        );
    }

    for (_, trackpad) in bindings.enabled_trackpad_zoom() {
        push_trackpad_zoom_pair(&mut rows, trackpad, inversion_sign);
    }

    if let Some(button_drag) = bindings.enabled_button_drag_zoom() {
        rows.push(describe_button_drag(button_drag));
    }

    if bindings.enabled_pinch_zoom_binding().is_some() {
        push_zoom_pair(
            &mut rows,
            PINCH_ZOOM_IN_LABEL,
            PINCH_ZOOM_OUT_LABEL,
            CameraInteractionSources::PINCH,
            inversion_sign,
        );
    }

    if let Some(touch) = bindings.enabled_touch_config() {
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
            GAMEPAD_BINDINGS_ROW_LABEL,
            CameraInteractionSources::GAMEPAD,
        ));
    }

    rows
}

fn describe_held_entry<A: HeldCameraAction>(
    entry: &HeldActionBindingEntry<A>,
    kind: OrbitCamInteractionKind,
) -> OrbitCamControlRow {
    let label = held_binding_stem(entry);
    control_row(kind, label, entry.sources()).with_speed(entry.speed())
}

fn describe_action_entry<A: CameraSemanticAction>(
    entry: &ActionBindingEntry<A>,
    kind: OrbitCamInteractionKind,
) -> OrbitCamControlRow {
    let label = descriptor_stem(entry.binding_descriptor(), entry.sources());
    control_row(kind, label, entry.sources())
}

fn describe_trackpad(
    trackpad: OrbitCamBindingWithSensitivity<OrbitCamTrackpadScroll>,
    kind: OrbitCamInteractionKind,
) -> OrbitCamControlRow {
    control_row(
        kind,
        trackpad_stem(trackpad.binding().mod_keys),
        CameraInteractionSources::SMOOTH_SCROLL,
    )
}

/// Describes one smooth-zoom held binding. A single signed binding (a gamepad
/// trigger or one zoom key) stays one row tagged with its direction; a
/// bidirectional binding (the keyboard `+`/`-` pair) splits into one row per
/// signed entry so each direction displays on its own.
fn describe_zoom_held_entry<A: HeldCameraAction>(
    entry: &HeldActionBindingEntry<A>,
) -> Vec<OrbitCamControlRow> {
    let motion_entries = entry.enabled_motion_entries().collect::<Vec<_>>();

    if motion_entries.len() == 1 {
        let direction = zoom_direction_from_sign(entry_net_sign(motion_entries[0]));
        return vec![
            control_row(
                OrbitCamInteractionKind::Zoom,
                held_binding_stem(entry),
                entry.sources(),
            )
            .with_speed(entry.speed())
            .with_zoom_direction(direction),
        ];
    }

    let split = motion_entries
        .iter()
        .map(|&motion_entry| {
            zoom_entry_label(motion_entry).map(|stem| {
                let direction = zoom_direction_from_sign(entry_net_sign(motion_entry));
                control_row(
                    OrbitCamInteractionKind::Zoom,
                    with_required_gates(entry.gates(), stem),
                    entry.sources(),
                )
                .with_speed(entry.speed())
                .with_zoom_direction(direction)
            })
        })
        .collect::<Option<Vec<_>>>();

    split.unwrap_or_else(|| vec![describe_held_entry(entry, OrbitCamInteractionKind::Zoom)])
}

/// Pushes the zoom-in and zoom-out rows for a bidirectional zoom source whose
/// two directions are synthesized rather than read from separate bindings
/// (mouse wheel, pinch, smooth-scroll). `inversion_sign` is `-1.0` when the
/// camera inverts zoom, which flips both tags so they keep matching the live
/// zoom sign.
fn push_zoom_pair(
    rows: &mut Vec<OrbitCamControlRow>,
    zoom_in_label: &str,
    zoom_out_label: &str,
    sources: CameraInteractionSources,
    inversion_sign: f32,
) {
    rows.push(
        control_row(OrbitCamInteractionKind::Zoom, zoom_in_label, sources)
            .with_zoom_direction(zoom_direction_from_sign(inversion_sign)),
    );
    rows.push(
        control_row(OrbitCamInteractionKind::Zoom, zoom_out_label, sources)
            .with_zoom_direction(zoom_direction_from_sign(-inversion_sign)),
    );
}

/// Pushes the smooth-scroll zoom-in and zoom-out rows, prefixing the gesture
/// label with any required modifier keys (matching `trackpad_stem`).
fn push_trackpad_zoom_pair(
    rows: &mut Vec<OrbitCamControlRow>,
    trackpad: OrbitCamBindingWithSensitivity<OrbitCamTrackpadScroll>,
    inversion_sign: f32,
) {
    let zoom_in = with_mod_keys(
        trackpad.binding().mod_keys,
        SMOOTH_SCROLL_ZOOM_IN_LABEL.to_string(),
    );
    let zoom_out = with_mod_keys(
        trackpad.binding().mod_keys,
        SMOOTH_SCROLL_ZOOM_OUT_LABEL.to_string(),
    );
    push_zoom_pair(
        rows,
        &zoom_in,
        &zoom_out,
        CameraInteractionSources::SMOOTH_SCROLL,
        inversion_sign,
    );
}

/// Returns the net sign a binding entry contributes to its action, combining the
/// scale modifier's sign with any negating axis transform. `+1.0` zooms in,
/// `-1.0` zooms out.
fn entry_net_sign(entry: &InputBindingEntry) -> f32 {
    let scale_sign = entry.modifiers().scale().map_or(1.0, f32::signum);
    let transform_sign = match entry.modifiers().axis_transform() {
        InputAxisTransform::Negate | InputAxisTransform::SwizzleNegate => -1.0,
        InputAxisTransform::None | InputAxisTransform::Swizzle => 1.0,
    };
    scale_sign * transform_sign
}

fn zoom_direction_from_sign(sign: f32) -> ZoomDirection {
    if sign < 0.0 {
        ZoomDirection::Out
    } else {
        ZoomDirection::In
    }
}

const fn zoom_inversion_sign(inversion: ZoomInversion) -> f32 {
    match inversion {
        ZoomInversion::Normal => 1.0,
        ZoomInversion::Inverted => -1.0,
    }
}

/// Returns a per-entry label for a single binding entry of a bidirectional zoom
/// binding, or `None` for an entry kind that has no standalone label.
fn zoom_entry_label(entry: &InputBindingEntry) -> Option<String> {
    match entry.binding() {
        Binding::Keyboard { key, mod_keys } => Some(with_mod_keys(mod_keys, key_label(key))),
        Binding::GamepadButton(button) => Some(gamepad_button_label(button)),
        Binding::MouseButton { .. }
        | Binding::MouseMotion { .. }
        | Binding::MouseWheel { .. }
        | Binding::GamepadAxis(_)
        | Binding::AnyKey
        | Binding::Custom(_)
        | Binding::None => None,
    }
}

fn describe_button_drag(
    button_drag: OrbitCamBindingWithSensitivity<OrbitCamButtonDragZoom>,
) -> OrbitCamControlRow {
    control_row(
        OrbitCamInteractionKind::Zoom,
        format!("{} drag", mouse_button_label(button_drag.binding().button)),
        CameraInteractionSources::MOUSE,
    )
}

fn describe_touch(touch: OrbitCamTouchBindingConfig) -> Vec<OrbitCamControlRow> {
    let mut rows = Vec::new();
    match touch.binding() {
        OrbitCamTouchBinding::OneFingerOrbit => {
            push_enabled_touch_row(
                &mut rows,
                touch.orbit_enabled(),
                OrbitCamInteractionKind::Orbit,
                ONE_FINGER_TOUCH_ROW_LABEL,
            );
            push_enabled_touch_row(
                &mut rows,
                touch.pan_enabled(),
                OrbitCamInteractionKind::Pan,
                TWO_FINGER_TOUCH_ROW_LABEL,
            );
            push_enabled_touch_row(
                &mut rows,
                touch.zoom_enabled(),
                OrbitCamInteractionKind::Zoom,
                TWO_FINGER_TOUCH_ROW_LABEL,
            );
        },
        OrbitCamTouchBinding::TwoFingerOrbit => {
            push_enabled_touch_row(
                &mut rows,
                touch.pan_enabled(),
                OrbitCamInteractionKind::Pan,
                ONE_FINGER_TOUCH_ROW_LABEL,
            );
            push_enabled_touch_row(
                &mut rows,
                touch.orbit_enabled(),
                OrbitCamInteractionKind::Orbit,
                TWO_FINGER_TOUCH_ROW_LABEL,
            );
            push_enabled_touch_row(
                &mut rows,
                touch.zoom_enabled(),
                OrbitCamInteractionKind::Zoom,
                TWO_FINGER_TOUCH_ROW_LABEL,
            );
        },
    }
    rows
}

fn push_enabled_touch_row(
    rows: &mut Vec<OrbitCamControlRow>,
    enabled: bool,
    kind: OrbitCamInteractionKind,
    label: &'static str,
) {
    if enabled {
        rows.push(touch_row(kind, label));
    }
}

fn touch_row(kind: OrbitCamInteractionKind, label: &'static str) -> OrbitCamControlRow {
    control_row(kind, label, CameraInteractionSources::TOUCH)
}

fn held_binding_stem<A: HeldCameraAction>(entry: &HeldActionBindingEntry<A>) -> String {
    let stem = if let Some(label) =
        mouse_drag_stem(entry.motion_descriptor(), entry.engagement_descriptor())
    {
        label
    } else {
        let motion = descriptor_stem(entry.motion_descriptor(), entry.sources());
        if entry.motion_descriptor() == entry.engagement_descriptor() {
            motion
        } else {
            let engagement = descriptor_stem(entry.engagement_descriptor(), entry.sources());
            format!("{motion} + {engagement}")
        }
    };

    with_required_gates(entry.gates(), stem)
}

fn with_required_gates(gates: &BindingGates, label: String) -> String {
    let required = gates
        .entries()
        .iter()
        .filter(|gate| gate.polarity == OrbitCamGatePolarity::Required)
        .map(|gate| match gate.input {
            OrbitCamGateInput::GamepadButton(button) => gamepad_button_label(button),
            OrbitCamGateInput::Key(key) => key_label(key),
        })
        .collect::<Vec<_>>();
    if required.is_empty() {
        label
    } else {
        format!("{}+{label}", required.join("+"))
    }
}

fn mouse_drag_stem(
    motion: &InputBindingDescriptor,
    engagement: &InputBindingDescriptor,
) -> Option<String> {
    let motion_mod_keys = mouse_motion_mod_keys(motion)?;
    let (button, button_mod_keys) = engagement.enabled_mouse_button_engagement()?;
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
        .enabled_entries()
        .find_map(|entry| match entry.binding() {
            Binding::MouseMotion { mod_keys } => Some(mod_keys),
            Binding::Keyboard { .. }
            | Binding::MouseButton { .. }
            | Binding::MouseWheel { .. }
            | Binding::GamepadButton(_)
            | Binding::GamepadAxis(_)
            | Binding::AnyKey
            | Binding::Custom(_)
            | Binding::None => None,
        })
}

fn descriptor_stem(
    descriptor: &InputBindingDescriptor,
    sources: CameraInteractionSources,
) -> String {
    let entries = descriptor.enabled_entries().collect::<Vec<_>>();
    if let Some(label) = keyboard_stem(&entries) {
        return label;
    }
    if let Some(label) = gamepad_axis_stem(&entries) {
        return label;
    }
    if let Some(label) = gamepad_button_stem(&entries) {
        return label;
    }
    if let Some((button, mod_keys)) = descriptor.enabled_mouse_button_engagement() {
        return with_mod_keys(mod_keys, mouse_button_label(button));
    }
    if entries.iter().any(|entry| {
        matches!(
            entry.binding(),
            Binding::MouseWheel { .. } | Binding::MouseMotion { .. }
        )
    }) {
        return MOUSE_DESCRIPTOR_LABEL.to_string();
    }
    if entries
        .iter()
        .any(|entry| matches!(entry.binding(), Binding::Custom(_)))
    {
        return CUSTOM_INPUT_ROW_LABEL.to_string();
    }

    source_stem(sources).to_string()
}

fn keyboard_stem(entries: &[&InputBindingEntry]) -> Option<String> {
    let keys = entries
        .iter()
        .map(|entry| match entry.binding() {
            Binding::Keyboard { key, mod_keys } => Some((key, mod_keys)),
            Binding::MouseButton { .. }
            | Binding::MouseMotion { .. }
            | Binding::MouseWheel { .. }
            | Binding::GamepadButton(_)
            | Binding::GamepadAxis(_)
            | Binding::AnyKey
            | Binding::Custom(_)
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
        keys if is_arrow_keys(keys) => Some("arrow keys".to_string()),
        keys if is_wasd(keys) => Some("wasd".to_string()),
        _ => Some("keyboard keys".to_string()),
    }
}

fn gamepad_axis_stem(entries: &[&InputBindingEntry]) -> Option<String> {
    let gamepad_axes = entries
        .iter()
        .map(|entry| match entry.binding() {
            Binding::GamepadAxis(axis) => Some(axis),
            Binding::Keyboard { .. }
            | Binding::MouseButton { .. }
            | Binding::MouseMotion { .. }
            | Binding::MouseWheel { .. }
            | Binding::GamepadButton(_)
            | Binding::AnyKey
            | Binding::Custom(_)
            | Binding::None => None,
        })
        .collect::<Option<Vec<_>>>()?;

    match gamepad_axes.as_slice() {
        [] => None,
        [GamepadAxis::LeftStickX, GamepadAxis::LeftStickY]
        | [GamepadAxis::LeftStickY, GamepadAxis::LeftStickX] => Some("ls".to_string()),
        [GamepadAxis::RightStickX, GamepadAxis::RightStickY]
        | [GamepadAxis::RightStickY, GamepadAxis::RightStickX] => Some("rs".to_string()),
        [single_axis] => Some(gamepad_axis_label(*single_axis)),
        _ => Some("gamepad axes".to_string()),
    }
}

fn gamepad_button_stem(entries: &[&InputBindingEntry]) -> Option<String> {
    let buttons = entries
        .iter()
        .map(|entry| match entry.binding() {
            Binding::GamepadButton(button) => Some(button),
            Binding::Keyboard { .. }
            | Binding::MouseButton { .. }
            | Binding::MouseMotion { .. }
            | Binding::MouseWheel { .. }
            | Binding::GamepadAxis(_)
            | Binding::AnyKey
            | Binding::Custom(_)
            | Binding::None => None,
        })
        .collect::<Option<Vec<_>>>()?;

    match buttons.as_slice() {
        [] => None,
        [GamepadButton::RightTrigger2, GamepadButton::LeftTrigger2]
        | [GamepadButton::LeftTrigger2, GamepadButton::RightTrigger2] => {
            Some("rt / lt".to_string())
        },
        [positive, negative] => Some(format!(
            "{} / {}",
            gamepad_button_label(*positive),
            gamepad_button_label(*negative)
        )),
        [button] => Some(gamepad_button_label(*button)),
        _ => Some("gamepad buttons".to_string()),
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
        TRACKPAD_SOURCE_LABEL.to_string()
    } else {
        format!("{}+{TRACKPAD_SOURCE_LABEL}", compact_mod_keys(mod_keys))
    }
}

fn with_mod_keys(mod_keys: ModKeys, label: String) -> String {
    if mod_keys.is_empty() {
        label
    } else {
        format!("{}+{label}", compact_mod_keys(mod_keys))
    }
}

fn compact_mod_keys(mod_keys: ModKeys) -> String {
    mod_keys.to_string().replace(" + ", "+").to_lowercase()
}

fn key_label(key: KeyCode) -> String {
    match key {
        KeyCode::ArrowUp => "up".to_string(),
        KeyCode::ArrowRight => "right".to_string(),
        KeyCode::ArrowDown => "down".to_string(),
        KeyCode::ArrowLeft => "left".to_string(),
        KeyCode::Equal => "+".to_string(),
        KeyCode::Minus => "-".to_string(),
        KeyCode::Space => "space".to_string(),
        KeyCode::KeyA => "a".to_string(),
        KeyCode::KeyD => "d".to_string(),
        KeyCode::KeyH => "h".to_string(),
        KeyCode::KeyM => "m".to_string(),
        KeyCode::KeyS => "s".to_string(),
        KeyCode::KeyW => "w".to_string(),
        _ => debug_name(key, "Key"),
    }
}

fn mouse_button_label(button: MouseButton) -> String {
    match button {
        MouseButton::Left => "lmb".to_string(),
        MouseButton::Right => "rmb".to_string(),
        MouseButton::Middle => "mmb".to_string(),
        _ => format!("{button:?}").to_lowercase(),
    }
}

fn gamepad_axis_label(axis: GamepadAxis) -> String {
    match axis {
        GamepadAxis::LeftStickX => "ls x".to_string(),
        GamepadAxis::LeftStickY => "ls y".to_string(),
        GamepadAxis::RightStickX => "rs x".to_string(),
        GamepadAxis::RightStickY => "rs y".to_string(),
        _ => debug_name(axis, ""),
    }
}

fn gamepad_button_label(button: GamepadButton) -> String {
    match button {
        GamepadButton::LeftTrigger => "lb".to_string(),
        GamepadButton::RightTrigger => "rb".to_string(),
        GamepadButton::LeftTrigger2 => "lt".to_string(),
        GamepadButton::RightTrigger2 => "rt".to_string(),
        _ => debug_name(button, ""),
    }
}

fn debug_name(value: impl std::fmt::Debug, prefix: &str) -> String {
    let name = format!("{value:?}");
    name.strip_prefix(prefix).unwrap_or(&name).to_lowercase()
}

const fn source_stem(sources: CameraInteractionSources) -> &'static str {
    if sources.contains(CameraInteractionSources::KEYBOARD) {
        KEYBOARD_BINDING_SOURCE_LABEL
    } else if sources.contains(CameraInteractionSources::GAMEPAD) {
        GAMEPAD_BINDING_SOURCE_LABEL
    } else if sources.contains(CameraInteractionSources::MOUSE) {
        MOUSE_BINDING_SOURCE_LABEL
    } else if sources.contains(CameraInteractionSources::WHEEL) {
        WHEEL_SOURCE_LABEL
    } else if sources.contains(CameraInteractionSources::SMOOTH_SCROLL) {
        TRACKPAD_SOURCE_LABEL
    } else if sources.contains(CameraInteractionSources::PINCH) {
        PINCH_SOURCE_LABEL
    } else if sources.contains(CameraInteractionSources::TOUCH) {
        TOUCH_SOURCE_LABEL
    } else if sources.contains(CameraInteractionSources::MANUAL) {
        MANUAL_INPUT_SOURCE_LABEL
    } else {
        INPUT_BINDING_SOURCE_LABEL
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
        speed: ControlSpeed::Normal,
        zoom_direction: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::InputSensitivity;
    use crate::input::OrbitCamBindingsError;
    use crate::input::OrbitCamBlenderLikePreset;
    use crate::input::OrbitCamButtonDragZoom;
    use crate::input::OrbitCamInputBinding;
    use crate::input::OrbitCamMouseDrag;
    use crate::input::OrbitCamMouseWheelZoom;
    use crate::input::OrbitCamPinchZoom;
    use crate::input::OrbitCamPreset;
    use crate::input::OrbitCamScalePolicy;
    use crate::input::OrbitCamSensitivity;
    use crate::input::OrbitCamSlowMode;
    use crate::input::OrbitCamTouchBinding;
    use crate::input::OrbitCamTrackpadScroll;

    const CUSTOM_SLOW_SCALE: f32 = 0.25;

    #[test]
    fn summary_labels_follow_input_mode_variant() -> Result<(), OrbitCamBindingsError> {
        let preset_summary = describe_orbit_cam_controls(&OrbitCamInputMode::with_preset(
            OrbitCamPreset::blender_like(),
        ));
        assert_eq!(preset_summary.mode_label, "Preset");
        assert_eq!(preset_summary.mode_value, "BlenderLike");

        let tuned_bindings = OrbitCamBlenderLikePreset::default()
            .slow_scale(0.25)
            .build()?;
        let bindings_summary =
            describe_orbit_cam_controls(&OrbitCamInputMode::Bindings(tuned_bindings));
        assert_eq!(bindings_summary.mode_label, "Input");
        assert_eq!(bindings_summary.mode_value, "custom bindings");

        let manual_summary = describe_orbit_cam_controls(&OrbitCamInputMode::Manual);
        assert_eq!(manual_summary.mode_label, "Input");
        assert_eq!(manual_summary.mode_value, "manual input");

        Ok(())
    }

    #[test]
    fn blender_like_summary_reflects_preset_bindings() {
        let summary = describe_orbit_cam_controls(&OrbitCamInputMode::with_preset(
            OrbitCamPreset::blender_like(),
        ));
        let labels = summary_labels(&summary);

        assert_eq!(summary.camera_label, "OrbitCam");
        assert_eq!(summary.mode_label, "Preset");
        assert_eq!(summary.mode_value, "BlenderLike");
        assert!(labels.contains(&"mmb drag"));
        assert!(labels.contains(&"smooth-scroll"));
        assert!(labels.contains(&"shift+mmb drag"));
        assert!(labels.contains(&"shift+smooth-scroll"));
        // Zoom sources split into one row per direction.
        assert!(labels.contains(&"wheel ↑"));
        assert!(labels.contains(&"wheel ↓"));
        assert!(labels.contains(&"ctrl+scroll ↑"));
        assert!(labels.contains(&"ctrl+scroll ↓"));
        assert!(labels.contains(&"pinch out"));
        assert!(labels.contains(&"pinch in"));
        assert_eq!(row_direction(&summary, "wheel ↑"), Some(ZoomDirection::In));
        assert_eq!(row_direction(&summary, "wheel ↓"), Some(ZoomDirection::Out));
        assert_eq!(
            row_direction(&summary, "ctrl+scroll ↑"),
            Some(ZoomDirection::In)
        );
        assert_eq!(
            row_direction(&summary, "ctrl+scroll ↓"),
            Some(ZoomDirection::Out)
        );
        assert_eq!(
            row_direction(&summary, "pinch out"),
            Some(ZoomDirection::In)
        );
        assert_eq!(
            row_direction(&summary, "pinch in"),
            Some(ZoomDirection::Out)
        );
        // Orbit and pan rows stay non-directional.
        assert_eq!(row_direction(&summary, "mmb drag"), None);
        assert_eq!(row_direction(&summary, "smooth-scroll"), None);
    }

    #[test]
    fn custom_bindings_override_preset_summary() -> Result<(), OrbitCamBindingsError> {
        let bindings = OrbitCamBindings::builder()
            .orbit(OrbitCamMouseDrag::new(MouseButton::Right))
            .build()?;
        let summary = describe_orbit_cam_controls(&OrbitCamInputMode::Bindings(bindings));
        let labels = summary_labels(&summary);

        assert_eq!(summary.mode_label, "Input");
        assert_eq!(summary.mode_value, "custom bindings");
        assert!(labels.contains(&"rmb drag"));
        assert!(!labels.contains(&"mmb drag"));

        Ok(())
    }

    #[test]
    fn disabled_native_bindings_are_omitted_from_summary() -> Result<(), OrbitCamBindingsError> {
        let bindings = OrbitCamBindings::builder()
            .orbit(
                OrbitCamMouseDrag::new(MouseButton::Right)
                    .with_sensitivity(InputSensitivity::DISABLED.0),
            )
            .build()?;
        let summary = describe_orbit_cam_controls(&OrbitCamInputMode::Bindings(bindings));

        assert!(summary.rows.is_empty());

        Ok(())
    }

    #[test]
    fn disabled_adapter_bindings_are_omitted_from_summary() -> Result<(), OrbitCamBindingsError> {
        let disabled = InputSensitivity::DISABLED.0;
        let bindings = OrbitCamBindings::builder()
            .orbit(OrbitCamTrackpadScroll::default().with_sensitivity(disabled))
            .pan(OrbitCamTrackpadScroll::default().with_sensitivity(disabled))
            .zoom(OrbitCamTrackpadScroll::default().with_sensitivity(disabled))
            .zoom(OrbitCamMouseWheelZoom.with_sensitivity(disabled))
            .zoom(OrbitCamPinchZoom.with_sensitivity(disabled))
            .zoom(OrbitCamButtonDragZoom::new(MouseButton::Middle).with_sensitivity(disabled))
            .touch_config(Some(
                OrbitCamTouchBinding::OneFingerOrbit
                    .with_sensitivity(OrbitCamSensitivity::uniform(disabled)),
            ))
            .build()?;
        let summary = describe_orbit_cam_controls(&OrbitCamInputMode::Bindings(bindings));

        assert!(summary.rows.is_empty());

        Ok(())
    }

    #[test]
    fn effective_slow_mode_keeps_tuned_blender_like_when_pinch_remains_enabled()
    -> Result<(), OrbitCamBindingsError> {
        let disabled = InputSensitivity::DISABLED.0;
        let preset = OrbitCamPreset::from(
            OrbitCamBlenderLikePreset::default()
                .mouse_sensitivity(OrbitCamSensitivity::uniform(disabled))
                .smooth_scroll_sensitivity(OrbitCamSensitivity::uniform(disabled)),
        );
        let bindings = preset.to_bindings()?;
        let mode = OrbitCamInputMode::with_preset(preset);
        let summary = describe_orbit_cam_controls(&mode);
        let labels = summary_labels(&summary);

        assert!(effective_slow_mode(&bindings).is_some());
        assert!(labels.contains(&"pinch out"));
        assert!(labels.contains(&"pinch in"));
        assert!(!labels.contains(&"mmb drag"));
        assert!(!labels.contains(&"smooth-scroll"));
        Ok(())
    }

    #[test]
    fn effective_slow_mode_omits_custom_slow_mode_without_enabled_controls()
    -> Result<(), OrbitCamBindingsError> {
        let disabled = InputSensitivity::DISABLED.0;
        let bindings = OrbitCamBindings::builder()
            .slow_mode(OrbitCamSlowMode {
                toggle_key: KeyCode::KeyS,
                mod_keys:   ModKeys::ALT,
                scale:      OrbitCamScalePolicy {
                    normal: InputSensitivity::DEFAULT.0,
                    slow:   CUSTOM_SLOW_SCALE,
                },
            })
            .orbit(OrbitCamMouseDrag::new(MouseButton::Middle).with_sensitivity(disabled))
            .build()?;
        assert!(effective_slow_mode(&bindings).is_none());
        let mode = OrbitCamInputMode::Bindings(bindings);

        assert!(describe_orbit_cam_controls(&mode).rows.is_empty());

        Ok(())
    }

    #[test]
    fn touch_summary_omits_disabled_actions_only() -> Result<(), OrbitCamBindingsError> {
        let bindings = OrbitCamBindings::builder()
            .touch_config(Some(
                OrbitCamTouchBinding::OneFingerOrbit.with_sensitivity(
                    OrbitCamSensitivity::new()
                        .orbit(InputSensitivity::DISABLED.0)
                        .zoom(InputSensitivity::DISABLED.0),
                ),
            ))
            .build()?;
        let summary = describe_orbit_cam_controls(&OrbitCamInputMode::Bindings(bindings));
        let labels = summary_labels(&summary);

        assert_eq!(summary.rows.len(), 1);
        assert_eq!(summary.rows[0].kind, OrbitCamInteractionKind::Pan);
        assert!(labels.contains(&TWO_FINGER_TOUCH_ROW_LABEL));

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
        let summary = describe_orbit_cam_controls(&OrbitCamInputMode::Bindings(bindings));
        let labels = summary_labels(&summary);

        assert!(labels.contains(&"arrow keys"));
        assert!(labels.contains(&"wasd"));
        // The bidirectional zoom key pair splits into one row per direction.
        assert!(labels.contains(&"+"));
        assert!(labels.contains(&"-"));
        assert!(!labels.contains(&"+ / -"));
        assert_eq!(row_direction(&summary, "+"), Some(ZoomDirection::In));
        assert_eq!(row_direction(&summary, "-"), Some(ZoomDirection::Out));

        Ok(())
    }

    #[test]
    fn gamepad_preset_summary_has_fast_and_slow_rows() {
        let summary =
            describe_orbit_cam_controls(&OrbitCamInputMode::with_preset(OrbitCamPreset::gamepad()));
        let labels = summary_labels(&summary);

        assert_eq!(summary.mode_value, "Gamepad");
        assert!(labels.contains(&"rs"));
        assert!(labels.contains(&"rb+rs"));
        assert!(labels.contains(&"ls"));
        assert!(labels.contains(&"lb+ls"));
        assert!(labels.contains(&"rt"));
        assert!(labels.contains(&"lt"));
        assert!(labels.contains(&"rb+rt"));
        assert!(labels.contains(&"lb+lt"));

        assert_eq!(row_speed(&summary, "rs"), Some(ControlSpeed::Normal));
        assert_eq!(row_speed(&summary, "rb+rs"), Some(ControlSpeed::Slow));
        assert_eq!(row_speed(&summary, "ls"), Some(ControlSpeed::Normal));
        assert_eq!(row_speed(&summary, "lb+ls"), Some(ControlSpeed::Slow));
        assert_eq!(row_speed(&summary, "rt"), Some(ControlSpeed::Normal));
        assert_eq!(row_speed(&summary, "lt"), Some(ControlSpeed::Normal));
        assert_eq!(row_speed(&summary, "rb+rt"), Some(ControlSpeed::Slow));
        assert_eq!(row_speed(&summary, "lb+lt"), Some(ControlSpeed::Slow));

        // The right triggers zoom in, the left triggers zoom out — the panel
        // highlights only the engaged direction off these tags.
        assert_eq!(row_direction(&summary, "rt"), Some(ZoomDirection::In));
        assert_eq!(row_direction(&summary, "lt"), Some(ZoomDirection::Out));
        assert_eq!(row_direction(&summary, "rb+rt"), Some(ZoomDirection::In));
        assert_eq!(row_direction(&summary, "lb+lt"), Some(ZoomDirection::Out));
        // Orbit and pan stick rows are not direction-specific.
        assert_eq!(row_direction(&summary, "rs"), None);
        assert_eq!(row_direction(&summary, "ls"), None);
    }

    #[test]
    fn manual_summary_wins_over_other_modes() {
        let summary = describe_orbit_cam_controls(&OrbitCamInputMode::Manual);
        let labels = summary_labels(&summary);

        assert_eq!(summary.mode_label, "Input");
        assert_eq!(summary.mode_value, "manual input");
        assert!(labels.contains(&"app-authored input"));
        assert!(summary.rows.iter().all(|row| {
            row.camera_interaction_sources
                .contains(CameraInteractionSources::MANUAL)
        }));
    }

    fn summary_labels(summary: &OrbitCamControlSummary) -> Vec<&str> {
        summary.rows.iter().map(|row| row.label.as_str()).collect()
    }

    fn row_speed(summary: &OrbitCamControlSummary, label: &str) -> Option<ControlSpeed> {
        summary
            .rows
            .iter()
            .find(|row| row.label == label)
            .map(|row| row.speed)
    }

    fn row_direction(summary: &OrbitCamControlSummary, label: &str) -> Option<ZoomDirection> {
        summary
            .rows
            .iter()
            .find(|row| row.label == label)
            .and_then(|row| row.zoom_direction)
    }
}
