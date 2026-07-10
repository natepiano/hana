//! Six quads folded from a cross net into a box.

use bevy::color::Srgba;
use bevy::color::palettes::css::CORAL;
use bevy::color::palettes::css::GOLD;
use bevy::color::palettes::css::MEDIUM_PURPLE;
use bevy::color::palettes::css::SEA_GREEN;
use bevy::color::palettes::css::SKY_BLUE;
use bevy::color::palettes::css::TURQUOISE;
use bevy::prelude::*;
use bevy::transform::TransformSystems;
use bevy_lagrange::OrbitCamPreset;
use fairy_dust::Anchor;
use fairy_dust::CameraHomeTarget;
use fairy_dust::TitleBar;
use hana_valence::AnchorPose;
use hana_valence::AnchorSystems;
use hana_valence::AnchoredTo;
use hana_valence::Edge;
use hana_valence::Hinge;
use hana_valence::ResolveDiagnostics;
use hana_valence::hinge_to_pose;
use hana_valence::resolve_anchors;

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
const SMOOTHSTEP_DOUBLE: f32 = 2.0;
const SMOOTHSTEP_TRIPLE: f32 = 3.0;

// camera home
const HOME_MARGIN: f32 = 0.4;
const HOME_PITCH: f32 = -0.87;
const HOME_YAW: f32 = 0.87;

// quad
const FACE_COLORS: [Srgba; FACE_COUNT] =
    [CORAL, GOLD, SKY_BLUE, SEA_GREEN, MEDIUM_PURPLE, TURQUOISE];
const FACE_COUNT: usize = 6;
const FACE_ROUGHNESS: f32 = 0.55;
const FACE_SIDE: f32 = 1.0;

#[derive(Component)]
struct FoldTarget {
    angle: f32,
}

fn main() {
    let mut app = fairy_dust::sprinkle_example()
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
                .with_anchor(Anchor::TopLeft),
        )
        .with_camera_control_panel()
        .init_resource::<ResolveDiagnostics>();
    app.app_mut().configure_sets(
        PostUpdate,
        (
            AnchorSystems::FillGeometry,
            AnchorSystems::AnimatePose,
            AnchorSystems::Resolve,
        )
            .chain()
            .before(TransformSystems::Propagate),
    );
    app.add_systems(
        PostUpdate,
        (
            drive_box_hinges.in_set(AnchorSystems::AnimatePose),
            hinge_to_pose
                .in_set(AnchorSystems::AnimatePose)
                .after(drive_box_hinges),
            resolve_anchors.in_set(AnchorSystems::Resolve),
        ),
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
    let center = spawn_face(
        &mut commands,
        face_mesh.clone(),
        center_material,
        Transform::default(),
    );
    let north = spawn_hinged_face(
        &mut commands,
        face_mesh.clone(),
        north_material,
        center,
        QUAD_BOTTOM_EDGE,
        QUAD_TOP_EDGE,
    );
    spawn_hinged_face(
        &mut commands,
        face_mesh.clone(),
        south_material,
        center,
        QUAD_TOP_EDGE,
        QUAD_BOTTOM_EDGE,
    );
    spawn_hinged_face(
        &mut commands,
        face_mesh.clone(),
        east_material,
        center,
        QUAD_LEFT_EDGE,
        QUAD_RIGHT_EDGE,
    );
    spawn_hinged_face(
        &mut commands,
        face_mesh.clone(),
        west_material,
        center,
        QUAD_RIGHT_EDGE,
        QUAD_LEFT_EDGE,
    );
    spawn_hinged_face(
        &mut commands,
        face_mesh,
        lid_material,
        north,
        QUAD_TOP_EDGE,
        QUAD_TOP_EDGE,
    );
}

fn drive_box_hinges(time: Res<Time>, mut hinges: Query<(&FoldTarget, &mut Hinge)>) {
    let progress = (time.elapsed_secs() / FOLD_SECONDS).clamp(0.0, 1.0);
    let eased = progress * progress * SMOOTHSTEP_DOUBLE.mul_add(-progress, SMOOTHSTEP_TRIPLE);
    for (target, mut hinge) in &mut hinges {
        hinge.angle = target.angle * eased;
    }
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
            CameraHomeTarget,
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
        FoldTarget {
            angle: BOX_FOLD_ANGLE,
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
