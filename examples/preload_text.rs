//! @generated `bevy_example_template`
//! Preloaded vs progressive glyph loading.
//!
//! MSDF glyph rasterization is asynchronous — the first time a character
//! is rendered, it must be rasterized in the background before it can
//! appear on screen. This example demonstrates two strategies for
//! handling that delay:
//!
//! # `GlyphLoadingPolicy::WhenReady` (default)
//!
//! Text stays invisible until **every** glyph has been rasterized, then
//! appears all at once. Combined with [`MsdfAtlas::preload`], you can
//! warm the atlas for a known character set in a [`FontRegistered`]
//! observer so the text is ready before it's ever spawned. This is the
//! recommended approach for UI text, labels, and anything where partial
//! rendering would look broken.
//!
//! # `GlyphLoadingPolicy::Progressive`
//!
//! Glyphs render as soon as they're available. Missing glyphs are
//! skipped, so text may appear with visible holes that fill in over
//! a few frames. Useful for debug overlays or situations where showing
//! *something* immediately is more important than visual polish.
//!
//! # What this example shows
//!
//! - **Green text** (top): embedded `JetBrains Mono`, preloaded via `atlas.preload()` in the
//!   observer — appears instantly.
//! - **Red text** (bottom): `Noto Sans` loaded async via `AssetServer`, no preload, progressive
//!   policy — glyphs pop in as they rasterize.

use std::time::Duration;

use bevy::picking::mesh_picking::MeshPickingPlugin;
use bevy::prelude::*;
use bevy_brp_extras::BrpExtrasPlugin;
use bevy_brp_extras::PortDisplay;
use bevy_diegetic::DiegeticUiPlugin;
use bevy_diegetic::Font;
use bevy_diegetic::FontRegistered;
use bevy_diegetic::FontRegistry;
use bevy_diegetic::FontSource;
use bevy_diegetic::GlyphLoadingPolicy;
use bevy_diegetic::GlyphShadowMode;
use bevy_diegetic::MsdfAtlas;
use bevy_diegetic::TextScale;
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
const SAMPLE_SIZE: f32 = 24.0;

/// Font size for headers.
const HEADER_SIZE: f32 = 14.0;

/// The text to display.
const SAMPLE_TEXT: &str = "The quick brown fox jumps over the lazy dog. 0123456789!?";

/// Keeps the Noto Sans handle alive so Bevy doesn't unload it.
#[derive(Resource, Default)]
struct FontHandles(Vec<Handle<Font>>);

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
        .insert_resource(TextScale(0.01))
        .init_resource::<FontHandles>()
        .add_observer(on_font_registered)
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
    // Load Noto Sans asynchronously.
    font_handles
        .0
        .push(asset_server.load("fonts/NotoSans-Regular.ttf"));

    // Ground plane.
    let ground = commands
        .spawn((
            Mesh3d(meshes.add(Plane3d::default().mesh().size(12.0, 12.0))),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: Color::srgba(0.3, 0.5, 0.3, 0.8),
                alpha_mode: AlphaMode::Blend,
                double_sided: true,
                cull_mode: None,
                ..default()
            })),
        ))
        .observe(on_ground_clicked)
        .id();

    commands.insert_resource(SceneBounds(ground));

    // Light.
    commands.spawn((
        DirectionalLight {
            shadows_enabled: false,
            illuminance: 5000.0,
            ..default()
        },
        Transform::from_xyz(2.0, 6.0, 8.0).looking_at(Vec3::new(0.0, 2.0, 0.0), Vec3::Y),
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
            radius: Some(8.0),
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
}

/// Embedded font → preloaded + `WhenReady`.
/// Loaded font → no preload + `Progressive`.
fn on_font_registered(
    trigger: On<FontRegistered>,
    mut atlas: ResMut<MsdfAtlas>,
    registry: Res<FontRegistry>,
    mut commands: Commands,
) {
    let font_id = trigger.id;
    let is_embedded = trigger.source == FontSource::Embedded;

    let (label, color, y, policy) = if is_embedded {
        // Preload the embedded font's glyphs.
        atlas.preload(SAMPLE_TEXT, font_id, &registry);
        (
            format!("preloaded ({}): {SAMPLE_TEXT}", trigger.name),
            Color::srgb(0.2, 0.8, 0.3),
            2.0,
            GlyphLoadingPolicy::WhenReady,
        )
    } else {
        // Loaded font — no preload, progressive rendering.
        (
            format!("progressive ({}): {SAMPLE_TEXT}", trigger.name),
            Color::srgb(0.8, 0.3, 0.2),
            1.0,
            GlyphLoadingPolicy::Progressive,
        )
    };

    // Header.
    commands
        .spawn((
            WorldText::new(format!(
                "{} — {} (FontId {})",
                if is_embedded {
                    "preloaded"
                } else {
                    "progressive"
                },
                trigger.name,
                font_id.0
            )),
            WorldTextStyle::new()
                .with_size(HEADER_SIZE)
                .with_font(font_id.0)
                .with_color(Color::srgb(0.6, 0.6, 0.6))
                .with_shadow_mode(GlyphShadowMode::None)
                .with_loading_policy(policy),
            Transform::from_xyz(0.0, y + 0.4, 0.0),
        ))
        .observe(on_text_clicked);

    // Sample text.
    commands
        .spawn((
            WorldText::new(label),
            WorldTextStyle::new()
                .with_size(SAMPLE_SIZE)
                .with_font(font_id.0)
                .with_color(color)
                .with_shadow_mode(GlyphShadowMode::None)
                .with_loading_policy(policy),
            Transform::from_xyz(0.0, y, 0.0),
        ))
        .observe(on_text_clicked);

    info!(
        "FontRegistered: {} ({}) — {}",
        trigger.name,
        trigger.source,
        if is_embedded {
            "preloaded"
        } else {
            "progressive"
        }
    );
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
