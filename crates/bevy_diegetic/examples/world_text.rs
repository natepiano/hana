//! @generated `bevy_example_template`
//! `WorldText` example — standalone MSDF text in world space.
//!
//! Demonstrates `WorldText` on a ground plane, on each face of a cube, and on
//! an anchor demo panel. Press `X`/`Y`/`Z` to rotate the anchor panel and the
//! labeled cube around the matching local axis; press `H` to return to the
//! home camera pose.

use bevy::prelude::*;
use bevy_diegetic::Anchor;
use bevy_diegetic::GlyphSidedness;
use bevy_diegetic::WorldText;
use bevy_diegetic::WorldTextStyle;
use bevy_lagrange::OrbitCamPreset;
use fairy_dust::TitleBar;

const HOME_FOCUS: Vec3 = Vec3::ZERO;
const HOME_FRAME_SIZE: f32 = 4.5;
const HOME_YAW: f32 = 0.015;
const HOME_PITCH: f32 = 0.5;

const X_ROTATE_CONTROL: &str = "X Rotate";
const Y_ROTATE_CONTROL: &str = "Y Rotate";
const Z_ROTATE_CONTROL: &str = "Z Rotate";

const ROTATION_SPEED: f32 = 1.5;

const GROUND_COLOR: Color = Color::srgba(0.08, 0.08, 0.08, 0.5);
const GROUND_SIZE_X: f32 = 10.0;
const GROUND_SIZE_Z: f32 = 6.0;

const ANCHOR_FRAME_COLOR: Color = Color::srgba(0.08, 0.08, 0.08, 0.18);
const ANCHOR_FRAME_SIZE: Vec3 = Vec3::new(3.6, 2.0, 0.02);
const ANCHOR_FRAME_LOCAL_OFFSET: Vec3 = Vec3::new(0.0, -0.2, -0.1);

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

fn main() {
    // `bevy_diegetic::DiegeticUiPlugin` is registered automatically by
    // `fairy_dust::sprinkle_example`.
    fairy_dust::sprinkle_example()
        .with_orbit_cam(|_| {}, OrbitCamPreset::BlenderLike)
        .with_camera_home(
            Transform::from_translation(HOME_FOCUS).with_scale(Vec3::splat(HOME_FRAME_SIZE)),
        )
        .yaw(HOME_YAW)
        .pitch(HOME_PITCH)
        .with_stable_transparency()
        .with_save_window_position()
        .with_brp_extras()
        .with_camera_control_panel()
        .with_title_bar(
            TitleBar::new("CONTROLS")
                .with_anchor(Anchor::TopLeft)
                .control(X_ROTATE_CONTROL)
                .control(Y_ROTATE_CONTROL)
                .control(Z_ROTATE_CONTROL),
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
        .init_resource::<AnchorRotation>()
        .add_systems(Startup, setup)
        .add_systems(Update, rotate_anchor_demo)
        .run();
}

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    spawn_ground(&mut commands, &mut meshes, &mut materials);
    spawn_labeled_cube(&mut commands, &mut meshes, &mut materials);
    spawn_anchor_demo(&mut commands, &mut meshes, &mut materials);
    spawn_ground_text(&mut commands);
    spawn_lighting(&mut commands);
}

/// Spawns the translucent ground plane.
fn spawn_ground(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
) {
    commands.spawn((
        Mesh3d(meshes.add(Plane3d::default().mesh().size(GROUND_SIZE_X, GROUND_SIZE_Z))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: GROUND_COLOR,
            alpha_mode: AlphaMode::Blend,
            double_sided: true,
            cull_mode: None,
            ..default()
        })),
    ));
}

/// Spawns a cube with `WorldText` labels on all six faces.
fn spawn_labeled_cube(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
) {
    commands
        .spawn((
            DemoCube,
            Mesh3d(meshes.add(Cuboid::default())),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: Color::srgb(0.8, 0.7, 0.6),
                ..default()
            })),
            Transform::from_xyz(-2.5, 1.0, 2.5)
                .with_rotation(Quat::from_rotation_y(20.0_f32.to_radians())),
        ))
        .with_children(|parent| {
            let one_sided_face_style = WorldTextStyle::new(0.20)
                .with_color(Color::srgb(0.9, 0.3, 0.1))
                .with_sidedness(GlyphSidedness::OneSided);

            // Front face (+Z).
            parent.spawn((
                WorldText::new("FRONT"),
                one_sided_face_style.clone(),
                Transform::from_xyz(0.0, 0.0, 0.501),
            ));

            // Back face (-Z).
            parent.spawn((
                WorldText::new("BACK"),
                one_sided_face_style.clone(),
                Transform::from_xyz(0.0, 0.0, -0.501)
                    .with_rotation(Quat::from_rotation_y(std::f32::consts::PI)),
            ));

            // Top face (+Y).
            parent.spawn((
                WorldText::new("TOP"),
                one_sided_face_style.clone(),
                Transform::from_xyz(0.0, 0.501, 0.0)
                    .with_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2)),
            ));

            // Bottom face (-Y).
            parent.spawn((
                WorldText::new("BOTTOM"),
                one_sided_face_style.clone(),
                Transform::from_xyz(0.0, -0.501, 0.0)
                    .with_rotation(Quat::from_rotation_x(std::f32::consts::FRAC_PI_2)),
            ));

            // Left face (-X).
            parent.spawn((
                WorldText::new("LEFT"),
                one_sided_face_style.clone(),
                Transform::from_xyz(-0.501, 0.0, 0.0)
                    .with_rotation(Quat::from_rotation_y(-std::f32::consts::FRAC_PI_2)),
            ));

            // Right face (+X).
            parent.spawn((
                WorldText::new("RIGHT"),
                one_sided_face_style,
                Transform::from_xyz(0.501, 0.0, 0.0)
                    .with_rotation(Quat::from_rotation_y(std::f32::consts::FRAC_PI_2)),
            ));
        });
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
    ));

    let sphere_mesh = meshes.add(Sphere::new(0.025));
    let sphere_material = materials.add(StandardMaterial {
        base_color: Color::srgb(1.0, 0.2, 0.2),
        unlit: true,
        ..default()
    });

    // Title.
    commands.spawn((
        WorldText::new("Text Anchors"),
        WorldTextStyle::new(0.16)
            .with_color(Color::srgb(0.7, 0.8, 1.0))
            .with_anchor(Anchor::TopCenter),
        Transform::from_translation(demo_center + demo_rotation * Vec3::new(0.0, 1.4, 0.0))
            .with_rotation(demo_rotation),
    ));

    // Instructions: red sphere marker + "= Transform translation".
    commands.spawn((
        Mesh3d(sphere_mesh.clone()),
        MeshMaterial3d(sphere_material.clone()),
        Transform::from_translation(demo_center + demo_rotation * Vec3::new(-0.60, 1.10, 0.01)),
    ));
    commands.spawn((
        WorldText::new("= Transform translation"),
        WorldTextStyle::new(0.10)
            .with_color(Color::WHITE)
            .with_anchor(Anchor::TopLeft),
        Transform::from_translation(demo_center + demo_rotation * Vec3::new(-0.55, 1.15, 0.0))
            .with_rotation(demo_rotation),
    ));

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
            WorldText::new(text),
            WorldTextStyle::new(0.125)
                .with_color(Color::WHITE)
                .with_anchor(anchor),
            Transform::from_translation(world_pos).with_rotation(demo_rotation),
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
        WorldText::new("GROUND PLANE"),
        WorldTextStyle::new(0.48)
            .with_color(Color::srgb(0.9, 0.9, 0.1))
            .with_sidedness(GlyphSidedness::OneSided),
        Transform::from_xyz(0.0, 0.001, 0.0)
            .with_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2)),
    ));
}

/// Spawns ambient light and directional lights. The orbit camera is set up
/// by `fairy_dust::with_orbit_cam` in `main`.
fn spawn_lighting(commands: &mut Commands) {
    commands.insert_resource(GlobalAmbientLight {
        color:                      Color::WHITE,
        brightness:                 1_000.0,
        affects_lightmapped_meshes: true,
    });

    commands.spawn((
        DirectionalLight {
            shadows_enabled: true,
            ..default()
        },
        Transform::from_xyz(4.0, 8.0, 4.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    commands.spawn((
        DirectionalLight {
            shadows_enabled: false,
            ..default()
        },
        Transform::from_xyz(-4.0, 8.0, -4.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
}

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
