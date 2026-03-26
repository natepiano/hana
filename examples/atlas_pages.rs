//! @generated `bevy_example_template`
//! MSDF atlas paging validation.
//!
//! Renders printable ASCII characters as `WorldText` entities in a grid,
//! using a small atlas (30 glyphs/page, `Medium` quality) to force
//! overflow onto multiple pages. Press `+` to add blocks of Unicode
//! Latin Extended characters, growing the page count. A `DiegeticPanel`
//! overlay shows the atlas config and live diagnostics. Click any
//! character to zoom-to-fit.

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
use bevy_diegetic::GlyphShadowMode;
use bevy_diegetic::LayoutBuilder;
use bevy_diegetic::LayoutTextStyle;
use bevy_diegetic::LayoutTree;
use bevy_diegetic::MsdfAtlas;
use bevy_diegetic::Padding;
use bevy_diegetic::RasterQuality;
use bevy_diegetic::Sizing;
use bevy_diegetic::Unit;
use bevy_diegetic::WorldText;
use bevy_diegetic::WorldTextStyle;
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
const CHAR_SIZE: f32 = 0.32;

/// Columns in the character grid.
const GRID_COLS: usize = 16;

/// Spacing between characters in world units.
const CHAR_SPACING: f32 = 0.35;

/// Atlas config used for this example.
const QUALITY: RasterQuality = RasterQuality::Medium;

/// Glyphs per atlas page.
const GLYPHS_PER_PAGE: u16 = 30;

/// Layout dimensions for the status panel (in meters, matching the scene scale).
const STATUS_LAYOUT_WIDTH: f32 = 2.0;
const STATUS_LAYOUT_HEIGHT: f32 = 0.8;

/// Font sizes for the status panel (in millimeters).
const STATUS_FONT_SIZE: f32 = 40.0;
const STATUS_TITLE_SIZE: f32 = 60.0;

/// Background color for panels.
const PANEL_BG: Color = Color::srgba(0.1, 0.1, 0.12, 0.85);

/// Border color for panels.
const PANEL_BORDER_COLOR: Color = Color::WHITE;

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

/// Marker for the status panel.
#[derive(Component)]
struct StatusPanel;

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

/// Tracks the last displayed status text to avoid unnecessary rebuilds.
#[derive(Resource, Default)]
struct LastDisplayedStatus {
    text: String,
}

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
        .init_resource::<LastDisplayedStatus>()
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
    commands.insert_resource(GridRoot(ground));

    // Spawn printable ASCII characters.
    let chars: Vec<char> = (33_u8..=126).map(|c| c as char).collect();
    let style = WorldTextStyle::new()
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
            trackpad_sensitivity: 0.5,
            trackpad_pinch_to_zoom_enabled: true,
            ..default()
        },
    ));

    // Status panel.
    commands.spawn((
        StatusPanel,
        DiegeticPanel {
            tree: build_status_panel(&StatusData {
                pages:     0,
                glyphs:    0,
                remaining: UNICODE_BLOCKS.len(),
            }),
            width: STATUS_LAYOUT_WIDTH,
            height: STATUS_LAYOUT_HEIGHT,
            font_unit: Some(Unit::Millimeters),
            ..default()
        },
        Transform::from_xyz(-2.5, 4.5, 0.0),
    ));

    // Minimal test panel — mirrors atlas layout structure.
    let mut test_builder = LayoutBuilder::new(2.0, 0.8);
    test_builder.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::GROW)
            .direction(Direction::TopToBottom)
            .child_gap(0.02)
            .background(PANEL_BG)
            .border(Border::all(0.001, Color::WHITE)),
        |_| {},
    );
    commands.spawn((
        DiegeticPanel {
            tree: test_builder.build(),
            width: 2.0,
            height: 0.8,
            ..default()
        },
        Transform::from_xyz(-1.5, 4.5, 0.0),
    ));

}

struct StatusData {
    pages:     usize,
    glyphs:    usize,
    remaining: usize,
}

fn build_status_panel(data: &StatusData) -> LayoutTree {
    let mut builder = LayoutBuilder::new(STATUS_LAYOUT_WIDTH, STATUS_LAYOUT_HEIGHT);
    let label_style = LayoutTextStyle::new(STATUS_FONT_SIZE)
        .with_color(Color::srgba(0.6, 0.6, 0.6, 0.9))
        .with_shadow_mode(GlyphShadowMode::None);
    let value_style = LayoutTextStyle::new(STATUS_FONT_SIZE)
        .with_color(Color::WHITE)
        .with_shadow_mode(GlyphShadowMode::None);
    let title_style = LayoutTextStyle::new(STATUS_TITLE_SIZE)
        .with_color(Color::srgb(0.4, 0.5, 0.9))
        .with_shadow_mode(GlyphShadowMode::None);
    let dim_style = LayoutTextStyle::new(STATUS_FONT_SIZE)
        .with_color(Color::srgba(0.5, 0.5, 0.5, 0.8))
        .with_shadow_mode(GlyphShadowMode::None);

    builder.with(
        El::new()
            .width(Sizing::FIT)
            .height(Sizing::FIT)
            .padding(Padding::all(0.03))
            .direction(Direction::TopToBottom)
            .child_gap(0.02)
            .background(PANEL_BG)
            .border(Border::all(0.001, PANEL_BORDER_COLOR)),
        |b| {
            b.text("atlas", title_style);

            // Divider
            b.with(
                El::new()
                    .width(Sizing::GROW)
                    .height(Sizing::fixed(0.003))
                    .background(Color::srgb(0.45, 0.45, 0.5)),
                |_| {},
            );

            // Label/value rows
            b.with(
                El::new()
                    .width(Sizing::FIT)
                    .height(Sizing::FIT)
                    .direction(Direction::LeftToRight)
                    .child_gap(0.025),
                |b| {
                    // Label column
                    b.with(
                        El::new().direction(Direction::TopToBottom).child_gap(0.01),
                        |b| {
                            b.text("quality", label_style.clone());
                            b.text("glyphs/page", label_style.clone());
                            b.text("pages", label_style.clone());
                            b.text("glyphs", label_style.clone());
                        },
                    );
                    // Value column
                    b.with(
                        El::new().direction(Direction::TopToBottom).child_gap(10.0),
                        |b| {
                            b.text(format!("{QUALITY:?}"), value_style.clone());
                            b.text(format!("~{GLYPHS_PER_PAGE}"), value_style.clone());
                            b.text(format!("{}", data.pages), value_style.clone());
                            b.text(format!("{}", data.glyphs), value_style.clone());
                        },
                    );
                },
            );

            // Instruction
            b.text(
                format!("'+' add Unicode block ({} remaining)", data.remaining),
                dim_style,
            );
        },
    );
    builder.build()
}

/// Spawns a single character `WorldText` at the given grid index.
#[allow(clippy::cast_precision_loss)]
fn spawn_char_at(
    commands: &mut Commands,
    parent: Entity,
    ch: char,
    index: usize,
    style: &WorldTextStyle,
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

    let style = WorldTextStyle::new()
        .with_size(CHAR_SIZE)
        .with_color(Color::srgb(0.8, 0.2, 0.15))
        .with_shadow_mode(GlyphShadowMode::None);

    for ch in block.chars() {
        spawn_char_at(&mut commands, grid_root.0, ch, char_count.0, &style);
        char_count.0 += 1;
    }
}

fn update_diagnostics(
    atlas: Res<MsdfAtlas>,
    mut panels: Query<&mut DiegeticPanel, With<StatusPanel>>,
    next_block: Res<NextBlock>,
    mut last_displayed: ResMut<LastDisplayedStatus>,
) {
    let pages = atlas.page_count();
    let glyphs = atlas.glyph_count();
    let remaining = UNICODE_BLOCKS.len() - next_block.0;
    let fingerprint = format!("{pages}/{glyphs}/{remaining}");

    if fingerprint != last_displayed.text {
        last_displayed.text = fingerprint;
        let data = StatusData {
            pages,
            glyphs,
            remaining,
        };
        for mut panel in &mut panels {
            panel.tree = build_status_panel(&data);
        }
    }
}

fn on_char_clicked(
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
            .margin(ZOOM_MARGIN_CHAR)
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
