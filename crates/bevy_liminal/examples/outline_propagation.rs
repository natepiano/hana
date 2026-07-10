//! Builds one robot from a parent/child mesh hierarchy to contrast two ways of
//! stopping an inherited outline. `NoOutline` skips the orange arm but permits
//! propagation to continue to its blue hand, while `OutlineBarrier` excludes
//! the purple arm and its pink hand.

use bevy::prelude::*;
use bevy_lagrange::OrbitCamPreset;
use bevy_liminal::LiminalPlugin;
use bevy_liminal::NoOutline;
use bevy_liminal::Outline;
use bevy_liminal::OutlineBarrier;
use bevy_liminal::OutlineCamera;
use fairy_dust::Anchor;
use fairy_dust::CameraHomeTarget;
use fairy_dust::DescriptionPanel;
use fairy_dust::LABEL_SIZE;
use fairy_dust::TitleBar;

const EXAMPLE_TITLE: &str = "Outline Exclusions";

fn main() {
    fairy_dust::sprinkle_example()
        .with_brp_extras()
        .with_save_window_position()
        .add_plugins(LiminalPlugin)
        .with_studio_lighting()
        .with_ground_plane()
        .with_orbit_cam_preset_bundle(|_| {}, OrbitCamPreset::blender_like(), OutlineCamera)
        .with_stable_transparency()
        .with_camera_home()
        .yaw(HOME_YAW)
        .pitch(HOME_PITCH)
        .margin(HOME_MARGIN)
        .with_title_bar(
            TitleBar::new()
                .with_title(EXAMPLE_TITLE)
                .with_anchor(Anchor::TopLeft),
        )
        .with_description_panel(description_panel())
        .with_camera_control_panel()
        .add_systems(Startup, spawn_outlined_hierarchy)
        .run();
}

// ═══════════════════════════════════════════════════════════════════════════════
// ROBOT HIERARCHY — the body owns `Outline`; every other mesh is its descendant.
// `NoOutline` skips one arm, while `OutlineBarrier` prunes the other arm's
// subtree.
// ═════════════════════════════════════════════════════════════════════════════════

const OUTLINE_WIDTH: f32 = 8.0;
const BODY_COLOR: Color = fairy_dust::EXAMPLE_CUBE_COLOR;
const NO_OUTLINE_COLOR: Color = Color::srgb(0.95, 0.35, 0.1);
const NO_OUTLINE_DESCENDANT_COLOR: Color = Color::srgb(0.15, 0.45, 0.95);
const BARRIER_COLOR: Color = Color::srgb(0.65, 0.2, 0.8);
const BARRIER_DESCENDANT_COLOR: Color = Color::srgb(0.85, 0.45, 0.95);

const BODY_SIZE: Vec3 = Vec3::new(1.2, 1.3, 0.7);
const BODY_TRANSLATION: Vec3 = Vec3::new(0.0, 1.35, 0.0);
const HEAD_RADIUS: f32 = 0.42;
const HEAD_TRANSLATION: Vec3 = Vec3::new(0.0, 1.08, 0.0);

const ARM_SIZE: Vec3 = Vec3::new(0.9, 0.28, 0.28);
const ARM_X_OFFSET: f32 = 1.0;
const ARM_Y: f32 = 0.15;
const NO_OUTLINE_TRANSLATION: Vec3 = Vec3::new(-ARM_X_OFFSET, ARM_Y, 0.0);
const BARRIER_TRANSLATION: Vec3 = Vec3::new(ARM_X_OFFSET, ARM_Y, 0.0);

const HAND_RADIUS: f32 = 0.3;
const HAND_X_OFFSET: f32 = 0.6;
const NO_OUTLINE_DESCENDANT_TRANSLATION: Vec3 = Vec3::new(-HAND_X_OFFSET, 0.0, 0.0);
const BARRIER_DESCENDANT_TRANSLATION: Vec3 = Vec3::new(HAND_X_OFFSET, 0.0, 0.0);

const LEG_SIZE: Vec3 = Vec3::new(0.34, 0.9, 0.38);
const LEG_X_OFFSET: f32 = 0.32;
const LEG_Y: f32 = -0.9;
const LEFT_LEG_TRANSLATION: Vec3 = Vec3::new(-LEG_X_OFFSET, LEG_Y, 0.0);
const RIGHT_LEG_TRANSLATION: Vec3 = Vec3::new(LEG_X_OFFSET, LEG_Y, 0.0);

const HOME_YAW: f32 = -0.35;
const HOME_PITCH: f32 = 0.3;
const HOME_MARGIN: f32 = 0.55;

fn spawn_outlined_hierarchy(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let body_mesh = meshes.add(Cuboid::from_size(BODY_SIZE));
    let head_mesh = meshes.add(Sphere::new(HEAD_RADIUS));
    let arm_mesh = meshes.add(Cuboid::from_size(ARM_SIZE));
    let hand_mesh = meshes.add(Sphere::new(HAND_RADIUS));
    let leg_mesh = meshes.add(Cuboid::from_size(LEG_SIZE));
    let body_material = materials.add(BODY_COLOR);
    let no_outline_material = materials.add(NO_OUTLINE_COLOR);
    let no_outline_descendant_material = materials.add(NO_OUTLINE_DESCENDANT_COLOR);
    let barrier_material = materials.add(BARRIER_COLOR);
    let barrier_descendant_material = materials.add(BARRIER_DESCENDANT_COLOR);

    commands
        .spawn((
            Name::new("Outlined robot body"),
            Mesh3d(body_mesh),
            MeshMaterial3d(body_material.clone()),
            Transform::from_translation(BODY_TRANSLATION),
            Outline::jump_flood(OUTLINE_WIDTH)
                .with_color(Color::WHITE)
                .build(),
            CameraHomeTarget,
        ))
        .with_children(|parent| {
            parent.spawn((
                Name::new("Outlined robot head"),
                Mesh3d(head_mesh),
                MeshMaterial3d(body_material.clone()),
                Transform::from_translation(HEAD_TRANSLATION),
                CameraHomeTarget,
            ));

            for (name, translation) in [
                ("Outlined left leg", LEFT_LEG_TRANSLATION),
                ("Outlined right leg", RIGHT_LEG_TRANSLATION),
            ] {
                parent.spawn((
                    Name::new(name),
                    Mesh3d(leg_mesh.clone()),
                    MeshMaterial3d(body_material.clone()),
                    Transform::from_translation(translation),
                    CameraHomeTarget,
                ));
            }

            parent
                .spawn((
                    Name::new("NoOutline robot arm"),
                    Mesh3d(arm_mesh.clone()),
                    MeshMaterial3d(no_outline_material),
                    Transform::from_translation(NO_OUTLINE_TRANSLATION),
                    NoOutline,
                    CameraHomeTarget,
                ))
                .with_child((
                    Name::new("Outlined hand beyond NoOutline"),
                    Mesh3d(hand_mesh.clone()),
                    MeshMaterial3d(no_outline_descendant_material),
                    Transform::from_translation(NO_OUTLINE_DESCENDANT_TRANSLATION),
                    CameraHomeTarget,
                ));

            parent
                .spawn((
                    Name::new("OutlineBarrier robot arm"),
                    Mesh3d(arm_mesh),
                    MeshMaterial3d(barrier_material),
                    Transform::from_translation(BARRIER_TRANSLATION),
                    OutlineBarrier,
                    CameraHomeTarget,
                ))
                .with_child((
                    Name::new("Unoutlined hand behind OutlineBarrier"),
                    Mesh3d(hand_mesh),
                    MeshMaterial3d(barrier_descendant_material),
                    Transform::from_translation(BARRIER_DESCENDANT_TRANSLATION),
                    CameraHomeTarget,
                ));
        });
}

// ════════════════════════════════════════════════════════════════════════════════
// UI — explain the mesh-only and subtree exclusion behaviors.
// ════════════════════════════════════════════════════════════════════════════════

const DESCRIPTION_HEADING: &str = "Propagation";
const DESCRIPTION_LINES: [&str; 5] = [
    "The robot is one parent/child hierarchy.",
    "Only its center body owns Outline.",
    "Its head and legs inherit the white outline.",
    "Orange NoOutline arm; blue hand still inherits.",
    "Purple OutlineBarrier arm; pink hand is excluded.",
];

fn description_panel() -> DescriptionPanel {
    DescriptionPanel::new(DESCRIPTION_HEADING)
        .with_fit_width()
        .with_body_size(LABEL_SIZE.0)
        .lines(DESCRIPTION_LINES)
}
