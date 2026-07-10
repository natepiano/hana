//! Triangle tiling arrangement driven through `TilingRule`.

use bevy::color::Srgba;
use bevy::color::palettes::css::CORAL;
use bevy::color::palettes::css::GOLD;
use bevy::color::palettes::css::MEDIUM_PURPLE;
use bevy::color::palettes::css::SEA_GREEN;
use bevy::color::palettes::css::SKY_BLUE;
use bevy::color::palettes::css::TURQUOISE;
use bevy::ecs::schedule::ApplyDeferred;
use bevy::prelude::*;
use bevy::transform::TransformSystems;
use bevy_lagrange::OrbitCamPreset;
use fairy_dust::Anchor;
use fairy_dust::CameraHomeTarget;
use fairy_dust::TitleBar;
use hana_valence::Accordion;
use hana_valence::AnchorId;
use hana_valence::AnchorSystems;
use hana_valence::Edge;
use hana_valence::FoldPattern;
use hana_valence::Member;
use hana_valence::ResolveDiagnostics;
use hana_valence::TilingRule;
use hana_valence::apply_member_placements;
use hana_valence::assign_member_indices;
use hana_valence::drive_arrangement_hinges;
use hana_valence::hinge_to_pose;
use hana_valence::on_member_added;
use hana_valence::on_member_removed;
use hana_valence::resolve_anchors;

#[path = "../fixtures.rs"]
#[allow(
    dead_code,
    reason = "shared geometry fixtures; this example uses a subset"
)]
mod fixtures;

// app
const EXAMPLE_TITLE: &str = "Triangle Accordion";

// animation
const ACCORDION_LEAN: f32 = core::f32::consts::FRAC_PI_3;
const FOLD_SPEED: f32 = 0.8;
const HALF_PHASE: f32 = 0.5;
const PHASE_OFFSET: f32 = 0.5;

// camera home
const HOME_MARGIN: f32 = 0.45;
const HOME_PITCH: f32 = -0.76;
const HOME_YAW: f32 = 0.63;

// triangle
const TILE_COLORS: [Srgba; TILE_COUNT] =
    [CORAL, GOLD, SKY_BLUE, SEA_GREEN, MEDIUM_PURPLE, TURQUOISE];
const TILE_COUNT: usize = 6;
const TILE_ROUGHNESS: f32 = 0.55;
const TRIANGLE_HEIGHT: f32 = 0.866_025_4;
const TRIANGLE_REST_FLIP: f32 = core::f32::consts::PI;
const TRIANGLE_SIDE: f32 = 1.0;
const TRIANGLE_TWO_THIRDS: f32 = 2.0 / 3.0;

#[derive(Component)]
struct TriangleTiling;

impl TilingRule for TriangleTiling {
    fn next_edge(&self, index: usize) -> (Edge, Edge) {
        let edge = fixtures::triangle_edge(index);
        (edge, edge)
    }

    fn edge_anchor(&self, edge: Edge) -> Option<AnchorId> { fixtures::triangle_edge_anchor(edge) }

    fn rest_delta(&self, _: usize) -> f32 { TRIANGLE_REST_FLIP }
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
        .init_resource::<ResolveDiagnostics>()
        .add_observer(on_member_added)
        .add_observer(on_member_removed);
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
            (
                assign_member_indices,
                ApplyDeferred,
                apply_member_placements::<TriangleTiling>,
                ApplyDeferred,
            )
                .chain()
                .after(AnchorSystems::FillGeometry)
                .before(AnchorSystems::AnimatePose),
            animate_accordion.in_set(AnchorSystems::AnimatePose),
            drive_arrangement_hinges::<TriangleTiling>
                .in_set(AnchorSystems::AnimatePose)
                .after(animate_accordion),
            hinge_to_pose
                .in_set(AnchorSystems::AnimatePose)
                .after(drive_arrangement_hinges::<TriangleTiling>),
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
    let triangle_mesh = meshes.add(triangle_mesh());
    let [
        root_material,
        gold_material,
        sky_material,
        green_material,
        purple_material,
        turquoise_material,
    ] = TILE_COLORS.map(|color| materials.add(tile_material(color)));
    let root = spawn_tile(
        &mut commands,
        triangle_mesh.clone(),
        root_material,
        Transform::default(),
    );
    commands.entity(root).insert((
        Accordion {
            fold:    0.0,
            lean:    ACCORDION_LEAN,
            pattern: FoldPattern::Accordion,
        },
        TriangleTiling,
    ));
    for material in [
        gold_material,
        sky_material,
        green_material,
        purple_material,
        turquoise_material,
    ] {
        spawn_member_tile(&mut commands, triangle_mesh.clone(), material, root);
    }
}

fn animate_accordion(time: Res<Time>, mut accordions: Query<&mut Accordion, With<TriangleTiling>>) {
    let fold = time
        .elapsed_secs()
        .mul_add(FOLD_SPEED, 0.0)
        .sin()
        .mul_add(HALF_PHASE, PHASE_OFFSET);
    for mut accordion in &mut accordions {
        accordion.fold = fold;
    }
}

fn spawn_member_tile(
    commands: &mut Commands,
    mesh: Handle<Mesh>,
    material: Handle<StandardMaterial>,
    arrangement: Entity,
) -> Entity {
    let entity = spawn_tile(commands, mesh, material, Transform::default());
    commands.entity(entity).insert(Member { arrangement });
    entity
}

fn spawn_tile(
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
            fixtures::triangle_geometry(),
            transform,
            GlobalTransform::from(transform),
        ))
        .id()
}

fn triangle_mesh() -> Triangle2d {
    let half_side = TRIANGLE_SIDE / 2.0;
    Triangle2d::new(
        Vec2::new(0.0, TRIANGLE_HEIGHT * TRIANGLE_TWO_THIRDS),
        Vec2::new(half_side, -TRIANGLE_HEIGHT / 3.0),
        Vec2::new(-half_side, -TRIANGLE_HEIGHT / 3.0),
    )
}

fn tile_material(color: Srgba) -> StandardMaterial {
    StandardMaterial {
        base_color: Color::from(color),
        cull_mode: None,
        double_sided: true,
        perceptual_roughness: TILE_ROUGHNESS,
        ..default()
    }
}
