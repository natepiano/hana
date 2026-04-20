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
use bevy_brp_extras::PortDisplay;
use bevy_diegetic::Border;
use bevy_diegetic::DiegeticPanel;
use bevy_diegetic::DiegeticUiPlugin;
use bevy_diegetic::Direction;
use bevy_diegetic::El;
use bevy_diegetic::HueOffset;
use bevy_diegetic::LayoutBuilder;
use bevy_diegetic::LayoutTextStyle;
use bevy_diegetic::LayoutTree;
use bevy_diegetic::Padding;
use bevy_diegetic::Sizing;
use bevy_diegetic::Unit;
use bevy_kana::ToF32;
use bevy_lagrange::InputControl;
use bevy_lagrange::LagrangePlugin;
use bevy_lagrange::OrbitCam;
use bevy_lagrange::TrackpadBehavior;
use bevy_lagrange::TrackpadInput;
use bevy_lagrange::ZoomToFit;
use bevy_window_manager::WindowManagerPlugin;

const LAYOUT_WIDTH: f32 = 1.0;
const LAYOUT_HEIGHT: f32 = 0.85;
const FONT_SIZE: f32 = 2.5;
const ROW_COUNT: usize = 10;
const ZOOM_MARGIN_SCENE: f32 = 0.08;
const ZOOM_DURATION_MS: u64 = 1000;

// ── Info panel dimensions (meters) ───────────────────────────────────
const INFO_PANEL_WIDTH: f32 = 0.14;
const INFO_PANEL_HEIGHT: f32 = 0.03;
const INFO_FONT_SIZE: f32 = 3.5;
const INFO_TITLE_FONT_SIZE: f32 = 4.2;

#[derive(Component)]
struct RotatingPanel;

#[derive(Resource)]
struct SceneBounds(Entity);

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
                base_color: Color::srgb(0.08, 0.08, 0.08),
                double_sided: true,
                cull_mode: None,
                ..default()
            })),
        ))
        .observe(on_ground_clicked)
        .id();

    commands.insert_resource(SceneBounds(ground));

    // Dark backdrop — bottom edge sits on the ground plane.
    let panel_height = LAYOUT_HEIGHT;
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
        DiegeticPanel::world()
            .size(LAYOUT_WIDTH, LAYOUT_HEIGHT)
            .font_unit(Unit::Millimeters)
            .with_tree(tree.clone())
            .build()
            .expect("valid panel dimensions"),
        Transform::from_xyz(-1.1, 1.05, 0.0),
    ));

    // Right panel — no hue offset (static colors).
    commands.spawn((
        DiegeticPanel::world()
            .size(LAYOUT_WIDTH, LAYOUT_HEIGHT)
            .font_unit(Unit::Millimeters)
            .with_tree(tree)
            .build()
            .expect("valid panel dimensions"),
        Transform::from_xyz(0.1, 1.05, 0.0),
    ));

    // Directional lights.
    commands.spawn((
        DirectionalLight {
            shadows_enabled: true,
            ..default()
        },
        Transform::from_xyz(0.5, 1.5, 1.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    commands.spawn((
        DirectionalLight {
            shadows_enabled: false,
            ..default()
        },
        Transform::from_xyz(-0.5, 1.5, -1.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));

    // Camera.
    commands.spawn((OrbitCam {
        focus: Vec3::new(0.0, panel_center_y, 0.0),
        radius: Some(3.5),
        yaw: Some(0.0),
        pitch: Some(0.1),
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

    // Info panel — below the two panels.
    commands.spawn((
        DiegeticPanel::world()
            .size(INFO_PANEL_WIDTH, INFO_PANEL_HEIGHT)
            .font_unit(Unit::Millimeters)
            .with_tree(build_info_panel())
            .build()
            .expect("valid info panel dimensions"),
        Transform::from_xyz(-0.07, -0.085, 0.0),
    ));
}

fn build_panel() -> bevy_diegetic::LayoutTree {
    let mut builder = LayoutBuilder::new(LAYOUT_WIDTH, LAYOUT_HEIGHT);
    builder.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::FIT)
            .padding(Padding::all(0.042))
            .direction(Direction::TopToBottom)
            .child_gap(0.017)
            .background(Color::srgb_u8(30, 34, 42))
            .border(Border::all(0.008, Color::srgb_u8(80, 90, 100))),
        |b| {
            for i in 0..ROW_COUNT {
                let hue = 360.0 * (i.to_f32() / ROW_COUNT.to_f32());
                let color = Color::hsl(hue, 0.8, 0.6);
                let config = LayoutTextStyle::new(FONT_SIZE).with_color(color);
                b.with(
                    El::new()
                        .width(Sizing::GROW)
                        .height(Sizing::FIT)
                        .direction(Direction::LeftToRight),
                    |b| {
                        b.text(format!("row {i}:"), config.clone());
                        b.with(
                            El::new().width(Sizing::GROW).height(Sizing::fixed(0.008)),
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

fn build_info_panel() -> LayoutTree {
    let border_color = Color::srgb(0.4, 0.4, 0.45);
    let divider_color = Color::srgb(0.45, 0.45, 0.5);
    let cfg = LayoutTextStyle::new(INFO_FONT_SIZE);
    let title_cfg = LayoutTextStyle::new(INFO_TITLE_FONT_SIZE);

    let mut builder = LayoutBuilder::new(INFO_PANEL_WIDTH, INFO_PANEL_HEIGHT);
    builder.with(
        El::new()
            .width(Sizing::FIT)
            .height(Sizing::FIT)
            .padding(Padding::all(0.002))
            .direction(Direction::TopToBottom)
            .child_gap(0.001)
            .background(Color::srgba(0.1, 0.1, 0.12, 0.85))
            .border(Border::all(0.0005, border_color)),
        |b| {
            b.text(
                "hue offset",
                title_cfg.with_color(Color::srgb(0.4, 0.5, 0.9)),
            );
            b.with(
                El::new()
                    .width(Sizing::GROW)
                    .height(Sizing::fixed(0.0002))
                    .background(divider_color),
                |_| {},
            );
            b.text(
                "Left panel rotates independently - materials are not shared",
                cfg,
            );
        },
    );
    builder.build()
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
