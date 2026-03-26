//! Systems for diegetic UI panel layout computation and debug rendering.

use std::sync::Arc;
use std::sync::Mutex;
use std::sync::PoisonError;
use std::time::Instant;

use bevy::prelude::*;

use super::components::ComputedDiegeticPanel;
use super::components::DiegeticPanel;
use super::components::DiegeticTextMeasurer;
use crate::layout::Border;
use crate::layout::BoundingBox;
use crate::layout::LayoutEngine;
use crate::layout::MeasureTextFn;
use crate::layout::RenderCommandKind;
use crate::layout::TextMeasure;
use crate::render::ShapedTextCache;

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

    bevy::log::warn!(
        "compute_panel_layouts: {} panels changed (is_added: {:?})",
        changed_entities.len(),
        panels
            .iter()
            .filter(|(_, r)| r.is_changed())
            .map(|(_, r)| r.is_added())
            .collect::<Vec<_>>()
    );
    let start = Instant::now();
    let mut panel_count = 0_usize;

    // Wrap the cache in Arc<Mutex<>> so the MeasureTextFn closure can capture it.
    let cache_ref = Arc::new(Mutex::new(cache.clone()));
    let parley_fn = Arc::clone(&measurer.measure_fn);

    let hits = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let misses = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let hits_clone = Arc::clone(&hits);
    let misses_clone = Arc::clone(&misses);

    let cached_measure: MeasureTextFn = Arc::new(move |text: &str, measure: &TextMeasure| {
        // Check cache first.
        {
            let cache_guard = cache_ref.lock().unwrap_or_else(PoisonError::into_inner);
            if let Some(dims) = cache_guard.get_measurement(text, measure) {
                hits_clone.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                return dims;
            }
        }
        // Cache miss — measure via parley and write back to cache.
        misses_clone.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
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

        let layout_unit = panel_ref.layout_unit.unwrap_or(unit_config.layout);
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

    let compute_ms = start.elapsed().as_secs_f32() * 1000.0;
    let h = hits.load(std::sync::atomic::Ordering::Relaxed);
    let m = misses.load(std::sync::atomic::Ordering::Relaxed);
    bevy::log::warn!(
        "compute_panel_layouts: {compute_ms:.1}ms, {panel_count} panels, {h} cache hits, {m} cache misses"
    );
    perf.last_compute_ms = compute_ms;
    perf.last_compute_panels = panel_count;
}

/// Controls whether text bounding-box gizmos are drawn.
///
/// When `false` (the default), only rectangles and borders are shown.
/// Toggle at runtime to debug text measurement and positioning.
///
/// **Note:** This API is provisional. Once panels render real geometry
/// (Phase 4), debug visualization will likely move to a per-panel debug
/// mode rather than a global resource.
#[derive(Resource, Default)]
pub struct ShowTextGizmos(pub bool);

/// Marker on gizmo entities spawned by [`rebuild_panel_gizmos`].
#[derive(Component)]
pub(super) struct PanelGizmoChild;

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

/// Parameters for spawning a gizmo rectangle on a panel.
struct GizmoRect<'a> {
    bounds:     &'a BoundingBox,
    pts_mpu:    f32,
    anchor_x:   f32,
    anchor_y:   f32,
    color:      Color,
    line_width: f32,
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
        rect.pts_mpu,
        rect.anchor_x,
        rect.anchor_y,
        rect.color,
    );
    commands.entity(panel_entity).with_child((
        PanelGizmoChild,
        Gizmo {
            handle: gizmo_assets.add(asset),
            line_config: GizmoLineConfig {
                width: rect.line_width,
                perspective: false,
                joints: GizmoLineJoint::Round(8),
                ..default()
            },
            ..default()
        },
        Transform::IDENTITY,
    ));
}

/// Rebuilds retained gizmo children for panels whose layout changed.
///
/// Each render command (background, border, text box) gets its own
/// `GizmoAsset` entity with per-entity `GizmoLineConfig`. Borders use
/// world-accurate line widths; backgrounds are inset by the border width.
#[allow(clippy::too_many_arguments)]
pub(super) fn rebuild_panel_gizmos(
    changed_panels: Query<
        (Entity, &DiegeticPanel, &ComputedDiegeticPanel),
        Changed<ComputedDiegeticPanel>,
    >,
    existing_gizmos: Query<(Entity, &ChildOf), With<PanelGizmoChild>>,
    show_text: Res<ShowTextGizmos>,
    unit_config: Res<super::UnitConfig>,
    cameras: Query<(&Camera, &Projection)>,
    mut gizmo_assets: ResMut<Assets<GizmoAsset>>,
    mut commands: Commands,
) {
    if changed_panels.is_empty() {
        return;
    }

    let ppm = pixels_per_meter(&cameras);

    for (panel_entity, panel, computed) in &changed_panels {
        let Some(result) = computed.result() else {
            continue;
        };

        let pts_mpu = panel.points_to_world(&unit_config);
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
                    let (il, ir, it, ib) =
                        border.map_or((0.0, 0.0, 0.0, 0.0), |b| (b.left, b.right, b.top, b.bottom));
                    let inset_bounds = BoundingBox {
                        x:      cmd.bounds.x + il,
                        y:      cmd.bounds.y + it,
                        width:  (cmd.bounds.width - il - ir).max(0.0),
                        height: (cmd.bounds.height - it - ib).max(0.0),
                    };
                    spawn_rect_gizmo(
                        &mut commands,
                        panel_entity,
                        &mut gizmo_assets,
                        &GizmoRect {
                            bounds: &inset_bounds,
                            pts_mpu,
                            anchor_x,
                            anchor_y,
                            color: *color,
                            line_width: 1.0,
                        },
                    );
                },
                RenderCommandKind::Border { border } => {
                    let hl = border.left * 0.5;
                    let hr = border.right * 0.5;
                    let ht = border.top * 0.5;
                    let hb = border.bottom * 0.5;
                    let inset_bounds = BoundingBox {
                        x:      cmd.bounds.x + hl,
                        y:      cmd.bounds.y + ht,
                        width:  (cmd.bounds.width - hl - hr).max(0.0),
                        height: (cmd.bounds.height - ht - hb).max(0.0),
                    };
                    let avg_border_pts =
                        (border.left + border.right + border.top + border.bottom) / 4.0;
                    let border_px = (avg_border_pts * pts_mpu * ppm).max(1.0);
                    spawn_rect_gizmo(
                        &mut commands,
                        panel_entity,
                        &mut gizmo_assets,
                        &GizmoRect {
                            bounds: &inset_bounds,
                            pts_mpu,
                            anchor_x,
                            anchor_y,
                            color: border.color,
                            line_width: border_px,
                        },
                    );
                },
                RenderCommandKind::Text { .. } => {
                    if !show_text.0 {
                        continue;
                    }
                    spawn_rect_gizmo(
                        &mut commands,
                        panel_entity,
                        &mut gizmo_assets,
                        &GizmoRect {
                            bounds: &cmd.bounds,
                            pts_mpu,
                            anchor_x,
                            anchor_y,
                            color: Color::srgba(0.9, 0.9, 0.2, 0.2),
                            line_width: 1.0,
                        },
                    );
                },
                RenderCommandKind::ScissorStart | RenderCommandKind::ScissorEnd => {},
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

    use super::*;
    use crate::layout::El;
    use crate::layout::LayoutBuilder;
    use crate::layout::LayoutEngine;
    use crate::layout::LayoutTextStyle;
    use crate::layout::LayoutTree;
    use crate::layout::Sizing;
    use crate::layout::TextDimensions;
    use crate::layout::TextMeasure;

    fn monospace_measure() -> MeasureTextFn {
        Arc::new(|text: &str, measure: &TextMeasure| {
            #[allow(clippy::cast_precision_loss)]
            let char_width = measure.size * 0.6;
            #[allow(clippy::cast_precision_loss)]
            let width = char_width * text.len() as f32;
            TextDimensions {
                width,
                height: measure.size,
                line_height: measure.size,
            }
        })
    }

    // ── Performance timing tests (run with --run-ignored all) ────────

    use crate::layout::Border;
    use crate::layout::Direction;
    use crate::layout::Padding;

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
        #[allow(clippy::cast_possible_truncation)]
        let per_iter = elapsed / iterations as u32;
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
