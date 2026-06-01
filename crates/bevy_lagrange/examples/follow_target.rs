//! Demonstrates how to drive `OrbitCam::target_focus` from a moving entity so
//! the camera tracks it. `animate_cube` orbits the cube around the Y axis;
//! `camera_follow` copies the cube translation into `target_focus` each frame
//! and the camera interpolates the rendered focus toward it.
//!
//! Controls:
//!   P - Pause cube motion

use std::f32::consts::FRAC_PI_2;
use std::f32::consts::PI;
use std::f32::consts::TAU;

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
use bevy_diegetic::LayoutTree;
use bevy_diegetic::Padding;
use bevy_diegetic::Sizing;
use bevy_diegetic::TextAlign;
use bevy_diegetic::TextStyle;
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
                .with_title("Follow Target")
                .with_anchor(Anchor::TopLeft)
                .control(PAUSE_CONTROL),
        )
        .wire_chip_to_activation::<AnimationPause>(PAUSE_CONTROL)
        .with_camera_control_panel()
        .init_resource::<AnimationPause>()
        .add_systems(Startup, spawn_track_ring)
        .add_systems(PostStartup, spawn_story_panels)
        .add_systems(Update, (toggle_pause, animate_cube, camera_follow).chain())
        .run();
}

// ═════════════════════════════════════════════════════════════════════════════
// CAMERA FOLLOW — drive OrbitCam::target_focus from the moving cube.
// ═════════════════════════════════════════════════════════════════════════════
//
// How it works: configure_camera disables panning at startup so user input
// cannot override the focus. animate_cube moves the cube around a horizontal
// circle each frame (P pauses it). camera_follow then copies the cube
// translation into OrbitCam::target_focus; the camera interpolates the
// rendered focus toward that target.

const CAMERA_PAN_SENSITIVITY: f32 = 0.0;
const CAMERA_PAN_SMOOTHNESS: f32 = 0.0;
const CAMERA_RADIUS: f32 = 5.0;

const CUBE_ORBIT_DEGREES_PER_SECOND: f32 = 20.0;
const CUBE_ORBIT_RADIUS: f32 = 1.5;
const CUBE_SPIN_DEGREES_PER_SECOND: f32 = 8.0;
const PAUSE_CONTROL: &str = "P Pause";

#[derive(Component)]
struct Cube;

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

const fn configure_camera(camera: &mut OrbitCam) {
    camera.focus = CUBE_TRANSLATION;
    camera.target_focus = CUBE_TRANSLATION;
    camera.yaw = Some(HOME_YAW);
    camera.pitch = Some(HOME_PITCH);
    camera.radius = Some(CAMERA_RADIUS);
    // Panning the camera changes the focus, so disable it while this example
    // drives the focus from the moving cube.
    camera.pan_sensitivity = CAMERA_PAN_SENSITIVITY;
    camera.pan_smoothness = CAMERA_PAN_SMOOTHNESS;
}

fn toggle_pause(keys: Res<ButtonInput<KeyCode>>, mut pause: ResMut<AnimationPause>) {
    if keys.just_pressed(KeyCode::KeyP) {
        pause.paused = !pause.paused;
    }
}

fn animate_cube(
    time: Res<Time>,
    pause: Res<AnimationPause>,
    mut cube_query: Query<&mut Transform, With<Cube>>,
    mut angle: Local<f32>,
) {
    if pause.paused {
        return;
    }

    if let Ok(mut cube_transform) = cube_query.single_mut() {
        *angle += CUBE_ORBIT_DEGREES_PER_SECOND.to_radians() * time.delta_secs() % TAU;
        let position = Vec3::new(
            angle.sin() * CUBE_ORBIT_RADIUS,
            CUBE_TRANSLATION.y,
            angle.cos() * CUBE_ORBIT_RADIUS,
        );
        cube_transform.translation = position;
        cube_transform.rotate_y(-CUBE_SPIN_DEGREES_PER_SECOND.to_radians() * time.delta_secs());
    }
}

fn camera_follow(
    mut orbit_cam_query: Query<&mut OrbitCam>,
    cube_query: Query<&Transform, With<Cube>>,
) {
    if let Ok(mut orbit_cam) = orbit_cam_query.single_mut()
        && let Ok(cube_transform) = cube_query.single()
    {
        orbit_cam.target_focus = cube_transform.translation;
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// TRACK RING — flat annulus marking the cube's orbit path on the ground.
// ═════════════════════════════════════════════════════════════════════════════

const TRACK_RING_COLOR: Color = Color::srgba(0.78, 0.82, 0.86, 0.85);
const TRACK_RING_RESOLUTION: u32 = 128;
const TRACK_RING_THICKNESS: f32 = 0.035;
const TRACK_RING_Y: f32 = 0.006;

fn spawn_track_ring(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    commands.spawn((
        Name::new("Cube orbit track"),
        Mesh3d(
            meshes.add(
                Annulus::new(
                    CUBE_ORBIT_RADIUS - TRACK_RING_THICKNESS,
                    CUBE_ORBIT_RADIUS + TRACK_RING_THICKNESS,
                )
                .mesh()
                .resolution(TRACK_RING_RESOLUTION),
            ),
        ),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: TRACK_RING_COLOR,
            alpha_mode: AlphaMode::Blend,
            cull_mode: None,
            double_sided: true,
            unlit: true,
            ..default()
        })),
        Transform::from_xyz(0.0, TRACK_RING_Y, 0.0)
            .with_rotation(Quat::from_rotation_x(-FRAC_PI_2)),
    ));
}

// ═════════════════════════════════════════════════════════════════════════════
// CUBE FACE STORY PANELS — diegetic panels narrate the follow flow.
// ═════════════════════════════════════════════════════════════════════════════

const FACE_PANEL_COLOR: Color = Color::linear_rgb(0.45, 1.25, 6.0);
const FACE_PANEL_OFFSET: f32 = CUBE_SIZE * 0.5 + 0.006;
const FACE_PANEL_PADDING: f32 = 0.06;
const FACE_PANEL_ROW_GAP: f32 = 0.025;
const FACE_PANEL_SIZE: f32 = CUBE_SIZE * 0.88;
const FACE_PANEL_TEXT_SIZE: f32 = 56.0;

const ANIMATE_CUBE_LINES: &[&str] = &["animate_cube moves", "the cube in a", "horizontal circle"];
const CAMERA_FOCUS_LINES: &[&str] = &["camera focus is", "updated to be", "centered on the cube"];
const PAN_DISABLED_LINES: &[&str] = &["pan is disabled", "so as not to", "move the focus"];
const ORBIT_CAM_LINES: &[&str] = &["OrbitCam uses", "the focus point", "to frame the view"];
const TOP_GREETING_LINES: &[&str] = &["Hello!"];
const BOTTOM_GREETING_LINES: &[&str] = &["Nice to", "see you!"];

fn spawn_story_panels(mut commands: Commands, cubes: Query<Entity, With<Cube>>) {
    let Ok(cube) = cubes.single() else {
        return;
    };

    commands.entity(cube).with_children(|parent| {
        spawn_story_panel(parent, Face::Front, ANIMATE_CUBE_LINES);
        spawn_story_panel(parent, Face::Right, CAMERA_FOCUS_LINES);
        spawn_story_panel(parent, Face::Back, PAN_DISABLED_LINES);
        spawn_story_panel(parent, Face::Left, ORBIT_CAM_LINES);
        spawn_story_panel(parent, Face::Top, TOP_GREETING_LINES);
        spawn_story_panel(parent, Face::Bottom, BOTTOM_GREETING_LINES);
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
                Name::new("Follow target story panel"),
                panel,
                face_panel_transform(face),
            ));
        },
        Err(error) => {
            error!("follow_target: failed to build cube face panel: {error}");
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

    let text = TextStyle::new(FACE_PANEL_TEXT_SIZE)
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
const HOME_PITCH: f32 = 0.25;
const HOME_YAW: f32 = 0.0;

const KEY_LIGHT_ILLUMINANCE: f32 = 2_500.0;
