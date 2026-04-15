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

use bevy::picking::mesh_picking::MeshPickingPlugin;
use bevy::prelude::*;
use bevy_brp_extras::BrpExtrasPlugin;
use bevy_brp_extras::PortDisplay;
use bevy_diegetic::AlignX;
use bevy_diegetic::AlignY;
use bevy_diegetic::Anchor;
use bevy_diegetic::Border;
use bevy_diegetic::CornerRadius;
use bevy_diegetic::DiegeticPanel;
use bevy_diegetic::DiegeticUiPlugin;
use bevy_diegetic::Direction;
use bevy_diegetic::El;
use bevy_diegetic::In;
use bevy_diegetic::LayoutBuilder;
use bevy_diegetic::LayoutTextStyle;
use bevy_diegetic::LayoutTree;
use bevy_diegetic::Mm;
use bevy_diegetic::Padding;
use bevy_diegetic::PaperSize;
use bevy_diegetic::Pt;
use bevy_diegetic::Px;
use bevy_diegetic::Sizing;
use bevy_diegetic::SurfaceShadow;
use bevy_diegetic::WorldText;
use bevy_diegetic::WorldTextStyle;
use bevy_kana::ToF32;
use bevy_kana::ToI32;
use bevy_lagrange::CameraMove;
use bevy_lagrange::ForceUpdate;
use bevy_lagrange::InputControl;
use bevy_lagrange::LagrangePlugin;
use bevy_lagrange::OrbitCam;
use bevy_lagrange::PlayAnimation;
use bevy_lagrange::TrackpadBehavior;
use bevy_lagrange::TrackpadInput;
use bevy_lagrange::ZoomToFit;
use bevy_window_manager::WindowManagerPlugin;

// ── A4 dimensions ────────────────────────────────────────────────────
const A4: PaperSize = PaperSize::A4;
const A4_W: Mm = Mm(A4.width_mm());
const A4_H: Mm = Mm(A4.height_mm());

// ── US business card dimensions ──────────────────────────────────────
const CARD_W: In = In(3.5);
const CARD_H: In = In(2.0);
const CARD_NAME_SIZE: Pt = Pt(15.0);
const CARD_TITLE_SIZE: Pt = Pt(13.0);
const CARD_DETAIL_SIZE: Pt = Pt(11.0);
const CARD_FOOTER_SIZE: Pt = Pt(6.0);

// ── Index card ──────────────────────────────────────────────────────
const INDEX_W: In = In(5.0);
const INDEX_H: In = In(7.0);
const INDEX_BG: Color = Color::srgba(0.08, 0.10, 0.14, 1.0);
const INDEX_HEADING_COLOR: Color = Color::srgb(0.4, 0.85, 0.75);
const INDEX_LABEL_COLOR: Color = Color::srgba(0.7, 0.75, 0.85, 0.9);
const INDEX_CODE_COLOR: Color = Color::srgb(0.95, 0.9, 0.75);
const INDEX_HEADING_SIZE: Pt = Pt(14.0);
const INDEX_SUBHEADING_SIZE: Pt = Pt(11.0);
const INDEX_LABEL_SIZE: Pt = Pt(10.0);
const INDEX_CODE_SIZE: Pt = Pt(10.0);
const INDEX_FOOTER_SIZE: Pt = Pt(8.0);

// ── HUD ─────────────────────────────────────────────────────────────
const HUD_HEIGHT: Px = Px(48.0);
const HUD_PADDING: Px = Px(12.0);
const HUD_GAP: Px = Px(14.0);
const HUD_TITLE_SIZE: Pt = Pt(16.0);
const HUD_HINT_SIZE: Pt = Pt(12.0);
const HUD_BACKGROUND: Color = Color::srgba(0.02, 0.03, 0.07, 0.80);
const HUD_FRAME_BACKGROUND: Color = Color::srgba(0.01, 0.01, 0.03, 0.95);
const HUD_BORDER_ACCENT: Color = Color::srgba(0.15, 0.7, 0.9, 0.5);
const HUD_BORDER_DIM: Color = Color::srgba(0.1, 0.4, 0.6, 0.3);
const HUD_TITLE_COLOR: Color = Color::srgb(0.9, 0.95, 1.0);
const HUD_ACTIVE_COLOR: Color = Color::srgb(0.3, 1.0, 0.8);
const HUD_DIVIDER_COLOR: Color = Color::srgba(0.15, 0.4, 0.6, 0.25);
const HUD_INACTIVE_COLOR: Color = Color::srgba(0.6, 0.65, 0.8, 0.85);

// ── Camera help panel ──────────────────────────────────────────────
const CAM_HELP_WIDTH: Px = Px(280.0);
const CAM_HELP_HEIGHT: Px = Px(160.0);
const CAM_HELP_LABEL_SIZE: Pt = Pt(11.0);
const CAM_HELP_HEADER_SIZE: Pt = Pt(13.0);
const CAM_HELP_TITLE_SIZE: Pt = Pt(16.0);
const CAM_HELP_RADIUS: Px = Px(15.0);
const CAM_HELP_FRAME_PAD: Px = Px(2.0);
const CAM_HELP_BORDER: Px = Px(2.0);
const CAM_HELP_INSET: Px = Px(CAM_HELP_FRAME_PAD.0 + CAM_HELP_BORDER.0);
const CAM_HELP_INNER_RADIUS: Px = Px(CAM_HELP_RADIUS.0 - CAM_HELP_INSET.0);

// ── Scene layout ─────────────────────────────────────────────────────
const GAP: Mm = Mm(15.0);
const LIFT: Mm = Mm(55.0);
const TITLE_GAP: Mm = Mm(8.0);
const GROUND_MARGIN: Mm = Mm(60.0);

// ── Ruler ────────────────────────────────────────────────────────────
const RULER_GAP: Mm = Mm(3.0);
const EDGE_LABEL_EXTRA: In = In(0.5);

// ── Panel ruler — metric (mm units) ─────────────────────────────────
const PANEL_RULER_CM_LINE: Mm = Mm(0.3);
const PANEL_RULER_CM_TICK: Mm = Mm(5.0);
const PANEL_RULER_MM1_LINE: Mm = Mm(0.1);
const PANEL_RULER_MM1_TICK: Mm = Mm(2.0);
const PANEL_RULER_MM5_LINE: Mm = Mm(0.1);
const PANEL_RULER_MM5_TICK: Mm = Mm(3.5);
const PANEL_RULER_SPINE: Mm = Mm(0.2);
const PANEL_RULER_WIDTH: Mm = Mm(10.0);
const PANEL_RULER_MM_LABEL_GAP: Mm = Mm(0.8);

// ── Panel ruler — imperial (inch units) ─────────────────────────────
const PANEL_RULER_16TH_LINE: In = In(0.003);
const PANEL_RULER_16TH_TICK: In = In(0.05);
const PANEL_RULER_8TH_LINE: In = In(0.004);
const PANEL_RULER_8TH_TICK: In = In(0.08);
const PANEL_RULER_HALF_LINE: In = In(0.006);
const PANEL_RULER_HALF_TICK: In = In(0.16);
const PANEL_RULER_INCH_LINE: In = In(0.012);
const PANEL_RULER_INCH_SPINE: In = In(0.008);
const PANEL_RULER_INCH_TICK: In = In(0.2);
const PANEL_RULER_INCH_WIDTH: In = In(0.45);
const PANEL_RULER_QTR_LINE: In = In(0.004);
const PANEL_RULER_QTR_TICK: In = In(0.12);
const PANEL_RULER_IN_LABEL_GAP: In = In(0.0315);

// ── Home / zoom ─────────────────────────────────────────────────────
const HOME_FOCUS_Y: Mm = Mm(A4_H.0 / 2.0 + LIFT.0);
const HOME_PITCH: f32 = 0.1;
const HOME_RADIUS: Mm = Mm(500.0);
const HOME_YAW: f32 = 0.0;
const ZOOM_DURATION_MS: u64 = 1000;
const ZOOM_MARGIN: f32 = 0.08;

// ── Colors ───────────────────────────────────────────────────────────
const A4_DIM_COLOR: Color = Color::srgba(0.0, 0.0, 0.1, 1.0);
const A4_TEXT_COLOR: Color = Color::BLACK;
const CARD_DIM_COLOR: Color = Color::WHITE;
const CARD_TEXT_COLOR: Color = Color::srgb(1.0, 1.0, 0.85);
/// Debug outline thickness shown as a physical 0.3mm line on every panel.
const DEBUG_OUTLINE: Pt = Pt(1.0);

// ── Marker components ────────────────────────────────────────────────

#[derive(Component)]
struct A4Panel;

#[derive(Component)]
struct CardPanel;

#[derive(Component)]
struct IndexPanel;

#[derive(Component)]
struct ControlsPanel;

#[derive(Component)]
struct PanelRuler;

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
        .add_systems(Update, update_controls_hud)
        .add_systems(Update, home_camera)
        .run();
}

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    windows: Query<&Window>,
) {
    let a4_width_m = f32::from(A4_W);
    let a4_height_m = f32::from(A4_H);
    let card_width_m = f32::from(CARD_W);
    let card_height_m = f32::from(CARD_H);
    let gap = f32::from(GAP);
    let lift = f32::from(LIFT);
    let title_gap = f32::from(TITLE_GAP);

    let total_w = a4_width_m + gap + card_width_m;
    let group_left = -total_w / 2.0;

    let a4_x = group_left + a4_width_m / 2.0;
    let a4_y = a4_height_m / 2.0 + lift;

    let a4_top = a4_y + a4_height_m / 2.0;
    let card_x = group_left + a4_width_m + gap + card_width_m / 2.0;
    let card_y = a4_top - card_height_m / 2.0;

    let ruler_color = Color::WHITE;

    // ── A4 page ──────────────────────────────────────────────────────
    commands
        .spawn((
            A4Panel,
            DiegeticPanel::world()
                .paper(PaperSize::A4)
                .anchor(Anchor::Center)
                .surface_shadow(SurfaceShadow::On)
                .layout(|b| build_a4_content(b, false))
                .build()
                .expect("valid A4 dimensions"),
            Transform::from_xyz(a4_x, a4_y, 0.0),
        ))
        .observe(on_panel_clicked);

    // ── Panel titles ────────────────────────────────────────────────
    let title_style = WorldTextStyle::new(Pt(18.0))
        .with_color(Color::WHITE)
        .with_anchor(Anchor::BottomCenter);
    commands.spawn((
        WorldText::new("A4 Paper — 210 × 297 mm"),
        title_style.clone(),
        Transform::from_xyz(a4_x, a4_top + title_gap, 0.0),
    ));
    commands.spawn((
        WorldText::new("US Business Card — 3½ × 2 in"),
        title_style,
        Transform::from_xyz(card_x, a4_top + title_gap, 0.0),
    ));

    spawn_rulers(
        &mut commands,
        ruler_color,
        a4_x,
        a4_y,
        a4_width_m,
        a4_height_m,
        card_x,
        card_y,
        card_width_m,
        card_height_m,
    );

    // ── Business card ────────────────────────────────────────────────
    commands
        .spawn((
            CardPanel,
            DiegeticPanel::world()
                .paper(PaperSize::BusinessCard)
                .anchor(Anchor::Center)
                .surface_shadow(SurfaceShadow::On)
                .layout(|b| build_card_content(b, false))
                .build()
                .expect("valid card dimensions"),
            Transform::from_xyz(card_x, card_y, 0.0),
        ))
        .observe(on_panel_clicked);

    // ── Photo 5×7 (portrait) ───────────────────────────────────────────
    let index_width_m = f32::from(INDEX_W);
    let index_height_m = f32::from(INDEX_H);
    // Left edge aligns with business card left edge.
    let card_left = card_x - card_width_m / 2.0;
    let index_x = card_left + index_width_m / 2.0;
    // Bottom aligns with A4 bottom.
    let a4_bottom = a4_y - a4_height_m / 2.0;
    let index_y = a4_bottom + index_height_m / 2.0;

    commands
        .spawn((
            IndexPanel,
            DiegeticPanel::world()
                .paper(PaperSize::Photo5x7)
                .anchor(Anchor::Center)
                .surface_shadow(SurfaceShadow::On)
                .layout(|b| build_index_content(b, false))
                .build()
                .expect("valid index card dimensions"),
            Transform::from_xyz(index_x, index_y, 0.0),
        ))
        .observe(on_panel_clicked);
    commands.spawn((
        WorldText::new("Photo — 5 × 7 in"),
        WorldTextStyle::new(Pt(18.0))
            .with_color(Color::WHITE)
            .with_anchor(Anchor::BottomCenter),
        Transform::from_xyz(index_x, index_y + index_height_m / 2.0 + title_gap, 0.0),
    ));

    // Index card ruler (right side, like business card).
    let index_ruler_x = index_x + index_width_m / 2.0 + f32::from(RULER_GAP);
    let index_sixteenths = (INDEX_H.0 * 16.0).round().to_i32();
    let index_ruler_height = In(INDEX_H.0 + EDGE_LABEL_EXTRA.0);
    let index_ruler_top = index_y + index_height_m / 2.0 + f32::from(EDGE_LABEL_EXTRA);
    commands.spawn((
        PanelRuler,
        DiegeticPanel::world()
            .size(PANEL_RULER_INCH_WIDTH, index_ruler_height)
            .anchor(Anchor::TopLeft)
            .with_tree(build_imperial_panel_ruler(
                index_sixteenths,
                index_ruler_height,
                ruler_color,
            ))
            .build()
            .expect("valid index vertical ruler dimensions"),
        Transform::from_xyz(index_ruler_x, index_ruler_top, 0.0),
    ));

    // Index card horizontal ruler (bottom).
    let index_bottom_ruler_x = index_x - index_width_m / 2.0;
    let index_bottom_ruler_y = index_y - index_height_m / 2.0 - f32::from(RULER_GAP);
    let index_w_sixteenths = (INDEX_W.0 * 16.0).round().to_i32();
    commands.spawn((
        PanelRuler,
        DiegeticPanel::world()
            .size(INDEX_W, PANEL_RULER_INCH_WIDTH)
            .anchor(Anchor::TopLeft)
            .with_tree(build_imperial_horizontal_ruler(
                index_w_sixteenths,
                ruler_color,
            ))
            .build()
            .expect("valid index horizontal ruler dimensions"),
        Transform::from_xyz(index_bottom_ruler_x, index_bottom_ruler_y, 0.0),
    ));

    spawn_hud_panels(&mut commands, &windows);

    // ── Ground plane ─────────────────────────────────────────────────
    spawn_ground_plane(
        &mut commands,
        &mut meshes,
        &mut materials,
        total_w,
        a4_height_m,
    );

    // ── Light + camera ───────────────────────────────────────────────
    spawn_lights_and_camera(&mut commands, a4_height_m);
}

fn spawn_hud_panels(commands: &mut Commands, windows: &Query<&Window>) {
    let unlit_material = bevy_diegetic::default_panel_material();
    let unlit = StandardMaterial {
        unlit: true,
        ..unlit_material
    };
    let hud_width = windows.iter().next().map_or(800.0, Window::width);
    commands.spawn((
        ControlsPanel,
        DiegeticPanel::screen()
            .size(Px(hud_width), HUD_HEIGHT)
            .anchor(Anchor::TopLeft)
            .material(unlit.clone())
            .text_material(unlit)
            .width_percent(1.0)
            .layout(|b| {
                build_controls_content(b, false, true, true);
            })
            .build()
            .expect("valid controls HUD dimensions"),
        Transform::default(),
    ));

    let cam_unlit = StandardMaterial {
        unlit: true,
        ..bevy_diegetic::default_panel_material()
    };
    commands.spawn((
        DiegeticPanel::screen()
            .size(CAM_HELP_WIDTH, CAM_HELP_HEIGHT)
            .anchor(Anchor::BottomRight)
            .material(cam_unlit.clone())
            .text_material(cam_unlit)
            .layout(build_camera_help)
            .build()
            .expect("valid camera help HUD dimensions"),
        Transform::default(),
    ));
}

fn spawn_rulers(
    commands: &mut Commands,
    ruler_color: Color,
    a4_x: f32,
    a4_y: f32,
    a4_width_m: f32,
    a4_height_m: f32,
    card_x: f32,
    card_y: f32,
    card_width_m: f32,
    card_height_m: f32,
) {
    // A4 vertical ruler (left side).
    let a4_ruler_x = a4_x - a4_width_m / 2.0 - f32::from(RULER_GAP);
    let a4_ruler_top = a4_y + a4_height_m / 2.0;
    commands.spawn((
        PanelRuler,
        DiegeticPanel::world()
            .size(PANEL_RULER_WIDTH, A4_H)
            .anchor(Anchor::TopRight)
            .with_tree(build_metric_panel_ruler(A4_H.0.to_i32(), ruler_color))
            .build()
            .expect("valid A4 vertical ruler dimensions"),
        Transform::from_xyz(a4_ruler_x, a4_ruler_top, 0.0),
    ));

    // A4 horizontal ruler (bottom).
    let a4_bottom_ruler_x = a4_x - a4_width_m / 2.0;
    let a4_bottom_ruler_y = a4_y - a4_height_m / 2.0 - f32::from(RULER_GAP);
    commands.spawn((
        PanelRuler,
        DiegeticPanel::world()
            .size(A4_W, PANEL_RULER_WIDTH)
            .anchor(Anchor::TopLeft)
            .with_tree(build_metric_horizontal_ruler(A4_W.0.to_i32(), ruler_color))
            .build()
            .expect("valid A4 horizontal ruler dimensions"),
        Transform::from_xyz(a4_bottom_ruler_x, a4_bottom_ruler_y, 0.0),
    ));

    // Card vertical ruler (right side).
    let card_ruler_x = card_x + card_width_m / 2.0 + f32::from(RULER_GAP);
    let card_sixteenths = (CARD_H.0 * 16.0).round().to_i32();
    let card_ruler_height = In(CARD_H.0 + EDGE_LABEL_EXTRA.0);
    let card_ruler_top = card_y + card_height_m / 2.0 + f32::from(EDGE_LABEL_EXTRA);
    commands.spawn((
        PanelRuler,
        DiegeticPanel::world()
            .size(PANEL_RULER_INCH_WIDTH, card_ruler_height)
            .anchor(Anchor::TopLeft)
            .with_tree(build_imperial_panel_ruler(
                card_sixteenths,
                card_ruler_height,
                ruler_color,
            ))
            .build()
            .expect("valid card vertical ruler dimensions"),
        Transform::from_xyz(card_ruler_x, card_ruler_top, 0.0),
    ));

    // Card horizontal ruler (bottom).
    let card_bottom_ruler_x = card_x - card_width_m / 2.0;
    let card_bottom_ruler_y = card_y - card_height_m / 2.0 - f32::from(RULER_GAP);
    let card_w_sixteenths = (CARD_W.0 * 16.0).round().to_i32();
    commands.spawn((
        PanelRuler,
        DiegeticPanel::world()
            .size(CARD_W, PANEL_RULER_INCH_WIDTH)
            .anchor(Anchor::TopLeft)
            .with_tree(build_imperial_horizontal_ruler(
                card_w_sixteenths,
                ruler_color,
            ))
            .build()
            .expect("valid card horizontal ruler dimensions"),
        Transform::from_xyz(card_bottom_ruler_x, card_bottom_ruler_y, 0.0),
    ));
}

fn spawn_ground_plane(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    total_width: f32,
    page_height: f32,
) {
    let ground_margin = f32::from(GROUND_MARGIN);
    let ground_width = (total_width + ground_margin) * 1.5;
    let ground_height = page_height + ground_margin;
    let ground = commands
        .spawn((
            Mesh3d(meshes.add(Plane3d::default().mesh().size(ground_width, ground_height))),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: Color::srgb(0.12, 0.08, 0.06),
                reflectance: 0.5,
                perceptual_roughness: 0.6,
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
            illuminance: 5_000.0,
            shadows_enabled: true,
            ..default()
        },
        Transform::from_xyz(0.5, 1.5, 1.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    commands.spawn((
        DirectionalLight {
            illuminance: 500.0,
            shadows_enabled: false,
            ..default()
        },
        Transform::from_xyz(-0.5, 1.5, -1.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));

    let mid_y = page_height / 2.0 + f32::from(LIFT);
    commands.spawn((
        OrbitCam {
            focus: Vec3::new(0.0, mid_y, 0.0),
            radius: Some(f32::from(HOME_RADIUS)),
            yaw: Some(0.0),
            pitch: Some(0.1),
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

const PERSPECTIVE_FOV: f32 = std::f32::consts::FRAC_PI_4;

fn persp_to_ortho_radius(r: f32) -> f32 { r * (PERSPECTIVE_FOV / 2.0).tan() * 2.0 }

fn ortho_to_persp_radius(r: f32) -> f32 { r / ((PERSPECTIVE_FOV / 2.0).tan() * 2.0) }

/// P key: switch to perspective. O key: switch to orthographic.
fn toggle_projection(
    keys: Res<ButtonInput<KeyCode>>,
    mut cameras: Query<(&mut Projection, &mut OrbitCam)>,
) {
    let to_perspective = keys.just_pressed(KeyCode::KeyP);
    let to_ortho = keys.just_pressed(KeyCode::KeyO);
    if !to_perspective && !to_ortho {
        return;
    }
    for (mut proj, mut poc) in &mut cameras {
        if to_ortho && matches!(&*proj, Projection::Perspective(_)) {
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
            poc.force_update = ForceUpdate::Pending;
        } else if to_perspective && matches!(&*proj, Projection::Orthographic(_)) {
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
            poc.force_update = ForceUpdate::Pending;
        }
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
                poc.force_update = ForceUpdate::Pending;
            }
        }
    }
}

// ── Toggle systems ───────────────────────────────────────────────────

fn toggle_debug_outlines(
    keys: Res<ButtonInput<KeyCode>>,
    mut debug: ResMut<DebugOutlines>,
    mut a4_panels: Query<&mut DiegeticPanel, With<A4Panel>>,
    mut card_panels: Query<
        &mut DiegeticPanel,
        (With<CardPanel>, Without<A4Panel>, Without<IndexPanel>),
    >,
    mut index_panels: Query<
        &mut DiegeticPanel,
        (With<IndexPanel>, Without<A4Panel>, Without<CardPanel>),
    >,
) {
    if !keys.just_pressed(KeyCode::KeyD) {
        return;
    }
    debug.0 = !debug.0;
    let on = debug.0;
    bevy::log::info!("debug outlines: {on}");

    for mut panel in &mut a4_panels {
        panel.set_tree(build_a4_page(on));
    }
    for mut panel in &mut card_panels {
        panel.set_tree(build_card(on));
    }
    for mut panel in &mut index_panels {
        panel.set_tree(build_index_page(on));
    }
}

fn toggle_rulers(
    keys: Res<ButtonInput<KeyCode>>,
    mut rulers_visible: ResMut<RulersVisible>,
    mut rulers: Query<&mut Visibility, With<PanelRuler>>,
) {
    if !keys.just_pressed(KeyCode::KeyR) {
        return;
    }
    rulers_visible.0 = !rulers_visible.0;
    let vis = if rulers_visible.0 {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    };
    for mut visibility in &mut rulers {
        *visibility = vis;
    }
}

// ── Panel rulers ────────────────────────────────────────────────────

/// Builds vertical tick rows for a ruler. Each slot has a bottom tick, and
/// the topmost slot also gets a tick at the upper edge.
fn build_vertical_ticks(
    b: &mut LayoutBuilder,
    count: i32,
    slot_height: f32,
    align: AlignX,
    ruler_color: Color,
    tick_size_fn: fn(i32) -> (f32, f32),
) {
    for idx in (0..count).rev() {
        let (tick_width, tick_line) = tick_size_fn(idx);
        let is_top = idx == count - 1;
        b.with(
            El::new()
                .width(Sizing::GROW)
                .height(Sizing::fixed(slot_height))
                .direction(Direction::TopToBottom)
                .child_align_x(align),
            |b| {
                if is_top {
                    let (tw, tl) = tick_size_fn(count);
                    b.with(
                        El::new()
                            .width(Sizing::fixed(tw))
                            .height(Sizing::fixed(tl))
                            .background(ruler_color),
                        |_| {},
                    );
                }
                b.with(El::new().height(Sizing::GROW), |_| {});
                b.with(
                    El::new()
                        .width(Sizing::fixed(tick_width))
                        .height(Sizing::fixed(tick_line))
                        .background(ruler_color),
                    |_| {},
                );
            },
        );
    }
}

/// Builds horizontal tick columns for a ruler. Each slot has a left tick, and
/// the rightmost slot also gets a tick at the right edge.
fn build_horizontal_ticks(
    b: &mut LayoutBuilder,
    count: i32,
    slot_width: f32,
    ruler_color: Color,
    tick_size_fn: fn(i32) -> (f32, f32),
) {
    for idx in 0..count {
        let (tick_height, tick_line) = tick_size_fn(idx);
        let is_last = idx == count - 1;
        b.with(
            El::new()
                .width(Sizing::fixed(slot_width))
                .height(Sizing::GROW)
                .direction(Direction::LeftToRight)
                .child_align_y(AlignY::Top),
            |b| {
                b.with(
                    El::new()
                        .width(Sizing::fixed(tick_line))
                        .height(Sizing::fixed(tick_height))
                        .background(ruler_color),
                    |_| {},
                );
                if is_last {
                    b.with(El::new().width(Sizing::GROW), |_| {});
                    let (tw, tl) = tick_size_fn(count);
                    b.with(
                        El::new()
                            .width(Sizing::fixed(tl))
                            .height(Sizing::fixed(tw))
                            .background(ruler_color),
                        |_| {},
                    );
                }
            },
        );
    }
}

fn build_metric_panel_ruler(height_mm: i32, ruler_color: Color) -> LayoutTree {
    let mut builder = LayoutBuilder::new(PANEL_RULER_WIDTH, Mm(height_mm.to_f32()));
    let label_style = LayoutTextStyle::new(Pt(8.0)).with_color(ruler_color);
    let last_cm = height_mm / 10;
    // Top spacer: distance from top of ruler to center of topmost cm block.
    let top_spacer = last_cm.to_f32().mul_add(-10.0, height_mm.to_f32()) - 5.0;

    builder.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::GROW)
            .direction(Direction::LeftToRight),
        |b| {
            // ── Left column: labels ─────────────────────────────
            b.with(
                El::new()
                    .width(Sizing::GROW)
                    .height(Sizing::GROW)
                    .direction(Direction::TopToBottom)
                    .child_align_x(AlignX::Right)
                    .padding(Padding::new(
                        Mm(0.0),
                        PANEL_RULER_MM_LABEL_GAP,
                        Mm(0.0),
                        Mm(0.0),
                    )),
                |b| {
                    // Top spacer.
                    if top_spacer > 0.0 {
                        b.with(
                            El::new()
                                .height(Sizing::fixed(Mm(top_spacer)))
                                .width(Sizing::GROW),
                            |_| {},
                        );
                    }
                    // One 10mm block per cm, with text centered.
                    for cm in (1..=last_cm).rev() {
                        b.with(
                            El::new()
                                .height(Sizing::fixed(Mm(10.0)))
                                .width(Sizing::GROW)
                                .child_align_x(AlignX::Right)
                                .child_align_y(AlignY::Center),
                            |b| {
                                b.text(format!("{cm}"), label_style.clone());
                            },
                        );
                    }
                    // Bottom spacer (5mm below cm 1).
                    b.with(
                        El::new().height(Sizing::fixed(Mm(5.0))).width(Sizing::GROW),
                        |_| {},
                    );
                },
            );

            // ── Right column: ticks + spine ─────────────────────
            b.with(
                El::new()
                    .width(Sizing::fixed(Mm(
                        PANEL_RULER_CM_TICK.0 + PANEL_RULER_SPINE.0
                    )))
                    .height(Sizing::fixed(Mm(height_mm.to_f32())))
                    .direction(Direction::LeftToRight)
                    .child_align_x(AlignX::Right),
                |b| {
                    b.with(
                        El::new()
                            .width(Sizing::GROW)
                            .height(Sizing::GROW)
                            .direction(Direction::TopToBottom)
                            .child_align_x(AlignX::Right),
                        |b| {
                            build_vertical_ticks(
                                b,
                                height_mm,
                                1.0,
                                AlignX::Right,
                                ruler_color,
                                mm_tick_size,
                            );
                        },
                    );
                    b.with(
                        El::new()
                            .width(Sizing::fixed(PANEL_RULER_SPINE))
                            .height(Sizing::GROW)
                            .background(ruler_color),
                        |_| {},
                    );
                },
            );
        },
    );

    builder.build()
}

const fn mm_tick_size(mm: i32) -> (f32, f32) {
    if mm % 10 == 0 {
        (PANEL_RULER_CM_TICK.0, PANEL_RULER_CM_LINE.0)
    } else if mm % 5 == 0 {
        (PANEL_RULER_MM5_TICK.0, PANEL_RULER_MM5_LINE.0)
    } else {
        (PANEL_RULER_MM1_TICK.0, PANEL_RULER_MM1_LINE.0)
    }
}

/// Imperial panel ruler — spine on the LEFT, ticks extending RIGHT, labels on RIGHT.
/// `panel_height` may be taller than the measurement range to fit edge labels.
fn build_imperial_panel_ruler(
    height_sixteenths: i32,
    panel_height: In,
    ruler_color: Color,
) -> LayoutTree {
    let height_in = height_sixteenths.to_f32() / 16.0;
    let last_label_inch = height_sixteenths / 16;
    let top_spacer = panel_height.0 - last_label_inch.to_f32() - 0.5;
    let mut builder = LayoutBuilder::new(PANEL_RULER_INCH_WIDTH, panel_height);
    let label_style = LayoutTextStyle::new(Pt(8.0)).with_color(ruler_color);
    let sixteenth_h = 1.0 / 16.0;

    builder.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::GROW)
            .direction(Direction::LeftToRight)
            .child_align_y(AlignY::Bottom),
        |b| {
            // ── Left column: spine + ticks ──────────────────────
            b.with(
                El::new()
                    .width(Sizing::fixed(In(
                        PANEL_RULER_INCH_TICK.0 + PANEL_RULER_INCH_SPINE.0
                    )))
                    .height(Sizing::fixed(In(height_in)))
                    .direction(Direction::LeftToRight),
                |b| {
                    // Spine.
                    b.with(
                        El::new()
                            .width(Sizing::fixed(PANEL_RULER_INCH_SPINE))
                            .height(Sizing::GROW)
                            .background(ruler_color),
                        |_| {},
                    );
                    b.with(
                        El::new()
                            .width(Sizing::GROW)
                            .height(Sizing::GROW)
                            .direction(Direction::TopToBottom)
                            .child_align_x(AlignX::Left),
                        |b| {
                            build_vertical_ticks(
                                b,
                                height_sixteenths,
                                sixteenth_h,
                                AlignX::Left,
                                ruler_color,
                                sixteenth_tick_size,
                            );
                        },
                    );
                },
            );

            // ── Right column: labels ────────────────────────────
            b.with(
                El::new()
                    .width(Sizing::GROW)
                    .height(Sizing::GROW)
                    .direction(Direction::TopToBottom)
                    .child_align_x(AlignX::Left)
                    .padding(Padding::new(
                        PANEL_RULER_IN_LABEL_GAP,
                        In(0.0),
                        In(0.0),
                        In(0.0),
                    )),
                |b| {
                    if top_spacer > 0.0 {
                        b.with(
                            El::new()
                                .height(Sizing::fixed(In(top_spacer)))
                                .width(Sizing::GROW),
                            |_| {},
                        );
                    }
                    for inch in (1..=last_label_inch).rev() {
                        b.with(
                            El::new()
                                .height(Sizing::fixed(In(1.0)))
                                .width(Sizing::GROW)
                                .child_align_x(AlignX::Left)
                                .child_align_y(AlignY::Center),
                            |b| {
                                b.text(format!("{inch}"), label_style.clone());
                            },
                        );
                    }
                    b.with(
                        El::new().height(Sizing::fixed(In(0.5))).width(Sizing::GROW),
                        |_| {},
                    );
                },
            );
        },
    );

    builder.build()
}

const fn sixteenth_tick_size(sixteenth: i32) -> (f32, f32) {
    if sixteenth % 16 == 0 {
        (PANEL_RULER_INCH_TICK.0, PANEL_RULER_INCH_LINE.0)
    } else if sixteenth % 8 == 0 {
        (PANEL_RULER_HALF_TICK.0, PANEL_RULER_HALF_LINE.0)
    } else if sixteenth % 4 == 0 {
        (PANEL_RULER_QTR_TICK.0, PANEL_RULER_QTR_LINE.0)
    } else if sixteenth % 2 == 0 {
        (PANEL_RULER_8TH_TICK.0, PANEL_RULER_8TH_LINE.0)
    } else {
        (PANEL_RULER_16TH_TICK.0, PANEL_RULER_16TH_LINE.0)
    }
}

/// Horizontal metric ruler — spine on TOP, ticks extending DOWN, labels below.
fn build_metric_horizontal_ruler(width_mm: i32, ruler_color: Color) -> LayoutTree {
    let mut builder = LayoutBuilder::new(Mm(width_mm.to_f32()), PANEL_RULER_WIDTH);
    let label_style = LayoutTextStyle::new(Pt(8.0)).with_color(ruler_color);
    // Labels go at cm 1..last_label_cm, each centered in a 10mm block.
    // Skip the cm at the exact edge (it's just a tick, no room for a label).
    let last_label_cm = (width_mm - 5) / 10;
    let right_spacer = last_label_cm
        .to_f32()
        .mul_add(-10.0, width_mm.to_f32() - 5.0);

    builder.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::GROW)
            .direction(Direction::TopToBottom),
        |b| {
            // ── Top row: spine + ticks ──────────────────────────
            b.with(
                El::new()
                    .width(Sizing::fixed(Mm(width_mm.to_f32())))
                    .height(Sizing::fixed(Mm(
                        PANEL_RULER_CM_TICK.0 + PANEL_RULER_SPINE.0
                    )))
                    .direction(Direction::TopToBottom),
                |b| {
                    // Spine.
                    b.with(
                        El::new()
                            .width(Sizing::GROW)
                            .height(Sizing::fixed(PANEL_RULER_SPINE))
                            .background(ruler_color),
                        |_| {},
                    );
                    b.with(
                        El::new()
                            .width(Sizing::GROW)
                            .height(Sizing::GROW)
                            .direction(Direction::LeftToRight)
                            .child_align_y(AlignY::Top),
                        |b| {
                            build_horizontal_ticks(b, width_mm, 1.0, ruler_color, mm_tick_size);
                        },
                    );
                },
            );

            // ── Bottom row: labels ──────────────────────────────
            b.with(
                El::new()
                    .width(Sizing::GROW)
                    .height(Sizing::GROW)
                    .direction(Direction::LeftToRight)
                    .child_align_y(AlignY::Top)
                    .padding(Padding::new(
                        Mm(0.0),
                        Mm(0.0),
                        PANEL_RULER_MM_LABEL_GAP,
                        Mm(0.0),
                    )),
                |b| {
                    // Left spacer (5mm to center of first cm block).
                    b.with(
                        El::new().width(Sizing::fixed(Mm(5.0))).height(Sizing::GROW),
                        |_| {},
                    );
                    for cm in 1..=last_label_cm {
                        b.with(
                            El::new()
                                .width(Sizing::fixed(Mm(10.0)))
                                .height(Sizing::GROW),
                            |b| {
                                b.with(
                                    El::new()
                                        .width(Sizing::GROW)
                                        .height(Sizing::GROW)
                                        .direction(Direction::TopToBottom)
                                        .child_align_x(AlignX::Center),
                                    |b| {
                                        b.text(format!("{cm}"), label_style.clone());
                                    },
                                );
                            },
                        );
                    }
                    if right_spacer > 0.0 {
                        b.with(
                            El::new()
                                .width(Sizing::fixed(Mm(right_spacer)))
                                .height(Sizing::GROW),
                            |_| {},
                        );
                    }
                },
            );
        },
    );

    builder.build()
}

/// Horizontal imperial ruler — spine on TOP, ticks extending DOWN, labels below.
fn build_imperial_horizontal_ruler(width_sixteenths: i32, ruler_color: Color) -> LayoutTree {
    let width_in = width_sixteenths.to_f32() / 16.0;
    let mut builder = LayoutBuilder::new(In(width_in), PANEL_RULER_INCH_WIDTH);
    let label_style = LayoutTextStyle::new(Pt(8.0)).with_color(ruler_color);
    let last_label_inch = width_sixteenths / 16;
    let right_spacer = width_in - 0.5 - last_label_inch.to_f32();
    let sixteenth_w = 1.0 / 16.0;

    builder.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::GROW)
            .direction(Direction::TopToBottom),
        |b| {
            // ── Top row: spine + ticks ──────────────────────────
            b.with(
                El::new()
                    .width(Sizing::fixed(In(width_in)))
                    .height(Sizing::fixed(In(
                        PANEL_RULER_INCH_TICK.0 + PANEL_RULER_INCH_SPINE.0
                    )))
                    .direction(Direction::TopToBottom),
                |b| {
                    // Spine.
                    b.with(
                        El::new()
                            .width(Sizing::GROW)
                            .height(Sizing::fixed(PANEL_RULER_INCH_SPINE))
                            .background(ruler_color),
                        |_| {},
                    );
                    // Tick row.
                    b.with(
                        El::new()
                            .width(Sizing::GROW)
                            .height(Sizing::GROW)
                            .direction(Direction::LeftToRight)
                            .child_align_y(AlignY::Top),
                        |b| {
                            build_horizontal_ticks(
                                b,
                                width_sixteenths,
                                sixteenth_w,
                                ruler_color,
                                sixteenth_tick_size,
                            );
                        },
                    );
                },
            );

            // ── Bottom row: labels ──────────────────────────────
            b.with(
                El::new()
                    .width(Sizing::GROW)
                    .height(Sizing::GROW)
                    .direction(Direction::LeftToRight)
                    .child_align_y(AlignY::Top)
                    .padding(Padding::new(
                        In(0.0),
                        In(0.0),
                        PANEL_RULER_IN_LABEL_GAP,
                        In(0.0),
                    )),
                |b| {
                    b.with(
                        El::new().width(Sizing::fixed(In(0.5))).height(Sizing::GROW),
                        |_| {},
                    );
                    for inch in 1..=last_label_inch {
                        b.with(
                            El::new().width(Sizing::fixed(In(1.0))).height(Sizing::GROW),
                            |b| {
                                b.with(
                                    El::new()
                                        .width(Sizing::GROW)
                                        .height(Sizing::GROW)
                                        .direction(Direction::TopToBottom)
                                        .child_align_x(AlignX::Center),
                                    |b| {
                                        b.text(format!("{inch}"), label_style.clone());
                                    },
                                );
                            },
                        );
                    }
                    if right_spacer > 0.0 {
                        b.with(
                            El::new()
                                .width(Sizing::fixed(In(right_spacer)))
                                .height(Sizing::GROW),
                            |_| {},
                        );
                    }
                },
            );
        },
    );

    builder.build()
}

// ── Panel content ────────────────────────────────────────────────────

const DEBUG_BORDER_COLOR: Color = Color::srgba(1.0, 0.2, 0.2, 0.8);

fn debug_border(debug: bool, width: impl Into<bevy_diegetic::Dimension>) -> Border {
    let color = if debug {
        DEBUG_BORDER_COLOR
    } else {
        Color::NONE
    };
    Border::all(width, color)
}

fn debug_text(
    b: &mut bevy_diegetic::LayoutBuilder,
    text: &str,
    style: LayoutTextStyle,
    db: Border,
) {
    b.with(El::new().width(Sizing::GROW).border(db), |b| {
        b.text(text, style);
    });
}

/// Builds an A4 page layout tree (used by toggle_debug_outlines for runtime rebuild).
fn build_a4_page(debug: bool) -> bevy_diegetic::LayoutTree {
    let mut builder = LayoutBuilder::new(A4_W, A4_H);
    build_a4_content(&mut builder, debug);
    builder.build()
}

/// Populates an A4 page layout into the given builder.
fn build_a4_content(builder: &mut LayoutBuilder, debug: bool) {
    let db = debug_border(debug, DEBUG_OUTLINE);

    let heading = LayoutTextStyle::new(Pt(18.0)).with_color(A4_TEXT_COLOR);
    let body = LayoutTextStyle::new(Pt(12.0)).with_color(A4_TEXT_COLOR);

    builder.with(
        El::new()
            .size(A4_W, A4_H)
            .padding(Padding::all(Mm(15.0)))
            .direction(Direction::TopToBottom)
            .child_gap(Mm(4.0))
            .background(Color::WHITE),
        |b| {
            build_font_samples_row(b, db);

            // ── Divider ─────────────────────────────────────────
            b.with(
                El::new()
                    .width(Sizing::GROW)
                    .height(Sizing::fixed(Mm(0.3)))
                    .background(A4_DIM_COLOR),
                |_| {},
            );

            build_two_column_article(b, &heading, &body, db);

            // ── Divider ─────────────────────────────────────────
            b.with(
                El::new()
                    .width(Sizing::GROW)
                    .height(Sizing::fixed(Mm(0.3)))
                    .background(A4_DIM_COLOR),
                |_| {},
            );

            // ── Footer ──────────────────────────────────────────
            b.with(
                El::new()
                    .width(Sizing::GROW)
                    .child_align_x(AlignX::Center)
                    .border(db),
                |b| {
                    b.with(El::new().border(db), |b| {
                        b.text(
                            "PaperSize::A4  |  layout: Millimeters  |  font: Points",
                            LayoutTextStyle::new(Pt(14.0)).with_color(A4_DIM_COLOR),
                        );
                    });
                },
            );
        },
    );
}

fn build_font_samples_row(b: &mut LayoutBuilder, db: Border) {
    b.with(
        El::new()
            .width(Sizing::GROW)
            .direction(Direction::LeftToRight)
            .child_gap(Mm(6.0))
            .child_align_y(AlignY::Bottom)
            .border(db),
        |b| {
            for (label, size) in [
                ("72pt", Pt(72.0)),
                ("36pt", Pt(36.0)),
                ("24pt", Pt(24.0)),
                ("18pt", Pt(18.0)),
                ("12pt", Pt(12.0)),
                ("9pt", Pt(9.0)),
            ] {
                debug_text(
                    b,
                    label,
                    LayoutTextStyle::new(size).with_color(A4_TEXT_COLOR),
                    db,
                );
            }
        },
    );
}

fn build_two_column_article(
    b: &mut LayoutBuilder,
    heading: &LayoutTextStyle,
    body: &LayoutTextStyle,
    db: Border,
) {
    b.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::GROW)
            .direction(Direction::LeftToRight)
            .child_gap(Mm(8.0))
            .border(db),
        |b| {
            b.with(
                El::new()
                    .width(Sizing::GROW)
                    .direction(Direction::TopToBottom)
                    .child_gap(Mm(4.0))
                    .border(db),
                |b| {
                    debug_text(b, "Real Units, Real Sizes", heading.clone(), db);
                    debug_text(
                        b,
                        "This page is 210 by 297 millimeters \u{2014} an A4 sheet \
                         at true physical scale, in a game world where one unit equals \
                         one meter. The business card beside it is 3.5 by 2 inches. \
                         Neither requires manual conversion. Each panel declares its \
                         own layout unit, and the system handles the rest.",
                        body.clone(),
                        db,
                    );
                    debug_text(
                        b,
                        "Font sizes are in typographic points. Padding and spacing \
                         follow the panel\u{2019}s layout unit. Mix freely: Mm(6.0) \
                         for a margin, Pt(18.0) for a heading, In(0.5) for a \
                         gutter. The convenience types carry their unit through the \
                         layout engine \u{2014} no math required.",
                        body.clone(),
                        db,
                    );
                    debug_text(
                        b,
                        "By default, a panel\u{2019}s dimensions map directly to \
                         meters. This 210mm-wide A4 page occupies 0.21 meters in \
                         the scene \u{2014} physically correct at 1:1 world scale.",
                        body.clone(),
                        db,
                    );
                },
            );

            b.with(
                El::new()
                    .width(Sizing::fixed(Mm(0.3)))
                    .height(Sizing::GROW)
                    .background(A4_DIM_COLOR),
                |_| {},
            );

            b.with(
                El::new()
                    .width(Sizing::GROW)
                    .direction(Direction::TopToBottom)
                    .child_gap(Mm(4.0))
                    .border(db),
                |b| {
                    debug_text(b, "Any Scale You Need", heading.clone(), db);
                    debug_text(
                        b,
                        "Set world_height on any panel and it scales uniformly to \
                         fit. A museum placard at 0.3 meters. A highway billboard \
                         at 12 meters. A planetary-scale announcement at 50,000 \
                         meters \u{2014} zoom out far enough and there it is. Text \
                         stays sharp at any distance by rendering glyphs using SDFs \
                         (signed distance fields), not rasterized bitmaps. \u{2014} \
                         The GPU evaluates the distance function per fragment - \
                         edges remain crisp regardless of scale.",
                        body.clone(),
                        db,
                    );
                    debug_text(
                        b,
                        "The global UnitConfig resource sets defaults for every \
                         panel: layout in meters, fonts in points. Override \
                         per-panel with layout_unit and font_unit, or per-element \
                         with types like Mm(10.0) and Pt(24.0) inline. The system \
                         converts at layout time so the engine always works in a \
                         consistent coordinate space internally.",
                        body.clone(),
                        db,
                    );
                    debug_text(
                        b,
                        "Custom(0.01) defines a centimeter. Custom(9.461e15) \
                         defines a light-year. The unit system does not care about \
                         scale \u{2014} it only needs to know the ratio to meters.",
                        body.clone(),
                        db,
                    );
                },
            );
        },
    );
}

/// Builds a business card layout tree (used by toggle_debug_outlines for runtime rebuild).
fn build_card(debug: bool) -> bevy_diegetic::LayoutTree {
    let mut builder = LayoutBuilder::new(CARD_W, CARD_H);
    build_card_content(&mut builder, debug);
    builder.build()
}

/// Populates a business card layout into the given builder.
fn build_card_content(builder: &mut LayoutBuilder, debug: bool) {
    let db = debug_border(debug, DEBUG_OUTLINE);

    builder.with(
        El::new()
            .size(CARD_W, CARD_H)
            .padding(Padding::all(In(0.15)))
            .direction(Direction::TopToBottom)
            .child_gap(In(0.04))
            .background(Color::srgb(0.392, 0.584, 0.929)),
        |b| {
            debug_text(
                b,
                "MARY JANE LOGICIELEUR",
                LayoutTextStyle::new(CARD_NAME_SIZE).with_color(CARD_TEXT_COLOR),
                db,
            );
            debug_text(
                b,
                "Software Engineer",
                LayoutTextStyle::new(CARD_TITLE_SIZE).with_color(CARD_DIM_COLOR),
                db,
            );
            debug_text(
                b,
                "mary-jane@example.com",
                LayoutTextStyle::new(CARD_DETAIL_SIZE).with_color(CARD_TEXT_COLOR),
                db,
            );
            debug_text(
                b,
                "+1 (555) 012-3456",
                LayoutTextStyle::new(CARD_DETAIL_SIZE).with_color(CARD_TEXT_COLOR),
                db,
            );

            // Spacer
            b.with(El::new().width(Sizing::GROW).height(Sizing::GROW), |_| {});

            // Footer
            debug_text(
                b,
                "PaperSize::BusinessCard  |  layout: Inches  |  font: Points",
                LayoutTextStyle::new(CARD_FOOTER_SIZE).with_color(CARD_DIM_COLOR),
                db,
            );
        },
    );
}

/// Builds a 5×7 index page layout tree (used by toggle_debug_outlines for runtime rebuild).
fn build_index_page(debug: bool) -> bevy_diegetic::LayoutTree {
    let mut builder = LayoutBuilder::new(INDEX_W, INDEX_H);
    build_index_content(&mut builder, debug);
    builder.build()
}

/// Populates the 5×7 card with a units API reference table.
fn build_index_content(builder: &mut LayoutBuilder, debug: bool) {
    let db = debug_border(debug, DEBUG_OUTLINE);
    let heading = LayoutTextStyle::new(INDEX_HEADING_SIZE).with_color(INDEX_HEADING_COLOR);
    let subheading = LayoutTextStyle::new(INDEX_SUBHEADING_SIZE).with_color(INDEX_HEADING_COLOR);
    let label = LayoutTextStyle::new(INDEX_LABEL_SIZE).with_color(INDEX_LABEL_COLOR);
    let code = LayoutTextStyle::new(INDEX_CODE_SIZE).with_color(INDEX_CODE_COLOR);
    let footer = LayoutTextStyle::new(INDEX_FOOTER_SIZE).with_color(INDEX_LABEL_COLOR);

    builder.with(
        El::new()
            .size(INDEX_W, INDEX_H)
            .padding(Padding::all(In(0.2)))
            .direction(Direction::TopToBottom)
            .child_gap(In(0.08))
            .background(INDEX_BG),
        |b| {
            debug_text(b, "Units API", heading.clone(), db);

            // ── Panel sizing ────────────────────────────────────
            debug_text(b, "Panel Sizing", subheading.clone(), db);
            index_row(b, "f32", ".size(0.127, 0.178)", &label, &code, db);
            index_row(b, "In", ".size(In(5.0), In(7.0))", &label, &code, db);
            index_row(b, "Mm", ".size(Mm(127.0), Mm(177.8))", &label, &code, db);
            index_row(b, "Pt", ".size(Pt(360.0), Pt(504.0))", &label, &code, db);
            index_row(
                b,
                "PaperSize",
                ".paper(PaperSize::Photo5x7)",
                &label,
                &code,
                db,
            );
            index_row(
                b,
                "Pixels",
                ".size(Px(800.0), Px(600.0))",
                &label,
                &code,
                db,
            );

            // ── Divider ─────────────────────────────────────────
            b.with(
                El::new()
                    .width(Sizing::GROW)
                    .height(Sizing::fixed(In(0.01)))
                    .background(INDEX_HEADING_COLOR),
                |_| {},
            );

            // ── Element sizing ──────────────────────────────────
            debug_text(b, "Element Sizing", subheading.clone(), db);
            index_row(
                b,
                "Fixed",
                "El::new().size(Mm(50.0), Mm(30.0))",
                &label,
                &code,
                db,
            );
            index_row(
                b,
                "Grow",
                "El::new().width(Sizing::GROW)",
                &label,
                &code,
                db,
            );
            index_row(b, "Fit", "El::new().width(Sizing::FIT)", &label, &code, db);
            index_row(
                b,
                "Percent",
                "El::new().width(Sizing::percent(0.5))",
                &label,
                &code,
                db,
            );

            // ── Divider ─────────────────────────────────────────
            b.with(
                El::new()
                    .width(Sizing::GROW)
                    .height(Sizing::fixed(In(0.01)))
                    .background(INDEX_HEADING_COLOR),
                |_| {},
            );

            // ── World scaling ───────────────────────────────────
            debug_text(b, "World Scaling", subheading.clone(), db);
            index_row(b, "Height", ".world_height(0.5)", &label, &code, db);
            index_row(b, "Width", ".world_width(1.0)", &label, &code, db);

            // Spacer
            b.with(El::new().width(Sizing::GROW).height(Sizing::GROW), |_| {});

            // Footer
            b.with(
                El::new()
                    .width(Sizing::GROW)
                    .child_align_x(AlignX::Center)
                    .border(db),
                |b| {
                    b.with(El::new().border(db), |b| {
                        b.text(
                            "PaperSize::Photo5x7  |  layout: Inches  |  font: Points",
                            footer,
                        );
                    });
                },
            );
        },
    );
}

/// A single row in the index card table: label on the left, code on the right.
fn index_row(
    b: &mut LayoutBuilder,
    label_text: &str,
    code_text: &str,
    label: &LayoutTextStyle,
    code: &LayoutTextStyle,
    db: Border,
) {
    b.with(
        El::new()
            .width(Sizing::GROW)
            .direction(Direction::LeftToRight)
            .child_gap(In(0.12))
            .border(db),
        |b| {
            b.with(
                El::new()
                    .width(Sizing::fixed(In(1.0)))
                    .direction(Direction::TopToBottom)
                    .border(db),
                |b| {
                    b.text(label_text, label.clone());
                },
            );
            b.with(
                El::new()
                    .width(Sizing::GROW)
                    .direction(Direction::TopToBottom)
                    .border(db),
                |b| {
                    b.text(code_text, code.clone());
                },
            );
        },
    );
}

fn build_controls_tree(debug: bool, rulers: bool, perspective: bool) -> LayoutTree {
    let mut builder = LayoutBuilder::with_root(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::fixed(HUD_HEIGHT)),
    );
    build_controls_content(&mut builder, debug, rulers, perspective);
    builder.build()
}

fn build_controls_content(b: &mut LayoutBuilder, debug: bool, rulers: bool, perspective: bool) {
    let title = LayoutTextStyle::new(HUD_TITLE_SIZE).with_color(HUD_TITLE_COLOR);

    b.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::GROW)
            .padding(Padding::all(Px(2.0)))
            .background(HUD_FRAME_BACKGROUND)
            .border(Border::all(Px(2.0), HUD_BORDER_ACCENT)),
        |b| {
            b.with(
                El::new()
                    .width(Sizing::GROW)
                    .height(Sizing::GROW)
                    .direction(Direction::LeftToRight)
                    .padding(Padding::new(Px(8.0), HUD_PADDING, Px(8.0), HUD_PADDING))
                    .child_gap(HUD_GAP)
                    .child_align_y(AlignY::Center)
                    .clip()
                    .background(HUD_BACKGROUND)
                    .border(Border::all(Px(1.0), HUD_BORDER_DIM)),
                |b| {
                    b.text("CONTROLS", title);
                    hud_separator(b);

                    b.text(
                        "H Home",
                        LayoutTextStyle::new(HUD_HINT_SIZE).with_color(HUD_INACTIVE_COLOR),
                    );
                    hud_separator(b);

                    let rulers_label = if rulers {
                        "R Rulers On"
                    } else {
                        "R Rulers Off"
                    };
                    let rulers_color = if rulers {
                        HUD_ACTIVE_COLOR
                    } else {
                        HUD_INACTIVE_COLOR
                    };
                    b.text(
                        rulers_label,
                        LayoutTextStyle::new(HUD_HINT_SIZE).with_color(rulers_color),
                    );
                    hud_separator(b);

                    let persp_color = if perspective {
                        HUD_ACTIVE_COLOR
                    } else {
                        HUD_INACTIVE_COLOR
                    };
                    b.text(
                        "P Perspective",
                        LayoutTextStyle::new(HUD_HINT_SIZE).with_color(persp_color),
                    );
                    hud_separator(b);
                    let ortho_color = if perspective {
                        HUD_INACTIVE_COLOR
                    } else {
                        HUD_ACTIVE_COLOR
                    };
                    b.text(
                        "O Orthographic",
                        LayoutTextStyle::new(HUD_HINT_SIZE).with_color(ortho_color),
                    );
                    hud_separator(b);

                    let debug_label = if debug {
                        "D Outlines On"
                    } else {
                        "D Outlines Off"
                    };
                    let debug_color = if debug {
                        HUD_ACTIVE_COLOR
                    } else {
                        HUD_INACTIVE_COLOR
                    };
                    b.text(
                        debug_label,
                        LayoutTextStyle::new(HUD_HINT_SIZE).with_color(debug_color),
                    );
                },
            );
        },
    );
}

fn build_camera_help(b: &mut LayoutBuilder) {
    let title = LayoutTextStyle::new(CAM_HELP_TITLE_SIZE).with_color(HUD_TITLE_COLOR);
    let header = LayoutTextStyle::new(CAM_HELP_HEADER_SIZE).with_color(HUD_ACTIVE_COLOR);
    let label = LayoutTextStyle::new(CAM_HELP_LABEL_SIZE).with_color(HUD_INACTIVE_COLOR);

    b.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::GROW)
            .padding(Padding::all(CAM_HELP_FRAME_PAD))
            .corner_radius(CornerRadius::new(
                CAM_HELP_RADIUS,
                Px(0.0),
                CAM_HELP_RADIUS,
                Px(0.0),
            ))
            .background(HUD_FRAME_BACKGROUND)
            .border(Border::all(CAM_HELP_BORDER, HUD_BORDER_ACCENT)),
        |b| {
            b.with(
                El::new()
                    .width(Sizing::GROW)
                    .height(Sizing::GROW)
                    .direction(Direction::TopToBottom)
                    .padding(Padding::all(Px(10.0)))
                    .child_gap(Px(6.0))
                    .corner_radius(CornerRadius::new(
                        CAM_HELP_INNER_RADIUS,
                        Px(0.0),
                        CAM_HELP_INNER_RADIUS,
                        Px(0.0),
                    ))
                    .background(HUD_BACKGROUND)
                    .border(Border::all(Px(1.0), HUD_BORDER_DIM)),
                |b| {
                    b.text("CAMERA", title);

                    // Two columns: Mouse | Trackpad
                    b.with(
                        El::new()
                            .width(Sizing::GROW)
                            .height(Sizing::GROW)
                            .direction(Direction::LeftToRight)
                            .child_gap(Px(12.0)),
                        |b| {
                            // Mouse column
                            b.with(
                                El::new()
                                    .width(Sizing::GROW)
                                    .direction(Direction::TopToBottom)
                                    .child_gap(Px(4.0)),
                                |b| {
                                    b.text("Mouse", header.clone());
                                    b.text("MMB drag \u{2192} Orbit", label.clone());
                                    b.text("Shift+MMB \u{2192} Pan", label.clone());
                                    b.text("Scroll \u{2192} Zoom", label.clone());
                                },
                            );

                            // Divider
                            b.with(
                                El::new()
                                    .width(Sizing::fixed(Px(1.0)))
                                    .height(Sizing::GROW)
                                    .background(HUD_DIVIDER_COLOR),
                                |_| {},
                            );

                            // Trackpad column
                            b.with(
                                El::new()
                                    .width(Sizing::GROW)
                                    .direction(Direction::TopToBottom)
                                    .child_gap(Px(4.0)),
                                |b| {
                                    b.text("Trackpad", header.clone());
                                    b.text("Scroll \u{2192} Orbit", label.clone());
                                    b.text("Shift+Scroll \u{2192} Pan", label.clone());
                                    b.text("Ctrl+Scroll \u{2192} Zoom", label.clone());
                                    b.text("Pinch \u{2192} Zoom", label.clone());
                                },
                            );
                        },
                    );
                },
            );
        },
    );
}

fn hud_separator(b: &mut LayoutBuilder) {
    b.with(
        El::new()
            .width(Sizing::fixed(Px(1.0)))
            .height(Sizing::GROW)
            .background(HUD_DIVIDER_COLOR),
        |_| {},
    );
}

fn update_controls_hud(
    mut huds: Query<&mut DiegeticPanel, With<ControlsPanel>>,
    debug: Res<DebugOutlines>,
    rulers: Res<RulersVisible>,
    cameras: Query<&Projection>,
    mut previous_state: Local<(bool, bool, bool)>,
) {
    let perspective = cameras
        .iter()
        .any(|p| matches!(p, Projection::Perspective(_)));

    let state = (debug.0, rulers.0, perspective);
    if *previous_state == state {
        return;
    }
    *previous_state = state;

    for mut panel in &mut huds {
        panel.set_tree(build_controls_tree(debug.0, rulers.0, perspective));
    }
}

// ── Home camera ─────────────────────────────────────────────────────

fn home_camera(
    keyboard: Res<ButtonInput<KeyCode>>,
    cameras: Query<Entity, With<OrbitCam>>,
    mut commands: Commands,
) {
    if !keyboard.just_pressed(KeyCode::KeyH) {
        return;
    }
    for camera in &cameras {
        commands.trigger(PlayAnimation::new(
            camera,
            [CameraMove::ToOrbit {
                focus:    Vec3::new(0.0, f32::from(HOME_FOCUS_Y), 0.0),
                yaw:      HOME_YAW,
                pitch:    HOME_PITCH,
                radius:   f32::from(HOME_RADIUS),
                duration: Duration::from_millis(ZOOM_DURATION_MS),
                easing:   bevy::math::curve::easing::EaseFunction::CubicOut,
            }],
        ));
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
