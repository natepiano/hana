//! Builds an app-owned keyboard binding set with `OrbitCamBindings::builder()`,
//! validates it, and gives it to `OrbitCam` as
//! `OrbitCamInputMode::Bindings(...)`. Pick this mode when you want a fixed
//! keymap rather than a preset and want Lagrange to handle the input routing
//! on your behalf — `keyboard_bindings` is the part to copy when wiring a
//! custom keymap into your own app.
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
use bevy_lagrange::OrbitCamBindings;
use bevy_lagrange::OrbitCamBindingsError;
use bevy_lagrange::OrbitCamInputBinding;
use bevy_lagrange::OrbitCamInputMode;
use fairy_dust::Anchor;
use fairy_dust::CameraHomeTarget;
use fairy_dust::DescriptionPanel;
use fairy_dust::Face;
use fairy_dust::FairyDustOrbitCam;
use fairy_dust::LABEL_SIZE;
use fairy_dust::TitleBar;
use fairy_dust::cube_face_text;

fn main() {
    // Build and validate the binding set up front; if validation fails the
    // example bails before any plugins spin up.
    let Ok(bindings) = keyboard_bindings() else {
        error!("keyboard camera bindings failed to validate");
        return;
    };

    fairy_dust::sprinkle_example()
        .insert_resource(
            CameraInputRoutingConfig::cursor_hit_test()
                .with_no_position_fallback(NoPositionFallback::OnlyEligibleCamera),
        )
        .insert_resource(KeyboardBindings(bindings))
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
// KEYBOARD BINDINGS — OrbitCamBindings::builder() + OrbitCamInputMode::Bindings.
//
// How it works:
//   1. `keyboard_bindings` constructs cardinal-key orbit and pan bindings, a bidirectional zoom
//      binding, and runs them through `OrbitCamBindings::builder().build()`, which validates the
//      set and returns `Result<OrbitCamBindings, OrbitCamBindingsError>`.
//   2. `main()` stashes the validated bindings in a `KeyboardBindings` resource before the app
//      starts.
//   3. `spawn_camera` (Startup) clones the resource's bindings into
//      `OrbitCamInputMode::Bindings(...)` on the OrbitCam entity; Lagrange's routing then
//      translates key state into orbit / pan / zoom intent.
// ═════════════════════════════════════════════════════════════════════════════

const CAMERA_FOCUS: Vec3 = CUBE_TRANSLATION;
const CAMERA_ORBIT_SENSITIVITY: f32 = 4.0;
const CAMERA_PAN_SENSITIVITY: f32 = 6.0;
const CAMERA_PITCH: f32 = 0.45;
const CAMERA_RADIUS: f32 = 6.0;
const CAMERA_YAW: f32 = 0.55;
const CAMERA_ZOOM_SENSITIVITY: f32 = 0.08;
const HOME_MARGIN: f32 = 0.5;

#[derive(Resource)]
struct KeyboardBindings(OrbitCamBindings);

fn spawn_camera(mut commands: Commands, bindings: Res<KeyboardBindings>) {
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
        OrbitCamInputMode::Bindings(bindings.0.clone()),
        FairyDustOrbitCam,
    ));
}

fn keyboard_bindings() -> Result<OrbitCamBindings, OrbitCamBindingsError> {
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

    OrbitCamBindings::builder()
        .orbit(orbit_keys)
        .pan(pan_keys)
        .zoom(zoom_keys)
        .build()
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
