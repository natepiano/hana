//! Systems for diegetic UI panel layout computation and debug rendering.

use std::sync::Arc;
use std::sync::Mutex;
use std::sync::PoisonError;
use std::time::Instant;

use bevy::asset::AssetLoadFailedEvent;
use bevy::camera::Camera3d;
use bevy::core_pipeline::oit::OrderIndependentTransparencySettings;
use bevy::prelude::*;
use bevy::render::render_resource::TextureUsages;
use bevy::render::view::Msaa;

use super::components::ComputedDiegeticPanel;
use super::components::DiegeticPanel;
use super::components::DiegeticTextMeasurer;
use super::components::RenderMode;
use super::components::ScreenSpace;
use super::screen_space::ScreenSpaceCamera;
use crate::constants::MILLISECONDS_PER_SECOND;
use crate::layout::Border;
use crate::layout::BoundingBox;
use crate::layout::LayoutEngine;
use crate::layout::MeasureTextFn;
use crate::layout::RenderCommandKind;
use crate::layout::TextMeasure;
use crate::render::ShapedTextCache;
use crate::text::Font;
use crate::text::FontId;
use crate::text::FontLoadFailed;
use crate::text::FontRegistered;
use crate::text::FontRegistry;
use crate::text::FontSource;
use crate::text::MsdfAtlas;

/// Gizmo group for diegetic panel debug wireframes.
///
/// Enable or disable via Bevy's [`GizmoConfigStore`].
///
/// **Note:** This API is provisional. Once panels render real geometry
/// (Phase 4), debug visualization will likely move to a per-panel debug
/// mode rather than a separate gizmo group.
#[derive(Default, Reflect, GizmoConfigGroup)]
pub struct DiegeticPanelGizmoGroup;

/// Ensures scene `Camera3d` entities have OIT enabled for correct
/// transparent panel rendering.
///
/// # Why OIT is needed
///
/// In Geometry mode, each panel element (background, border, text) is a
/// separate transparent mesh. When multiple transparent fragments overlap
/// at a pixel, standard alpha blending composites them in submission
/// order — which can be wrong when the camera moves (distance-based sort
/// flips). OIT stores ALL transparent fragments in a linked list and
/// resolves them by actual depth, producing correct compositing
/// regardless of camera angle.
///
/// # Relationship with `depth_bias` and `oit_depth_offset`
///
/// `depth_bias` on `StandardMaterial` controls the `Transparent3d` sort
/// key (submission order) and wins the GPU depth test for coplanar
/// fragments. `oit_depth_offset` (a custom uniform added to `position.z`
/// before `oit_draw`) separates coplanar layers in the OIT linked list
/// so the resolve pass composites them in the correct painter's order.
/// Pipeline `depth_bias` does NOT affect `in.position.z` seen by
/// `oit_draw`, so the manual offset is required.
///
/// All three mechanisms are complementary:
/// - `depth_bias` → sort order + depth test
/// - `oit_depth_offset` → OIT fragment ordering for coplanar layers
/// - OIT → correct alpha compositing for overlapping transparents
///
/// # Constraints
///
/// - OIT requires `Msaa::Off` — this system disables MSAA if present.
/// - Only activates when at least one panel uses [`RenderMode::Geometry`], since OIT is unnecessary
///   for texture-only panels.
/// - Screen-space overlay cameras are excluded via [`Without<ScreenSpaceCamera>`] — they don't need
///   OIT and adding it corrupts the shared OIT buffer.
pub(super) fn ensure_oit_on_cameras(
    panels: Query<&DiegeticPanel>,
    mut cameras: Query<
        (Entity, &mut Camera3d, Option<&Msaa>),
        (
            Without<OrderIndependentTransparencySettings>,
            Without<ScreenSpaceCamera>,
        ),
    >,
    mut commands: Commands,
) {
    let needs_oit = panels.iter().any(|p| p.render_mode == RenderMode::Geometry);
    if !needs_oit {
        return;
    }

    for (entity, mut camera_3d, msaa) in &mut cameras {
        // Disable MSAA if it's enabled — OIT panics with MSAA > 1.
        if msaa.is_some_and(|m| m.samples() > 1) {
            commands.entity(entity).insert(Msaa::Off);
        }
        // Set depth texture usage BEFORE extraction — Bevy's built-in OIT
        // hook only patches `Added<Camera3d>` and misses cameras that gain
        // OIT later via deferred commands.
        camera_3d.depth_texture_usages.0 |= TextureUsages::TEXTURE_BINDING.bits();
        commands
            .entity(entity)
            .insert(OrderIndependentTransparencySettings::default());
    }
}

/// Enables perspective-scaled line widths on panel debug gizmos.
pub(super) fn configure_panel_gizmos(mut config_store: ResMut<bevy::prelude::GizmoConfigStore>) {
    let (config, _) = config_store.config_mut::<DiegeticPanelGizmoGroup>();
    config.line.perspective = true;
}

/// Creates the empty GPU `Image` for the MSDF atlas at startup and
/// fires [`FontRegistered`] for the embedded default font.
pub(super) fn init_atlas_and_embedded_font(
    mut atlas: ResMut<MsdfAtlas>,
    mut images: ResMut<Assets<Image>>,
    mut commands: Commands,
) {
    atlas.upload_to_gpu(&mut images);
    // Fire FontRegistered for the embedded font so observers see it.
    commands.trigger(FontRegistered {
        id:     FontId::MONOSPACE,
        name:   "JetBrains Mono".to_string(),
        source: FontSource::Embedded,
    });
}

/// Watches for newly loaded [`Font`] assets and registers them with
/// [`FontRegistry`]. Fires [`FontRegistered`] for each successful
/// registration.
pub(super) fn consume_loaded_fonts(
    mut events: MessageReader<AssetEvent<Font>>,
    font_assets: Res<Assets<Font>>,
    mut registry: ResMut<FontRegistry>,
    mut commands: Commands,
) {
    for event in events.read() {
        if let AssetEvent::Added { id } = event
            && let Some(font) = font_assets.get(*id)
        {
            // Skip if already registered (e.g., embedded font).
            if registry.font_id_by_name(font.name()).is_some() {
                continue;
            }
            if let Some(font_id) = registry.register_font(font.name(), font.data()) {
                commands.trigger(FontRegistered {
                    id:     font_id,
                    name:   (*font.name()).to_string(),
                    source: FontSource::Loaded,
                });
            }
        }
    }
}

/// Watches for failed [`Font`] asset loads and fires [`FontLoadFailed`].
pub(super) fn watch_font_failures(
    mut failures: MessageReader<AssetLoadFailedEvent<Font>>,
    mut commands: Commands,
) {
    for event in failures.read() {
        commands.trigger(FontLoadFailed {
            path:  event.path.to_string(),
            error: event.error.to_string(),
        });
    }
}

/// Lightweight timing data for diegetic UI systems.
///
/// These values are updated by the built-in layout and text extraction systems
/// so examples and applications can inspect where time is being spent during
/// content-heavy updates.
///
/// **Note:** This API is provisional. Field names and structure are coupled
/// to the current internal system architecture and may change as the
/// library matures. Consider using Bevy's `DiagnosticsStore` for
/// production profiling.
#[derive(Resource, Clone, Debug, Default, Reflect)]
#[reflect(Resource)]
pub struct DiegeticPerfStats {
    /// Duration of the most recent `compute_panel_layouts` run, in milliseconds.
    pub last_compute_ms:                  f32,
    /// Number of panels processed by the most recent layout run.
    pub last_compute_panels:              usize,
    /// Duration of the most recent text extraction run, in milliseconds.
    pub last_text_extract_ms:             f32,
    /// Number of panels processed by the most recent text extraction run.
    pub last_text_extract_panels:         usize,
    /// Time spent shaping text during the most recent panel text extraction.
    pub last_text_shape_ms:               f32,
    /// Time spent in atlas lookups/queueing during the most recent panel text extraction.
    pub last_text_atlas_ms:               f32,
    /// Time spent spawning mesh/material batches during the most recent panel text extraction.
    pub last_text_spawn_ms:               f32,
    /// Number of glyphs newly queued for rasterization during the most recent panel text
    /// extraction.
    pub last_text_queued_glyphs:          usize,
    /// Number of glyphs still pending rasterization during the most recent panel text extraction.
    pub last_text_pending_glyphs:         usize,
    /// Time spent draining async atlas results in the most recent atlas poll.
    pub last_atlas_poll_ms:               f32,
    /// Time spent syncing dirty atlas pages to GPU images in the most recent atlas poll.
    pub last_atlas_sync_ms:               f32,
    /// Number of completed async glyph jobs drained by the most recent atlas poll.
    pub last_atlas_completed_glyphs:      usize,
    /// Number of visible glyphs inserted into atlas pages by the most recent atlas poll.
    pub last_atlas_inserted_glyphs:       usize,
    /// Number of invisible glyph entries cached by the most recent atlas poll.
    pub last_atlas_invisible_glyphs:      usize,
    /// Number of atlas pages added by the most recent atlas poll.
    pub last_atlas_pages_added:           usize,
    /// Number of dirty atlas pages observed before the most recent GPU sync.
    pub last_atlas_dirty_pages:           usize,
    /// Number of glyph raster jobs still in flight after the most recent atlas poll.
    pub last_atlas_in_flight_glyphs:      usize,
    /// Number of glyph raster jobs actively executing at the end of the most recent atlas poll.
    pub last_atlas_active_jobs:           usize,
    /// Peak concurrently executing glyph raster jobs observed so far.
    pub last_atlas_peak_active_jobs:      usize,
    /// Number of distinct worker threads that completed jobs in the most recent atlas poll.
    pub last_atlas_worker_threads:        usize,
    /// Average worker-side glyph raster duration for the most recent drained batch.
    pub last_atlas_avg_raster_ms:         f32,
    /// Maximum worker-side glyph raster duration for the most recent drained batch.
    pub last_atlas_max_raster_ms:         f32,
    /// Highest active-job count reported by any job in the most recent drained batch.
    pub last_atlas_batch_max_active_jobs: usize,
    /// Total number of glyphs currently cached in the atlas.
    pub last_atlas_total_glyphs:          usize,
}

/// Recomputes layout for panels whose [`DiegeticPanel`] component has changed.
///
/// Uses the [`ShapedTextCache`] for measurement: if a text string has already
/// been shaped (by a previous layout or render pass), its dimensions are
/// returned from the cache without calling parley. On cache miss, falls back
/// to the parley-backed [`DiegeticTextMeasurer`].
pub(super) fn compute_panel_layouts(
    panels: Query<(Entity, Ref<DiegeticPanel>)>,
    mut computed_panels: Query<&mut ComputedDiegeticPanel>,
    measurer: Res<DiegeticTextMeasurer>,
    cache: Res<ShapedTextCache>,
    mut perf: ResMut<DiegeticPerfStats>,
    unit_config: Res<super::UnitConfig>,
) {
    // Only process panels where DiegeticPanel actually changed.
    let changed_entities: Vec<Entity> = panels
        .iter()
        .filter(|(_, panel_ref)| panel_ref.is_changed())
        .map(|(entity, _)| entity)
        .collect();

    if changed_entities.is_empty() {
        perf.last_compute_ms = 0.0;
        perf.last_compute_panels = 0;
        return;
    }

    let start = Instant::now();
    let mut panel_count = 0_usize;

    // Wrap the cache in Arc<Mutex<>> so the MeasureTextFn closure can capture it.
    let cache_ref = Arc::new(Mutex::new(cache.clone()));
    let parley_fn = Arc::clone(&measurer.measure_fn);

    let cached_measure: MeasureTextFn = Arc::new(move |text: &str, measure: &TextMeasure| {
        // Check cache first.
        {
            let cache_guard = cache_ref.lock().unwrap_or_else(PoisonError::into_inner);
            if let Some(dims) = cache_guard.get_measurement(text, measure) {
                return dims;
            }
        }
        // Cache miss — measure via parley and write back to cache.
        let dims = parley_fn(text, measure);
        {
            let mut cache_guard = cache_ref.lock().unwrap_or_else(PoisonError::into_inner);
            cache_guard.insert_measurement(text, measure, dims);
        }
        dims
    });

    for entity in &changed_entities {
        let Ok((_, panel_ref)) = panels.get(*entity) else {
            continue;
        };
        let Ok(mut computed) = computed_panels.get_mut(*entity) else {
            continue;
        };
        panel_count += 1;

        let layout_unit = panel_ref.layout_unit;
        let font_unit = panel_ref.font_unit.unwrap_or(unit_config.font);
        let layout_to_pts = layout_unit.to_points();
        let font_to_pts = font_unit.to_points();

        // Pre-scale tree to points so parley always gets reasonable font sizes.
        let scaled_tree = panel_ref.tree.scaled(layout_to_pts, font_to_pts);
        let engine = LayoutEngine::new(Arc::clone(&cached_measure));
        let result = engine.compute(
            &scaled_tree,
            panel_ref.width * layout_to_pts,
            panel_ref.height * layout_to_pts,
            1.0, // tree is already in points — no additional font scaling
        );

        if let Some(bounds) = result.content_bounds() {
            let s = panel_ref.points_to_world(&unit_config);
            computed.set_content_size(bounds.width * s, bounds.height * s);
        }

        computed.set_result(result);
    }

    let compute_ms = start.elapsed().as_secs_f32() * MILLISECONDS_PER_SECOND;
    perf.last_compute_ms = compute_ms;
    perf.last_compute_panels = panel_count;
}

/// Controls whether debug gizmos (text bounding boxes, element outlines)
/// are drawn. Toggle at runtime to debug layout measurement and positioning.
#[derive(Resource, Default)]
pub enum ShowTextGizmos {
    /// Debug gizmos are not drawn (default).
    #[default]
    Hidden,
    /// Debug gizmos are drawn.
    Shown,
}

/// Marker on gizmo entities spawned by the layout gizmo renderer.
#[derive(Component)]
pub(super) struct PanelGizmoChild;

/// Marker on gizmo entities spawned by the debug gizmo renderer.
#[derive(Component)]
pub(super) struct DebugGizmoChild;

/// Approximate pixels-per-meter from the first camera's projection.
fn pixels_per_meter(cameras: &Query<(&Camera, &Projection)>) -> f32 {
    cameras
        .iter()
        .next()
        .and_then(|(cam, proj)| {
            let vp_height = cam.logical_viewport_size()?.y;
            match proj {
                Projection::Perspective(p) => Some(vp_height / (2.0 * (p.fov / 2.0).tan())),
                Projection::Orthographic(o) => Some(vp_height / o.scale),
                Projection::Custom(_) => None,
            }
        })
        .unwrap_or(1000.0)
}

/// Which gizmo system spawned the child.
enum GizmoChildMarker {
    Layout,
    Debug,
}

/// Parameters for spawning a gizmo rectangle on a panel.
struct GizmoRect<'a> {
    bounds:          &'a BoundingBox,
    points_to_world: f32,
    anchor_x:        f32,
    anchor_y:        f32,
    color:           Color,
    line_width:      f32,
    marker:          GizmoChildMarker,
}

/// Spawns a gizmo rectangle child on `panel_entity`.
fn spawn_rect_gizmo(
    commands: &mut Commands,
    panel_entity: Entity,
    gizmo_assets: &mut Assets<GizmoAsset>,
    rect: &GizmoRect<'_>,
) {
    let mut asset = GizmoAsset::default();
    add_rect_to_gizmo(
        &mut asset,
        rect.bounds,
        rect.points_to_world,
        rect.anchor_x,
        rect.anchor_y,
        rect.color,
    );
    let gizmo = Gizmo {
        handle: gizmo_assets.add(asset),
        line_config: GizmoLineConfig {
            width: rect.line_width,
            perspective: false,
            joints: GizmoLineJoint::Round(8),
            ..default()
        },
        ..default()
    };
    match rect.marker {
        GizmoChildMarker::Layout => {
            commands
                .entity(panel_entity)
                .with_child((PanelGizmoChild, gizmo, Transform::IDENTITY));
        },
        GizmoChildMarker::Debug => {
            commands
                .entity(panel_entity)
                .with_child((DebugGizmoChild, gizmo, Transform::IDENTITY));
        },
    }
}

/// Renders layout visuals (backgrounds, borders, between-children
/// dividers) as retained gizmos. This is the production rendering
/// path for panel layout geometry — always active.
pub(super) fn render_layout_gizmos(
    changed_panels: Query<
        (
            Entity,
            &DiegeticPanel,
            &ComputedDiegeticPanel,
            Has<ScreenSpace>,
        ),
        Changed<ComputedDiegeticPanel>,
    >,
    existing_gizmos: Query<(Entity, &ChildOf), With<PanelGizmoChild>>,
    unit_config: Res<super::UnitConfig>,
    cameras: Query<(&Camera, &Projection)>,
    mut gizmo_assets: ResMut<Assets<GizmoAsset>>,
    mut commands: Commands,
) {
    if changed_panels.is_empty() {
        return;
    }

    let screen_pixels_per_meter = pixels_per_meter(&cameras);

    for (panel_entity, panel, computed, is_screen_space) in &changed_panels {
        // Geometry mode renders real meshes — gizmos are redundant.
        if panel.render_mode == RenderMode::Geometry {
            continue;
        }

        let Some(result) = computed.result() else {
            continue;
        };

        let points_to_world = panel.points_to_world(&unit_config);
        for (entity, child_of) in &existing_gizmos {
            if child_of.parent() == panel_entity {
                commands.entity(entity).despawn();
            }
        }

        let (anchor_x, anchor_y) = panel.anchor_offsets(&unit_config);

        let mut border_by_idx: std::collections::HashMap<usize, &Border> =
            std::collections::HashMap::new();
        for cmd in &result.commands {
            if let RenderCommandKind::Border { ref border } = cmd.kind {
                border_by_idx.insert(cmd.element_idx, border);
            }
        }

        for cmd in &result.commands {
            match &cmd.kind {
                RenderCommandKind::Rectangle { color, .. } => {
                    let border = border_by_idx.get(&cmd.element_idx);
                    let (inset_left, inset_right, inset_top, inset_bottom) = border
                        .map_or((0.0, 0.0, 0.0, 0.0), |b| {
                            (b.left.value, b.right.value, b.top.value, b.bottom.value)
                        });
                    let inset_bounds = BoundingBox {
                        x:      cmd.bounds.x + inset_left,
                        y:      cmd.bounds.y + inset_top,
                        width:  (cmd.bounds.width - inset_left - inset_right).max(0.0),
                        height: (cmd.bounds.height - inset_top - inset_bottom).max(0.0),
                    };
                    spawn_rect_gizmo(
                        &mut commands,
                        panel_entity,
                        &mut gizmo_assets,
                        &GizmoRect {
                            bounds: &inset_bounds,
                            points_to_world,
                            anchor_x,
                            anchor_y,
                            color: *color,
                            line_width: 1.0,
                            marker: GizmoChildMarker::Layout,
                        },
                    );
                },
                RenderCommandKind::Border { border } => {
                    let half_left = border.left.value * 0.5;
                    let half_right = border.right.value * 0.5;
                    let half_top = border.top.value * 0.5;
                    let half_bottom = border.bottom.value * 0.5;
                    let has_sides = border.left.value > 0.0
                        || border.right.value > 0.0
                        || border.top.value > 0.0
                        || border.bottom.value > 0.0;
                    if has_sides {
                        let inset_bounds = BoundingBox {
                            x:      cmd.bounds.x + half_left,
                            y:      cmd.bounds.y + half_top,
                            width:  (cmd.bounds.width - half_left - half_right).max(0.0),
                            height: (cmd.bounds.height - half_top - half_bottom).max(0.0),
                        };
                        let avg_border_pts = (border.left.value
                            + border.right.value
                            + border.top.value
                            + border.bottom.value)
                            / 4.0;
                        // Screen-space panels: 1 layout pt = 1 screen px,
                        // so border width in points IS the pixel width.
                        // World-space panels: convert through camera projection.
                        let border_px = if is_screen_space {
                            avg_border_pts.max(1.0)
                        } else {
                            (avg_border_pts * points_to_world * screen_pixels_per_meter).max(1.0)
                        };
                        spawn_rect_gizmo(
                            &mut commands,
                            panel_entity,
                            &mut gizmo_assets,
                            &GizmoRect {
                                bounds: &inset_bounds,
                                points_to_world,
                                anchor_x,
                                anchor_y,
                                color: border.color,
                                line_width: border_px,
                                marker: GizmoChildMarker::Layout,
                            },
                        );
                    }
                    // between_children dividers are emitted as Rectangle
                    // commands by the layout engine — handled above.
                },
                _ => {},
            }
        }
    }
}

/// Renders debug overlays (text bounding boxes, element outlines) as
/// retained gizmos. Only active when [`ShowTextGizmos`] is enabled.
/// Separate from layout gizmos so debug can be toggled independently.
pub(super) fn render_debug_gizmos(
    changed_panels: Query<
        (Entity, &DiegeticPanel, &ComputedDiegeticPanel),
        Changed<ComputedDiegeticPanel>,
    >,
    existing_gizmos: Query<(Entity, &ChildOf), With<DebugGizmoChild>>,
    show_text: Res<ShowTextGizmos>,
    unit_config: Res<super::UnitConfig>,
    mut gizmo_assets: ResMut<Assets<GizmoAsset>>,
    mut commands: Commands,
) {
    if !matches!(*show_text, ShowTextGizmos::Shown) || changed_panels.is_empty() {
        return;
    }

    for (panel_entity, panel, computed) in &changed_panels {
        let Some(result) = computed.result() else {
            continue;
        };

        let points_to_world = panel.points_to_world(&unit_config);
        for (entity, child_of) in &existing_gizmos {
            if child_of.parent() == panel_entity {
                commands.entity(entity).despawn();
            }
        }

        let (anchor_x, anchor_y) = panel.anchor_offsets(&unit_config);

        for cmd in &result.commands {
            if matches!(cmd.kind, RenderCommandKind::Text { .. }) {
                spawn_rect_gizmo(
                    &mut commands,
                    panel_entity,
                    &mut gizmo_assets,
                    &GizmoRect {
                        bounds: &cmd.bounds,
                        points_to_world,
                        anchor_x,
                        anchor_y,
                        color: Color::srgba(0.9, 0.9, 0.2, 0.2),
                        line_width: 1.0,
                        marker: GizmoChildMarker::Debug,
                    },
                );
            }
        }
    }
}

/// Adds a rectangle outline to a `GizmoAsset` in panel-local coordinates.
fn add_rect_to_gizmo(
    asset: &mut GizmoAsset,
    bounds: &BoundingBox,
    scale: f32,
    anchor_x: f32,
    anchor_y: f32,
    color: Color,
) {
    let left = bounds.x.mul_add(scale, -anchor_x);
    let right = (bounds.x + bounds.width).mul_add(scale, -anchor_x);
    let top = (-bounds.y).mul_add(scale, anchor_y);
    let bottom = (-(bounds.y + bounds.height)).mul_add(scale, anchor_y);

    let tl = Vec3::new(left, top, 0.0);
    let tr = Vec3::new(right, top, 0.0);
    let br = Vec3::new(right, bottom, 0.0);
    let bl = Vec3::new(left, bottom, 0.0);

    asset.line(tl, tr, color);
    asset.line(tr, br, color);
    asset.line(br, bl, color);
    asset.line(bl, tl, color);
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use bevy_kana::ToF32;
    use bevy_kana::ToU32;

    use super::*;
    use crate::constants::MONOSPACE_WIDTH_RATIO;
    use crate::layout::Border;
    use crate::layout::Direction;
    use crate::layout::El;
    use crate::layout::LayoutBuilder;
    use crate::layout::LayoutEngine;
    use crate::layout::LayoutTextStyle;
    use crate::layout::LayoutTree;
    use crate::layout::Padding;
    use crate::layout::Sizing;
    use crate::layout::TextDimensions;
    use crate::layout::TextMeasure;

    fn monospace_measure() -> MeasureTextFn {
        Arc::new(|text: &str, measure: &TextMeasure| {
            let char_width = measure.size * MONOSPACE_WIDTH_RATIO;
            let width = char_width * text.len().to_f32();
            TextDimensions {
                width,
                height: measure.size,
                line_height: measure.size,
            }
        })
    }

    // ── Performance timing tests (run with --run-ignored all) ────────

    const PERF_FONT_SIZE: f32 = 7.0;
    const PERF_LAYOUT_WIDTH: f32 = 800.0;
    const PERF_LAYOUT_HEIGHT: f32 = 1200.0;

    fn build_stress_tree(row_count: usize) -> LayoutTree {
        let mut builder = LayoutBuilder::new(PERF_LAYOUT_WIDTH, PERF_LAYOUT_HEIGHT);
        builder.with(
            El::new()
                .width(Sizing::GROW)
                .height(Sizing::FIT)
                .direction(Direction::TopToBottom)
                .child_gap(2.0)
                .padding(Padding::all(4.0))
                .border(Border::all(1.0, bevy::color::Color::WHITE)),
            |b| {
                for i in 0..row_count {
                    b.with(
                        El::new()
                            .width(Sizing::GROW)
                            .height(Sizing::FIT)
                            .direction(Direction::LeftToRight)
                            .child_gap(4.0),
                        |b| {
                            b.text(format!("item {i}:"), LayoutTextStyle::new(PERF_FONT_SIZE));
                            b.with(
                                El::new().width(Sizing::GROW).height(Sizing::fixed(1.0)),
                                |_| {},
                            );
                            b.text("value", LayoutTextStyle::new(PERF_FONT_SIZE));
                        },
                    );
                }
            },
        );
        builder.build()
    }

    fn run_timing(label: &str, iterations: usize, mut f: impl FnMut()) {
        // Warm up.
        for _ in 0..5 {
            f();
        }
        let start = std::time::Instant::now();
        for _ in 0..iterations {
            f();
        }
        let elapsed = start.elapsed();
        let per_iter = elapsed / iterations.to_u32();
        println!(
            "{label}: {per_iter:?} per iteration ({iterations} iterations, {elapsed:?} total)"
        );
    }

    #[test]
    #[ignore = "manual perf benchmark — run with --ignored"]
    fn perf_element_sizes() {
        println!(
            "TextConfig size: {} bytes",
            std::mem::size_of::<LayoutTextStyle>()
        );
    }

    #[test]
    #[ignore = "manual perf benchmark — run with --ignored"]
    fn perf_tree_build() {
        for &rows in &[10, 100, 500, 1000] {
            let iters = if rows <= 100 { 1000 } else { 100 };
            run_timing(&format!("tree_build_{rows}_rows"), iters, || {
                std::hint::black_box(build_stress_tree(rows));
            });
        }
    }

    /// Populates a `LayoutBuilder` with label/value rows inside a vertical container.
    fn populate_label_rows(builder: &mut LayoutBuilder, labels: &[String]) {
        builder.with(
            El::new()
                .width(Sizing::GROW)
                .height(Sizing::FIT)
                .direction(Direction::TopToBottom)
                .child_gap(2.0)
                .padding(Padding::all(4.0))
                .border(Border::all(1.0, bevy::color::Color::WHITE)),
            |b| {
                for label in labels {
                    b.with(
                        El::new()
                            .width(Sizing::GROW)
                            .height(Sizing::FIT)
                            .direction(Direction::LeftToRight)
                            .child_gap(4.0),
                        |b| {
                            b.text(label, LayoutTextStyle::new(PERF_FONT_SIZE));
                            b.with(
                                El::new().width(Sizing::GROW).height(Sizing::fixed(1.0)),
                                |_| {},
                            );
                            b.text("value", LayoutTextStyle::new(PERF_FONT_SIZE));
                        },
                    );
                }
            },
        );
    }

    /// Benchmarks `format!` overhead in isolation.
    fn bench_string_format(rows: usize, iters: usize) {
        run_timing(&format!("string_format_{rows}_rows"), iters, || {
            for i in 0..rows {
                std::hint::black_box(format!("item {i}:"));
            }
        });
    }

    /// Benchmarks tree build + `build()` with pre-built label strings.
    fn bench_tree_build_prebuilt(labels: &[String], rows: usize, iters: usize) {
        run_timing(
            &format!("tree_build_prebuilt_strings_{rows}_rows"),
            iters,
            || {
                let mut builder = LayoutBuilder::new(PERF_LAYOUT_WIDTH, PERF_LAYOUT_HEIGHT);
                populate_label_rows(&mut builder, labels);
                std::hint::black_box(builder.build());
            },
        );
    }

    /// Benchmarks tree construction without the final `build()` call to isolate hash cost.
    fn bench_tree_build_no_hash(labels: &[String], rows: usize, iters: usize) {
        run_timing(&format!("tree_build_no_hash_{rows}_rows"), iters, || {
            let mut builder = LayoutBuilder::new(PERF_LAYOUT_WIDTH, PERF_LAYOUT_HEIGHT);
            populate_label_rows(&mut builder, labels);
            std::hint::black_box(builder);
        });
    }

    /// Benchmarks tree build + `build()` with pre-allocated builder capacity.
    fn bench_tree_build_preallocated(labels: &[String], rows: usize, iters: usize) {
        run_timing(
            &format!("tree_build_preallocated_{rows}_rows"),
            iters,
            || {
                let capacity = rows * 4 + 2;
                let mut builder =
                    LayoutBuilder::with_capacity(PERF_LAYOUT_WIDTH, PERF_LAYOUT_HEIGHT, capacity);
                populate_label_rows(&mut builder, labels);
                std::hint::black_box(builder.build());
            },
        );
    }

    #[test]
    #[ignore = "manual perf benchmark — run with --ignored"]
    fn perf_tree_build_breakdown() {
        let rows = 1000;
        let iters = 100;
        let labels: Vec<String> = (0..rows).map(|i| format!("item {i}:")).collect();

        bench_string_format(rows, iters);
        bench_tree_build_prebuilt(&labels, rows, iters);
        bench_tree_build_no_hash(&labels, rows, iters);
        bench_tree_build_preallocated(&labels, rows, iters);
    }

    #[test]
    #[ignore = "manual perf benchmark — run with --ignored"]
    fn perf_engine_compute() {
        let measure = monospace_measure();
        for &rows in &[10, 100, 500, 1000] {
            let tree = build_stress_tree(rows);
            let engine = LayoutEngine::new(Arc::clone(&measure));
            let iters = if rows <= 100 { 1000 } else { 100 };
            run_timing(&format!("engine_compute_{rows}_rows"), iters, || {
                std::hint::black_box(engine.compute(
                    &tree,
                    PERF_LAYOUT_WIDTH,
                    PERF_LAYOUT_HEIGHT,
                    1.0,
                ));
            });
        }
    }
}
