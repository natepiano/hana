//! @generated `bevy_example_template`
//! MSDF atlas paging — querying the atlas with [`GlyphKey`] and [`GlyphMetrics`].
//!
//! Demonstrates how to inspect the MSDF texture atlas at runtime:
//!
//! - **Character → glyph ID** via `ttf_parser::Face::glyph_index`
//! - **Glyph ID → atlas page** via [`MsdfAtlas::get_metrics`] with a [`GlyphKey`]
//! - **Atlas page → GPU texture** via [`MsdfAtlas::image_handle`]
//!
//! Renders a tilted grid (Star Wars crawl style) where each cell
//! represents an actual atlas page in memory. The left side of each
//! cell shows the rendered glyphs on that page; the right side shows
//! the raw MSDF atlas texture. Printable ASCII characters are loaded
//! at startup; pressing `+` adds Unicode Latin Extended blocks,
//! growing the atlas. A status panel shows live diagnostics.

use bevy::prelude::*;
use bevy_brp_extras::BrpExtrasPlugin;
use bevy_brp_extras::PortDisplay;
use bevy_diegetic::AlignX;
use bevy_diegetic::AlignY;
use bevy_diegetic::Anchor;
use bevy_diegetic::Border;
use bevy_diegetic::DiegeticPanel;
use bevy_diegetic::DiegeticUiPlugin;
use bevy_diegetic::Direction;
use bevy_diegetic::El;
use bevy_diegetic::GlyphShadowMode;
use bevy_diegetic::LayoutBuilder;
use bevy_diegetic::LayoutTextStyle;
use bevy_diegetic::MsdfAtlas;
use bevy_diegetic::Padding;
use bevy_diegetic::RasterQuality;
use bevy_diegetic::Sizing;
use bevy_diegetic::Unit;
use bevy_panorbit_camera::PanOrbitCamera;
use bevy_panorbit_camera::PanOrbitCameraPlugin;
use bevy_panorbit_camera::TrackpadBehavior;
use bevy_panorbit_camera_ext::PanOrbitCameraExtPlugin;
use bevy_window_manager::WindowManagerPlugin;

// ── Grid geometry ──────────────────────────────────────────────────

/// Columns in the page grid.
const GRID_COLUMNS: usize = 4;

/// Rows in the page grid.
const GRID_ROWS: usize = 2;

/// Grid layout width in points (2:1 aspect ratio → square cells).
const GRID_LAYOUT_WIDTH: f32 = 1000.0;

/// Grid layout height in points.
const GRID_LAYOUT_HEIGHT: f32 = 500.0;

/// World-space height of the grid in meters.
const GRID_WORLD_HEIGHT: f32 = 5.0;

/// Tilt angle from vertical, in degrees. Higher values recline
/// the grid more (0 = upright, 90 = flat on the ground).
const TILT_DEGREES: f32 = 55.0;

// ── Atlas config ───────────────────────────────────────────────────

/// Atlas rasterization quality.
const ATLAS_QUALITY: RasterQuality = RasterQuality::Medium;

/// Glyphs per atlas page.
const GLYPHS_PER_PAGE: u16 = 30;

/// Characters per row inside each glyph cell.
const GLYPHS_PER_ROW: usize = 6;

// ── Visual style ───────────────────────────────────────────────────

/// Font size for glyphs in cells (points).
const GLYPH_FONT_SIZE: f32 = 30.0;

/// Extra letter spacing between glyphs (points).
const GLYPH_LETTER_SPACING: f32 = 6.0;

/// Font size for page number labels (points).
const PAGE_LABEL_SIZE: f32 = 18.0;

/// Grid outer border width (points).
const GRID_BORDER_WIDTH: f32 = 0.5;

/// Grid interior padding (points).
const GRID_INTERIOR_PADDING: f32 = 2.0;

/// Divider width between cells and rows (points).
const CELL_DIVIDER_WIDTH: f32 = 0.5;

/// Fraction of cell height used for glyph panels (provides margin).
const GLYPH_PANEL_SCALE: f32 = 0.88;

const GRID_BORDER_COLOR: Color = Color::srgba(0.5, 0.55, 0.7, 0.6);
const GRID_BACKGROUND_COLOR: Color = Color::srgba(0.06, 0.06, 0.06, 0.85);
const PAGE_LABEL_COLOR: Color = Color::srgba(0.4, 0.4, 0.5, 0.5);
const ASCII_GLYPH_COLOR: Color = Color::srgb(0.15, 0.25, 0.8);
const UNICODE_GLYPH_COLOR: Color = Color::srgb(0.8, 0.2, 0.15);

// ── Status panel ───────────────────────────────────────────────────

const STATUS_LAYOUT_WIDTH: f32 = 400.0;
const STATUS_LAYOUT_HEIGHT: f32 = 280.0;
const STATUS_WORLD_HEIGHT: f32 = 1.8;
const STATUS_FONT_SIZE: f32 = 18.0;
const STATUS_TITLE_SIZE: f32 = 28.0;
const STATUS_BACKGROUND: Color = Color::srgb(1.0, 1.0, 0.0);
const STATUS_BORDER_COLOR: Color = Color::WHITE;

// ── Unicode blocks ─────────────────────────────────────────────────

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

/// Number of printable ASCII characters (U+0021–U+007E).
const ASCII_CHARACTER_COUNT: usize = 94;

// ── Components ─────────────────────────────────────────────────────

/// Marker for the grid overlay panel.
#[derive(Component)]
struct GridOverlay;

/// Marks a glyph panel.
#[derive(Component)]
struct GlyphCell;

/// Marker for the status panel.
#[derive(Component)]
struct StatusPanel;

// ── Resources ──────────────────────────────────────────────────────

/// Root entity that tilts the entire grid scene.
#[derive(Resource)]
struct TiltRoot(Entity);

/// Tracks which Unicode block to add next.
#[derive(Resource)]
struct NextBlock(usize);

/// All characters accumulated so far.
#[derive(Resource)]
struct AccumulatedCharacters(Vec<char>);

/// Entity handles for spawned glyph panels, indexed by page.
#[derive(Resource)]
struct GlyphPanelEntities(Vec<Option<Entity>>);

/// Deduplicates status panel updates.
#[derive(Resource, Default)]
struct LastDisplayedStatus {
    fingerprint: String,
}

// ── Main ───────────────────────────────────────────────────────────

fn main() {
    App::new()
        .add_plugins((
            DefaultPlugins,
            DiegeticUiPlugin::with_atlas()
                .quality(ATLAS_QUALITY)
                .glyphs_per_page(GLYPHS_PER_PAGE),
            PanOrbitCameraPlugin,
            PanOrbitCameraExtPlugin,
            BrpExtrasPlugin::default().port_in_title(PortDisplay::NonDefault),
            WindowManagerPlugin,
        ))
        .insert_resource(NextBlock(0))
        .init_resource::<LastDisplayedStatus>()
        .add_systems(Startup, setup)
        .add_systems(Update, (handle_input, update_diagnostics))
        .run();
}

// ── Setup ──────────────────────────────────────────────────────────

fn setup(mut commands: Commands) {
    let total_pages = total_page_count();

    // Tilted root — Star Wars crawl angle.
    let tilt_radians = TILT_DEGREES.to_radians();
    let tilt_root = commands
        .spawn(Transform::from_rotation(Quat::from_rotation_x(
            -tilt_radians,
        )))
        .id();
    commands.insert_resource(TiltRoot(tilt_root));

    // Grid overlay panel (cell outlines + page labels).
    let grid_entity = commands
        .spawn((GridOverlay, build_grid_panel(), Transform::IDENTITY))
        .id();
    commands.entity(tilt_root).add_child(grid_entity);

    // Fill initial ASCII characters.
    let ascii_chars: Vec<char> = (33_u8..=126).map(|c| c as char).collect();
    let mut panel_entities = GlyphPanelEntities(vec![None; total_pages]);
    rebuild_glyph_panels(&mut commands, tilt_root, &ascii_chars, &mut panel_entities);

    commands.insert_resource(AccumulatedCharacters(ascii_chars));
    commands.insert_resource(panel_entities);

    // Lighting.
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
            focus: Vec3::new(0.0, 0.5, 0.0),
            radius: Some(9.0),
            yaw: Some(0.0),
            pitch: Some(-0.15),
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

    // Status panel (floating in world, outside the tilt hierarchy).
    commands.spawn((
        StatusPanel,
        build_status_panel(&StatusData {
            pages:        0,
            glyphs:       0,
            remaining:    UNICODE_BLOCKS.len(),
            cells_filled: cells_filled_count(&(33_u8..=126).map(|c| c as char).collect::<Vec<_>>()),
            total_cells:  total_pages,
        }),
        Transform::from_xyz(-4.5, 3.5, 2.0),
    ));
}

// ── Grid geometry helpers ──────────────────────────────────────────

fn grid_world_width() -> f32 { GRID_WORLD_HEIGHT * (GRID_LAYOUT_WIDTH / GRID_LAYOUT_HEIGHT) }

#[allow(clippy::cast_precision_loss)]
fn cell_world_width() -> f32 { grid_world_width() / GRID_COLUMNS as f32 }

#[allow(clippy::cast_precision_loss)]
fn cell_world_height() -> f32 { GRID_WORLD_HEIGHT / GRID_ROWS as f32 }

fn total_page_count() -> usize {
    let unicode_count: usize = UNICODE_BLOCKS.iter().map(|b| b.chars().count()).sum();
    let total = ASCII_CHARACTER_COUNT + unicode_count;
    total.div_ceil(GLYPHS_PER_PAGE as usize)
}

const fn cells_filled_count(chars: &[char]) -> usize {
    if chars.is_empty() {
        return 0;
    }
    chars.len().div_ceil(GLYPHS_PER_PAGE as usize)
}

/// World position for the center of cell `(row, col)` relative to the
/// grid panel with [`Anchor::Center`].
#[allow(clippy::cast_precision_loss)]
fn cell_center_position(row: usize, col: usize) -> Vec3 {
    let cw = cell_world_width();
    let ch = cell_world_height();
    let x = (GRID_COLUMNS as f32).mul_add(-0.5, col as f32 + 0.5) * cw;
    let y = (GRID_ROWS as f32).mul_add(0.5, -(row as f32) - 0.5) * ch;
    Vec3::new(x, y, 0.002)
}

// ── Grid overlay panel ─────────────────────────────────────────────

fn build_grid_panel() -> DiegeticPanel {
    DiegeticPanel::builder()
        .size((GRID_LAYOUT_WIDTH, GRID_LAYOUT_HEIGHT))
        .layout_unit(Unit::Points)
        .world_height(GRID_WORLD_HEIGHT)
        .anchor(Anchor::Center)
        .layout(|b| {
            b.with(
                El::new()
                    .width(Sizing::GROW)
                    .height(Sizing::GROW)
                    .padding(Padding::all(GRID_INTERIOR_PADDING))
                    .direction(Direction::TopToBottom)
                    .background(GRID_BACKGROUND_COLOR)
                    .border(
                        Border::all(GRID_BORDER_WIDTH, GRID_BORDER_COLOR)
                            .between_children(CELL_DIVIDER_WIDTH),
                    ),
                |b| {
                    for _ in 0..GRID_ROWS {
                        build_grid_row(b);
                    }
                },
            );
        })
        .build()
}

fn build_grid_row(b: &mut LayoutBuilder) {
    b.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::GROW)
            .direction(Direction::LeftToRight)
            .border(
                Border::new()
                    .color(GRID_BORDER_COLOR)
                    .between_children(CELL_DIVIDER_WIDTH),
            ),
        |b| {
            for _ in 0..GRID_COLUMNS {
                build_grid_cell(b);
            }
        },
    );
}

fn build_grid_cell(b: &mut LayoutBuilder) {
    b.with(El::new().width(Sizing::GROW).height(Sizing::GROW), |_| {});
}

// ── Glyph panels ──────────────────────────────────────────────────

#[allow(clippy::cast_precision_loss)]
fn build_glyph_panel(chars: &[char], color: Color, page: usize) -> DiegeticPanel {
    let row_count = chars.len().div_ceil(GLYPHS_PER_ROW);
    let cell_layout_width = GRID_LAYOUT_WIDTH / GRID_COLUMNS as f32;
    let cell_layout_height = GRID_LAYOUT_HEIGHT / GRID_ROWS as f32;

    let glyph_style = LayoutTextStyle::new(GLYPH_FONT_SIZE)
        .with_color(color)
        .with_shadow_mode(GlyphShadowMode::None)
        .with_letter_spacing(GLYPH_LETTER_SPACING);
    let label_style = LayoutTextStyle::new(PAGE_LABEL_SIZE)
        .with_color(PAGE_LABEL_COLOR)
        .with_shadow_mode(GlyphShadowMode::None);

    let char_count = chars.len();

    DiegeticPanel::builder()
        .size((cell_layout_width, cell_layout_height))
        .layout_unit(Unit::Points)
        .world_height(cell_world_height() * GLYPH_PANEL_SCALE)
        .anchor(Anchor::Center)
        .layout(|b| {
            b.with(
                El::new()
                    .width(Sizing::GROW)
                    .height(Sizing::GROW)
                    .padding(Padding::all(4.0))
                    .direction(Direction::TopToBottom)
                    .child_gap(2.0),
                |b| {
                    // Page number — upper left.
                    b.with(
                        El::new()
                            .width(Sizing::GROW)
                            .height(Sizing::FIT)
                            .child_align_x(AlignX::Left),
                        |b| {
                            b.text(format!("{page}"), label_style.clone());
                        },
                    );

                    // Glyph rows — centered.
                    b.with(
                        El::new()
                            .width(Sizing::GROW)
                            .height(Sizing::GROW)
                            .direction(Direction::TopToBottom)
                            .child_gap(2.0)
                            .child_alignment(AlignX::Center, AlignY::Center),
                        |b| {
                            for row_index in 0..row_count {
                                let start = row_index * GLYPHS_PER_ROW;
                                let end = (start + GLYPHS_PER_ROW).min(chars.len());
                                let row_text: String = chars[start..end].iter().copied().collect();
                                b.text(row_text, glyph_style.clone());
                            }
                        },
                    );

                    // Glyph count — bottom right.
                    b.with(
                        El::new()
                            .width(Sizing::GROW)
                            .height(Sizing::FIT)
                            .child_align_x(AlignX::Right),
                        |b| {
                            b.text(format!("glyphs: {char_count}"), label_style);
                        },
                    );
                },
            );
        })
        .build()
}

fn rebuild_glyph_panels(
    commands: &mut Commands,
    tilt_root: Entity,
    all_chars: &[char],
    panels: &mut GlyphPanelEntities,
) {
    // Despawn existing glyph panels.
    for slot in &mut panels.0 {
        if let Some(entity) = slot.take() {
            commands.entity(entity).despawn();
        }
    }

    let page_size = GLYPHS_PER_PAGE as usize;
    for (page, chunk) in all_chars.chunks(page_size).enumerate() {
        let first_char_index = page * page_size;
        let color = if first_char_index < ASCII_CHARACTER_COUNT {
            ASCII_GLYPH_COLOR
        } else {
            UNICODE_GLYPH_COLOR
        };

        let row = page / GRID_COLUMNS;
        let col = page % GRID_COLUMNS;
        let position = cell_center_position(row, col);

        let entity = commands
            .spawn((
                GlyphCell,
                build_glyph_panel(chunk, color, page),
                Transform::from_translation(position),
            ))
            .id();
        commands.entity(tilt_root).add_child(entity);

        if page < panels.0.len() {
            panels.0[page] = Some(entity);
        }
    }
}

// ── Input ──────────────────────────────────────────────────────────

fn handle_input(
    keys: Res<ButtonInput<KeyCode>>,
    mut next_block: ResMut<NextBlock>,
    mut all_chars: ResMut<AccumulatedCharacters>,
    tilt_root: Res<TiltRoot>,
    mut panels: ResMut<GlyphPanelEntities>,
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

    all_chars.0.extend(block.chars());
    rebuild_glyph_panels(&mut commands, tilt_root.0, &all_chars.0, &mut panels);
}

// ── Status panel ───────────────────────────────────────────────────

struct StatusData {
    pages:        usize,
    glyphs:       usize,
    remaining:    usize,
    cells_filled: usize,
    total_cells:  usize,
}

fn build_status_panel(data: &StatusData) -> DiegeticPanel {
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

    DiegeticPanel::builder()
        .size((STATUS_LAYOUT_WIDTH, STATUS_LAYOUT_HEIGHT))
        .layout_unit(Unit::Points)
        .world_height(STATUS_WORLD_HEIGHT)
        .anchor(Anchor::TopLeft)
        .layout(|b| {
            b.with(
                El::new()
                    .width(Sizing::FIT)
                    .height(Sizing::FIT)
                    .padding(Padding::all(8.0))
                    .direction(Direction::TopToBottom)
                    .child_gap(4.0)
                    .background(STATUS_BACKGROUND)
                    .border(Border::all(0.5, STATUS_BORDER_COLOR)),
                |b| {
                    b.text("atlas", title_style);

                    // Divider.
                    b.with(
                        El::new()
                            .width(Sizing::GROW)
                            .height(Sizing::fixed(1.5))
                            .background(Color::srgb(0.45, 0.45, 0.5)),
                        |_| {},
                    );

                    build_status_rows(b, data, &label_style, &value_style);
                    build_status_instructions(b, data, &dim_style);
                },
            );
        })
        .build()
}

fn build_status_rows(
    b: &mut LayoutBuilder,
    data: &StatusData,
    label_style: &LayoutTextStyle,
    value_style: &LayoutTextStyle,
) {
    b.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::FIT)
            .direction(Direction::LeftToRight)
            .child_gap(12.0),
        |b| {
            b.with(
                El::new().direction(Direction::TopToBottom).child_gap(2.0),
                |b| {
                    b.text("quality", label_style.clone());
                    b.text("glyphs/page", label_style.clone());
                    b.text("pages", label_style.clone());
                    b.text("glyphs", label_style.clone());
                    b.text("cells", label_style.clone());
                },
            );
            b.with(
                El::new()
                    .width(Sizing::GROW)
                    .direction(Direction::TopToBottom)
                    .child_gap(2.0)
                    .child_align_x(AlignX::Right),
                |b| {
                    b.text(format!("{ATLAS_QUALITY:?}"), value_style.clone());
                    b.text(format!("~{GLYPHS_PER_PAGE}"), value_style.clone());
                    b.text(format!("{}", data.pages), value_style.clone());
                    b.text(format!("{}", data.glyphs), value_style.clone());
                    b.text(
                        format!("{}/{}", data.cells_filled, data.total_cells),
                        value_style.clone(),
                    );
                },
            );
        },
    );
}

fn build_status_instructions(
    b: &mut LayoutBuilder,
    data: &StatusData,
    dim_style: &LayoutTextStyle,
) {
    b.with(
        El::new().width(Sizing::GROW).child_align_x(AlignX::Center),
        |b| {
            b.text("'+' add Unicode block", dim_style.clone());
        },
    );
    b.with(
        El::new().width(Sizing::GROW).child_align_x(AlignX::Center),
        |b| {
            b.text(format!("{} remaining", data.remaining), dim_style.clone());
        },
    );
}

fn update_diagnostics(
    atlas: Res<MsdfAtlas>,
    mut status_panels: Query<&mut DiegeticPanel, With<StatusPanel>>,
    next_block: Res<NextBlock>,
    all_chars: Res<AccumulatedCharacters>,
    mut last_displayed: ResMut<LastDisplayedStatus>,
) {
    let pages = atlas.page_count();
    let glyphs = atlas.glyph_count();
    let remaining = UNICODE_BLOCKS.len() - next_block.0;
    let filled = cells_filled_count(&all_chars.0);
    let total = total_page_count();
    let fingerprint = format!("{pages}/{glyphs}/{remaining}/{filled}");

    if fingerprint == last_displayed.fingerprint {
        return;
    }
    last_displayed.fingerprint = fingerprint;

    let data = StatusData {
        pages,
        glyphs,
        remaining,
        cells_filled: filled,
        total_cells: total,
    };
    for mut panel in &mut status_panels {
        *panel = build_status_panel(&data);
    }
}
