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
use fairy_dust::TitleBar;
use hana_valence::AnchorPose;
use hana_valence::AnchorSystems;
use hana_valence::AnchoredTo;
use hana_valence::Edge;
use hana_valence::Hinge;
use hana_valence::hinge_to_pose;

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
const EXAMPLE_TITLE: &str = "Box Net";

// animation
const BOX_FOLD_ANGLE: f32 = -core::f32::consts::FRAC_PI_2;
const FOLD_SECONDS: f32 = 2.5;
const LID_FOLD_PORTION: f32 = 0.3;
const SMOOTHSTEP_DOUBLE: f32 = 2.0;
const SMOOTHSTEP_TRIPLE: f32 = 3.0;

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

#[derive(Component)]
struct FoldTarget {
    angle: f32,
    phase: FoldPhase,
}

#[derive(Clone, Copy)]
enum FoldPhase {
    Lid,
    Box,
}

// Directed fold clock. `progress` runs 0 (flat net) to 1 (folded box); each
// frame it steps toward `folding`'s target so pressing `R` reverses the fold in
// place instead of restarting from scratch.
#[derive(Resource)]
struct FoldPlayback {
    progress: f32,
    folding:  bool,
}

impl Default for FoldPlayback {
    fn default() -> Self {
        Self {
            progress: 0.0,
            folding:  true,
        }
    }
}

fn main() {
    let app = fairy_dust::sprinkle_example()
        .with_brp_extras()
        .with_save_window_position()
        .with_studio_lighting()
        .with_ground_plane()
        .with_orbit_cam_preset(|_| {}, OrbitCamPreset::blender_like())
        .with_camera_home()
        .pitch(HOME_PITCH)
        .yaw(HOME_YAW)
        .margin(HOME_MARGIN)
        .with_title_bar(
            TitleBar::new()
                .with_title(EXAMPLE_TITLE)
                .with_anchor(Anchor::TopLeft)
                .control("R Replay"),
        )
        .with_camera_control_panel()
        .with_shortcut(KeyCode::KeyR, toggle_box_fold);
    // `DiegeticUiPlugin` (added by `sprinkle_example`) already registers the
    // anchor pipeline — the `AnchorSystems` chain, `hinge_to_pose`,
    // `resolve_anchors`, and `ResolveDiagnostics`. This example only adds its
    // own hinge driver, ordered before the shared `hinge_to_pose`.
    app.init_resource::<FoldPlayback>()
        .add_systems(
            PostUpdate,
            drive_box_hinges
                .in_set(AnchorSystems::AnimatePose)
                .before(hinge_to_pose),
        )
        .add_systems(Startup, setup)
        .run();
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
        FoldPhase::Box,
    );
    spawn_hinged_face(
        &mut commands,
        face_mesh.clone(),
        south_material,
        center,
        QUAD_TOP_EDGE,
        QUAD_BOTTOM_EDGE,
        FoldPhase::Box,
    );
    spawn_hinged_face(
        &mut commands,
        face_mesh.clone(),
        east_material,
        center,
        QUAD_LEFT_EDGE,
        QUAD_RIGHT_EDGE,
        FoldPhase::Box,
    );
    spawn_hinged_face(
        &mut commands,
        face_mesh.clone(),
        west_material,
        center,
        QUAD_RIGHT_EDGE,
        QUAD_LEFT_EDGE,
        FoldPhase::Box,
    );
    // The lid hinges off north's free outer edge. Its `FoldPhase::Lid` motion
    // raises it first; `FoldPhase::Box` then raises north and carries the lid
    // over the top of the box.
    spawn_hinged_face(
        &mut commands,
        face_mesh,
        lid_material,
        north,
        QUAD_BOTTOM_EDGE,
        QUAD_TOP_EDGE,
        FoldPhase::Lid,
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

fn drive_box_hinges(
    time: Res<Time>,
    mut playback: ResMut<FoldPlayback>,
    mut hinges: Query<(&FoldTarget, &mut Hinge)>,
) {
    let goal = if playback.folding { 1.0 } else { 0.0 };
    let step = time.delta_secs() / FOLD_SECONDS;
    playback.progress += (goal - playback.progress).clamp(-step, step);
    let progress = playback.progress;
    for (target, mut hinge) in &mut hinges {
        let progress = match target.phase {
            FoldPhase::Lid => progress / LID_FOLD_PORTION,
            FoldPhase::Box => (progress - LID_FOLD_PORTION) / (1.0 - LID_FOLD_PORTION),
        }
        .clamp(0.0, 1.0);
        let eased = progress * progress * SMOOTHSTEP_DOUBLE.mul_add(-progress, SMOOTHSTEP_TRIPLE);
        hinge.angle = target.angle * eased;
    }
}

// `R` handler: reverse the fold direction. Because `drive_box_hinges` chases the
// current target, this both replays (net -> box) and rewinds (box -> net).
fn toggle_box_fold(mut playback: ResMut<FoldPlayback>) { playback.folding = !playback.folding; }

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
    phase: FoldPhase,
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
        FoldTarget {
            angle: BOX_FOLD_ANGLE,
            phase,
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
