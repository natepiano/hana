//! Side-by-side layout comparison: Clay (C FFI) vs `bevy_diegetic` (Rust).
//!
//! Renders the same status panel using two layout engines side by side.
//! Both use the same parley-backed text measurement, so any visual
//! differences expose real layout bugs rather than measurement drift.
//!
//! - **Right (Diegetic)**: Uses [`DiegeticPanel`] — the plugin handles layout computation, MSDF
//!   text rendering, and debug gizmos. This is the standard usage pattern for `bevy_diegetic`.
//!
//! - **Left (Clay)**: Uses `clay-layout` (C FFI) for layout, then spawns [`WorldText`] entities at
//!   the positions clay computed. This side demonstrates how to use [`DiegeticTextMeasurer`] as a
//!   bridge: clay calls its own measurement callback, which delegates to our parley-backed measurer
//!   via [`TextMeasure`] and [`TextDimensions`]. This pattern works for any external layout engine
//!   that needs a text measurement callback.
//!
//! Controls:
//! - `S` — cycle panel size (small / medium / large)
//! - `D` — toggle text bounding-box debug gizmos
//!
//! Run with:
//! ```sh
//! cargo run --example side_by_side
//! ```

#![allow(clippy::cast_precision_loss)]
#![allow(clippy::needless_borrow)]

use std::sync::Arc;

use bevy::color::Color;
use bevy::color::palettes::css;
use bevy::diagnostic::DiagnosticsStore;
use bevy::diagnostic::FrameTimeDiagnosticsPlugin;
use bevy::prelude::*;
use bevy_brp_extras::BrpExtrasPlugin;
use bevy_brp_extras::PortDisplay;
use bevy_diegetic::AlignX;
use bevy_diegetic::AlignY;
use bevy_diegetic::Border;
use bevy_diegetic::DiegeticPanel;
use bevy_diegetic::DiegeticTextMeasurer;
use bevy_diegetic::DiegeticUiPlugin;
use bevy_diegetic::Direction;
use bevy_diegetic::El;
use bevy_diegetic::LayoutBuilder;
use bevy_diegetic::LayoutTextStyle;
use bevy_diegetic::LayoutTree;
use bevy_diegetic::Padding;
use bevy_diegetic::ShowTextGizmos;
use bevy_diegetic::Sizing;
use bevy_diegetic::TextDimensions;
use bevy_diegetic::Unit;
use bevy_diegetic::WorldText;
use bevy_diegetic::WorldTextStyle;
use bevy_panorbit_camera::PanOrbitCamera;
use bevy_panorbit_camera::PanOrbitCameraPlugin;
use bevy_panorbit_camera::TrackpadBehavior;
use bevy_window_manager::WindowManagerPlugin;
use clay_layout::Clay;
use clay_layout::ClayLayoutScope;
use clay_layout::Declaration;
use clay_layout::fit;
use clay_layout::fixed;
use clay_layout::grow;
use clay_layout::layout::Alignment;
use clay_layout::layout::LayoutAlignmentX;
use clay_layout::layout::LayoutAlignmentY;
use clay_layout::layout::LayoutDirection;
use clay_layout::math::Dimensions;
use clay_layout::render_commands::RenderCommandConfig;
use clay_layout::text::TextElementConfigWrapMode;

// ── Constants ────────────────────────────────────────────────────────────────

/// Layout-unit presets cycled with 'S'. World size is computed from the camera.
const LAYOUT_PRESETS: &[(f32, &str)] = &[(100.0, "small"), (160.0, "medium"), (240.0, "large")];

/// Number of gutter-width gaps: left margin + center gutter + right margin.
const GUTTER_COUNT: f32 = 3.0;

/// Fraction of visible width used as each gutter/margin.
const GUTTER_FRACTION: f32 = 0.06;

/// Panel height-to-width ratio (taller than wide to fit wrapped text).
const PANEL_ASPECT: f32 = 1.4;
const HEADER_HEIGHT: f32 = 20.0;
const DIVIDER_HEIGHT: f32 = 4.0;
const FONT_SIZE: f32 = 2.5;
const SUBTITLE_FONT_SIZE: f32 = 1.4;
const CLAY_FONT_SIZE: u16 = 7;
const CLAY_SUBTITLE_FONT_SIZE: u16 = 4;
const CLAY_RENDERER: &str = "clay";
const DIEGETIC_RENDERER: &str = "diegetic";
const DYNAMIC_UPDATE_INTERVAL: f32 = 1.0;
const WRAP_TEXT: &str = "Lorem ipsum dolor sit amet, consectetur adipiscing elit";

/// `WorldText` uses a fixed scale of 0.01 (layout units → world units).
const WORLD_TEXT_SCALE: f32 = 0.01;

// ── Controls panel dimensions (meters) ──────────────────────────────
const CTRL_W: f32 = 0.089;
const CTRL_H: f32 = 0.03;
const CTRL_FONT: f32 = 3.5;
const CTRL_TITLE_FONT: f32 = 4.2;
const CTRL_ARROW_SIZE: f32 = CTRL_FONT * 0.5;
const CTRL_ROW_H: f32 = 0.005;

// ── Gizmo groups ─────────────────────────────────────────────────────────────

#[derive(Default, Reflect, GizmoConfigGroup)]
struct ClayGizmoGroup;

// ── Marker components ────────────────────────────────────────────────────────

#[derive(Component)]
struct ClayPanelMarker;

/// Marker for `WorldText` entities spawned by the clay side, so we can
/// despawn them on rebuild.
#[derive(Component)]
struct ClayTextEntity;

// ── Stored layout results ────────────────────────────────────────────────────

#[derive(Resource, Default)]
struct ClayLayoutResult(Vec<ClayRect>);

struct ClayRect {
    x:      f32,
    y:      f32,
    width:  f32,
    height: f32,
    kind:   ClayRectKind,
}

enum ClayRectKind {
    Rectangle,
    Text(String, f32),
    Border,
}

/// Current panel sizing. Cycled with 'S'.
///
/// `world_size` is computed from the camera's visible width so panels
/// always fit on screen with equal gutters.
#[derive(Resource)]
struct PanelSizing {
    index:       usize,
    layout_size: f32,
    world_size:  f32,
    gutter:      f32,
}

impl PanelSizing {
    fn scale(&self) -> f32 { self.world_size / self.layout_size }

    fn cycle(&mut self) {
        self.index = (self.index + 1) % LAYOUT_PRESETS.len();
        self.layout_size = LAYOUT_PRESETS[self.index].0;
    }

    fn label(&self) -> &'static str { LAYOUT_PRESETS[self.index].1 }

    fn layout_units_label(&self) -> String { format!("{:.0}", self.layout_size) }

    /// Recompute `world_size` and `gutter` from the visible area at z=0.
    fn fit_to_view(&mut self, visible_width: f32, visible_height: f32) {
        self.gutter = visible_width * GUTTER_FRACTION;
        let from_width = GUTTER_COUNT.mul_add(-self.gutter, visible_width) / 2.0;
        let from_height = self.gutter.mul_add(-2.0, visible_height) / PANEL_ASPECT;
        let max_world_size = from_width.min(from_height);
        let max_layout_size = LAYOUT_PRESETS
            .iter()
            .map(|(s, _)| *s)
            .fold(0.0_f32, f32::max);
        self.world_size = max_world_size * (self.layout_size / max_layout_size);
    }
}

impl Default for PanelSizing {
    fn default() -> Self {
        Self {
            index:       1,
            layout_size: LAYOUT_PRESETS[1].0,
            world_size:  1.5,
            gutter:      0.3,
        }
    }
}

/// Whether text bounding-box debug gizmos are visible. Toggled with 'D'.
#[derive(Resource)]
struct ShowTextDebug(bool);

impl Default for ShowTextDebug {
    fn default() -> Self { Self(true) }
}

/// Dynamic row values updated on a timer.
#[derive(Resource)]
struct DynamicRows {
    timer:    Timer,
    radius:   String,
    fps:      String,
    frame_ms: String,
}

impl Default for DynamicRows {
    fn default() -> Self {
        Self {
            timer:    Timer::from_seconds(DYNAMIC_UPDATE_INTERVAL, TimerMode::Repeating),
            radius:   "--".to_string(),
            fps:      "--".to_string(),
            frame_ms: "--".to_string(),
        }
    }
}

// ── App ──────────────────────────────────────────────────────────────────────

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(BrpExtrasPlugin::default().port_in_title(PortDisplay::NonDefault))
        .add_plugins(DiegeticUiPlugin)
        .add_plugins(PanOrbitCameraPlugin)
        .add_plugins(WindowManagerPlugin)
        .init_gizmo_group::<ClayGizmoGroup>()
        .insert_resource(ShowTextGizmos(true))
        .init_resource::<ClayLayoutResult>()
        .init_resource::<DynamicRows>()
        .init_resource::<PanelSizing>()
        .init_resource::<ShowTextDebug>()
        .add_systems(Startup, setup)
        .add_systems(
            Update,
            (
                cycle_panel_size,
                update_dynamic_rows,
                rebuild_diegetic_panel
                    .after(update_dynamic_rows)
                    .after(cycle_panel_size),
                rebuild_clay_layout
                    .after(update_dynamic_rows)
                    .after(cycle_panel_size),
                spawn_clay_text.after(rebuild_clay_layout),
                toggle_text_debug,
                draw_clay_gizmos,
            ),
        )
        .run();
}

// ── Clay text measurement ────────────────────────────────────────────────────

#[allow(clippy::type_complexity)]
fn clay_measure_with_parley(
    text: &str,
    config: &clay_layout::text::TextConfig,
    measurer: &mut Arc<dyn Fn(&str, &bevy_diegetic::TextMeasure) -> TextDimensions + Send + Sync>,
) -> Dimensions {
    let measure = bevy_diegetic::TextMeasure {
        font_id:        0,
        size:           f32::from(config.font_size),
        weight:         bevy_diegetic::FontWeight::NORMAL,
        slant:          bevy_diegetic::FontSlant::Normal,
        line_height:    f32::from(config.line_height),
        letter_spacing: 0.0,
        word_spacing:   0.0,
        font_features:  bevy_diegetic::FontFeatures::default(),
    };
    let dims = measurer(text, &measure);
    Dimensions {
        width:  dims.width,
        height: dims.height,
    }
}

// ── Setup ────────────────────────────────────────────────────────────────────

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    sizing: Res<PanelSizing>,
    dynamic: Res<DynamicRows>,
) {
    // Ground plane.
    commands.spawn((
        Mesh3d(meshes.add(Plane3d::default().mesh().size(5.0, 5.0))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.08, 0.08, 0.08),
            double_sided: true,
            cull_mode: None,
            ..default()
        })),
        Transform::from_xyz(0.0, -1.2, 0.0),
    ));

    // Dark backdrop behind the panels.
    commands.spawn((
        Mesh3d(meshes.add(Rectangle::new(2.9, 2.0))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.15, 0.15, 0.15),
            double_sided: true,
            cull_mode: None,
            ..default()
        })),
        Transform::from_xyz(0.0, 0.0, -0.5),
    ));

    // Directional lights.
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

    // Camera.
    let midpoint = Vec3::ZERO;
    commands.spawn((
        Camera3d::default(),
        Transform {
            translation: Vec3::new(0.00, 0.15, 2.7),
            rotation: Quat::from_xyzw(0.00, 0.0, 0.0, 1.0),
            ..default()
        },
        PanOrbitCamera {
            focus: midpoint,
            trackpad_behavior: TrackpadBehavior::blender_default(),
            trackpad_sensitivity: 0.5,
            trackpad_pinch_to_zoom_enabled: true,
            ..default()
        },
    ));

    // Panel entities.
    let offset = panel_offset(&sizing);
    let world_size = sizing.world_size;

    // Clay panel (left) — just a marker entity for positioning + gizmo drawing.
    commands.spawn((ClayPanelMarker, Transform::from_xyz(-offset, 0.0, 0.0)));

    // Diegetic panel (right) — uses the plugin for layout + text rendering.
    let diegetic_rows = build_rows(&dynamic, &sizing, DIEGETIC_RENDERER);
    let tree = build_diegetic_tree(&diegetic_rows, world_size);
    let world_height = world_size * PANEL_ASPECT;
    commands.spawn((
        DiegeticPanel {
            tree,
            width: world_size,
            height: world_height,
            font_unit: Some(Unit::Millimeters),
            ..default()
        },
        Transform::from_xyz(offset, world_height * 0.5, 0.0),
    ));

    // Controls panel — below the two panels.
    commands.spawn((
        DiegeticPanel {
            tree: build_controls_panel(),
            width: CTRL_W,
            height: CTRL_H,
            font_unit: Some(Unit::Millimeters),
            ..default()
        },
        Transform::from_xyz(-CTRL_W * 0.5, CTRL_H.mul_add(0.5, -0.85), 0.0),
    ));
}

/// Half-width of one panel plus half the gutter.
const fn panel_offset(sizing: &PanelSizing) -> f32 {
    f32::midpoint(sizing.world_size, sizing.gutter)
}

fn cycle_panel_size(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut sizing: ResMut<PanelSizing>,
    camera: Query<(&GlobalTransform, &Projection)>,
    mut clay_panels: Query<&mut Transform, (With<ClayPanelMarker>, Without<DiegeticPanel>)>,
    mut diegetic_panels: Query<(&mut Transform, &mut DiegeticPanel), Without<ClayPanelMarker>>,
    mut initialized: Local<bool>,
) {
    let pressed = keyboard.just_pressed(KeyCode::KeyS);
    if pressed {
        sizing.cycle();
    }

    // Refit on first frame (camera projection now available) or on key press.
    if !*initialized || pressed {
        *initialized = true;
        if let Ok((gt, Projection::Perspective(persp))) = camera.single() {
            let distance = gt.translation().z.abs();
            let half_height = distance * (persp.fov * 0.5).tan();
            let visible_height = half_height * 2.0;
            let visible_width = visible_height * persp.aspect_ratio;
            sizing.fit_to_view(visible_width, visible_height);
        }
        let offset = panel_offset(&sizing);
        for mut t in &mut clay_panels {
            t.translation.x = -offset;
        }
        for (mut t, mut panel) in &mut diegetic_panels {
            let world_height = sizing.world_size * PANEL_ASPECT;
            t.translation.x = offset;
            t.translation.y = world_height * 0.5;
            // Update panel dimensions to match new sizing.
            panel.width = sizing.world_size;
            panel.height = world_height;
        }
    }
}

// ── Dynamic row updates ──────────────────────────────────────────────────────

fn update_dynamic_rows(
    time: Res<Time>,
    diagnostics: Res<DiagnosticsStore>,
    camera: Query<&PanOrbitCamera>,
    mut dynamic: ResMut<DynamicRows>,
) {
    if let Ok(cam) = camera.single() {
        dynamic.radius = format!("{:.1}", cam.radius.unwrap_or(3.0));
    }

    dynamic.timer.tick(time.delta());
    if dynamic.timer.just_finished() {
        let fps = diagnostics
            .get(&FrameTimeDiagnosticsPlugin::FPS)
            .and_then(bevy::diagnostic::Diagnostic::smoothed);
        let frame_time = diagnostics
            .get(&FrameTimeDiagnosticsPlugin::FRAME_TIME)
            .and_then(bevy::diagnostic::Diagnostic::smoothed);

        dynamic.fps = fps.map_or_else(|| "--".to_string(), |v| format!("{v:.0}"));
        dynamic.frame_ms = frame_time.map_or_else(|| "--".to_string(), |v| format!("{v:.0}"));
    }
}

fn build_controls_panel() -> LayoutTree {
    let border_color = Color::srgb(0.4, 0.4, 0.45);
    let divider_color = Color::srgb(0.45, 0.45, 0.5);
    let cfg = LayoutTextStyle::new(CTRL_FONT);
    let arrow_cfg = LayoutTextStyle::new(CTRL_ARROW_SIZE);
    let title_cfg = LayoutTextStyle::new(CTRL_TITLE_FONT);
    let row_h = Sizing::fixed(CTRL_ROW_H);
    let dim_color = Color::srgba(0.6, 0.6, 0.6, 0.8);

    let mut builder = LayoutBuilder::new(CTRL_W, CTRL_H);
    builder.with(
        El::new()
            .width(Sizing::FIT)
            .height(Sizing::FIT)
            .padding(Padding::all(0.002))
            .direction(Direction::TopToBottom)
            .child_gap(0.001)
            .background(Color::srgba(0.1, 0.1, 0.12, 0.85))
            .border(Border::all(0.0005, border_color)),
        |b| {
            b.text("controls", title_cfg.with_color(Color::srgb(0.4, 0.5, 0.9)));
            b.with(
                El::new()
                    .width(Sizing::GROW)
                    .height(Sizing::fixed(0.0002))
                    .background(divider_color),
                |_| {},
            );
            b.with(
                El::new()
                    .width(Sizing::FIT)
                    .height(Sizing::FIT)
                    .direction(Direction::LeftToRight)
                    .child_gap(0.0015),
                |b| {
                    // Key column
                    b.with(
                        El::new()
                            .direction(Direction::TopToBottom)
                            .child_align_x(AlignX::Center),
                        |b| {
                            b.with(El::new().height(row_h), |b| {
                                b.text("d", cfg.clone());
                            });
                            b.with(El::new().height(row_h), |b| {
                                b.text("s", cfg.clone());
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
                                b.text("\u{2192}", arrow_cfg.clone().with_color(dim_color));
                            });
                            b.with(El::new().height(row_h), |b| {
                                b.text("\u{2192}", arrow_cfg.clone().with_color(dim_color));
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
                                b.text("toggle debug", cfg.clone());
                            });
                            b.with(El::new().height(row_h), |b| {
                                b.text("cycle size", cfg.clone());
                            });
                        },
                    );
                },
            );
        },
    );
    builder.build()
}

fn build_rows(
    dynamic: &DynamicRows,
    sizing: &PanelSizing,
    renderer: &str,
) -> Vec<(String, String)> {
    vec![
        ("panel size:".to_string(), sizing.label().to_string()),
        ("layout units:".to_string(), sizing.layout_units_label()),
        ("renderer:".to_string(), renderer.to_string()),
        ("radius:".to_string(), dynamic.radius.clone()),
        ("fps:".to_string(), dynamic.fps.clone()),
        ("frame ms:".to_string(), dynamic.frame_ms.clone()),
    ]
}

/// Replaces ASCII spaces with non-breaking spaces (`\u{00a0}`) for Clay text.
fn spaces_to_nbsp(text: &str) -> String { text.replace(' ', "\u{00a0}") }

fn build_clay_rows(dynamic: &DynamicRows, sizing: &PanelSizing) -> Vec<(String, String)> {
    build_rows(dynamic, sizing, CLAY_RENDERER)
        .into_iter()
        .map(|(l, v)| (spaces_to_nbsp(&l), spaces_to_nbsp(&v)))
        .collect()
}

// ── Diegetic panel update ────────────────────────────────────────────────────

/// Updates the diegetic panel's tree when dynamic data or sizing changes.
fn rebuild_diegetic_panel(
    dynamic: Res<DynamicRows>,
    sizing: Res<PanelSizing>,
    mut panels: Query<&mut DiegeticPanel>,
) {
    if !dynamic.is_changed() && !sizing.is_changed() {
        return;
    }
    let rows = build_rows(&dynamic, &sizing, DIEGETIC_RENDERER);
    for mut panel in &mut panels {
        panel.tree = build_diegetic_tree(&rows, sizing.world_size);
    }
}

// ── Clay layout + WorldText spawning ─────────────────────────────────────────

/// Recomputes the clay layout when dynamic data or sizing changes.
fn rebuild_clay_layout(
    dynamic: Res<DynamicRows>,
    measurer: Res<DiegeticTextMeasurer>,
    sizing: Res<PanelSizing>,
    mut clay_result: ResMut<ClayLayoutResult>,
) {
    if !dynamic.is_changed() && !sizing.is_changed() {
        return;
    }
    let clay_rows = build_clay_rows(&dynamic, &sizing);
    clay_result.0 = compute_clay_layout(&clay_rows, &measurer, sizing.layout_size);
}

/// Spawns `WorldText` entities for clay text at positions computed from the
/// clay layout. Each text rect becomes a `WorldText` child of the clay panel.
fn spawn_clay_text(
    mut commands: Commands,
    old_text: Query<Entity, With<ClayTextEntity>>,
    clay_panels: Query<&GlobalTransform, With<ClayPanelMarker>>,
    clay_result: Res<ClayLayoutResult>,
    sizing: Res<PanelSizing>,
) {
    if !clay_result.is_changed() {
        return;
    }

    // Despawn previous clay text entities.
    for entity in &old_text {
        commands.entity(entity).despawn();
    }

    let scale = sizing.scale();
    let half_w = sizing.world_size * 0.5;
    let half_h = sizing.world_size * PANEL_ASPECT * 0.5;

    // Scale factor: WorldText uses 0.01, panel uses `scale`.
    let text_entity_scale = scale / WORLD_TEXT_SCALE;

    for panel_gt in &clay_panels {
        for rect in &clay_result.0 {
            let (text, font_size) = match &rect.kind {
                ClayRectKind::Text(text, font_size) => (text.as_str(), *font_size),
                _ => continue,
            };

            // Convert layout coords to panel-local world coords.
            // Layout: top-left origin, Y-down.
            // Panel-local: center origin, Y-up.
            let local_x = rect.x.mul_add(scale, -half_w);
            let local_y = (-rect.y).mul_add(scale, half_h);

            // Transform to world space.
            let world_pos = panel_gt.transform_point(Vec3::new(local_x, local_y, 0.001));

            commands.spawn((
                ClayTextEntity,
                WorldText::new(text),
                WorldTextStyle::new(font_size * WORLD_TEXT_SCALE)
                    .with_anchor(bevy_diegetic::Anchor::TopLeft),
                Transform::from_translation(world_pos).with_scale(Vec3::splat(text_entity_scale)),
            ));
        }
    }
}

// ── Diegetic tree builder ────────────────────────────────────────────────────

/// Builds the diegetic layout tree. Returns a `LayoutTree`.
/// The plugin handles layout computation and text rendering.
///
/// `world_size` is the panel width in meters (layout coordinates = meters).
fn build_diegetic_tree(rows: &[(String, String)], world_size: f32) -> LayoutTree {
    let world_height = world_size * PANEL_ASPECT;
    // Scale factor: spatial values are proportional to world_size.
    let s = world_size / 160.0;
    let mut builder = LayoutBuilder::with_root(
        El::new()
            .width(Sizing::fixed(world_size))
            .height(Sizing::fixed(world_height))
            .padding(Padding::all(8.0 * s))
            .direction(Direction::TopToBottom)
            .background(Color::srgb_u8(180, 96, 122)),
    );

    // Inset frame.
    builder.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::GROW)
            .padding(Padding::all(5.0 * s))
            .direction(Direction::TopToBottom)
            .child_gap(5.0 * s)
            .border(Border::all(5.0 * s, Color::srgb_u8(255, 255, 255)))
            .background(Color::srgb_u8(56, 16, 24)),
        |b| {
            build_diegetic_header(b, s);
            build_diegetic_divider(b, s);
            build_diegetic_body(b, rows, s);
        },
    );

    builder.build()
}

/// Builds the header container with title and subtitle for the diegetic panel.
fn build_diegetic_header(b: &mut LayoutBuilder, s: f32) {
    b.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::grow_range(7.0 * s, HEADER_HEIGHT * s))
            .padding(Padding::new(5.0 * s, 5.0 * s, 4.0 * s, 4.0 * s))
            .child_align_y(AlignY::Center)
            .background(Color::srgb_u8(52, 98, 90)),
        |b| {
            b.with(
                El::new()
                    .width(Sizing::GROW)
                    .height(Sizing::FIT)
                    .direction(Direction::LeftToRight),
                |b| {
                    // Title slot.
                    b.with(El::new().width(Sizing::FIT).height(Sizing::GROW), |b| {
                        b.text("STATUS", LayoutTextStyle::new(FONT_SIZE));
                    });
                    // Grow spacer.
                    b.with(
                        El::new().width(Sizing::GROW).height(Sizing::fixed(1.0 * s)),
                        |_| {},
                    );
                    // Subtitle slot.
                    b.with(
                        El::new()
                            .width(Sizing::FIT)
                            .height(Sizing::GROW)
                            .child_align_x(AlignX::Right),
                        |b| {
                            b.text("DIEGETIC LAYOUT", LayoutTextStyle::new(SUBTITLE_FONT_SIZE));
                        },
                    );
                },
            );
        },
    );
}

/// Builds the accent divider bar.
fn build_diegetic_divider(b: &mut LayoutBuilder, s: f32) {
    b.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::fixed(DIVIDER_HEIGHT * s))
            .background(Color::srgb_u8(74, 196, 172)),
        |_| {},
    );
}

/// Builds the body section containing key-value rows and wrap text.
fn build_diegetic_body(b: &mut LayoutBuilder, rows: &[(String, String)], s: f32) {
    b.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::GROW)
            .background(Color::srgb_u8(22, 28, 34)),
        |b| {
            b.with(
                El::new()
                    .width(Sizing::GROW)
                    .padding(Padding::all(5.0 * s))
                    .direction(Direction::TopToBottom)
                    .child_gap(2.0 * s),
                |b| {
                    for (label, value) in rows {
                        b.with(
                            El::new()
                                .width(Sizing::GROW)
                                .height(Sizing::FIT)
                                .direction(Direction::LeftToRight),
                            |b| {
                                b.text(label, LayoutTextStyle::new(FONT_SIZE));
                                b.with(
                                    El::new().width(Sizing::GROW).height(Sizing::fixed(1.0 * s)),
                                    |_| {},
                                );
                                b.text(value, LayoutTextStyle::new(FONT_SIZE));
                            },
                        );
                    }

                    // Spacer.
                    b.with(
                        El::new().width(Sizing::GROW).height(Sizing::fixed(4.0 * s)),
                        |_| {},
                    );

                    // Word-wrap cell.
                    b.text(WRAP_TEXT, LayoutTextStyle::new(FONT_SIZE));
                },
            );
        },
    );
}

// ── Clay layout ──────────────────────────────────────────────────────────────

fn compute_clay_layout(
    rows: &[(String, String)],
    measurer: &DiegeticTextMeasurer,
    layout_size: f32,
) -> Vec<ClayRect> {
    let layout_height = layout_size * PANEL_ASPECT;
    let mut clay = Clay::new((layout_size, layout_height).into());
    let measure_fn = Arc::clone(&measurer.measure_fn);
    clay.set_measure_text_function_user_data(measure_fn, clay_measure_with_parley);

    let mut layout = clay.begin::<(), ()>();

    layout.with(
        &Declaration::new()
            .layout()
            .width(fixed!(layout_size))
            .height(fixed!(layout_height))
            .padding(clay_layout::layout::Padding::all(8))
            .direction(LayoutDirection::TopToBottom)
            .end()
            .background_color((180, 96, 122).into()),
        |clay| {
            // Inset frame.
            clay.with(
                &Declaration::new()
                    .layout()
                    .width(grow!())
                    .height(grow!())
                    .padding(clay_layout::layout::Padding::all(5))
                    .direction(LayoutDirection::TopToBottom)
                    .child_gap(5)
                    .end()
                    .border()
                    .all_directions(5)
                    .color(clay_layout::Color::u_rgb(255, 255, 255))
                    .end()
                    .background_color((56, 16, 24).into()),
                |clay| {
                    build_clay_header(clay);
                    build_clay_divider(clay);
                    build_clay_body(clay, rows);
                },
            );
        },
    );

    collect_clay_rects(layout)
}

/// Builds the clay header container with title and subtitle.
fn build_clay_header<'a>(clay: &mut ClayLayoutScope<'a, 'a, (), ()>) {
    clay.with(
        &Declaration::new()
            .layout()
            .width(grow!())
            .height(grow!(FONT_SIZE, HEADER_HEIGHT))
            .padding(clay_layout::layout::Padding::new(5, 5, 4, 4))
            .child_alignment(Alignment::new(
                LayoutAlignmentX::Left,
                LayoutAlignmentY::Center,
            ))
            .end()
            .background_color((52, 98, 90).into()),
        |clay| {
            clay.with(
                &Declaration::new()
                    .layout()
                    .width(grow!())
                    .height(fit!())
                    .direction(LayoutDirection::LeftToRight)
                    .end(),
                |clay| {
                    // Title slot.
                    clay.with(
                        &Declaration::new()
                            .layout()
                            .width(fit!())
                            .height(grow!())
                            .end(),
                        |clay| {
                            clay.text(
                                "STATUS",
                                clay_layout::text::TextConfig::new()
                                    .font_size(CLAY_FONT_SIZE)
                                    .wrap_mode(TextElementConfigWrapMode::None)
                                    .end(),
                            );
                        },
                    );
                    // Grow spacer.
                    clay.with(
                        &Declaration::new()
                            .layout()
                            .width(grow!())
                            .height(fixed!(1.0))
                            .end(),
                        |_| {},
                    );
                    // Subtitle slot.
                    clay.with(
                        &Declaration::new()
                            .layout()
                            .width(fit!())
                            .height(grow!())
                            .child_alignment(Alignment::new(
                                LayoutAlignmentX::Right,
                                LayoutAlignmentY::Top,
                            ))
                            .end(),
                        |clay| {
                            clay.text(
                                "CLAY LAYOUT",
                                clay_layout::text::TextConfig::new()
                                    .font_size(CLAY_SUBTITLE_FONT_SIZE)
                                    .wrap_mode(TextElementConfigWrapMode::None)
                                    .end(),
                            );
                        },
                    );
                },
            );
        },
    );
}

/// Builds the clay accent divider bar.
fn build_clay_divider<'a>(clay: &mut ClayLayoutScope<'a, 'a, (), ()>) {
    clay.with(
        &Declaration::new()
            .layout()
            .width(grow!())
            .height(fixed!(DIVIDER_HEIGHT))
            .end()
            .background_color((74, 196, 172).into()),
        |_| {},
    );
}

/// Builds the clay body section containing key-value rows and wrap text.
fn build_clay_body<'a>(clay: &mut ClayLayoutScope<'a, 'a, (), ()>, rows: &[(String, String)]) {
    clay.with(
        &Declaration::new()
            .layout()
            .width(grow!())
            .height(grow!())
            .end()
            .background_color((22, 28, 34).into()),
        |clay| {
            clay.with(
                &Declaration::new()
                    .layout()
                    .width(grow!())
                    .padding(clay_layout::layout::Padding::all(5))
                    .direction(LayoutDirection::TopToBottom)
                    .child_gap(2)
                    .end(),
                |clay| {
                    for (label, value) in rows {
                        clay.with(
                            &Declaration::new()
                                .layout()
                                .width(grow!())
                                .height(fit!())
                                .direction(LayoutDirection::LeftToRight)
                                .end(),
                            |clay| {
                                clay.text(
                                    label,
                                    clay_layout::text::TextConfig::new()
                                        .font_size(CLAY_FONT_SIZE)
                                        .wrap_mode(TextElementConfigWrapMode::None)
                                        .end(),
                                );
                                clay.with(
                                    &Declaration::new()
                                        .layout()
                                        .width(grow!())
                                        .height(fixed!(1.0))
                                        .end(),
                                    |_| {},
                                );
                                clay.text(
                                    value,
                                    clay_layout::text::TextConfig::new()
                                        .font_size(CLAY_FONT_SIZE)
                                        .wrap_mode(TextElementConfigWrapMode::None)
                                        .end(),
                                );
                            },
                        );
                    }

                    // Spacer.
                    clay.with(
                        &Declaration::new()
                            .layout()
                            .width(grow!())
                            .height(fixed!(4.0))
                            .end(),
                        |_| {},
                    );

                    // Word-wrap cell.
                    clay.text(
                        WRAP_TEXT,
                        clay_layout::text::TextConfig::new()
                            .font_size(CLAY_FONT_SIZE)
                            .end(),
                    );
                },
            );
        },
    );
}

/// Collects render commands from a finished clay layout into `ClayRect` entries.
fn collect_clay_rects<'a>(mut layout: ClayLayoutScope<'a, 'a, (), ()>) -> Vec<ClayRect> {
    let mut rects = Vec::new();
    for cmd in layout.end() {
        let kind = match cmd.config {
            RenderCommandConfig::Rectangle(_) => ClayRectKind::Rectangle,
            RenderCommandConfig::Text(t) => {
                ClayRectKind::Text((*t.text).to_string(), f32::from(t.font_size))
            },
            RenderCommandConfig::Border(_) => ClayRectKind::Border,
            _ => continue,
        };
        rects.push(ClayRect {
            x: cmd.bounding_box.x,
            y: cmd.bounding_box.y,
            width: cmd.bounding_box.width,
            height: cmd.bounding_box.height,
            kind,
        });
    }
    rects
}

// ── Gizmo rendering ──────────────────────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
fn draw_rect_on_panel(
    gizmos: &mut Gizmos<impl GizmoConfigGroup>,
    panel_transform: &GlobalTransform,
    bounds_x: f32,
    bounds_y: f32,
    bounds_w: f32,
    bounds_h: f32,
    color: Color,
    sizing: &PanelSizing,
) {
    let half_w = sizing.world_size * 0.5;
    let half_h = sizing.world_size * PANEL_ASPECT * 0.5;
    let scale = sizing.scale();

    let left = bounds_x.mul_add(scale, -half_w);
    let right = (bounds_x + bounds_w).mul_add(scale, -half_w);
    let top = (-bounds_y).mul_add(scale, half_h);
    let bottom = (-(bounds_y + bounds_h)).mul_add(scale, half_h);

    let tl = panel_transform.transform_point(Vec3::new(left, top, 0.0));
    let tr = panel_transform.transform_point(Vec3::new(right, top, 0.0));
    let br = panel_transform.transform_point(Vec3::new(right, bottom, 0.0));
    let bl = panel_transform.transform_point(Vec3::new(left, bottom, 0.0));

    gizmos.line(tl, tr, color);
    gizmos.line(tr, br, color);
    gizmos.line(br, bl, color);
    gizmos.line(bl, tl, color);
}

fn toggle_text_debug(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut debug: ResMut<ShowTextDebug>,
    mut show_text: ResMut<ShowTextGizmos>,
) {
    if keyboard.just_pressed(KeyCode::KeyD) {
        debug.0 = !debug.0;
        show_text.0 = debug.0;
    }
}

fn draw_clay_gizmos(
    mut gizmos: Gizmos<ClayGizmoGroup>,
    panels: Query<&GlobalTransform, With<ClayPanelMarker>>,
    result: Res<ClayLayoutResult>,
    debug: Res<ShowTextDebug>,
    sizing: Res<PanelSizing>,
) {
    for panel_transform in &panels {
        for rect in &result.0 {
            let color = match &rect.kind {
                ClayRectKind::Rectangle => Color::from(css::MAGENTA),
                ClayRectKind::Text(_, _) => {
                    if !debug.0 {
                        continue;
                    }
                    Color::from(css::LIME)
                },
                ClayRectKind::Border => Color::from(css::ORANGE_RED),
            };
            draw_rect_on_panel(
                &mut gizmos,
                panel_transform,
                rect.x,
                rect.y,
                rect.width,
                rect.height,
                color,
                &sizing,
            );
        }
    }
}
