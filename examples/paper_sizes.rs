//! @generated `bevy_example_template`
//! Paper sizes — portrait and landscape for standard sizes.
//!
//! A single [`DiegeticPanel`] with five columns: four groups of paper
//! sizes (A-series, US, Cards, Photos) and a commentary column. Each
//! group shows 4 sizes with portrait + landscape rectangles at relative
//! scale. The panel is laid out in points and scaled to screen width
//! via `world_width`.

use std::time::Duration;

use bevy::picking::mesh_picking::MeshPickingPlugin;
use bevy::prelude::*;
use bevy_brp_extras::BrpExtrasPlugin;
use bevy_brp_extras::PortDisplay;
use bevy_diegetic::Anchor;
use bevy_diegetic::Border;
use bevy_diegetic::DiegeticPanel;
use bevy_diegetic::DiegeticUiPlugin;
use bevy_diegetic::Direction;
use bevy_diegetic::El;
use bevy_diegetic::LayoutBuilder;
use bevy_diegetic::LayoutTextStyle;
use bevy_diegetic::Padding;
use bevy_diegetic::PaperSize;
use bevy_diegetic::Pt;
use bevy_diegetic::Sizing;
use bevy_diegetic::WorldText;
use bevy_diegetic::WorldTextStyle;
use bevy_lagrange::InputControl;
use bevy_lagrange::LagrangePlugin;
use bevy_lagrange::OrbitCam;
use bevy_lagrange::TrackpadBehavior;
use bevy_lagrange::TrackpadInput;
use bevy_lagrange::ZoomToFit;
use bevy_window_manager::WindowManagerPlugin;

// ── Panel layout (all in points) ─────────────────────────────────────
// The panel is designed in points like a document, then world_width
// scales it to fit the viewport.
const PANEL_WIDTH: f32 = 1200.0; // pt
const PANEL_HEIGHT: f32 = 900.0; // pt
const OUTER_PADDING: f32 = 16.0; // pt
const COLUMN_GAP: f32 = 12.0; // pt between columns
const ROW_SPACING: f32 = 8.0; // pt between rows within a column
const PAIR_GAP: f32 = 4.0; // pt between portrait and landscape

// World scale: panel spans ~1.2m wide so text at 10pt is readable.
const WORLD_WIDTH: f32 = 3.0;

// ── Visual ───────────────────────────────────────────────────────────
const LABEL_COLOR: Color = Color::srgba(0.55, 0.55, 0.55, 0.9);
const HEADER_COLOR: Color = Color::srgba(0.65, 0.75, 0.95, 1.0);
const BORDER_COLOR: Color = Color::srgba(0.85, 0.75, 0.55, 1.0);
const NOTE_COLOR: Color = Color::srgba(0.72, 0.72, 0.72, 0.95);
const CODE_COLOR: Color = Color::srgba(0.85, 0.75, 0.55, 1.0);

// ── Zoom ─────────────────────────────────────────────────────────────
const ZOOM_MARGIN: f32 = 0.06;
const ZOOM_DURATION_MS: u64 = 600;

/// Group entry: `(title, show_in_inches, sizes)`.
type PaperGroup = (&'static str, bool, &'static [(PaperSize, &'static str)]);

/// Four groups of 4 paper sizes each.
const GROUPS: &[PaperGroup] = &[
    (
        "A-Series",
        false,
        &[
            (PaperSize::A3, "A3"),
            (PaperSize::A4, "A4"),
            (PaperSize::A5, "A5"),
            (PaperSize::A8, "A8"),
        ],
    ),
    (
        "US Sizes",
        true,
        &[
            (PaperSize::USLetter, "Letter"),
            (PaperSize::USLegal, "Legal"),
            (PaperSize::USLedger, "Ledger"),
            (PaperSize::USExecutive, "Executive"),
        ],
    ),
    (
        "Cards",
        true,
        &[
            (PaperSize::BusinessCard, "Business Card"),
            (PaperSize::IndexCard3x5, "Index 3x5"),
            (PaperSize::IndexCard4x6, "Index 4x6"),
            (PaperSize::IndexCard5x8, "Index 5x8"),
        ],
    ),
    (
        "Photos",
        true,
        &[
            (PaperSize::Photo4x6, "Photo 4x6"),
            (PaperSize::Photo5x7, "Photo 5x7"),
            (PaperSize::Photo8x10, "Photo 8x10"),
            (PaperSize::B4, "B4"),
        ],
    ),
];

/// Computes a uniform scale factor for a group so all papers in the
/// group use the same scale and relative sizes are correct.
fn group_scale(sizes: &[(PaperSize, &str)], max_pair_w: f32) -> f32 {
    let max_h = 120.0; // pt — cap row height
    let mut scale = 1.0_f32;
    for (size, _) in sizes {
        let (pw, ph) = size.portrait();
        let (lw, lh) = size.landscape();
        let natural_w = pw.0 + PAIR_GAP + lw.0;
        let natural_h = ph.0.max(lh.0);
        let s = (max_pair_w / natural_w).min(max_h / natural_h);
        scale = scale.min(s);
    }
    scale.min(1.0)
}

/// Returns the widest portrait width (in mm) across all sizes in the group.
fn max_portrait_width(sizes: &[(PaperSize, &str)]) -> f32 {
    sizes
        .iter()
        .map(|(size, _)| size.portrait().0.0)
        .reduce(f32::max)
        .unwrap_or(0.0)
}

/// Returns the widest landscape width (in mm) across all sizes in the group.
fn max_landscape_width(sizes: &[(PaperSize, &str)]) -> f32 {
    sizes
        .iter()
        .map(|(size, _)| size.landscape().0.0)
        .reduce(f32::max)
        .unwrap_or(0.0)
}

#[derive(Resource)]
struct Backdrop(Entity);

fn main() {
    App::new()
        .add_plugins((
            DefaultPlugins,
            LagrangePlugin,
            BrpExtrasPlugin::default().port_in_title(PortDisplay::NonDefault),
            WindowManagerPlugin,
            MeshPickingPlugin,
            DiegeticUiPlugin,
        ))
        .add_systems(Startup, setup)
        .add_systems(PostStartup, fit_camera_on_start)
        .run();
}

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let world_h = WORLD_WIDTH * PANEL_HEIGHT / PANEL_WIDTH;

    // ── Backdrop ─────────────────────────────────────────────────────
    let backdrop = commands
        .spawn((
            Mesh3d(meshes.add(Plane3d::new(
                Vec3::Z,
                Vec2::new(WORLD_WIDTH / 2.0 + 0.03, world_h / 2.0 + 0.03),
            ))),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: Color::srgba(0.12, 0.12, 0.14, 0.0),
                alpha_mode: AlphaMode::Blend,
                unlit: true,
                ..default()
            })),
            Transform::from_xyz(0.0, world_h / 2.0, -0.001),
        ))
        .id();
    commands.insert_resource(Backdrop(backdrop));

    // ── Title ────────────────────────────────────────────────────────
    commands.spawn((
        WorldText::new("PaperSize — Portrait & Landscape"),
        WorldTextStyle::new(0.04)
            .with_color(HEADER_COLOR)
            .with_anchor(Anchor::BottomCenter),
        Transform::from_xyz(0.0, world_h + 0.02, 0.0),
    ));

    // ── Main panel ───────────────────────────────────────────────────
    commands.spawn((
        DiegeticPanel::world()
            .size(Pt(PANEL_WIDTH), Pt(PANEL_HEIGHT))
            .world_width(WORLD_WIDTH)
            .anchor(Anchor::TopCenter)
            .with_tree(build_panel())
            .build()
            .expect("valid panel dimensions"),
        Transform::from_xyz(0.0, world_h, 0.0),
    ));

    // ── Lighting ─────────────────────────────────────────────────────
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

    // ── Camera ───────────────────────────────────────────────────────
    commands.spawn((
        OrbitCam {
            focus: Vec3::new(0.0, world_h / 2.0, 0.0),
            radius: Some(3.5),
            yaw: Some(0.0),
            pitch: Some(0.0),
            button_orbit: MouseButton::Middle,
            button_pan: MouseButton::Middle,
            modifier_pan: Some(KeyCode::ShiftLeft),
            input_control: Some(InputControl {
                trackpad: Some(TrackpadInput {
                    behavior:    TrackpadBehavior::BlenderLike {
                        modifier_pan:  Some(KeyCode::ShiftLeft),
                        modifier_zoom: Some(KeyCode::ControlLeft),
                    },
                    sensitivity: 1.0,
                }),
                ..default()
            }),
            zoom_sensitivity: 1.0,
            zoom_lower_limit: 0.000_000_1,
            ..default()
        },
        Projection::Perspective(PerspectiveProjection {
            near: 0.001,
            near_clip_plane: Vec4::new(0.0, 0.0, -1.0, -0.001),
            ..default()
        }),
    ));
}

fn fit_camera_on_start(
    backdrop: Res<Backdrop>,
    cameras: Query<Entity, With<OrbitCam>>,
    mut commands: Commands,
) {
    if let Ok(camera) = cameras.single() {
        commands.trigger(
            ZoomToFit::new(camera, backdrop.0)
                .margin(ZOOM_MARGIN)
                .duration(Duration::from_millis(ZOOM_DURATION_MS)),
        );
    }
}

// ── Panel builder ────────────────────────────────────────────────────

/// Builds the entire panel: outer border, 5 columns (4 groups + commentary).
fn build_panel() -> bevy_diegetic::LayoutTree {
    let col_header = LayoutTextStyle::new(24.0).with_color(HEADER_COLOR);
    let name_style = LayoutTextStyle::new(16.0).with_color(LABEL_COLOR);
    let dim_style = LayoutTextStyle::new(14.0).with_color(NOTE_COLOR);
    let note_style = LayoutTextStyle::new(9.0).with_color(NOTE_COLOR);
    let code_style = LayoutTextStyle::new(8.0).with_color(CODE_COLOR);

    // Column content width = (PANEL_WIDTH - padding*2 - gaps) / 5.
    let col_content_w =
        OUTER_PADDING.mul_add(-2.0, COLUMN_GAP.mul_add(-4.0, PANEL_WIDTH)) / 5.0;
    // Each paper pair (portrait + gap + landscape) must fit in col_content_w - some margin.
    let pair_max_w = col_content_w - 8.0; // leave room for column padding

    let mut builder = LayoutBuilder::new(PANEL_WIDTH, PANEL_HEIGHT);
    builder.with(
        El::new()
            .direction(Direction::LeftToRight)
            .padding(Padding::all(OUTER_PADDING))
            .child_gap(COLUMN_GAP)
            .border(Border::all(Pt(1.0), BORDER_COLOR))
            .width(Sizing::grow_min(0.0))
            .height(Sizing::grow_min(0.0)),
        |b| {
            // 4 paper size columns.
            for (group_name, inches, sizes) in GROUPS {
                let scale = group_scale(sizes, pair_max_w);
                let portrait_slot = max_portrait_width(sizes) * scale;
                let landscape_slot = max_landscape_width(sizes) * scale;
                // Equal spacing: left = gap = right. Three equal gaps.
                // col_content_w = 3*gap + portrait_slot + landscape_slot
                let spacing =
                    ((col_content_w - portrait_slot - landscape_slot) / 3.0).max(PAIR_GAP);
                b.with(
                    El::new()
                        .direction(Direction::TopToBottom)
                        .child_gap(ROW_SPACING)
                        .border(Border::all(Pt(0.5), BORDER_COLOR))
                        .padding(Padding::xy(0.0, 6.0))
                        .width(Sizing::fixed(col_content_w))
                        .height(Sizing::grow_min(0.0)),
                    |b| {
                        // Column title.
                        b.text(*group_name, col_header.clone());

                        // 4 paper sizes — each row takes 25% of remaining height.
                        for (size, name) in *sizes {
                            b.with(
                                El::new()
                                    .width(Sizing::grow_min(0.0))
                                    .height(Sizing::Percent(0.25)),
                                |b| {
                                    build_paper_row(
                                        b,
                                        *size,
                                        name,
                                        &PaperRowParams {
                                            scale,
                                            portrait_slot,
                                            landscape_slot,
                                            pair_spacing: spacing,
                                            inches: *inches,
                                            name_style: &name_style,
                                            dim_style: &dim_style,
                                        },
                                    );
                                },
                            );
                        }
                    },
                );
            }

            // Commentary column.
            b.with(
                El::new()
                    .direction(Direction::TopToBottom)
                    .child_gap(8.0)
                    .border(Border::all(Pt(0.5), BORDER_COLOR))
                    .padding(Padding::all(6.0))
                    .width(Sizing::fixed(col_content_w))
                    .height(Sizing::grow_min(0.0)),
                |b| {
                    b.text("PaperSize", col_header);
                    b.text(
                        "28 standard paper, card, photo, and poster \
                         sizes with dimensions in millimeters.",
                        note_style.clone(),
                    );
                    b.text(
                        "Use .portrait() or .landscape() for explicit orientation:",
                        note_style.clone(),
                    );
                    b.text(".size(PaperSize::A4)", code_style.clone());
                    b.text(".size(PaperSize::A4.landscape())", code_style.clone());
                    b.text(".size(PaperSize::BusinessCard.portrait())", code_style);
                    b.text(
                        "Default is natural — portrait for paper, \
                         landscape for cards.",
                        note_style.clone(),
                    );
                    b.text(
                        "Rectangles show true relative proportions \
                         within each group.",
                        note_style,
                    );
                },
            );
        },
    );
    builder.build()
}

/// Parameters for [`build_paper_row`].
struct PaperRowParams<'a> {
    scale:          f32,
    portrait_slot:  f32,
    landscape_slot: f32,
    pair_spacing:   f32,
    inches:         bool,
    name_style:     &'a LayoutTextStyle,
    dim_style:      &'a LayoutTextStyle,
}

/// Adds one paper size row to the builder: name, dimensions, portrait + landscape pair.
fn build_paper_row(b: &mut LayoutBuilder, size: PaperSize, name: &str, params: &PaperRowParams) {
    let (pw, ph) = size.portrait();
    let (lw, lh) = size.landscape();

    let portrait_width = pw.0 * params.scale;
    let portrait_height = ph.0 * params.scale;
    let landscape_width = lw.0 * params.scale;
    let landscape_height = lh.0 * params.scale;

    let label = if params.inches {
        let w_in = pw.0 / 25.4;
        let h_in = ph.0 / 25.4;
        format!("{name} — {w_in:.1}x{h_in:.1}in")
    } else {
        format!("{name} — {:.0}x{:.0}mm", pw.0, ph.0)
    };

    b.with(
        El::new()
            .direction(Direction::TopToBottom)
            .child_gap(2.0)
            .width(Sizing::grow_min(0.0))
            .height(Sizing::fit_min(0.0)),
        |b| {
            b.text(label, params.name_style.clone());

            b.with(
                El::new()
                    .direction(Direction::LeftToRight)
                    .child_gap(params.pair_spacing)
                    .padding(Padding::xy(params.pair_spacing, 0.0))
                    .width(Sizing::grow_min(0.0))
                    .height(Sizing::fit_min(0.0)),
                |b| {
                    b.with(
                        El::new()
                            .direction(Direction::TopToBottom)
                            .child_gap(1.0)
                            .width(Sizing::fixed(params.portrait_slot))
                            .height(Sizing::fit_min(0.0)),
                        |b| {
                            b.with(
                                El::new()
                                    .width(Sizing::fixed(portrait_width))
                                    .height(Sizing::fixed(portrait_height))
                                    .border(Border::all(Pt(0.5), BORDER_COLOR)),
                                |_| {},
                            );
                            b.text("portrait", params.dim_style.clone());
                        },
                    );
                    b.with(
                        El::new()
                            .direction(Direction::TopToBottom)
                            .child_gap(1.0)
                            .width(Sizing::fixed(params.landscape_slot))
                            .height(Sizing::fit_min(0.0)),
                        |b| {
                            b.with(
                                El::new()
                                    .width(Sizing::fixed(landscape_width))
                                    .height(Sizing::fixed(landscape_height))
                                    .border(Border::all(Pt(0.5), BORDER_COLOR)),
                                |_| {},
                            );
                            b.text("landscape", params.dim_style.clone());
                        },
                    );
                },
            );
        },
    );
}
