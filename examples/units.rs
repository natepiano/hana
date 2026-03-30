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
//! Press **D** to toggle debug outlines. Press **R** to toggle rulers.

use std::time::Duration;

use bevy::camera::visibility::NoFrustumCulling;
use bevy::picking::mesh_picking::MeshPickingPlugin;
use bevy::prelude::*;
use bevy_brp_extras::BrpExtrasPlugin;
use bevy_brp_extras::PortDisplay;
use bevy_diegetic::AlignX;
use bevy_diegetic::Anchor;
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
use bevy_diegetic::WorldText;
use bevy_diegetic::WorldTextStyle;
use bevy_kana::ToF32;
use bevy_kana::ToI32;
use bevy_lagrange::LagrangePlugin;
use bevy_lagrange::OrbitCam;
use bevy_lagrange::TrackpadBehavior;
use bevy_lagrange::ZoomToFit;
use bevy_window_manager::WindowManagerPlugin;

// ── A4 dimensions ────────────────────────────────────────────────────
const A4_W: f32 = 210.0; // mm
const A4_H: f32 = 297.0; // mm

// ── US business card dimensions ──────────────────────────────────────
const CARD_W: f32 = 3.5; // inches
const CARD_H: f32 = 2.0; // inches

// ── Controls panel dimensions (mm) ───────────────────────────────────
const CTRL_W: f32 = 89.0;
const CTRL_H: f32 = 40.0;
const CTRL_FONT: f32 = 12.0;
const CTRL_TITLE_FONT: f32 = 14.0;
const CTRL_ROW_H: f32 = 6.0;

// ── Conversion ───────────────────────────────────────────────────────
const MM_TO_M: f32 = 0.001;
const IN_TO_M: f32 = 0.0254;

// ── Scene layout ─────────────────────────────────────────────────────
const GAP: f32 = 0.015;
const LIFT: f32 = 0.055;

// ── Ruler ────────────────────────────────────────────────────────────
const RULER_GAP: f32 = 0.003;
const RULER_Z: f32 = 0.0;
const CM_TICK: f32 = 0.005;
const MM5_TICK: f32 = 0.0035;
const MM1_TICK: f32 = 0.002;
const INCH_TICK: f32 = 0.005;
const HALF_TICK: f32 = 0.004;
const QTR_TICK: f32 = 0.003;
const EIGHTH_TICK: f32 = 0.002;
const RULER_LINE_WIDTH: f32 = 1.0;
const LABEL_SIZE: f32 = 8.0; // points
const LABEL_GAP: f32 = 0.001;

// ── Zoom ─────────────────────────────────────────────────────────────
const ZOOM_MARGIN: f32 = 0.08;
const ZOOM_DURATION_MS: u64 = 1000;

// ── Colors ───────────────────────────────────────────────────────────
const TEXT_COLOR: Color = Color::WHITE;
const DIM_COLOR: Color = Color::srgba(0.6, 0.6, 0.6, 0.8);

// ── Marker components ────────────────────────────────────────────────

#[derive(Component)]
struct A4Panel;

#[derive(Component)]
struct CardPanel;

#[derive(Component)]
struct RulerContainer;

#[derive(Resource, Default)]
struct DebugOutlines(bool);

#[derive(Resource)]
struct RulersVisible(bool);

impl Default for RulersVisible {
    fn default() -> Self { Self(true) }
}

#[derive(Resource)]
struct SceneBounds(Entity);

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
        .init_resource::<DebugOutlines>()
        .init_resource::<RulersVisible>()
        .add_systems(Startup, setup)
        .add_systems(Update, toggle_debug_outlines)
        .add_systems(Update, toggle_rulers)
        .add_systems(Update, toggle_projection)
        .add_systems(Update, dynamic_near_far)
        .run();
}

#[allow(clippy::similar_names)]
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

    let a4_x = group_left + a4_w_m / 2.0;
    let a4_y = a4_h_m / 2.0 + LIFT;

    let a4_top = a4_y + a4_h_m / 2.0;
    let card_x = group_left + a4_w_m + GAP + card_w_m / 2.0;
    let card_y = a4_top - card_h_m / 2.0;

    let ruler_color = Color::srgba(0.55, 0.55, 0.55, 0.7);
    let label_style = WorldTextStyle::new(LABEL_SIZE)
        .with_unit(Unit::Points)
        .with_color(ruler_color);

    // ── A4 page ──────────────────────────────────────────────────────
    let a4_entity = commands
        .spawn((
            A4Panel,
            DiegeticPanel {
                tree: build_a4_page(false),
                width: A4_W,
                height: A4_H,
                layout_unit: Some(Unit::Millimeters),
                anchor: Anchor::Center,
                ..default()
            },
            Transform::from_xyz(a4_x, a4_y, 0.0),
        ))
        .observe(on_panel_clicked)
        .id();

    spawn_ruler_on_panel(
        &mut commands,
        &mut gizmo_assets,
        a4_entity,
        build_metric_ruler(a4_w_m, a4_h_m, ruler_color),
        |cmd, container| {
            spawn_metric_labels(cmd, container, a4_w_m, a4_h_m, label_style.clone());
        },
    );

    // ── Business card ────────────────────────────────────────────────
    let card_entity = commands
        .spawn((
            CardPanel,
            DiegeticPanel {
                tree: build_card(false),
                width: CARD_W,
                height: CARD_H,
                layout_unit: Some(Unit::Inches),
                anchor: Anchor::Center,
                ..default()
            },
            Transform::from_xyz(card_x, card_y, 0.0),
        ))
        .observe(on_panel_clicked)
        .id();

    spawn_ruler_on_panel(
        &mut commands,
        &mut gizmo_assets,
        card_entity,
        build_inch_ruler(card_w_m, card_h_m, ruler_color),
        |cmd, container| {
            spawn_inch_labels(cmd, container, card_w_m, card_h_m, label_style);
        },
    );

    // ── Controls panel ───────────────────────────────────────────────
    let card_left = card_x - card_w_m / 2.0;
    let card_bottom = card_y - card_h_m / 2.0;
    let ruler_bottom = card_bottom - RULER_GAP - INCH_TICK;
    let ctrl_w_m = CTRL_W * MM_TO_M;
    let ctrl_h_m = CTRL_H * MM_TO_M;
    let ctrl_x = card_left + ctrl_w_m / 2.0;
    let ctrl_y = ruler_bottom - GAP - ctrl_h_m / 2.0;

    commands
        .spawn((
            DiegeticPanel {
                tree: build_controls_panel(),
                width: CTRL_W,
                height: CTRL_H,
                layout_unit: Some(Unit::Millimeters),
                anchor: Anchor::Center,
                ..default()
            },
            Transform::from_xyz(ctrl_x, ctrl_y, 0.0),
        ))
        .observe(on_panel_clicked);

    // ── Ground plane ─────────────────────────────────────────────────
    spawn_ground_plane(&mut commands, &mut meshes, &mut materials, total_w, a4_h_m);

    // ── Light + camera ───────────────────────────────────────────────
    spawn_lights_and_camera(&mut commands, a4_h_m);
}

fn spawn_ground_plane(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    total_width: f32,
    page_height: f32,
) {
    let ground_width = total_width + 0.06;
    let ground_height = page_height + 0.06;
    let ground = commands
        .spawn((
            Mesh3d(meshes.add(Plane3d::default().mesh().size(ground_width, ground_height))),
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
}

fn spawn_lights_and_camera(commands: &mut Commands, page_height: f32) {
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

    let mid_y = page_height / 2.0 + LIFT;
    commands.spawn((
        OrbitCam {
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

const PERSPECTIVE_FOV: f32 = std::f32::consts::FRAC_PI_4;

fn persp_to_ortho_radius(r: f32) -> f32 { r * (PERSPECTIVE_FOV / 2.0).tan() * 2.0 }

fn ortho_to_persp_radius(r: f32) -> f32 { r / ((PERSPECTIVE_FOV / 2.0).tan() * 2.0) }

/// P key: manual ortho/persp toggle.
fn toggle_projection(
    keys: Res<ButtonInput<KeyCode>>,
    mut cameras: Query<(&mut Projection, &mut OrbitCam)>,
) {
    if !keys.just_pressed(KeyCode::KeyP) {
        return;
    }
    for (mut proj, mut poc) in &mut cameras {
        match &*proj {
            Projection::Perspective(_) => {
                let r = poc.radius.unwrap_or(1.0);
                let ortho_r = persp_to_ortho_radius(r);
                poc.radius = Some(ortho_r);
                poc.target_radius = ortho_r;
                *proj = Projection::Orthographic(OrthographicProjection {
                    scaling_mode: bevy::camera::ScalingMode::FixedVertical {
                        viewport_height: 1.0,
                    },
                    far: 40.0,
                    ..OrthographicProjection::default_3d()
                });
            },
            Projection::Orthographic(_) => {
                let r = poc.radius.unwrap_or(1.0);
                let persp_r = ortho_to_persp_radius(r);
                poc.radius = Some(persp_r);
                poc.target_radius = persp_r;
                *proj = Projection::Perspective(PerspectiveProjection {
                    near: 0.001,
                    near_clip_plane: Vec4::new(0.0, 0.0, -1.0, -0.001),
                    fov: PERSPECTIVE_FOV,
                    ..default()
                });
            },
            Projection::Custom(_) => {},
        }
        poc.force_update = true;
    }
}

/// Tightens near/far planes proportionally to camera radius.
/// Keeps the near:far ratio constant regardless of zoom level,
/// preventing depth clipping at close range.
fn dynamic_near_far(mut cameras: Query<(&mut Projection, &mut OrbitCam)>) {
    for (mut proj, mut poc) in &mut cameras {
        if let Projection::Perspective(ref mut p) = *proj {
            let radius = poc.radius.unwrap_or(1.0);

            let new_near = (radius * 0.001).max(1e-6);
            let new_far = (radius * 100.0).max(1000.0);

            if (p.near - new_near).abs() > new_near * 0.1 || (p.far - new_far).abs() > new_far * 0.1
            {
                p.near = new_near;
                p.far = new_far;
                p.near_clip_plane = Vec4::new(0.0, 0.0, -1.0, -new_near);
                poc.force_update = true;
            }
        }
    }
}

// ── Ruler labels ─────────────────────────────────────────────────────

fn spawn_metric_labels(
    commands: &mut Commands,
    container: Entity,
    w: f32,
    h: f32,
    style: WorldTextStyle,
) {
    let half_w = w / 2.0;
    let half_h = h / 2.0;
    let vx = -half_w - RULER_GAP - CM_TICK - LABEL_GAP;
    let hy = -half_h - RULER_GAP - CM_TICK - LABEL_GAP;

    let v_style = style.clone().with_anchor(Anchor::CenterRight);
    let h_style = style.with_anchor(Anchor::TopCenter);

    let h_cm = (h / MM_TO_M / 10.0).floor().to_i32();
    for cm in 1..=h_cm {
        let y = cm.to_f32().mul_add(0.01, -half_h);
        commands.entity(container).with_child((
            WorldText(format!("{cm}")),
            v_style.clone(),
            Transform::from_xyz(vx, y, RULER_Z),
        ));
    }

    let w_cm = (w / MM_TO_M / 10.0).floor().to_i32();
    for cm in 1..=w_cm {
        let x = cm.to_f32().mul_add(0.01, -half_w);
        commands.entity(container).with_child((
            WorldText(format!("{cm}")),
            h_style.clone(),
            Transform::from_xyz(x, hy, RULER_Z),
        ));
    }
}

fn spawn_inch_labels(
    commands: &mut Commands,
    container: Entity,
    w: f32,
    h: f32,
    style: WorldTextStyle,
) {
    let half_w = w / 2.0;
    let half_h = h / 2.0;
    let vx = half_w + RULER_GAP + INCH_TICK + LABEL_GAP;
    let hy = -half_h - RULER_GAP - INCH_TICK - LABEL_GAP;

    let v_style = style.clone().with_anchor(Anchor::CenterLeft);
    let h_style = style.with_anchor(Anchor::TopCenter);

    let h_in = (h / IN_TO_M).floor().to_i32();
    for inch in 1..=h_in {
        let y = inch.to_f32().mul_add(IN_TO_M, -half_h);
        commands.entity(container).with_child((
            WorldText(format!("{inch}")),
            v_style.clone(),
            Transform::from_xyz(vx, y, RULER_Z),
        ));
    }

    let w_in = (w / IN_TO_M).floor().to_i32();
    for inch in 1..=w_in {
        let x = inch.to_f32().mul_add(IN_TO_M, -half_w);
        commands.entity(container).with_child((
            WorldText(format!("{inch}")),
            h_style.clone(),
            Transform::from_xyz(x, hy, RULER_Z),
        ));
    }
}

// ── Toggle systems ───────────────────────────────────────────────────

fn toggle_debug_outlines(
    keys: Res<ButtonInput<KeyCode>>,
    mut debug: ResMut<DebugOutlines>,
    mut a4_panels: Query<&mut DiegeticPanel, With<A4Panel>>,
    mut card_panels: Query<&mut DiegeticPanel, (With<CardPanel>, Without<A4Panel>)>,
) {
    if !keys.just_pressed(KeyCode::KeyD) {
        return;
    }
    debug.0 = !debug.0;
    let on = debug.0;
    bevy::log::info!("debug outlines: {on}");

    for mut panel in &mut a4_panels {
        panel.tree = build_a4_page(on);
    }
    for mut panel in &mut card_panels {
        panel.tree = build_card(on);
    }
}

#[allow(clippy::similar_names)]
fn toggle_rulers(
    keys: Res<ButtonInput<KeyCode>>,
    mut rulers_visible: ResMut<RulersVisible>,
    existing: Query<Entity, With<RulerContainer>>,
    a4_panels: Query<Entity, With<A4Panel>>,
    card_panels: Query<Entity, (With<CardPanel>, Without<A4Panel>)>,
    mut commands: Commands,
    mut gizmo_assets: ResMut<Assets<GizmoAsset>>,
) {
    if !keys.just_pressed(KeyCode::KeyR) {
        return;
    }
    rulers_visible.0 = !rulers_visible.0;

    // Despawn all existing ruler containers.
    for entity in &existing {
        commands.entity(entity).despawn();
    }

    if !rulers_visible.0 {
        bevy::log::info!("rulers hidden");
        return;
    }

    // Respawn rulers.
    let a4_w_m = A4_W * MM_TO_M;
    let a4_h_m = A4_H * MM_TO_M;
    let card_w_m = CARD_W * IN_TO_M;
    let card_h_m = CARD_H * IN_TO_M;
    let ruler_color = Color::srgba(0.55, 0.55, 0.55, 0.7);
    let label_style = WorldTextStyle::new(LABEL_SIZE)
        .with_unit(Unit::Points)
        .with_color(ruler_color);

    for a4_entity in &a4_panels {
        spawn_ruler_on_panel(
            &mut commands,
            &mut gizmo_assets,
            a4_entity,
            build_metric_ruler(a4_w_m, a4_h_m, ruler_color),
            |cmd, container| {
                spawn_metric_labels(cmd, container, a4_w_m, a4_h_m, label_style.clone());
            },
        );
    }

    for card_entity in &card_panels {
        spawn_ruler_on_panel(
            &mut commands,
            &mut gizmo_assets,
            card_entity,
            build_inch_ruler(card_w_m, card_h_m, ruler_color),
            |cmd, container| {
                spawn_inch_labels(cmd, container, card_w_m, card_h_m, label_style.clone());
            },
        );
    }

    bevy::log::info!("rulers shown");
}

fn spawn_ruler_on_panel(
    commands: &mut Commands,
    gizmo_assets: &mut Assets<GizmoAsset>,
    panel_entity: Entity,
    gizmo: GizmoAsset,
    spawn_labels: impl FnOnce(&mut Commands, Entity),
) {
    let container = commands
        .spawn((RulerContainer, Transform::IDENTITY, Visibility::Inherited))
        .id();
    commands.entity(panel_entity).add_child(container);

    commands.entity(container).with_child((
        Gizmo {
            handle: gizmo_assets.add(gizmo),
            line_config: GizmoLineConfig {
                width: RULER_LINE_WIDTH,
                ..default()
            },
            ..default()
        },
        Transform::IDENTITY,
        Visibility::Inherited,
        NoFrustumCulling,
    ));

    spawn_labels(commands, container);
}

// ── Panel content ────────────────────────────────────────────────────

fn debug_border(debug: bool) -> Option<Border> {
    if debug {
        Some(Border::all(0.002, Color::srgba(1.0, 0.3, 0.3, 0.4)))
    } else {
        None
    }
}

fn debug_text(
    b: &mut bevy_diegetic::LayoutBuilder,
    text: &str,
    style: LayoutTextStyle,
    db: Option<Border>,
) {
    if let Some(border) = db {
        b.with(El::new().border(border), |b| {
            b.text(text, style);
        });
    } else {
        b.text(text, style);
    }
}

fn build_a4_page(debug: bool) -> bevy_diegetic::LayoutTree {
    let mut builder = LayoutBuilder::new(A4_W, A4_H);
    let db = debug_border(debug);

    builder.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::GROW)
            .padding(Padding::all(15.0))
            .direction(Direction::TopToBottom)
            .child_gap(2.0)
            .border(Border::all(0.3, Color::srgba(0.4, 0.4, 0.4, 0.5))),
        |b| {
            debug_text(
                b,
                "A4 Paper — 210 × 297 mm",
                LayoutTextStyle::new(14.0).with_color(TEXT_COLOR),
                db,
            );
            debug_text(
                b,
                "layout: Millimeters  |  fonts: Points",
                LayoutTextStyle::new(8.0).with_color(DIM_COLOR),
                db,
            );
            debug_text(
                b,
                "36pt",
                LayoutTextStyle::new(36.0).with_color(TEXT_COLOR),
                db,
            );
            debug_text(
                b,
                "24pt",
                LayoutTextStyle::new(24.0).with_color(TEXT_COLOR),
                db,
            );
            debug_text(
                b,
                "18pt",
                LayoutTextStyle::new(18.0).with_color(TEXT_COLOR),
                db,
            );
            debug_text(
                b,
                "12pt",
                LayoutTextStyle::new(12.0).with_color(TEXT_COLOR),
                db,
            );
            debug_text(
                b,
                "9pt",
                LayoutTextStyle::new(9.0).with_color(TEXT_COLOR),
                db,
            );
        },
    );

    builder.build()
}

fn build_card(debug: bool) -> bevy_diegetic::LayoutTree {
    let mut builder = LayoutBuilder::new(CARD_W, CARD_H);
    let db = debug_border(debug);

    builder.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::GROW)
            .padding(Padding::all(0.15))
            .direction(Direction::TopToBottom)
            .child_gap(0.04)
            .border(Border::all(0.008, Color::srgba(0.4, 0.4, 0.4, 0.5))),
        |b| {
            debug_text(
                b,
                "JANE DOE",
                LayoutTextStyle::new(18.0).with_color(TEXT_COLOR),
                db,
            );
            debug_text(
                b,
                "Software Engineer",
                LayoutTextStyle::new(12.0).with_color(DIM_COLOR),
                db,
            );
            debug_text(
                b,
                "jane@example.com",
                LayoutTextStyle::new(10.0).with_color(TEXT_COLOR),
                db,
            );
            debug_text(
                b,
                "+1 (555) 012-3456",
                LayoutTextStyle::new(10.0).with_color(TEXT_COLOR),
                db,
            );

            // Spacer
            b.with(El::new().width(Sizing::GROW).height(Sizing::GROW), |_| {});

            // Footer
            b.with(
                El::new()
                    .width(Sizing::GROW)
                    .direction(Direction::LeftToRight),
                |b| {
                    debug_text(
                        b,
                        "layout: Inches  |  fonts: Points",
                        LayoutTextStyle::new(8.0).with_color(DIM_COLOR),
                        db,
                    );
                    b.with(El::new().width(Sizing::GROW), |_| {});
                    debug_text(
                        b,
                        "3½ × 2 in",
                        LayoutTextStyle::new(10.0).with_color(DIM_COLOR),
                        db,
                    );
                },
            );
        },
    );

    builder.build()
}

fn build_controls_panel() -> bevy_diegetic::LayoutTree {
    let border_color = Color::srgb(0.4, 0.4, 0.45);
    let divider_color = Color::srgb(0.45, 0.45, 0.5);
    let cfg = LayoutTextStyle::new(CTRL_FONT);
    let title_cfg = LayoutTextStyle::new(CTRL_TITLE_FONT);
    let row_h = Sizing::fixed(CTRL_ROW_H);

    let mut builder = LayoutBuilder::new(CTRL_W, CTRL_H);
    builder.with(
        El::new()
            .width(Sizing::FIT)
            .height(Sizing::FIT)
            .padding(Padding::all(2.0))
            .direction(Direction::TopToBottom)
            .child_gap(1.0)
            .background(Color::srgba(0.1, 0.1, 0.12, 0.85))
            .border(Border::all(0.5, border_color)),
        |b| {
            b.text("controls", title_cfg.with_color(Color::srgb(0.4, 0.5, 0.9)));
            b.with(
                El::new()
                    .width(Sizing::GROW)
                    .height(Sizing::fixed(0.2))
                    .background(divider_color),
                |_| {},
            );
            b.with(
                El::new()
                    .width(Sizing::FIT)
                    .height(Sizing::FIT)
                    .direction(Direction::LeftToRight)
                    .child_gap(1.5),
                |b| {
                    // Key column
                    b.with(
                        El::new()
                            .direction(Direction::TopToBottom)
                            .child_align_x(AlignX::Center),
                        |b| {
                            b.with(El::new().height(row_h), |b| {
                                b.text("d", cfg.clone().with_color(TEXT_COLOR));
                            });
                            b.with(El::new().height(row_h), |b| {
                                b.text("r", cfg.clone().with_color(TEXT_COLOR));
                            });
                            b.with(El::new().height(row_h), |b| {
                                b.text("p", cfg.clone().with_color(TEXT_COLOR));
                            });
                        },
                    );
                    // Arrow column
                    b.with(
                        El::new()
                            .direction(Direction::TopToBottom)
                            .child_align_x(AlignX::Center),
                        |b| {
                            b.with(El::new().height(row_h), |b| {
                                b.text("→", cfg.clone().with_color(DIM_COLOR));
                            });
                            b.with(El::new().height(row_h), |b| {
                                b.text("→", cfg.clone().with_color(DIM_COLOR));
                            });
                            b.with(El::new().height(row_h), |b| {
                                b.text("→", cfg.clone().with_color(DIM_COLOR));
                            });
                        },
                    );
                    // Description column
                    b.with(
                        El::new()
                            .direction(Direction::TopToBottom)
                            .child_align_x(AlignX::Left),
                        |b| {
                            b.with(El::new().height(row_h), |b| {
                                b.text("debug outlines", cfg.clone().with_color(TEXT_COLOR));
                            });
                            b.with(El::new().height(row_h), |b| {
                                b.text("toggle rulers", cfg.clone().with_color(TEXT_COLOR));
                            });
                            b.with(El::new().height(row_h), |b| {
                                b.text("ortho / persp", cfg.clone().with_color(TEXT_COLOR));
                            });
                        },
                    );
                },
            );
        },
    );
    builder.build()
}

// ── Ruler builders ───────────────────────────────────────────────────

fn build_metric_ruler(w: f32, h: f32, color: Color) -> GizmoAsset {
    let mut gizmo = GizmoAsset::default();
    let half_w = w / 2.0;
    let half_h = h / 2.0;

    let vx = -half_w - RULER_GAP;
    let bottom = -half_h;
    let top = half_h;
    gizmo.line(
        Vec3::new(vx, bottom, RULER_Z),
        Vec3::new(vx, top, RULER_Z),
        color,
    );

    let hy = -half_h - RULER_GAP;
    let left = -half_w;
    let right = half_w;
    gizmo.line(
        Vec3::new(left, hy, RULER_Z),
        Vec3::new(right, hy, RULER_Z),
        color,
    );

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

    // Vertical ticks (extend left from spine; first/last extend right to panel edge).
    let h_mm = (h / MM_TO_M).round().to_i32();
    for mm in 0..=h_mm {
        let y = mm.to_f32().mul_add(MM_TO_M, bottom);
        let len = mm_tick_len(mm);
        gizmo.line(
            Vec3::new(vx, y, RULER_Z),
            Vec3::new(vx - len, y, RULER_Z),
            color,
        );
        if mm == 0 || mm == h_mm {
            gizmo.line(
                Vec3::new(vx, y, RULER_Z),
                Vec3::new(-half_w, y, RULER_Z),
                color,
            );
        }
    }

    // Horizontal ticks (extend down from spine; first/last extend up to panel edge).
    let w_mm = (w / MM_TO_M).round().to_i32();
    for mm in 0..=w_mm {
        let x = mm.to_f32().mul_add(MM_TO_M, left);
        let len = mm_tick_len(mm);
        gizmo.line(
            Vec3::new(x, hy, RULER_Z),
            Vec3::new(x, hy - len, RULER_Z),
            color,
        );
        if mm == 0 || mm == w_mm {
            gizmo.line(
                Vec3::new(x, hy, RULER_Z),
                Vec3::new(x, -half_h, RULER_Z),
                color,
            );
        }
    }

    gizmo
}

fn build_inch_ruler(w: f32, h: f32, color: Color) -> GizmoAsset {
    let mut gizmo = GizmoAsset::default();
    let half_w = w / 2.0;
    let half_h = h / 2.0;

    let vx = half_w + RULER_GAP;
    let bottom = -half_h;
    let top = half_h;
    gizmo.line(
        Vec3::new(vx, bottom, RULER_Z),
        Vec3::new(vx, top, RULER_Z),
        color,
    );

    let hy = -half_h - RULER_GAP;
    let left = -half_w;
    let right = half_w;
    gizmo.line(
        Vec3::new(left, hy, RULER_Z),
        Vec3::new(right, hy, RULER_Z),
        color,
    );

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

    // Vertical ticks (extend right from spine; first/last extend left to panel edge).
    let eighth_m = IN_TO_M / 8.0;
    let h_eighths = (h / IN_TO_M * 8.0).round().to_i32();
    for eighth in 0..=h_eighths {
        let y = eighth.to_f32().mul_add(eighth_m, bottom);
        let len = inch_tick_len(eighth);
        gizmo.line(
            Vec3::new(vx, y, RULER_Z),
            Vec3::new(vx + len, y, RULER_Z),
            color,
        );
        if eighth == 0 || eighth == h_eighths {
            gizmo.line(
                Vec3::new(vx, y, RULER_Z),
                Vec3::new(half_w, y, RULER_Z),
                color,
            );
        }
    }

    // Horizontal ticks (extend down from spine; first/last extend up to panel edge).
    let w_eighths = (w / IN_TO_M * 8.0).round().to_i32();
    for eighth in 0..=w_eighths {
        let x = eighth.to_f32().mul_add(eighth_m, left);
        let len = inch_tick_len(eighth);
        gizmo.line(
            Vec3::new(x, hy, RULER_Z),
            Vec3::new(x, hy - len, RULER_Z),
            color,
        );
        if eighth == 0 || eighth == w_eighths {
            gizmo.line(
                Vec3::new(x, hy, RULER_Z),
                Vec3::new(x, -half_h, RULER_Z),
                color,
            );
        }
    }

    gizmo
}

const fn mm_tick_len(mm: i32) -> f32 {
    if mm % 10 == 0 {
        CM_TICK
    } else if mm % 5 == 0 {
        MM5_TICK
    } else {
        MM1_TICK
    }
}

const fn inch_tick_len(eighth: i32) -> f32 {
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
