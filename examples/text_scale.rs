//! Physical text scaling demo.
//!
//! Shows two diegetic panels side by side:
//!
//! - **Left**: An A4 page (210×297mm) at real physical scale.
//! - **Right**: A business card (85×55mm) scaled up so it appears roughly the same height as the A4
//!   page — like zooming into the card.
//!
//! Both use millimeters as layout units. The panel's `world_width / layout_width`
//! ratio converts mm to meters (0.001 for the A4, larger for the scaled card).

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
use bevy_panorbit_camera::PanOrbitCamera;
use bevy_panorbit_camera::PanOrbitCameraPlugin;
use bevy_panorbit_camera::TrackpadBehavior;
use bevy_panorbit_camera_ext::PanOrbitCameraExtPlugin;
use bevy_panorbit_camera_ext::ZoomToFit;
use bevy_window_manager::WindowManagerPlugin;

// ── Paper dimensions (mm for layout, meters for world) ───────────────────

const A4_MM_W: f32 = 210.0;
const A4_MM_H: f32 = 297.0;
const A4_WORLD_W: f32 = 0.210;
const A4_WORLD_H: f32 = 0.297;

const CARD_MM_W: f32 = 85.0;
const CARD_MM_H: f32 = 55.0;
const CARD_WORLD_W: f32 = 0.085;
const CARD_WORLD_H: f32 = 0.055;

/// Scale the card up so its height matches the A4 height.
const CARD_SCALE: f32 = A4_WORLD_H / CARD_WORLD_H;

// ── Font sizes (in mm, since layout units are mm) ────────────────────────

const TITLE_SIZE: f32 = 20.0;
const BODY_SIZE: f32 = 12.0;
const CAPTION_SIZE: f32 = 9.0;

// ── Zoom ─────────────────────────────────────────────────────────────────

const ZOOM_MARGIN: f32 = 0.08;
const ZOOM_DURATION_MS: u64 = 1000;
const GAP: f32 = 0.02;

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
        .run();
}

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let border_color = Color::WHITE;
    let text_color = Color::srgb(0.9, 0.9, 0.9);
    let dim_color = Color::srgba(0.7, 0.7, 0.7, 0.6);

    // ── Group centering ───────────────────────────────────────────────
    let card_scaled_w = CARD_WORLD_W * CARD_SCALE;
    let card_scaled_h = CARD_WORLD_H * CARD_SCALE;
    let max_h = card_scaled_h.max(A4_WORLD_H);
    let total_w = A4_WORLD_W + GAP + card_scaled_w;
    let group_left = -total_w / 2.0;

    // ── A4 page (left) — real physical scale ────────────────────────────
    let a4_x = group_left + A4_WORLD_W / 2.0;
    let title_label = format!("Title: {TITLE_SIZE}mm");
    let body_label = format!("Body: {BODY_SIZE}mm");
    let caption_label = format!("Caption: {CAPTION_SIZE}mm");
    let a4_lines: Vec<(&str, f32, Color)> = vec![
        ("A4 — 210 × 297 mm", TITLE_SIZE, text_color),
        ("1 world unit = 1 meter", CAPTION_SIZE, dim_color),
        ("Layout units are millimeters", CAPTION_SIZE, dim_color),
        ("world_width: 0.210  (210mm)", CAPTION_SIZE, dim_color),
        ("world_height: 0.297  (297mm)", CAPTION_SIZE, dim_color),
        ("Text sizes are in mm:", BODY_SIZE, text_color),
        (&title_label, CAPTION_SIZE, dim_color),
        (&body_label, CAPTION_SIZE, dim_color),
        (&caption_label, CAPTION_SIZE, dim_color),
    ];
    commands
        .spawn((
            DiegeticPanel {
                tree:          build_page(A4_MM_W, A4_MM_H, border_color, &a4_lines),
                layout_width:  A4_MM_W,
                layout_height: A4_MM_H,
                world_width:   A4_WORLD_W,
                world_height:  A4_WORLD_H,
            },
            Transform::from_xyz(a4_x, A4_WORLD_H / 2.0 + 0.01, 0.0),
        ))
        .observe(on_panel_clicked);

    // ── Business card (right) — scaled up to match A4 height ────────────
    let card_x = group_left + A4_WORLD_W + GAP + card_scaled_w / 2.0;
    let card_lines: Vec<(&str, f32, Color)> = vec![];
    commands
        .spawn((
            DiegeticPanel {
                tree:          build_page(CARD_MM_W, CARD_MM_H, border_color, &card_lines),
                layout_width:  CARD_MM_W,
                layout_height: CARD_MM_H,
                world_width:   card_scaled_w,
                world_height:  card_scaled_h,
            },
            Transform::from_xyz(card_x, card_scaled_h / 2.0 + 0.01, 0.0),
        ))
        .observe(on_panel_clicked);

    // ── Ground plane ────────────────────────────────────────────────────
    let ground_w = total_w + 0.1;
    let ground_h = max_h + 0.1;
    let ground = commands
        .spawn((
            Mesh3d(meshes.add(Plane3d::default().mesh().size(ground_w, ground_h))),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: Color::srgb(0.12, 0.12, 0.12),
                double_sided: true,
                cull_mode: None,
                ..default()
            })),
            Transform::from_xyz(0.0, 0.0, 0.0),
        ))
        .observe(on_ground_clicked)
        .id();
    commands.insert_resource(SceneBounds(ground));

    // ── Light ────────────────────────────────────────────────────────────
    commands.spawn((
        DirectionalLight {
            shadows_enabled: true,
            ..default()
        },
        Transform::from_xyz(0.5, 1.5, 1.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));

    // ── Camera ──────────────────────────────────────────────────────────
    let mid_y = max_h / 2.0;
    commands.spawn((PanOrbitCamera {
        focus: Vec3::new(0.0, mid_y, 0.0),
        radius: Some(0.6),
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
    },));
}

fn build_page(
    layout_w: f32,
    layout_h: f32,
    border_color: Color,
    lines: &[(&str, f32, Color)],
) -> bevy_diegetic::LayoutTree {
    let mut builder = LayoutBuilder::new(layout_w, layout_h);

    builder.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::GROW)
            .padding(Padding::all(TITLE_SIZE * 0.6))
            .direction(Direction::TopToBottom)
            .child_gap(BODY_SIZE * 0.3)
            .border(Border::all(0.5, border_color)),
        |b| {
            for &(text, size, color) in lines {
                b.text(text, LayoutTextStyle::new(size).with_color(color));
            }
        },
    );

    builder.build()
}

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
