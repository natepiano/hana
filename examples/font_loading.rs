//! @generated `bevy_example_template`
//! Font loading validation.
//!
//! Demonstrates loading fonts via `AssetServer` and observing
//! `FontRegistered` / `FontLoadFailed` events. The embedded `JetBrains
//! Mono` font is available at startup; `Noto Sans` loads asynchronously
//! from the assets directory. A HUD shows registered fonts as they
//! arrive, and `WorldText` labels render each font name in its own font.

use std::time::Duration;

use bevy::picking::mesh_picking::MeshPickingPlugin;
use bevy::prelude::*;
use bevy_brp_extras::BrpExtrasPlugin;
use bevy_brp_extras::PortDisplay;
use bevy_diegetic::DiegeticUiPlugin;
use bevy_diegetic::Font;
use bevy_diegetic::FontLoadFailed;
use bevy_diegetic::FontRegistered;
use bevy_diegetic::GlyphShadowMode;
use bevy_diegetic::Unit;
use bevy_diegetic::UnitConfig;
use bevy_diegetic::WorldText;
use bevy_diegetic::WorldTextStyle;
use bevy_panorbit_camera::PanOrbitCamera;
use bevy_panorbit_camera::PanOrbitCameraPlugin;
use bevy_panorbit_camera::TrackpadBehavior;
use bevy_panorbit_camera_ext::PanOrbitCameraExtPlugin;
use bevy_panorbit_camera_ext::ZoomToFit;
use bevy_window_manager::WindowManagerPlugin;

const ZOOM_MARGIN_TEXT: f32 = 0.3;
const ZOOM_MARGIN_SCENE: f32 = 0.08;
const ZOOM_DURATION_MS: u64 = 1000;

/// Font size for the sample text.
const SAMPLE_SIZE: f32 = 28.0;

/// Vertical spacing between font samples.
const LINE_SPACING: f32 = 0.5;

/// Tracks how many fonts have been registered (for vertical positioning).
#[derive(Resource, Default)]
struct FontCount(usize);

/// Keeps font handles alive so Bevy doesn't unload the assets.
#[derive(Resource, Default)]
struct FontHandles(Vec<Handle<Font>>);

/// Marker for the HUD text.
#[derive(Component)]
struct HudText;

#[derive(Resource)]
struct SceneBounds(Entity);

fn main() {
    App::new()
        .add_plugins((
            DefaultPlugins,
            DiegeticUiPlugin,
            PanOrbitCameraPlugin,
            PanOrbitCameraExtPlugin,
            BrpExtrasPlugin::default().port_in_title(PortDisplay::NonDefault),
            WindowManagerPlugin,
            MeshPickingPlugin,
        ))
        .insert_resource(UnitConfig {
            layout: Unit::Meters,
            font:   Unit::Custom(0.01),
        })
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
        PanOrbitCamera {
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
    ));

    // HUD.
    commands.spawn((
        HudText,
        Text::new("Fonts: loading..."),
        TextFont {
            font_size: 16.0,
            ..default()
        },
        TextColor(Color::srgba(1.0, 1.0, 1.0, 0.8)),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(12.0),
            left: Val::Px(12.0),
            ..default()
        },
    ));
}

/// Observer: fires when a font is successfully registered.
#[allow(clippy::cast_precision_loss)]
fn on_font_registered(
    trigger: On<FontRegistered>,
    mut font_count: ResMut<FontCount>,
    mut hud: Query<&mut Text, With<HudText>>,
    mut commands: Commands,
) {
    let idx = font_count.0;
    font_count.0 += 1;

    let y = (idx as f32).mul_add(LINE_SPACING, 1.5);

    // Spawn a WorldText label in this font.
    let label = format!(
        "[FontId {}] {} ({})",
        trigger.id.0, trigger.name, trigger.source
    );
    commands
        .spawn((
            WorldText::new(label),
            WorldTextStyle::new()
                .with_size(SAMPLE_SIZE)
                .with_font(trigger.id.0)
                .with_color(Color::srgb(0.2, 0.3, 0.9))
                .with_shadow_mode(GlyphShadowMode::None),
            Transform::from_xyz(0.0, y, 0.0),
        ))
        .observe(on_text_clicked);

    // Update HUD.
    for mut text in &mut hud {
        **text = format!("Fonts registered: {}", font_count.0);
    }

    info!(
        "FontRegistered: {} (id: {}, {})",
        trigger.name, trigger.id.0, trigger.source
    );
}

/// Observer: fires when a font fails to load.
fn on_font_load_failed(trigger: On<FontLoadFailed>, mut hud: Query<&mut Text, With<HudText>>) {
    warn!("FontLoadFailed: {} — {}", trigger.path, trigger.error);

    for mut text in &mut hud {
        **text = format!("{}\nFAILED: {}", **text, trigger.path);
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
