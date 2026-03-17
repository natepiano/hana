//! Side-by-side layout comparison example.
//!
//! Renders the same status panel layout using both `clay-layout` (C FFI) and
//! `bevy_diegetic` (pure Rust), side by side as 3D gizmo wireframes with
//! real 3D text rendered via `bevy_rich_text3d`. Orbit around them with
//! `bevy_panorbit_camera`.
//!
//! Text is measured by `bevy_rich_text3d` via `Text3dDimensionOut` and fed back
//! into both layout engines through a measurement cache, matching hana's approach.
//!
//! Run with:
//! ```sh
//! cargo run --example side_by_side
//! ```

#![allow(clippy::cast_precision_loss)]
#![allow(clippy::needless_borrow)]

use std::collections::HashMap;
use std::hash::DefaultHasher;
use std::hash::Hash;
use std::hash::Hasher;
use std::sync::Arc;

use bevy::color::Color;
use bevy::color::palettes::css;
use bevy::diagnostic::DiagnosticsStore;
use bevy::diagnostic::FrameTimeDiagnosticsPlugin;
use bevy::prelude::*;
use bevy_brp_extras::BrpExtrasPlugin;
use bevy_diegetic::AlignX;
use bevy_diegetic::AlignY;
use bevy_diegetic::Border;
use bevy_diegetic::BoundingBox;
use bevy_diegetic::Direction;
use bevy_diegetic::El;
use bevy_diegetic::LayoutBuilder;
use bevy_diegetic::LayoutEngine;
use bevy_diegetic::LayoutResult;
use bevy_diegetic::MeasureTextFn;
use bevy_diegetic::Padding;
use bevy_diegetic::RenderCommandKind;
use bevy_diegetic::Sizing;
use bevy_diegetic::TextConfig;
use bevy_diegetic::TextDimensions;
use bevy_diegetic::TextMeasure;
use bevy_panorbit_camera::PanOrbitCamera;
use bevy_panorbit_camera::PanOrbitCameraPlugin;
use bevy_panorbit_camera::TrackpadBehavior;
use bevy_rich_text3d::LoadFonts;
use bevy_rich_text3d::Text3d;
use bevy_rich_text3d::Text3dDimensionOut;
use bevy_rich_text3d::Text3dPlugin;
use bevy_rich_text3d::Text3dSet;
use bevy_rich_text3d::Text3dStyling;
use bevy_rich_text3d::TextAlign;
use bevy_rich_text3d::TextAnchor;
use bevy_rich_text3d::TextAtlas;
use bevy_window_manager::WindowManagerPlugin;
use clay_layout::Clay;
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

const PANEL_FACE_OFFSET: f32 = 0.001;

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
const FONT_SIZE: f32 = 7.0;
const SUBTITLE_FONT_SIZE: f32 = 4.0;
const CLAY_FONT_SIZE: u16 = 7;
const CLAY_SUBTITLE_FONT_SIZE: u16 = 4;
const CHAR_WIDTH_FACTOR: f32 = 0.6;
const RASTER_SIZE: f32 = 64.0;
const MIN_MEASUREMENT_THRESHOLD: f32 = 0.1;
const FONT_FAMILY: &str = "JetBrains Mono";

const STATIC_ROWS: &[(&str, &str)] = &[("ux:", "12")];
const DYNAMIC_UPDATE_INTERVAL: f32 = 1.0;
const WRAP_TEXT: &str = "Lorem ipsum dolor sit amet, consectetur adipiscing elit";

// ── Gizmo groups ─────────────────────────────────────────────────────────────

#[derive(Default, Reflect, GizmoConfigGroup)]
struct ClayGizmoGroup;

#[derive(Default, Reflect, GizmoConfigGroup)]
struct DiegeticGizmoGroup;

// ── Marker components ────────────────────────────────────────────────────────

#[derive(Component)]
struct ClayPanelMarker;

#[derive(Component)]
struct DiegeticPanelMarker;

/// Marker for text entities that get despawned and respawned each frame.
#[derive(Component)]
struct LayoutTextEntity;

/// Metadata stored on each rendered text entity for the measurement cache.
#[derive(Component)]
struct RenderedTextMeta {
    text:        String,
    font_size:   f32,
    world_scale: f32,
}

// ── Measurement cache ────────────────────────────────────────────────────────

#[derive(Clone, Eq, PartialEq, Hash)]
struct MeasurementKey {
    text_hash: u64,
    font_size: u16,
}

impl MeasurementKey {
    fn new(text: &str, font_size: u16) -> Self {
        let mut hasher = DefaultHasher::new();
        text.hash(&mut hasher);
        Self {
            text_hash: hasher.finish(),
            font_size,
        }
    }
}

#[derive(Resource, Default)]
struct TextMeasurementCache {
    measurements: HashMap<MeasurementKey, TextDimensions>,
}

// ── Stored layout results ────────────────────────────────────────────────────

#[derive(Resource, Default)]
struct DiegeticLayoutResult(Option<LayoutResult>);

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
    Text(String, u16),
    Border,
}

/// Shared text material handle.
#[derive(Resource)]
struct TextMaterialHandle(Handle<StandardMaterial>);

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
        // Compute the world size that makes the *largest* preset fill the view.
        // Smaller presets then get proportionally smaller panels at the same
        // world-units-per-layout-unit scale, so content stays the same physical
        // size and only the panel boundary shrinks.
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
        .add_plugins(BrpExtrasPlugin::default())
        .insert_resource(LoadFonts {
            font_directories: vec![
                concat!(env!("CARGO_MANIFEST_DIR"), "/assets/fonts").to_string(),
            ],
            ..default()
        })
        .add_plugins(Text3dPlugin::default())
        .add_plugins(PanOrbitCameraPlugin)
        .add_plugins(WindowManagerPlugin)
        .init_gizmo_group::<ClayGizmoGroup>()
        .init_gizmo_group::<DiegeticGizmoGroup>()
        .init_resource::<DiegeticLayoutResult>()
        .init_resource::<ClayLayoutResult>()
        .init_resource::<TextMeasurementCache>()
        .init_resource::<DynamicRows>()
        .init_resource::<PanelSizing>()
        .init_resource::<ShowTextDebug>()
        .add_systems(Startup, setup)
        .add_systems(
            Update,
            (
                cycle_panel_size,
                update_dynamic_rows,
                rebuild_layouts
                    .after(update_dynamic_rows)
                    .after(cycle_panel_size),
                spawn_text_entities.after(rebuild_layouts),
                toggle_text_debug,
                draw_clay_gizmos,
                draw_diegetic_gizmos,
            ),
        )
        .add_systems(PostUpdate, update_measurement_cache.after(Text3dSet))
        .run();
}

// ── Text measurement ─────────────────────────────────────────────────────────

fn fallback_measure(text: &str, measure: &TextMeasure) -> TextDimensions {
    let line_height = measure.effective_line_height();
    let char_width = measure.size * CHAR_WIDTH_FACTOR;
    let mut max_line_width: f32 = 0.0;
    let mut line_count = 0_u32;
    for line in text.lines() {
        line_count += 1;
        let width = line.chars().count() as f32 * char_width;
        max_line_width = max_line_width.max(width);
    }
    if line_count == 0 {
        line_count = 1;
    }
    TextDimensions {
        width:  max_line_width,
        height: line_height * line_count as f32,
    }
}

fn create_measure_fn(cache: &TextMeasurementCache) -> MeasureTextFn {
    let lookup = cache.measurements.clone();
    Arc::new(move |text: &str, measure: &TextMeasure| {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let key = MeasurementKey::new(text, measure.size as u16);
        lookup
            .get(&key)
            .copied()
            .unwrap_or_else(|| fallback_measure(text, measure))
    })
}

fn create_clay_measure_fn(cache: &TextMeasurementCache) -> HashMap<MeasurementKey, TextDimensions> {
    cache.measurements.clone()
}

fn clay_measure_with_cache(
    text: &str,
    config: &clay_layout::text::TextConfig,
    lookup: &mut HashMap<MeasurementKey, TextDimensions>,
) -> Dimensions {
    let key = MeasurementKey::new(text, config.font_size);
    if let Some(cached) = lookup.get(&key) {
        return Dimensions {
            width:  cached.width,
            height: cached.height,
        };
    }
    // Fallback.
    let font_size = f32::from(config.font_size);
    let char_width = font_size * CHAR_WIDTH_FACTOR;
    let line_height = if config.line_height == 0 {
        font_size
    } else {
        f32::from(config.line_height)
    };
    let mut max_line_width: f32 = 0.0;
    let mut line_count = 0_u32;
    for line in text.lines() {
        line_count += 1;
        let width = line.chars().count() as f32 * char_width;
        max_line_width = max_line_width.max(width);
    }
    if line_count == 0 {
        line_count = 1;
    }
    Dimensions {
        width:  max_line_width,
        height: line_height * line_count as f32,
    }
}

// ── Measurement cache update ─────────────────────────────────────────────────

fn update_measurement_cache(
    query: Query<
        (&RenderedTextMeta, &Text3dDimensionOut, &Text3dStyling),
        Changed<Text3dDimensionOut>,
    >,
    mut cache: ResMut<TextMeasurementCache>,
    sizing: Res<PanelSizing>,
) {
    let panel_scale = sizing.scale();
    for (meta, dim_out, styling) in &query {
        let raster_size = styling.size;

        // Convert: raster pixels → world units → layout units.
        let layout_width = (dim_out.dimension.x * meta.world_scale / raster_size) / panel_scale;
        let layout_height = (dim_out.dimension.y * meta.world_scale / raster_size) / panel_scale;

        if layout_width < MIN_MEASUREMENT_THRESHOLD || layout_height < MIN_MEASUREMENT_THRESHOLD {
            continue;
        }

        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let key = MeasurementKey::new(&meta.text, meta.font_size as u16);
        cache.measurements.insert(
            key,
            TextDimensions {
                width:  layout_width,
                height: layout_height,
            },
        );
    }
}

// ── Setup ────────────────────────────────────────────────────────────────────

fn setup(
    mut commands: Commands,
    mut materials: ResMut<Assets<StandardMaterial>>,
    sizing: Res<PanelSizing>,
) {
    // Camera.
    let midpoint = Vec3::ZERO;
    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(0.0, 0.0, 3.0).looking_at(midpoint, Vec3::Y),
        PanOrbitCamera {
            focus: midpoint,
            trackpad_behavior: TrackpadBehavior::blender_default(),
            trackpad_pinch_to_zoom_enabled: true,
            ..default()
        },
    ));

    // Panel entities.
    let offset = panel_offset(&sizing);

    commands.spawn((ClayPanelMarker, Transform::from_xyz(-offset, 0.0, 0.0)));

    commands.spawn((DiegeticPanelMarker, Transform::from_xyz(offset, 0.0, 0.0)));

    // Shared text material.
    let text_material = materials.add(StandardMaterial {
        base_color_texture: Some(TextAtlas::DEFAULT_IMAGE.clone()),
        alpha_mode: AlphaMode::Mask(0.5),
        unlit: true,
        cull_mode: None,
        ..default()
    });
    commands.insert_resource(TextMaterialHandle(text_material));

    // Help text overlay.
    commands.spawn((
        Text::new("'D' toggle debug  'S' cycle size"),
        TextFont {
            font_size: 14.0,
            ..default()
        },
        TextColor(Color::srgba(1.0, 1.0, 1.0, 0.6)),
        Node {
            position_type: PositionType::Absolute,
            bottom: Val::Px(12.0),
            left: Val::Px(12.0),
            ..default()
        },
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
    mut clay_panels: Query<&mut Transform, (With<ClayPanelMarker>, Without<DiegeticPanelMarker>)>,
    mut diegetic_panels: Query<
        &mut Transform,
        (With<DiegeticPanelMarker>, Without<ClayPanelMarker>),
    >,
    mut initialized: Local<bool>,
) {
    let pressed = keyboard.just_pressed(KeyCode::KeyS);
    if pressed {
        sizing.cycle();
    }

    // Refit on first frame (camera projection now available) or on key press.
    if !*initialized || pressed {
        *initialized = true;
        refit_panels(&camera, &mut sizing, &mut clay_panels, &mut diegetic_panels);
    }
}

/// Compute visible width at z=0 from the camera and refit panels.
#[allow(clippy::too_many_arguments)]
fn refit_panels(
    camera: &Query<(&GlobalTransform, &Projection)>,
    sizing: &mut PanelSizing,
    clay_panels: &mut Query<&mut Transform, (With<ClayPanelMarker>, Without<DiegeticPanelMarker>)>,
    diegetic_panels: &mut Query<
        &mut Transform,
        (With<DiegeticPanelMarker>, Without<ClayPanelMarker>),
    >,
) {
    if let Ok((gt, Projection::Perspective(persp))) = camera.single() {
        let distance = gt.translation().z.abs();
        let half_height = distance * (persp.fov * 0.5).tan();
        let visible_height = half_height * 2.0;
        let visible_width = visible_height * persp.aspect_ratio;
        sizing.fit_to_view(visible_width, visible_height);
    }
    let offset = panel_offset(sizing);
    for mut t in clay_panels {
        t.translation.x = -offset;
    }
    for mut t in diegetic_panels {
        t.translation.x = offset;
    }
}

// ── Dynamic row updates ──────────────────────────────────────────────────────

fn update_dynamic_rows(
    time: Res<Time>,
    diagnostics: Res<DiagnosticsStore>,
    camera: Query<&PanOrbitCamera>,
    mut dynamic: ResMut<DynamicRows>,
) {
    // Radius updates every frame (follows camera orbit).
    if let Ok(cam) = camera.single() {
        dynamic.radius = format!("{:.1}", cam.radius.unwrap_or(3.0));
    }

    // FPS and frame ms update on a timer.
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

fn build_rows(dynamic: &DynamicRows, sizing: &PanelSizing) -> Vec<(String, String)> {
    let mut rows = vec![
        ("panel size:".to_string(), sizing.label().to_string()),
        ("layout units:".to_string(), sizing.layout_units_label()),
    ];

    for (l, v) in STATIC_ROWS {
        rows.push(((*l).to_string(), (*v).to_string()));
    }

    rows.push(("radius:".to_string(), dynamic.radius.clone()));
    rows.push(("fps:".to_string(), dynamic.fps.clone()));
    rows.push(("frame ms:".to_string(), dynamic.frame_ms.clone()));

    rows
}

/// Replaces ASCII spaces with non-breaking spaces (`\u{00a0}`) for Clay text.
///
/// Clay tokenizes text at word boundaries (spaces) and measures each token via
/// the measurement callback. `bevy_rich_text3d` produces near-zero
/// `Text3dDimensionOut` for space-only text (no visible glyphs), so the callback
/// can never return an accurate width for the space token. Non-breaking spaces
/// prevent Clay from splitting the text, so the full string is measured as one
/// piece — giving accurate widths from cosmic-text's real advance metrics.
fn spaces_to_nbsp(text: &str) -> String { text.replace(' ', "\u{00a0}") }

fn build_clay_rows(dynamic: &DynamicRows, sizing: &PanelSizing) -> Vec<(String, String)> {
    build_rows(dynamic, sizing)
        .into_iter()
        .map(|(l, v)| (spaces_to_nbsp(&l), spaces_to_nbsp(&v)))
        .collect()
}

// ── Rebuild layouts each frame ───────────────────────────────────────────────

fn rebuild_layouts(
    dynamic: Res<DynamicRows>,
    cache: Res<TextMeasurementCache>,
    sizing: Res<PanelSizing>,
    mut diegetic_result: ResMut<DiegeticLayoutResult>,
    mut clay_result: ResMut<ClayLayoutResult>,
) {
    let rows = build_rows(&dynamic, &sizing);
    let clay_rows = build_clay_rows(&dynamic, &sizing);
    let layout_size = sizing.layout_size;
    diegetic_result.0 = Some(compute_diegetic_layout(&rows, &cache, layout_size));
    clay_result.0 = compute_clay_layout(&clay_rows, &cache, layout_size);
}

// ── Spawn 3D text entities from layout results ──────────────────────────────

fn layout_to_face_position(bounds: &BoundingBox, sizing: &PanelSizing) -> Vec3 {
    let scale = sizing.scale();
    let half_w = sizing.world_size * 0.5;
    let half_h = sizing.world_size * PANEL_ASPECT * 0.5;
    let center_x = bounds.width.mul_add(0.5, bounds.x).mul_add(scale, -half_w);
    let center_y = bounds.height.mul_add(0.5, bounds.y).mul_add(-scale, half_h);
    Vec3::new(center_x, center_y, PANEL_FACE_OFFSET)
}

#[allow(clippy::too_many_lines, clippy::too_many_arguments)]
fn spawn_text_entities(
    mut commands: Commands,
    old_text: Query<Entity, With<LayoutTextEntity>>,
    diegetic_panels: Query<(&GlobalTransform, Entity), With<DiegeticPanelMarker>>,
    clay_panels: Query<(&GlobalTransform, Entity), With<ClayPanelMarker>>,
    diegetic_result: Res<DiegeticLayoutResult>,
    clay_result: Res<ClayLayoutResult>,
    text_material: Res<TextMaterialHandle>,
    sizing: Res<PanelSizing>,
) {
    // Despawn previous frame's text entities.
    for entity in &old_text {
        commands.entity(entity).despawn();
    }

    let material = MeshMaterial3d(text_material.0.clone());

    // Spawn diegetic text.
    if let Some(result) = &diegetic_result.0 {
        for (panel_gt, _) in &diegetic_panels {
            for cmd in &result.commands {
                let (text, font_size) = match &cmd.kind {
                    RenderCommandKind::Text { text, config } => (text.clone(), config.size()),
                    _ => continue,
                };

                let world_scale = font_size * sizing.scale();
                let local_pos = layout_to_face_position(&cmd.bounds, &sizing);
                let world_pos = panel_gt.transform_point(local_pos);

                commands.spawn((
                    LayoutTextEntity,
                    RenderedTextMeta {
                        text: text.clone(),
                        font_size,
                        world_scale,
                    },
                    Text3d::new(text),
                    Text3dStyling {
                        font: FONT_FAMILY.into(),
                        size: RASTER_SIZE,
                        color: bevy::color::Srgba::new(0.85, 0.95, 0.55, 1.0),
                        align: TextAlign::Left,
                        anchor: TextAnchor::CENTER,
                        world_scale: Some(Vec2::splat(world_scale)),
                        layer_offset: 0.0015,
                        ..default()
                    },
                    Transform::from_translation(world_pos),
                    Mesh3d::default(),
                    material.clone(),
                ));
            }
        }
    }

    // Spawn clay text.
    for (panel_gt, _) in &clay_panels {
        for rect in &clay_result.0 {
            let (text, font_size) = match &rect.kind {
                ClayRectKind::Text(text, font_size) => (text.clone(), f32::from(*font_size)),
                _ => continue,
            };

            let world_scale = font_size * sizing.scale();
            let bounds = BoundingBox {
                x:      rect.x,
                y:      rect.y,
                width:  rect.width,
                height: rect.height,
            };
            let local_pos = layout_to_face_position(&bounds, &sizing);
            let world_pos = panel_gt.transform_point(local_pos);

            commands.spawn((
                LayoutTextEntity,
                RenderedTextMeta {
                    text: text.clone(),
                    font_size,
                    world_scale,
                },
                Text3d::new(text),
                Text3dStyling {
                    size: RASTER_SIZE,
                    color: bevy::color::Srgba::new(0.85, 0.95, 0.55, 1.0),
                    align: TextAlign::Left,
                    anchor: TextAnchor::CENTER,
                    world_scale: Some(Vec2::splat(world_scale)),
                    layer_offset: 0.0015,
                    ..default()
                },
                Transform::from_translation(world_pos),
                Mesh3d::default(),
                material.clone(),
            ));
        }
    }
}

// ── Diegetic layout ──────────────────────────────────────────────────────────

#[allow(clippy::too_many_lines)]
fn compute_diegetic_layout(
    rows: &[(String, String)],
    cache: &TextMeasurementCache,
    layout_size: f32,
) -> LayoutResult {
    let layout_height = layout_size * PANEL_ASPECT;
    let mut builder = LayoutBuilder::with_root(
        El::new()
            .width(Sizing::fixed(layout_size))
            .height(Sizing::fixed(layout_height))
            .padding(Padding::all(8.0))
            .direction(Direction::TopToBottom)
            .background(Color::srgb_u8(180, 96, 122)),
    );

    // Inset frame.
    builder.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::GROW)
            .padding(Padding::all(5.0))
            .direction(Direction::TopToBottom)
            .child_gap(5.0)
            .border(Border::all(5.0, Color::srgb_u8(255, 255, 255)))
            .background(Color::srgb_u8(56, 16, 24)),
        |b| {
            // Header container.
            b.with(
                El::new()
                    .width(Sizing::GROW)
                    .height(Sizing::grow_range(FONT_SIZE, HEADER_HEIGHT))
                    .padding(Padding::new(5.0, 5.0, 4.0, 4.0))
                    .child_align_y(AlignY::Center)
                    .background(Color::srgb_u8(52, 98, 90)),
                |b| {
                    // Header text row.
                    b.with(
                        El::new()
                            .width(Sizing::GROW)
                            .height(Sizing::FIT)
                            .direction(Direction::LeftToRight),
                        |b| {
                            // Title slot.
                            b.with(El::new().width(Sizing::FIT).height(Sizing::GROW), |b| {
                                b.text("STATUS", TextConfig::new(FONT_SIZE));
                            });
                            // Grow spacer.
                            b.with(
                                El::new().width(Sizing::GROW).height(Sizing::fixed(1.0)),
                                |_| {},
                            );
                            // Subtitle slot.
                            b.with(
                                El::new()
                                    .width(Sizing::FIT)
                                    .height(Sizing::GROW)
                                    .child_align_x(AlignX::Right),
                                |b| {
                                    b.text("DIEGETIC", TextConfig::new(SUBTITLE_FONT_SIZE));
                                },
                            );
                        },
                    );
                },
            );

            // Accent divider.
            b.with(
                El::new()
                    .width(Sizing::GROW)
                    .height(Sizing::fixed(DIVIDER_HEIGHT))
                    .background(Color::srgb_u8(74, 196, 172)),
                |_| {},
            );

            // Body.
            b.with(
                El::new()
                    .width(Sizing::GROW)
                    .height(Sizing::GROW)
                    .background(Color::srgb_u8(22, 28, 34)),
                |b| {
                    // Content.
                    b.with(
                        El::new()
                            .width(Sizing::GROW)
                            .padding(Padding::all(5.0))
                            .direction(Direction::TopToBottom)
                            .child_gap(2.0),
                        |b| {
                            for (label, value) in rows {
                                b.with(
                                    El::new()
                                        .width(Sizing::GROW)
                                        .height(Sizing::FIT)
                                        .direction(Direction::LeftToRight),
                                    |b| {
                                        b.text(label, TextConfig::new(FONT_SIZE));
                                        b.with(
                                            El::new()
                                                .width(Sizing::GROW)
                                                .height(Sizing::fixed(1.0)),
                                            |_| {},
                                        );
                                        b.text(value, TextConfig::new(FONT_SIZE));
                                    },
                                );
                            }

                            // Spacer.
                            b.with(
                                El::new().width(Sizing::GROW).height(Sizing::fixed(4.0)),
                                |_| {},
                            );

                            // Word-wrap cell: validates that wrapping
                            // measures spaces correctly.
                            b.text(WRAP_TEXT, TextConfig::new(FONT_SIZE));
                        },
                    );
                },
            );
        },
    );

    let tree = builder.build();
    let engine = LayoutEngine::new(create_measure_fn(cache));
    engine.compute(&tree, layout_size, layout_height)
}

// ── Clay layout ──────────────────────────────────────────────────────────────

#[allow(clippy::too_many_lines)]
fn compute_clay_layout(
    rows: &[(String, String)],
    cache: &TextMeasurementCache,
    layout_size: f32,
) -> Vec<ClayRect> {
    let layout_height = layout_size * PANEL_ASPECT;
    let mut clay = Clay::new((layout_size, layout_height).into());
    let lookup = create_clay_measure_fn(cache);
    clay.set_measure_text_function_user_data(lookup, clay_measure_with_cache);

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
                    // Header container.
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
                            // Header text row.
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
                                                "CLAY",
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

                    // Accent divider.
                    clay.with(
                        &Declaration::new()
                            .layout()
                            .width(grow!())
                            .height(fixed!(DIVIDER_HEIGHT))
                            .end()
                            .background_color((74, 196, 172).into()),
                        |_| {},
                    );

                    // Body.
                    clay.with(
                        &Declaration::new()
                            .layout()
                            .width(grow!())
                            .height(grow!())
                            .end()
                            .background_color((22, 28, 34).into()),
                        |clay| {
                            // Content.
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

                                    // Word-wrap cell: uses regular spaces (no
                                    // nbsp) so Clay actually word-wraps. This
                                    // exposes Clay's space-token measurement
                                    // weakness vs diegetic's full-string path.
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
                },
            );
        },
    );

    let mut rects = Vec::new();
    for cmd in layout.end() {
        let kind = match cmd.config {
            RenderCommandConfig::Rectangle(_) => ClayRectKind::Rectangle,
            RenderCommandConfig::Text(t) => ClayRectKind::Text((*t.text).to_string(), t.font_size),
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

fn toggle_text_debug(keyboard: Res<ButtonInput<KeyCode>>, mut debug: ResMut<ShowTextDebug>) {
    if keyboard.just_pressed(KeyCode::KeyD) {
        debug.0 = !debug.0;
    }
}

fn draw_diegetic_gizmos(
    mut gizmos: Gizmos<DiegeticGizmoGroup>,
    panels: Query<&GlobalTransform, With<DiegeticPanelMarker>>,
    result: Res<DiegeticLayoutResult>,
    debug: Res<ShowTextDebug>,
    sizing: Res<PanelSizing>,
) {
    let Some(result) = &result.0 else { return };
    for panel_transform in &panels {
        for cmd in &result.commands {
            let color = match &cmd.kind {
                RenderCommandKind::Rectangle { .. } => Color::from(css::AQUA),
                RenderCommandKind::Text { .. } => {
                    if !debug.0 {
                        continue;
                    }
                    Color::from(css::SPRING_GREEN)
                },
                RenderCommandKind::Border { .. } => Color::from(css::CORAL),
                _ => continue,
            };
            draw_rect_on_panel(
                &mut gizmos,
                panel_transform,
                cmd.bounds.x,
                cmd.bounds.y,
                cmd.bounds.width,
                cmd.bounds.height,
                color,
                &sizing,
            );
        }
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
