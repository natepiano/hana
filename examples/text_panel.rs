//! @generated `bevy_example_template`
//! MSDF text rendering test — Phase 3 visual validation gate.
//!
//! Same scene as the default scaffold (ground plane, light, orbit camera)
//! but with the cube replaced by a diegetic panel with MSDF-rendered text.

use std::time::Duration;

use bevy::diagnostic::DiagnosticsStore;
use bevy::diagnostic::FrameTimeDiagnosticsPlugin;
use bevy::picking::mesh_picking::MeshPickingPlugin;
use bevy::prelude::*;
use bevy_brp_extras::BrpExtrasPlugin;
use bevy_brp_extras::PortDisplay;
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
const FPS_UPDATE_INTERVAL: f32 = 1.0;
const LAYOUT_WIDTH: f32 = 160.0;
const LAYOUT_HEIGHT: f32 = 120.0;
const TITLE_FONT_SIZE: f32 = 12.0;
const BODY_FONT_SIZE: f32 = 8.0;
const CONTROLS_LAYOUT_WIDTH: f32 = 120.0;
const CONTROLS_LAYOUT_HEIGHT: f32 = 30.0;
const CONTROLS_WORLD_WIDTH: f32 = 0.8;
const CONTROLS_WORLD_HEIGHT: f32 = 0.2;
const CONTROLS_FONT_SIZE: f32 = 8.0;

#[derive(Resource)]
struct SceneBounds(Entity);

#[derive(Component)]
struct FpsPanel;

#[derive(Component)]
struct ControlsPanel;

#[derive(Resource)]
struct FpsState {
    timer:    Timer,
    fps:      String,
    frame_ms: String,
}

impl Default for FpsState {
    fn default() -> Self {
        Self {
            timer:    Timer::from_seconds(FPS_UPDATE_INTERVAL, TimerMode::Repeating),
            fps:      "--".to_string(),
            frame_ms: "--".to_string(),
        }
    }
}

fn main() {
    App::new()
        .add_plugins((
            DefaultPlugins,
            DiegeticUiPlugin,
            FrameTimeDiagnosticsPlugin::default(),
            PanOrbitCameraPlugin,
            PanOrbitCameraExtPlugin,
            BrpExtrasPlugin::default().port_in_title(PortDisplay::NonDefault),
            WindowManagerPlugin,
            MeshPickingPlugin,
        ))
        .init_resource::<FpsState>()
        .add_systems(Startup, setup)
        .add_systems(
            Update,
            (
                update_fps_panel,
                toggle_debug_gizmos,
                billboard_controls_panel,
            ),
        )
        .run();
}

#[allow(clippy::too_many_arguments)]
fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    fps_state: Res<FpsState>,
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
    let tree = build_panel(&fps_state.fps, &fps_state.frame_ms);
    commands
        .spawn((
            FpsPanel,
            DiegeticPanel {
                tree,
                layout_width: LAYOUT_WIDTH,
                layout_height: LAYOUT_HEIGHT,
                world_width: panel_w,
                world_height: panel_h,
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

    // Controls panel — bottom-left, billboards toward camera.
    commands.spawn((
        ControlsPanel,
        DiegeticPanel {
            tree:          build_controls_panel(),
            layout_width:  CONTROLS_LAYOUT_WIDTH,
            layout_height: CONTROLS_LAYOUT_HEIGHT,
            world_width:   CONTROLS_WORLD_WIDTH,
            world_height:  CONTROLS_WORLD_HEIGHT,
        },
        Transform::from_xyz(-2.0, 2.5, 0.5),
    ));

    // Camera.
    commands.spawn((PanOrbitCamera {
        focus: Vec3::new(-0.13, 1.55, -0.12),
        radius: Some(4.22),
        yaw: Some(-0.01),
        pitch: Some(0.02),
        button_orbit: MouseButton::Middle,
        button_pan: MouseButton::Middle,
        modifier_pan: Some(KeyCode::ShiftLeft),
        trackpad_behavior: TrackpadBehavior::BlenderLike {
            modifier_pan:  Some(KeyCode::ShiftLeft),
            modifier_zoom: Some(KeyCode::ControlLeft),
        },
        trackpad_sensitivity: 0.5,
        trackpad_pinch_to_zoom_enabled: true,
        ..default()
    },));
}

fn build_panel(fps: &str, frame_ms: &str) -> bevy_diegetic::LayoutTree {
    let mut builder = LayoutBuilder::new(LAYOUT_WIDTH, LAYOUT_HEIGHT);
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
            b.with(El::new().width(Sizing::GROW).height(Sizing::GROW), |b| {
                b.text("Hello, World!", TextConfig::new(TITLE_FONT_SIZE));
            });
            // Divider.
            b.with(
                El::new()
                    .width(Sizing::GROW)
                    .height(Sizing::fixed(2.0))
                    .background(Color::srgb_u8(60, 130, 180)),
                |_| {},
            );
            // FPS row.
            key_value_row(b, "FPS", fps);
            // Frame ms row.
            key_value_row(b, "frame ms", frame_ms);
            // Static rows.
            key_value_row(b, "renderer:", "msdf");
            key_value_row(b, "engine:", "diegetic");
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
                TextConfig::new(BODY_FONT_SIZE),
            );
        },
    );
    builder.build()
}

fn key_value_row(b: &mut LayoutBuilder, label: &str, value: &str) {
    b.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::FIT)
            .direction(Direction::LeftToRight),
        |b| {
            b.text(label, TextConfig::new(BODY_FONT_SIZE));
            b.with(
                El::new().width(Sizing::GROW).height(Sizing::fixed(1.0)),
                |_| {},
            );
            b.text(value, TextConfig::new(BODY_FONT_SIZE));
        },
    );
}

fn update_fps_panel(
    time: Res<Time>,
    diagnostics: Res<DiagnosticsStore>,
    mut fps_state: ResMut<FpsState>,
    mut panels: Query<&mut DiegeticPanel, With<FpsPanel>>,
) {
    fps_state.timer.tick(time.delta());
    if fps_state.timer.just_finished() {
        let fps = diagnostics
            .get(&FrameTimeDiagnosticsPlugin::FPS)
            .and_then(bevy::diagnostic::Diagnostic::smoothed);
        let frame_ms = diagnostics
            .get(&FrameTimeDiagnosticsPlugin::FRAME_TIME)
            .and_then(bevy::diagnostic::Diagnostic::smoothed);
        fps_state.fps = fps.map_or_else(|| "--".to_string(), |v| format!("{v:.0}"));
        fps_state.frame_ms = frame_ms.map_or_else(|| "--".to_string(), |v| format!("{v:.1}"));
        for mut panel in &mut panels {
            panel.tree = build_panel(&fps_state.fps, &fps_state.frame_ms);
        }
    }
}

fn build_controls_panel() -> bevy_diegetic::LayoutTree {
    let mut builder = LayoutBuilder::new(CONTROLS_LAYOUT_WIDTH, CONTROLS_LAYOUT_HEIGHT);
    builder.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::GROW)
            .padding(Padding::all(4.0))
            .direction(Direction::TopToBottom)
            .child_gap(2.0)
            .background(Color::srgb_u8(30, 34, 42))
            .border(Border::all(1.0, Color::srgb_u8(80, 90, 100))),
        |b| {
            b.text("'D' toggle debug", TextConfig::new(CONTROLS_FONT_SIZE));
        },
    );
    builder.build()
}

fn billboard_controls_panel(
    camera: Query<&GlobalTransform, With<Camera3d>>,
    mut panels: Query<&mut Transform, With<ControlsPanel>>,
) {
    let Ok(camera_gt) = camera.single() else {
        return;
    };
    let camera_pos = camera_gt.translation();
    for mut transform in &mut panels {
        let direction = camera_pos - transform.translation;
        if direction.length_squared() > f32::EPSILON {
            transform.look_to(-direction, Vec3::Y);
        }
    }
}

fn toggle_debug_gizmos(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut show_text: ResMut<bevy_diegetic::ShowTextGizmos>,
) {
    if keyboard.just_pressed(KeyCode::KeyD) {
        show_text.0 = !show_text.0;
    }
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
