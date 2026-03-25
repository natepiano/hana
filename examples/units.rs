//! Unit system demo — real physical sizes.
//!
//! Two panels at their true physical scale in a world where 1 unit = 1 meter:
//!
//! - **Left**: A4 page (210 × 297 mm) with metric rulers (cm/mm ticks).
//! - **Right**: US business card (3½ × 2 inches) with imperial rulers (⅛″ ticks).
//!
//! Font sizes are specified in typographic points. The unit system converts
//! them to each panel's layout unit automatically.
//!
//! Rulers are spawned as retained gizmo children of each panel — they follow
//! the panel's transform automatically.

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
use bevy_diegetic::LayoutBuilder;
use bevy_diegetic::LayoutTextStyle;
use bevy_diegetic::Padding;
use bevy_diegetic::Sizing;
use bevy_diegetic::Unit;
use bevy_panorbit_camera::PanOrbitCamera;
use bevy_panorbit_camera::PanOrbitCameraPlugin;
use bevy_panorbit_camera::TrackpadBehavior;
use bevy_panorbit_camera_ext::PanOrbitCameraExtPlugin;
use bevy_panorbit_camera_ext::ZoomToFit;
use bevy_window_manager::WindowManagerPlugin;

// ── A4 dimensions ────────────────────────────────────────────────────
const A4_W: f32 = 210.0; // mm
const A4_H: f32 = 297.0; // mm

// ── US business card dimensions ──────────────────────────────────────
const CARD_W: f32 = 3.5; // inches
const CARD_H: f32 = 2.0; // inches

// ── Conversion ───────────────────────────────────────────────────────
const MM_TO_M: f32 = 0.001;
const IN_TO_M: f32 = 0.0254;

// ── Scene layout ─────────────────────────────────────────────────────
const GAP: f32 = 0.015;
const LIFT: f32 = 0.01;

// ── Ruler ────────────────────────────────────────────────────────────
const RULER_GAP: f32 = 0.003;
const RULER_Z: f32 = 0.0005;
const CM_TICK: f32 = 0.005;
const MM5_TICK: f32 = 0.0035;
const MM1_TICK: f32 = 0.002;
const INCH_TICK: f32 = 0.005;
const HALF_TICK: f32 = 0.004;
const QTR_TICK: f32 = 0.003;
const EIGHTH_TICK: f32 = 0.002;
const RULER_LINE_WIDTH: f32 = 1.0;

// ── Zoom ─────────────────────────────────────────────────────────────
const ZOOM_MARGIN: f32 = 0.08;
const ZOOM_DURATION_MS: u64 = 1000;

#[derive(Resource)]
struct SceneBounds(Entity);

fn main() {
    App::new()
        .add_plugins((
            DefaultPlugins,
            PanOrbitCameraPlugin,
            PanOrbitCameraExtPlugin,
            BrpExtrasPlugin::default().port_in_title(PortDisplay::NonDefault),
            WindowManagerPlugin,
            MeshPickingPlugin,
            DiegeticUiPlugin,
        ))
        .add_systems(Startup, setup)
        .add_systems(Update, dynamic_near_far)
        .run();
}

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut gizmo_assets: ResMut<Assets<GizmoAsset>>,
) {
    let a4_w_m = A4_W * MM_TO_M;
    let a4_h_m = A4_H * MM_TO_M;
    let card_w_m = CARD_W * IN_TO_M;
    let card_h_m = CARD_H * IN_TO_M;

    let total_w = a4_w_m + GAP + card_w_m;
    let group_left = -total_w / 2.0;

    // A4 center: left-aligned in the group
    let a4_x = group_left + a4_w_m / 2.0;
    let a4_y = a4_h_m / 2.0 + LIFT;

    // Card center: right of A4, top-aligned with A4 top
    let a4_top = a4_y + a4_h_m / 2.0;
    let card_x = group_left + a4_w_m + GAP + card_w_m / 2.0;
    let card_y = a4_top - card_h_m / 2.0; // top-aligned

    let text_color = Color::WHITE;
    let dim_color = Color::srgba(0.6, 0.6, 0.6, 0.8);
    let ruler_color = Color::srgba(0.55, 0.55, 0.55, 0.7);

    // ── A4 page ──────────────────────────────────────────────────────
    let a4_ruler = build_metric_ruler(a4_w_m, a4_h_m, ruler_color);
    let a4_entity = commands
        .spawn((
            DiegeticPanel {
                tree: build_a4_page(text_color, dim_color),
                width: A4_W,
                height: A4_H,
                layout_unit: Some(Unit::Millimeters),
                ..default()
            },
            Transform::from_xyz(a4_x, a4_y, 0.0),
        ))
        .observe(on_panel_clicked)
        .id();

    commands.entity(a4_entity).with_child((
        Gizmo {
            handle: gizmo_assets.add(a4_ruler),
            line_config: GizmoLineConfig {
                width: RULER_LINE_WIDTH,
                ..default()
            },
            ..default()
        },
        Transform::IDENTITY,
    ));

    // ── Business card ────────────────────────────────────────────────
    let card_ruler = build_inch_ruler(card_w_m, card_h_m, ruler_color);
    let card_entity = commands
        .spawn((
            DiegeticPanel {
                tree: build_card(text_color, dim_color),
                width: CARD_W,
                height: CARD_H,
                layout_unit: Some(Unit::Inches),
                ..default()
            },
            Transform::from_xyz(card_x, card_y, 0.0),
        ))
        .observe(on_panel_clicked)
        .id();

    commands.entity(card_entity).with_child((
        Gizmo {
            handle: gizmo_assets.add(card_ruler),
            line_config: GizmoLineConfig {
                width: RULER_LINE_WIDTH,
                ..default()
            },
            ..default()
        },
        Transform::IDENTITY,
    ));

    // ── Ground plane ─────────────────────────────────────────────────
    let ground_w = total_w + 0.06;
    let ground_h = a4_h_m + 0.06;
    let ground = commands
        .spawn((
            Mesh3d(meshes.add(Plane3d::default().mesh().size(ground_w, ground_h))),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: Color::srgb(0.08, 0.08, 0.08),
                double_sided: true,
                cull_mode: None,
                ..default()
            })),
        ))
        .observe(on_ground_clicked)
        .id();
    commands.insert_resource(SceneBounds(ground));

    // ── Light + camera ───────────────────────────────────────────────
    commands.spawn((
        DirectionalLight {
            shadows_enabled: true,
            ..default()
        },
        Transform::from_xyz(0.5, 1.5, 1.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));

    let mid_y = a4_h_m / 2.0 + LIFT;
    commands.spawn((PanOrbitCamera {
        focus: Vec3::new(0.0, mid_y, 0.0),
        radius: Some(0.5),
        yaw: Some(0.0),
        pitch: Some(0.1),
        button_orbit: MouseButton::Middle,
        button_pan: MouseButton::Middle,
        modifier_pan: Some(KeyCode::ShiftLeft),
        trackpad_behavior: TrackpadBehavior::BlenderLike {
            modifier_pan:  Some(KeyCode::ShiftLeft),
            modifier_zoom: Some(KeyCode::ControlLeft),
        },
        trackpad_sensitivity: 0.5,
        trackpad_pinch_to_zoom_enabled: true,
        zoom_lower_limit: 0.0000001,
        ..default()
    },
    Projection::Perspective(PerspectiveProjection {
        near: 0.001,
        near_clip_plane: Vec4::new(0.0, 0.0, -1.0, -0.001),
        ..default()
    }),
    ));
}

/// Tightens near/far planes proportionally to camera radius.
/// Keeps the near:far ratio constant regardless of zoom level,
/// preventing depth clipping at close range.
fn dynamic_near_far(mut cameras: Query<(&mut Projection, &mut PanOrbitCamera)>) {
    for (mut proj, mut poc) in &mut cameras {
        if let Projection::Perspective(ref mut p) = *proj {
            let radius = poc.radius.unwrap_or(1.0);

            let new_near = (radius * 0.001).max(1e-6);
            let new_far = (radius * 100.0).max(1000.0);

            if (p.near - new_near).abs() > new_near * 0.1
                || (p.far - new_far).abs() > new_far * 0.1
            {
                p.near = new_near;
                p.far = new_far;
                p.near_clip_plane = Vec4::new(0.0, 0.0, -1.0, -new_near);
                poc.force_update = true;
            }
        }
    }
}

// ── Panel content ────────────────────────────────────────────────────

fn build_a4_page(text_color: Color, dim_color: Color) -> bevy_diegetic::LayoutTree {
    let mut builder = LayoutBuilder::new(A4_W, A4_H);

    builder.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::GROW)
            .padding(Padding::all(15.0))
            .direction(Direction::TopToBottom)
            .child_gap(2.0)
            .border(Border::all(0.3, Color::srgba(0.4, 0.4, 0.4, 0.5))),
        |b| {
            b.text(
                "A4 Paper — 210 × 297 mm",
                LayoutTextStyle::new(14.0).with_color(text_color),
            );
            b.text(
                "layout: Millimeters  |  fonts: Points",
                LayoutTextStyle::new(8.0).with_color(dim_color),
            );
            b.text("36pt", LayoutTextStyle::new(36.0).with_color(text_color));
            b.text("24pt", LayoutTextStyle::new(24.0).with_color(text_color));
            b.text("18pt", LayoutTextStyle::new(18.0).with_color(text_color));
            b.text("12pt", LayoutTextStyle::new(12.0).with_color(text_color));
            b.text("9pt", LayoutTextStyle::new(9.0).with_color(text_color));
        },
    );

    builder.build()
}

fn build_card(text_color: Color, dim_color: Color) -> bevy_diegetic::LayoutTree {
    let mut builder = LayoutBuilder::new(CARD_W, CARD_H);

    builder.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::GROW)
            .padding(Padding::all(0.15))
            .direction(Direction::TopToBottom)
            .child_gap(0.04)
            .border(Border::all(0.03, Color::srgba(0.4, 0.4, 0.4, 0.5))),
        |b| {
            let db = Border::all(0.005, Color::srgba(1.0, 0.3, 0.3, 0.5));

            // ── Main content (top) ───────────────────────────────────
            b.with(El::new().border(db), |b| {
                b.text(
                    "JANE DOE",
                    LayoutTextStyle::new(18.0).with_color(text_color),
                );
            });
            b.with(El::new().border(db), |b| {
                b.text(
                    "Software Engineer",
                    LayoutTextStyle::new(12.0).with_color(dim_color),
                );
            });
            b.with(El::new().border(db), |b| {
                b.text(
                    "jane@example.com",
                    LayoutTextStyle::new(10.0).with_color(text_color),
                );
            });
            b.with(El::new().border(db), |b| {
                b.text(
                    "+1 (555) 012-3456",
                    LayoutTextStyle::new(10.0).with_color(text_color),
                );
            });

            // ── Spacer pushes footer to bottom ───────────────────────
            b.with(
                El::new()
                    .width(Sizing::GROW)
                    .height(Sizing::GROW)
                    .border(db),
                |_| {},
            );

            // ── Footer row ───────────────────────────────────────────
            b.with(
                El::new()
                    .width(Sizing::GROW)
                    .direction(Direction::LeftToRight)
                    .border(db),
                |b| {
                    b.with(El::new().border(db), |b| {
                        b.text(
                            "layout: Inches  |  fonts: Points",
                            LayoutTextStyle::new(6.0).with_color(dim_color),
                        );
                    });
                    b.with(El::new().width(Sizing::GROW).border(db), |_| {});
                    b.with(El::new().border(db), |b| {
                        b.text(
                            "3½ × 2 in",
                            LayoutTextStyle::new(10.0).with_color(dim_color),
                        );
                    });
                },
            );
        },
    );

    builder.build()
}

// ── Ruler builders ───────────────────────────────────────────────────
//
// Rulers are built in panel-local space (origin at panel center, Y-up).
// They're spawned as retained gizmo children so they follow the panel.

/// Builds a metric ruler gizmo (left + bottom) for a panel of the given world size.
fn build_metric_ruler(w: f32, h: f32, color: Color) -> GizmoAsset {
    let mut gizmo = GizmoAsset::default();

    let half_w = w / 2.0;
    let half_h = h / 2.0;

    // Left ruler: vertical spine
    let vx = -half_w - RULER_GAP;
    let bottom = -half_h;
    let top = half_h;
    gizmo.line(
        Vec3::new(vx, bottom, RULER_Z),
        Vec3::new(vx, top, RULER_Z),
        color,
    );

    // Bottom ruler: horizontal spine
    let hy = -half_h - RULER_GAP;
    let left = -half_w;
    let right = half_w;
    gizmo.line(
        Vec3::new(left, hy, RULER_Z),
        Vec3::new(right, hy, RULER_Z),
        color,
    );

    // Corner
    gizmo.line(
        Vec3::new(vx, hy, RULER_Z),
        Vec3::new(vx, bottom, RULER_Z),
        color,
    );
    gizmo.line(
        Vec3::new(vx, hy, RULER_Z),
        Vec3::new(left, hy, RULER_Z),
        color,
    );

    // Vertical ticks (extend left from spine)
    let h_mm = (h / MM_TO_M).round() as i32;
    for mm in 0..=h_mm {
        let y = bottom + mm as f32 * MM_TO_M;
        let len = mm_tick_len(mm);
        gizmo.line(
            Vec3::new(vx, y, RULER_Z),
            Vec3::new(vx - len, y, RULER_Z),
            color,
        );
    }

    // Horizontal ticks (extend down from spine)
    let w_mm = (w / MM_TO_M).round() as i32;
    for mm in 0..=w_mm {
        let x = left + mm as f32 * MM_TO_M;
        let len = mm_tick_len(mm);
        gizmo.line(
            Vec3::new(x, hy, RULER_Z),
            Vec3::new(x, hy - len, RULER_Z),
            color,
        );
    }

    gizmo
}

/// Builds an imperial ruler gizmo (right + bottom) for a panel of the given world size.
fn build_inch_ruler(w: f32, h: f32, color: Color) -> GizmoAsset {
    let mut gizmo = GizmoAsset::default();

    let half_w = w / 2.0;
    let half_h = h / 2.0;

    // Right ruler: vertical spine
    let vx = half_w + RULER_GAP;
    let bottom = -half_h;
    let top = half_h;
    gizmo.line(
        Vec3::new(vx, bottom, RULER_Z),
        Vec3::new(vx, top, RULER_Z),
        color,
    );

    // Bottom ruler: horizontal spine
    let hy = -half_h - RULER_GAP;
    let left = -half_w;
    let right = half_w;
    gizmo.line(
        Vec3::new(left, hy, RULER_Z),
        Vec3::new(right, hy, RULER_Z),
        color,
    );

    // Corner
    gizmo.line(
        Vec3::new(vx, hy, RULER_Z),
        Vec3::new(vx, bottom, RULER_Z),
        color,
    );
    gizmo.line(
        Vec3::new(vx, hy, RULER_Z),
        Vec3::new(right, hy, RULER_Z),
        color,
    );

    // Vertical ticks (extend right from spine)
    let eighth_m = IN_TO_M / 8.0;
    let h_eighths = (h / IN_TO_M * 8.0).round() as i32;
    for eighth in 0..=h_eighths {
        let y = bottom + eighth as f32 * eighth_m;
        let len = inch_tick_len(eighth);
        gizmo.line(
            Vec3::new(vx, y, RULER_Z),
            Vec3::new(vx + len, y, RULER_Z),
            color,
        );
    }

    // Horizontal ticks (extend down from spine)
    let w_eighths = (w / IN_TO_M * 8.0).round() as i32;
    for eighth in 0..=w_eighths {
        let x = left + eighth as f32 * eighth_m;
        let len = inch_tick_len(eighth);
        gizmo.line(
            Vec3::new(x, hy, RULER_Z),
            Vec3::new(x, hy - len, RULER_Z),
            color,
        );
    }

    gizmo
}

/// Tick length for a millimeter mark on a metric ruler.
fn mm_tick_len(mm: i32) -> f32 {
    if mm % 10 == 0 {
        CM_TICK
    } else if mm % 5 == 0 {
        MM5_TICK
    } else {
        MM1_TICK
    }
}

/// Tick length for an eighth-inch mark on an imperial ruler.
fn inch_tick_len(eighth: i32) -> f32 {
    if eighth % 8 == 0 {
        INCH_TICK
    } else if eighth % 4 == 0 {
        HALF_TICK
    } else if eighth % 2 == 0 {
        QTR_TICK
    } else {
        EIGHTH_TICK
    }
}

// ── Click handlers ───────────────────────────────────────────────────

fn on_panel_clicked(mut click: On<Pointer<Click>>, mut commands: Commands) {
    if click.button != PointerButton::Primary {
        return;
    }
    click.propagate(false);
    let camera = click.hit.camera;
    commands.trigger(
        ZoomToFit::new(camera, click.entity)
            .margin(ZOOM_MARGIN)
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
            .margin(ZOOM_MARGIN)
            .duration(Duration::from_millis(ZOOM_DURATION_MS)),
    );
}
