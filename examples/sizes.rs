//! @generated `bevy_example_template`
//! Font size newtypes — `Pt`, `Mm`, `In`, and bare `f32`.
//!
//! Left: a [`DiegeticPanel`] with four rows, each using a different size
//! newtype. Center: standalone [`WorldText`] entities with matching sizes.
//! Right: commentary panel explaining how the unit system works.
//!
//! All "Hello" samples render at the same height (~6.35mm), showing that
//! the unit system converts consistently across both rendering paths.

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
use bevy_diegetic::In;
use bevy_diegetic::LayoutBuilder;
use bevy_diegetic::LayoutTextStyle;
use bevy_diegetic::Mm;
use bevy_diegetic::Padding;
use bevy_diegetic::Pt;
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

// ── Layout ───────────────────────────────────────────────────────────
const PANEL_WIDTH: f32 = 60.0; // mm
const PANEL_HEIGHT: f32 = 42.0; // mm — content only (no title row)
const PANEL_PAD: f32 = 3.0; // mm
const ROW_GAP: f32 = 1.5; // mm between rows
const LABEL_COL: f32 = 22.0; // mm — fixed label column
const COL_GAP: f32 = 0.008; // meters between columns
const HEADER_GAP: f32 = 0.003; // meters between header and content
const MARGIN: f32 = 0.004; // meters — backdrop overshoot
const MM_TO_M: f32 = 0.001;

// ── Commentary panel ─────────────────────────────────────────────────
const NOTE_WIDTH: f32 = 80.0; // mm
const NOTE_HEIGHT: f32 = 50.0; // mm — content only (no title row)
const NOTE_PAD: f32 = 3.5; // mm
const NOTE_FONT: f32 = 8.0; // pt

// ── Sizes — all target ~6.35mm (18pt) ────────────────────────────────
const SIZE_PT: f32 = 18.0; // 18pt = 6.35mm
const SIZE_MM: f32 = 6.35; // 6.35mm
const SIZE_IN: f32 = 0.25; // 1/4 inch = 6.35mm
const SIZE_BARE_PANEL: f32 = 18.0; // 18pt (panel font_unit = Points)
const SIZE_BARE_WORLD: f32 = 0.00635; // 6.35mm in meters

// ── Visual ───────────────────────────────────────────────────────────
const LABEL_COLOR: Color = Color::srgba(0.55, 0.55, 0.55, 0.9);
const SAMPLE_COLOR: Color = Color::WHITE;
const HEADER_COLOR: Color = Color::srgba(0.65, 0.75, 0.95, 1.0);
const BORDER_COLOR: Color = Color::srgba(0.35, 0.35, 0.45, 0.5);
const BORDER_WIDTH: f32 = 0.25; // mm

// ── Row labels ───────────────────────────────────────────────────────
const WORLD_LABELS: &[&str] = &["Pt(18)", "Mm(6.35)", "In(0.25)", "0.00635"];

// ── Zoom ─────────────────────────────────────────────────────────────
const ZOOM_MARGIN: f32 = 0.06;
const ZOOM_DURATION_MS: u64 = 600;

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
    let panel_w = PANEL_WIDTH * MM_TO_M;
    let panel_h = PANEL_HEIGHT * MM_TO_M;
    let note_w = NOTE_WIDTH * MM_TO_M;
    let note_h = NOTE_HEIGHT * MM_TO_M;

    // Three columns: demo panel | WorldText | commentary.
    // WorldText column is narrower than the panel — size to actual content.
    let wt_col_w = (LABEL_COL + 28.0) * MM_TO_M; // label col + "Hello" width
    let total_w = panel_w + COL_GAP + wt_col_w + COL_GAP + note_w;
    let left_x = -total_w / 2.0;
    let right_x = left_x + panel_w + COL_GAP;
    let note_x = right_x + wt_col_w + COL_GAP;

    // All three headers top-align at this Y. Content sits below.
    let header_style = WorldTextStyle::new(Pt(9.0))
        .with_color(HEADER_COLOR)
        .with_anchor(Anchor::TopLeft);
    let max_content_h = panel_h.max(note_h);
    let lift = max_content_h * 0.10; // shift everything up 10%
    let header_y = max_content_h + HEADER_GAP + lift;
    let content_top = Pt(9.0)
        .0
        .mul_add(-Unit::Points.meters_per_unit(), header_y - HEADER_GAP);

    let total_h = header_y;
    spawn_backdrop(&mut commands, &mut meshes, &mut materials, total_w, total_h);
    spawn_headers(&mut commands, &header_style, left_x, note_x, header_y);
    spawn_panels(&mut commands, left_x, note_x, content_top);
    spawn_world_text_column(&mut commands, header_style, right_x, header_y);
    spawn_lighting_and_camera(&mut commands, total_h);
}

fn spawn_backdrop(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    total_w: f32,
    total_h: f32,
) {
    let backdrop = commands
        .spawn((
            Mesh3d(meshes.add(Plane3d::new(
                Vec3::Z,
                Vec2::new(total_w / 2.0 + MARGIN, total_h / 2.0 + MARGIN),
            ))),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: Color::srgba(0.12, 0.12, 0.14, 0.0),
                alpha_mode: AlphaMode::Blend,
                unlit: true,
                ..default()
            })),
            Transform::from_xyz(0.0, total_h / 2.0, -0.001),
        ))
        .id();
    commands.insert_resource(Backdrop(backdrop));
}

fn spawn_headers(
    commands: &mut Commands,
    header_style: &WorldTextStyle,
    left_x: f32,
    note_x: f32,
    header_y: f32,
) {
    commands.spawn((
        WorldText::new("DiegeticPanel"),
        header_style.clone(),
        Transform::from_xyz(left_x, header_y, 0.0),
    ));
    commands.spawn((
        WorldText::new("How font sizes work"),
        header_style.clone(),
        Transform::from_xyz(note_x, header_y, 0.0),
    ));
}

fn spawn_panels(commands: &mut Commands, left_x: f32, note_x: f32, content_top: f32) {
    commands.spawn((
        DiegeticPanel {
            tree: build_demo_panel(),
            width: PANEL_WIDTH,
            height: PANEL_HEIGHT,
            layout_unit: Some(Unit::Millimeters),
            anchor: Anchor::TopLeft,
            ..default()
        },
        Transform::from_xyz(left_x, content_top, 0.0),
    ));
    commands.spawn((
        DiegeticPanel {
            tree: build_commentary(),
            width: NOTE_WIDTH,
            height: NOTE_HEIGHT,
            layout_unit: Some(Unit::Millimeters),
            anchor: Anchor::TopLeft,
            ..default()
        },
        Transform::from_xyz(note_x, content_top, 0.0),
    ));
}

fn spawn_world_text_column(
    commands: &mut Commands,
    header_style: WorldTextStyle,
    right_x: f32,
    header_y: f32,
) {
    let wt_title = commands
        .spawn((
            WorldText::new("WorldText"),
            header_style,
            Transform::from_xyz(right_x, header_y, 0.0),
        ))
        .id();

    let label_style = WorldTextStyle::new(Pt(8.0))
        .with_color(LABEL_COLOR)
        .with_anchor(Anchor::TopLeft);

    let world_styles: &[WorldTextStyle] = &[
        WorldTextStyle::new(Pt(SIZE_PT)).with_color(SAMPLE_COLOR),
        WorldTextStyle::new(Mm(SIZE_MM)).with_color(SAMPLE_COLOR),
        WorldTextStyle::new(In(SIZE_IN)).with_color(SAMPLE_COLOR),
        WorldTextStyle::new(SIZE_BARE_WORLD).with_color(SAMPLE_COLOR),
    ];

    let first_row_dy = -(PANEL_PAD + BORDER_WIDTH).mul_add(MM_TO_M, HEADER_GAP);
    let sample_dx = LABEL_COL * MM_TO_M;
    let row_step = (SIZE_MM + ROW_GAP + 2.0) * MM_TO_M;

    for (i, (label, style)) in WORLD_LABELS.iter().zip(world_styles.iter()).enumerate() {
        let dy = first_row_dy - row_step * i.to_f32();

        commands.entity(wt_title).with_child((
            WorldText::new(*label),
            label_style.clone(),
            Transform::from_xyz(0.0, dy, 0.0),
        ));
        commands.entity(wt_title).with_child((
            WorldText::new("Hello"),
            style.clone().with_anchor(Anchor::TopLeft),
            Transform::from_xyz(sample_dx, dy, 0.0),
        ));
    }
}

fn spawn_lighting_and_camera(commands: &mut Commands, total_h: f32) {
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

    commands.spawn((
        OrbitCam {
            focus: Vec3::new(0.0, total_h / 2.0, 0.0),
            radius: Some(0.25),
            yaw: Some(0.0),
            pitch: Some(0.0),
            button_orbit: MouseButton::Middle,
            button_pan: MouseButton::Middle,
            modifier_pan: Some(KeyCode::ShiftLeft),
            trackpad_behavior: TrackpadBehavior::BlenderLike {
                modifier_pan:  Some(KeyCode::ShiftLeft),
                modifier_zoom: Some(KeyCode::ControlLeft),
            },
            trackpad_sensitivity: 1.0,
            trackpad_pinch_to_zoom_enabled: true,
            zoom_sensitivity: 1.0,
            zoom_lower_limit: 0.000_000_1,
            ..default()
        },
        Projection::Perspective(PerspectiveProjection {
            near: 0.001,
            near_clip_plane: Vec4::new(0.0, 0.0, -1.0, -0.001),
            ..default()
        }),
        bevy::anti_alias::taa::TemporalAntiAliasing::default(),
    ));
}

/// Fires a [`ZoomToFit`] on the backdrop so the camera frames everything.
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

// ── Panel builders ───────────────────────────────────────────────────

/// Demo panel: four rows of label + sample. Title is external.
fn build_demo_panel() -> bevy_diegetic::LayoutTree {
    let label_style = LayoutTextStyle::new(Pt(8.0)).with_color(LABEL_COLOR);

    let sample_styles: &[(&str, LayoutTextStyle)] = &[
        (
            "Pt(18)",
            LayoutTextStyle::new(Pt(SIZE_PT)).with_color(SAMPLE_COLOR),
        ),
        (
            "Mm(6.35)",
            LayoutTextStyle::new(Mm(SIZE_MM)).with_color(SAMPLE_COLOR),
        ),
        (
            "In(0.25)",
            LayoutTextStyle::new(In(SIZE_IN)).with_color(SAMPLE_COLOR),
        ),
        (
            "18.0",
            LayoutTextStyle::new(SIZE_BARE_PANEL).with_color(SAMPLE_COLOR),
        ),
    ];

    let mut builder = LayoutBuilder::new(PANEL_WIDTH, PANEL_HEIGHT);
    builder.with(
        El::new()
            .direction(Direction::TopToBottom)
            .padding(Padding::all(PANEL_PAD))
            .child_gap(ROW_GAP)
            .border(Border::all(BORDER_WIDTH, BORDER_COLOR))
            .width(Sizing::grow_min(0.0))
            .height(Sizing::grow_min(0.0)),
        |b| {
            for (label, sample_style) in sample_styles {
                b.with(
                    El::new()
                        .direction(Direction::LeftToRight)
                        .width(Sizing::grow_min(0.0))
                        .height(Sizing::fit_min(0.0)),
                    |b| {
                        b.with(
                            El::new()
                                .width(Sizing::fixed(LABEL_COL))
                                .height(Sizing::fit_min(0.0)),
                            |b| {
                                b.text(*label, label_style.clone());
                            },
                        );
                        b.with(
                            El::new()
                                .width(Sizing::grow_min(0.0))
                                .height(Sizing::fit_min(0.0)),
                            |b| {
                                b.text("Hello", sample_style.clone());
                            },
                        );
                    },
                );
            }
        },
    );
    builder.build()
}

/// Commentary panel: word-wrapped explanation. Title is external.
fn build_commentary() -> bevy_diegetic::LayoutTree {
    let note_color = Color::srgba(0.72, 0.72, 0.72, 0.95);
    let note_style = LayoutTextStyle::new(Pt(NOTE_FONT)).with_color(note_color);

    let mut builder = LayoutBuilder::new(NOTE_WIDTH, NOTE_HEIGHT);
    builder.with(
        El::new()
            .direction(Direction::TopToBottom)
            .padding(Padding::all(NOTE_PAD))
            .child_gap(2.5)
            .border(Border::all(BORDER_WIDTH, BORDER_COLOR))
            .width(Sizing::grow_min(0.0))
            .height(Sizing::grow_min(0.0)),
        |b| {
            b.text(
                "Pt (Points), Mm (Millimeters), and In (Inches) are \
                 newtypes that carry the unit. Pt(18), Mm(6.35), and \
                 In(0.25) all describe the same physical size — the \
                 constructor knows how to convert.",
                note_style.clone(),
            );
            b.text(
                "Bare 18.0 uses the contextual default: panel text \
                 inherits font_unit from the panel (Points by default). \
                 WorldText inherits world_font from UnitConfig \
                 (Meters by default).",
                note_style.clone(),
            );
            b.text(
                "Both the DiegeticPanel and the individual WorldText \
                 instances render \"Hello\" at the same height, \
                 showing that Pt, Mm, In, and bare f32 all convert \
                 consistently.",
                note_style,
            );
        },
    );
    builder.build()
}
