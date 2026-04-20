//! [`compute_panel_layouts`] — recomputes layout for changed panels.

use std::sync::Arc;
use std::sync::Mutex;
use std::sync::PoisonError;
use std::time::Instant;

use bevy::prelude::*;

use super::diegetic_panel::ComputedDiegeticPanel;
use super::diegetic_panel::DiegeticPanel;
use super::perf::DiegeticPerfStats;
use crate::constants::MILLISECONDS_PER_SECOND;
use crate::layout::LayoutEngine;
use crate::layout::MeasureTextFn;
use crate::layout::ShapedTextCache;
use crate::layout::TextMeasure;
use crate::layout::UnitConfig;
use crate::text::DiegeticTextMeasurer;

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
    unit_config: Res<UnitConfig>,
) {
    let changed_entities: Vec<Entity> = panels
        .iter()
        .filter(|(_, panel_ref)| panel_ref.is_changed())
        .map(|(entity, _)| entity)
        .collect();

    if changed_entities.is_empty() {
        perf.compute_ms = 0.0;
        perf.compute_panels = 0;
        return;
    }

    let start = Instant::now();
    let mut panel_count = 0_usize;

    let cache_ref = Arc::new(Mutex::new(cache.clone()));
    let parley_fn = Arc::clone(&measurer.measure_fn);

    let cached_measure: MeasureTextFn = Arc::new(move |text: &str, measure: &TextMeasure| {
        {
            let cache_guard = cache_ref.lock().unwrap_or_else(PoisonError::into_inner);
            if let Some(dims) = cache_guard.get_measurement(text, measure) {
                return dims;
            }
        }
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

        let layout_unit = panel_ref.layout_unit();
        let font_unit = panel_ref.font_unit().unwrap_or(unit_config.font);
        let layout_to_pts = layout_unit.to_points();
        let font_to_pts = font_unit.to_points();

        let scaled_tree = panel_ref.tree().scaled(layout_to_pts, font_to_pts);
        let engine = LayoutEngine::new(Arc::clone(&cached_measure));
        let result = engine.compute(
            &scaled_tree,
            panel_ref.width() * layout_to_pts,
            panel_ref.height() * layout_to_pts,
            1.0,
        );

        if let Some(bounds) = result.content_bounds() {
            let s = panel_ref.points_to_world(&unit_config);
            computed.set_content_size(bounds.width * s, bounds.height * s);
        }

        computed.set_result(result);
    }

    let compute_ms = start.elapsed().as_secs_f32() * MILLISECONDS_PER_SECOND;
    perf.compute_ms = compute_ms;
    perf.compute_panels = panel_count;
}
