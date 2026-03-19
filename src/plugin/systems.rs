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
use crate::layout::RenderCommandKind;
use crate::render::ShapedTextCache;

/// Lightweight timing data for diegetic UI systems.
///
/// These values are updated by the built-in layout and text extraction systems
/// so examples and applications can inspect where time is being spent during
/// content-heavy updates.
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
    let parley_fn = Arc::clone(&measurer.0);

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
        let engine = LayoutEngine::new(Arc::clone(&cached_measure));
        let result = engine.compute(&panel.tree, panel.layout_width, panel.layout_height);

        if let Some(bounds) = result.content_bounds() {
            let scale_x = panel.world_width / panel.layout_width;
            let scale_y = panel.world_height / panel.layout_height;
            computed.world_width = bounds.width * scale_x;
            computed.world_height = bounds.height * scale_y;
        }

        computed.result = Some(result);
    }

    perf.last_compute_ms = start.elapsed().as_secs_f32() * 1000.0;
    perf.last_compute_panels = panel_count;
}

/// Controls whether text bounding-box gizmos are drawn.
///
/// When `false` (the default), only rectangles and borders are shown.
/// Toggle at runtime to debug text measurement and positioning.
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
        let Some(result) = &computed.result else {
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
                RenderCommandKind::Rectangle { color } => *color,
                RenderCommandKind::Text { .. } => Color::srgba(0.9, 0.9, 0.2, 0.8),
                RenderCommandKind::Border { border } => border.color,
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
