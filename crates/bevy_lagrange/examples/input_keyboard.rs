//! Demonstrates keyboard user input through `OrbitCamPreset::Keyboard`.
//!
//! Controls:
//!   Arrows — orbit
//!   WASD   — pan
//!   +/-    — zoom

use bevy::prelude::*;
use bevy_diegetic::WorldText;
use bevy_lagrange::CameraInputRoutingConfig;
use bevy_lagrange::NoPositionFallback;
use bevy_lagrange::OrbitCam;
use bevy_lagrange::OrbitCamInputMode;
use bevy_lagrange::OrbitCamPreset;
use fairy_dust::Anchor;
use fairy_dust::CameraHomeTarget;
use fairy_dust::DescriptionPanel;
use fairy_dust::Face;
use fairy_dust::FairyDustOrbitCam;
use fairy_dust::LABEL_SIZE;
use fairy_dust::TitleBar;
use fairy_dust::cube_face_text;

fn main() {
    fairy_dust::sprinkle_example()
        .insert_resource(
            CameraInputRoutingConfig::cursor_hit_test()
                .with_no_position_fallback(NoPositionFallback::OnlyEligibleCamera),
        )
        .with_brp_extras()
        .with_save_window_position()
        .with_studio_lighting()
        .with_ground_plane()
        .size(GROUND_SIZE)
        .with_cube()
        .size(CUBE_SIZE)
        .color(CUBE_COLOR)
        .transform(Transform::from_translation(CUBE_TRANSLATION))
        .insert((CameraHomeTarget, KeyboardInputCube))
        .with_camera_home()
        .yaw(CAMERA_YAW)
        .pitch(CAMERA_PITCH)
        .margin(HOME_MARGIN)
        .with_title_bar(
            TitleBar::new()
                .with_title("Keyboard Bindings")
                .with_anchor(Anchor::TopLeft),
        )
        .with_description_panel(description_panel())
        .with_camera_control_panel()
        .add_systems(Startup, spawn_camera)
        .add_systems(PostStartup, spawn_face_labels)
        .insert_resource(FaceLabelHold::default())
        .add_systems(Update, update_face_labels)
        .run();
}

// ═════════════════════════════════════════════════════════════════════════════
// KEYBOARD PRESET — OrbitCamInputMode::Preset(OrbitCamPreset::Keyboard).
// ═════════════════════════════════════════════════════════════════════════════

const CAMERA_FOCUS: Vec3 = CUBE_TRANSLATION;
const CAMERA_ORBIT_SENSITIVITY: f32 = 4.0;
const CAMERA_PAN_SENSITIVITY: f32 = 6.0;
const CAMERA_PITCH: f32 = 0.45;
const CAMERA_RADIUS: f32 = 6.0;
const CAMERA_YAW: f32 = 0.55;
const CAMERA_ZOOM_SENSITIVITY: f32 = 0.08;
const HOME_MARGIN: f32 = 0.5;

fn spawn_camera(mut commands: Commands) {
    commands.spawn((
        OrbitCam {
            focus: CAMERA_FOCUS,
            yaw: Some(CAMERA_YAW),
            pitch: Some(CAMERA_PITCH),
            radius: Some(CAMERA_RADIUS),
            orbit_sensitivity: CAMERA_ORBIT_SENSITIVITY,
            pan_sensitivity: CAMERA_PAN_SENSITIVITY,
            zoom_sensitivity: CAMERA_ZOOM_SENSITIVITY,
            ..default()
        },
        OrbitCamInputMode::Preset(OrbitCamPreset::Keyboard),
        FairyDustOrbitCam,
    ));
}

// ═════════════════════════════════════════════════════════════════════════════
// CUBE FACE LABELS — live WorldText labels showing which bound keys are down.
// ═════════════════════════════════════════════════════════════════════════════

const FACE_LABEL_COLOR: Color = Color::srgb(0.1, 0.35, 1.0);
const FACE_LABEL_RELEASE_DELAY_SECS: f32 = 0.5;
const FACE_LABEL_SIZE: f32 = 0.095;

const KEYBOARD_ZOOM_LABEL: &str = "+ / -";
const ORBIT_LABEL: &str = "Arrows";
const PAN_LABEL: &str = "WASD";

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
    orbit: HeldFaceLabel,
    pan:   HeldFaceLabel,
    zoom:  HeldFaceLabel,
}

#[derive(Default)]
struct HeldFaceLabel {
    remaining_secs: f32,
    pressed:        Option<String>,
}

#[derive(Component, Clone, Copy)]
enum KeyboardFaceLabel {
    Orbit,
    Pan,
    Zoom,
}

#[derive(Component)]
struct KeyboardInputCube;

fn spawn_face_labels(mut commands: Commands, cubes: Query<Entity, With<KeyboardInputCube>>) {
    let Ok(cube) = cubes.single() else {
        return;
    };

    commands.entity(cube).with_children(|parent| {
        for face in [Face::Front, Face::Back] {
            parent.spawn((
                cube_face_text(
                    face,
                    orbit_face_label(&ButtonInput::default()),
                    CUBE_SIZE,
                    FACE_LABEL_SIZE,
                    FACE_LABEL_COLOR,
                ),
                KeyboardFaceLabel::Orbit,
            ));
        }
        for face in [Face::Left, Face::Right] {
            parent.spawn((
                cube_face_text(
                    face,
                    pan_face_label(&ButtonInput::default()),
                    CUBE_SIZE,
                    FACE_LABEL_SIZE,
                    FACE_LABEL_COLOR,
                ),
                KeyboardFaceLabel::Pan,
            ));
        }
        for face in [Face::Top, Face::Bottom] {
            parent.spawn((
                cube_face_text(
                    face,
                    zoom_face_label(&ButtonInput::default()),
                    CUBE_SIZE,
                    FACE_LABEL_SIZE,
                    FACE_LABEL_COLOR,
                ),
                KeyboardFaceLabel::Zoom,
            ));
        }
    });
}

fn update_face_labels(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    mut hold: ResMut<FaceLabelHold>,
    mut labels: Query<(&KeyboardFaceLabel, &mut WorldText)>,
) {
    let orbit = held_label(
        &mut hold.orbit,
        time.delta_secs(),
        pressed_key_label(&keys, &ORBIT_FACE_KEYS),
        ORBIT_LABEL,
        "Orbit",
    );
    let pan = held_label(
        &mut hold.pan,
        time.delta_secs(),
        pressed_key_label(&keys, &PAN_FACE_KEYS),
        PAN_LABEL,
        "Pan",
    );
    let zoom = held_label(
        &mut hold.zoom,
        time.delta_secs(),
        pressed_key_label(&keys, &ZOOM_FACE_KEYS),
        KEYBOARD_ZOOM_LABEL,
        "Zoom",
    );

    for (kind, mut label) in &mut labels {
        let next = match kind {
            KeyboardFaceLabel::Orbit => orbit.as_str(),
            KeyboardFaceLabel::Pan => pan.as_str(),
            KeyboardFaceLabel::Zoom => zoom.as_str(),
        };
        if label.text() != next {
            label.set_text(next);
        }
    }
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
    hold: &mut HeldFaceLabel,
    delta_secs: f32,
    pressed: Option<String>,
    idle: &str,
    action: &str,
) -> String {
    if let Some(pressed) = pressed {
        hold.remaining_secs = FACE_LABEL_RELEASE_DELAY_SECS;
        hold.pressed = Some(pressed);
        return action_face_label(action, idle, hold.pressed.as_deref());
    }

    hold.remaining_secs = (hold.remaining_secs - delta_secs).max(0.0);
    if hold.remaining_secs > 0.0 {
        return action_face_label(action, idle, hold.pressed.as_deref());
    }

    hold.pressed = None;
    action_face_label(action, idle, None)
}

fn action_face_label(action: &str, idle: &str, pressed: Option<&str>) -> String {
    if let Some(pressed) = pressed {
        format!("{action}: {pressed}")
    } else {
        format!("{action}: {idle}")
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// DESCRIPTION PANEL — on-screen explainer for the keyboard-bindings flow.
// ═════════════════════════════════════════════════════════════════════════════

const DESCRIPTION_TITLE: &str = "Keyboard Bindings";
const DESCRIPTION_LINES: [&str; 4] = [
    "This example uses OrbitCamInputMode::Bindings with keyboard-only controls.",
    "Use it when the camera should have a keymap and Lagrange should route input.",
    "Unlike Manual mode, your app does not write per-frame input to camera intent (orbit, pan, zoom).",
    "Mapped keys can be held at the same time and will apply in the same frame.",
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

const CUBE_COLOR: Color = Color::srgb(0.8, 0.7, 0.6);
const CUBE_GROUND_CLEARANCE: f32 = 0.1;
const CUBE_SIZE: f32 = 1.0;
const CUBE_TRANSLATION: Vec3 = Vec3::new(0.0, CUBE_SIZE * 0.5 + CUBE_GROUND_CLEARANCE, 0.0);

const GROUND_SIZE: f32 = 5.0;
