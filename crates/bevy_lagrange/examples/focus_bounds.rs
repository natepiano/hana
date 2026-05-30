//! Demonstrates how to keep the camera focus inside a cuboid by setting
//! `OrbitCam::focus_bounds_shape` and `focus_bounds_origin`. The Left/Right
//! arrows resize the bounds; the gold sphere marks the clamped focus.
//!
//! Controls:
//!   P      - Pause cube spin
//!   ← / →  - Shrink / grow the focus bounds cuboid

use std::f32::consts::FRAC_PI_2;
use std::f32::consts::PI;

use bevy::color::LinearRgba;
use bevy::prelude::*;
use bevy_diegetic::AlignX;
use bevy_diegetic::AlignY;
use bevy_diegetic::Anchor as PanelAnchor;
use bevy_diegetic::DiegeticPanel;
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
use bevy_lagrange::OrbitCam;
use bevy_lagrange::OrbitCamPreset;
use fairy_dust::Anchor;
use fairy_dust::CameraHomeTarget;
use fairy_dust::ControlActivation;
use fairy_dust::Face;
use fairy_dust::TitleBar;
use fairy_dust::TitleChipActivation;

fn main() {
    fairy_dust::sprinkle_example()
        .with_brp_extras()
        .with_save_window_position()
        .with_studio_lighting()
        .key_light_illuminance(KEY_LIGHT_ILLUMINANCE)
        .with_ground_plane()
        .with_cube()
        .size(CUBE_SIZE)
        .color(CUBE_COLOR)
        .transform(Transform::from_translation(CUBE_TRANSLATION))
        .insert((Cube, CameraHomeTarget))
        .with_orbit_cam_preset(configure_camera, OrbitCamPreset::BlenderLike)
        .with_camera_home()
        .yaw(HOME_YAW)
        .pitch(HOME_PITCH)
        .margin(HOME_MARGIN)
        .with_title_bar(
            TitleBar::new()
                .with_title("Focus Bounds")
                .with_anchor(Anchor::TopLeft)
                .control(PAUSE_CONTROL)
                .control(BOUNDS_SMALLER_CONTROL)
                .control(BOUNDS_LARGER_CONTROL),
        )
        .wire_chip_to_activation::<AnimationPause>(PAUSE_CONTROL)
        .with_camera_control_panel()
        .init_resource::<AnimationPause>()
        .init_resource::<FocusBounds>()
        .add_systems(PostStartup, spawn_story_panels)
        .add_systems(
            Update,
            (
                toggle_pause,
                resize_focus_bounds,
                sync_focus_bounds,
                rotate_cube,
                draw_focus_gizmos,
            )
                .chain(),
        )
        .run();
}

// ═════════════════════════════════════════════════════════════════════════════
// FOCUS BOUNDS — apply, resize, and visualize the OrbitCam focus cuboid.
// ═════════════════════════════════════════════════════════════════════════════
//
// How it works: configure_camera writes focus_bounds_shape and
// focus_bounds_origin onto the OrbitCam at startup. resize_focus_bounds reads
// Left/Right input into the FocusBounds resource; sync_focus_bounds writes the
// updated size back to every OrbitCam each frame. draw_focus_gizmos draws the
// bounds cuboid and a sphere at the camera's clamped focus point.

const BOUNDS_GIZMO_COLOR: Color = Color::linear_rgb(1.6, 1.6, 1.5);
const BOUNDS_LARGER_CONTROL: &str = "→ Increase";
const BOUNDS_SMALLER_CONTROL: &str = "← Decrease";
const FOCUS_BOUNDS_INITIAL_SIZE: f32 = 1.0;
const FOCUS_BOUNDS_MAX_SIZE: f32 = 3.0;
const FOCUS_BOUNDS_MIN_SIZE: f32 = 0.5;
const FOCUS_BOUNDS_ORIGIN: Vec3 = Vec3::splat(1.0);
const FOCUS_BOUNDS_STEP: f32 = 0.25;
const FOCUS_GIZMO_COLOR: Color = Color::linear_rgb(3.5, 2.8, 0.2);
const FOCUS_GIZMO_RADIUS: f32 = 0.06;

#[derive(Component)]
struct Cube;

#[derive(Resource)]
struct FocusBounds {
    size: f32,
}

impl Default for FocusBounds {
    fn default() -> Self {
        Self {
            size: FOCUS_BOUNDS_INITIAL_SIZE,
        }
    }
}

fn configure_camera(camera: &mut OrbitCam) {
    apply_focus_bounds(camera, FOCUS_BOUNDS_INITIAL_SIZE);
}

fn apply_focus_bounds(camera: &mut OrbitCam, size: f32) {
    camera.focus_bounds_shape = Some(Cuboid::new(size, size, size).into());
    camera.focus_bounds_origin = FOCUS_BOUNDS_ORIGIN;
}

fn resize_focus_bounds(keys: Res<ButtonInput<KeyCode>>, mut bounds: ResMut<FocusBounds>) {
    let step = match (
        keys.just_pressed(KeyCode::ArrowLeft),
        keys.just_pressed(KeyCode::ArrowRight),
    ) {
        (true, false) => -FOCUS_BOUNDS_STEP,
        (false, true) => FOCUS_BOUNDS_STEP,
        _ => return,
    };
    bounds.size = (bounds.size + step).clamp(FOCUS_BOUNDS_MIN_SIZE, FOCUS_BOUNDS_MAX_SIZE);
}

fn sync_focus_bounds(bounds: Res<FocusBounds>, mut cameras: Query<&mut OrbitCam>) {
    if !bounds.is_changed() {
        return;
    }
    for mut camera in &mut cameras {
        apply_focus_bounds(&mut camera, bounds.size);
    }
}

fn draw_focus_gizmos(bounds: Res<FocusBounds>, cameras: Query<&OrbitCam>, mut gizmos: Gizmos) {
    gizmos.cube(
        Transform::from_translation(FOCUS_BOUNDS_ORIGIN).with_scale(Vec3::splat(bounds.size)),
        BOUNDS_GIZMO_COLOR,
    );

    let Ok(camera) = cameras.single() else {
        return;
    };
    gizmos.sphere(
        Isometry3d::new(camera.focus, Quat::IDENTITY),
        FOCUS_GIZMO_RADIUS,
        FOCUS_GIZMO_COLOR,
    );
}

// ═════════════════════════════════════════════════════════════════════════════
// CUBE FACE STORY PANELS — diegetic panels narrate the focus bounds API.
// ═════════════════════════════════════════════════════════════════════════════

const FACE_PANEL_COLOR: Color = Color::linear_rgb(0.45, 1.25, 6.0);
const FACE_PANEL_OFFSET: f32 = CUBE_SIZE * 0.5 + 0.006;
const FACE_PANEL_PADDING: f32 = 0.06;
const FACE_PANEL_ROW_GAP: f32 = 0.025;
const FACE_PANEL_SIZE: f32 = CUBE_SIZE * 0.88;
const FACE_PANEL_TEXT_SIZE: f32 = 56.0;

const BOUNDS_SHAPE_LINES: &[&str] = &["OrbitCam has", "focus_bounds_shape", "set to a cuboid"];
const BOUNDS_ORIGIN_LINES: &[&str] =
    &["focus_bounds_origin", "places the cuboid", "in world space"];
const PAN_CLAMP_LINES: &[&str] = &["panning clamps", "camera focus", "inside the cuboid"];
const RESIZE_LINES: &[&str] = &["Left and Right", "resize the", "bounds cuboid"];
const TOP_LABEL_LINES: &[&str] = &["Focus", "Bounds"];
const BOTTOM_LABEL_LINES: &[&str] = &["P pauses", "cube spin"];

fn spawn_story_panels(mut commands: Commands, cubes: Query<Entity, With<Cube>>) {
    let Ok(cube) = cubes.single() else {
        return;
    };

    commands.entity(cube).with_children(|parent| {
        spawn_story_panel(parent, Face::Front, BOUNDS_SHAPE_LINES);
        spawn_story_panel(parent, Face::Right, BOUNDS_ORIGIN_LINES);
        spawn_story_panel(parent, Face::Back, PAN_CLAMP_LINES);
        spawn_story_panel(parent, Face::Left, RESIZE_LINES);
        spawn_story_panel(parent, Face::Top, TOP_LABEL_LINES);
        spawn_story_panel(parent, Face::Bottom, BOTTOM_LABEL_LINES);
    });
}

fn spawn_story_panel(
    parent: &mut ChildSpawnerCommands,
    face: Face,
    lines: &'static [&'static str],
) {
    match story_panel(lines) {
        Ok(panel) => {
            parent.spawn((
                Name::new("Focus bounds story panel"),
                panel,
                face_panel_transform(face),
            ));
        },
        Err(error) => {
            error!("focus_bounds: failed to build cube face panel: {error}");
        },
    }
}

fn face_panel_transform(face: Face) -> Transform {
    match face {
        Face::Front => Transform::from_xyz(0.0, 0.0, FACE_PANEL_OFFSET),
        Face::Right => Transform::from_xyz(FACE_PANEL_OFFSET, 0.0, 0.0)
            .with_rotation(Quat::from_rotation_y(FRAC_PI_2)),
        Face::Back => Transform::from_xyz(0.0, 0.0, -FACE_PANEL_OFFSET)
            .with_rotation(Quat::from_rotation_y(PI)),
        Face::Left => Transform::from_xyz(-FACE_PANEL_OFFSET, 0.0, 0.0)
            .with_rotation(Quat::from_rotation_y(-FRAC_PI_2)),
        Face::Top => Transform::from_xyz(0.0, FACE_PANEL_OFFSET, 0.0)
            .with_rotation(Quat::from_rotation_x(-FRAC_PI_2)),
        Face::Bottom => Transform::from_xyz(0.0, -FACE_PANEL_OFFSET, 0.0)
            .with_rotation(Quat::from_rotation_x(FRAC_PI_2)),
    }
}

fn story_panel(lines: &'static [&'static str]) -> Result<DiegeticPanel, InvalidSize> {
    let transparent = transparent_panel_material();
    DiegeticPanel::world()
        .size(FACE_PANEL_SIZE, FACE_PANEL_SIZE)
        .font_unit(Unit::Millimeters)
        .anchor(PanelAnchor::Center)
        .material(transparent)
        .text_material(emissive_text_material())
        .with_tree(build_story_panel_tree(lines))
        .build()
}

fn transparent_panel_material() -> StandardMaterial {
    StandardMaterial {
        base_color: Color::NONE,
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        ..default_panel_material()
    }
}

fn emissive_text_material() -> StandardMaterial {
    let mut emissive: LinearRgba = FACE_PANEL_COLOR.into();
    emissive.red *= 4.0;
    emissive.green *= 4.0;
    emissive.blue *= 4.0;

    StandardMaterial {
        base_color: Color::NONE,
        emissive,
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        ..default_panel_material()
    }
}

fn build_story_panel_tree(lines: &'static [&'static str]) -> LayoutTree {
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

    let text = LayoutTextStyle::new(FACE_PANEL_TEXT_SIZE)
        .with_color(FACE_PANEL_COLOR)
        .with_align(TextAlign::Center)
        .with_shadow_mode(GlyphShadowMode::None);
    for line in lines {
        builder.text(*line, text.clone());
    }

    builder.build()
}

// ═════════════════════════════════════════════════════════════════════════════
// SCENE SCAFFOLDING — camera home pose, lighting, cube placement.
// ═════════════════════════════════════════════════════════════════════════════

const CUBE_COLOR: Color = fairy_dust::EXAMPLE_CUBE_COLOR;
const CUBE_SIZE: f32 = fairy_dust::EXAMPLE_CUBE_SIZE;
const CUBE_TRANSLATION: Vec3 = fairy_dust::example_cube_on_ground(0.1);

const HOME_MARGIN: f32 = 0.5;
const HOME_PITCH: f32 = 0.3;
const HOME_YAW: f32 = 0.0;

const KEY_LIGHT_ILLUMINANCE: f32 = 2_500.0;

// ═════════════════════════════════════════════════════════════════════════════
// CUBE SPIN — decorative cube rotation with a pause toggle.
// ═════════════════════════════════════════════════════════════════════════════

const CUBE_SPIN_DEGREES_PER_SECOND: f32 = 8.0;
const PAUSE_CONTROL: &str = "P Pause";

#[derive(Resource, Default)]
struct AnimationPause {
    paused: bool,
}

impl AnimationPause {
    const fn control_activation(&self) -> ControlActivation {
        if self.paused {
            ControlActivation::Active
        } else {
            ControlActivation::Inactive
        }
    }
}

impl TitleChipActivation for AnimationPause {
    fn activation(&self) -> ControlActivation { self.control_activation() }
}

fn toggle_pause(keys: Res<ButtonInput<KeyCode>>, mut pause: ResMut<AnimationPause>) {
    if keys.just_pressed(KeyCode::KeyP) {
        pause.paused = !pause.paused;
    }
}

fn rotate_cube(
    time: Res<Time>,
    pause: Res<AnimationPause>,
    mut cube_query: Query<&mut Transform, With<Cube>>,
) {
    if pause.paused {
        return;
    }

    if let Ok(mut cube_transform) = cube_query.single_mut() {
        cube_transform.rotate_y(-CUBE_SPIN_DEGREES_PER_SECOND.to_radians() * time.delta_secs());
    }
}
