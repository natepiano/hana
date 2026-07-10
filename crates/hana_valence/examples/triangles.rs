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
use bevy_diegetic::DiegeticText;
use bevy_diegetic::Sidedness;
use bevy_kana::ToF32;
use bevy_lagrange::OrbitCamPreset;
use fairy_dust::Anchor;
use fairy_dust::CameraHomeTarget;
use fairy_dust::ControlActivation;
use fairy_dust::TitleBar;
use fairy_dust::TitleBarControl;
use fairy_dust::TitleBarSegment;
use hana_valence::Accordion;
use hana_valence::AnchorId;
use hana_valence::AnchorSystems;
use hana_valence::Edge;
use hana_valence::FoldPattern;
use hana_valence::Hinge;
use hana_valence::Member;
use hana_valence::MemberIndex;
use hana_valence::ResolvedAnchorGeometry;
use hana_valence::ResolvedAnchorOffset;
use hana_valence::TilingRule;
use hana_valence::apply_member_placements;
use hana_valence::assign_member_indices;
use hana_valence::hinge_to_pose;

#[path = "../fixtures.rs"]
#[allow(
    dead_code,
    reason = "shared geometry fixtures; this example uses a subset"
)]
mod fixtures;

// app
const EXAMPLE_TITLE: &str = "Triangles";

// title-bar chips. The fold/unfold/replay chips light while that motion runs; the
// mode chip is a segmented "T" control whose active word (Accordion or Wrap) marks
// the current fold style.
const FOLD_CONTROL: &str = "Space Fold";
const UNFOLD_CONTROL: &str = "Shift+Space Unfold";
const REPLAY_CONTROL: &str = "R Replay";
const MODE_KEY: &str = "T";
const ACCORDION_CONTROL: &str = "Accordion";
const WRAP_CONTROL: &str = "Wrap";

// animation
// Each crease folds a half-turn, so the folded strip collapses onto a single
// triangle footprint. Real folded paper still stacks cleanly because its
// thickness holds neighboring panels apart; `TILE_GAP` stands in for that
// thickness — `apply_crease_gap` pivots every crease about a line lifted half a
// gap off the tile face, so closed tiles land a full gap apart instead of
// coincident. Creases close one at a time down the strip.
const ACCORDION_LEAN: f32 = core::f32::consts::PI;
const CREASE_COUNT: f32 = 5.0;
const EVEN_PERIOD: usize = 2;
const FOLD_SECONDS: f32 = 2.5;
const MOUNTAIN_SIGN: f32 = 1.0;
const VALLEY_SIGN: f32 = -1.0;
// Ease closer than this to the target counts as settled, so the motion chips clear.
const MOTION_EPSILON: f32 = 1.0e-4;
const SMOOTHSTEP_DOUBLE: f32 = 2.0;
const SMOOTHSTEP_TRIPLE: f32 = 3.0;
// One step folds or unfolds a single crease; the strip has one crease per member.
const STEP_COUNT: i32 = 5;
// Face-to-face spacing between folded tiles. Must exceed two label offsets so the
// labels floating off facing tile surfaces keep their own clearance in the stack.
const TILE_GAP: f32 = 0.001;

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

#[derive(Component)]
struct TriangleTiling;

// Accordion clock. `step` is the target number of closed creases (0 flat, up to
// `STEP_COUNT` fully folded); `progress` eases toward `step / CREASE_COUNT` each
// frame. `Space` raises `step` one crease, `Shift+Space` lowers it, `R` snaps
// between flat and fully folded.
#[derive(Default, Resource)]
struct AccordionPlayback {
    progress: f32,
    step:     i32,
}

// Fold shape. Each tile rests flipped a half-turn from its neighbor, so a constant
// crease sign swings consecutive creases to opposite world sides (`Accordion`, the
// back-and-forth zigzag pleat), while an alternating crease sign cancels the rest
// flip so every crease folds the same world direction and the strip curls onto
// itself (`Wrap`). `T` toggles between them.
#[derive(Clone, Copy, Default, PartialEq, Resource)]
enum FoldStyle {
    #[default]
    Accordion,
    Wrap,
}

// Current fold motion, mirrored onto the title-bar chips. `animate_accordion`
// updates `direction` each frame from the sign of the remaining ease; `R` sets
// `replaying` until the strip settles. The fold/unfold chips stay dark during a
// replay so each of the three actions lights its own chip.
#[derive(Clone, Copy, Default, PartialEq, Resource)]
struct FoldMotion {
    direction: MotionDirection,
    replaying: bool,
}

#[derive(Clone, Copy, Default, PartialEq)]
enum MotionDirection {
    #[default]
    Idle,
    Folding,
    Unfolding,
}

impl FoldMotion {
    // A replay steers the same fold/unfold ease, so the manual chips stay dark while
    // `replaying` is set; only the replay chip lights.
    fn fold_activation(self) -> ControlActivation {
        Self::activation(self.direction == MotionDirection::Folding && !self.replaying)
    }

    fn unfold_activation(self) -> ControlActivation {
        Self::activation(self.direction == MotionDirection::Unfolding && !self.replaying)
    }

    const fn replay_activation(self) -> ControlActivation { Self::activation(self.replaying) }

    const fn activation(active: bool) -> ControlActivation {
        if active {
            ControlActivation::Active
        } else {
            ControlActivation::Inactive
        }
    }
}

impl FoldStyle {
    fn activation_for(self, style: Self) -> ControlActivation {
        if self == style {
            ControlActivation::Active
        } else {
            ControlActivation::Inactive
        }
    }
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
        .with_camera_home()
        .pitch(HOME_PITCH)
        .yaw(HOME_YAW)
        .margin(HOME_MARGIN)
        .with_title_bar(
            TitleBar::new()
                .with_title(EXAMPLE_TITLE)
                .with_anchor(Anchor::TopLeft)
                .control(FOLD_CONTROL)
                .control(UNFOLD_CONTROL)
                .control(REPLAY_CONTROL)
                .control(TitleBarControl::segmented(
                    MODE_KEY,
                    [
                        TitleBarSegment::new(ACCORDION_CONTROL, "Accordion"),
                        TitleBarSegment::new(WRAP_CONTROL, "Wrap"),
                    ],
                )),
        )
        .wire_chip_to_state::<FoldMotion, _>(FOLD_CONTROL, |motion| motion.fold_activation())
        .wire_chip_to_state::<FoldMotion, _>(UNFOLD_CONTROL, |motion| motion.unfold_activation())
        .wire_chip_to_state::<FoldMotion, _>(REPLAY_CONTROL, |motion| motion.replay_activation())
        .wire_chip_to_state::<FoldStyle, _>(ACCORDION_CONTROL, |style| {
            style.activation_for(FoldStyle::Accordion)
        })
        .wire_chip_to_state::<FoldStyle, _>(WRAP_CONTROL, |style| {
            style.activation_for(FoldStyle::Wrap)
        })
        .with_camera_control_panel()
        .with_shortcut(KeyCode::KeyR, toggle_accordion)
        .with_shortcut(KeyCode::KeyT, toggle_fold_style);
    // `DiegeticUiPlugin` (added by `sprinkle_example`) already registers the
    // shared anchor pipeline — the `AnchorSystems` chain, `assign_member_indices`,
    // `hinge_to_pose`, `resolve_anchors`, `ResolveDiagnostics`, and the member
    // observers. This example adds `apply_member_placements::<TriangleTiling>` to
    // seat the strip, then `animate_accordion` writes each member hinge angle
    // directly (in place of `drive_arrangement_hinges`) so the creases can close
    // one at a time, and `apply_crease_gap` writes each member's offset so folded
    // tiles stack a gap apart. Re-registering `hinge_to_pose` here is what
    // produced the per-frame "hinge overwrote an AnchorPose" spam.
    app.init_resource::<AccordionPlayback>()
        .init_resource::<FoldStyle>()
        .init_resource::<FoldMotion>()
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
                (animate_accordion, apply_crease_gap)
                    .chain()
                    .in_set(AnchorSystems::AnimatePose)
                    .before(hinge_to_pose),
            ),
        )
        .add_systems(Update, step_accordion)
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
        let number = FIRST_TILE_NUMBER + 1 + offset;
        let tile = spawn_member_tile(&mut commands, root);
        spawn_tile_visual(&mut commands, tile, triangle_mesh.clone(), material, number);
    }
}

fn animate_accordion(
    time: Res<Time>,
    style: Res<FoldStyle>,
    mut playback: ResMut<AccordionPlayback>,
    mut motion: ResMut<FoldMotion>,
    mut hinges: Query<(&MemberIndex, &mut Hinge), With<Member>>,
) {
    let goal = playback.step.to_f32() / CREASE_COUNT;
    let step = time.delta_secs() / FOLD_SECONDS;
    playback.progress += (goal - playback.progress).clamp(-step, step);
    let progress = playback.progress;
    for (index, mut hinge) in &mut hinges {
        hinge.angle = crease_angle(index.index, progress, *style);
    }
    // Mirror the current ease onto the title-bar chips. Touch the resource only on a
    // real transition so change detection fires exactly when a chip flips.
    let remaining = goal - progress;
    let direction = if remaining.abs() <= MOTION_EPSILON {
        MotionDirection::Idle
    } else if remaining > 0.0 {
        MotionDirection::Folding
    } else {
        MotionDirection::Unfolding
    };
    if motion.direction != direction {
        motion.direction = direction;
    }
    if direction == MotionDirection::Idle && motion.replaying {
        motion.replaying = false;
    }
}

// Hinge angle for the crease seating member `index`, swinging the tile from its
// rest flip by the eased fold fraction times the lean.
fn crease_angle(index: usize, progress: f32, style: FoldStyle) -> f32 {
    crease_fraction(index, progress)
        .mul_add(ACCORDION_LEAN * fold_sign(index, style), TRIANGLE_REST_FLIP)
}

// Eased closed-fraction of member `index`'s crease, `0` flat to `1` fully closed.
// Creases close in order down the strip: member 1's crease closes over the first
// `1 / CREASE_COUNT` of `progress`, member 2's over the next slice, and so on, so
// the fold builds a flat stack a triangle at a time.
fn crease_fraction(index: usize, progress: f32) -> f32 {
    let start = (index - 1).to_f32();
    let local = progress.mul_add(CREASE_COUNT, -start).clamp(0.0, 1.0);
    local * local * SMOOTHSTEP_DOUBLE.mul_add(-local, SMOOTHSTEP_TRIPLE)
}

// Sign of each crease's fold. Because every tile rests flipped a half-turn from its
// neighbor, a constant sign swings consecutive creases to opposite world sides (the
// back-and-forth zigzag pleat), while an alternating sign folds every crease the
// same world direction (the inward curl).
const fn fold_sign(index: usize, style: FoldStyle) -> f32 {
    match style {
        FoldStyle::Accordion => MOUNTAIN_SIGN,
        FoldStyle::Wrap if index.is_multiple_of(EVEN_PERIOD) => MOUNTAIN_SIGN,
        FoldStyle::Wrap => VALLEY_SIGN,
    }
}

// Pivot lift per crease, in half-gap units. Accordion tiles land one gap above
// their predecessor, while each wrap crease coils around every layer already
// inside it and must clear them all, so its lift grows with the member index.
fn gap_scale(index: usize, style: FoldStyle) -> f32 {
    match style {
        FoldStyle::Accordion => 1.0,
        FoldStyle::Wrap => index.to_f32(),
    }
}

// Lift each crease's pivot off the tile face so folded tiles stack a gap apart.
// Rotating about a line at `pivot` instead of in the face plane keeps the same
// rotation but adds the translation `pivot - swing * pivot`: zero while the strip
// is flat, a full gap along the parent's normal once the crease closes to a
// half-turn. Because the translation is derived from the crease's own swing, the
// tile always lands on the side it approached from.
fn apply_crease_gap(
    style: Res<FoldStyle>,
    mut tiles: Query<
        (
            &MemberIndex,
            &Hinge,
            &ResolvedAnchorGeometry,
            &mut ResolvedAnchorOffset,
        ),
        With<Member>,
    >,
) {
    for (index, hinge, geometry, mut offset) in &mut tiles {
        let Ok(axis) = hinge.edge.axis(geometry) else {
            continue;
        };
        let swing = Quat::from_axis_angle(*axis, hinge.angle - TRIANGLE_REST_FLIP);
        let lift = fold_sign(index.index, *style) * gap_scale(index.index, *style) * TILE_GAP / 2.0;
        let pivot = Vec3::Z * lift;
        offset.0 = pivot - swing * pivot;
    }
}

// `R` handler: snap the target between flat and fully folded so the strip replays
// the whole fold or unfold. `replaying` lights the R chip until the strip settles.
fn toggle_accordion(mut playback: ResMut<AccordionPlayback>, mut motion: ResMut<FoldMotion>) {
    playback.step = if playback.step >= STEP_COUNT {
        0
    } else {
        STEP_COUNT
    };
    motion.replaying = true;
}

// `T` handler: switch between the back-and-forth accordion and the inward wrap,
// snapping the strip flat so the new fold pattern reads from the first crease.
fn toggle_fold_style(mut style: ResMut<FoldStyle>, mut playback: ResMut<AccordionPlayback>) {
    *style = match *style {
        FoldStyle::Accordion => FoldStyle::Wrap,
        FoldStyle::Wrap => FoldStyle::Accordion,
    };
    playback.step = 0;
}

// `Space` closes one more crease; `Shift+Space` opens the last one. Each press
// nudges the target crease count and `animate_accordion` eases the strip to it, so
// the fold advances one triangle at a time in either direction.
fn step_accordion(keys: Res<ButtonInput<KeyCode>>, mut playback: ResMut<AccordionPlayback>) {
    if !keys.just_pressed(KeyCode::Space) {
        return;
    }
    let reverse = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);
    playback.step = if reverse {
        (playback.step - 1).max(0)
    } else {
        (playback.step + 1).min(STEP_COUNT)
    };
}

fn spawn_member_tile(commands: &mut Commands, arrangement: Entity) -> Entity {
    let entity = spawn_tile(commands, Transform::default());
    commands
        .entity(entity)
        .insert((Member { arrangement }, ResolvedAnchorOffset::default()));
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
