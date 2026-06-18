//! Batching validation scene for SDF surfaces, text, and analytic shapes.
//!
//! This example is intentionally kept in sync with the material-table batching
//! plan. It starts from today's counters and leaves explicit pending rows for
//! draw/material-table counts that land in later phases.

use bevy::camera::primitives::Aabb;
use bevy::diagnostic::Diagnostic;
use bevy::diagnostic::DiagnosticsStore;
use bevy::diagnostic::FrameTimeDiagnosticsPlugin;
use bevy::prelude::*;
use bevy_diegetic::AlignX;
use bevy_diegetic::AlignY;
use bevy_diegetic::Anchor;
use bevy_diegetic::Border;
use bevy_diegetic::CalloutCap;
use bevy_diegetic::ChildDivider;
use bevy_diegetic::CornerRadius;
use bevy_diegetic::DiegeticPanel;
use bevy_diegetic::DiegeticPanelCommands;
use bevy_diegetic::DiegeticPerfStats;
use bevy_diegetic::El;
use bevy_diegetic::GlyphShadowMode;
use bevy_diegetic::LayoutBuilder;
use bevy_diegetic::LayoutTree;
use bevy_diegetic::Mm;
use bevy_diegetic::Padding;
use bevy_diegetic::PanelCircle;
use bevy_diegetic::PanelDraw;
use bevy_diegetic::PanelLine;
use bevy_diegetic::PanelPoint;
use bevy_diegetic::Sizing;
use bevy_diegetic::TextStyle;
use bevy_diegetic::default_panel_material;
use bevy_kana::ToF32;
use bevy_lagrange::OrbitCamPreset;
use fairy_dust::CameraHomeTarget;
use fairy_dust::StatsPanelRow;
use fairy_dust::TitleBar;
use fairy_dust::diegetic_stats_panel;
use fairy_dust::diegetic_stats_tree;

const PANEL_W: f32 = 120.0;
const PANEL_H: f32 = 92.0;
const PANEL_GAP: f32 = 0.16;
const MM_TO_WORLD: f32 = 0.001;
const PANEL_STEP_X: f32 = (PANEL_W * MM_TO_WORLD) + PANEL_GAP;
const GROUND_SIZE: f32 = 1.45;
const HOME_FOCUS: Vec3 = Vec3::new(0.0, 0.08, 0.0);
const HOME_RADIUS: f32 = 0.95;
const HOME_PITCH: f32 = 0.18;

const AUTHORED_PANELS: usize = 4;
const AUTHORED_SDF_FILLS: usize = 22;
const AUTHORED_SDF_BORDERS: usize = 15;
const AUTHORED_TEXT_RUNS: usize = 23;
const AUTHORED_SHAPE_GROUPS: usize = 9;
const AUTHORED_SHAPE_PRIMITIVES: usize = 20;

const FPS_UPDATE_INTERVAL: f32 = 1.0;
const CARD_RADIUS: Mm = Mm(4.0);
const PANEL_PAD: Mm = Mm(4.0);
const ROW_GAP: f32 = 3.0;
const CARD_BG: Color = Color::srgba(0.055, 0.065, 0.075, 0.94);
const CARD_BG_ALT: Color = Color::srgba(0.075, 0.055, 0.075, 0.94);
const CARD_BORDER: Color = Color::srgba(0.34, 0.56, 0.72, 0.75);
const CARD_BORDER_WARM: Color = Color::srgba(0.84, 0.56, 0.26, 0.78);
const ACCENT_BLUE: Color = Color::srgb(0.24, 0.62, 0.95);
const ACCENT_GREEN: Color = Color::srgb(0.32, 0.88, 0.54);
const ACCENT_YELLOW: Color = Color::srgb(0.95, 0.78, 0.24);
const ACCENT_RED: Color = Color::srgb(0.95, 0.34, 0.30);
const TEXT_MAIN: Color = Color::srgb(0.90, 0.92, 0.96);
const TEXT_MUTED: Color = Color::srgba(0.64, 0.70, 0.78, 0.9);

#[derive(Component)]
struct BatchValidationPanel;

#[derive(Component)]
struct BatchValidationStatsPanel;

#[derive(Resource, Default)]
struct LastDisplayedStats {
    key: String,
}

fn main() {
    // `bevy_diegetic::DiegeticUiPlugin` is registered automatically by
    // `fairy_dust::sprinkle_example`.
    fairy_dust::sprinkle_example()
        .with_brp_extras()
        .with_perf_mode()
        .with_save_window_position()
        .with_studio_lighting()
        .with_ground_plane()
        .size(GROUND_SIZE)
        .with_orbit_cam_preset(
            |cam| {
                cam.focus = HOME_FOCUS;
                cam.radius = Some(HOME_RADIUS);
                cam.yaw = Some(0.0);
                cam.pitch = Some(HOME_PITCH);
            },
            OrbitCamPreset::BlenderLike,
        )
        .with_stable_transparency()
        .with_camera_home()
        .yaw(0.0)
        .pitch(HOME_PITCH)
        .with_title_bar(
            TitleBar::new()
                .with_title("Batch Validation")
                .with_anchor(Anchor::TopLeft),
        )
        .with_camera_control_panel()
        .init_resource::<LastDisplayedStats>()
        .add_systems(Startup, (spawn_validation_panels, spawn_stats_panel))
        .add_systems(Update, update_stats_panel)
        .run();
}

fn spawn_validation_panels(mut commands: Commands) {
    let panels = [
        ("sdf-surfaces", build_sdf_surface_panel()),
        ("text-materials", build_text_panel()),
        ("analytic-shapes", build_shape_panel()),
        ("mixed-stack", build_mixed_panel()),
    ];
    let left = -PANEL_STEP_X * 1.5;
    for (index, (name, tree)) in panels.into_iter().enumerate() {
        let x = index.to_f32().mul_add(PANEL_STEP_X, left);
        let panel = validation_panel(tree, index);
        match panel {
            Ok(panel) => {
                commands.spawn((
                    Name::new(format!("batch validation {name}")),
                    BatchValidationPanel,
                    CameraHomeTarget,
                    panel,
                    Transform::from_xyz(x, 0.08, 0.0),
                ));
            },
            Err(error) => error!("batch_validation: failed to build {name}: {error}"),
        }
    }
    commands.spawn((
        CameraHomeTarget,
        Aabb::from_min_max(Vec3::new(-0.72, -0.02, -0.12), Vec3::new(0.72, 0.26, 0.12)),
        Transform::default(),
    ));
}

fn validation_panel(
    tree: LayoutTree,
    index: usize,
) -> Result<DiegeticPanel, bevy_diegetic::PanelBuildError> {
    let mut material = default_panel_material();
    material.base_color = if index.is_multiple_of(2) {
        CARD_BG
    } else {
        CARD_BG_ALT
    };
    DiegeticPanel::world()
        .size(Mm(PANEL_W), Mm(PANEL_H))
        .anchor(Anchor::Center)
        .material(material)
        .with_tree(tree)
        .build()
}

fn spawn_stats_panel(mut commands: Commands) {
    let rows = validation_stats_rows(None, 0.0, 0.0);
    match diegetic_stats_panel(&rows) {
        Ok(panel) => {
            commands.spawn((BatchValidationStatsPanel, panel, Transform::default()));
        },
        Err(error) => error!("batch_validation: failed to build stats panel: {error}"),
    }
}

fn update_stats_panel(
    time: Res<Time>,
    diagnostics: Res<DiagnosticsStore>,
    diegetic_perf: Res<DiegeticPerfStats>,
    panels: Query<Entity, With<BatchValidationStatsPanel>>,
    mut last: ResMut<LastDisplayedStats>,
    mut commands: Commands,
    mut timer: Local<Option<Timer>>,
) {
    let timer =
        timer.get_or_insert_with(|| Timer::from_seconds(FPS_UPDATE_INTERVAL, TimerMode::Repeating));
    timer.tick(time.delta());
    if !timer.just_finished() {
        return;
    }

    let fps = diagnostics
        .get(&FrameTimeDiagnosticsPlugin::FPS)
        .and_then(Diagnostic::smoothed);
    let rows = validation_stats_rows(Some(&diegetic_perf), fps.unwrap_or(0.0), time.delta_secs());
    let key = stats_key(&rows);
    if key == last.key {
        return;
    }
    last.key = key;
    for panel in &panels {
        commands.set_tree(panel, diegetic_stats_tree(&rows));
    }
}

fn validation_stats_rows(
    perf: Option<&DiegeticPerfStats>,
    fps: f64,
    frame_secs: f32,
) -> Vec<StatsPanelRow> {
    let perf = perf.cloned().unwrap_or_default();
    let batch = &perf.batch;
    let shape_batch = perf.line_batch;
    let sdf_quads = perf.panel_geometry.sdf_quads;
    let current_draws = sdf_quads + batch.batches + shape_batch.batches;
    vec![
        StatsPanelRow::new(
            "profile",
            if cfg!(debug_assertions) {
                "debug"
            } else {
                "release"
            },
        ),
        StatsPanelRow::new("fps", format!("{fps:.0}"))
            .detail(format!("frame: {:.2} ms", frame_secs * 1000.0)),
        StatsPanelRow::new("authored panels", AUTHORED_PANELS.to_string())
            .detail("four world panels, each a different batching case"),
        StatsPanelRow::new("sdf fills", AUTHORED_SDF_FILLS.to_string())
            .detail(format!("live SDF quads today: {sdf_quads}")),
        StatsPanelRow::new("sdf borders", AUTHORED_SDF_BORDERS.to_string())
            .detail("fills and borders should become SDF batch records"),
        StatsPanelRow::new("text runs", batch.runs.to_string())
            .detail(format!("authored target: {AUTHORED_TEXT_RUNS}")),
        StatsPanelRow::new("text glyphs", batch.glyph_records.to_string())
            .detail("live glyph instance records"),
        StatsPanelRow::new("shape groups", AUTHORED_SHAPE_GROUPS.to_string())
            .detail(format!("path records today: {}", shape_batch.records)),
        StatsPanelRow::new("shape primitives", AUTHORED_SHAPE_PRIMITIVES.to_string())
            .detail("lines, arrows, circles, dividers"),
        StatsPanelRow::new("draws today", current_draws.to_string()).detail(format!(
            "sdf {sdf_quads} + text {} + shapes {}",
            batch.batches, shape_batch.batches
        )),
        StatsPanelRow::new("material table", "pending")
            .detail("row/build/upload counts land with the batching phases"),
    ]
}

fn stats_key(rows: &[StatsPanelRow]) -> String {
    let mut key = String::new();
    for row in rows {
        key.push_str(&row.label);
        key.push('=');
        key.push_str(&row.value);
        key.push('|');
        for detail in &row.details {
            key.push_str(detail);
            key.push('|');
        }
    }
    key
}

fn build_sdf_surface_panel() -> LayoutTree {
    let mut builder = panel_root();
    panel_header(
        &mut builder,
        "SDF fills + borders",
        "nested cards, borders, dividers",
    );
    builder.with(
        El::row()
            .width(Sizing::GROW)
            .height(Sizing::GROW)
            .gap(ROW_GAP),
        |builder| {
            for column in 0..3 {
                builder.with(
                    El::column()
                        .width(Sizing::GROW)
                        .height(Sizing::GROW)
                        .gap(ROW_GAP),
                    |builder| {
                        for row in 0..3 {
                            swatch_card(builder, column, row);
                        }
                    },
                );
            }
        },
    );
    builder.build()
}

fn build_text_panel() -> LayoutTree {
    let mut builder = panel_root();
    panel_header(
        &mut builder,
        "Text material cases",
        "shared style, varied scalar values",
    );
    for (label, value, color) in [
        ("base color", "cool blue", ACCENT_BLUE),
        ("emissive", "warm readout", ACCENT_YELLOW),
        ("roughness", "matte copy", TEXT_MUTED),
        ("alpha split", "transparent row", ACCENT_GREEN),
        ("override", "separate material handle", ACCENT_RED),
    ] {
        builder.with(
            El::row()
                .width(Sizing::GROW)
                .height(Sizing::FIT)
                .padding(Padding::new(3.0, 3.0, 2.0, 2.0))
                .corner_radius(CornerRadius::all(Mm(1.5)))
                .background(Color::srgba(0.02, 0.03, 0.04, 0.38))
                .alignment(AlignX::Left, AlignY::Center),
            |builder| {
                builder.with(
                    El::new().width(Sizing::fixed(34.0)).height(Sizing::FIT),
                    |builder| {
                        builder.text(label, body_style(TEXT_MUTED));
                    },
                );
                builder.text(value, body_style(color));
            },
        );
    }
    builder.build()
}

fn build_shape_panel() -> LayoutTree {
    let mut builder = panel_root();
    panel_header(&mut builder, "Analytic shapes", "lines, arrows, circles");
    for (index, color) in [ACCENT_BLUE, ACCENT_GREEN, ACCENT_YELLOW]
        .into_iter()
        .enumerate()
    {
        builder.with(
            El::new()
                .width(Sizing::GROW)
                .height(Sizing::fixed(16.0))
                .draw(PanelDraw::shapes(shape_row(index, color))),
            |_builder| {},
        );
    }
    builder.with(
        El::row()
            .width(Sizing::GROW)
            .height(Sizing::GROW)
            .gap(ROW_GAP),
        |builder| {
            shape_badge(builder, "circle");
            shape_badge(builder, "arrow");
            shape_badge(builder, "line");
        },
    );
    builder.build()
}

fn build_mixed_panel() -> LayoutTree {
    let mut builder = panel_root();
    panel_header(&mut builder, "Mixed stack", "SDF surface + text + shapes");
    builder.with(
        El::column()
            .width(Sizing::GROW)
            .height(Sizing::GROW)
            .gap(ROW_GAP)
            .child_divider(ChildDivider::new(
                Mm(0.35),
                Color::srgba(0.5, 0.7, 0.9, 0.55),
            )),
        |builder| {
            mixed_row(builder, "panel fill", "SDF record", ACCENT_BLUE);
            mixed_row(builder, "run A", "Path text", ACCENT_GREEN);
            mixed_row(builder, "run B", "same batch?", ACCENT_YELLOW);
            mixed_row(builder, "shape", "Path primitive", ACCENT_RED);
        },
    );
    builder.build()
}

fn panel_root() -> LayoutBuilder {
    LayoutBuilder::with_root(
        El::column()
            .width(Sizing::GROW)
            .height(Sizing::GROW)
            .gap(ROW_GAP)
            .padding(Padding::all(PANEL_PAD))
            .border(Border::all(Mm(0.45), CARD_BORDER))
            .corner_radius(CornerRadius::all(CARD_RADIUS)),
    )
}

fn panel_header(builder: &mut LayoutBuilder, title: &str, subtitle: &str) {
    builder.with(
        El::column()
            .width(Sizing::GROW)
            .height(Sizing::FIT)
            .gap(1.0)
            .padding(Padding::new(1.0, 1.0, 0.0, 2.0)),
        |builder| {
            builder.text(title, title_style());
            builder.text(subtitle, small_style(TEXT_MUTED));
        },
    );
}

fn swatch_card(builder: &mut LayoutBuilder, column: usize, row: usize) {
    let accent = match (column + row) % 4 {
        0 => ACCENT_BLUE,
        1 => ACCENT_GREEN,
        2 => ACCENT_YELLOW,
        _ => ACCENT_RED,
    };
    builder.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::GROW)
            .padding(Padding::all(2.0))
            .background(Color::srgba(
                column.to_f32().mul_add(0.025, 0.03),
                0.045,
                0.055,
                0.82,
            ))
            .border(Border::all(Mm(0.3), accent))
            .corner_radius(CornerRadius::all(Mm(1.4)))
            .alignment(AlignX::Center, AlignY::Center),
        |builder| {
            builder.text(
                format!("{}:{}", column + 1, row + 1),
                small_style(TEXT_MAIN),
            );
        },
    );
}

fn shape_row(index: usize, color: Color) -> Vec<bevy_diegetic::PanelShape> {
    let y = 8.0;
    let index_factor = index.to_f32();
    let base = index_factor * 4.0;
    vec![
        PanelLine::new(
            PanelPoint::new(4.0, y),
            PanelPoint::new(34.0, y + base * 0.2),
        )
        .width(Mm(0.45))
        .color(color)
        .end_cap(CalloutCap::arrow().solid().length(4.0).width(3.0))
        .into(),
        PanelCircle::new(PanelPoint::new(48.0, y), Mm(index_factor.mul_add(0.4, 2.2)))
            .color(color)
            .into(),
        PanelLine::new(
            PanelPoint::new(60.0, y + 4.0),
            PanelPoint::new(104.0, y - 4.0),
        )
        .width(Mm(index_factor.mul_add(0.1, 0.25)))
        .color(color)
        .start_cap(CalloutCap::circle().radius(1.8))
        .end_cap(CalloutCap::diamond().width(3.0).height(3.0))
        .into(),
    ]
}

fn shape_badge(builder: &mut LayoutBuilder, label: &str) {
    builder.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::fixed(18.0))
            .background(Color::srgba(0.02, 0.03, 0.04, 0.42))
            .border(Border::all(Mm(0.25), CARD_BORDER_WARM))
            .corner_radius(CornerRadius::all(Mm(1.2)))
            .alignment(AlignX::Center, AlignY::Center),
        |builder| {
            builder.text(label, body_style(TEXT_MAIN));
        },
    );
}

fn mixed_row(builder: &mut LayoutBuilder, label: &str, value: &str, color: Color) {
    builder.with(
        El::row()
            .width(Sizing::GROW)
            .height(Sizing::GROW)
            .gap(ROW_GAP)
            .padding(Padding::new(2.0, 2.0, 2.0, 2.0))
            .background(Color::srgba(0.02, 0.03, 0.04, 0.34))
            .draw(PanelDraw::lines([PanelLine::new(
                PanelPoint::new(4.0, 4.0),
                PanelPoint::new(106.0, 4.0),
            )
            .width(Mm(0.3))
            .color(color)])),
        |builder| {
            builder.with(
                El::new().width(Sizing::fixed(42.0)).height(Sizing::FIT),
                |builder| {
                    builder.text(label, body_style(color));
                },
            );
            builder.text(value, body_style(TEXT_MAIN));
        },
    );
}

fn title_style() -> TextStyle {
    TextStyle::new(8.0)
        .bold()
        .with_color(TEXT_MAIN)
        .with_shadow_mode(GlyphShadowMode::None)
}

fn body_style(color: Color) -> TextStyle {
    TextStyle::new(5.3)
        .with_color(color)
        .with_shadow_mode(GlyphShadowMode::None)
}

fn small_style(color: Color) -> TextStyle {
    TextStyle::new(4.0)
        .with_color(color)
        .with_shadow_mode(GlyphShadowMode::None)
}
