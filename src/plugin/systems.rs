//! Systems for diegetic UI panel layout computation and debug rendering.

use std::sync::Arc;
use std::sync::Mutex;
use std::sync::PoisonError;
use std::time::Instant;

use bevy::prelude::*;

use super::DiegeticPanelGizmoGroup;
use super::components::ComputedDiegeticPanel;
use super::components::DiegeticPanel;
use super::components::DiegeticTextMeasurer;
use crate::layout::BoundingBox;
use crate::layout::LayoutEngine;
use crate::layout::LayoutResult;
use crate::layout::LayoutTree;
use crate::layout::RectangleSource;
use crate::layout::RenderCommandKind;
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
    pub last_compute_ms:          f32,
    /// Number of panels processed by the most recent layout run.
    pub last_compute_panels:      usize,
    /// Duration of the most recent text extraction run, in milliseconds.
    pub last_text_extract_ms:     f32,
    /// Number of panels processed by the most recent text extraction run.
    pub last_text_extract_panels: usize,
}

/// Recomputes layout for panels whose [`DiegeticPanel`] component has changed.
///
/// Uses the [`ShapedTextCache`] for measurement: if a text string has already
/// been shaped (by a previous layout or render pass), its dimensions are
/// returned from the cache without calling parley. On cache miss, falls back
/// to the parley-backed [`DiegeticTextMeasurer`].
pub fn compute_panel_layouts(
    mut panels: Query<(&DiegeticPanel, &mut ComputedDiegeticPanel), Changed<DiegeticPanel>>,
    measurer: Res<DiegeticTextMeasurer>,
    cache: Res<ShapedTextCache>,
    mut perf: ResMut<DiegeticPerfStats>,
) {
    if panels.is_empty() {
        perf.last_compute_ms = 0.0;
        perf.last_compute_panels = 0;
        return;
    }

    let start = Instant::now();
    let mut panel_count = 0_usize;

    // Wrap the cache in Arc<Mutex<>> so the MeasureTextFn closure can capture it.
    let cache_ref = Arc::new(Mutex::new(cache.clone()));
    let parley_fn = Arc::clone(&measurer.measure_fn);

    let cached_measure: crate::layout::MeasureTextFn =
        Arc::new(move |text: &str, measure: &crate::layout::TextMeasure| {
            // Check cache first.
            let cache_guard = cache_ref.lock().unwrap_or_else(PoisonError::into_inner);
            if let Some(dims) = cache_guard.get_measurement(text, measure) {
                return dims;
            }
            drop(cache_guard);
            // Cache miss — fall back to parley.
            parley_fn(text, measure)
        });

    for (panel, mut computed) in &mut panels {
        panel_count += 1;

        if computed.is_color_only_change(
            panel.tree.layout_hash(),
            panel.layout_width,
            panel.layout_height,
        ) {
            // Layout structure is identical — only render properties (colors)
            // changed. Patch colors in the existing render commands and skip
            // the expensive `engine.compute()` call.
            patch_colors(&panel.tree, computed.result_mut().unwrap());
            computed.set_color_only(true);
        } else {
            // Full layout recomputation.
            let engine = LayoutEngine::new(Arc::clone(&cached_measure));
            let result = engine.compute(&panel.tree, panel.layout_width, panel.layout_height);

            if let Some(bounds) = result.content_bounds() {
                let scale_x = panel.world_width / panel.layout_width;
                let scale_y = panel.world_height / panel.layout_height;
                computed.set_content_size(bounds.width * scale_x, bounds.height * scale_y);
            }

            computed.set_last_layout(
                panel.tree.layout_hash(),
                panel.layout_width,
                panel.layout_height,
            );
            computed.set_result(result);
            computed.set_color_only(false);
        }
    }

    perf.last_compute_ms = start.elapsed().as_secs_f32() * 1000.0;
    perf.last_compute_panels = panel_count;
}

/// Updates render command colors from the new tree without recomputing layout.
///
/// Each [`RenderCommand`] stores its source `element_idx`, so we can look up
/// the current color from the tree and patch it into the existing command.
fn patch_colors(tree: &LayoutTree, result: &mut LayoutResult) {
    for cmd in &mut result.commands {
        let Some(colors) = tree.element_colors_at(cmd.element_idx) else {
            continue;
        };
        match &mut cmd.kind {
            RenderCommandKind::Text { config, .. } => {
                if let Some(c) = colors.text {
                    config.set_color(c);
                }
            },
            RenderCommandKind::Rectangle { color, source } => match source {
                RectangleSource::Background => {
                    if let Some(bg) = colors.background {
                        *color = bg;
                    }
                },
                RectangleSource::BetweenChildrenBorder => {
                    if let Some(c) = colors.border {
                        *color = c;
                    }
                },
            },
            RenderCommandKind::Border { border } => {
                if let Some(c) = colors.border {
                    border.color = c;
                }
            },
            _ => {},
        }
    }
}

/// Controls whether text bounding-box gizmos are drawn.
///
/// When `false` (the default), only rectangles and borders are shown.
/// Toggle at runtime to debug text measurement and positioning.
///
/// **Note:** This API is provisional. Once panels render real geometry
/// (Phase 4), debug visualization will likely move to a per-panel debug
/// mode rather than a global resource.
#[derive(Resource)]
pub struct ShowTextGizmos(pub bool);

impl Default for ShowTextGizmos {
    fn default() -> Self { Self(false) }
}

/// Renders debug gizmo wireframes for all panels with computed layouts.
///
/// Skips text bounding boxes unless [`ShowTextGizmos`] is enabled.
pub(super) fn render_panel_gizmos(
    panels: Query<(&DiegeticPanel, &ComputedDiegeticPanel, &GlobalTransform)>,
    mut gizmos: Gizmos<DiegeticPanelGizmoGroup>,
    show_text: Res<ShowTextGizmos>,
) {
    for (panel, computed, global_transform) in &panels {
        let Some(result) = computed.result() else {
            continue;
        };

        let scale_x = panel.world_width / panel.layout_width;
        let scale_y = panel.world_height / panel.layout_height;
        let half_w = panel.world_width * 0.5;
        let half_h = panel.world_height * 0.5;

        for cmd in &result.commands {
            let z_offset = match &cmd.kind {
                RenderCommandKind::Rectangle { .. } => 0.0,
                RenderCommandKind::Text { .. } => {
                    if !show_text.0 {
                        continue;
                    }
                    0.001
                },
                RenderCommandKind::Border { .. } => 0.002,
                RenderCommandKind::ScissorStart | RenderCommandKind::ScissorEnd => continue,
            };

            let color = match &cmd.kind {
                RenderCommandKind::Rectangle { color, .. } => color.with_alpha(0.2),
                RenderCommandKind::Text { .. } => Color::srgba(0.9, 0.9, 0.2, 0.2),
                RenderCommandKind::Border { border } => border.color.with_alpha(0.2),
                _ => continue,
            };

            draw_rect_outline(
                &mut gizmos,
                global_transform,
                &cmd.bounds,
                scale_x,
                scale_y,
                half_w,
                half_h,
                z_offset,
                color,
            );
        }
    }
}

/// Draws a rectangle outline in world space from layout-space bounds.
///
/// Transforms layout coordinates (top-left origin, Y-down) to panel-local
/// coordinates (center origin, Y-up), then to world space via the entity's
/// [`GlobalTransform`].
#[allow(clippy::too_many_arguments)]
fn draw_rect_outline(
    gizmos: &mut Gizmos<DiegeticPanelGizmoGroup>,
    global_transform: &GlobalTransform,
    bounds: &BoundingBox,
    scale_x: f32,
    scale_y: f32,
    half_w: f32,
    half_h: f32,
    z: f32,
    color: Color,
) {
    // Layout coordinates → panel-local coordinates.
    // Layout: origin at top-left, X-right, Y-down.
    // Panel local: origin at center, X-right, Y-up.
    let left = bounds.x.mul_add(scale_x, -half_w);
    let right = (bounds.x + bounds.width).mul_add(scale_x, -half_w);
    let top = (-bounds.y).mul_add(scale_y, half_h);
    let bottom = (-(bounds.y + bounds.height)).mul_add(scale_y, half_h);

    // Panel-local → world via the entity's transform.
    let tl = global_transform.transform_point(Vec3::new(left, top, z));
    let tr = global_transform.transform_point(Vec3::new(right, top, z));
    let br = global_transform.transform_point(Vec3::new(right, bottom, z));
    let bl = global_transform.transform_point(Vec3::new(left, bottom, z));

    gizmos.line(tl, tr, color);
    gizmos.line(tr, br, color);
    gizmos.line(br, bl, color);
    gizmos.line(bl, tl, color);
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::layout::El;
    use crate::layout::LayoutBuilder;
    use crate::layout::LayoutEngine;
    use crate::layout::Sizing;
    use crate::layout::TextConfig;
    use crate::layout::TextDimensions;
    use crate::layout::TextMeasure;

    fn monospace_measure() -> crate::layout::MeasureTextFn {
        Arc::new(|text: &str, measure: &TextMeasure| {
            let line_height = measure.effective_line_height();
            #[allow(clippy::cast_precision_loss)]
            let char_width = measure.size * 0.6;
            #[allow(clippy::cast_precision_loss)]
            let width = char_width * text.len() as f32;
            TextDimensions {
                width,
                height: line_height,
            }
        })
    }

    /// The color-only guard must detect when panel dimensions change even
    /// though the tree's `layout_hash` is unchanged. Without the dimension
    /// check, resizing a panel reuses the old layout — wrong wrapping, wrong
    /// bounds.
    #[test]
    fn dimension_change_not_treated_as_color_only() {
        let mut b = LayoutBuilder::new(800.0, 400.0);
        b.with(
            El::new().width(Sizing::Grow {
                min: 0.0,
                max: f32::MAX,
            }),
            |b| {
                b.text("some text", TextConfig::default());
            },
        );
        let tree = b.build();
        let hash = tree.layout_hash();
        assert_ne!(hash, 0, "tree should have a valid layout hash");

        // Simulate: first full layout completed and stored the hash.
        let engine = LayoutEngine::new(monospace_measure());
        let result = engine.compute(&tree, 800.0, 400.0);

        let mut computed = ComputedDiegeticPanel::default();
        computed.set_result(result);
        computed.set_content_size(1.0, 1.0);
        computed.set_last_layout(hash, 800.0, 400.0);
        computed.set_color_only(false);

        // Same hash, same dimensions — guard should say color-only.
        assert!(
            computed.is_color_only_change(hash, 800.0, 400.0),
            "identical hash and dimensions should be detected as color-only"
        );

        // Panel dimensions changed (layout_width 800 → 200) but the tree
        // object is the same so layout_hash is unchanged. The guard must
        // NOT return true — layout needs recomputing for the new width.
        assert!(
            !computed.is_color_only_change(hash, 200.0, 400.0),
            "dimension change must trigger full recompute, not color-only path"
        );
    }

    // ── Performance timing tests (run with --run-ignored all) ────────

    use crate::layout::Border;
    use crate::layout::Direction;
    use crate::layout::Padding;

    const PERF_FONT_SIZE: f32 = 7.0;
    const PERF_LAYOUT_WIDTH: f32 = 800.0;
    const PERF_LAYOUT_HEIGHT: f32 = 1200.0;

    fn build_stress_tree(row_count: usize) -> crate::layout::LayoutTree {
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
                            b.text(&format!("item {i}:"), TextConfig::new(PERF_FONT_SIZE));
                            b.with(
                                El::new().width(Sizing::GROW).height(Sizing::fixed(1.0)),
                                |_| {},
                            );
                            b.text("value", TextConfig::new(PERF_FONT_SIZE));
                        },
                    );
                }
            },
        );
        builder.build()
    }

    fn build_stress_tree_colored(row_count: usize, hue_offset: f32) -> crate::layout::LayoutTree {
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
                #[allow(clippy::cast_precision_loss)]
                for i in 0..row_count {
                    let hue = (360.0 * (i as f32 / row_count as f32) + hue_offset) % 360.0;
                    let color = bevy::color::Color::hsl(hue, 0.8, 0.6);
                    let config = TextConfig::new(PERF_FONT_SIZE).with_color(color);
                    b.with(
                        El::new()
                            .width(Sizing::GROW)
                            .height(Sizing::FIT)
                            .direction(Direction::LeftToRight)
                            .child_gap(4.0),
                        |b| {
                            b.text(&format!("item {i}:"), config.clone());
                            b.with(
                                El::new().width(Sizing::GROW).height(Sizing::fixed(1.0)),
                                |_| {},
                            );
                            b.text("value", config);
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
        let per_iter = elapsed / iterations as u32;
        println!(
            "{label}: {per_iter:?} per iteration ({iterations} iterations, {elapsed:?} total)"
        );
    }

    #[test]
    #[ignore]
    fn perf_element_sizes() {
        println!(
            "TextConfig size: {} bytes",
            std::mem::size_of::<TextConfig>()
        );
    }

    #[test]
    #[ignore]
    fn perf_tree_build() {
        for &rows in &[10, 100, 500, 1000] {
            let iters = if rows <= 100 { 1000 } else { 100 };
            run_timing(&format!("tree_build_{rows}_rows"), iters, || {
                std::hint::black_box(build_stress_tree(rows));
            });
        }
    }

    #[test]
    #[ignore]
    fn perf_tree_build_breakdown() {
        let rows = 1000;
        let iters = 100;

        // 1. Just the string formatting cost.
        run_timing(&format!("string_format_{rows}_rows"), iters, || {
            for i in 0..rows {
                std::hint::black_box(format!("item {i}:"));
            }
        });

        // 2. Tree build with pre-built strings (no format! per row).
        let labels: Vec<String> = (0..rows).map(|i| format!("item {i}:")).collect();
        run_timing(
            &format!("tree_build_prebuilt_strings_{rows}_rows"),
            iters,
            || {
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
                        for label in &labels {
                            b.with(
                                El::new()
                                    .width(Sizing::GROW)
                                    .height(Sizing::FIT)
                                    .direction(Direction::LeftToRight)
                                    .child_gap(4.0),
                                |b| {
                                    b.text(label, TextConfig::new(PERF_FONT_SIZE));
                                    b.with(
                                        El::new().width(Sizing::GROW).height(Sizing::fixed(1.0)),
                                        |_| {},
                                    );
                                    b.text("value", TextConfig::new(PERF_FONT_SIZE));
                                },
                            );
                        }
                    },
                );
                std::hint::black_box(builder.build());
            },
        );

        // 3. Tree build with pre-allocated capacity.
        run_timing(
            &format!("tree_build_preallocated_{rows}_rows"),
            iters,
            || {
                let capacity = rows * 4 + 2;
                let mut builder =
                    LayoutBuilder::with_capacity(PERF_LAYOUT_WIDTH, PERF_LAYOUT_HEIGHT, capacity);
                builder.with(
                    El::new()
                        .width(Sizing::GROW)
                        .height(Sizing::FIT)
                        .direction(Direction::TopToBottom)
                        .child_gap(2.0)
                        .padding(Padding::all(4.0))
                        .border(Border::all(1.0, bevy::color::Color::WHITE)),
                    |b| {
                        for label in &labels {
                            b.with(
                                El::new()
                                    .width(Sizing::GROW)
                                    .height(Sizing::FIT)
                                    .direction(Direction::LeftToRight)
                                    .child_gap(4.0),
                                |b| {
                                    b.text(label, TextConfig::new(PERF_FONT_SIZE));
                                    b.with(
                                        El::new().width(Sizing::GROW).height(Sizing::fixed(1.0)),
                                        |_| {},
                                    );
                                    b.text("value", TextConfig::new(PERF_FONT_SIZE));
                                },
                            );
                        }
                    },
                );
                std::hint::black_box(builder.build());
            },
        );
    }

    #[test]
    #[ignore]
    fn perf_engine_compute() {
        let measure = monospace_measure();
        for &rows in &[10, 100, 500, 1000] {
            let tree = build_stress_tree(rows);
            let engine = LayoutEngine::new(Arc::clone(&measure));
            let iters = if rows <= 100 { 1000 } else { 100 };
            run_timing(&format!("engine_compute_{rows}_rows"), iters, || {
                std::hint::black_box(engine.compute(&tree, PERF_LAYOUT_WIDTH, PERF_LAYOUT_HEIGHT));
            });
        }
    }

    #[test]
    #[ignore]
    fn perf_patch_colors() {
        let measure = monospace_measure();
        for &rows in &[10, 100, 500, 1000] {
            let tree = build_stress_tree(rows);
            let engine = LayoutEngine::new(Arc::clone(&measure));
            let mut result = engine.compute(&tree, PERF_LAYOUT_WIDTH, PERF_LAYOUT_HEIGHT);

            // Build a tree with different colors but same structure.
            let colored_tree = build_stress_tree_colored(rows, 90.0);

            let iters = if rows <= 100 { 10000 } else { 1000 };
            run_timing(&format!("patch_colors_{rows}_rows"), iters, || {
                patch_colors(&colored_tree, &mut result);
            });
        }
    }

    #[test]
    #[ignore]
    fn perf_layout_hash() {
        for &rows in &[10, 100, 500, 1000] {
            let tree = build_stress_tree(rows);
            let hash = tree.layout_hash();
            let iters = if rows <= 100 { 10000 } else { 1000 };
            run_timing(&format!("layout_hash_compare_{rows}_rows"), iters, || {
                std::hint::black_box(hash == tree.layout_hash());
            });
        }
    }
}
