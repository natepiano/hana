//! @generated `bevy_example_template`
//! MSDF atlas paging validation.
//!
//! Renders printable ASCII characters as `WorldText` entities in a grid,
//! using a small atlas (30 glyphs/page, `Medium` quality) to force
//! overflow onto multiple pages. Press `+` to add blocks of Unicode
//! Latin Extended characters, growing the page count. A HUD overlay
//! shows the atlas config and live diagnostics. Click any character to
//! zoom-to-fit.

use std::time::Duration;

use bevy::picking::mesh_picking::MeshPickingPlugin;
use bevy::prelude::*;
use bevy_brp_extras::BrpExtrasPlugin;
use bevy_brp_extras::PortDisplay;
use bevy_diegetic::DiegeticUiPlugin;
use bevy_diegetic::GlyphShadowMode;
use bevy_diegetic::RasterQuality;
use bevy_diegetic::TextStyle;
use bevy_diegetic::WorldText;
use bevy_panorbit_camera::PanOrbitCamera;
use bevy_panorbit_camera::PanOrbitCameraPlugin;
use bevy_panorbit_camera::TrackpadBehavior;
use bevy_panorbit_camera_ext::PanOrbitCameraExtPlugin;
use bevy_panorbit_camera_ext::ZoomToFit;
use bevy_window_manager::WindowManagerPlugin;

const ZOOM_MARGIN_CHAR: f32 = 0.3;
const ZOOM_MARGIN_SCENE: f32 = 0.08;
const ZOOM_DURATION_MS: u64 = 1000;

/// Font size for the character grid.
const CHAR_SIZE: f32 = 32.0;

/// Columns in the character grid.
const GRID_COLS: usize = 16;

/// Spacing between characters in world units.
const CHAR_SPACING: f32 = 0.35;

/// Atlas config used for this example.
const QUALITY: RasterQuality = RasterQuality::Medium;

/// Glyphs per atlas page.
const GLYPHS_PER_PAGE: u16 = 30;

/// Unicode blocks added by pressing `+`. Each press adds the next block.
const UNICODE_BLOCKS: &[&str] = &[
    // Latin Extended-A (subset)
    "ĀāĂăĄąĆćĈĉĊċČčĎďĐđĒēĔĕĖėĘęĚěĜĝĞğ",
    // Latin Extended-A (continued)
    "ĠġĢģĤĥĦħĨĩĪīĬĭĮįİıĲĳĴĵĶķĸĹĺĻļĽľĿ",
    // Latin Extended-B (subset)
    "ŀŁłŃńŅņŇňŉŊŋŌōŎŏŐőŒœŔŕŖŗŘřŚśŜŝŞş",
    // More Latin Extended-B
    "ŠšŢţŤťŦŧŨũŪūŬŭŮůŰűŲųŴŵŶŷŸŹźŻżŽž",
];

/// Marker for the HUD diagnostics text.
#[derive(Component)]
struct DiagnosticsHud;

/// Tracks which Unicode block to add next.
#[derive(Resource)]
struct NextBlock(usize);

/// Root entity for the character grid.
#[derive(Resource)]
struct GridRoot(Entity);

/// Tracks how many characters have been spawned (for grid positioning).
#[derive(Resource)]
struct CharCount(usize);

#[derive(Resource)]
struct SceneBounds(Entity);

fn main() {
    App::new()
        .add_plugins((
            DefaultPlugins,
            DiegeticUiPlugin::with_atlas()
                .quality(QUALITY)
                .glyphs_per_page(GLYPHS_PER_PAGE),
            PanOrbitCameraPlugin,
            PanOrbitCameraExtPlugin,
            BrpExtrasPlugin::default().port_in_title(PortDisplay::NonDefault),
            WindowManagerPlugin,
            MeshPickingPlugin,
        ))
        .insert_resource(NextBlock(0))
        .add_systems(Startup, setup)
        .add_systems(Update, (handle_input, update_diagnostics))
        .run();
}

#[allow(clippy::cast_precision_loss)]
fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // Ground plane.
    let ground = commands
        .spawn((
            Mesh3d(meshes.add(Plane3d::default().mesh().size(20.0, 20.0))),
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
    commands.insert_resource(GridRoot(ground));

    // Spawn printable ASCII characters.
    let chars: Vec<char> = (33_u8..=126).map(|c| c as char).collect();
    let style = TextStyle::new()
        .with_size(CHAR_SIZE)
        .with_color(Color::srgb(0.15, 0.25, 0.8))
        .with_shadow_mode(GlyphShadowMode::None);

    for (i, &ch) in chars.iter().enumerate() {
        spawn_char_at(&mut commands, ground, ch, i, &style);
    }
    commands.insert_resource(CharCount(chars.len()));

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
            focus: Vec3::new(0.0, 2.5, 0.0),
            radius: Some(6.0),
            yaw: Some(0.0),
            pitch: Some(-0.1),
            button_orbit: MouseButton::Middle,
            button_pan: MouseButton::Middle,
            modifier_pan: Some(KeyCode::ShiftLeft),
            trackpad_behavior: TrackpadBehavior::BlenderLike {
                modifier_pan:  Some(KeyCode::ShiftLeft),
                modifier_zoom: Some(KeyCode::ControlLeft),
            },
            trackpad_pinch_to_zoom_enabled: true,
            ..default()
        },
    ));

    // HUD.
    commands.spawn((
        DiagnosticsHud,
        Text::new("loading..."),
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

/// Spawns a single character `WorldText` at the given grid index.
#[allow(clippy::cast_precision_loss)]
fn spawn_char_at(
    commands: &mut Commands,
    parent: Entity,
    ch: char,
    index: usize,
    style: &TextStyle,
) {
    let col = index % GRID_COLS;
    let row = index / GRID_COLS;
    let grid_width = (GRID_COLS - 1) as f32 * CHAR_SPACING;
    let x = (col as f32).mul_add(CHAR_SPACING, -grid_width * 0.5);
    // Stack rows upward from y=1.0.
    let y = (row as f32).mul_add(CHAR_SPACING, 1.0);

    let entity = commands
        .spawn((
            WorldText::new(String::from(ch)),
            style.clone(),
            Transform::from_xyz(x, y, 0.0),
        ))
        .observe(on_char_clicked)
        .id();
    commands.entity(parent).add_child(entity);
}

/// Handles `+` key to add the next Unicode block.
fn handle_input(
    keys: Res<ButtonInput<KeyCode>>,
    mut next_block: ResMut<NextBlock>,
    mut char_count: ResMut<CharCount>,
    grid_root: Res<GridRoot>,
    mut commands: Commands,
) {
    if !keys.just_pressed(KeyCode::Equal) {
        return;
    }

    if next_block.0 >= UNICODE_BLOCKS.len() {
        return;
    }

    let block = UNICODE_BLOCKS[next_block.0];
    next_block.0 += 1;

    let style = TextStyle::new()
        .with_size(CHAR_SIZE)
        .with_color(Color::srgb(0.8, 0.2, 0.15))
        .with_shadow_mode(GlyphShadowMode::None);

    for ch in block.chars() {
        spawn_char_at(&mut commands, grid_root.0, ch, char_count.0, &style);
        char_count.0 += 1;
    }
}

fn update_diagnostics(
    atlas: Res<bevy_diegetic::MsdfAtlas>,
    mut query: Query<&mut Text, With<DiagnosticsHud>>,
    next_block: Res<NextBlock>,
) {
    let remaining = UNICODE_BLOCKS.len() - next_block.0;
    for mut text in &mut query {
        **text = format!(
            "quality: {QUALITY:?}, ~{GLYPHS_PER_PAGE} glyphs/page (estimate)\n\
             pages: {}, glyphs: {}\n\
             '+' to add Unicode block ({remaining} remaining)",
            atlas.page_count(),
            atlas.glyph_count(),
        );
    }
}

fn on_char_clicked(
    mut click: On<Pointer<Click>>,
    children: Query<&Children>,
    meshes_q: Query<(), With<Mesh3d>>,
    mut commands: Commands,
) {
    click.propagate(false);
    let camera = click.hit.camera;
    let target = children
        .get(click.entity)
        .ok()
        .and_then(|kids| kids.iter().find(|&kid| meshes_q.contains(kid)))
        .unwrap_or(click.entity);
    commands.trigger(
        ZoomToFit::new(camera, target)
            .margin(ZOOM_MARGIN_CHAR)
            .duration(Duration::from_millis(ZOOM_DURATION_MS)),
    );
}

fn on_ground_clicked(click: On<Pointer<Click>>, mut commands: Commands, scene: Res<SceneBounds>) {
    let camera = click.hit.camera;
    commands.trigger(
        ZoomToFit::new(camera, scene.0)
            .margin(ZOOM_MARGIN_SCENE)
            .duration(Duration::from_millis(ZOOM_DURATION_MS)),
    );
}
