//! @generated `bevy_example_template`
//! `HueOffset` material sharing validation.
//!
//! Two panels side by side with identical content. The left panel has a
//! rotating [`HueOffset`]; the right panel has none. Only the left
//! panel's colors should rotate — the right panel stays static.
//!
//! The library automatically shares a single GPU material across all
//! panels for performance. When a panel receives a [`HueOffset`]
//! component, the library transparently splits it onto its own private
//! material so the hue rotation only affects that panel. This happens
//! without any user intervention — just insert `HueOffset` and the
//! framework handles the rest.
//!
//! If both panels rotate, the material splitting is broken.

use std::time::Duration;

use bevy::picking::mesh_picking::MeshPickingPlugin;
use bevy::prelude::*;
use bevy_brp_extras::BrpExtrasPlugin;
use bevy_diegetic::Border;
use bevy_diegetic::DiegeticPanel;
use bevy_diegetic::DiegeticUiPlugin;
use bevy_diegetic::Direction;
use bevy_diegetic::El;
use bevy_diegetic::HueOffset;
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

const LAYOUT_SIZE: f32 = 120.0;
const PANEL_ASPECT: f32 = 0.85;
const FONT_SIZE: f32 = 7.0;
const ROW_COUNT: usize = 10;
const ZOOM_MARGIN_SCENE: f32 = 0.08;
const ZOOM_DURATION_MS: u64 = 1000;

#[derive(Component)]
struct RotatingPanel;

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
        .add_systems(Update, rotate_hue)
        .run();
}

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let tree = build_panel();

    // Ground plane.
    let ground = commands
        .spawn((
            Mesh3d(meshes.add(Plane3d::default().mesh().size(5.0, 5.0))),
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

    // Dark backdrop — bottom edge sits on the ground plane.
    let panel_height = PANEL_ASPECT;
    let panel_center_y = panel_height.mul_add(0.5, 0.2);
    commands.spawn((
        Mesh3d(meshes.add(Rectangle::new(3.0, panel_height + 0.4))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.15, 0.15, 0.15),
            double_sided: true,
            cull_mode: None,
            ..default()
        })),
        Transform::from_xyz(0.0, panel_center_y, -0.5),
    ));

    // Left panel — rotating hue.
    commands.spawn((
        RotatingPanel,
        DiegeticPanel {
            tree:          tree.clone(),
            layout_width:  LAYOUT_SIZE,
            layout_height: LAYOUT_SIZE * PANEL_ASPECT,
            world_width:   1.0,
            world_height:  panel_height,
        },
        Transform::from_xyz(-0.6, panel_center_y, 0.0),
    ));

    // Right panel — no hue offset (static colors).
    commands.spawn((
        DiegeticPanel {
            tree,
            layout_width: LAYOUT_SIZE,
            layout_height: LAYOUT_SIZE * PANEL_ASPECT,
            world_width: 1.0,
            world_height: panel_height,
        },
        Transform::from_xyz(0.6, panel_center_y, 0.0),
    ));

    // Light.
    commands.spawn((
        PointLight {
            intensity: 200_000.0,
            shadows_enabled: true,
            range: 30.0,
            ..default()
        },
        Transform::from_xyz(0.0, panel_center_y, 4.0),
    ));

    // Camera.
    commands.spawn((PanOrbitCamera {
        focus: Vec3::new(0.0, panel_center_y, 0.0),
        radius: Some(3.5),
        yaw: Some(0.0),
        pitch: Some(0.1),
        button_orbit: MouseButton::Middle,
        button_pan: MouseButton::Middle,
        modifier_pan: Some(KeyCode::ShiftLeft),
        trackpad_behavior: TrackpadBehavior::BlenderLike {
            modifier_pan:  Some(KeyCode::ShiftLeft),
            modifier_zoom: Some(KeyCode::ControlLeft),
        },
        trackpad_pinch_to_zoom_enabled: true,
        ..default()
    },));

    // Labels.
    commands.spawn((
        Text::new("Left panel rotates independently — materials are not shared"),
        TextFont {
            font_size: 14.0,
            ..default()
        },
        TextColor(Color::srgba(1.0, 1.0, 1.0, 0.6)),
        Node {
            position_type: PositionType::Absolute,
            bottom: Val::Px(12.0),
            left: Val::Px(12.0),
            ..default()
        },
    ));
}

#[allow(clippy::cast_precision_loss)]
fn build_panel() -> bevy_diegetic::LayoutTree {
    let mut builder = LayoutBuilder::new(LAYOUT_SIZE, LAYOUT_SIZE * PANEL_ASPECT);
    builder.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::FIT)
            .padding(Padding::all(5.0))
            .direction(Direction::TopToBottom)
            .child_gap(2.0)
            .background(Color::srgb_u8(30, 34, 42))
            .border(Border::all(1.0, Color::srgb_u8(80, 90, 100))),
        |b| {
            for i in 0..ROW_COUNT {
                let hue = 360.0 * (i as f32 / ROW_COUNT as f32);
                let color = Color::hsl(hue, 0.8, 0.6);
                let config = TextConfig::new(FONT_SIZE).with_color(color);
                b.with(
                    El::new()
                        .width(Sizing::GROW)
                        .height(Sizing::FIT)
                        .direction(Direction::LeftToRight),
                    |b| {
                        b.text(format!("row {i}:"), config.clone());
                        b.with(
                            El::new().width(Sizing::GROW).height(Sizing::fixed(1.0)),
                            |_| {},
                        );
                        b.text("value", config);
                    },
                );
            }
        },
    );
    builder.build()
}

fn rotate_hue(panels: Query<Entity, With<RotatingPanel>>, mut commands: Commands, time: Res<Time>) {
    let hue = (time.elapsed_secs() * 2.0) % std::f32::consts::TAU;
    for entity in &panels {
        commands.entity(entity).insert(HueOffset(hue));
    }
}

fn on_ground_clicked(click: On<Pointer<Click>>, mut commands: Commands, scene: Res<SceneBounds>) {
    let camera = click.hit.camera;
    commands.trigger(
        ZoomToFit::new(camera, scene.0)
            .margin(ZOOM_MARGIN_SCENE)
            .duration(Duration::from_millis(ZOOM_DURATION_MS)),
    );
}
