//! Spawns an `OrbitCam` with `OrbitCamInputMode::Manual` and writes orbit /
//! pan / zoom intent ourselves through `OrbitCamManualInputWriter`. Pick this
//! mode when the app — not a preset, not a binding list — decides what
//! counts as camera input: `write_manual_input` reads the keyboard directly
//! every `PreUpdate` and hands Lagrange the resulting pixel deltas and zoom
//! amount. Press orbit, pan, and zoom keys together to see multiple manual
//! inputs merge in the same frame.
//!
//! Controls:
//!   Arrows — orbit
//!   WASD   — pan
//!   +/-    — zoom

use bevy::prelude::*;
use bevy_diegetic::DiegeticTextMut;
use bevy_lagrange::CameraInteractionSources;
use bevy_lagrange::ManualInputSource;
use bevy_lagrange::OrbitCam;
use bevy_lagrange::OrbitCamInputMode;
use bevy_lagrange::OrbitCamInputPhase;
use bevy_lagrange::OrbitCamInteractionKind;
use bevy_lagrange::OrbitCamManualInputWriter;
use fairy_dust::Anchor;
use fairy_dust::CameraGuidance;
use fairy_dust::CameraGuidanceRow;
use fairy_dust::CameraHomeTarget;
use fairy_dust::DescriptionPanel;
use fairy_dust::Face;
use fairy_dust::FairyDustOrbitCam;
use fairy_dust::HoldState;
use fairy_dust::LABEL_SIZE;
use fairy_dust::ReleaseHold;
use fairy_dust::TitleBar;
use fairy_dust::apply_example_orbit_cam_limits;
use fairy_dust::cube_face_label;

fn main() {
    fairy_dust::sprinkle_example()
        .with_brp_extras()
        .with_save_window_position()
        .with_studio_lighting()
        .with_ground_plane()
        .with_cube()
        .size(CUBE_SIZE)
        .color(CUBE_COLOR)
        .transform(Transform::from_translation(CUBE_TRANSLATION))
        .insert((CameraHomeTarget, ManualInputCube))
        .with_camera_home()
        .yaw(CAMERA_YAW)
        .pitch(CAMERA_PITCH)
        .margin(HOME_MARGIN)
        .with_title_bar(
            TitleBar::new()
                .with_title("Manual Input")
                .with_anchor(Anchor::TopLeft),
        )
        .with_cube_spin::<ManualInputCube>()
        .with_description_panel(description_panel())
        .with_camera_control_panel()
        .add_systems(Startup, spawn_camera)
        .add_systems(PostStartup, spawn_face_labels)
        .insert_resource(FaceLabelHold::default())
        // `OrbitCamInputPhase::WriteManual` is the only schedule slot in which
        // `OrbitCamManualInputWriter` accepts writes.
        .add_systems(
            PreUpdate,
            write_manual_input.in_set(OrbitCamInputPhase::WriteManual),
        )
        .add_systems(Update, update_face_labels)
        .run();
}

// ═════════════════════════════════════════════════════════════════════════════
// MANUAL INPUT — OrbitCamInputMode::Manual + OrbitCamManualInputWriter.
//
// How it works:
//   1. `spawn_camera` (Startup) spawns the OrbitCam with `OrbitCamInputMode::Manual`, a
//      `ManualCamera` marker used by the writer query, custom `CameraGuidance` rows so the control
//      panel advertises the keys this example actually reads, and the example shell marker.
//   2. `write_manual_input` (PreUpdate, in `OrbitCamInputPhase::WriteManual`) reads the keyboard
//      directly, clears last frame's manual intent, gathers orbit/pan pixel deltas and a zoom
//      amount, and pushes them into `OrbitCamManualInputWriter`. Arrows orbit, WASD pans, and +/-
//      zooms can all be held together; the writer merges those simultaneous inputs into the same
//      frame's camera intent.
// ═════════════════════════════════════════════════════════════════════════════

// Camera spawn.
const CAMERA_FOCUS: Vec3 = CUBE_TRANSLATION;
const CAMERA_PITCH: f32 = 0.45;
const CAMERA_RADIUS: f32 = 6.0;
const CAMERA_YAW: f32 = 0.55;
const HOME_MARGIN: f32 = 0.5;

// Control-panel guidance labels.
const KEYBOARD_ZOOM_LABEL: &str = "+ / -";
const ORBIT_LABEL: &str = "Arrows";
const PAN_LABEL: &str = "WASD";

// Per-frame write magnitudes.
const ORBIT_PIXELS: f32 = 4.0;
const PAN_PIXELS: f32 = 6.0;
const ZOOM_AMOUNT: f32 = 0.08;

#[derive(Component)]
struct ManualCamera;

fn spawn_camera(mut commands: Commands) {
    let mut camera = OrbitCam {
        focus: CAMERA_FOCUS,
        yaw: Some(CAMERA_YAW),
        pitch: Some(CAMERA_PITCH),
        radius: Some(CAMERA_RADIUS),
        ..default()
    };
    apply_example_orbit_cam_limits(&mut camera);
    commands.spawn((
        camera,
        OrbitCamInputMode::Manual,
        ManualCamera,
        manual_guidance(),
        FairyDustOrbitCam,
    ));
}

fn write_manual_input(
    keys: Res<ButtonInput<KeyCode>>,
    cameras: Query<Entity, With<ManualCamera>>,
    mut writer: OrbitCamManualInputWriter,
) {
    for camera in &cameras {
        let Ok(mut input) = writer.get_mut(camera, ManualInputSource::observed_keyboard()) else {
            continue;
        };
        input.clear();

        let orbit = signed_vec2(
            &keys,
            (KeyCode::ArrowRight, KeyCode::ArrowLeft),
            (KeyCode::ArrowUp, KeyCode::ArrowDown),
            ORBIT_PIXELS,
        );

        let pan = signed_vec2(
            &keys,
            (KeyCode::KeyD, KeyCode::KeyA),
            (KeyCode::KeyW, KeyCode::KeyS),
            PAN_PIXELS,
        );

        let zoom = signed_axis(&keys, KeyCode::Equal, KeyCode::Minus, ZOOM_AMOUNT);

        if orbit.is_none() && pan.is_none() && zoom.is_none() {
            continue;
        }

        let Ok(mut input) = writer.get_mut(camera, ManualInputSource::observed_keyboard()) else {
            continue;
        };
        if let Some(orbit) = orbit {
            input.orbit_pixels(orbit);
        }
        if let Some(pan) = pan {
            input.pan_pixels(pan);
        }
        if let Some(zoom) = zoom {
            input.zoom_smooth_amount(zoom);
        }
    }
}

fn manual_guidance() -> CameraGuidance {
    CameraGuidance::custom([
        CameraGuidanceRow::new(OrbitCamInteractionKind::Orbit, ORBIT_LABEL)
            .with_camera_interaction_sources(CameraInteractionSources::KEYBOARD),
        CameraGuidanceRow::new(OrbitCamInteractionKind::Pan, PAN_LABEL)
            .with_camera_interaction_sources(CameraInteractionSources::KEYBOARD),
        CameraGuidanceRow::new(OrbitCamInteractionKind::Zoom, KEYBOARD_ZOOM_LABEL)
            .with_camera_interaction_sources(CameraInteractionSources::KEYBOARD),
    ])
}

fn signed_axis(
    keys: &ButtonInput<KeyCode>,
    positive: KeyCode,
    negative: KeyCode,
    amount: f32,
) -> Option<f32> {
    match (keys.pressed(positive), keys.pressed(negative)) {
        (true, false) => Some(amount),
        (false, true) => Some(-amount),
        (true, true) => Some(0.0),
        (false, false) => None,
    }
}

fn signed_vec2(
    keys: &ButtonInput<KeyCode>,
    x_axis: (KeyCode, KeyCode),
    y_axis: (KeyCode, KeyCode),
    amount: f32,
) -> Option<Vec2> {
    let x = signed_axis(keys, x_axis.0, x_axis.1, amount);
    let y = signed_axis(keys, y_axis.0, y_axis.1, amount);
    match (x, y) {
        (None, None) => None,
        _ => Some(Vec2::new(x.unwrap_or(0.0), y.unwrap_or(0.0))),
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// CUBE FACE LABELS — live world-space DiegeticText labels showing which manual keys are down.
// ═════════════════════════════════════════════════════════════════════════════

const ORBIT_FACE_KEYS: [(KeyCode, &str); 4] = [
    (KeyCode::ArrowUp, "Up"),
    (KeyCode::ArrowDown, "Down"),
    (KeyCode::ArrowLeft, "Left"),
    (KeyCode::ArrowRight, "Right"),
];
const PAN_FACE_KEYS: [(KeyCode, &str); 4] = [
    (KeyCode::KeyW, "W"),
    (KeyCode::KeyA, "A"),
    (KeyCode::KeyS, "S"),
    (KeyCode::KeyD, "D"),
];
const ZOOM_FACE_KEYS: [(KeyCode, &str); 2] = [(KeyCode::Equal, "+"), (KeyCode::Minus, "-")];

#[derive(Resource, Default)]
struct FaceLabelHold {
    orbit: ReleaseHold<String>,
    pan:   ReleaseHold<String>,
    zoom:  ReleaseHold<String>,
}

#[derive(Component, Clone, Copy)]
enum ManualFaceLabel {
    Orbit,
    Pan,
    Zoom,
}

#[derive(Component)]
struct ManualInputCube;

fn spawn_face_labels(mut commands: Commands, cubes: Query<Entity, With<ManualInputCube>>) {
    let Ok(cube) = cubes.single() else {
        return;
    };

    commands.entity(cube).with_children(|parent| {
        for face in [Face::Front, Face::Back] {
            parent.spawn((
                cube_face_label(face, orbit_face_label(&ButtonInput::default()), CUBE_SIZE),
                ManualFaceLabel::Orbit,
            ));
        }
        for face in [Face::Left, Face::Right] {
            parent.spawn((
                cube_face_label(face, pan_face_label(&ButtonInput::default()), CUBE_SIZE),
                ManualFaceLabel::Pan,
            ));
        }
        for face in [Face::Top, Face::Bottom] {
            parent.spawn((
                cube_face_label(face, zoom_face_label(&ButtonInput::default()), CUBE_SIZE),
                ManualFaceLabel::Zoom,
            ));
        }
    });
}

fn update_face_labels(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    mut hold: ResMut<FaceLabelHold>,
    mut labels: DiegeticTextMut<ManualFaceLabel>,
) {
    let orbit = held_label(
        &mut hold.orbit,
        time.delta(),
        pressed_key_label(&keys, &ORBIT_FACE_KEYS),
        ORBIT_LABEL,
        "Orbit",
    );
    let pan = held_label(
        &mut hold.pan,
        time.delta(),
        pressed_key_label(&keys, &PAN_FACE_KEYS),
        PAN_LABEL,
        "Pan",
    );
    let zoom = held_label(
        &mut hold.zoom,
        time.delta(),
        pressed_key_label(&keys, &ZOOM_FACE_KEYS),
        KEYBOARD_ZOOM_LABEL,
        "Zoom",
    );

    labels.for_each_mut(|kind, label| {
        let next = match kind {
            ManualFaceLabel::Orbit => orbit.as_str(),
            ManualFaceLabel::Pan => pan.as_str(),
            ManualFaceLabel::Zoom => zoom.as_str(),
        };
        if label.text() != next {
            label.set_text(next);
        }
    });
}

fn orbit_face_label(keys: &ButtonInput<KeyCode>) -> String {
    let pressed = pressed_key_label(keys, &ORBIT_FACE_KEYS);
    action_face_label("Orbit", ORBIT_LABEL, pressed.as_deref())
}

fn pan_face_label(keys: &ButtonInput<KeyCode>) -> String {
    let pressed = pressed_key_label(keys, &PAN_FACE_KEYS);
    action_face_label("Pan", PAN_LABEL, pressed.as_deref())
}

fn zoom_face_label(keys: &ButtonInput<KeyCode>) -> String {
    let pressed = pressed_key_label(keys, &ZOOM_FACE_KEYS);
    action_face_label("Zoom", KEYBOARD_ZOOM_LABEL, pressed.as_deref())
}

fn pressed_key_label(keys: &ButtonInput<KeyCode>, bindings: &[(KeyCode, &str)]) -> Option<String> {
    let pressed = bindings
        .iter()
        .filter_map(|(key, label)| keys.pressed(*key).then_some(*label))
        .collect::<Vec<_>>();
    (!pressed.is_empty()).then(|| pressed.join("+"))
}

fn held_label(
    hold: &mut ReleaseHold<String>,
    delta: std::time::Duration,
    pressed: Option<String>,
    idle: &str,
    action: &str,
) -> String {
    match hold.update(delta, pressed) {
        HoldState::Active(pressed) | HoldState::Held(pressed) => {
            action_face_label(action, idle, Some(pressed))
        },
        HoldState::Idle => action_face_label(action, idle, None),
    }
}

fn action_face_label(action: &str, idle: &str, pressed: Option<&str>) -> String {
    if let Some(pressed) = pressed {
        format!("{action}: {pressed}")
    } else {
        format!("{action}: {idle}")
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// DESCRIPTION PANEL — on-screen explainer for the manual-input flow.
// ═════════════════════════════════════════════════════════════════════════════

const DESCRIPTION_TITLE: &str = "Manual Input";
const DESCRIPTION_LINES: [&str; 5] = [
    "Manual mode lets your app own input mapping.",
    "Read any controls, then write per-frame camera intent.",
    "Use it for gameplay state, custom keybinds, UI focus rules, or another input layer.",
    "Orbit, pan, and zoom can be written together in the same frame.",
    "Lagrange merges them into one camera update.",
];

fn description_panel() -> DescriptionPanel {
    DescriptionPanel::new(DESCRIPTION_TITLE)
        .with_fit_width()
        .with_body_size(LABEL_SIZE.0)
        .lines(DESCRIPTION_LINES)
}

// ═════════════════════════════════════════════════════════════════════════════
// SCENE SCAFFOLDING — cube body and ground sized to match.
// ═════════════════════════════════════════════════════════════════════════════

const CUBE_GROUND_CLEARANCE: f32 = 0.1;
const CUBE_COLOR: Color = fairy_dust::EXAMPLE_CUBE_COLOR;
const CUBE_SIZE: f32 = fairy_dust::EXAMPLE_CUBE_SIZE;
const CUBE_TRANSLATION: Vec3 = fairy_dust::example_cube_on_ground(CUBE_GROUND_CLEARANCE);
