//! @generated `bevy_example_template`
//! Font loading validation.
//!
//! Demonstrates loading fonts via `AssetServer` and observing
//! `FontRegistered` / `FontLoadFailed` events. The embedded `JetBrains
//! Mono` font is available at startup; `Noto Sans` loads asynchronously
//! from the assets directory. A `DiegeticPanel` shows registered fonts as
//! they arrive, and `WorldText` labels render each font name in its own font.

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
use bevy_diegetic::Font;
use bevy_diegetic::FontLoadFailed;
use bevy_diegetic::FontRegistered;
use bevy_diegetic::GlyphShadowMode;
use bevy_diegetic::LayoutBuilder;
use bevy_diegetic::LayoutTextStyle;
use bevy_diegetic::LayoutTree;
use bevy_diegetic::Padding;
use bevy_diegetic::Sizing;
use bevy_diegetic::Unit;
use bevy_diegetic::WorldText;
use bevy_diegetic::WorldTextStyle;
use bevy_kana::ToF32;
use bevy_lagrange::LagrangePlugin;
use bevy_lagrange::OrbitCam;
use bevy_lagrange::TrackpadBehavior;
use bevy_lagrange::ZoomToFit;
use bevy_window_manager::WindowManagerPlugin;

const ZOOM_MARGIN_TEXT: f32 = 0.3;
const ZOOM_MARGIN_SCENE: f32 = 0.08;
const ZOOM_DURATION_MS: u64 = 1000;

/// Font size for the sample text.
const SAMPLE_SIZE: f32 = 0.28;

/// Vertical spacing between font samples.
const LINE_SPACING: f32 = 0.5;

/// Layout dimensions for the status panel (in meters).
const STATUS_LAYOUT_WIDTH: f32 = 0.08;
const STATUS_LAYOUT_HEIGHT: f32 = 0.03;

/// Font size for the status panel text (in millimeters).
const STATUS_FONT_SIZE: f32 = 3.5;

/// Background color for panels.
const PANEL_BG: Color = Color::srgba(0.1, 0.1, 0.12, 0.85);

/// Border color for panels.
const PANEL_BORDER_COLOR: Color = Color::srgb(0.4, 0.4, 0.45);

/// Tracks how many fonts have been registered (for vertical positioning).
#[derive(Resource, Default)]
struct FontCount(usize);

/// Keeps font handles alive so Bevy doesn't unload the assets.
#[derive(Resource, Default)]
struct FontHandles(Vec<Handle<Font>>);

/// Marker for the status panel.
#[derive(Component)]
struct StatusPanel;

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
        .init_resource::<FontCount>()
        .init_resource::<FontHandles>()
        .add_observer(on_font_registered)
        .add_observer(on_font_load_failed)
        .add_systems(Startup, setup)
        .run();
}

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    asset_server: Res<AssetServer>,
    mut font_handles: ResMut<FontHandles>,
) {
    // Ground plane.
    let ground = commands
        .spawn((
            Mesh3d(meshes.add(Plane3d::default().mesh().size(12.0, 12.0))),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: Color::srgba(0.08, 0.08, 0.08, 0.8),
                alpha_mode: AlphaMode::Blend,
                double_sided: true,
                cull_mode: None,
                ..default()
            })),
        ))
        .observe(on_ground_clicked)
        .id();

    commands.insert_resource(SceneBounds(ground));

    // Load Noto Sans asynchronously from assets directory.
    font_handles
        .0
        .push(asset_server.load("fonts/NotoSans-Regular.ttf"));

    // Try loading a font that doesn't exist to test FontLoadFailed.
    font_handles
        .0
        .push(asset_server.load("fonts/DoesNotExist.ttf"));

    // Light.
    commands.spawn((
        DirectionalLight {
            shadows_enabled: false,
            illuminance: 5000.0,
            ..default()
        },
        Transform::from_xyz(2.0, 6.0, 8.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    commands.spawn((
        DirectionalLight {
            shadows_enabled: false,
            ..default()
        },
        Transform::from_xyz(-2.0, 6.0, -8.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));

    // Camera.
    commands.spawn((
        AmbientLight {
            color:                      Color::WHITE,
            brightness:                 300.0,
            affects_lightmapped_meshes: false,
        },
        OrbitCam {
            focus: Vec3::new(0.0, 2.0, 0.0),
            radius: Some(5.0),
            yaw: Some(0.0),
            pitch: Some(-0.1),
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
        },
        Msaa::Off,
        bevy::anti_alias::taa::TemporalAntiAliasing::default(),
    ));

    // Status panel.
    commands.spawn((
        StatusPanel,
        DiegeticPanel {
            tree: build_status_panel("Fonts: loading..."),
            width: STATUS_LAYOUT_WIDTH,
            height: STATUS_LAYOUT_HEIGHT,
            font_unit: Some(Unit::Millimeters),
            ..default()
        },
        Transform::from_xyz(-1.54, 3.515, 0.0),
    ));
}

fn build_status_panel(text: &str) -> LayoutTree {
    let mut builder = LayoutBuilder::new(STATUS_LAYOUT_WIDTH, STATUS_LAYOUT_HEIGHT);
    builder.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::FIT)
            .padding(Padding::all(0.002))
            .direction(Direction::TopToBottom)
            .child_gap(0.001)
            .background(PANEL_BG)
            .border(Border::all(0.0005, PANEL_BORDER_COLOR)),
        |b| {
            b.text(
                text,
                LayoutTextStyle::new(STATUS_FONT_SIZE)
                    .with_color(Color::srgba(1.0, 1.0, 1.0, 0.9))
                    .with_shadow_mode(GlyphShadowMode::None),
            );
        },
    );
    builder.build()
}

/// Observer: fires when a font is successfully registered.
fn on_font_registered(
    trigger: On<FontRegistered>,
    mut font_count: ResMut<FontCount>,
    mut panels: Query<&mut DiegeticPanel, With<StatusPanel>>,
    mut commands: Commands,
) {
    let idx = font_count.0;
    font_count.0 += 1;

    let y = idx.to_f32().mul_add(LINE_SPACING, 1.5);

    // Spawn a WorldText label in this font.
    let label = format!(
        "[FontId {}] {} ({})",
        trigger.id.0, trigger.name, trigger.source
    );
    commands
        .spawn((
            WorldText::new(label),
            WorldTextStyle::new(SAMPLE_SIZE)
                .with_font(trigger.id.0)
                .with_color(Color::srgb(0.2, 0.3, 0.9))
                .with_shadow_mode(GlyphShadowMode::None),
            Transform::from_xyz(0.0, y, 0.0),
        ))
        .observe(on_text_clicked);

    // Update status panel.
    let status = format!("Fonts registered: {}", font_count.0);
    for mut panel in &mut panels {
        panel.tree = build_status_panel(&status);
    }

    info!(
        "FontRegistered: {} (id: {}, {})",
        trigger.name, trigger.id.0, trigger.source
    );
}

/// Observer: fires when a font fails to load.
fn on_font_load_failed(
    trigger: On<FontLoadFailed>,
    mut panels: Query<&mut DiegeticPanel, With<StatusPanel>>,
) {
    warn!("FontLoadFailed: {} — {}", trigger.path, trigger.error);

    let status = format!("FAILED: {}", trigger.path);
    for mut panel in &mut panels {
        panel.tree = build_status_panel(&status);
    }
}

fn on_text_clicked(
    mut click: On<Pointer<Click>>,
    children: Query<&Children>,
    meshes_q: Query<(), With<Mesh3d>>,
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
        .and_then(|kids| kids.iter().find(|&kid| meshes_q.contains(kid)))
        .unwrap_or(click.entity);
    commands.trigger(
        ZoomToFit::new(camera, target)
            .margin(ZOOM_MARGIN_TEXT)
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
