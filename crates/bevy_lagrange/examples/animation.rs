//! Demonstrates three distinct ways to drive an `OrbitCam`, explained by the
//! three boxes in the lower-left panel:
//!
//! - **Manual (M)** writes `OrbitCam` fields every frame for a continuous orbit loop — input
//!   disabled, smoothing zeroed.
//! - **`PlayAnimation` (P)** hands the camera a `VecDeque<CameraMove>`; each move eases over its
//!   own duration, then advances to the next.
//! - **`AnimateToFit` (A)** declaratively frames a target entity — here the cube wearing its
//!   "`AnimateToFit` target" name on each face.
//!
//! Two cubes sit as a centered pair on the ground: the home cube and the
//! `AnimateToFit` target. Both are `CameraHomeTarget`s, so `H` frames their union;
//! `A` flies in to frame the `AnimateToFit` target alone.
//!
//! `AnimateToFit` and Home both fit a target, so they would be
//! indistinguishable by animation source alone. The `A AnimateToFit` chip is
//! wired with `fairy_dust`'s `wire_chip_to_fit_target`, which matches on the
//! framed `target` entity, so pressing `A` and `H` light only their own chips.
//!
//! Controls:
//!   M - Toggle manual orbit animation on/off
//!   P - `PlayAnimation` 5-step sequence
//!   A - `AnimateToFit` the labeled target cube
//!   H - Return to the camera home pose

use std::f32::consts::TAU;
use std::time::Duration;

use bevy::color::LinearRgba;
use bevy::math::curve::easing::EaseFunction;
use bevy::prelude::*;
use bevy_diegetic::AlignX;
use bevy_diegetic::AlignY;
use bevy_diegetic::Border;
use bevy_diegetic::CornerRadius;
use bevy_diegetic::DiegeticPanel;
use bevy_diegetic::El;
use bevy_diegetic::Fit;
use bevy_diegetic::GlyphShadowMode;
use bevy_diegetic::LayoutBuilder;
use bevy_diegetic::LayoutTree;
use bevy_diegetic::Padding;
use bevy_diegetic::PanelBuildError;
use bevy_diegetic::Px;
use bevy_diegetic::Sizing;
use bevy_diegetic::TextAlign;
use bevy_diegetic::TextStyle;
use bevy_diegetic::Unit;
use bevy_diegetic::default_panel_material;
use bevy_lagrange::AnimateToFit;
use bevy_lagrange::AnimationBegin;
use bevy_lagrange::AnimationEnd;
use bevy_lagrange::AnimationSource;
use bevy_lagrange::CameraInputDisabled;
use bevy_lagrange::CameraMove;
use bevy_lagrange::OrbitCam;
use bevy_lagrange::OrbitCamPreset;
use bevy_lagrange::PlayAnimation;
use fairy_dust::Anchor;
use fairy_dust::CameraHomeTarget;
use fairy_dust::ControlActivation;
use fairy_dust::DEFAULT_PANEL_BACKGROUND;
use fairy_dust::EXAMPLE_CUBE_SIZE;
use fairy_dust::Face;
use fairy_dust::LABEL_SIZE;
use fairy_dust::TITLE_SIZE;
use fairy_dust::TitleBar;
use fairy_dust::cube_face_transform;
use fairy_dust::example_cube_on_ground;

const EXAMPLE_TITLE: &str = "Animation";

fn main() {
    fairy_dust::sprinkle_example()
        .with_brp_extras()
        .with_save_window_position()
        .with_studio_lighting()
        .key_light_illuminance(KEY_LIGHT_ILLUMINANCE)
        .with_ground_plane()
        .with_orbit_cam_preset(|_| {}, OrbitCamPreset::blender_like())
        .with_camera_home()
        .yaw(HOME_YAW)
        .pitch(HOME_PITCH)
        .margin(HOME_MARGIN)
        .with_title_bar(
            TitleBar::new()
                .with_title(EXAMPLE_TITLE)
                .with_anchor(Anchor::TopLeft)
                .control(MANUAL_CONTROL)
                .control(PLAY_CONTROL)
                .control(FIT_CONTROL),
        )
        .wire_chip_to_state::<ManualAnimationState, _>(MANUAL_CONTROL, |state| {
            state.mode.control_activation()
        })
        .wire_chip_to_events_filtered::<AnimationBegin, AnimationEnd, _, _>(
            PLAY_CONTROL,
            |event| event.source == AnimationSource::PlayAnimation,
            |event| event.source == AnimationSource::PlayAnimation,
        )
        .wire_chip_to_fit_target::<FitTarget>(FIT_CONTROL)
        .with_camera_control_panel()
        .init_resource::<ManualAnimationState>()
        .add_systems(
            Startup,
            (spawn_target, spawn_fit_target, spawn_explainer_panel),
        )
        // M / A / P run through Fairy Dust's shortcut binding, which fires each
        // only when no modifier is held — so the `Ctrl+Shift+A` home-gizmo chord
        // no longer also triggers the bare-`A` AnimateToFit. `H` is bound by
        // Fairy Dust's camera home; `stop_manual_on_animation_begin` lets manual
        // orbit yield to that fit (and to A / P) without the example reading `H`.
        .with_shortcut(KeyCode::KeyM, toggle_manual)
        .with_shortcut(KeyCode::KeyA, animate_to_fit)
        .with_shortcut(KeyCode::KeyP, play_animation)
        .add_observer(stop_manual_on_animation_begin)
        .add_systems(Update, manual_animate)
        .run();
}

// ═════════════════════════════════════════════════════════════════════════════
// CAMERA DRIVERS — three ways to drive one OrbitCam: manual writes, PlayAnimation,
// and AnimateToFit.
// ═════════════════════════════════════════════════════════════════════════════
//
// How it works: `main` wires one HUD chip per mechanism and binds M / A / P
// through Fairy Dust's shortcut API. `manual_animate` is the only per-frame
// `Update` system; the rest are one-shot handlers and an observer:
//   - `M` (`toggle_manual`) flips `ManualAnimationState`. While active, `manual_animate` writes
//     `OrbitCam` targets every frame with camera input disabled and smoothing zeroed, so the writes
//     apply with no easing lag.
//   - `P` (`play_animation`) triggers `PlayAnimation`, handing the camera a `VecDeque<CameraMove>`
//     built from `PLAY_ANIMATION_STEPS`; bevy_lagrange interpolates it move by move.
//   - `A` (`animate_to_fit`) triggers `AnimateToFit` on the `FitTarget` cube.
//   - `stop_manual_on_animation_begin` observes `AnimationBegin` and leaves manual mode whenever
//     any animation starts — A, P, or Fairy Dust's `H` home fit — so manual writes never fight it.
//     `H` itself is bound by `fairy_dust`'s camera home, not this example.

// HUD chip strings
const FIT_CONTROL: &str = "A AnimateToFit";
const MANUAL_CONTROL: &str = "M Manual Orbit";
const PLAY_CONTROL: &str = "P Play Animation";

// manual orbit
const MANUAL_MODE_SMOOTHNESS_ACTIVE: f32 = 0.0;
const MANUAL_MODE_SMOOTHNESS_INACTIVE: f32 = 0.8;
const MANUAL_ORBIT_PITCH_AMPLITUDE: f32 = TAU * 0.1;
const MANUAL_ORBIT_RADIUS_BASE: f32 = 4.0;
const MANUAL_ORBIT_RADIUS_DELTA: f32 = 2.0;
const MANUAL_ORBIT_RADIUS_FREQUENCY: f32 = 2.0;
const MANUAL_ORBIT_YAW_RADIANS_PER_SECOND: f32 = TAU / 24.0;

// animate-to-fit
const ANIMATE_TO_FIT_DURATION: Duration = Duration::from_millis(1200);
const ANIMATE_TO_FIT_MARGIN: f32 = 0.15;
const ANIMATE_TO_FIT_PITCH: f32 = TAU / 12.0;
const ANIMATE_TO_FIT_YAW: f32 = TAU / 8.0;

// camera home
const HOME_MARGIN: f32 = 0.5;
const HOME_PITCH: f32 = ANIMATE_TO_FIT_PITCH;
const HOME_YAW: f32 = ANIMATE_TO_FIT_YAW;

/// One step of the `P` sequence; `play_animation` maps each to a `CameraMove::ToOrbit`.
#[derive(Clone, Copy)]
struct OrbitAnimationStep {
    duration: Duration,
    easing:   EaseFunction,
    pitch:    f32,
    radius:   f32,
    yaw:      f32,
}

const PLAY_ANIMATION_FOCUS: Vec3 =
    Vec3::new(0.0, example_cube_on_ground(CUBE_GROUND_CLEARANCE).y, 0.0);
const PLAY_ANIMATION_STEPS: [OrbitAnimationStep; 5] = [
    OrbitAnimationStep {
        duration: Duration::from_millis(800),
        easing:   EaseFunction::CubicInOut,
        pitch:    0.2,
        radius:   4.0,
        yaw:      1.5,
    },
    OrbitAnimationStep {
        duration: Duration::from_millis(1200),
        easing:   EaseFunction::CubicIn,
        pitch:    1.3,
        radius:   20.0,
        yaw:      2.5,
    },
    OrbitAnimationStep {
        duration: Duration::from_millis(1200),
        easing:   EaseFunction::SineInOut,
        pitch:    0.6,
        radius:   14.0,
        yaw:      4.5,
    },
    OrbitAnimationStep {
        duration: Duration::from_secs(1),
        easing:   EaseFunction::CubicIn,
        pitch:    0.1,
        radius:   2.0,
        yaw:      5.5,
    },
    OrbitAnimationStep {
        duration: Duration::from_millis(1200),
        easing:   EaseFunction::BounceOut,
        pitch:    0.3,
        radius:   8.0,
        yaw:      0.0,
    },
];

#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum ManualAnimationMode {
    #[default]
    Inactive,
    Active,
}

impl ManualAnimationMode {
    const fn control_activation(self) -> ControlActivation {
        match self {
            Self::Inactive => ControlActivation::Inactive,
            Self::Active => ControlActivation::Active,
        }
    }
}

#[derive(Resource, Default)]
struct ManualAnimationState {
    mode: ManualAnimationMode,
}

/// Marker for the cube the `A` key's `AnimateToFit` frames. `wire_chip_to_fit_target`
/// keys the `A AnimateToFit` chip on fits whose `target` carries this marker.
#[derive(Component)]
struct FitTarget;

/// `M` — toggles manual orbit. Turning it on disables camera input and zeroes
/// smoothing so `manual_animate`'s per-frame writes apply with no easing lag;
/// turning it off restores both via `stop_manual`.
fn toggle_manual(
    mut commands: Commands,
    mut manual: ResMut<ManualAnimationState>,
    mut orbit_cam_query: Query<(Entity, &mut OrbitCam)>,
) {
    let Ok((camera, mut orbit_cam)) = orbit_cam_query.single_mut() else {
        return;
    };

    if manual.mode == ManualAnimationMode::Active {
        stop_manual(&mut commands, &mut manual, camera, &mut orbit_cam);
    } else {
        manual.mode = ManualAnimationMode::Active;
        commands.entity(camera).insert(CameraInputDisabled);
        orbit_cam.orbit.set_damping(MANUAL_MODE_SMOOTHNESS_ACTIVE);
        orbit_cam.zoom.set_damping(MANUAL_MODE_SMOOTHNESS_ACTIVE);
        orbit_cam.pan.set_damping(MANUAL_MODE_SMOOTHNESS_ACTIVE);
        if let (Some(yaw), Some(pitch)) = (orbit_cam.yaw, orbit_cam.pitch) {
            orbit_cam.target_yaw = yaw;
            orbit_cam.target_pitch = pitch;
        }
    }
}

/// `A` — frames the `FitTarget` cube with `AnimateToFit`. Manual orbit, if
/// active, yields through `stop_manual_on_animation_begin`.
fn animate_to_fit(
    mut commands: Commands,
    camera_query: Query<Entity, With<OrbitCam>>,
    fit_target_query: Query<Entity, With<FitTarget>>,
) {
    let Ok(camera) = camera_query.single() else {
        return;
    };
    let Ok(fit_target) = fit_target_query.single() else {
        return;
    };
    commands.trigger(
        AnimateToFit::new(camera, fit_target)
            .yaw(ANIMATE_TO_FIT_YAW)
            .pitch(ANIMATE_TO_FIT_PITCH)
            .margin(ANIMATE_TO_FIT_MARGIN)
            .duration(ANIMATE_TO_FIT_DURATION),
    );
}

/// `P` — plays the five-step `PLAY_ANIMATION_STEPS` sequence as one queued
/// `PlayAnimation`. Manual orbit yields through `stop_manual_on_animation_begin`.
fn play_animation(mut commands: Commands, camera_query: Query<Entity, With<OrbitCam>>) {
    let Ok(camera) = camera_query.single() else {
        return;
    };
    let moves = PLAY_ANIMATION_STEPS.map(|step| CameraMove::ToOrbit {
        focus:    PLAY_ANIMATION_FOCUS,
        yaw:      step.yaw,
        pitch:    step.pitch,
        radius:   step.radius,
        duration: step.duration,
        easing:   step.easing,
    });

    commands.trigger(PlayAnimation::new(camera, moves));
}

/// Leaves manual orbit when any camera animation starts, so the per-frame
/// manual writes never fight a triggered fly. This is what handles `H`: Fairy
/// Dust's camera home triggers the home fit, whose `AnimationBegin` lands here
/// — the example never reads the reserved `H` key itself.
fn stop_manual_on_animation_begin(
    _begin: On<AnimationBegin>,
    mut commands: Commands,
    mut manual: ResMut<ManualAnimationState>,
    mut orbit_cam_query: Query<(Entity, &mut OrbitCam)>,
) {
    if manual.mode != ManualAnimationMode::Active {
        return;
    }
    let Ok((camera, mut orbit_cam)) = orbit_cam_query.single_mut() else {
        return;
    };
    stop_manual(&mut commands, &mut manual, camera, &mut orbit_cam);
}

/// Per-frame manual animation; only runs when the resource flag is active.
fn manual_animate(
    time: Res<Time>,
    manual: Res<ManualAnimationState>,
    mut query: Query<&mut OrbitCam>,
) {
    if manual.mode != ManualAnimationMode::Active {
        return;
    }
    for mut orbit_cam in &mut query {
        orbit_cam.target_yaw =
            MANUAL_ORBIT_YAW_RADIANS_PER_SECOND.mul_add(time.delta_secs(), orbit_cam.target_yaw);
        orbit_cam.target_pitch = time.elapsed_secs_wrapped().sin() * MANUAL_ORBIT_PITCH_AMPLITUDE;
        orbit_cam.radius = Some(
            (((time.elapsed_secs_wrapped() * MANUAL_ORBIT_RADIUS_FREQUENCY).cos() + 1.0) * 0.5)
                .mul_add(MANUAL_ORBIT_RADIUS_DELTA, MANUAL_ORBIT_RADIUS_BASE),
        );
        orbit_cam.force_update();
    }
}

/// Leaves manual mode: re-enables camera input and restores smoothing so the
/// next triggered animation eases normally.
fn stop_manual(
    commands: &mut Commands,
    manual: &mut ManualAnimationState,
    camera: Entity,
    orbit_cam: &mut OrbitCam,
) {
    if manual.mode == ManualAnimationMode::Active {
        manual.mode = ManualAnimationMode::Inactive;
        commands.entity(camera).remove::<CameraInputDisabled>();
        orbit_cam.orbit.set_damping(MANUAL_MODE_SMOOTHNESS_INACTIVE);
        orbit_cam.zoom.set_damping(MANUAL_MODE_SMOOTHNESS_INACTIVE);
        orbit_cam.pan.set_damping(MANUAL_MODE_SMOOTHNESS_INACTIVE);
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// TARGET CUBES — the two cubes the camera frames, each wearing its name on its
// faces in emissive text.
// ═════════════════════════════════════════════════════════════════════════════

/// Key light dimmed well below the studio default (`13_500` lux) so the emissive
/// face text reads against the lit cubes instead of being washed out.
const KEY_LIGHT_ILLUMINANCE: f32 = 2_500.0;

// Both cubes use the canonical `example_cube_on_ground` launch height and sit
// mirrored across the origin (`±CUBE_OFFSET_X`) so the pair reads as a centered
// group on the ground plane with a gap between them. Both are
// `CameraHomeTarget`s, so `H` frames their union; `A` frames only the
// AnimateToFit target.
const CUBE_SIZE: f32 = EXAMPLE_CUBE_SIZE;
const CUBE_OFFSET_X: f32 = 1.5;
/// Lift off the ground plane, matching the other examples' canonical clearance.
const CUBE_GROUND_CLEARANCE: f32 = 0.1;
// Each cube wears its name on its faces via a transparent cube-face
// `DiegeticPanel` whose `text_material` is strongly emissive — the same
// example-level recipe as `focus_bounds`/`follow_target`, so no bevy_diegetic
// change is needed. Panel font sizes are in millimeters (the cube is 1 m).
const FACE_LABEL_PANEL_SIZE: f32 = CUBE_SIZE * 0.88;
const FACE_LABEL_TEXT_SIZE: f32 = 88.0;
const FACE_LABEL_PADDING: f32 = 0.06;
/// Over-bright base color of the face text; the emissive material multiplies
/// it. Pushed past 1.0 so it clamps to a full, punchy white on an SDR camera.
const FACE_LABEL_COLOR: Color = Color::linear_rgb(2.0, 2.0, 2.2);
/// How hard the face text glows — the emissive color is the base times this.
const FACE_LABEL_EMISSIVE_BOOST: f32 = 6.0;

const TARGET_COLOR: Color = Color::srgb(0.8, 0.7, 0.6);
const TARGET_LABEL: &str = "Just a cube";
const TARGET_TRANSLATION: Vec3 = Vec3::new(
    -CUBE_OFFSET_X,
    example_cube_on_ground(CUBE_GROUND_CLEARANCE).y,
    0.0,
);

const FIT_TARGET_COLOR: Color = Color::srgb(0.45, 0.62, 0.85);
const FIT_TARGET_LABEL: &str = "AnimateToFit target";
const FIT_TARGET_TRANSLATION: Vec3 = Vec3::new(
    CUBE_OFFSET_X,
    example_cube_on_ground(CUBE_GROUND_CLEARANCE).y,
    0.0,
);

fn spawn_target(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let face_material = materials.add(transparent_face_material());
    let face_text_material = materials.add(emissive_text_material());
    commands
        .spawn((
            Mesh3d(meshes.add(Cuboid::from_length(CUBE_SIZE))),
            MeshMaterial3d(materials.add(TARGET_COLOR)),
            Transform::from_translation(TARGET_TRANSLATION),
            CameraHomeTarget,
        ))
        .with_children(|parent| {
            spawn_face_label_panels(
                parent,
                TARGET_LABEL,
                face_material.clone(),
                face_text_material.clone(),
            );
        });
}

/// Spawns the cube that the `A` key's `AnimateToFit` frames — a `FitTarget`-marked
/// entity wearing its name on each face, like the `input_*` examples. It is also a
/// `CameraHomeTarget`, so `H` frames it together with the home cube; `A` frames it
/// alone. The `target` entity on the animation events is what keeps the `A` and `H`
/// chips from co-lighting, even though both fits share the `AnimateToFit` source.
fn spawn_fit_target(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let face_material = materials.add(transparent_face_material());
    let face_text_material = materials.add(emissive_text_material());
    commands
        .spawn((
            Mesh3d(meshes.add(Cuboid::from_length(CUBE_SIZE))),
            MeshMaterial3d(materials.add(FIT_TARGET_COLOR)),
            Transform::from_translation(FIT_TARGET_TRANSLATION),
            FitTarget,
            CameraHomeTarget,
        ))
        .with_children(|parent| {
            spawn_face_label_panels(
                parent,
                FIT_TARGET_LABEL,
                face_material.clone(),
                face_text_material.clone(),
            );
        });
}

/// Spawns one transparent cube-face [`DiegeticPanel`] per side face, each
/// centered on the cube and carrying `label` in strongly-emissive text. The
/// emissive lives in the panel's `text_material` (a `StandardMaterial` whose
/// `emissive` is the boosted [`FACE_LABEL_COLOR`]), the same example-level
/// recipe as `focus_bounds`/`follow_target`.
fn spawn_face_label_panels(
    parent: &mut ChildSpawnerCommands,
    label: &'static str,
    face_material: Handle<StandardMaterial>,
    face_text_material: Handle<StandardMaterial>,
) {
    for face in [Face::Front, Face::Back, Face::Left, Face::Right] {
        match face_label_panel(label, face_material.clone(), face_text_material.clone()) {
            Ok(panel) => {
                parent.spawn((panel, cube_face_transform(face, CUBE_SIZE)));
            },
            Err(error) => {
                error!("animation: failed to build cube face label panel: {error}");
            },
        }
    }
}

fn face_label_panel(
    label: &str,
    face_material: Handle<StandardMaterial>,
    face_text_material: Handle<StandardMaterial>,
) -> Result<DiegeticPanel, PanelBuildError> {
    DiegeticPanel::world()
        .size(FACE_LABEL_PANEL_SIZE, FACE_LABEL_PANEL_SIZE)
        .font_unit(Unit::Millimeters)
        .anchor(Anchor::Center)
        .material(face_material)
        .text_material(face_text_material)
        .with_tree(face_label_tree(label))
        .build()
}

/// Transparent, unlit panel background — only the emissive text shows.
fn transparent_face_material() -> StandardMaterial {
    StandardMaterial {
        base_color: Color::NONE,
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        ..default_panel_material()
    }
}

/// Strongly-emissive text material: [`FACE_LABEL_COLOR`] multiplied by
/// [`FACE_LABEL_EMISSIVE_BOOST`], so the glyphs read as self-lit.
fn emissive_text_material() -> StandardMaterial {
    let mut emissive: LinearRgba = FACE_LABEL_COLOR.into();
    emissive.red *= FACE_LABEL_EMISSIVE_BOOST;
    emissive.green *= FACE_LABEL_EMISSIVE_BOOST;
    emissive.blue *= FACE_LABEL_EMISSIVE_BOOST;
    StandardMaterial {
        base_color: Color::NONE,
        emissive,
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        ..default_panel_material()
    }
}

fn face_label_tree(label: &str) -> LayoutTree {
    let mut builder = LayoutBuilder::with_root(
        El::column()
            .width(Sizing::fixed(FACE_LABEL_PANEL_SIZE))
            .height(Sizing::fixed(FACE_LABEL_PANEL_SIZE))
            .alignment(AlignX::Center, AlignY::Center)
            .padding(Padding::all(FACE_LABEL_PADDING))
            .clip(),
    );
    builder.text((
        label,
        TextStyle::new(FACE_LABEL_TEXT_SIZE)
            .with_color(FACE_LABEL_COLOR)
            .with_align(TextAlign::Center)
            .with_shadow_mode(GlyphShadowMode::None),
    ));
    builder.build()
}

// ═════════════════════════════════════════════════════════════════════════════
// EXPLAINER PANEL — a lower-left stack of one bordered box per mechanism, in the
// `aa_text` style. Static copy, spawned once; nothing refreshes it.
// ═════════════════════════════════════════════════════════════════════════════

const EXPLAINER_BOX_WIDTH: Px = Px(264.0);
const EXPLAINER_DIVIDER_HEIGHT: Px = Px(1.0);
const EXPLAINER_PADDING: Px = Px(10.0);
const EXPLAINER_RADIUS: Px = Px(10.0);
const EXPLAINER_BORDER_WIDTH: Px = Px(1.0);
const EXPLAINER_ROW_GAP: Px = Px(4.0);
const EXPLAINER_STACK_GAP: Px = Px(8.0);
const EXPLAINER_PANEL_NAME: &str = "Animation explainer panel";
const EXPLAINER_HEADER_COLOR: Color = Color::srgb(0.95, 0.95, 0.97);
const EXPLAINER_BODY_COLOR: Color = Color::srgba(0.68, 0.72, 0.82, 0.9);
const EXPLAINER_BORDER_COLOR: Color = Color::srgba(0.15, 0.7, 0.9, 0.4);

/// One mechanism's box: a heading over wrapped body lines.
struct Explainer {
    title: &'static str,
    lines: &'static [&'static str],
}

const EXPLAINERS: [Explainer; 3] = [
    Explainer {
        title: "Manual Orbit",
        lines: &[
            "Writes OrbitCam target_yaw, target_pitch, and radius directly each frame for a continuous orbit.",
            "Camera input is disabled and smoothing zeroed so the writes apply with no easing lag.",
        ],
    },
    Explainer {
        title: "Play Animation",
        lines: &[
            "Triggers a VecDeque<CameraMove> queue on the camera.",
            "Each move eases over its own duration, then advances to the next.",
        ],
    },
    Explainer {
        title: "AnimateToFit",
        lines: &[
            "Trigger AnimateToFit with a target entity.",
            "The camera eases to fit it to the screen with a provided margin.",
        ],
    },
];

/// Spawns the lower-left explainer panel: a transparent, unlit screen panel
/// stacking one bordered box per [`Explainer`].
fn spawn_explainer_panel(mut commands: Commands, mut materials: ResMut<Assets<StandardMaterial>>) {
    let unlit = materials.add(StandardMaterial {
        unlit: true,
        ..default_panel_material()
    });
    let panel = DiegeticPanel::screen()
        .size(Fit, Fit)
        .anchor(Anchor::BottomLeft)
        .material(unlit.clone())
        .text_material(unlit)
        .with_tree(build_explainer_tree())
        .build();

    match panel {
        Ok(panel) => {
            commands.spawn((Name::new(EXPLAINER_PANEL_NAME), panel, Transform::default()));
        },
        Err(error) => {
            error!("animation: failed to build explainer panel: {error}");
        },
    }
}

fn build_explainer_tree() -> LayoutTree {
    let mut builder = LayoutBuilder::with_root(El::new().width(Sizing::FIT).height(Sizing::FIT));
    let title = TextStyle::new(TITLE_SIZE).with_color(EXPLAINER_HEADER_COLOR);
    // Wrapped body text flows to the fixed box width.
    let body = TextStyle::new(LABEL_SIZE).with_color(EXPLAINER_BODY_COLOR);
    builder.with(
        El::column()
            .width(Sizing::fixed(EXPLAINER_BOX_WIDTH))
            .height(Sizing::FIT)
            .gap(EXPLAINER_STACK_GAP),
        |builder| {
            for explainer in &EXPLAINERS {
                build_explainer_box(builder, explainer, &title, &body);
            }
        },
    );
    builder.build()
}

fn build_explainer_box(
    builder: &mut LayoutBuilder,
    explainer: &Explainer,
    title: &TextStyle,
    body: &TextStyle,
) {
    builder.with(
        El::column()
            .width(Sizing::GROW)
            .height(Sizing::FIT)
            .gap(EXPLAINER_ROW_GAP)
            .padding(Padding::all(EXPLAINER_PADDING))
            .corner_radius(CornerRadius::all(EXPLAINER_RADIUS))
            .background(DEFAULT_PANEL_BACKGROUND)
            .border(Border::all(EXPLAINER_BORDER_WIDTH, EXPLAINER_BORDER_COLOR)),
        |builder| {
            builder.text((explainer.title, title.clone()));
            explainer_divider(builder);
            for line in explainer.lines {
                builder.text((*line, body.clone()));
            }
        },
    );
}

/// A horizontal hairline rule spanning the box width, drawn under each heading.
fn explainer_divider(builder: &mut LayoutBuilder) {
    builder.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::fixed(EXPLAINER_DIVIDER_HEIGHT))
            .background(EXPLAINER_BORDER_COLOR),
        |_| {},
    );
}
