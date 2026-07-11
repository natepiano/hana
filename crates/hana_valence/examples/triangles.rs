//! Triangle tiling arrangement driven through `TilingRule`.

use bevy::color::Srgba;
use bevy::color::palettes::css::CORAL;
use bevy::color::palettes::css::GOLD;
use bevy::color::palettes::css::MEDIUM_PURPLE;
use bevy::color::palettes::css::SEA_GREEN;
use bevy::color::palettes::css::SKY_BLUE;
use bevy::color::palettes::css::TURQUOISE;
use bevy::ecs::schedule::ApplyDeferred;
use bevy::math::curve::easing::EaseFunction;
use bevy::prelude::*;
use bevy_diegetic::DiegeticText;
use bevy_diegetic::Sidedness;
use bevy_enhanced_input::prelude::*;
use bevy_kana::Keybindings;
use bevy_kana::ToF32;
use bevy_kana::action;
use bevy_kana::bind_action_system;
use bevy_kana::event;
use bevy_lagrange::OrbitCamPreset;
use fairy_dust::Anchor;
use fairy_dust::CameraHomeTarget;
use fairy_dust::ControlActivation;
use fairy_dust::DescriptionPanel;
use fairy_dust::TitleBar;
use fairy_dust::TitleBarControl;
use fairy_dust::TitleBarSegment;
use hana_valence::Accordion;
use hana_valence::AnchorId;
use hana_valence::AnchorSystems;
use hana_valence::Edge;
use hana_valence::FoldAngles;
use hana_valence::FoldFromArrangement;
use hana_valence::FoldPattern;
use hana_valence::FoldSequence;
use hana_valence::FoldSequenceState;
use hana_valence::FoldSystems;
use hana_valence::HingePivot;
use hana_valence::Member;
use hana_valence::MemberIndex;
use hana_valence::TilingRule;
use hana_valence::apply_member_placements;
use hana_valence::assign_member_indices;

#[path = "../fixtures.rs"]
#[allow(
    dead_code,
    reason = "shared geometry fixtures; this example uses a subset"
)]
mod fixtures;

// app
const EXAMPLE_TITLE: &str = "Triangles";

// title-bar chips
const MODE_KEY: &str = "T";
const ACCORDION_CONTROL: &str = "Accordion";
const WRAP_CONTROL: &str = "Wrap";

// animation
const ACCORDION_LEAN: f32 = core::f32::consts::PI;
const EVEN_PERIOD: usize = 2;
const MOUNTAIN_SIGN: f32 = 1.0;
const STEP_SECONDS: f32 = 0.5;
const VALLEY_SIGN: f32 = -1.0;
// Face-to-face spacing between folded tiles. Must exceed two label offsets so the
// labels floating off facing tile surfaces keep their own clearance in the stack.
const TILE_GAP: f32 = 0.001;

// teaching panel
const DESCRIPTION_TITLE: &str = "Arrangement-derived folding";
const DESCRIPTION_LINES: [&str; 3] = [
    "Arrangement order becomes one zero-based fold stage per triangle.",
    "Space and Shift+Space step one crease; P continues the remembered direction.",
    "T selects Accordion or Wrap; mid-fold selections wait until fully unfolded.",
];

// labels
const FIRST_TILE_NUMBER: usize = 1;
const LABEL_COLOR: Color = Color::BLACK;
const LABEL_SIZE: f32 = 0.3;
// Nudge each face label off the triangle surface so it does not z-fight the tile.
const LABEL_Z_OFFSET: f32 = 0.0005;

// camera home
const HOME_MARGIN: f32 = 0.45;
const HOME_PITCH: f32 = 0.76;
const HOME_YAW: f32 = 0.63;

// triangle
const TILE_COLORS: [Srgba; TILE_COUNT] =
    [CORAL, GOLD, SKY_BLUE, SEA_GREEN, MEDIUM_PURPLE, TURQUOISE];
const TILE_COUNT: usize = 6;
const TILE_ROUGHNESS: f32 = 0.55;
// The strip cascades downward in -Y and the accordion fold swings the tail tile
// deeper still; lift the whole strip so its lowest vertex clears the ground
// plane across the full fold cycle (deepest measured reach ~2.9 with margin).
const ROOT_LIFT: f32 = 3.3;
const TRIANGLE_HEIGHT: f32 = 0.866_025_4;
const TRIANGLE_REST_FLIP: f32 = core::f32::consts::PI;
const TRIANGLE_SIDE: f32 = 1.0;
const TRIANGLE_TWO_THIRDS: f32 = 2.0 / 3.0;

struct TriangleAlgorithmInputPlugin;

impl Plugin for TriangleAlgorithmInputPlugin {
    fn build(&self, app: &mut App) {
        app.add_input_context::<TriangleAlgorithmInput>()
            .add_systems(Startup, spawn_algorithm_input);
        bind_action_system!(app, ToggleAlgorithm, ToggleAlgorithmEvent, toggle_algorithm);
    }
}

#[derive(Component)]
struct TriangleAlgorithmInput;

action!(
    /// Selects the next triangle fold algorithm.
    ToggleAlgorithm
);
action!(
    /// Tracks Shift so the bare T binding is modifier-safe.
    AlgorithmShift
);
event!(
    /// Routes a fold-algorithm selection request into the example system.
    ToggleAlgorithmEvent
);

#[derive(Component)]
struct TriangleTiling;

#[derive(Clone, Copy, Default, Eq, PartialEq)]
enum FoldAlgorithm {
    #[default]
    Accordion,
    Wrap,
}

#[derive(Resource)]
struct AlgorithmSelection {
    selected_algorithm: FoldAlgorithm,
    active_algorithm:   FoldAlgorithm,
}

impl Default for AlgorithmSelection {
    fn default() -> Self {
        Self {
            selected_algorithm: FoldAlgorithm::Accordion,
            active_algorithm:   FoldAlgorithm::Accordion,
        }
    }
}

impl AlgorithmSelection {
    fn activation_for(&self, algorithm: FoldAlgorithm) -> ControlActivation {
        if self.selected_algorithm == algorithm {
            ControlActivation::Active
        } else {
            ControlActivation::Inactive
        }
    }
}

#[derive(Clone, Copy)]
struct AlgorithmMemberProfile {
    fold_angles: FoldAngles,
    hinge_pivot: HingePivot,
}

impl TilingRule for TriangleTiling {
    fn next_edge(&self, index: usize) -> (Edge, Edge) {
        let edge = fixtures::triangle_edge(index);
        (edge, edge)
    }

    fn edge_anchor(&self, edge: Edge) -> Option<AnchorId> { fixtures::triangle_edge_anchor(edge) }

    fn rest_delta(&self, _: usize) -> f32 { TRIANGLE_REST_FLIP }
}

fn main() {
    let app = fairy_dust::sprinkle_example()
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
                .with_anchor(Anchor::TopLeft)
                .control(TitleBarControl::segmented(
                    MODE_KEY,
                    [
                        TitleBarSegment::new(ACCORDION_CONTROL, "Accordion"),
                        TitleBarSegment::new(WRAP_CONTROL, "Wrap"),
                    ],
                )),
        )
        .wire_chip_to_state::<AlgorithmSelection, _>(ACCORDION_CONTROL, |selection| {
            selection.activation_for(FoldAlgorithm::Accordion)
        })
        .wire_chip_to_state::<AlgorithmSelection, _>(WRAP_CONTROL, |selection| {
            selection.activation_for(FoldAlgorithm::Wrap)
        })
        .with_description_panel(description_panel())
        .with_camera_control_panel()
        .add_plugins(TriangleAlgorithmInputPlugin);
    app.init_resource::<AlgorithmSelection>()
        .add_systems(
            PostUpdate,
            (
                (
                    ApplyDeferred,
                    apply_member_placements::<TriangleTiling>,
                    ApplyDeferred,
                )
                    .chain()
                    .after(assign_member_indices)
                    .before(AnchorSystems::AnimatePose),
                activate_selected_algorithm
                    .in_set(AnchorSystems::AnimatePose)
                    .after(FoldSystems::Advance)
                    .before(FoldSystems::Actuate),
            ),
        )
        .add_systems(Startup, setup)
        .run();
}

fn description_panel() -> DescriptionPanel {
    DescriptionPanel::new(DESCRIPTION_TITLE)
        .with_fit_width()
        .lines(DESCRIPTION_LINES)
}

fn spawn_algorithm_input(mut commands: Commands) {
    commands.spawn((
        TriangleAlgorithmInput,
        Actions::<TriangleAlgorithmInput>::spawn(SpawnWith(spawn_algorithm_actions)),
    ));
}

fn spawn_algorithm_actions(spawner: &mut ActionSpawner<TriangleAlgorithmInput>) {
    let keybindings = Keybindings::new::<AlgorithmShift>(spawner, ActionSettings::default());
    keybindings.spawn_key::<ToggleAlgorithm>(spawner, KeyCode::KeyT);
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
    let root = spawn_tile(&mut commands, Transform::from_xyz(0.0, ROOT_LIFT, 0.0));
    commands.entity(root).insert((
        Accordion {
            fold:    0.0,
            lean:    ACCORDION_LEAN,
            pattern: FoldPattern::Accordion,
        },
        TriangleTiling,
    ));
    spawn_tile_visual(
        &mut commands,
        root,
        triangle_mesh.clone(),
        root_material,
        FIRST_TILE_NUMBER,
    );
    for (offset, material) in [
        gold_material,
        sky_material,
        green_material,
        purple_material,
        turquoise_material,
    ]
    .into_iter()
    .enumerate()
    {
        let member_index = offset + 1;
        let number = FIRST_TILE_NUMBER + member_index;
        let tile = spawn_member_tile(&mut commands, root, member_index, FoldAlgorithm::Accordion);
        spawn_tile_visual(&mut commands, tile, triangle_mesh.clone(), material, number);
    }
    commands.entity(root).insert((
        FoldSequence {
            easing: EaseFunction::SmoothStep,
            ..FoldSequence::new(STEP_SECONDS)
        },
        FoldFromArrangement::new(root),
    ));
}

fn toggle_algorithm(mut selection: ResMut<AlgorithmSelection>) {
    selection.selected_algorithm = match selection.selected_algorithm {
        FoldAlgorithm::Accordion => FoldAlgorithm::Wrap,
        FoldAlgorithm::Wrap => FoldAlgorithm::Accordion,
    };
}

fn activate_selected_algorithm(
    mut selection: ResMut<AlgorithmSelection>,
    sequences: Query<(Entity, &FoldSequenceState), With<TriangleTiling>>,
    mut members: Query<(&Member, &MemberIndex, &mut FoldAngles, &mut HingePivot)>,
) {
    if selection.selected_algorithm == selection.active_algorithm {
        return;
    }
    let Ok((sequence, state)) = sequences.single() else {
        return;
    };
    if !state.is_ready() || state.position() != 0.0 {
        return;
    }
    for (member, index, mut fold_angles, mut hinge_pivot) in &mut members {
        if member.arrangement != sequence {
            continue;
        }
        let profile = algorithm_member_profile(index.index, selection.selected_algorithm);
        *fold_angles = profile.fold_angles;
        *hinge_pivot = profile.hinge_pivot;
    }
    selection.active_algorithm = selection.selected_algorithm;
}

fn algorithm_member_profile(index: usize, algorithm: FoldAlgorithm) -> AlgorithmMemberProfile {
    let sign = fold_sign(index, algorithm);
    let signed_lean = ACCORDION_LEAN * sign;
    let pivot_offset = Vec3::Z * (sign * pivot_scale(index, algorithm) * TILE_GAP / 2.0);
    AlgorithmMemberProfile {
        fold_angles: FoldAngles {
            unfolded: TRIANGLE_REST_FLIP,
            folded:   TRIANGLE_REST_FLIP + signed_lean,
        },
        hinge_pivot: HingePivot {
            offset:          pivot_offset,
            reference_angle: TRIANGLE_REST_FLIP,
        },
    }
}

const fn fold_sign(index: usize, algorithm: FoldAlgorithm) -> f32 {
    match algorithm {
        FoldAlgorithm::Accordion => MOUNTAIN_SIGN,
        FoldAlgorithm::Wrap if index.is_multiple_of(EVEN_PERIOD) => MOUNTAIN_SIGN,
        FoldAlgorithm::Wrap => VALLEY_SIGN,
    }
}

fn pivot_scale(index: usize, algorithm: FoldAlgorithm) -> f32 {
    match algorithm {
        FoldAlgorithm::Accordion => 1.0,
        FoldAlgorithm::Wrap => index.to_f32(),
    }
}

fn spawn_member_tile(
    commands: &mut Commands,
    arrangement: Entity,
    index: usize,
    algorithm: FoldAlgorithm,
) -> Entity {
    let entity = spawn_tile(commands, Transform::default());
    let profile = algorithm_member_profile(index, algorithm);
    commands.entity(entity).insert((
        Member { arrangement },
        profile.fold_angles,
        profile.hinge_pivot,
    ));
    entity
}

// The anchored tile entity carries only the fold geometry and pose the pipeline
// drives; `resolve_anchors` owns its `Transform`, so anything static must ride a
// child. The visible mesh and labels live on `spawn_tile_visual`'s child instead.
fn spawn_tile(commands: &mut Commands, transform: Transform) -> Entity {
    commands
        .spawn((
            fixtures::triangle_geometry(),
            transform,
            GlobalTransform::from(transform),
            Visibility::default(),
        ))
        .id()
}

// Spawn the visible triangle plus its front/back number labels. The mesh sits on
// the tile with no offset, so the strip lies flat and coplanar when unfolded.
// Even-numbered tiles rest flipped a half-turn (each member adds the PI rest
// delta), so their local +Z faces away from the viewer — place `{n}F` on whichever
// local face points at the camera at rest so all "F" read on one side.
fn spawn_tile_visual(
    commands: &mut Commands,
    tile: Entity,
    mesh: Handle<Mesh>,
    material: Handle<StandardMaterial>,
    number: usize,
) {
    let front_on_plus_z = !number.is_multiple_of(EVEN_PERIOD);
    commands.entity(tile).with_children(|parent| {
        parent
            .spawn((Mesh3d(mesh), MeshMaterial3d(material), CameraHomeTarget))
            .with_children(|visual| {
                visual.spawn(face_label(format!("{number}F"), front_on_plus_z));
                visual.spawn(face_label(format!("{number}B"), !front_on_plus_z));
            });
    });
}

fn face_label(text: String, on_plus_z: bool) -> impl Bundle {
    let offset = if on_plus_z {
        LABEL_Z_OFFSET
    } else {
        -LABEL_Z_OFFSET
    };
    let facing = if on_plus_z {
        Quat::IDENTITY
    } else {
        Quat::from_rotation_y(core::f32::consts::PI)
    };
    DiegeticText::world(text)
        .size(LABEL_SIZE)
        .color(LABEL_COLOR)
        .sidedness(Sidedness::FrontOnly)
        .transform(Transform::from_xyz(0.0, 0.0, offset).with_rotation(facing))
        .build()
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
