//! Five hinged panels unfold from a visible fixed mount with staggered tweens.

use bevy::camera::primitives::Aabb;
use bevy::color::Srgba;
use bevy::color::palettes::css::CORAL;
use bevy::color::palettes::css::GOLD;
use bevy::color::palettes::css::MEDIUM_PURPLE;
use bevy::color::palettes::css::SEA_GREEN;
use bevy::color::palettes::css::SILVER;
use bevy::color::palettes::css::SKY_BLUE;
use bevy::color::palettes::css::TURQUOISE;
use bevy::prelude::*;
use bevy_diegetic::DiegeticText;
use bevy_diegetic::PanelSystems;
use bevy_diegetic::Sidedness;
use bevy_kana::ToF32;
use bevy_kana::ToUsize;
use bevy_lagrange::OrbitCamPreset;
use bevy_tween::BevyTweenRegisterSystems;
use bevy_tween::DefaultTweenPlugins;
use bevy_tween::TweenSystemSet;
use bevy_tween::bevy_time_runner::TimeRunner;
use bevy_tween::combinator::forward;
use bevy_tween::combinator::sequence;
use bevy_tween::combinator::tween;
use bevy_tween::prelude::AnimationBuilderExt;
use bevy_tween::prelude::Duration;
use bevy_tween::prelude::EaseKind;
use bevy_tween::prelude::IntoTarget;
use bevy_tween::prelude::Repeat;
use bevy_tween::prelude::RepeatStyle;
use bevy_tween::tween::component_tween_system;
use fairy_dust::Anchor;
use fairy_dust::CameraHomeTarget;
use fairy_dust::ControlActivation;
use fairy_dust::DescriptionPanel;
use fairy_dust::TitleBar;
use fairy_dust::TitleChipActivation;
use hana_valence::AnchorId;
use hana_valence::AnchorPose;
use hana_valence::AnchorPoseLens;
use hana_valence::AnchorSystems;
use hana_valence::AnchoredTo;
use hana_valence::Hinge;
use hana_valence::HingeAngleLens;
use hana_valence::ResolvedAnchorGeometry;
use hana_valence::ResolvedAnchorOffset;

#[path = "../fixtures.rs"]
#[allow(
    dead_code,
    reason = "shared geometry fixtures; this example uses a subset"
)]
mod fixtures;

use fixtures::QUAD_LEFT_EDGE;

// app
const EXAMPLE_TITLE: &str = "Staggered Unfold";
const FOLD_CONTROL: &str = "Space Fold";
const UNFOLD_CONTROL: &str = "Shift+Space Unfold";
const PAUSE_CONTROL: &str = "P Pause";

// animation
const CASCADE_SPAN: Duration = Duration::from_millis(800);
const FLAT_PAUSE: Duration = Duration::from_millis(900);
const FOLD_SECONDS: f32 = 0.8;
const FULL_FOLD_ANGLE: f32 = core::f32::consts::PI;
const FOLDED_PAUSE: Duration = Duration::from_millis(700);
const HALF_FOLD_ANGLE: f32 = core::f32::consts::FRAC_PI_2;
// Panel 1 stops parallel to the fixed mount. The remaining signs alternate for
// the accordion motion; ±PI close to the same panel plane, forming a stack.
const PANEL_FOLD_ANGLES: [f32; PANEL_COUNT] = [
    HALF_FOLD_ANGLE,
    -FULL_FOLD_ANGLE,
    FULL_FOLD_ANGLE,
    -FULL_FOLD_ANGLE,
    FULL_FOLD_ANGLE,
];
const PANEL_START_DELAYS: [Duration; PANEL_COUNT] = [
    Duration::from_millis(0),
    Duration::from_millis(200),
    Duration::from_millis(400),
    Duration::from_millis(600),
    Duration::from_millis(800),
];
const UNFOLD_DURATION: Duration = Duration::from_millis(1_100);

// camera home
const HOME_MARGIN: f32 = 0.43;
const HOME_OFFSET_PX: Vec2 = Vec2::new(-55.0, 55.0);
const HOME_PITCH: f32 = 0.36;
const HOME_TARGET_NAME: &str = "Fully unfolded chain bounds";
const HOME_TARGET_POSITION: Vec3 =
    Vec3::new(-(MOUNT_WIDTH + BASE_WIDTH) / 4.0, MOUNT_HEIGHT / 2.0, 0.0);
const HOME_TARGET_SIZE: Vec3 = Vec3::new(
    PANEL_SPAN + f32::midpoint(MOUNT_WIDTH, BASE_WIDTH),
    MOUNT_HEIGHT,
    BASE_DEPTH,
);
const HOME_YAW: f32 = 0.37;

// description panel
const DESCRIPTION_LINES: [&str; 4] = [
    "Gold mount: authored world transform for the chain.",
    "AnchoredTo links panels 1–5 at shared side edges.",
    "HingeAngleLens staggers each panel's bevy_tween track.",
    "Ping-pong playback refolds from panel 5 to panel 1.",
];
const DESCRIPTION_TITLE: &str = "Anchor Chain";

// labels
const FIRST_PANEL_NUMBER: usize = 1;
const FIXED_ROOT_LABEL: &str = "FIXED ROOT";
const FIXED_ROOT_LABEL_OFFSET: Vec3 =
    Vec3::new(0.0, MOUNT_HEIGHT / 2.0 + 0.25, MOUNT_DEPTH / 2.0 + 0.03);
const FIXED_ROOT_LABEL_SIZE: f32 = 0.18;
const LABEL_COLOR: Color = Color::BLACK;
const LABEL_SIZE: f32 = 0.32;
const LABEL_Z_OFFSET: f32 = PANEL_THICKNESS / 2.0 + 0.006;

// mount
const BASE_DEPTH: f32 = 0.8;
const BASE_HEIGHT: f32 = 0.14;
const BASE_WIDTH: f32 = 0.9;
const MOUNT_DEPTH: f32 = 0.42;
const MOUNT_HEIGHT: f32 = 1.8;
const MOUNT_POSITION: Vec3 = Vec3::new(
    -PANEL_SPAN / 2.0 - MOUNT_WIDTH / 2.0,
    MOUNT_HEIGHT / 2.0,
    0.0,
);
const METAL_ROUGHNESS: f32 = 0.24;
const MOUNT_WIDTH: f32 = 0.28;

// panels
const HINGE_HEIGHT: f32 = PANEL_HEIGHT + 0.12;
const HINGE_RADIUS: f32 = 0.065;
const HINGE_AXIS_OFFSET: f32 = PANEL_THICKNESS / 2.0 + HINGE_RADIUS;
const PANEL_COLORS: [Srgba; PANEL_COUNT] = [CORAL, SKY_BLUE, SEA_GREEN, MEDIUM_PURPLE, TURQUOISE];
const PANEL_COUNT: usize = 5;
const PANEL_HEIGHT: f32 = 1.2;
const PANEL_ROUGHNESS: f32 = 0.42;
const PANEL_SOURCE_ANCHOR: AnchorId = AnchorId::EdgeMid(3);
const PANEL_SPAN: f32 = 7.25;
const PANEL_TARGET_ANCHOR: AnchorId = AnchorId::EdgeMid(1);
const PANEL_THICKNESS: f32 = 0.08;
const PANEL_WIDTH: f32 = 1.45;

// scene
const GROUND_SIZE: f32 = 11.0;

#[derive(Clone, Copy)]
enum Face {
    Front,
    Back,
}

struct PanelAssets {
    hinge_material: Handle<StandardMaterial>,
    hinge_mesh:     Handle<Mesh>,
    panel_mesh:     Handle<Mesh>,
}

#[derive(Clone, Copy)]
struct PanelTween {
    angle: f32,
    delay: Duration,
}

#[derive(Component)]
struct UnfoldAnimation;

#[derive(Component)]
struct PanelIndex(usize);

#[derive(Default, Resource)]
struct FoldStepPlayback {
    manual:   bool,
    progress: f32,
    target:   usize,
}

#[derive(Clone, Copy, Default, Eq, PartialEq, Resource)]
enum Playback {
    #[default]
    Playing,
    Paused,
}

impl TitleChipActivation for Playback {
    fn activation(&self) -> ControlActivation {
        match self {
            Self::Playing => ControlActivation::Inactive,
            Self::Paused => ControlActivation::Active,
        }
    }
}

fn main() {
    // `bevy_diegetic::DiegeticUiPlugin` is registered automatically by
    // `fairy_dust::sprinkle_example`.
    let mut app = fairy_dust::sprinkle_example()
        .with_brp_extras()
        .with_save_window_position()
        .with_studio_lighting()
        .with_ground_plane()
        .size(GROUND_SIZE)
        .with_orbit_cam_preset(|_| {}, OrbitCamPreset::blender_like())
        .with_stable_transparency()
        .with_camera_home()
        .pitch(HOME_PITCH)
        .yaw(HOME_YAW)
        .margin(HOME_MARGIN)
        .offset_px(HOME_OFFSET_PX)
        .with_title_bar(
            TitleBar::new()
                .with_title(EXAMPLE_TITLE)
                .with_anchor(Anchor::TopLeft)
                .control(FOLD_CONTROL)
                .control(UNFOLD_CONTROL)
                .control(PAUSE_CONTROL),
        )
        .wire_chip_to_activation::<Playback>(PAUSE_CONTROL)
        .with_description_panel(description_panel())
        .with_camera_control_panel()
        .add_plugins(DefaultTweenPlugins::<()>::in_schedule(PostUpdate))
        .init_resource::<Playback>()
        .init_resource::<FoldStepPlayback>()
        .add_systems(Update, step_fold)
        .with_shortcut(KeyCode::KeyP, toggle_pause);
    // `DiegeticUiPlugin` already registers `hinge_to_pose`. Ordering
    // `TweenSystemSet::ApplyTween` before `PanelSystems::AnimateAnchorPose`
    // puts each `HingeAngleLens` write before that shared system without
    // registering a second copy of it.
    app.app_mut().configure_sets(
        PostUpdate,
        TweenSystemSet::ApplyTween
            .in_set(AnchorSystems::AnimatePose)
            .before(PanelSystems::AnimateAnchorPose),
    );
    app.app_mut().add_tween_systems(
        PostUpdate,
        (
            component_tween_system::<HingeAngleLens>(),
            component_tween_system::<AnchorPoseLens>(),
        ),
    );
    app.app_mut().add_systems(
        PostUpdate,
        apply_fold_steps
            .in_set(AnchorSystems::AnimatePose)
            .after(TweenSystemSet::ApplyTween)
            .before(apply_hinge_pivot),
    );
    app.app_mut().add_systems(
        PostUpdate,
        apply_hinge_pivot
            .in_set(AnchorSystems::AnimatePose)
            .before(PanelSystems::AnimateAnchorPose),
    );
    app.add_systems(Startup, setup).run();
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
    let panel_assets = PanelAssets {
        hinge_material: materials.add(metal_material(SILVER)),
        hinge_mesh:     meshes.add(Cylinder::new(HINGE_RADIUS, HINGE_HEIGHT)),
        panel_mesh:     meshes.add(Cuboid::new(PANEL_WIDTH, PANEL_HEIGHT, PANEL_THICKNESS)),
    };
    let panel_materials = PANEL_COLORS.map(|color| materials.add(panel_material(color)));

    spawn_home_target(&mut commands);
    let mut parent = spawn_fixed_mount(&mut commands, &mut meshes, &mut materials);
    for (index, ((angle, delay), material)) in PANEL_FOLD_ANGLES
        .into_iter()
        .zip(PANEL_START_DELAYS)
        .zip(panel_materials)
        .enumerate()
    {
        parent = spawn_hinged_panel(
            &mut commands,
            &panel_assets,
            material,
            parent,
            PanelTween { angle, delay },
            index + FIRST_PANEL_NUMBER,
        );
    }
}

// Every `AnchoredTo` chain ends at an entity whose `Transform` is authored
// instead of resolved. The gold mount is that reference: it exposes anchor
// geometry, but carries neither `AnchoredTo` nor `Hinge`.
fn spawn_fixed_mount(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
) -> Entity {
    let material = materials.add(metal_material(GOLD));
    let mount = commands
        .spawn((
            Mesh3d(meshes.add(Cuboid::new(MOUNT_WIDTH, MOUNT_HEIGHT, MOUNT_DEPTH))),
            MeshMaterial3d(material.clone()),
            fixtures::quad_geometry(MOUNT_WIDTH, MOUNT_HEIGHT),
            Transform::from_translation(MOUNT_POSITION),
            GlobalTransform::from_translation(MOUNT_POSITION),
        ))
        .id();
    commands.entity(mount).with_children(|root| {
        root.spawn((
            Mesh3d(meshes.add(Cuboid::new(BASE_WIDTH, BASE_HEIGHT, BASE_DEPTH))),
            MeshMaterial3d(material),
            Transform::from_xyz(0.0, BASE_HEIGHT / 2.0 - MOUNT_POSITION.y, 0.0),
        ));
        root.spawn(fixed_root_label());
    });
    mount
}

fn spawn_hinged_panel(
    commands: &mut Commands,
    panel_assets: &PanelAssets,
    material: Handle<StandardMaterial>,
    parent: Entity,
    panel_tween: PanelTween,
    number: usize,
) -> Entity {
    let entity = commands
        .spawn((
            Mesh3d(panel_assets.panel_mesh.clone()),
            MeshMaterial3d(material),
            fixtures::quad_geometry(PANEL_WIDTH, PANEL_HEIGHT),
            PanelIndex(number - FIRST_PANEL_NUMBER),
            Transform::default(),
            GlobalTransform::default(),
            AnchoredTo::new(parent, PANEL_SOURCE_ANCHOR, PANEL_TARGET_ANCHOR),
            AnchorPose::default(),
            ResolvedAnchorOffset::default(),
            Hinge {
                edge:  QUAD_LEFT_EDGE,
                angle: panel_tween.angle,
            },
        ))
        .id();
    spawn_panel_details(commands, panel_assets, entity, number);
    spawn_unfold_tween(commands, entity, panel_tween);
    entity
}

// The panel meshes begin folded, so their live AABBs cannot define the startup
// home view. This proxy matches the fully unfolded panels plus the fixed mount
// and base; `CameraHomeTarget` gives startup and `H Home` the same stable region.
fn spawn_home_target(commands: &mut Commands) {
    let half_size = HOME_TARGET_SIZE / 2.0;
    commands.spawn((
        Name::new(HOME_TARGET_NAME),
        CameraHomeTarget,
        Aabb::from_min_max(
            HOME_TARGET_POSITION - half_size,
            HOME_TARGET_POSITION + half_size,
        ),
        Transform::default(),
    ));
}

fn spawn_panel_details(
    commands: &mut Commands,
    panel_assets: &PanelAssets,
    panel: Entity,
    number: usize,
) {
    commands.entity(panel).with_children(|visual| {
        visual.spawn((
            Mesh3d(panel_assets.hinge_mesh.clone()),
            MeshMaterial3d(panel_assets.hinge_material.clone()),
            Transform::from_xyz(-PANEL_WIDTH / 2.0, 0.0, HINGE_AXIS_OFFSET),
        ));
        visual.spawn(panel_label(number, Face::Front));
        visual.spawn(panel_label(number, Face::Back));
    });
}

fn spawn_unfold_tween(commands: &mut Commands, entity: Entity, panel_tween: PanelTween) {
    let folded_delay = FOLDED_PAUSE + panel_tween.delay;
    let flat_delay = FLAT_PAUSE + CASCADE_SPAN.saturating_sub(panel_tween.delay);
    let target = entity.into_target();
    let mut animation = commands.animation();
    animation.entity_commands().insert(UnfoldAnimation);
    animation
        .repeat(Repeat::Infinitely)
        .repeat_style(RepeatStyle::PingPong)
        .insert(sequence((
            forward(folded_delay),
            tween(
                UNFOLD_DURATION,
                EaseKind::SmootherStep,
                target.with(HingeAngleLens {
                    start: panel_tween.angle,
                    end:   0.0,
                }),
            ),
            forward(flat_delay),
        )));
}

fn toggle_pause(
    mut playback: ResMut<Playback>,
    mut animations: Query<&mut TimeRunner, With<UnfoldAnimation>>,
) {
    *playback = match *playback {
        Playback::Playing => Playback::Paused,
        Playback::Paused => Playback::Playing,
    };
    let paused = *playback == Playback::Paused;
    for mut time_runner in &mut animations {
        time_runner.set_paused(paused);
    }
}

// `Space` and `Shift+Space` switch from the looping tween to the stepped clock.
// The current hinge angles seed the first step, so a keypress never snaps the
// panels to an unrelated stage of the fold.
fn step_fold(
    keys: Res<ButtonInput<KeyCode>>,
    mut playback: ResMut<FoldStepPlayback>,
    panels: Query<(&PanelIndex, &Hinge)>,
    mut animations: Query<&mut TimeRunner, With<UnfoldAnimation>>,
) {
    if !keys.just_pressed(KeyCode::Space) {
        return;
    }
    if !playback.manual {
        playback.progress = panels
            .iter()
            .map(|(index, hinge)| {
                let fraction = (hinge.angle / PANEL_FOLD_ANGLES[index.0]).clamp(0.0, 1.0);
                index.0.to_f32() + fraction
            })
            .fold(0.0, f32::max);
        playback.manual = true;
        for mut animation in &mut animations {
            animation.set_paused(true);
        }
    }
    let current_step = playback.progress.round().to_usize();
    let reverse = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);
    playback.target = if reverse {
        current_step.saturating_sub(1)
    } else {
        (current_step + 1).min(PANEL_COUNT)
    };
}

fn apply_fold_steps(
    time: Res<Time>,
    playback: Res<Playback>,
    mut steps: ResMut<FoldStepPlayback>,
    mut panels: Query<(&PanelIndex, &mut Hinge)>,
) {
    if !steps.manual || *playback == Playback::Paused {
        return;
    }
    let target = steps.target.to_f32();
    let step = time.delta_secs() / FOLD_SECONDS;
    steps.progress += (target - steps.progress).clamp(-step, step);
    for (index, mut hinge) in &mut panels {
        let fraction = (steps.progress - index.0.to_f32()).clamp(0.0, 1.0);
        hinge.angle = PANEL_FOLD_ANGLES[index.0] * fraction;
    }
}

// The cylinder axis sits just beyond the panel face. `ResolvedAnchorOffset`
// keeps that axis fixed while its panel rotates, so the visual pole and the
// hinge pivot remain the same line throughout the accordion motion.
fn apply_hinge_pivot(
    mut panels: Query<(&Hinge, &ResolvedAnchorGeometry, &mut ResolvedAnchorOffset)>,
) {
    let pivot = Vec3::Z * HINGE_AXIS_OFFSET;
    for (hinge, geometry, mut offset) in &mut panels {
        let Ok(axis) = hinge.edge.axis(geometry) else {
            continue;
        };
        let swing = Quat::from_axis_angle(*axis, hinge.angle);
        offset.0 = pivot - swing * pivot;
    }
}

fn panel_label(number: usize, face: Face) -> impl Bundle {
    let (offset, facing) = match face {
        Face::Front => (LABEL_Z_OFFSET, Quat::IDENTITY),
        Face::Back => (
            -LABEL_Z_OFFSET,
            Quat::from_rotation_y(core::f32::consts::PI),
        ),
    };
    DiegeticText::world(number.to_string())
        .size(LABEL_SIZE)
        .color(LABEL_COLOR)
        .sidedness(Sidedness::FrontOnly)
        .transform(Transform::from_xyz(0.0, 0.0, offset).with_rotation(facing))
        .build()
}

fn fixed_root_label() -> impl Bundle {
    DiegeticText::world(FIXED_ROOT_LABEL)
        .size(FIXED_ROOT_LABEL_SIZE)
        .color(Color::from(GOLD))
        .sidedness(Sidedness::FrontOnly)
        .transform(Transform::from_translation(FIXED_ROOT_LABEL_OFFSET))
        .build()
}

fn panel_material(color: Srgba) -> StandardMaterial {
    StandardMaterial {
        base_color: Color::from(color),
        perceptual_roughness: PANEL_ROUGHNESS,
        ..default()
    }
}

fn metal_material(color: Srgba) -> StandardMaterial {
    StandardMaterial {
        base_color: Color::from(color),
        metallic: 1.0,
        perceptual_roughness: METAL_ROUGHNESS,
        ..default()
    }
}
