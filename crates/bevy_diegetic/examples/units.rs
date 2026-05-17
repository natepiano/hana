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

use bevy::prelude::*;
use bevy_diegetic::AlignX;
use bevy_diegetic::AlignY;
use bevy_diegetic::Anchor;
use bevy_diegetic::Border;
use bevy_diegetic::DiegeticPanel;
use bevy_diegetic::DiegeticPanelCommands;
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
use bevy_diegetic::Sizing;
use bevy_diegetic::SurfaceShadow;
use bevy_diegetic::WorldText;
use bevy_diegetic::WorldTextStyle;
use bevy_kana::ToF32;
use bevy_kana::ToI32;
use bevy_lagrange::AnimationBegin;
use bevy_lagrange::AnimationEnd;
use bevy_lagrange::AnimationSource;
use bevy_lagrange::OrbitCam;
use bevy_lagrange::OrbitCamPreset;
use bevy_lagrange::ZoomToFit;
use fairy_dust::ControlActivation;
use fairy_dust::DEFAULT_PANEL_BACKGROUND;
use fairy_dust::TitleBar;

// ── A4 dimensions ────────────────────────────────────────────────────
const A4: PaperSize = PaperSize::A4;
const A4_WIDTH: Mm = Mm(A4.width_mm());
const A4_HEIGHT: Mm = Mm(A4.height_mm());

// ── US business card dimensions ──────────────────────────────────────
const CARD_WIDTH: In = In(3.5);
const CARD_HEIGHT: In = In(2.0);
const CARD_NAME_SIZE: Pt = Pt(15.0);
const CARD_TITLE_SIZE: Pt = Pt(13.0);
const CARD_DETAIL_SIZE: Pt = Pt(11.0);
const CARD_FOOTER_SIZE: Pt = Pt(6.0);

// ── Index card ──────────────────────────────────────────────────────
const INDEX_WIDTH: In = In(5.0);
const INDEX_HEIGHT: In = In(7.0);
const INDEX_BG: Color = Color::srgba(0.08, 0.10, 0.14, 1.0);
const INDEX_HEADING_COLOR: Color = Color::srgb(0.4, 0.85, 0.75);
const INDEX_LABEL_COLOR: Color = Color::srgba(0.7, 0.75, 0.85, 0.9);
const INDEX_CODE_COLOR: Color = Color::srgb(0.95, 0.9, 0.75);
const INDEX_HEADING_SIZE: Pt = Pt(14.0);
const INDEX_SUBHEADING_SIZE: Pt = Pt(11.0);
const INDEX_LABEL_SIZE: Pt = Pt(10.0);
const INDEX_CODE_SIZE: Pt = Pt(10.0);
const INDEX_FOOTER_SIZE: Pt = Pt(8.0);

// ── Screen panel styling ─────────────────────────────────────────────
/// Inner-background alpha applied to both the title bar and the camera
/// control panel. Higher than `fairy_dust`'s default (0.50) so the example's
/// HUD reads as a more opaque surface against the 3D scene.
const PANEL_BACKGROUND_ALPHA: f32 = 0.90;

// ── Scene layout ─────────────────────────────────────────────────────
const GAP: Mm = Mm(15.0);
const LIFT: Mm = Mm(55.0);
const TITLE_GAP: Mm = Mm(8.0);
/// Square edge length of the ground plane in meters. Sized to comfortably
/// extend beyond the three-panel layout (~0.35m × 0.30m) without dominating
/// the scene.
const GROUND_PLANE_SIZE: f32 = 1.0;

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
const HOME_PITCH: f32 = 0.1;
const HOME_YAW: f32 = 0.0;
const HOME_DEPTH: f32 = 0.01;
const HOME_MARGIN: f32 = 0.25;
const ZOOM_DURATION_MS: u64 = 1000;
const ZOOM_MARGIN: f32 = 0.08;

/// Union AABB of the three world-space panels (A4 + business card + photo
/// card). Mirrors the placement math in `setup`. The home cube sits at the
/// AABB center with a scale equal to its dimensions; `with_camera_home`'s
/// `.margin(HOME_MARGIN)` adds the breathing room.
fn compute_home_transform() -> Transform {
    let a4_w = f32::from(A4_WIDTH);
    let a4_h = f32::from(A4_HEIGHT);
    let card_w = f32::from(CARD_WIDTH);
    let card_h = f32::from(CARD_HEIGHT);
    let index_w = f32::from(INDEX_WIDTH);
    let index_h = f32::from(INDEX_HEIGHT);
    let gap = f32::from(GAP);
    let lift = f32::from(LIFT);

    let total_width = a4_w + gap + card_w;
    let group_left = -total_width / 2.0;

    let a4_x = group_left + a4_w / 2.0;
    let a4_y = lift + a4_h / 2.0;

    let card_x = group_left + a4_w + gap + card_w / 2.0;
    let card_y = (a4_y + a4_h / 2.0) - card_h / 2.0;

    let card_left = group_left + a4_w + gap;
    let index_x = card_left + index_w / 2.0;
    let index_y = lift + index_h / 2.0;

    let aabb =
        |cx: f32, cy: f32, w: f32, h: f32| (cx - w / 2.0, cx + w / 2.0, cy - h / 2.0, cy + h / 2.0);
    let (a4l, a4r, a4b, a4t) = aabb(a4_x, a4_y, a4_w, a4_h);
    let (cl, cr, cb, ct) = aabb(card_x, card_y, card_w, card_h);
    let (il, ir, ib, it) = aabb(index_x, index_y, index_w, index_h);

    let min_x = a4l.min(cl).min(il);
    let max_x = a4r.max(cr).max(ir);
    let min_y = a4b.min(cb).min(ib);
    let max_y = a4t.max(ct).max(it);

    Transform {
        translation: Vec3::new(
            f32::midpoint(min_x, max_x),
            f32::midpoint(min_y, max_y),
            0.0,
        ),
        scale: Vec3::new(max_x - min_x, max_y - min_y, HOME_DEPTH),
        ..default()
    }
}

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
struct PanelRuler;

#[derive(Resource, Clone, Copy, Default, PartialEq, Eq)]
enum DebugOutlines {
    On,
    #[default]
    Off,
}

impl DebugOutlines {
    const fn is_on(self) -> bool { matches!(self, Self::On) }

    const fn toggle(&mut self) {
        *self = match *self {
            Self::On => Self::Off,
            Self::Off => Self::On,
        };
    }
}

#[derive(Resource, Clone, Copy, Default, PartialEq, Eq)]
enum Rulers {
    #[default]
    Visible,
    Hidden,
}

impl Rulers {
    const fn toggle(&mut self) {
        *self = match *self {
            Self::Visible => Self::Hidden,
            Self::Hidden => Self::Visible,
        };
    }
}

#[derive(Resource, Clone, Copy, Default, PartialEq, Eq)]
enum CameraProjection {
    #[default]
    Perspective,
    Orthographic,
}

fn build_panel_or_log(
    panel: Result<DiegeticPanel, bevy_diegetic::InvalidSize>,
    label: &str,
) -> Option<DiegeticPanel> {
    match panel {
        Ok(panel) => Some(panel),
        Err(error) => {
            error!("failed to build {label}: {error}");
            None
        },
    }
}

fn main() {
    fairy_dust::sprinkle_example()
        .with_brp_extras()
        .with_save_window_position()
        .with_studio_lighting()
        .with_ground_plane()
        .size(GROUND_PLANE_SIZE)
        .with_orbit_cam(
            |cam| {
                cam.zoom_lower_limit = 0.000_000_1;
            },
            OrbitCamPreset::BlenderLike,
        )
        .with_stable_transparency()
        .with_camera_home(compute_home_transform())
        .yaw(HOME_YAW)
        .pitch(HOME_PITCH)
        .duration(Duration::from_millis(ZOOM_DURATION_MS))
        .margin(HOME_MARGIN)
        .with_title_bar(
            TitleBar::new()
                .with_background_color(DEFAULT_PANEL_BACKGROUND.with_alpha(PANEL_BACKGROUND_ALPHA))
                .control("D Outlines")
                .control("R Rulers")
                .control("P Perspective")
                .control("O Orthographic")
                .control("Click to Zoom"),
        )
        .wire_chip_to_state::<DebugOutlines, _>("D Outlines", |state| match state {
            DebugOutlines::On => ControlActivation::Active,
            DebugOutlines::Off => ControlActivation::Inactive,
        })
        .wire_chip_to_state::<Rulers, _>("R Rulers", |state| match state {
            Rulers::Visible => ControlActivation::Active,
            Rulers::Hidden => ControlActivation::Inactive,
        })
        .wire_chip_to_state::<CameraProjection, _>("P Perspective", |state| match state {
            CameraProjection::Perspective => ControlActivation::Active,
            CameraProjection::Orthographic => ControlActivation::Inactive,
        })
        .wire_chip_to_state::<CameraProjection, _>("O Orthographic", |state| match state {
            CameraProjection::Orthographic => ControlActivation::Active,
            CameraProjection::Perspective => ControlActivation::Inactive,
        })
        .wire_chip_to_events_filtered::<AnimationBegin, AnimationEnd, _, _>(
            "Click to Zoom",
            |event| event.source == AnimationSource::ZoomToFit,
            |event| event.source == AnimationSource::ZoomToFit,
        )
        .with_camera_control_panel()
        .with_camera_control_panel_background_color(
            DEFAULT_PANEL_BACKGROUND.with_alpha(PANEL_BACKGROUND_ALPHA),
        )
        .init_resource::<DebugOutlines>()
        .init_resource::<Rulers>()
        .init_resource::<CameraProjection>()
        .add_systems(Startup, setup)
        .add_systems(Update, toggle_debug_outlines)
        .add_systems(Update, toggle_rulers)
        .add_systems(Update, toggle_projection)
        .run();
}

fn setup(mut commands: Commands) {
    let a4_width_meters = f32::from(A4_WIDTH);
    let a4_height_meters = f32::from(A4_HEIGHT);
    let card_width_m = f32::from(CARD_WIDTH);
    let card_height_m = f32::from(CARD_HEIGHT);
    let gap = f32::from(GAP);
    let lift = f32::from(LIFT);
    let title_gap = f32::from(TITLE_GAP);

    let total_width = a4_width_meters + gap + card_width_m;
    let group_left = -total_width / 2.0;

    let a4_page_x = group_left + a4_width_meters / 2.0;
    let a4_page_y = a4_height_meters / 2.0 + lift;

    let a4_page_top = a4_page_y + a4_height_meters / 2.0;
    let card_x = group_left + a4_width_meters + gap + card_width_m / 2.0;
    let card_y = a4_page_top - card_height_m / 2.0;

    let ruler_color = Color::WHITE;

    spawn_a4_with_titles(
        &mut commands,
        a4_page_x,
        a4_page_y,
        a4_page_top,
        card_x,
        title_gap,
    );

    spawn_rulers(
        &mut commands,
        ruler_color,
        a4_page_x,
        a4_page_y,
        a4_width_meters,
        a4_height_meters,
        card_x,
        card_y,
        card_width_m,
        card_height_m,
    );

    spawn_card_panel(&mut commands, card_x, card_y);

    let index_width_m = f32::from(INDEX_WIDTH);
    let index_height_m = f32::from(INDEX_HEIGHT);
    let card_left = card_x - card_width_m / 2.0;
    let index_x = card_left + index_width_m / 2.0;
    let a4_page_bottom = a4_page_y - a4_height_meters / 2.0;
    let index_y = a4_page_bottom + index_height_m / 2.0;

    spawn_photo_panel_with_title(&mut commands, index_x, index_y, index_height_m, title_gap);
    spawn_index_card_rulers(
        &mut commands,
        index_x,
        index_y,
        index_width_m,
        index_height_m,
        ruler_color,
    );
}

fn spawn_a4_with_titles(
    commands: &mut Commands,
    a4_page_x: f32,
    a4_page_y: f32,
    a4_page_top: f32,
    card_x: f32,
    title_gap: f32,
) {
    let Some(a4_panel) = build_panel_or_log(
        DiegeticPanel::world()
            .paper(PaperSize::A4)
            .anchor(Anchor::Center)
            .surface_shadow(SurfaceShadow::On)
            .layout(|b| build_a4_content(b, false))
            .build(),
        "A4 dimensions",
    ) else {
        return;
    };

    commands
        .spawn((
            A4Panel,
            a4_panel,
            Transform::from_xyz(a4_page_x, a4_page_y, 0.0),
        ))
        .observe(on_panel_clicked);

    let title_style = WorldTextStyle::new(Pt(18.0))
        .with_color(Color::WHITE)
        .with_anchor(Anchor::BottomCenter);
    commands.spawn((
        WorldText::new("A4 Paper — 210 × 297 mm"),
        title_style.clone(),
        Transform::from_xyz(a4_page_x, a4_page_top + title_gap, 0.0),
    ));
    commands.spawn((
        WorldText::new("US Business Card — 3½ × 2 in"),
        title_style,
        Transform::from_xyz(card_x, a4_page_top + title_gap, 0.0),
    ));
}

fn spawn_card_panel(commands: &mut Commands, card_x: f32, card_y: f32) {
    let Some(card_panel) = build_panel_or_log(
        DiegeticPanel::world()
            .paper(PaperSize::BusinessCard)
            .anchor(Anchor::Center)
            .surface_shadow(SurfaceShadow::On)
            .layout(|b| build_card_content(b, false))
            .build(),
        "card dimensions",
    ) else {
        return;
    };

    commands
        .spawn((
            CardPanel,
            card_panel,
            Transform::from_xyz(card_x, card_y, 0.0),
        ))
        .observe(on_panel_clicked);
}

fn spawn_photo_panel_with_title(
    commands: &mut Commands,
    index_x: f32,
    index_y: f32,
    index_height_m: f32,
    title_gap: f32,
) {
    let Some(index_panel) = build_panel_or_log(
        DiegeticPanel::world()
            .paper(PaperSize::Photo5x7)
            .anchor(Anchor::Center)
            .surface_shadow(SurfaceShadow::On)
            .layout(|b| build_index_content(b, false))
            .build(),
        "index card dimensions",
    ) else {
        return;
    };

    commands
        .spawn((
            IndexPanel,
            index_panel,
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
}

fn spawn_index_card_rulers(
    commands: &mut Commands,
    index_x: f32,
    index_y: f32,
    index_width_m: f32,
    index_height_m: f32,
    ruler_color: Color,
) {
    // Vertical ruler (right side).
    let index_ruler_x = index_x + index_width_m / 2.0 + f32::from(RULER_GAP);
    let index_sixteenths = (INDEX_HEIGHT.0 * 16.0).round().to_i32();
    let index_ruler_height = In(INDEX_HEIGHT.0 + EDGE_LABEL_EXTRA.0);
    let index_ruler_top = index_y + index_height_m / 2.0 + f32::from(EDGE_LABEL_EXTRA);
    let Some(index_vertical_ruler) = build_panel_or_log(
        DiegeticPanel::world()
            .size(PANEL_RULER_INCH_WIDTH, index_ruler_height)
            .anchor(Anchor::TopLeft)
            .with_tree(build_imperial_panel_ruler(
                index_sixteenths,
                index_ruler_height,
                ruler_color,
            ))
            .build(),
        "index vertical ruler dimensions",
    ) else {
        return;
    };

    commands.spawn((
        PanelRuler,
        index_vertical_ruler,
        Transform::from_xyz(index_ruler_x, index_ruler_top, 0.0),
    ));

    // Horizontal ruler (bottom).
    let index_bottom_ruler_x = index_x - index_width_m / 2.0;
    let index_bottom_ruler_y = index_y - index_height_m / 2.0 - f32::from(RULER_GAP);
    let index_width_sixteenths = (INDEX_WIDTH.0 * 16.0).round().to_i32();
    let Some(index_horizontal_ruler) = build_panel_or_log(
        DiegeticPanel::world()
            .size(INDEX_WIDTH, PANEL_RULER_INCH_WIDTH)
            .anchor(Anchor::TopLeft)
            .with_tree(build_imperial_horizontal_ruler(
                index_width_sixteenths,
                ruler_color,
            ))
            .build(),
        "index horizontal ruler dimensions",
    ) else {
        return;
    };

    commands.spawn((
        PanelRuler,
        index_horizontal_ruler,
        Transform::from_xyz(index_bottom_ruler_x, index_bottom_ruler_y, 0.0),
    ));
}

fn spawn_rulers(
    commands: &mut Commands,
    ruler_color: Color,
    a4_page_x: f32,
    a4_page_y: f32,
    a4_width_meters: f32,
    a4_height_meters: f32,
    card_x: f32,
    card_y: f32,
    card_width_m: f32,
    card_height_m: f32,
) {
    // A4 vertical ruler (left side).
    let a4_ruler_x = a4_page_x - a4_width_meters / 2.0 - f32::from(RULER_GAP);
    let a4_ruler_top = a4_page_y + a4_height_meters / 2.0;
    let Some(a4_vertical_ruler) = build_panel_or_log(
        DiegeticPanel::world()
            .size(PANEL_RULER_WIDTH, A4_HEIGHT)
            .anchor(Anchor::TopRight)
            .with_tree(build_metric_panel_ruler(A4_HEIGHT.0.to_i32(), ruler_color))
            .build(),
        "A4 vertical ruler dimensions",
    ) else {
        return;
    };

    commands.spawn((
        PanelRuler,
        a4_vertical_ruler,
        Transform::from_xyz(a4_ruler_x, a4_ruler_top, 0.0),
    ));

    // A4 horizontal ruler (bottom).
    let a4_bottom_ruler_x = a4_page_x - a4_width_meters / 2.0;
    let a4_bottom_ruler_y = a4_page_y - a4_height_meters / 2.0 - f32::from(RULER_GAP);
    let Some(a4_horizontal_ruler) = build_panel_or_log(
        DiegeticPanel::world()
            .size(A4_WIDTH, PANEL_RULER_WIDTH)
            .anchor(Anchor::TopLeft)
            .with_tree(build_metric_horizontal_ruler(
                A4_WIDTH.0.to_i32(),
                ruler_color,
            ))
            .build(),
        "A4 horizontal ruler dimensions",
    ) else {
        return;
    };

    commands.spawn((
        PanelRuler,
        a4_horizontal_ruler,
        Transform::from_xyz(a4_bottom_ruler_x, a4_bottom_ruler_y, 0.0),
    ));

    // Card vertical ruler (right side).
    let card_ruler_x = card_x + card_width_m / 2.0 + f32::from(RULER_GAP);
    let card_sixteenths = (CARD_HEIGHT.0 * 16.0).round().to_i32();
    let card_ruler_height = In(CARD_HEIGHT.0 + EDGE_LABEL_EXTRA.0);
    let card_ruler_top = card_y + card_height_m / 2.0 + f32::from(EDGE_LABEL_EXTRA);
    let Some(card_vertical_ruler) = build_panel_or_log(
        DiegeticPanel::world()
            .size(PANEL_RULER_INCH_WIDTH, card_ruler_height)
            .anchor(Anchor::TopLeft)
            .with_tree(build_imperial_panel_ruler(
                card_sixteenths,
                card_ruler_height,
                ruler_color,
            ))
            .build(),
        "card vertical ruler dimensions",
    ) else {
        return;
    };

    commands.spawn((
        PanelRuler,
        card_vertical_ruler,
        Transform::from_xyz(card_ruler_x, card_ruler_top, 0.0),
    ));

    // Card horizontal ruler (bottom).
    let card_bottom_ruler_x = card_x - card_width_m / 2.0;
    let card_bottom_ruler_y = card_y - card_height_m / 2.0 - f32::from(RULER_GAP);
    let card_w_sixteenths = (CARD_WIDTH.0 * 16.0).round().to_i32();
    let Some(card_horizontal_ruler) = build_panel_or_log(
        DiegeticPanel::world()
            .size(CARD_WIDTH, PANEL_RULER_INCH_WIDTH)
            .anchor(Anchor::TopLeft)
            .with_tree(build_imperial_horizontal_ruler(
                card_w_sixteenths,
                ruler_color,
            ))
            .build(),
        "card horizontal ruler dimensions",
    ) else {
        return;
    };

    commands.spawn((
        PanelRuler,
        card_horizontal_ruler,
        Transform::from_xyz(card_bottom_ruler_x, card_bottom_ruler_y, 0.0),
    ));
}

const PERSPECTIVE_FOV: f32 = std::f32::consts::FRAC_PI_4;

fn perspective_to_orthographic_radius(r: f32) -> f32 { r * (PERSPECTIVE_FOV / 2.0).tan() * 2.0 }

fn orthographic_to_perspective_radius(r: f32) -> f32 { r / ((PERSPECTIVE_FOV / 2.0).tan() * 2.0) }

/// P key: switch to perspective. O key: switch to orthographic.
fn toggle_projection(
    keys: Res<ButtonInput<KeyCode>>,
    mut cameras: Query<(&mut Projection, &mut OrbitCam)>,
    mut projection_state: ResMut<CameraProjection>,
) {
    let to_perspective = keys.just_pressed(KeyCode::KeyP);
    let to_ortho = keys.just_pressed(KeyCode::KeyO);
    if !to_perspective && !to_ortho {
        return;
    }
    for (mut proj, mut poc) in &mut cameras {
        if to_ortho && matches!(&*proj, Projection::Perspective(_)) {
            let r = poc.radius.unwrap_or(1.0);
            let ortho_r = perspective_to_orthographic_radius(r);
            poc.radius = Some(ortho_r);
            poc.target_radius = ortho_r;
            *proj = Projection::Orthographic(OrthographicProjection {
                scaling_mode: bevy::camera::ScalingMode::FixedVertical {
                    viewport_height: 1.0,
                },
                far: 40.0,
                ..OrthographicProjection::default_3d()
            });
            poc.force_update();
            *projection_state = CameraProjection::Orthographic;
        } else if to_perspective && matches!(&*proj, Projection::Orthographic(_)) {
            let r = poc.radius.unwrap_or(1.0);
            let persp_r = orthographic_to_perspective_radius(r);
            poc.radius = Some(persp_r);
            poc.target_radius = persp_r;
            *proj = Projection::Perspective(PerspectiveProjection {
                near: 0.001,
                near_clip_plane: Vec4::new(0.0, 0.0, -1.0, -0.001),
                fov: PERSPECTIVE_FOV,
                ..default()
            });
            poc.force_update();
            *projection_state = CameraProjection::Perspective;
        }
    }
}

// ── Toggle systems ───────────────────────────────────────────────────

fn toggle_debug_outlines(
    keys: Res<ButtonInput<KeyCode>>,
    mut debug: ResMut<DebugOutlines>,
    a4_panels: Query<Entity, With<A4Panel>>,
    card_panels: Query<Entity, (With<CardPanel>, Without<A4Panel>, Without<IndexPanel>)>,
    index_panels: Query<Entity, (With<IndexPanel>, Without<A4Panel>, Without<CardPanel>)>,
    mut commands: Commands,
) {
    if !keys.just_pressed(KeyCode::KeyD) {
        return;
    }
    debug.toggle();
    let on = debug.is_on();
    bevy::log::info!("debug outlines: {on}");

    for entity in &a4_panels {
        commands.set_tree(entity, build_a4_page(on));
    }
    for entity in &card_panels {
        commands.set_tree(entity, build_card(on));
    }
    for entity in &index_panels {
        commands.set_tree(entity, build_index_page(on));
    }
}

fn toggle_rulers(
    keys: Res<ButtonInput<KeyCode>>,
    mut rulers_state: ResMut<Rulers>,
    mut rulers: Query<&mut Visibility, With<PanelRuler>>,
) {
    if !keys.just_pressed(KeyCode::KeyR) {
        return;
    }
    rulers_state.toggle();
    let vis = match *rulers_state {
        Rulers::Visible => Visibility::Inherited,
        Rulers::Hidden => Visibility::Hidden,
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

fn build_metric_panel_ruler(height_millimeters: i32, ruler_color: Color) -> LayoutTree {
    let mut builder = LayoutBuilder::new(PANEL_RULER_WIDTH, Mm(height_millimeters.to_f32()));
    let label_style = LayoutTextStyle::new(Pt(8.0)).with_color(ruler_color);
    let last_centimeter_mark = height_millimeters / 10;
    // Top spacer: distance from top of ruler to center of topmost cm block.
    let top_spacer = last_centimeter_mark
        .to_f32()
        .mul_add(-10.0, height_millimeters.to_f32())
        - 5.0;

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
                    for centimeter in (1..=last_centimeter_mark).rev() {
                        b.with(
                            El::new()
                                .height(Sizing::fixed(Mm(10.0)))
                                .width(Sizing::GROW)
                                .child_align_x(AlignX::Right)
                                .child_align_y(AlignY::Center),
                            |b| {
                                b.text(format!("{centimeter}"), label_style.clone());
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
                    .height(Sizing::fixed(Mm(height_millimeters.to_f32())))
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
                                height_millimeters,
                                1.0,
                                AlignX::Right,
                                ruler_color,
                                millimeter_tick_size,
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

const fn millimeter_tick_size(mm: i32) -> (f32, f32) {
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
    let height_inches = height_sixteenths.to_f32() / 16.0;
    let last_label_inch = height_sixteenths / 16;
    let top_spacer = panel_height.0 - last_label_inch.to_f32() - 0.5;
    let mut builder = LayoutBuilder::new(PANEL_RULER_INCH_WIDTH, panel_height);
    let label_style = LayoutTextStyle::new(Pt(8.0)).with_color(ruler_color);
    let sixteenth_height = 1.0 / 16.0;

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
                    .height(Sizing::fixed(In(height_inches)))
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
                                sixteenth_height,
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
fn build_metric_horizontal_ruler(width_millimeters: i32, ruler_color: Color) -> LayoutTree {
    let mut builder = LayoutBuilder::new(Mm(width_millimeters.to_f32()), PANEL_RULER_WIDTH);
    let label_style = LayoutTextStyle::new(Pt(8.0)).with_color(ruler_color);
    // Labels go at centimeter 1 through the last label slot, each centered in
    // a 10mm block.
    // Skip the cm at the exact edge (it's just a tick, no room for a label).
    let last_label_centimeter = (width_millimeters - 5) / 10;
    let right_spacer = last_label_centimeter
        .to_f32()
        .mul_add(-10.0, width_millimeters.to_f32() - 5.0);

    builder.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::GROW)
            .direction(Direction::TopToBottom),
        |b| {
            // ── Top row: spine + ticks ──────────────────────────
            b.with(
                El::new()
                    .width(Sizing::fixed(Mm(width_millimeters.to_f32())))
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
                            build_horizontal_ticks(
                                b,
                                width_millimeters,
                                1.0,
                                ruler_color,
                                millimeter_tick_size,
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
                    for centimeter in 1..=last_label_centimeter {
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
                                        b.text(format!("{centimeter}"), label_style.clone());
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
    let width_inches = width_sixteenths.to_f32() / 16.0;
    let mut builder = LayoutBuilder::new(In(width_inches), PANEL_RULER_INCH_WIDTH);
    let label_style = LayoutTextStyle::new(Pt(8.0)).with_color(ruler_color);
    let last_label_inch = width_sixteenths / 16;
    let right_spacer = width_inches - 0.5 - last_label_inch.to_f32();
    let sixteenth_width = 1.0 / 16.0;

    builder.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::GROW)
            .direction(Direction::TopToBottom),
        |b| {
            // ── Top row: spine + ticks ──────────────────────────
            b.with(
                El::new()
                    .width(Sizing::fixed(In(width_inches)))
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
                                sixteenth_width,
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

/// Builds an A4 page layout tree (used by `toggle_debug_outlines` for runtime rebuild).
fn build_a4_page(debug: bool) -> bevy_diegetic::LayoutTree {
    let mut builder = LayoutBuilder::new(A4_WIDTH, A4_HEIGHT);
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
            .size(A4_WIDTH, A4_HEIGHT)
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
                        "The global CascadeDefaults resource sets defaults for \
                         every panel: layout in meters, fonts in points. \
                         Override per-panel with layout_unit and font_unit, or \
                         per-element with types like Mm(10.0) and Pt(24.0) \
                         inline. The system converts at layout time so the \
                         engine always works in a consistent coordinate space \
                         internally.",
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

/// Builds a business card layout tree (used by `toggle_debug_outlines` for runtime rebuild).
fn build_card(debug: bool) -> bevy_diegetic::LayoutTree {
    let mut builder = LayoutBuilder::new(CARD_WIDTH, CARD_HEIGHT);
    build_card_content(&mut builder, debug);
    builder.build()
}

/// Populates a business card layout into the given builder.
fn build_card_content(builder: &mut LayoutBuilder, debug: bool) {
    let db = debug_border(debug, DEBUG_OUTLINE);

    builder.with(
        El::new()
            .size(CARD_WIDTH, CARD_HEIGHT)
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

/// Builds a 5×7 index page layout tree (used by `toggle_debug_outlines` for runtime rebuild).
fn build_index_page(debug: bool) -> bevy_diegetic::LayoutTree {
    let mut builder = LayoutBuilder::new(INDEX_WIDTH, INDEX_HEIGHT);
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
            .size(INDEX_WIDTH, INDEX_HEIGHT)
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
