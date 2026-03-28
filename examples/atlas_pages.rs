//! @generated `bevy_example_template`
//! MSDF atlas paging — inspecting committed atlas pages directly.
//!
//! Demonstrates how to inspect the MSDF texture atlas at runtime while
//! comparing rendered glyph examples against the raw atlas page texture.
//!
//! Renders a tilted grid (Star Wars crawl style) where each cell
//! represents an actual atlas page in memory. The left side of each
//! cell shows rendered glyph examples for that page; the right side shows
//! the committed MSDF atlas texture.
//! Printable ASCII characters are loaded
//! at startup; pressing `+` adds Unicode Latin Extended blocks,
//! growing the atlas. A status panel shows live diagnostics.

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

/// Layout width per column in points.
const COLUMN_LAYOUT_WIDTH: f32 = 900.0;

/// Layout height per row in points.
const ROW_LAYOUT_HEIGHT: f32 = 600.0;

/// World-space width per column in meters.
const COLUMN_WORLD_WIDTH: f32 = 3.75;

/// World-space height per row in meters.
const ROW_WORLD_HEIGHT: f32 = 2.5;

/// Tilt angle from vertical, in degrees.
const TILT_DEGREES: f32 = 0.0;

// ── Atlas config ───────────────────────────────────────────────────

/// Atlas rasterization quality.
const ATLAS_QUALITY: RasterQuality = RasterQuality::Medium;

/// Glyphs per atlas page (budget for atlas packer — display is independent).
const GLYPHS_PER_PAGE: u16 = 30;

/// Characters per display cell.
const CHARS_PER_CELL: usize = 30;

/// Characters per row inside each display cell.
const CHARS_PER_ROW: usize = 6;

// ── Visual style ───────────────────────────────────────────────────

/// Font size for glyphs in cells (points).
const GLYPH_FONT_SIZE: f32 = 48.0;

/// Extra letter spacing between glyphs (points).
/// Font size for header/footer labels in cells (points).
const CELL_LABEL_SIZE: f32 = 40.0;

/// Color for header/footer labels.
const CELL_LABEL_COLOR: Color = Color::srgb(0.55, 0.75, 1.0);

const ASCII_GLYPH_COLOR: Color = Color::WHITE;
const UNICODE_GLYPH_COLOR: Color = Color::WHITE;
const FALLBACK_GLYPH_COLOR: Color = Color::WHITE;

/// Padding inside each cell's text column (points).
const CELL_TEXT_PADDING: f32 = 26.0;

/// Gap between glyph rows (points).
const GLYPH_ROW_GAP: f32 = 6.0;

/// Gap between individual glyph cells in a row (points).
const GLYPH_CELL_GAP: f32 = 4.0;

/// Grid interior padding (points).
const GRID_INTERIOR_PADDING: f32 = 12.0;

/// Atlas quad horizontal offset within a cell (fraction of cell width).
const ATLAS_QUAD_OFFSET: f32 = 0.25;

/// Atlas quad size as fraction of cell height.
const ATLAS_QUAD_SCALE: f32 = 0.65;

/// Zoom-to-fit margin when clicking the back plane (whole scene).
const ZOOM_MARGIN_SCENE: f32 = 0.08;

/// Zoom-to-fit margin when clicking a cell plane (single page).
const ZOOM_MARGIN_CELL: f32 = 0.15;

/// Zoom-to-fit animation duration in milliseconds.
const ZOOM_DURATION_MS: u64 = 1000;

/// How much larger the back plane is than the grid (ensures clickable after orbiting).
const BACK_PLANE_OVERFLOW: f32 = 0.15;

// ── Status panel ───────────────────────────────────────────────────

const STATUS_LAYOUT_WIDTH: f32 = 400.0;
const STATUS_LAYOUT_HEIGHT: f32 = 280.0;
const STATUS_WORLD_HEIGHT: f32 = 1.8;
const STATUS_FONT_SIZE: f32 = 18.0;
const STATUS_TITLE_SIZE: f32 = 28.0;
const STATUS_BACKGROUND: Color = Color::srgb(1.0, 1.0, 0.0);
const STATUS_BORDER_COLOR: Color = Color::WHITE;

// ── Unicode blocks ─────────────────────────────────────────────────

/// Character batches added by pressing `+`. Each press adds one batch.
/// The first batch is loaded at startup; subsequent batches are added
/// interactively. Batches are sized to roughly fill one atlas page each.
const CHARACTER_BATCHES: &[&str] = &[
    // Latin Extended-A
    "ĀāĂăĄąĆćĈĉĊċČčĎďĐđĒēĔĕĖėĘęĚěĜĝĞğ",
    "ĠġĢģĤĥĦħĨĩĪīĬĭĮįİıĲĳĴĵĶķĸĹĺĻļĽľĿ",
    // Latin Extended-B
    "ŀŁłŃńŅņŇňŉŊŋŌōŎŏŐőŒœŔŕŖŗŘřŚśŜŝŞş",
    "ŠšŢţŤťŦŧŨũŪūŬŭŮůŰűŲųŴŵŶŷŸŹźŻżŽž",
];

// ── Components ─────────────────────────────────────────────────────

/// Marker for the visible grid panel.
#[derive(Component)]
struct GridPanel;

/// Marks a committed atlas texture quad spawned beside the text grid.
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

/// Camera entity for deferred initial zoom-to-fit.
#[derive(Resource)]
struct CameraEntity(Entity);

/// Tracks which character batch to add next.
#[derive(Resource)]
struct NextBatch(usize);

/// All characters fed to the atlas so far.
#[derive(Resource)]
struct AccumulatedCharacters(Vec<char>);

/// A display page — exactly `CHARS_PER_CELL` characters (or fewer for
/// the last page). Not tied to atlas page boundaries.
struct CommittedPage {
    chars: Vec<char>,
}

/// Atlas state machine.
#[derive(Resource)]
enum AtlasPhase {
    /// Waiting for all `AccumulatedCharacters` to be rasterized.
    Loading {
        start:            std::time::Instant,
        last_glyph_count: usize,
        stable_frames:    u32,
    },
    /// All current chars rasterized, pages committed.
    Ready,
}

/// Frozen page assignments — the single source of truth for display.
#[derive(Resource, Default)]
struct CommittedPages(Vec<CommittedPage>);

/// How many committed pages to display (invariant: <= committed.len()).
#[derive(Resource)]
struct PagesRevealed(usize);

/// Entities spawned by the display system (atlas quads + click planes).
#[derive(Resource, Default)]
struct SpawnedCellEntities(Vec<Entity>);

/// Shared invisible material for click planes.
#[derive(Resource)]
struct InvisibleMaterial(Handle<StandardMaterial>);

/// Triggers a grid rebuild when true.
#[derive(Resource, Default)]
struct DisplayDirty(bool);

/// Whether the initial zoom-to-fit has fired.
#[derive(Resource, Default)]
struct InitialZoomFired(bool);

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
        .insert_resource(NextBatch(0))
        .insert_resource(PagesRevealed(0))
        .init_resource::<CommittedPages>()
        .init_resource::<SpawnedCellEntities>()
        .init_resource::<DisplayDirty>()
        .init_resource::<InitialZoomFired>()
        .init_resource::<LastDisplayedStatus>()
        .add_systems(Startup, setup)
        .add_systems(
            Update,
            (
                commit_atlas_pages,
                display_committed_pages,
                handle_input,
                update_diagnostics,
            ),
        )
        .run();
}

// ── Setup ──────────────────────────────────────────────────────────

fn setup(
    atlas: Res<MsdfAtlas>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // Backdrop sized for max grid; panel grows dynamically.
    #[allow(clippy::cast_precision_loss)]
    let grid_w = COLUMN_WORLD_WIDTH * GRID_COLUMNS as f32;
    #[allow(clippy::cast_precision_loss)]
    let grid_h = ROW_WORLD_HEIGHT * GRID_ROWS as f32;

    // Tilted root — Star Wars crawl angle.
    let tilt_radians = TILT_DEGREES.to_radians();
    let tilt_root = commands
        .spawn(Transform::from_rotation(Quat::from_rotation_x(
            -tilt_radians,
        )))
        .id();
    commands.insert_resource(TiltRoot(tilt_root));

    // Visible backdrop — opaque dark panel behind the grid.
    let backdrop_w = grid_w * (1.0 + BACK_PLANE_OVERFLOW);
    let backdrop_h = grid_h * (1.0 + BACK_PLANE_OVERFLOW);
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

    // Grid panel — visible display, built from committed snapshots.
    let grid_entity = commands
        .spawn((GridPanel, build_grid_panel(&[]), Transform::IDENTITY))
        .id();
    commands.entity(tilt_root).add_child(grid_entity);

    // Load all ASCII at startup. The grid panel initially shows all chars
    // in a single "loading" cell to trigger atlas rasterization. Once
    // committed, it rebuilds with proper per-page cells.
    let ascii_chars: Vec<char> = (33_u8..=126).map(|c| c as char).collect();
    commands.insert_resource(AccumulatedCharacters(ascii_chars));
    commands.insert_resource(AtlasPhase::Loading {
        start:            std::time::Instant::now(),
        last_glyph_count: atlas.glyph_count(),
        stable_frames:    0,
    });
    // Mark dirty so the display system builds the loading view.
    commands.insert_resource(DisplayDirty(true));

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
                focus: Vec3::ZERO,
                radius: Some(12.0),
                yaw: Some(0.0),
                pitch: Some(0.0),
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

    // Initial zoom-to-fit is deferred to `display_committed_pages` so the
    // ground plane bounds are propagated before framing.
    commands.insert_resource(CameraEntity(camera));

    // Status panel (floating in world, outside the tilt hierarchy).
    commands.spawn((
        StatusPanel,
        build_status_panel(&StatusData {
            pages:     0,
            glyphs:    0,
            remaining: CHARACTER_BATCHES.len(),
        }),
        Transform::from_xyz(-6.0, 4.0, 2.0),
    ));
}

// ── Grid geometry helpers ──────────────────────────────────────────

fn cell_world_width() -> f32 { COLUMN_WORLD_WIDTH }

fn cell_world_height() -> f32 { ROW_WORLD_HEIGHT }

/// World position for the center of cell `(row, col)` relative to the
/// grid panel with [`Anchor::Center`].
#[allow(clippy::cast_precision_loss)]
fn cell_center_position(row: usize, col: usize, total_rows: usize, total_cols: usize) -> Vec3 {
    let cw = cell_world_width();
    let ch = cell_world_height();
    let x = (total_cols as f32).mul_add(-0.5, col as f32 + 0.5) * cw;
    let y = (total_rows as f32).mul_add(0.5, -(row as f32) - 0.5) * ch;
    Vec3::new(x, y, 0.0)
}

// ── Grid panel (single panel with all cell content) ────────────────

/// Holds the per-cell data needed to populate a grid cell.
struct PageCellData<'a> {
    cell_index:  usize,
    chars:       &'a [char],
    glyph_color: Color,
    atlas_image: Option<Handle<Image>>,
}

fn glyph_color_for(chars: &[char]) -> Color {
    let Some(first_char) = chars.first().copied() else {
        return FALLBACK_GLYPH_COLOR;
    };
    if first_char <= '\u{007E}' {
        ASCII_GLYPH_COLOR
    } else {
        UNICODE_GLYPH_COLOR
    }
}

/// Builds the single grid panel containing committed page text cells.
fn build_grid_panel(cells: &[PageCellData]) -> DiegeticPanel {
    let total_cells = cells.len();
    let cols = GRID_COLUMNS.min(total_cells).max(1);
    let rows = total_cells.div_ceil(cols).max(1);

    #[allow(clippy::cast_precision_loss)]
    let layout_width = COLUMN_LAYOUT_WIDTH * cols as f32;
    #[allow(clippy::cast_precision_loss)]
    let layout_height = ROW_LAYOUT_HEIGHT * rows as f32;
    #[allow(clippy::cast_precision_loss)]
    let world_height = ROW_WORLD_HEIGHT * rows as f32;

    DiegeticPanel::builder()
        .size((layout_width, layout_height))
        .layout_unit(Unit::Points)
        .world_height(world_height)
        .anchor(Anchor::Center)
        .layout(|b| {
            b.with(
                El::new()
                    .width(Sizing::GROW)
                    .height(Sizing::GROW)
                    .padding(Padding::all(GRID_INTERIOR_PADDING))
                    .direction(Direction::TopToBottom)
                    .border(Border::all(0.5, Color::WHITE)),
                |b| {
                    for row in 0..rows {
                        b.with(
                            El::new()
                                .width(Sizing::GROW)
                                .height(Sizing::GROW)
                                .direction(Direction::LeftToRight)
                                .border(Border::new().color(Color::WHITE).between_children(0.5)),
                            |b| {
                                let start = row * cols;
                                let end = (start + cols).min(total_cells);
                                for cell in &cells[start..end] {
                                    build_cell(b, Some(cell), cols);
                                }
                            },
                        );
                    }
                },
            );
        })
        .build()
}

/// Builds a single text cell with a header, glyph grid, and footer.
#[allow(clippy::cast_precision_loss)]
fn build_cell(b: &mut LayoutBuilder, data: Option<&PageCellData>, cols: usize) {
    let cell_width = Sizing::percent(1.0 / cols as f32);

    let Some(data) = data else {
        b.with(El::new().width(cell_width).height(Sizing::GROW), |_| {});
        return;
    };

    let header_style = LayoutTextStyle::new(CELL_LABEL_SIZE).with_color(CELL_LABEL_COLOR);
    let glyph_style = LayoutTextStyle::new(GLYPH_FONT_SIZE).with_color(data.glyph_color);

    let page_label = format!("page {}", data.cell_index);

    // Cell: left-to-right — text column | atlas image.
    b.with(
        El::new()
            .width(cell_width)
            .height(Sizing::GROW)
            .padding(Padding::new(
                CELL_TEXT_PADDING,
                CELL_TEXT_PADDING,
                CELL_TEXT_PADDING * 0.5,
                CELL_TEXT_PADDING * 0.5,
            ))
            .direction(Direction::LeftToRight)
            .child_gap(CELL_TEXT_PADDING),
        |b| {
            // ── Left column: header, glyph grid, footer ──
            b.with(
                El::new()
                    .width(Sizing::GROW)
                    .height(Sizing::GROW)
                    .direction(Direction::TopToBottom)
                    .child_gap(6.0)
                    .border(Border::all(0.5, Color::srgba(1.0, 1.0, 1.0, 0.2))),
                |b| {
                    // Header: "atlas page: N"
                    b.text(page_label, header_style.clone());

                    // Glyph grid — rows of individually spaced characters.
                    b.with(
                        El::new()
                            .width(Sizing::GROW)
                            .height(Sizing::GROW)
                            .padding(Padding::all(30.0))
                            .direction(Direction::TopToBottom)
                            .child_gap(GLYPH_ROW_GAP)
                            .child_alignment(AlignX::Center, AlignY::Center),
                        |b| {
                            build_glyph_grid(b, &data.chars, &glyph_style);
                        },
                    );

                    // Footer: "glyphs: N"
                    b.with(
                        El::new()
                            .width(Sizing::GROW)
                            .height(Sizing::FIT)
                            .child_align_x(AlignX::Right),
                        |b| {
                            b.text(format!("chars: {}", data.chars.len()), header_style);
                        },
                    );
                },
            );

            // ── Right column: atlas page texture ──
            if let Some(ref handle) = data.atlas_image {
                b.image(
                    El::new()
                        .width(Sizing::GROW)
                        .height(Sizing::GROW)
                        .border(Border::all(0.5, Color::srgba(1.0, 1.0, 1.0, 0.2))),
                    handle.clone(),
                    Color::WHITE,
                );
            }
        },
    );
}

/// Builds the glyph grid: rows of evenly-spaced, centered glyphs.
fn build_glyph_grid(b: &mut LayoutBuilder, chars: &[char], glyph_style: &LayoutTextStyle) {
    let row_count = chars.len().div_ceil(CHARS_PER_ROW);
    for row_index in 0..row_count {
        let start = row_index * CHARS_PER_ROW;
        let end = (start + CHARS_PER_ROW).min(chars.len());

        b.with(
            El::new()
                .width(Sizing::GROW)
                .height(Sizing::GROW)
                .direction(Direction::LeftToRight)
                .child_gap(GLYPH_CELL_GAP),
            |b| {
                for &ch in &chars[start..end] {
                    b.with(
                        El::new()
                            .width(Sizing::GROW)
                            .height(Sizing::GROW)
                            .child_alignment(AlignX::Center, AlignY::Center),
                        |b| {
                            b.text(String::from(ch), glyph_style.clone());
                        },
                    );
                }
            },
        );
    }
}

// ── State machine systems ──────────────────────────────────────────
//
// Three concerns are fully separated:
//   1. commit_atlas_pages — reads live atlas, writes CommittedPages snapshot
//   2. display_committed_pages — reads CommittedPages only, writes grid panel
//   3. handle_input — reads CommittedPages, writes PagesRevealed or queues batch
//
// Invariants:
//   - revealed <= committed.len()
//   - rendering/status derive ONLY from committed[..revealed]
//   - live atlas state is ONLY read by the commit system

/// Watches the atlas during the Loading phase. Once glyph rasterization
/// stabilizes, chunks `AccumulatedCharacters` into display pages of
/// `CHARS_PER_CELL` and transitions to Ready.
fn commit_atlas_pages(
    atlas: Res<MsdfAtlas>,
    all_chars: Res<AccumulatedCharacters>,
    mut phase: ResMut<AtlasPhase>,
    mut committed: ResMut<CommittedPages>,
    mut revealed: ResMut<PagesRevealed>,
    mut dirty: ResMut<DisplayDirty>,
) {
    let current_glyph_count = atlas.glyph_count();
    let start = {
        let AtlasPhase::Loading {
            start,
            last_glyph_count,
            stable_frames,
        } = &mut *phase
        else {
            return;
        };

        // Don't consider stable until at least one glyph exists.
        if current_glyph_count == 0 {
            return;
        }

        if *last_glyph_count == current_glyph_count {
            *stable_frames += 1;
        } else {
            *last_glyph_count = current_glyph_count;
            *stable_frames = 0;
        }

        if *stable_frames < 2 {
            return;
        }

        *start
    };

    if all_chars.0.is_empty() {
        return;
    }

    // Chunk characters into display pages of CHARS_PER_CELL.
    let elapsed = start.elapsed();
    info!(
        "Atlas rasterization stabilized: {} glyphs in {:.1}ms — committing {} chars in {} display pages",
        current_glyph_count,
        elapsed.as_secs_f64() * 1000.0,
        all_chars.0.len(),
        all_chars.0.len().div_ceil(CHARS_PER_CELL),
    );

    let previous_count = committed.0.len();
    committed.0.clear();
    for chunk in all_chars.0.chunks(CHARS_PER_CELL) {
        committed.0.push(CommittedPage {
            chars: chunk.to_vec(),
        });
    }

    // Reveal the first new page (or keep current if re-committing).
    if previous_count == 0 {
        revealed.0 = 1;
    } else {
        revealed.0 = (previous_count + 1).min(committed.0.len());
    }

    *phase = AtlasPhase::Ready;
    dirty.0 = true;
}

/// Rebuilds the grid panel from committed snapshots (Ready) or shows
/// a loading cell with all chars (Loading) to trigger rasterization.
#[allow(clippy::cast_precision_loss)]
fn display_committed_pages(
    atlas: Res<MsdfAtlas>,
    phase: Res<AtlasPhase>,
    committed: Res<CommittedPages>,
    revealed: Res<PagesRevealed>,
    all_chars: Res<AccumulatedCharacters>,
    tilt_root: Res<TiltRoot>,
    invisible_mat: Res<InvisibleMaterial>,
    mut dirty: ResMut<DisplayDirty>,
    mut initial_zoom: ResMut<InitialZoomFired>,
    mut spawned: ResMut<SpawnedCellEntities>,
    mut grid_panels: Query<&mut DiegeticPanel, With<GridPanel>>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    camera_entity: Res<CameraEntity>,
    scene_bounds: Res<SceneBounds>,
) {
    if !dirty.0 {
        return;
    }
    dirty.0 = false;

    let mut cells: Vec<PageCellData> = Vec::new();
    if matches!(*phase, AtlasPhase::Loading { .. }) {
        // During loading, show all chars in a single cell to trigger
        // atlas rasterization.
        if !all_chars.0.is_empty() {
            cells.push(PageCellData {
                cell_index:  0,
                chars:       &all_chars.0,
                glyph_color: glyph_color_for(&all_chars.0),
                atlas_image: None,
            });
        }
    } else {
        let shown = revealed.0.min(committed.0.len());
        for (i, page) in committed.0[..shown].iter().enumerate() {
            cells.push(PageCellData {
                cell_index:  i,
                chars:       &page.chars,
                glyph_color: glyph_color_for(&page.chars),
                atlas_image: atlas.image_handle(i as u32).cloned(),
            });
        }
    }

    // Update the grid panel tree.
    for mut panel in &mut grid_panels {
        *panel = build_grid_panel(&cells);
    }

    // Despawn previous click planes.
    for entity in spawned.0.drain(..) {
        commands.entity(entity).despawn();
    }

    let total_cells = cells.len();
    let cols = GRID_COLUMNS.min(total_cells).max(1);
    let rows = total_cells.div_ceil(cols).max(1);
    let cw = cell_world_width();
    let ch = cell_world_height();
    let quad_size = ch * ATLAS_QUAD_SCALE;

    // Spawn committed atlas texture quads and per-cell invisible click planes.
    for cell_index in 0..total_cells {
        let row = cell_index / cols;
        let col = cell_index % cols;
        if row >= rows {
            break;
        }
        let center = cell_center_position(row, col, rows, cols);

        if let Some(ref image_handle) = cells[cell_index].atlas_image {
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

    // Initial zoom-to-fit after first display.
    if !initial_zoom.0 && total_cells > 0 {
        initial_zoom.0 = true;
        commands.trigger(
            ZoomToFit::new(camera_entity.0, scene_bounds.0)
                .margin(ZOOM_MARGIN_SCENE)
                .duration(Duration::from_millis(ZOOM_DURATION_MS)),
        );
    }
}

// ── Input ──────────────────────────────────────────────────────────

fn handle_input(
    atlas: Res<MsdfAtlas>,
    keys: Res<ButtonInput<KeyCode>>,
    committed: Res<CommittedPages>,
    mut revealed: ResMut<PagesRevealed>,
    mut next_batch: ResMut<NextBatch>,
    mut all_chars: ResMut<AccumulatedCharacters>,
    mut atlas_phase: ResMut<AtlasPhase>,
    mut dirty: ResMut<DisplayDirty>,
) {
    if !keys.just_pressed(KeyCode::Equal) {
        return;
    }
    // Don't accept input while loading.
    if matches!(*atlas_phase, AtlasPhase::Loading { .. }) {
        return;
    }

    // If there are unrevealed committed pages, reveal the next one.
    if revealed.0 < committed.0.len() {
        revealed.0 += 1;
        dirty.0 = true;
        return;
    }

    // All committed pages revealed — add the next character batch
    // and enter loading phase.
    if next_batch.0 >= CHARACTER_BATCHES.len() {
        return;
    }
    let batch = CHARACTER_BATCHES[next_batch.0];
    next_batch.0 += 1;
    all_chars.0.extend(batch.chars());
    *atlas_phase = AtlasPhase::Loading {
        start:            std::time::Instant::now(),
        last_glyph_count: atlas.glyph_count(),
        stable_frames:    0,
    };
    dirty.0 = true;
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
            b.text("'+' reveal page / add batch", dim_style.clone());
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
    committed: Res<CommittedPages>,
    revealed: Res<PagesRevealed>,
    mut status_panels: Query<&mut DiegeticPanel, With<StatusPanel>>,
    next_batch: Res<NextBatch>,
    mut last_displayed: ResMut<LastDisplayedStatus>,
) {
    let shown = revealed.0.min(committed.0.len());
    let glyphs: usize = committed.0[..shown].iter().map(|p| p.chars.len()).sum();
    let unrevealed = committed.0.len() - shown;
    let remaining = unrevealed + CHARACTER_BATCHES.len() - next_batch.0;
    let fingerprint = format!("{shown}/{glyphs}/{remaining}");

    if fingerprint == last_displayed.fingerprint {
        return;
    }
    last_displayed.fingerprint = fingerprint;

    let data = StatusData {
        pages: shown,
        glyphs,
        remaining,
    };
    for mut panel in &mut status_panels {
        *panel = build_status_panel(&data);
    }
}
