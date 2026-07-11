//! Six quads folded from a cross net into a box.

use bevy::color::Srgba;
use bevy::color::palettes::css::CORAL;
use bevy::color::palettes::css::GOLD;
use bevy::color::palettes::css::MEDIUM_PURPLE;
use bevy::color::palettes::css::SEA_GREEN;
use bevy::color::palettes::css::SKY_BLUE;
use bevy::color::palettes::css::TURQUOISE;
use bevy::prelude::*;
use bevy_lagrange::OrbitCamPreset;
use fairy_dust::Anchor;
use fairy_dust::CameraHomeTarget;
use fairy_dust::DescriptionPanel;
use fairy_dust::TitleBar;
use hana_valence::AnchorPose;
use hana_valence::AnchoredTo;
use hana_valence::Edge;
use hana_valence::FoldAngles;
use hana_valence::FoldSequence;
use hana_valence::FoldSequenceBuilder;
use hana_valence::Hinge;

#[path = "../fixtures.rs"]
#[allow(
    dead_code,
    reason = "shared geometry fixtures; this example uses a subset"
)]
mod fixtures;

use fixtures::QUAD_BOTTOM_EDGE;
use fixtures::QUAD_LEFT_EDGE;
use fixtures::QUAD_RIGHT_EDGE;
use fixtures::QUAD_TOP_EDGE;

// app
const DESCRIPTION_LINES: [&str; 4] = [
    "Space: stage 0 raises the lid",
    "Space: stage 1 raises all four walls",
    "Shift+Space reverses walls, then lid",
    "P plays toward the remembered endpoint; center stays fixed",
];
const DESCRIPTION_TITLE: &str = "Two-stage fold sequence";
const EXAMPLE_TITLE: &str = "Box Net";

// animation
const BOX_FOLD_ANGLE: f32 = -core::f32::consts::FRAC_PI_2;
const FOLD_SECONDS: f32 = 2.5;

// camera home
const HOME_MARGIN: f32 = 0.4;
const HOME_PITCH: f32 = 0.87;
const HOME_YAW: f32 = 0.87;

// placement: the net folds up out of the XZ ground plane. `center` is the box's
// bottom face; this lifts it clear of the ground so it never z-fights the plane.
const GROUND_CLEARANCE: f32 = 0.02;
const BOX_ROOT_ROTATION: f32 = -core::f32::consts::FRAC_PI_2;

// quad
const FACE_COLORS: [Srgba; FACE_COUNT] =
    [CORAL, GOLD, SKY_BLUE, SEA_GREEN, MEDIUM_PURPLE, TURQUOISE];
const FACE_COUNT: usize = 6;
const FACE_ROUGHNESS: f32 = 0.55;
const FACE_SIDE: f32 = 1.0;

fn main() {
    fairy_dust::sprinkle_example()
        .with_brp_extras()
        .with_save_window_position()
        .with_studio_lighting()
        .with_ground_plane()
        .with_orbit_cam_preset(|_| {}, OrbitCamPreset::blender_like())
        .with_stable_transparency()
        .with_fold_controls()
        .with_camera_home()
        .pitch(HOME_PITCH)
        .yaw(HOME_YAW)
        .margin(HOME_MARGIN)
        .with_title_bar(
            TitleBar::new()
                .with_title(EXAMPLE_TITLE)
                .with_anchor(Anchor::TopLeft),
        )
        .with_description_panel(description_panel())
        .with_camera_control_panel()
        .add_systems(Startup, setup)
        .run();
}

fn description_panel() -> DescriptionPanel {
    DescriptionPanel::new(DESCRIPTION_TITLE)
        .with_fit_width()
        .lines(DESCRIPTION_LINES)
}

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let face_mesh = meshes.add(Rectangle::new(FACE_SIDE, FACE_SIDE));
    let [
        center_material,
        north_material,
        south_material,
        east_material,
        west_material,
        lid_material,
    ] = FACE_COLORS.map(|color| materials.add(face_material(color)));
    // Rotate the net so its fold axis (local +Z) points up (world +Y): the flat
    // net lies on the XZ ground and folds into a box that sits entirely above it.
    let center = spawn_face(
        &mut commands,
        face_mesh.clone(),
        center_material,
        Transform::from_xyz(0.0, GROUND_CLEARANCE, 0.0)
            .with_rotation(Quat::from_rotation_x(BOX_ROOT_ROTATION)),
    );
    let north = spawn_hinged_face(
        &mut commands,
        face_mesh.clone(),
        north_material,
        center,
        QUAD_BOTTOM_EDGE,
        QUAD_TOP_EDGE,
    );
    let south = spawn_hinged_face(
        &mut commands,
        face_mesh.clone(),
        south_material,
        center,
        QUAD_TOP_EDGE,
        QUAD_BOTTOM_EDGE,
    );
    let east = spawn_hinged_face(
        &mut commands,
        face_mesh.clone(),
        east_material,
        center,
        QUAD_LEFT_EDGE,
        QUAD_RIGHT_EDGE,
    );
    let west = spawn_hinged_face(
        &mut commands,
        face_mesh.clone(),
        west_material,
        center,
        QUAD_RIGHT_EDGE,
        QUAD_LEFT_EDGE,
    );
    // The lid hinges off north's free outer edge. Stage zero raises the lid;
    // stage one then raises north and carries the lid over the box.
    let lid = spawn_hinged_face(
        &mut commands,
        face_mesh,
        lid_material,
        north,
        QUAD_BOTTOM_EDGE,
        QUAD_TOP_EDGE,
    );
    let sequence = commands.spawn(FoldSequence::new(FOLD_SECONDS)).id();
    let authored = FoldSequenceBuilder::new(&mut commands, sequence)
        .stage([lid])
        .stage([north, south, east, west])
        .finish();
    assert!(
        authored.is_ok(),
        "box fold sequence authoring failed: {authored:?}",
    );
    // Frame the space the folded box occupies, not the unfolded cross: a hidden
    // unit cube resting on the ground at the box's final position. The faces
    // themselves are not home targets, so the startup snap ignores their spread-
    // out flat layout.
    commands.spawn((
        CameraHomeTarget,
        Mesh3d(meshes.add(Cuboid::from_size(Vec3::splat(FACE_SIDE)))),
        Transform::from_xyz(0.0, FACE_SIDE / 2.0 + GROUND_CLEARANCE, 0.0),
        Visibility::Hidden,
    ));
}

fn spawn_face(
    commands: &mut Commands,
    mesh: Handle<Mesh>,
    material: Handle<StandardMaterial>,
    transform: Transform,
) -> Entity {
    commands
        .spawn((
            Mesh3d(mesh),
            MeshMaterial3d(material),
            fixtures::quad_geometry(FACE_SIDE, FACE_SIDE),
            transform,
            GlobalTransform::from(transform),
        ))
        .id()
}

fn spawn_hinged_face(
    commands: &mut Commands,
    mesh: Handle<Mesh>,
    material: Handle<StandardMaterial>,
    parent: Entity,
    source_edge: Edge,
    target_edge: Edge,
) -> Entity {
    let entity = spawn_face(commands, mesh, material, Transform::default());
    let source_anchor = fixtures::quad_edge_anchor(source_edge);
    let target_anchor = fixtures::quad_edge_anchor(target_edge);
    assert!(
        source_anchor.is_some() && target_anchor.is_some(),
        "fixture edge constant must map to a quad anchor",
    );
    let (Some(source_anchor), Some(target_anchor)) = (source_anchor, target_anchor) else {
        return entity;
    };
    commands.entity(entity).insert((
        AnchoredTo::new(parent, source_anchor, target_anchor),
        AnchorPose::default(),
        FoldAngles {
            unfolded: 0.0,
            folded:   BOX_FOLD_ANGLE,
        },
        Hinge {
            edge:  source_edge,
            angle: 0.0,
        },
    ));
    entity
}

fn face_material(color: Srgba) -> StandardMaterial {
    StandardMaterial {
        base_color: Color::from(color),
        cull_mode: None,
        double_sided: true,
        perceptual_roughness: FACE_ROUGHNESS,
        ..default()
    }
}
