//! Builds an app-owned `OrbitCamBindings` set that mixes mouse, trackpad,
//! wheel, and pinch inputs, then attaches it through
//! `OrbitCamInputMode::Bindings(...)`. Press `T` to toggle
//! `CameraInputDisabled` and see how an app can disable camera controls at
//! runtime without rebuilding the binding set.
//!
//! Controls:
//!   Orbit — middle mouse drag, trackpad scroll
//!   Pan   — right mouse drag, Shift + trackpad scroll
//!   Zoom  — wheel, Ctrl + trackpad scroll, pinch
//!   T     — toggle camera input

use std::f32::consts::TAU;

use bevy::prelude::*;
use bevy_diegetic::DiegeticPanelCommands;
use bevy_enhanced_input::prelude::ModKeys;
use bevy_lagrange::CameraInputDisabled;
use bevy_lagrange::CameraInputRoutingConfig;
use bevy_lagrange::CameraInteractionSources;
use bevy_lagrange::NoPositionFallback;
use bevy_lagrange::OrbitCam;
use bevy_lagrange::OrbitCamBindings;
use bevy_lagrange::OrbitCamBindingsError;
use bevy_lagrange::OrbitCamControlSummary;
use bevy_lagrange::OrbitCamInputMode;
use bevy_lagrange::OrbitCamInteractionKind;
use bevy_lagrange::OrbitCamInteractionStarted;
use bevy_lagrange::OrbitCamInteractionState;
use bevy_lagrange::OrbitCamMouseDrag;
use bevy_lagrange::OrbitCamMouseWheelZoom;
use bevy_lagrange::OrbitCamPinchZoom;
use bevy_lagrange::OrbitCamTrackpadScroll;
use bevy_lagrange::ZoomInversion;
use bevy_lagrange::describe_orbit_cam_controls;
use fairy_dust::Anchor;
use fairy_dust::CameraHomeTarget;
use fairy_dust::ControlActivation;
use fairy_dust::CubeFacePanelContent;
use fairy_dust::CubeFacePanelStyle;
use fairy_dust::DescriptionPanel;
use fairy_dust::Face;
use fairy_dust::FairyDustOrbitCam;
use fairy_dust::HoldState;
use fairy_dust::LABEL_SIZE;
use fairy_dust::ReleaseHold;
use fairy_dust::TitleBar;
use fairy_dust::TitleChipActivation;
use fairy_dust::apply_example_orbit_cam_limits;
use fairy_dust::cube_face_panel;
use fairy_dust::cube_face_panel_tree;
use fairy_dust::cube_face_transform;

fn main() {
    let Ok(bindings) = custom_bindings() else {
        error!("custom camera bindings failed to validate");
        return;
    };

    fairy_dust::sprinkle_example()
        .insert_resource(
            CameraInputRoutingConfig::cursor_hit_test()
                .with_no_position_fallback(NoPositionFallback::OnlyEligibleCamera),
        )
        .insert_resource(CustomBindings(bindings))
        .init_resource::<InputDisabledState>()
        .with_brp_extras()
        .with_save_window_position()
        .with_studio_lighting()
        .with_ground_plane()
        .size(GROUND_SIZE)
        .with_cube()
        .size(CUBE_SIZE)
        .color(CUBE_COLOR)
        .transform(Transform::from_translation(CUBE_TRANSLATION))
        .insert((CameraHomeTarget, CustomInputCube))
        .with_camera_home()
        .yaw(CAMERA_YAW)
        .pitch(CAMERA_PITCH)
        .margin(HOME_MARGIN)
        .with_title_bar(
            TitleBar::new()
                .with_title("Custom Bindings")
                .with_anchor(Anchor::TopLeft)
                .control(INPUT_DISABLED_CONTROL),
        )
        .wire_chip_to_activation::<InputDisabledState>(INPUT_DISABLED_CONTROL)
        .with_cube_spin::<CustomInputCube>()
        .with_description_panel(description_panel())
        .with_camera_control_panel()
        .add_systems(Startup, spawn_camera)
        .add_systems(PostStartup, spawn_face_labels)
        .insert_resource(FaceLabelHold::default())
        .add_systems(Update, (toggle_camera_controls, update_face_labels))
        .add_observer(capture_zoom_started)
        .run();
}

// ═════════════════════════════════════════════════════════════════════════════
// CUSTOM BINDINGS — OrbitCamBindings::builder() + CameraInputDisabled toggling.
//
// How it works:
//   1. `custom_bindings` builds one validated `OrbitCamBindings` value with mouse-drag,
//      trackpad-scroll, wheel, and pinch entries.
//   2. `spawn_camera` clones that binding set into `OrbitCamInputMode::Bindings(...)` on the
//      `OrbitCam`, while also showing camera-level sensitivity, limit, and upside-down settings.
//   3. `toggle_camera_controls` adds or removes `CameraInputDisabled`; Lagrange keeps the binding
//      set installed, but ignores camera input while the component is present.
// ═════════════════════════════════════════════════════════════════════════════

const CAMERA_FOCUS: Vec3 = CUBE_TRANSLATION;
const CAMERA_ORBIT_SENSITIVITY: f32 = 1.5;
const CAMERA_PAN_SENSITIVITY: f32 = 0.5;
const CAMERA_PITCH: f32 = TAU / 8.0;
const CAMERA_RADIUS: f32 = 5.0;
const CAMERA_YAW: f32 = TAU / 8.0;
const CAMERA_ZOOM_SENSITIVITY: f32 = 0.5;
const HOME_MARGIN: f32 = 0.5;
const INPUT_DISABLED_CONTROL: &str = "T Disabled";

#[derive(Resource)]
struct CustomBindings(OrbitCamBindings);

#[derive(Resource, Default)]
struct InputDisabledState {
    disabled: bool,
}

impl TitleChipActivation for InputDisabledState {
    fn activation(&self) -> ControlActivation { activation_for(self.disabled) }
}

#[derive(Component)]
struct CustomCamera;

fn spawn_camera(mut commands: Commands, bindings: Res<CustomBindings>) {
    let mut camera = OrbitCam {
        focus: CAMERA_FOCUS,
        yaw: Some(CAMERA_YAW),
        pitch: Some(CAMERA_PITCH),
        radius: Some(CAMERA_RADIUS),
        orbit_sensitivity: CAMERA_ORBIT_SENSITIVITY,
        pan_sensitivity: CAMERA_PAN_SENSITIVITY,
        zoom_sensitivity: CAMERA_ZOOM_SENSITIVITY,
        ..default()
    };
    apply_example_orbit_cam_limits(&mut camera);
    commands.spawn((
        camera,
        OrbitCamInputMode::Bindings(bindings.0.clone()),
        CustomCamera,
        FairyDustOrbitCam,
    ));
}

fn toggle_camera_controls(
    key_input: Res<ButtonInput<KeyCode>>,
    mut commands: Commands,
    mut disabled_state: ResMut<InputDisabledState>,
    cameras: Query<(Entity, Option<&CameraInputDisabled>), With<CustomCamera>>,
) {
    if !key_input.just_pressed(KeyCode::KeyT) {
        return;
    }

    for (camera, disabled) in &cameras {
        if disabled.is_some() {
            commands.entity(camera).remove::<CameraInputDisabled>();
            disabled_state.disabled = false;
        } else {
            commands.entity(camera).insert(CameraInputDisabled);
            disabled_state.disabled = true;
        }
    }
}

fn custom_bindings() -> Result<OrbitCamBindings, OrbitCamBindingsError> {
    OrbitCamBindings::builder()
        .orbit(OrbitCamMouseDrag::new(MouseButton::Middle))
        .orbit(OrbitCamTrackpadScroll::default())
        .pan(OrbitCamMouseDrag::new(MouseButton::Right))
        .pan(OrbitCamTrackpadScroll::default().with_mod_keys(ModKeys::SHIFT))
        .zoom(OrbitCamMouseWheelZoom)
        .zoom(OrbitCamTrackpadScroll::default().with_mod_keys(ModKeys::CONTROL))
        .zoom(OrbitCamPinchZoom)
        .zoom_inversion(ZoomInversion::Inverted)
        .build()
}

const fn activation_for(active: bool) -> ControlActivation {
    if active {
        ControlActivation::Active
    } else {
        ControlActivation::Inactive
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// CUBE FACE PANELS — live face panels showing active custom input sources.
// ═════════════════════════════════════════════════════════════════════════════

const FACE_PANEL_STYLE: CubeFacePanelStyle = CubeFacePanelStyle::for_cube(CUBE_SIZE);

#[derive(Component)]
struct CustomInputCube;

#[derive(Component, Clone, Copy)]
enum CustomFaceLabel {
    Orbit,
    Pan,
    Zoom,
}

impl CustomFaceLabel {
    const fn kind(self) -> OrbitCamInteractionKind {
        match self {
            Self::Orbit => OrbitCamInteractionKind::Orbit,
            Self::Pan => OrbitCamInteractionKind::Pan,
            Self::Zoom => OrbitCamInteractionKind::Zoom,
        }
    }

    const fn title(self) -> &'static str {
        match self {
            Self::Orbit => "Orbit",
            Self::Pan => "Pan",
            Self::Zoom => "Zoom",
        }
    }
}

/// Holds the bindings' described controls so idle/active face labels share the
/// camera control panel's vocabulary.
#[derive(Resource)]
struct FaceGuidance(OrbitCamControlSummary);

#[derive(Resource, Default)]
struct FaceLabelHold {
    orbit: ReleaseHold<Vec<String>>,
    pan:   ReleaseHold<Vec<String>>,
    zoom:  ReleaseHold<Vec<String>>,
}

impl FaceLabelHold {
    fn clear(&mut self) {
        self.orbit.clear();
        self.pan.clear();
        self.zoom.clear();
    }
}

fn spawn_face_labels(
    mut commands: Commands,
    bindings: Res<CustomBindings>,
    cubes: Query<Entity, With<CustomInputCube>>,
) {
    let Ok(cube) = cubes.single() else {
        return;
    };

    let summary = describe_orbit_cam_controls(&OrbitCamInputMode::Bindings(bindings.0.clone()));
    commands.entity(cube).with_children(|parent| {
        for face in [Face::Front, Face::Back] {
            spawn_face_panel(parent, face, CustomFaceLabel::Orbit, &summary);
        }
        for face in [Face::Left, Face::Right] {
            spawn_face_panel(parent, face, CustomFaceLabel::Pan, &summary);
        }
        for face in [Face::Top, Face::Bottom] {
            spawn_face_panel(parent, face, CustomFaceLabel::Zoom, &summary);
        }
    });
    commands.insert_resource(FaceGuidance(summary));
}

fn update_face_labels(
    mut commands: Commands,
    time: Res<Time>,
    mut hold: ResMut<FaceLabelHold>,
    guidance: Res<FaceGuidance>,
    cameras: Query<(&OrbitCamInteractionState, Option<&CameraInputDisabled>), With<CustomCamera>>,
    labels: Query<(Entity, &CustomFaceLabel)>,
) {
    let Ok((interaction_state, disabled)) = cameras.single() else {
        return;
    };

    let (orbit, pan, zoom) = if disabled.is_some() {
        hold.clear();
        (
            CubeFacePanelContent::active("Orbit", ["Disabled"]),
            CubeFacePanelContent::active("Pan", ["Disabled"]),
            CubeFacePanelContent::active("Zoom", ["Disabled"]),
        )
    } else {
        (
            held_content(
                &mut hold.orbit,
                time.delta(),
                active_labels(
                    &guidance.0,
                    OrbitCamInteractionKind::Orbit,
                    interaction_state.orbit_sources(),
                ),
                &guidance.0,
                CustomFaceLabel::Orbit,
            ),
            held_content(
                &mut hold.pan,
                time.delta(),
                active_labels(
                    &guidance.0,
                    OrbitCamInteractionKind::Pan,
                    interaction_state.pan_sources(),
                ),
                &guidance.0,
                CustomFaceLabel::Pan,
            ),
            held_content(
                &mut hold.zoom,
                time.delta(),
                active_labels(
                    &guidance.0,
                    OrbitCamInteractionKind::Zoom,
                    interaction_state.zoom_sources(),
                ),
                &guidance.0,
                CustomFaceLabel::Zoom,
            ),
        )
    };

    for (entity, kind) in &labels {
        let next = match kind {
            CustomFaceLabel::Orbit => orbit.clone(),
            CustomFaceLabel::Pan => pan.clone(),
            CustomFaceLabel::Zoom => zoom.clone(),
        };
        commands.set_tree(entity, cube_face_panel_tree(FACE_PANEL_STYLE, next));
    }
}

fn capture_zoom_started(
    event: On<OrbitCamInteractionStarted>,
    cameras: Query<(), With<CustomCamera>>,
    guidance: Res<FaceGuidance>,
    mut hold: ResMut<FaceLabelHold>,
) {
    if event.kind != OrbitCamInteractionKind::Zoom || cameras.get(event.camera).is_err() {
        return;
    }
    let Some(lines) = active_labels(&guidance.0, OrbitCamInteractionKind::Zoom, event.sources)
    else {
        return;
    };
    hold.zoom.update(std::time::Duration::ZERO, Some(lines));
}

fn spawn_face_panel(
    parent: &mut ChildSpawnerCommands,
    face: Face,
    kind: CustomFaceLabel,
    summary: &OrbitCamControlSummary,
) {
    let content = CubeFacePanelContent::idle(kind.title(), idle_labels(summary, kind.kind()));
    match cube_face_panel(FACE_PANEL_STYLE, content) {
        Ok(panel) => {
            parent.spawn((
                Name::new("Custom input face panel"),
                kind,
                panel,
                cube_face_transform(face, CUBE_SIZE),
            ));
        },
        Err(error) => {
            error!("input_custom: failed to build cube face panel: {error}");
        },
    }
}

/// All control labels configured for `kind`, shown while idle.
fn idle_labels(summary: &OrbitCamControlSummary, kind: OrbitCamInteractionKind) -> Vec<String> {
    summary
        .rows
        .iter()
        .filter(|row| row.kind == kind)
        .map(|row| row.label.clone())
        .collect()
}

/// The control labels for `kind` whose sources are live, or `None` when idle.
fn active_labels(
    summary: &OrbitCamControlSummary,
    kind: OrbitCamInteractionKind,
    sources: CameraInteractionSources,
) -> Option<Vec<String>> {
    let labels = summary
        .rows
        .iter()
        .filter(|row| row.kind == kind && row.camera_interaction_sources.intersects(sources))
        .map(|row| row.label.clone())
        .collect::<Vec<_>>();
    (!labels.is_empty()).then_some(labels)
}

fn held_content(
    hold: &mut ReleaseHold<Vec<String>>,
    delta: std::time::Duration,
    active_lines: Option<Vec<String>>,
    summary: &OrbitCamControlSummary,
    label: CustomFaceLabel,
) -> CubeFacePanelContent {
    match hold.update(delta, active_lines) {
        HoldState::Active(lines) | HoldState::Held(lines) => {
            CubeFacePanelContent::active(label.title(), lines.clone())
        },
        HoldState::Idle => {
            CubeFacePanelContent::idle(label.title(), idle_labels(summary, label.kind()))
        },
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// DESCRIPTION PANEL — on-screen explainer for custom bindings.
// ═════════════════════════════════════════════════════════════════════════════

const DESCRIPTION_TITLE: &str = "Custom Bindings";
const DESCRIPTION_LINES: [&str; 5] = [
    "This example uses OrbitCamInputMode::Bindings with a multi-device OrbitCamBindings set.",
    "Bindings can combine mouse drag, trackpad scroll, wheel, and pinch input.",
    "Unlike keyboard-only bindings, one camera action can accept several source types.",
    "Unlike Manual mode, Lagrange still reads and routes the input for you.",
    "Press T to toggle CameraInputDisabled on the camera.",
];

fn description_panel() -> DescriptionPanel {
    DescriptionPanel::new(DESCRIPTION_TITLE)
        .with_fit_width()
        .with_body_size(LABEL_SIZE.0)
        .lines(DESCRIPTION_LINES)
}

// ═════════════════════════════════════════════════════════════════════════════
// CUBE SPIN — decorative idle spin toggled by Fairy Dust's `P Pause` helper.
// ═════════════════════════════════════════════════════════════════════════════

// ═════════════════════════════════════════════════════════════════════════════
// SCENE SCAFFOLDING — cube body and ground sized to match.
// ═════════════════════════════════════════════════════════════════════════════

const CUBE_GROUND_CLEARANCE: f32 = 0.1;
const CUBE_COLOR: Color = fairy_dust::EXAMPLE_CUBE_COLOR;
const CUBE_SIZE: f32 = fairy_dust::EXAMPLE_CUBE_SIZE;
const CUBE_TRANSLATION: Vec3 = fairy_dust::example_cube_on_ground(CUBE_GROUND_CLEARANCE);

const GROUND_SIZE: f32 = fairy_dust::EXAMPLE_GROUND_SIZE;
