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
use bevy_diegetic::AlignX;
use bevy_diegetic::AlignY;
use bevy_diegetic::Anchor as PanelAnchor;
use bevy_diegetic::DiegeticPanel;
use bevy_diegetic::DiegeticPanelCommands;
use bevy_diegetic::Direction;
use bevy_diegetic::El;
use bevy_diegetic::GlyphShadowMode;
use bevy_diegetic::InvalidSize;
use bevy_diegetic::LayoutBuilder;
use bevy_diegetic::LayoutTextStyle;
use bevy_diegetic::LayoutTree;
use bevy_diegetic::Padding;
use bevy_diegetic::Sizing;
use bevy_diegetic::TextAlign;
use bevy_diegetic::Unit;
use bevy_diegetic::default_panel_material;
use bevy_enhanced_input::prelude::ModKeys;
use bevy_lagrange::CameraInputDisabled;
use bevy_lagrange::CameraInputRoutingConfig;
use bevy_lagrange::CameraInteractionSources;
use bevy_lagrange::NoPositionFallback;
use bevy_lagrange::OrbitCam;
use bevy_lagrange::OrbitCamBindings;
use bevy_lagrange::OrbitCamBindingsError;
use bevy_lagrange::OrbitCamInputMode;
use bevy_lagrange::OrbitCamInteractionKind;
use bevy_lagrange::OrbitCamInteractionStarted;
use bevy_lagrange::OrbitCamInteractionState;
use bevy_lagrange::OrbitCamMouseDrag;
use bevy_lagrange::OrbitCamMouseWheelZoom;
use bevy_lagrange::OrbitCamPinchZoom;
use bevy_lagrange::OrbitCamTrackpadScroll;
use bevy_lagrange::UpsideDownPolicy;
use bevy_lagrange::ZoomDirection;
use fairy_dust::Anchor;
use fairy_dust::CameraHomeTarget;
use fairy_dust::ControlActivation;
use fairy_dust::DescriptionPanel;
use fairy_dust::Face;
use fairy_dust::FairyDustOrbitCam;
use fairy_dust::LABEL_SIZE;
use fairy_dust::TitleBar;

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
        .wire_chip_to_state::<InputDisabledState, _>(INPUT_DISABLED_CONTROL, |state| {
            activation_for(state.disabled)
        })
        .with_description_panel(description_panel())
        .with_camera_control_panel()
        .add_systems(Startup, spawn_camera)
        .add_systems(PostStartup, spawn_face_labels)
        .insert_resource(FaceLabelHold::default())
        .add_systems(
            Update,
            (toggle_camera_controls, update_face_labels, spin_cube),
        )
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
const CAMERA_PITCH_LIMIT: f32 = TAU / 3.0;
const CAMERA_RADIUS: f32 = 5.0;
const CAMERA_YAW: f32 = TAU / 8.0;
const CAMERA_ZOOM_LOWER_LIMIT: f32 = 1.0;
const CAMERA_ZOOM_SENSITIVITY: f32 = 0.5;
const CAMERA_ZOOM_UPPER_LIMIT: f32 = 5.0;
const HOME_MARGIN: f32 = 0.5;
const INPUT_DISABLED_CONTROL: &str = "T Disabled";

#[derive(Resource)]
struct CustomBindings(OrbitCamBindings);

#[derive(Resource, Default)]
struct InputDisabledState {
    disabled: bool,
}

#[derive(Component)]
struct CustomCamera;

fn spawn_camera(mut commands: Commands, bindings: Res<CustomBindings>) {
    commands.spawn((
        OrbitCam {
            focus: CAMERA_FOCUS,
            yaw: Some(CAMERA_YAW),
            pitch: Some(CAMERA_PITCH),
            radius: Some(CAMERA_RADIUS),
            pitch_upper_limit: Some(CAMERA_PITCH_LIMIT),
            pitch_lower_limit: Some(-CAMERA_PITCH_LIMIT),
            zoom_upper_limit: Some(CAMERA_ZOOM_UPPER_LIMIT),
            zoom_lower_limit: CAMERA_ZOOM_LOWER_LIMIT,
            orbit_sensitivity: CAMERA_ORBIT_SENSITIVITY,
            pan_sensitivity: CAMERA_PAN_SENSITIVITY,
            zoom_sensitivity: CAMERA_ZOOM_SENSITIVITY,
            upside_down_policy: UpsideDownPolicy::Allow,
            ..default()
        },
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
        .zoom(OrbitCamMouseWheelZoom::default())
        .zoom(OrbitCamTrackpadScroll::default().with_mod_keys(ModKeys::CONTROL))
        .zoom(OrbitCamPinchZoom)
        .zoom_direction(ZoomDirection::Reversed)
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

const FACE_LABEL_COLOR: Color = Color::srgb(0.1, 0.35, 1.0);
const FACE_LABEL_RELEASE_DELAY_SECS: f32 = 0.5;
const FACE_PANEL_ACTIVE_SIZE: f32 = 46.0;
const FACE_PANEL_BODY_SIZE: f32 = 40.0;
const FACE_PANEL_OFFSET: f32 = CUBE_SIZE * 0.5 + 0.006;
const FACE_PANEL_PADDING: f32 = 0.065;
const FACE_PANEL_ROW_GAP: f32 = 0.025;
const FACE_PANEL_SIZE: f32 = CUBE_SIZE * 0.88;
const FACE_PANEL_TITLE_SIZE: f32 = 66.0;

const ORBIT_LABELS: &[&str] = &["Middle mouse drag", "Trackpad scroll"];
const PAN_LABELS: &[&str] = &["Right mouse drag", "Shift + trackpad"];
const ZOOM_LABELS: &[&str] = &["Wheel", "Ctrl + trackpad", "Pinch"];

#[derive(Component)]
struct CustomInputCube;

#[derive(Component, Clone, Copy)]
enum CustomFaceLabel {
    Orbit,
    Pan,
    Zoom,
}

#[derive(Resource, Default)]
struct FaceLabelHold {
    orbit: HeldFaceLabel,
    pan:   HeldFaceLabel,
    zoom:  HeldFaceLabel,
}

impl FaceLabelHold {
    fn clear(&mut self) {
        self.orbit.clear();
        self.pan.clear();
        self.zoom.clear();
    }
}

#[derive(Default)]
struct HeldFaceLabel {
    remaining_secs: f32,
    lines:          Option<Vec<String>>,
}

impl HeldFaceLabel {
    fn clear(&mut self) {
        self.remaining_secs = 0.0;
        self.lines = None;
    }

    fn hold_sources(&mut self, sources: CameraInteractionSources) {
        let Some(lines) = source_lines(sources) else {
            return;
        };
        self.remaining_secs = FACE_LABEL_RELEASE_DELAY_SECS;
        self.lines = Some(lines);
    }
}

#[derive(Clone)]
struct FacePanelContent {
    title:  &'static str,
    lines:  Vec<String>,
    active: bool,
}

impl FacePanelContent {
    fn idle(title: &'static str, lines: &'static [&'static str]) -> Self {
        Self {
            title,
            lines: lines.iter().map(|line| (*line).to_string()).collect(),
            active: false,
        }
    }

    const fn active(title: &'static str, lines: Vec<String>) -> Self {
        Self {
            title,
            lines,
            active: true,
        }
    }
}

fn spawn_face_labels(mut commands: Commands, cubes: Query<Entity, With<CustomInputCube>>) {
    let Ok(cube) = cubes.single() else {
        return;
    };

    commands.entity(cube).with_children(|parent| {
        for face in [Face::Front, Face::Back] {
            spawn_face_panel(
                parent,
                face,
                CustomFaceLabel::Orbit,
                FacePanelContent::idle("Orbit", ORBIT_LABELS),
            );
        }
        for face in [Face::Left, Face::Right] {
            spawn_face_panel(
                parent,
                face,
                CustomFaceLabel::Pan,
                FacePanelContent::idle("Pan", PAN_LABELS),
            );
        }
        for face in [Face::Top, Face::Bottom] {
            spawn_face_panel(
                parent,
                face,
                CustomFaceLabel::Zoom,
                FacePanelContent::idle("Zoom", ZOOM_LABELS),
            );
        }
    });
}

fn spin_cube(time: Res<Time>, mut cubes: Query<&mut Transform, With<CustomInputCube>>) {
    for mut transform in &mut cubes {
        transform.rotate_y(CUBE_SPIN_SPEED * time.delta_secs());
    }
}

fn update_face_labels(
    mut commands: Commands,
    time: Res<Time>,
    mut hold: ResMut<FaceLabelHold>,
    cameras: Query<(&OrbitCamInteractionState, Option<&CameraInputDisabled>), With<CustomCamera>>,
    labels: Query<(Entity, &CustomFaceLabel)>,
) {
    let Ok((interaction_state, disabled)) = cameras.single() else {
        return;
    };

    let (orbit, pan, zoom) = if disabled.is_some() {
        hold.clear();
        (
            FacePanelContent::active("Orbit", vec!["Disabled".to_string()]),
            FacePanelContent::active("Pan", vec!["Disabled".to_string()]),
            FacePanelContent::active("Zoom", vec!["Disabled".to_string()]),
        )
    } else {
        (
            held_content(
                &mut hold.orbit,
                time.delta_secs(),
                source_lines(interaction_state.orbit_sources()),
                ORBIT_LABELS,
                "Orbit",
            ),
            held_content(
                &mut hold.pan,
                time.delta_secs(),
                source_lines(interaction_state.pan_sources()),
                PAN_LABELS,
                "Pan",
            ),
            held_content(
                &mut hold.zoom,
                time.delta_secs(),
                source_lines(interaction_state.zoom_sources()),
                ZOOM_LABELS,
                "Zoom",
            ),
        )
    };

    for (entity, kind) in &labels {
        let next = match kind {
            CustomFaceLabel::Orbit => orbit.clone(),
            CustomFaceLabel::Pan => pan.clone(),
            CustomFaceLabel::Zoom => zoom.clone(),
        };
        commands.set_tree(entity, build_face_panel_tree(next));
    }
}

fn capture_zoom_started(
    event: On<OrbitCamInteractionStarted>,
    cameras: Query<(), With<CustomCamera>>,
    mut hold: ResMut<FaceLabelHold>,
) {
    if event.kind != OrbitCamInteractionKind::Zoom || cameras.get(event.camera).is_err() {
        return;
    }
    hold.zoom.hold_sources(event.sources);
}

fn spawn_face_panel(
    parent: &mut ChildSpawnerCommands,
    face: Face,
    kind: CustomFaceLabel,
    content: FacePanelContent,
) {
    match face_panel(content) {
        Ok(panel) => {
            parent.spawn((
                Name::new("Custom input face panel"),
                kind,
                panel,
                face_panel_transform(face),
            ));
        },
        Err(error) => {
            error!("input_custom: failed to build cube face panel: {error}");
        },
    }
}

fn face_panel_transform(face: Face) -> Transform {
    match face {
        Face::Front => Transform::from_xyz(0.0, 0.0, FACE_PANEL_OFFSET),
        Face::Back => Transform::from_xyz(0.0, 0.0, -FACE_PANEL_OFFSET)
            .with_rotation(Quat::from_rotation_y(std::f32::consts::PI)),
        Face::Right => Transform::from_xyz(FACE_PANEL_OFFSET, 0.0, 0.0)
            .with_rotation(Quat::from_rotation_y(std::f32::consts::FRAC_PI_2)),
        Face::Left => Transform::from_xyz(-FACE_PANEL_OFFSET, 0.0, 0.0)
            .with_rotation(Quat::from_rotation_y(-std::f32::consts::FRAC_PI_2)),
        Face::Top => Transform::from_xyz(0.0, FACE_PANEL_OFFSET, 0.0)
            .with_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2)),
        Face::Bottom => Transform::from_xyz(0.0, -FACE_PANEL_OFFSET, 0.0)
            .with_rotation(Quat::from_rotation_x(std::f32::consts::FRAC_PI_2)),
    }
}

fn face_panel(content: FacePanelContent) -> Result<DiegeticPanel, InvalidSize> {
    let transparent = face_panel_material();
    DiegeticPanel::world()
        .size(FACE_PANEL_SIZE, FACE_PANEL_SIZE)
        .font_unit(Unit::Millimeters)
        .anchor(PanelAnchor::Center)
        .material(transparent.clone())
        .text_material(transparent)
        .with_tree(build_face_panel_tree(content))
        .build()
}

fn face_panel_material() -> StandardMaterial {
    StandardMaterial {
        base_color: Color::NONE,
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        ..default_panel_material()
    }
}

fn build_face_panel_tree(content: FacePanelContent) -> LayoutTree {
    let mut builder = LayoutBuilder::with_root(
        El::new()
            .width(Sizing::fixed(FACE_PANEL_SIZE))
            .height(Sizing::fixed(FACE_PANEL_SIZE))
            .direction(Direction::TopToBottom)
            .child_alignment(AlignX::Center, AlignY::Center)
            .child_gap(FACE_PANEL_ROW_GAP)
            .padding(Padding::all(FACE_PANEL_PADDING))
            .clip(),
    );

    builder.text(
        content.title,
        LayoutTextStyle::new(FACE_PANEL_TITLE_SIZE)
            .with_color(FACE_LABEL_COLOR)
            .with_align(TextAlign::Center)
            .with_shadow_mode(GlyphShadowMode::None),
    );

    let body_size = if content.active {
        FACE_PANEL_ACTIVE_SIZE
    } else {
        FACE_PANEL_BODY_SIZE
    };
    let body = LayoutTextStyle::new(body_size)
        .with_color(FACE_LABEL_COLOR)
        .with_align(TextAlign::Center)
        .with_shadow_mode(GlyphShadowMode::None);

    for line in content.lines {
        builder.text(line, body.clone());
    }

    builder.build()
}

fn source_lines(sources: CameraInteractionSources) -> Option<Vec<String>> {
    let mut labels = Vec::new();
    if sources.contains(CameraInteractionSources::MOUSE) {
        labels.push("Mouse".to_string());
    }
    if sources.contains(CameraInteractionSources::WHEEL) {
        labels.push("Wheel".to_string());
    }
    if sources.contains(CameraInteractionSources::SMOOTH_SCROLL) {
        labels.push("Trackpad".to_string());
    }
    if sources.contains(CameraInteractionSources::PINCH) {
        labels.push("Pinch".to_string());
    }
    if sources.contains(CameraInteractionSources::KEYBOARD) {
        labels.push("Keyboard".to_string());
    }
    if sources.contains(CameraInteractionSources::GAMEPAD) {
        labels.push("Gamepad".to_string());
    }
    if sources.contains(CameraInteractionSources::MANUAL) {
        labels.push("Manual".to_string());
    }
    (!labels.is_empty()).then_some(labels)
}

fn held_content(
    hold: &mut HeldFaceLabel,
    delta_secs: f32,
    active_lines: Option<Vec<String>>,
    idle_lines: &'static [&'static str],
    title: &'static str,
) -> FacePanelContent {
    if let Some(lines) = active_lines {
        hold.remaining_secs = FACE_LABEL_RELEASE_DELAY_SECS;
        hold.lines = Some(lines);
        return FacePanelContent::active(title, hold.lines.clone().unwrap_or_default());
    }

    hold.remaining_secs = (hold.remaining_secs - delta_secs).max(0.0);
    if hold.remaining_secs > 0.0 {
        return FacePanelContent::active(title, hold.lines.clone().unwrap_or_default());
    }

    hold.lines = None;
    FacePanelContent::idle(title, idle_lines)
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
// SCENE SCAFFOLDING — cube body and ground sized to match.
// ═════════════════════════════════════════════════════════════════════════════

const CUBE_COLOR: Color = Color::srgb(0.8, 0.7, 0.6);
const CUBE_GROUND_CLEARANCE: f32 = 0.1;
const CUBE_SIZE: f32 = 1.0;
const CUBE_SPIN_SPEED: f32 = 0.2;
const CUBE_TRANSLATION: Vec3 = Vec3::new(0.0, CUBE_SIZE * 0.5 + CUBE_GROUND_CLEARANCE, 0.0);

const GROUND_SIZE: f32 = 5.0;
