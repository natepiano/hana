//! @generated `bevy_example_template`
//! `WorldText` example — standalone MSDF text in world space.
//!
//! Demonstrates `WorldText` on a ground plane and on the front face of a cube.
//! Click the cube to zoom in, click the plane to zoom back out.

use std::time::Duration;

use bevy::picking::mesh_picking::MeshPickingPlugin;
use bevy::prelude::*;
use bevy_brp_extras::BrpExtrasPlugin;
use bevy_brp_extras::PortDisplay;
use bevy_diegetic::Anchor;
use bevy_diegetic::DiegeticUiPlugin;
use bevy_diegetic::WorldText;
use bevy_diegetic::WorldTextStyle;
use bevy_lagrange::InputControl;
use bevy_lagrange::LagrangePlugin;
use bevy_lagrange::OrbitCam;
use bevy_lagrange::TrackpadBehavior;
use bevy_lagrange::TrackpadInput;
use bevy_lagrange::ZoomToFit;
use bevy_window_manager::WindowManagerPlugin;

const ZOOM_MARGIN_MESH: f32 = 0.15;
const ZOOM_MARGIN_SCENE: f32 = 0.08;
const ZOOM_DURATION_MS: u64 = 1000;

#[derive(Resource)]
struct SceneBounds(Entity);

/// Marker for anchor demo text entities that can be rotated with 'R'.
#[derive(Component)]
struct AnchorDemoText {
    /// The world-space position of the anchor point (stays fixed during rotation).
    anchor_pos:    Vec3,
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
    App::new()
        .add_plugins((
            DefaultPlugins,
            DiegeticUiPlugin,
            LagrangePlugin,
            BrpExtrasPlugin::default().port_in_title(PortDisplay::NonDefault),
            WindowManagerPlugin,
            MeshPickingPlugin,
        ))
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
    let ground = spawn_ground(&mut commands, &mut meshes, &mut materials);
    commands.insert_resource(SceneBounds(ground));

    spawn_labeled_cube(&mut commands, &mut meshes, &mut materials);
    spawn_anchor_demo(&mut commands, &mut meshes, &mut materials);
    spawn_ground_text(&mut commands);
    spawn_lighting_and_camera(&mut commands);
}

/// Spawns the translucent ground plane.
fn spawn_ground(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
) -> Entity {
    commands
        .spawn((
            Mesh3d(meshes.add(Plane3d::default().mesh().size(8.0, 8.0))),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: Color::srgba(0.08, 0.08, 0.08, 0.5),
                alpha_mode: AlphaMode::Blend,
                double_sided: true,
                cull_mode: None,
                ..default()
            })),
        ))
        .observe(on_ground_clicked)
        .id()
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
        .observe(on_mesh_clicked)
        .with_children(|parent| {
            let face_style = WorldTextStyle::new(0.20).with_color(Color::srgb(0.9, 0.3, 0.1));

            // Front face (+Z).
            parent
                .spawn((
                    WorldText::new("FRONT"),
                    face_style.clone(),
                    Transform::from_xyz(0.0, 0.0, 0.501),
                ))
                .observe(on_text_clicked);

            // Back face (-Z).
            parent
                .spawn((
                    WorldText::new("BACK"),
                    face_style.clone(),
                    Transform::from_xyz(0.0, 0.0, -0.501)
                        .with_rotation(Quat::from_rotation_y(std::f32::consts::PI)),
                ))
                .observe(on_text_clicked);

            // Top face (+Y).
            parent
                .spawn((
                    WorldText::new("TOP"),
                    face_style.clone(),
                    Transform::from_xyz(0.0, 0.501, 0.0)
                        .with_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2)),
                ))
                .observe(on_text_clicked);

            // Bottom face (-Y).
            parent
                .spawn((
                    WorldText::new("BOTTOM"),
                    face_style.clone(),
                    Transform::from_xyz(0.0, -0.501, 0.0)
                        .with_rotation(Quat::from_rotation_x(std::f32::consts::FRAC_PI_2)),
                ))
                .observe(on_text_clicked);

            // Left face (-X).
            parent
                .spawn((
                    WorldText::new("LEFT"),
                    face_style.clone(),
                    Transform::from_xyz(-0.501, 0.0, 0.0)
                        .with_rotation(Quat::from_rotation_y(-std::f32::consts::FRAC_PI_2)),
                ))
                .observe(on_text_clicked);

            // Right face (+X).
            parent
                .spawn((
                    WorldText::new("RIGHT"),
                    face_style,
                    Transform::from_xyz(0.501, 0.0, 0.0)
                        .with_rotation(Quat::from_rotation_y(std::f32::consts::FRAC_PI_2)),
                ))
                .observe(on_text_clicked);
        });
}

/// Spawns the anchor demo panel: title, instructions, nine anchor-point
/// labels with red dot markers, and the `AnchorDemoText` components.
fn spawn_anchor_demo(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
) {
    let demo_center = Vec3::new(2.0, 1.5, -0.5);
    let demo_rotation = Quat::from_rotation_y(-15.0_f32.to_radians());

    // Title.
    commands
        .spawn((
            WorldText::new("Text Anchors"),
            WorldTextStyle::new(0.16)
                .with_color(Color::srgb(0.7, 0.8, 1.0))
                .with_anchor(Anchor::TopCenter),
            Transform::from_translation(demo_center + demo_rotation * Vec3::new(0.0, 1.4, 0.0))
                .with_rotation(demo_rotation),
        ))
        .observe(on_text_clicked);

    // Instructions.
    commands
        .spawn((
            WorldText::new("red dot = Transform translation\n'X' 'Y' 'Z' to rotate around axis"),
            WorldTextStyle::new(0.10)
                .with_color(Color::WHITE)
                .with_anchor(Anchor::TopCenter),
            Transform::from_translation(demo_center + demo_rotation * Vec3::new(0.0, 1.15, 0.0))
                .with_rotation(demo_rotation),
        ))
        .observe(on_text_clicked);

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

    let sphere_mesh = meshes.add(Sphere::new(0.025));
    let sphere_material = materials.add(StandardMaterial {
        base_color: Color::srgb(1.0, 0.2, 0.2),
        unlit: true,
        ..default()
    });

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
        commands
            .spawn((
                WorldText::new(text),
                WorldTextStyle::new(0.125)
                    .with_color(Color::WHITE)
                    .with_anchor(anchor),
                Transform::from_translation(world_pos).with_rotation(demo_rotation),
                AnchorDemoText {
                    anchor_pos:    world_pos,
                    base_rotation: demo_rotation,
                },
            ))
            .observe(on_text_clicked);
    }
}

/// Spawns flat text on the ground plane: "GROUND" label and click instructions.
fn spawn_ground_text(commands: &mut Commands) {
    commands
        .spawn((
            WorldText::new("GROUND"),
            WorldTextStyle::new(0.48).with_color(Color::srgb(0.9, 0.9, 0.1)),
            Transform::from_xyz(0.0, 0.001, 1.5)
                .with_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2)),
        ))
        .observe(on_text_clicked);

    commands
        .spawn((
            WorldText::new("click the box to zoom in\nclick the text to zoom in\nclick the plane to zoom back out"),
            WorldTextStyle::new(0.16)
                .with_color(Color::WHITE)
                .with_anchor(Anchor::TopLeft),
            Transform::from_xyz(-3.8, 0.001, -3.8)
                .with_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2)),
        ))
        .observe(on_text_clicked);
}

/// Spawns ambient light, directional light, and the orbit camera.
fn spawn_lighting_and_camera(commands: &mut Commands) {
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

    commands.spawn((OrbitCam {
        focus: Vec3::ZERO,
        radius: Some(11.33),
        yaw: Some(0.015),
        pitch: Some(0.667),
        button_orbit: MouseButton::Middle,
        button_pan: MouseButton::Middle,
        modifier_pan: Some(KeyCode::ShiftLeft),
        input_control: Some(InputControl {
            trackpad: Some(TrackpadInput {
                behavior:    TrackpadBehavior::BlenderLike {
                    modifier_pan:  Some(KeyCode::ShiftLeft),
                    modifier_zoom: Some(KeyCode::ControlLeft),
                },
                sensitivity: 0.5,
            }),
            ..default()
        }),
        ..default()
    },));
}

fn on_mesh_clicked(click: On<Pointer<Click>>, mut commands: Commands) {
    if click.button != PointerButton::Primary {
        return;
    }
    let camera = click.hit.camera;
    commands.trigger(
        ZoomToFit::new(camera, click.entity)
            .margin(ZOOM_MARGIN_MESH)
            .duration(Duration::from_millis(ZOOM_DURATION_MS)),
    );
}

fn on_ground_clicked(click: On<Pointer<Click>>, mut commands: Commands, scene: Res<SceneBounds>) {
    if click.button != PointerButton::Primary {
        return;
    }
    let camera = click.hit.camera;
    commands.trigger(
        ZoomToFit::new(camera, scene.0)
            .margin(ZOOM_MARGIN_SCENE)
            .duration(Duration::from_millis(ZOOM_DURATION_MS)),
    );
}

/// Zooms to fit the clicked `WorldText` entity.
///
/// Attached per-entity via `.observe()`. Pointer events bubble up from the
/// child mesh, so this fires even though the mesh is on a child entity.
fn on_text_clicked(
    mut click: On<Pointer<Click>>,
    children: Query<&Children>,
    meshes: Query<(), With<Mesh3d>>,
    mut commands: Commands,
) {
    if click.button != PointerButton::Primary {
        return;
    }
    click.propagate(false);
    let camera = click.hit.camera;
    let target = children
        .get(click.entity)
        .ok()
        .and_then(|kids| kids.iter().find(|&kid| meshes.contains(kid)))
        .unwrap_or(click.entity);
    commands.trigger(
        ZoomToFit::new(camera, target)
            .margin(ZOOM_MARGIN_MESH)
            .duration(Duration::from_millis(ZOOM_DURATION_MS)),
    );
}

/// Press X, Y, or Z to start a full rotation around that local axis.
/// Anchor demo texts rotate around their anchor point (red dot stays fixed).
/// The cube rotates around its own center on the same axis simultaneously.
fn rotate_anchor_demo(
    keyboard: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
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
            // Capture the cube's current rotation as its base.
            if let Ok(cube_t) = cube.single() {
                *cube_base_rotation = Some(cube_t.rotation);
            }
        }
    }

    let Some(angle) = state.angle.as_mut() else {
        return;
    };

    let speed = 1.5;
    *angle = time.delta_secs().mul_add(speed, *angle);
    let current_angle = *angle;
    let axis = state.axis;

    if current_angle >= std::f32::consts::TAU {
        // Snap back to start.
        for (demo, mut transform) in &mut texts {
            *transform =
                Transform::from_translation(demo.anchor_pos).with_rotation(demo.base_rotation);
        }
        if let (Ok(mut cube_t), Some(base)) = (cube.single_mut(), *cube_base_rotation) {
            cube_t.rotation = base;
        }
        state.angle = None;
        *cube_base_rotation = None;
        return;
    }

    let rot = Quat::from_axis_angle(axis, current_angle);

    // Rotate anchor demo texts around their anchor point.
    for (demo, mut transform) in &mut texts {
        transform.rotation = demo.base_rotation * rot;
    }

    // Rotate cube on the same local axis.
    if let (Ok(mut cube_t), Some(base)) = (cube.single_mut(), *cube_base_rotation) {
        cube_t.rotation = base * rot;
    }
}
