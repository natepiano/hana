//! MSDF text rendering test — Phase 3 visual validation gate.
//!
//! Same scene as the default scaffold (ground plane, light, orbit camera)
//! but with the cube replaced by a diegetic panel with MSDF-rendered text.

use std::time::Duration;

use bevy::picking::mesh_picking::MeshPickingPlugin;
use bevy::prelude::*;
use bevy_brp_extras::BrpExtrasPlugin;
use bevy_diegetic::Border;
use bevy_diegetic::DiegeticPanel;
use bevy_diegetic::DiegeticUiPlugin;
use bevy_diegetic::Direction;
use bevy_diegetic::El;
use bevy_diegetic::LayoutBuilder;
use bevy_diegetic::Padding;
use bevy_diegetic::Sizing;
use bevy_diegetic::TextConfig;
use bevy_panorbit_camera::PanOrbitCamera;
use bevy_panorbit_camera::PanOrbitCameraPlugin;
use bevy_panorbit_camera::TrackpadBehavior;
use bevy_panorbit_camera_ext::PanOrbitCameraExtPlugin;
use bevy_panorbit_camera_ext::ZoomToFit;
use bevy_window_manager::WindowManagerPlugin;

const ZOOM_MARGIN_MESH: f32 = 0.15;
const ZOOM_MARGIN_SCENE: f32 = 0.08;
const ZOOM_DURATION_MS: u64 = 1000;

#[derive(Resource)]
struct SceneBounds(Entity);

fn main() {
    App::new()
        .add_plugins((
            DefaultPlugins,
            DiegeticUiPlugin,
            PanOrbitCameraPlugin,
            PanOrbitCameraExtPlugin,
            BrpExtrasPlugin::default(),
            WindowManagerPlugin,
            MeshPickingPlugin,
        ))
        .add_systems(Startup, setup)
        .run();
}

#[allow(clippy::too_many_arguments)]
fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // Ground plane.
    let ground = commands
        .spawn((
            Mesh3d(meshes.add(Plane3d::default().mesh().size(12.0, 12.0))),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: Color::srgb(0.3, 0.5, 0.3),
                double_sided: true,
                cull_mode: None,
                ..default()
            })),
        ))
        .observe(on_ground_clicked)
        .id();

    commands.insert_resource(SceneBounds(ground));

    // Panel (replaces the cube).
    let panel_w = 2.0;
    let panel_h = 1.5;
    let tree = build_panel();
    commands
        .spawn((
            DiegeticPanel {
                tree,
                layout_width:  160.0,
                layout_height: 120.0,
                world_width:   panel_w,
                world_height:  panel_h,
            },
            // Transparent quad for picking / zoom-to-fit.
            Mesh3d(meshes.add(Rectangle::new(panel_w, panel_h))),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: Color::srgba(0.0, 0.0, 0.0, 0.0),
                alpha_mode: AlphaMode::Blend,
                ..default()
            })),
            Transform::from_xyz(0.0, 1.5, 0.0),
        ))
        .observe(on_mesh_clicked);

    // White backdrop behind the panel.
    commands.spawn((
        Mesh3d(meshes.add(Rectangle::new(3.0, 2.5))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::WHITE,
            double_sided: true,
            cull_mode: None,
            ..default()
        })),
        Transform::from_xyz(0.0, 1.5, -1.0),
    ));

    // Point light behind/above the camera.
    commands.spawn((
        PointLight {
            intensity: 500_000.0,
            shadows_enabled: true,
            range: 30.0,
            ..default()
        },
        Transform::from_xyz(0.0, 6.0, 10.0),
    ));

    // Camera.
    commands.spawn((
        PanOrbitCamera {
            button_orbit: MouseButton::Middle,
            button_pan: MouseButton::Middle,
            modifier_pan: Some(KeyCode::ShiftLeft),
            trackpad_behavior: TrackpadBehavior::BlenderLike {
                modifier_pan: Some(KeyCode::ShiftLeft),
                modifier_zoom: Some(KeyCode::ControlLeft),
            },
            trackpad_pinch_to_zoom_enabled: true,
            ..default()
        },
        Transform::from_xyz(0.0, 8.0, 12.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
}

fn build_panel() -> bevy_diegetic::LayoutTree {
    let mut builder = LayoutBuilder::new(160.0, 120.0);
    builder.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::GROW)
            .padding(Padding::all(8.0))
            .direction(Direction::TopToBottom)
            .child_gap(6.0)
            .background(Color::srgb_u8(40, 44, 52))
            .border(Border::all(2.0, Color::srgb_u8(120, 130, 140))),
        |b| {
            // Upper text — GROW height to fill available space.
            b.with(
                El::new()
                    .width(Sizing::GROW)
                    .height(Sizing::GROW),
                |b| {
                    b.text("Hello, World!", TextConfig::new(12.0));
                },
            );
            // Divider.
            b.with(
                El::new()
                    .width(Sizing::GROW)
                    .height(Sizing::fixed(2.0))
                    .background(Color::srgb_u8(60, 130, 180)),
                |_| {},
            );
            // Lower text — word-wrap test with enough text to break.
            b.text(
                "The quick brown fox jumps over the lazy dog. MSDF rendering at any scale.",
                TextConfig::new(8.0),
            );
        },
    );
    builder.build()
}

fn on_mesh_clicked(click: On<Pointer<Click>>, mut commands: Commands) {
    let camera = click.hit.camera;
    commands.trigger(
        ZoomToFit::new(camera, click.entity)
            .margin(ZOOM_MARGIN_MESH)
            .duration(Duration::from_millis(ZOOM_DURATION_MS)),
    );
}

fn on_ground_clicked(
    click: On<Pointer<Click>>,
    mut commands: Commands,
    scene: Res<SceneBounds>,
) {
    let camera = click.hit.camera;
    commands.trigger(
        ZoomToFit::new(camera, scene.0)
            .margin(ZOOM_MARGIN_SCENE)
            .duration(Duration::from_millis(ZOOM_DURATION_MS)),
    );
}
