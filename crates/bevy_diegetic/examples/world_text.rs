//! @generated `bevy_example_template`
//! `WorldText` example — standalone MSDF text in world space.
//!
//! Demonstrates `WorldText` on a ground plane, on each face of a cube, and on
//! an anchor demo panel. Press `X`/`Y`/`Z` to rotate the anchor panel and the
//! labeled cube around the matching local axis; press `H` to return to the
//! home camera pose.
//!
//! This example runs OIT (`.with_stable_transparency()`), which forces
//! `Msaa::Off`, so the cube's silhouette edges would alias. MSAA can't coexist
//! with OIT, but post-process anti-aliasing can: SMAA is on by default to
//! recover the edge AA. Press `S` to toggle it off and watch the cube
//! silhouette alias — that is the AA cost OIT alone would impose. SMAA runs on
//! the composited image after the OIT pass, so the two are compatible.
//!
//! # Code layout
//!
//! Read top-to-bottom for the demonstrated API:
//!   - `main()` wires the app (cube + face text, stable-transparency OIT, title-bar chips).
//!   - **WORLD TEXT** section spawns the anchor demo and the ground label.
//!   - **SMAA TOGGLE** section adds/removes [`Smaa`] on the camera.
//!   - **ROTATION ANIMATION** at the end is decorative scaffolding.

use bevy::anti_alias::smaa::Smaa;
use bevy::light::NotShadowCaster;
use bevy::prelude::*;
use bevy_diegetic::Anchor;
use bevy_diegetic::WorldText;
use bevy_lagrange::OrbitCam;
use bevy_lagrange::OrbitCamPreset;
use fairy_dust::CameraHomeTarget;
use fairy_dust::ControlActivation;
use fairy_dust::Face;
use fairy_dust::TitleBar;
use fairy_dust::TitleChipActivation;

// Camera home pose.
const HOME_YAW: f32 = 0.015;
const HOME_PITCH: f32 = 0.5;

// Title-bar control labels (also the keys for chip wiring).
const X_ROTATE_CONTROL: &str = "X Rotate";
const Y_ROTATE_CONTROL: &str = "Y Rotate";
const Z_ROTATE_CONTROL: &str = "Z Rotate";
const SMAA_CONTROL: &str = "S SMAA";

// Rotation animation.
const ROTATION_SPEED: f32 = 1.5;

// Cube + per-face label styling.
const CUBE_SIZE: f32 = fairy_dust::EXAMPLE_CUBE_SIZE;
const CUBE_TRANSLATION: Vec3 = Vec3::new(-2.5, 1.0, 2.5);
const CUBE_COLOR: Color = fairy_dust::EXAMPLE_CUBE_COLOR;
const CUBE_YAW: f32 = 20.0;

// Translucent backdrop behind the anchor demo labels.
const ANCHOR_FRAME_COLOR: Color = Color::srgba(0.08, 0.08, 0.08, 0.18);
const ANCHOR_FRAME_SIZE: Vec3 = Vec3::new(3.6, 2.0, 0.02);
const ANCHOR_FRAME_LOCAL_OFFSET: Vec3 = Vec3::new(0.0, -0.2, -0.1);

fn main() {
    // `bevy_diegetic::DiegeticUiPlugin` is registered automatically by
    // `fairy_dust::sprinkle_example`.
    fairy_dust::sprinkle_example()
        .with_brp_extras()
        .with_save_window_position()
        .with_studio_lighting()
        .with_ground_plane()
        .with_cube()
        .size(CUBE_SIZE)
        .color(CUBE_COLOR)
        .transform(
            Transform::from_translation(CUBE_TRANSLATION)
                .with_rotation(Quat::from_rotation_y(CUBE_YAW.to_radians())),
        )
        .insert((CameraHomeTarget, DemoCube))
        .face_label(Face::Front, "FRONT")
        .face_label(Face::Back, "BACK")
        .face_label(Face::Top, "TOP")
        .face_label(Face::Bottom, "BOTTOM")
        .face_label(Face::Left, "LEFT")
        .face_label(Face::Right, "RIGHT")
        .with_orbit_cam_preset(|_| {}, OrbitCamPreset::BlenderLike)
        .with_stable_transparency()
        .with_camera_home()
        .yaw(HOME_YAW)
        .pitch(HOME_PITCH)
        .with_title_bar(
            TitleBar::new()
                .with_title("World Text")
                .with_anchor(Anchor::TopLeft)
                .control(X_ROTATE_CONTROL)
                .control(Y_ROTATE_CONTROL)
                .control(Z_ROTATE_CONTROL)
                .control(SMAA_CONTROL),
        )
        .wire_chip_to_events_filtered::<RotationBegin, RotationEnd, _, _>(
            X_ROTATE_CONTROL,
            |e| e.axis == Vec3::X,
            |e| e.axis == Vec3::X,
        )
        .wire_chip_to_events_filtered::<RotationBegin, RotationEnd, _, _>(
            Y_ROTATE_CONTROL,
            |e| e.axis == Vec3::Y,
            |e| e.axis == Vec3::Y,
        )
        .wire_chip_to_events_filtered::<RotationBegin, RotationEnd, _, _>(
            Z_ROTATE_CONTROL,
            |e| e.axis == Vec3::Z,
            |e| e.axis == Vec3::Z,
        )
        .wire_chip_to_activation::<SmaaState>(SMAA_CONTROL)
        .with_camera_control_panel()
        .init_resource::<AnchorRotation>()
        .init_resource::<SmaaState>()
        .add_systems(Startup, setup)
        .add_systems(Update, (rotate_anchor_demo, toggle_smaa))
        .run();
}

// ═════════════════════════════════════════════════════════════════════════════
// WORLD TEXT — spawning [`WorldText`] entities with [`WorldTextStyle`] and the
// nine [`Anchor`] variants. This is the primary API the example demonstrates;
// the labels on the cube faces are added in `main()` via `face_text`.
// ═════════════════════════════════════════════════════════════════════════════

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    spawn_anchor_demo(&mut commands, &mut meshes, &mut materials);
    spawn_ground_text(&mut commands);
}

/// Spawns the anchor demo: a translucent backdrop plane, title, instructions,
/// nine anchor-point labels with red dot markers, and the `AnchorDemoText`
/// components.
fn spawn_anchor_demo(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
) {
    let demo_center = Vec3::new(2.0, 1.5, -0.5);
    let demo_rotation = Quat::from_rotation_y(-15.0_f32.to_radians());

    // Backdrop frame plane — sits behind the anchor labels in the demo's
    // local Z, slightly transparent, ground-plane color.
    commands.spawn((
        CameraHomeTarget,
        Mesh3d(meshes.add(Cuboid::new(
            ANCHOR_FRAME_SIZE.x,
            ANCHOR_FRAME_SIZE.y,
            ANCHOR_FRAME_SIZE.z,
        ))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: ANCHOR_FRAME_COLOR,
            alpha_mode: AlphaMode::Blend,
            double_sided: true,
            cull_mode: None,
            ..default()
        })),
        Transform::from_translation(demo_center + demo_rotation * ANCHOR_FRAME_LOCAL_OFFSET)
            .with_rotation(demo_rotation),
        NotShadowCaster,
    ));

    let sphere_mesh = meshes.add(Sphere::new(0.025));
    let sphere_material = materials.add(StandardMaterial {
        base_color: Color::srgb(1.0, 0.2, 0.2),
        unlit: true,
        ..default()
    });

    // Title.
    commands.spawn(
        WorldText::new("Text Anchors")
            .size(0.16)
            .color(Color::srgb(0.7, 0.8, 1.0))
            .anchor(Anchor::TopCenter)
            .transform(
                Transform::from_translation(demo_center + demo_rotation * Vec3::new(0.0, 1.4, 0.0))
                    .with_rotation(demo_rotation),
            )
            .bundle(),
    );

    commands.spawn((
        Mesh3d(sphere_mesh.clone()),
        MeshMaterial3d(sphere_material.clone()),
        Transform::from_translation(demo_center + demo_rotation * Vec3::new(-0.60, 1.10, 0.01)),
    ));
    commands.spawn(
        WorldText::new(" = Transform translation")
            .size(0.10)
            .color(Color::WHITE)
            .anchor(Anchor::TopLeft)
            .transform(
                Transform::from_translation(
                    demo_center + demo_rotation * Vec3::new(-0.55, 1.15, 0.0),
                )
                .with_rotation(demo_rotation),
            )
            .bundle(),
    );

    let anchor_demo = [
        (Anchor::TopLeft, "TopLeft", -1.3, 0.5),
        (Anchor::TopCenter, "TopCenter", 0.0, 0.5),
        (Anchor::TopRight, "TopRight", 1.3, 0.5),
        (Anchor::CenterLeft, "CenterLeft", -1.3, -0.2),
        (Anchor::Center, "Center", 0.0, -0.2),
        (Anchor::CenterRight, "CenterRight", 1.3, -0.2),
        (Anchor::BottomLeft, "BottomLeft", -1.3, -0.9),
        (Anchor::BottomCenter, "BottomCenter", 0.0, -0.9),
        (Anchor::BottomRight, "BottomRight", 1.3, -0.9),
    ];

    for (anchor, text, local_x, local_y) in anchor_demo {
        let local_offset = Vec3::new(local_x, local_y, 0.01);
        let world_pos = demo_center + demo_rotation * local_offset;

        // Sphere at the anchor origin.
        commands.spawn((
            Mesh3d(sphere_mesh.clone()),
            MeshMaterial3d(sphere_material.clone()),
            Transform::from_translation(world_pos),
        ));

        // Text with the given anchor.
        commands.spawn((
            WorldText::new(text)
                .size(0.125)
                .color(Color::WHITE)
                .anchor(anchor)
                .transform(Transform::from_translation(world_pos).with_rotation(demo_rotation))
                .bundle(),
            AnchorDemoText {
                position:      world_pos,
                base_rotation: demo_rotation,
            },
        ));
    }
}

/// Spawns the "GROUND PLANE" label flat on and centered on the ground plane.
fn spawn_ground_text(commands: &mut Commands) {
    commands.spawn((
        CameraHomeTarget,
        WorldText::new("GROUND PLANE")
            .size(0.48)
            .color(Color::srgb(0.9, 0.9, 0.1))
            .transform(
                Transform::from_xyz(0.0, 0.001, 0.0)
                    .with_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2)),
            )
            .bundle(),
    ));
}

// ═════════════════════════════════════════════════════════════════════════════
// SMAA TOGGLE — demonstrates that post-process SMAA composes with OIT. OIT
// forces `Msaa::Off`, so without SMAA the cube silhouette aliases; toggling
// SMAA on the camera entity recovers edge AA without disturbing the OIT pass.
// This is the second primary API the example demonstrates.
// ═════════════════════════════════════════════════════════════════════════════

/// Source of truth for the post-process SMAA toggle. Drives both the camera
/// component and the title-bar chip highlight.
#[derive(Resource, Default, Clone, Copy, PartialEq, Eq)]
enum SmaaState {
    /// No post-process AA: under OIT (`Msaa::Off`) the cube edges alias.
    #[default]
    Off,
    /// SMAA on: mesh edges resolve while the OIT text stays stable.
    On,
}

impl TitleChipActivation for SmaaState {
    fn activation(&self) -> ControlActivation {
        match self {
            Self::On => ControlActivation::Active,
            Self::Off => ControlActivation::Inactive,
        }
    }
}

/// On `S`, flip [`SmaaState`] and add or remove [`Smaa`] on the scene camera.
/// SMAA is a post-process pass that runs on the composited image after OIT
/// resolves, so it anti-aliases the mesh edges that `Msaa::Off` leaves jagged
/// without disturbing the OIT text composite.
fn toggle_smaa(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut state: ResMut<SmaaState>,
    cameras: Query<Entity, With<OrbitCam>>,
    mut commands: Commands,
) {
    if !keyboard.just_pressed(KeyCode::KeyS) {
        return;
    }
    *state = match *state {
        SmaaState::Off => SmaaState::On,
        SmaaState::On => SmaaState::Off,
    };
    for camera in &cameras {
        match *state {
            SmaaState::On => {
                commands.entity(camera).insert(Smaa::default());
            },
            SmaaState::Off => {
                commands.entity(camera).remove::<Smaa>();
            },
        }
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// ROTATION ANIMATION — supporting (decorative). Driving the anchor demo and
// the labeled cube around X/Y/Z so the viewer can see anchor points stay
// fixed while the text rotates. Not part of the demonstrated `WorldText` or
// SMAA APIs; safe to skim.
// ═════════════════════════════════════════════════════════════════════════════

/// Fired when an axis rotation of the anchor demo begins.
#[derive(Event)]
struct RotationBegin {
    axis: Vec3,
}

/// Fired when an axis rotation of the anchor demo completes a full revolution.
#[derive(Event)]
struct RotationEnd {
    axis: Vec3,
}

/// Marker for anchor demo text entities that can be rotated with 'R'.
#[derive(Component)]
struct AnchorDemoText {
    /// The world-space position of the anchor point (stays fixed during rotation).
    position:      Vec3,
    /// The base rotation of the demo panel.
    base_rotation: Quat,
}

#[derive(Resource, Default)]
struct AnchorRotation {
    /// Current rotation angle in radians (0..TAU). `None` = not rotating.
    angle: Option<f32>,
    /// Which local axis to rotate around.
    axis:  Vec3,
}

/// Marker for the cube entity so the rotation system can find it.
#[derive(Component)]
struct DemoCube;

/// Press X, Y, or Z to start a full rotation around that local axis.
/// `Anchor` demo texts rotate around their anchor point (red dot stays fixed).
/// The cube rotates around its own center on the same axis simultaneously.
/// Fires [`RotationBegin`]/[`RotationEnd`] events so the title bar chips can
/// highlight while the rotation runs.
fn rotate_anchor_demo(
    keyboard: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    mut commands: Commands,
    mut state: ResMut<AnchorRotation>,
    mut texts: Query<(&AnchorDemoText, &mut Transform), Without<DemoCube>>,
    mut cube: Query<&mut Transform, With<DemoCube>>,
    mut cube_base_rotation: Local<Option<Quat>>,
) {
    if state.angle.is_none() {
        let axis = if keyboard.just_pressed(KeyCode::KeyX) {
            Some(Vec3::X)
        } else if keyboard.just_pressed(KeyCode::KeyY) {
            Some(Vec3::Y)
        } else if keyboard.just_pressed(KeyCode::KeyZ) {
            Some(Vec3::Z)
        } else {
            None
        };
        if let Some(axis) = axis {
            state.angle = Some(0.0);
            state.axis = axis;
            if let Ok(cube_t) = cube.single() {
                *cube_base_rotation = Some(cube_t.rotation);
            }
            commands.trigger(RotationBegin { axis });
        }
    }

    let Some(angle) = state.angle.as_mut() else {
        return;
    };

    *angle = time.delta_secs().mul_add(ROTATION_SPEED, *angle);
    let current_angle = *angle;
    let axis = state.axis;

    if current_angle >= std::f32::consts::TAU {
        for (demo, mut transform) in &mut texts {
            *transform =
                Transform::from_translation(demo.position).with_rotation(demo.base_rotation);
        }
        if let (Ok(mut cube_t), Some(base)) = (cube.single_mut(), *cube_base_rotation) {
            cube_t.rotation = base;
        }
        state.angle = None;
        *cube_base_rotation = None;
        commands.trigger(RotationEnd { axis });
        return;
    }

    let rot = Quat::from_axis_angle(axis, current_angle);

    for (demo, mut transform) in &mut texts {
        transform.rotation = demo.base_rotation * rot;
    }

    if let (Ok(mut cube_t), Some(base)) = (cube.single_mut(), *cube_base_rotation) {
        cube_t.rotation = base * rot;
    }
}
