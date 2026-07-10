//! Staggered hinge tweens across five anchored quads.

use std::time::Duration;

use bevy::color::Srgba;
use bevy::color::palettes::css::CORAL;
use bevy::color::palettes::css::GOLD;
use bevy::color::palettes::css::MEDIUM_PURPLE;
use bevy::color::palettes::css::SEA_GREEN;
use bevy::color::palettes::css::SKY_BLUE;
use bevy::prelude::*;
use bevy::transform::TransformSystems;
use bevy_lagrange::OrbitCamPreset;
use bevy_tween::BevyTweenRegisterSystems;
use bevy_tween::DefaultTweenPlugins;
use bevy_tween::TweenSystemSet;
use bevy_tween::combinator::forward;
use bevy_tween::combinator::sequence;
use bevy_tween::combinator::tween;
use bevy_tween::prelude::AnimationBuilderExt;
use bevy_tween::prelude::EaseKind;
use bevy_tween::prelude::IntoTarget;
use bevy_tween::tween::component_tween_system;
use fairy_dust::Anchor;
use fairy_dust::CameraHomeTarget;
use fairy_dust::TitleBar;
use hana_valence::AnchorId;
use hana_valence::AnchorPose;
use hana_valence::AnchorPoseLens;
use hana_valence::AnchorSystems;
use hana_valence::AnchoredTo;
use hana_valence::Hinge;
use hana_valence::HingeAngleLens;
use hana_valence::ResolveDiagnostics;
use hana_valence::hinge_to_pose;
use hana_valence::resolve_anchors;

#[path = "../fixtures.rs"]
#[allow(
    dead_code,
    reason = "shared geometry fixtures; this example uses a subset"
)]
mod fixtures;

use fixtures::QUAD_TOP_EDGE;

// app
const EXAMPLE_TITLE: &str = "Staggered Unfold";

// animation
const HINGE_START_ANGLES: [f32; HINGED_TILE_COUNT] = [
    core::f32::consts::FRAC_PI_2,
    -core::f32::consts::FRAC_PI_2,
    core::f32::consts::FRAC_PI_2,
    -core::f32::consts::FRAC_PI_2,
];
const HINGE_START_DELAYS: [Duration; HINGED_TILE_COUNT] = [
    Duration::from_millis(0),
    Duration::from_millis(280),
    Duration::from_millis(560),
    Duration::from_millis(840),
];
const HINGED_TILE_COUNT: usize = TILE_COUNT - 1;
const UNFOLD_DURATION: Duration = Duration::from_millis(900);

// camera home
const HOME_MARGIN: f32 = 0.45;
const HOME_PITCH: f32 = -0.98;
const HOME_YAW: f32 = 0.0;

// quad
const QUAD_HEIGHT: f32 = 1.0;
const QUAD_WIDTH: f32 = 2.0;
const TILE_COUNT: usize = 5;
const TILE_ROUGHNESS: f32 = 0.55;

// scene
const TILE_COLORS: [Srgba; TILE_COUNT] = [CORAL, GOLD, SKY_BLUE, SEA_GREEN, MEDIUM_PURPLE];

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
        .add_plugins(DefaultTweenPlugins::<()>::in_schedule(PostUpdate))
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
    app.app_mut().configure_sets(
        PostUpdate,
        TweenSystemSet::ApplyTween.in_set(AnchorSystems::AnimatePose),
    );
    app.app_mut().add_tween_systems(
        PostUpdate,
        (
            component_tween_system::<HingeAngleLens>(),
            component_tween_system::<AnchorPoseLens>(),
        ),
    );
    app.add_systems(
        PostUpdate,
        (
            hinge_to_pose
                .in_set(AnchorSystems::AnimatePose)
                .after(TweenSystemSet::ApplyTween),
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
    let quad_mesh = meshes.add(Rectangle::new(QUAD_WIDTH, QUAD_HEIGHT));
    let [
        root_surface,
        gold_surface,
        sky_surface,
        green_surface,
        purple_surface,
    ] = TILE_COLORS.map(|color| materials.add(tile_material(color)));
    let child_materials = [gold_surface, sky_surface, green_surface, purple_surface];

    let root = spawn_tile(
        &mut commands,
        quad_mesh.clone(),
        root_surface,
        Transform::default(),
    );
    let mut parent = root;
    for ((start_angle, start_delay), material) in HINGE_START_ANGLES
        .into_iter()
        .zip(HINGE_START_DELAYS)
        .zip(child_materials)
    {
        let entity = spawn_hinged_tile(
            &mut commands,
            quad_mesh.clone(),
            material,
            parent,
            start_angle,
        );
        spawn_unfold_tween(&mut commands, entity, start_angle, start_delay);
        parent = entity;
    }
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
            fixtures::quad_geometry(QUAD_WIDTH, QUAD_HEIGHT),
            transform,
            GlobalTransform::from(transform),
        ))
        .id()
}

fn spawn_hinged_tile(
    commands: &mut Commands,
    mesh: Handle<Mesh>,
    material: Handle<StandardMaterial>,
    parent: Entity,
    start_angle: f32,
) -> Entity {
    let entity = spawn_tile(commands, mesh, material, Transform::default());
    commands.entity(entity).insert((
        AnchoredTo::new(parent, AnchorId::Vertex(0), AnchorId::Vertex(1)),
        AnchorPose::default(),
        Hinge {
            edge:  QUAD_TOP_EDGE,
            angle: start_angle,
        },
    ));
    entity
}

fn spawn_unfold_tween(commands: &mut Commands, entity: Entity, start_angle: f32, delay: Duration) {
    let target = entity.into_target();
    commands.animation().insert(sequence((
        forward(delay),
        tween(
            UNFOLD_DURATION,
            EaseKind::SmootherStep,
            target.with(HingeAngleLens {
                start: start_angle,
                end:   0.0,
            }),
        ),
    )));
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
