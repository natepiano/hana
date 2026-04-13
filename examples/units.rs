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
use bevy_diegetic::AlignY;
use bevy_diegetic::Anchor;
use bevy_diegetic::Border;
use bevy_diegetic::DiegeticPanel;
use bevy_diegetic::DiegeticUiPlugin;
use bevy_diegetic::Direction;
use bevy_diegetic::El;
use bevy_diegetic::LayoutBuilder;
use bevy_diegetic::LayoutTextStyle;
use bevy_diegetic::LayoutTree;
use bevy_diegetic::Padding;
use bevy_diegetic::Sizing;
use bevy_diegetic::Unit;
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
const A4_W: f32 = 210.0; // mm
const A4_H: f32 = 297.0; // mm

// ── US business card dimensions ──────────────────────────────────────
const CARD_W: f32 = 3.5; // inches
const CARD_H: f32 = 2.0; // inches

// ── HUD ─────────────────────────────────────────────────────────────
const HUD_HEIGHT: f32 = 48.0;
const HUD_PADDING: f32 = 12.0;
const HUD_GAP: f32 = 14.0;
const HUD_TITLE_SIZE: f32 = 16.0;
const HUD_HINT_SIZE: f32 = 12.0;
const HUD_BACKGROUND: Color = Color::srgba(0.02, 0.03, 0.07, 0.92);
const HUD_FRAME_BACKGROUND: Color = Color::srgba(0.01, 0.01, 0.03, 0.95);
const HUD_BORDER_ACCENT: Color = Color::srgba(0.15, 0.7, 0.9, 0.5);
const HUD_BORDER_DIM: Color = Color::srgba(0.1, 0.4, 0.6, 0.3);
const HUD_TITLE_COLOR: Color = Color::srgb(0.9, 0.95, 1.0);
const HUD_ACTIVE_COLOR: Color = Color::srgb(0.3, 1.0, 0.8);
const HUD_DIVIDER_COLOR: Color = Color::srgba(0.15, 0.4, 0.6, 0.25);
const HUD_INACTIVE_COLOR: Color = Color::srgba(0.6, 0.65, 0.8, 0.85);

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
const RULER_LINE_WIDTH: f32 = 2.0;
const LABEL_SIZE: f32 = 8.0; // points
const LABEL_GAP: f32 = 0.001;

// ── Panel ruler (mm units) ──────────────────────────────────────────
const PANEL_RULER_CM_LINE: f32 = 0.3;
const PANEL_RULER_CM_TICK: f32 = 5.0;
const PANEL_RULER_MM1_LINE: f32 = 0.15;
const PANEL_RULER_MM1_TICK: f32 = 2.0;
const PANEL_RULER_MM5_LINE: f32 = 0.2;
const PANEL_RULER_MM5_TICK: f32 = 3.5;
const PANEL_RULER_SPINE: f32 = 0.3;
const PANEL_RULER_WIDTH: f32 = 10.0;

// ── Home / zoom ─────────────────────────────────────────────────────
const HOME_FOCUS_Y: f32 = A4_H * MM_TO_M / 2.0 + LIFT;
const HOME_PITCH: f32 = 0.1;
const HOME_RADIUS: f32 = 0.5;
const HOME_YAW: f32 = 0.0;
const ZOOM_DURATION_MS: u64 = 1000;
const ZOOM_MARGIN: f32 = 0.08;

// ── Colors ───────────────────────────────────────────────────────────
const A4_DIM_COLOR: Color = Color::srgba(0.0, 0.0, 0.1, 1.0);
const A4_TEXT_COLOR: Color = Color::BLACK;
const CARD_DIM_COLOR: Color = Color::srgba(0.85, 0.88, 0.95, 0.9);
const CARD_TEXT_COLOR: Color = Color::WHITE;
const DEBUG_COLOR_WIDTH: f32 = 0.6;

// ── Marker components ────────────────────────────────────────────────

#[derive(Component)]
struct A4Panel;

#[derive(Component)]
struct CardPanel;

#[derive(Component)]
struct ControlsPanel;

#[derive(Component)]
struct PanelRuler;

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
        .add_systems(Update, update_controls_hud)
        .add_systems(Update, home_camera)
        .run();
}

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut gizmo_assets: ResMut<Assets<GizmoAsset>>,
    windows: Query<&Window>,
) {
    let a4_width_m = A4_W * MM_TO_M;
    let a4_height_m = A4_H * MM_TO_M;
    let card_width_m = CARD_W * IN_TO_M;
    let card_height_m = CARD_H * IN_TO_M;

    let total_w = a4_width_m + GAP + card_width_m;
    let group_left = -total_w / 2.0;

    let a4_x = group_left + a4_width_m / 2.0;
    let a4_y = a4_height_m / 2.0 + LIFT;

    let a4_top = a4_y + a4_height_m / 2.0;
    let card_x = group_left + a4_width_m + GAP + card_width_m / 2.0;
    let card_y = a4_top - card_height_m / 2.0;

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
        build_metric_ruler(a4_width_m, a4_height_m, ruler_color),
        |cmd, container| {
            spawn_metric_labels(cmd, container, a4_width_m, a4_height_m, label_style.clone());
        },
    );

    // ── Panel-based ruler (comparison) ──────────────────────────────
    let gizmo_spine_x = a4_x - a4_width_m / 2.0 - RULER_GAP;
    let panel_ruler_right = gizmo_spine_x - CM_TICK - LABEL_GAP - 0.012;
    let panel_ruler_top = a4_y + a4_height_m / 2.0;
    commands.spawn((
        PanelRuler,
        DiegeticPanel {
            tree: build_panel_ruler(A4_H.to_i32(), Color::WHITE),
            width: PANEL_RULER_WIDTH,
            height: A4_H,
            layout_unit: Some(Unit::Millimeters),
            anchor: Anchor::TopRight,
            ..default()
        },
        Transform::from_xyz(panel_ruler_right, panel_ruler_top, 0.0),
    ));

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
        build_inch_ruler(card_width_m, card_height_m, ruler_color),
        |cmd, container| {
            spawn_inch_labels(cmd, container, card_width_m, card_height_m, label_style);
        },
    );

    // ── Controls HUD ────────────────────────────────────────────────
    let unlit_material = bevy_diegetic::default_panel_material();
    let unlit = StandardMaterial {
        unlit: true,
        ..unlit_material
    };
    let hud_width = windows.iter().next().map_or(800.0, Window::width);
    let (mut hud_panel, screen_space) = DiegeticPanel::builder()
        .size_px(hud_width, HUD_HEIGHT)
        .anchor(Anchor::TopLeft)
        .material(unlit.clone())
        .text_material(unlit)
        .layout(|b| {
            build_controls_content(b, false, true, true);
        })
        .build_screen_space();
    hud_panel.tree = build_controls_tree(hud_width, false, true, true);
    commands.spawn((ControlsPanel, hud_panel, screen_space, Transform::default()));

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
            illuminance: 3_000.0,
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
    let a4_width_m = A4_W * MM_TO_M;
    let a4_height_m = A4_H * MM_TO_M;
    let card_width_m = CARD_W * IN_TO_M;
    let card_height_m = CARD_H * IN_TO_M;
    let ruler_color = Color::srgba(0.55, 0.55, 0.55, 0.7);
    let label_style = WorldTextStyle::new(LABEL_SIZE)
        .with_unit(Unit::Points)
        .with_color(ruler_color);

    for a4_entity in &a4_panels {
        spawn_ruler_on_panel(
            &mut commands,
            &mut gizmo_assets,
            a4_entity,
            build_metric_ruler(a4_width_m, a4_height_m, ruler_color),
            |cmd, container| {
                spawn_metric_labels(cmd, container, a4_width_m, a4_height_m, label_style.clone());
            },
        );
    }

    for card_entity in &card_panels {
        spawn_ruler_on_panel(
            &mut commands,
            &mut gizmo_assets,
            card_entity,
            build_inch_ruler(card_width_m, card_height_m, ruler_color),
            |cmd, container| {
                spawn_inch_labels(
                    cmd,
                    container,
                    card_width_m,
                    card_height_m,
                    label_style.clone(),
                );
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

// ── Panel ruler ─────────────────────────────────────────────────────

fn build_panel_ruler(height_mm: i32, ruler_color: Color) -> LayoutTree {
    let mut builder = LayoutBuilder::new(PANEL_RULER_WIDTH, height_mm.to_f32());
    let label_style = LayoutTextStyle::new(8.0).with_color(ruler_color);
    let last_cm = height_mm / 10;
    // Top spacer: distance from top of ruler to center of topmost cm block.
    let top_spacer = height_mm.to_f32() - last_cm.to_f32() * 10.0 - 5.0;

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
                    .padding(Padding::new(0.0, 0.8, 0.0, 0.0)),
                |b| {
                    // Top spacer.
                    if top_spacer > 0.0 {
                        b.with(
                            El::new()
                                .height(Sizing::fixed(top_spacer))
                                .width(Sizing::GROW),
                            |_| {},
                        );
                    }
                    // One 10mm block per cm, with text centered.
                    for cm in (1..=last_cm).rev() {
                        b.with(
                            El::new()
                                .height(Sizing::fixed(10.0))
                                .width(Sizing::GROW)
                                .child_align_x(AlignX::Right)
                                .child_align_y(AlignY::Center),
                            |b| {
                                b.text(&format!("{cm}"), label_style.clone());
                            },
                        );
                    }
                    // Bottom spacer (5mm below cm 1).
                    b.with(
                        El::new().height(Sizing::fixed(5.0)).width(Sizing::GROW),
                        |_| {},
                    );
                },
            );

            // ── Right column: ticks + spine ─────────────────────
            b.with(
                El::new()
                    .width(Sizing::fixed(PANEL_RULER_CM_TICK + PANEL_RULER_SPINE))
                    .height(Sizing::GROW)
                    .direction(Direction::LeftToRight)
                    .child_align_x(AlignX::Right),
                |b| {
                    // Tick column: background elements that butt against
                    // the spine but do not overlap it.
                    b.with(
                        El::new()
                            .width(Sizing::GROW)
                            .height(Sizing::GROW)
                            .direction(Direction::TopToBottom)
                            .child_align_x(AlignX::Right),
                        |b| {
                            for mm in (0..height_mm).rev() {
                                let (tick_width, tick_line) = mm_tick_size(mm);
                                b.with(
                                    El::new()
                                        .width(Sizing::GROW)
                                        .height(Sizing::fixed(1.0))
                                        .child_align_x(AlignX::Right)
                                        .child_align_y(AlignY::Bottom),
                                    |b| {
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
                        },
                    );
                    // Spine: a narrow column with background.
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
        (PANEL_RULER_CM_TICK, PANEL_RULER_CM_LINE)
    } else if mm % 5 == 0 {
        (PANEL_RULER_MM5_TICK, PANEL_RULER_MM5_LINE)
    } else {
        (PANEL_RULER_MM1_TICK, PANEL_RULER_MM1_LINE)
    }
}

// ── Panel content ────────────────────────────────────────────────────

const DEBUG_BORDER_COLOR: Color = Color::srgba(1.0, 0.2, 0.2, 0.8);

fn debug_border(debug: bool, width: f32) -> Option<Border> {
    if debug {
        Some(Border::all(width, DEBUG_BORDER_COLOR))
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
    let db = debug_border(debug, DEBUG_COLOR_WIDTH);

    builder.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::GROW)
            .padding(Padding::all(15.0))
            .direction(Direction::TopToBottom)
            .child_gap(2.0)
            .background(Color::WHITE),
        |b| {
            debug_text(
                b,
                "A4 Paper — 210 × 297 mm",
                LayoutTextStyle::new(24.0).with_color(A4_TEXT_COLOR),
                db,
            );
            debug_text(
                b,
                "layout: Millimeters  |  fonts: Points",
                LayoutTextStyle::new(16.0).with_color(A4_DIM_COLOR),
                db,
            );
            debug_text(
                b,
                "72pt",
                LayoutTextStyle::new(72.0).with_color(A4_TEXT_COLOR),
                db,
            );
            debug_text(
                b,
                "36pt",
                LayoutTextStyle::new(36.0).with_color(A4_TEXT_COLOR),
                db,
            );
            debug_text(
                b,
                "24pt",
                LayoutTextStyle::new(24.0).with_color(A4_TEXT_COLOR),
                db,
            );
            debug_text(
                b,
                "18pt",
                LayoutTextStyle::new(18.0).with_color(A4_TEXT_COLOR),
                db,
            );
            debug_text(
                b,
                "12pt",
                LayoutTextStyle::new(12.0).with_color(A4_TEXT_COLOR),
                db,
            );
            debug_text(
                b,
                "9pt",
                LayoutTextStyle::new(9.0).with_color(A4_TEXT_COLOR),
                db,
            );
        },
    );

    builder.build()
}

fn build_card(debug: bool) -> bevy_diegetic::LayoutTree {
    let mut builder = LayoutBuilder::new(CARD_W, CARD_H);
    let db = debug_border(debug, 0.008);

    builder.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::GROW)
            .padding(Padding::all(0.15))
            .direction(Direction::TopToBottom)
            .child_gap(0.04)
            .background(Color::srgb(0.392, 0.584, 0.929)),
        |b| {
            debug_text(
                b,
                "JANE DOE",
                LayoutTextStyle::new(18.0).with_color(CARD_TEXT_COLOR),
                db,
            );
            debug_text(
                b,
                "Software Engineer",
                LayoutTextStyle::new(15.0).with_color(CARD_DIM_COLOR),
                db,
            );
            debug_text(
                b,
                "jane@example.com",
                LayoutTextStyle::new(12.5).with_color(CARD_TEXT_COLOR),
                db,
            );
            debug_text(
                b,
                "+1 (555) 012-3456",
                LayoutTextStyle::new(12.5).with_color(CARD_TEXT_COLOR),
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
                        LayoutTextStyle::new(10.0).with_color(CARD_DIM_COLOR),
                        db,
                    );
                    b.with(El::new().width(Sizing::GROW), |_| {});
                    debug_text(
                        b,
                        "3½ × 2 in",
                        LayoutTextStyle::new(12.5).with_color(CARD_DIM_COLOR),
                        db,
                    );
                },
            );
        },
    );

    builder.build()
}

fn build_controls_tree(width: f32, debug: bool, rulers: bool, perspective: bool) -> LayoutTree {
    let mut builder = LayoutBuilder::new(width, HUD_HEIGHT);
    build_controls_content(&mut builder, debug, rulers, perspective);
    builder.build()
}

fn build_controls_content(b: &mut LayoutBuilder, debug: bool, rulers: bool, perspective: bool) {
    let title = LayoutTextStyle::new(HUD_TITLE_SIZE).with_color(HUD_TITLE_COLOR);

    b.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::GROW)
            .padding(Padding::all(2.0))
            .background(HUD_FRAME_BACKGROUND)
            .border(Border::all(2.0, HUD_BORDER_ACCENT)),
        |b| {
            b.with(
                El::new()
                    .width(Sizing::GROW)
                    .height(Sizing::GROW)
                    .direction(Direction::LeftToRight)
                    .padding(Padding::new(8.0, HUD_PADDING, 8.0, HUD_PADDING))
                    .child_gap(HUD_GAP)
                    .child_align_y(AlignY::Center)
                    .clip()
                    .background(HUD_BACKGROUND)
                    .border(Border::all(1.0, HUD_BORDER_DIM)),
                |b| {
                    b.text("CONTROLS", title);
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
                    b.text(
                        "H Home",
                        LayoutTextStyle::new(HUD_HINT_SIZE).with_color(HUD_INACTIVE_COLOR),
                    );
                },
            );
        },
    );
}

fn hud_separator(b: &mut LayoutBuilder) {
    b.with(
        El::new()
            .width(Sizing::fixed(1.0))
            .height(Sizing::GROW)
            .background(HUD_DIVIDER_COLOR),
        |_| {},
    );
}

fn update_controls_hud(
    windows: Query<&Window>,
    mut huds: Query<(&mut Transform, &mut DiegeticPanel), With<ControlsPanel>>,
    debug: Res<DebugOutlines>,
    rulers: Res<RulersVisible>,
    cameras: Query<&Projection>,
    mut previous_state: Local<(u32, bool, bool, bool)>,
) {
    let Ok(window) = windows.single() else {
        return;
    };
    let win_width = window.width();
    let half_width = win_width / 2.0;
    let half_height = window.height() / 2.0;
    let width_bits = win_width.to_bits();

    let perspective = cameras
        .iter()
        .any(|p| matches!(p, Projection::Perspective(_)));

    let state = (width_bits, debug.0, rulers.0, perspective);

    for (mut transform, mut panel) in &mut huds {
        transform.translation.x = -half_width;
        transform.translation.y = half_height;

        if *previous_state != state {
            panel.width = win_width;
            panel.tree = build_controls_tree(win_width, debug.0, rulers.0, perspective);
        }
    }
    *previous_state = state;
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
                focus:    Vec3::new(0.0, HOME_FOCUS_Y, 0.0),
                yaw:      HOME_YAW,
                pitch:    HOME_PITCH,
                radius:   HOME_RADIUS,
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
