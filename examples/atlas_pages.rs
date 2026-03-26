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

use std::collections::BTreeMap;
use std::time::Duration;

use bevy::picking::mesh_picking::MeshPickingPlugin;
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
use bevy_diegetic::Font;
use bevy_diegetic::FontId;
use bevy_diegetic::FontRegistry;
use bevy_diegetic::GlyphKey;
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
use bevy_panorbit_camera_ext::ZoomToFit;
use bevy_window_manager::WindowManagerPlugin;

// ── Grid geometry ──────────────────────────────────────────────────

/// Columns in the page grid.
const GRID_COLUMNS: usize = 4;

/// Rows in the page grid.
const GRID_ROWS: usize = 2;

/// Grid layout width in points.
const GRID_LAYOUT_WIDTH: f32 = 3600.0;

/// Grid layout height in points.
const GRID_LAYOUT_HEIGHT: f32 = 1200.0;

/// World-space height of the grid in meters.
const GRID_WORLD_HEIGHT: f32 = 5.0;

/// Tilt angle from vertical, in degrees.
const TILT_DEGREES: f32 = 45.0;

// ── Atlas config ───────────────────────────────────────────────────

/// Atlas rasterization quality.
const ATLAS_QUALITY: RasterQuality = RasterQuality::Medium;

/// Glyphs per atlas page (budget — actual count varies).
const GLYPHS_PER_PAGE: u16 = 30;

/// Characters per row inside each glyph cell.
const GLYPHS_PER_ROW: usize = 6;

// ── Visual style ───────────────────────────────────────────────────

/// Font size for glyphs in cells (points).
const GLYPH_FONT_SIZE: f32 = 18.0;

/// Extra letter spacing between glyphs (points).
const GLYPH_LETTER_SPACING: f32 = 3.0;

/// Font size for page number labels (points).
const PAGE_LABEL_SIZE: f32 = 14.0;

/// Grid outer border width (points).
const GRID_BORDER_WIDTH: f32 = 0.5;

/// Divider width between cells and rows (points).
const CELL_DIVIDER_WIDTH: f32 = 0.5;

/// Debug border width on glyph panel elements (points, 0 to hide).
const GLYPH_PANEL_DEBUG_BORDER: f32 = 0.5;

/// Grid interior padding (points).
const GRID_INTERIOR_PADDING: f32 = 12.0;

/// Horizontal offset for atlas texture quad within cell (fraction of cell width).
const ATLAS_QUAD_OFFSET: f32 = 0.25;

/// Atlas quad size as fraction of cell height.
const ATLAS_QUAD_SCALE: f32 = 0.65;

/// Zoom-to-fit margin when clicking the back plane (whole scene).
const ZOOM_MARGIN_SCENE: f32 = 0.08;

/// Zoom-to-fit margin when clicking a cell plane (single page).
const ZOOM_MARGIN_CELL: f32 = 0.15;

/// Zoom-to-fit animation duration in milliseconds.
const ZOOM_DURATION_MS: u64 = 1000;

/// How much larger the back plane is than the grid (fraction extra).
const BACK_PLANE_OVERFLOW: f32 = 0.15;

const GRID_BORDER_COLOR: Color = Color::srgba(0.5, 0.55, 0.7, 0.6);
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
    "ĀāĂăĄąĆćĈĉĊċČčĎďĐđĒēĔĕĖėĘęĚěĜĝĞğ",
    "ĠġĢģĤĥĦħĨĩĪīĬĭĮįİıĲĳĴĵĶķĸĹĺĻļĽľĿ",
    "ŀŁłŃńŅņŇňŉŊŋŌōŎŏŐőŒœŔŕŖŗŘřŚśŜŝŞş",
    "ŠšŢţŤťŦŧŨũŪūŬŭŮůŰűŲųŴŵŶŷŸŹźŻżŽž",
];

// ── Components ─────────────────────────────────────────────────────

/// Marker for the grid panel (single panel containing all cell text).
#[derive(Component)]
struct GridPanel;

/// Marks an atlas texture quad spawned by the atlas sync system.
#[derive(Component)]
struct AtlasTextureQuad;

/// Invisible click plane per cell for zoom-to-cell.
#[derive(Component)]
struct CellClickPlane;

/// Marker for the status panel.
#[derive(Component)]
struct StatusPanel;

// ── Resources ──────────────────────────────────────────────────────

/// Root entity that tilts the entire grid scene.
#[derive(Resource)]
struct TiltRoot(Entity);

/// Ground plane entity for zoom-to-fit targeting.
#[derive(Resource)]
struct SceneBounds(Entity);

/// Tracks which Unicode block to add next.
#[derive(Resource)]
struct NextBlock(usize);

/// All characters accumulated so far.
#[derive(Resource)]
struct AccumulatedCharacters(Vec<char>);

/// Entities spawned by the atlas sync system (atlas quads + click planes).
#[derive(Resource, Default)]
struct SpawnedCellEntities(Vec<Entity>);

/// Shared invisible material for click planes.
#[derive(Resource)]
struct InvisibleMaterial(Handle<StandardMaterial>);

/// Change-detection state for the atlas sync system.
#[derive(Resource, Default)]
struct AtlasSyncState {
    last_glyph_count: usize,
    last_char_count:  usize,
}

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
            MeshPickingPlugin,
        ))
        .insert_resource(NextBlock(0))
        .init_resource::<SpawnedCellEntities>()
        .init_resource::<AtlasSyncState>()
        .init_resource::<LastDisplayedStatus>()
        .add_systems(Startup, setup)
        .add_systems(Update, (handle_input, sync_atlas_cells, update_diagnostics))
        .run();
}

// ── Setup ──────────────────────────────────────────────────────────

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let grid_w = grid_world_width();
    let grid_h = GRID_WORLD_HEIGHT;

    // Tilted root — Star Wars crawl angle.
    let tilt_radians = TILT_DEGREES.to_radians();
    let tilt_root = commands
        .spawn(Transform::from_rotation(Quat::from_rotation_x(
            -tilt_radians,
        )))
        .id();
    commands.insert_resource(TiltRoot(tilt_root));

    // Visible backdrop — opaque dark panel behind the grid.
    let backdrop_w = grid_w * (1.1 + BACK_PLANE_OVERFLOW);
    let backdrop_h = grid_h * (1.1 + BACK_PLANE_OVERFLOW);
    let ground = commands
        .spawn((
            Mesh3d(meshes.add(Rectangle::new(backdrop_w, backdrop_h))),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: Color::srgb(0.08, 0.08, 0.08),
                alpha_mode: AlphaMode::Opaque,
                double_sided: true,
                cull_mode: None,
                ..default()
            })),
            Transform::from_xyz(0.0, 0.0, -1.0),
        ))
        .observe(on_ground_clicked)
        .id();
    commands.entity(tilt_root).add_child(ground);
    commands.insert_resource(SceneBounds(ground));

    // Shared invisible material for per-cell click planes.
    let invisible_material = materials.add(StandardMaterial {
        base_color: Color::srgba(0.0, 0.0, 0.0, 0.01),
        alpha_mode: AlphaMode::Blend,
        double_sided: true,
        cull_mode: None,
        unlit: true,
        ..default()
    });
    commands.insert_resource(InvisibleMaterial(invisible_material));

    // Grid panel — single panel containing all cell outlines and text.
    // Built empty; `sync_atlas_cells` populates the cells.
    let grid_entity = commands
        .spawn((
            GridPanel,
            build_grid_panel(&BTreeMap::new(), &[]),
            Transform::IDENTITY,
        ))
        .id();
    commands.entity(tilt_root).add_child(grid_entity);

    // Initial ASCII characters. Cell content built by `sync_atlas_cells`.
    let ascii_chars: Vec<char> = (33_u8..=126).map(|c| c as char).collect();
    commands.insert_resource(AccumulatedCharacters(ascii_chars));

    // Lighting — shadows on primary, fill from behind.
    commands.spawn((
        DirectionalLight {
            shadows_enabled: true,
            illuminance: 5000.0,
            ..default()
        },
        Transform::from_xyz(0.0, 5.0, 8.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    commands.spawn((
        DirectionalLight {
            shadows_enabled: false,
            ..default()
        },
        Transform::from_xyz(0.0, 5.0, -8.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));

    // Camera.
    let camera = commands
        .spawn((
            AmbientLight {
                color:                      Color::WHITE,
                brightness:                 300.0,
                affects_lightmapped_meshes: false,
            },
            PanOrbitCamera {
                focus: Vec3::new(0.0, 0.5, 0.0),
                radius: Some(12.0),
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
        ))
        .id();

    // Initial zoom-to-fit on the back plane.
    commands.trigger(
        ZoomToFit::new(camera, ground)
            .margin(ZOOM_MARGIN_SCENE)
            .duration(Duration::from_millis(ZOOM_DURATION_MS)),
    );

    // Status panel (floating in world, outside the tilt hierarchy).
    commands.spawn((
        StatusPanel,
        build_status_panel(&StatusData {
            pages:     0,
            glyphs:    0,
            remaining: UNICODE_BLOCKS.len(),
        }),
        Transform::from_xyz(-6.0, 4.0, 2.0),
    ));
}

// ── Grid geometry helpers ──────────────────────────────────────────

fn grid_world_width() -> f32 { GRID_WORLD_HEIGHT * (GRID_LAYOUT_WIDTH / GRID_LAYOUT_HEIGHT) }

#[allow(clippy::cast_precision_loss)]
fn cell_world_width() -> f32 { grid_world_width() / GRID_COLUMNS as f32 }

#[allow(clippy::cast_precision_loss)]
fn cell_world_height() -> f32 { GRID_WORLD_HEIGHT / GRID_ROWS as f32 }

/// World position for the center of cell `(row, col)` relative to the
/// grid panel with [`Anchor::Center`].
#[allow(clippy::cast_precision_loss)]
fn cell_center_position(row: usize, col: usize) -> Vec3 {
    let cw = cell_world_width();
    let ch = cell_world_height();
    let x = (GRID_COLUMNS as f32).mul_add(-0.5, col as f32 + 0.5) * cw;
    let y = (GRID_ROWS as f32).mul_add(0.5, -(row as f32) - 0.5) * ch;
    Vec3::new(x, y, 0.0)
}

// ── Grid panel (single panel with all cell content) ────────────────

/// Holds the per-page data needed to populate a grid cell.
struct PageCellData<'a> {
    page:        u32,
    chars:       &'a [char],
    glyph_count: usize,
    color:       Color,
}

/// Builds the single grid panel containing all cell outlines and text.
fn build_grid_panel(page_groups: &BTreeMap<u32, Vec<char>>, unresolved: &[char]) -> DiegeticPanel {
    // Collect page data into cell slots.
    let mut cells: Vec<Option<PageCellData>> =
        (0..GRID_COLUMNS * GRID_ROWS).map(|_| None).collect();
    for (cell_index, (&page, chars)) in page_groups.iter().enumerate() {
        if cell_index >= cells.len() {
            break;
        }
        let first_char = chars.first().copied().unwrap_or('?');
        let color = if first_char <= '\u{007E}' {
            ASCII_GLYPH_COLOR
        } else {
            UNICODE_GLYPH_COLOR
        };
        cells[cell_index] = Some(PageCellData {
            page,
            chars,
            glyph_count: chars.len(),
            color,
        });
    }
    // Unresolved chars go in the next available cell.
    if !unresolved.is_empty() {
        let next = page_groups.len();
        if next < cells.len() {
            cells[next] = Some(PageCellData {
                page:        u32::MAX,
                chars:       unresolved,
                glyph_count: unresolved.len(),
                color:       Color::srgba(0.5, 0.5, 0.5, 0.4),
            });
        }
    }

    let dbg_border = Border::all(GLYPH_PANEL_DEBUG_BORDER, Color::WHITE);

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
                    .border(
                        Border::all(GRID_BORDER_WIDTH, GRID_BORDER_COLOR)
                            .between_children(CELL_DIVIDER_WIDTH),
                    ),
                |b| {
                    for row in 0..GRID_ROWS {
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
                                for col in 0..GRID_COLUMNS {
                                    let idx = row * GRID_COLUMNS + col;
                                    build_cell(b, cells[idx].as_ref(), dbg_border);
                                }
                            },
                        );
                    }
                },
            );
        })
        .build()
}

/// Builds a single cell: page label (top), glyph rows (center), glyph
/// count (bottom). Empty cells get a plain spacer.
fn build_cell(b: &mut LayoutBuilder, data: Option<&PageCellData>, dbg_border: Border) {
    let Some(data) = data else {
        b.with(El::new().width(Sizing::GROW).height(Sizing::GROW), |_| {});
        return;
    };

    let glyph_style = LayoutTextStyle::new(GLYPH_FONT_SIZE)
        .with_color(data.color)
        .with_shadow_mode(GlyphShadowMode::None)
        .with_letter_spacing(GLYPH_LETTER_SPACING);
    let label_style = LayoutTextStyle::new(PAGE_LABEL_SIZE)
        .with_color(PAGE_LABEL_COLOR)
        .with_shadow_mode(GlyphShadowMode::None);

    let page_label = if data.page == u32::MAX {
        "pending".to_string()
    } else {
        format!("{}", data.page)
    };

    let row_count = data.chars.len().div_ceil(GLYPHS_PER_ROW);

    b.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::GROW)
            .padding(Padding::all(4.0))
            .direction(Direction::TopToBottom)
            .child_gap(2.0)
            .border(dbg_border),
        |b| {
            // Page number — upper left.
            b.with(
                El::new()
                    .width(Sizing::GROW)
                    .height(Sizing::FIT)
                    .child_align_x(AlignX::Left)
                    .border(dbg_border),
                |b| {
                    b.text(page_label, label_style.clone());
                },
            );

            // Glyph rows — centered.
            b.with(
                El::new()
                    .width(Sizing::GROW)
                    .height(Sizing::GROW)
                    .direction(Direction::TopToBottom)
                    .child_gap(2.0)
                    .child_alignment(AlignX::Center, AlignY::Center)
                    .border(dbg_border),
                |b| {
                    for row_index in 0..row_count {
                        let start = row_index * GLYPHS_PER_ROW;
                        let end = (start + GLYPHS_PER_ROW).min(data.chars.len());
                        let row_text: String = data.chars[start..end].iter().copied().collect();
                        b.text(row_text, glyph_style.clone());
                    }
                },
            );

            // Glyph count — bottom right.
            b.with(
                El::new()
                    .width(Sizing::GROW)
                    .height(Sizing::FIT)
                    .child_align_x(AlignX::Right)
                    .border(dbg_border),
                |b| {
                    b.text(format!("glyphs: {}", data.glyph_count), label_style);
                },
            );
        },
    );
}

// ── Atlas sync system ──────────────────────────────────────────────

/// Groups characters by their actual atlas page using [`GlyphKey`] and
/// [`MsdfAtlas::get_metrics`], then updates the grid panel tree and
/// spawns atlas texture quads.
#[allow(clippy::cast_precision_loss)]
fn sync_atlas_cells(
    atlas: Res<MsdfAtlas>,
    registry: Res<FontRegistry>,
    all_chars: Res<AccumulatedCharacters>,
    tilt_root: Res<TiltRoot>,
    invisible_mat: Res<InvisibleMaterial>,
    mut sync_state: ResMut<AtlasSyncState>,
    mut spawned: ResMut<SpawnedCellEntities>,
    mut grid_panels: Query<&mut DiegeticPanel, With<GridPanel>>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let glyph_count = atlas.glyph_count();
    let char_count = all_chars.0.len();

    // Early-out if nothing changed.
    if glyph_count == sync_state.last_glyph_count && char_count == sync_state.last_char_count {
        return;
    }
    sync_state.last_glyph_count = glyph_count;
    sync_state.last_char_count = char_count;

    // Parse the default font for char → glyph ID mapping.
    let Some(font_data) = registry.font(FontId(0)).map(Font::data) else {
        return;
    };
    let Ok(face) = ttf_parser::Face::parse(font_data, 0) else {
        return;
    };

    // Map each character to its atlas page.
    let mut page_groups: BTreeMap<u32, Vec<char>> = BTreeMap::new();
    let mut unresolved: Vec<char> = Vec::new();

    for &ch in &all_chars.0 {
        let Some(glyph_id) = face.glyph_index(ch) else {
            continue;
        };
        let key = GlyphKey {
            font_id:     0,
            glyph_index: glyph_id.0,
        };
        if let Some(metrics) = atlas.get_metrics(key) {
            page_groups.entry(metrics.page_index).or_default().push(ch);
        } else {
            unresolved.push(ch);
        }
    }

    // Update the grid panel tree with real page data.
    for mut panel in &mut grid_panels {
        *panel = build_grid_panel(&page_groups, &unresolved);
    }

    // Despawn previous atlas quads and click planes.
    for entity in spawned.0.drain(..) {
        commands.entity(entity).despawn();
    }

    let cw = cell_world_width();
    let ch = cell_world_height();
    let quad_size = ch * ATLAS_QUAD_SCALE;

    // Spawn atlas texture quads and per-cell click planes.
    for (cell_index, (&page, _)) in page_groups.iter().enumerate() {
        let row = cell_index / GRID_COLUMNS;
        let col = cell_index % GRID_COLUMNS;
        if row >= GRID_ROWS {
            break;
        }
        let center = cell_center_position(row, col);

        // Atlas texture quad — right side of cell.
        if let Some(image_handle) = atlas.image_handle(page) {
            let quad_pos = center + Vec3::new(cw * ATLAS_QUAD_OFFSET, 0.0, 0.001);
            let quad_entity = commands
                .spawn((
                    AtlasTextureQuad,
                    Mesh3d(meshes.add(Rectangle::new(quad_size, quad_size))),
                    MeshMaterial3d(materials.add(StandardMaterial {
                        base_color_texture: Some(image_handle.clone()),
                        unlit: true,
                        double_sided: true,
                        cull_mode: None,
                        alpha_mode: AlphaMode::Blend,
                        ..default()
                    })),
                    Transform::from_translation(quad_pos),
                ))
                .id();
            commands.entity(tilt_root.0).add_child(quad_entity);
            spawned.0.push(quad_entity);
        }

        // Invisible per-cell click plane — zoom-to-fit this cell.
        let cell_plane = commands
            .spawn((
                CellClickPlane,
                Mesh3d(meshes.add(Rectangle::new(cw * 0.95, ch * 0.95))),
                MeshMaterial3d(invisible_mat.0.clone()),
                Transform::from_translation(center + Vec3::new(0.0, 0.0, 0.01)),
            ))
            .observe(on_cell_clicked)
            .id();
        commands.entity(tilt_root.0).add_child(cell_plane);
        spawned.0.push(cell_plane);
    }
}

// ── Input ──────────────────────────────────────────────────────────

fn handle_input(
    keys: Res<ButtonInput<KeyCode>>,
    mut next_block: ResMut<NextBlock>,
    mut all_chars: ResMut<AccumulatedCharacters>,
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
}

// ── Click handlers ─────────────────────────────────────────────────

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

fn on_cell_clicked(mut click: On<Pointer<Click>>, mut commands: Commands) {
    if click.button != PointerButton::Primary {
        return;
    }
    click.propagate(false);
    let camera = click.hit.camera;
    commands.trigger(
        ZoomToFit::new(camera, click.entity)
            .margin(ZOOM_MARGIN_CELL)
            .duration(Duration::from_millis(ZOOM_DURATION_MS)),
    );
}

// ── Status panel ───────────────────────────────────────────────────

struct StatusData {
    pages:     usize,
    glyphs:    usize,
    remaining: usize,
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
                    .border(Border::all(0.25, STATUS_BORDER_COLOR)),
                |b| {
                    b.text("atlas", title_style);

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
    mut last_displayed: ResMut<LastDisplayedStatus>,
) {
    let pages = atlas.page_count();
    let glyphs = atlas.glyph_count();
    let remaining = UNICODE_BLOCKS.len() - next_block.0;
    let fingerprint = format!("{pages}/{glyphs}/{remaining}");

    if fingerprint == last_displayed.fingerprint {
        return;
    }
    last_displayed.fingerprint = fingerprint;

    let data = StatusData {
        pages,
        glyphs,
        remaining,
    };
    for mut panel in &mut status_panels {
        *panel = build_status_panel(&data);
    }
}
