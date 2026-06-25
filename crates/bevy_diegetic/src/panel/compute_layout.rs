//! [`compute_panel_layouts`] — recomputes layout for changed panels, plus
//! [`resolve_world_panel_fit`] — shrinks `Fit`-axis world panels to their
//! content bounds after layout runs.

use std::sync::Arc;
use std::time::Instant;

use bevy::prelude::*;

use super::constants::PANEL_RESIZE_EPSILON;
use super::coordinate_space::CoordinateSpace;
use super::diegetic_panel::ComputedDiegeticPanel;
use super::diegetic_panel::DiegeticPanel;
use super::diegetic_panel::DiegeticPanelChangeClassification;
use super::diegetic_panel::ScaledLayoutTreeCache;
use super::events;
use super::events::LastPanelDimensions;
use super::events::PanelChangeKind;
use super::field;
use super::perf::DiegeticPerfStats;
use crate::cascade::CascadeDefaults;
use crate::cascade::FontUnit;
use crate::cascade::Resolved;
use crate::constants::MILLISECONDS_PER_SECOND;
use crate::layout::LayoutEngine;
use crate::layout::LayoutResult;
use crate::layout::LayoutTree;
use crate::layout::LayoutTreeChange;
use crate::layout::MeasureTextFn;
use crate::layout::ShapedTextCache;
use crate::layout::Sizing;
use crate::layout::TextMeasure;
use crate::text::DiegeticTextMeasurer;

/// Recomputes layout for panels whose [`DiegeticPanel`] component has changed.
///
/// Uses the [`ShapedTextCache`] for measurement: if a text string has already
/// been shaped (by a previous layout or render pass), its dimensions are
/// returned from the cache without calling parley. On cache miss, falls back
/// to the parley-backed [`DiegeticTextMeasurer`].
pub(super) fn compute_panel_layouts(
    mut panels: Query<(
        Entity,
        Ref<DiegeticPanel>,
        &mut DiegeticPanelChangeClassification,
        &mut ScaledLayoutTreeCache,
    )>,
    mut computed_panels: Query<&mut ComputedDiegeticPanel>,
    mut commands: Commands,
    panel_font_units: Query<&Resolved<FontUnit>>,
    measurer: Res<DiegeticTextMeasurer>,
    cache: Res<ShapedTextCache>,
    mut perf: ResMut<DiegeticPerfStats>,
    defaults: Res<CascadeDefaults>,
    mut trace_init: Local<bool>,
    mut trace_remaining: Local<u32>,
) {
    let start = Instant::now();
    let mut panel_count = 0_usize;
    if !*trace_init {
        *trace_init = true;
        *trace_remaining = 30;
    }

    let cached_measure = build_cached_measure(&cache, &measurer);

    for (entity, panel_ref, mut tree_change, mut scaled_tree_cache) in &mut panels {
        let panel_changed = panel_ref.is_changed();
        let pending_some = tree_change.pending().is_some();
        if !panel_changed && !pending_some {
            continue;
        }
        if *trace_remaining > 0 {
            *trace_remaining -= 1;
            bevy::log::debug!(
                target: "compute_panel_layouts_trace",
                "entity={entity:?} panel_changed={panel_changed} pending={pending_some} budget_left={}",
                *trace_remaining,
            );
        }
        let Ok(mut computed) = computed_panels.get_mut(entity) else {
            continue;
        };

        let layout_unit = panel_ref.layout_unit();
        // Every panel carries a seeded `Override<FontUnit>`, so `Resolved` is
        // always present; the fallback to the construction-time
        // `panel_font_unit` seed only guards a missing-component edge.
        let font_unit = panel_font_units
            .get(entity)
            .map_or(defaults.panel_font_unit, |resolved| resolved.0.0);
        let layout_to_points = layout_unit.to_points();
        let font_to_points = font_unit.to_points();

        let (pending_change, tree_visual_geometry_stable) =
            tree_change.take_with_tree_visual_geometry_stable();
        if matches!(pending_change, Some(LayoutTreeChange::Identical))
            && computed.result().is_some()
        {
            continue;
        }
        let had_result = computed.result().is_some();

        let scaled_tree = scaled_tree_cache.get_or_update(
            panel_ref.tree(),
            panel_ref.tree_revision(),
            layout_to_points,
            font_to_points,
        );

        let viewport_width = panel_ref.width() * layout_to_points;
        let viewport_height = panel_ref.height() * layout_to_points;

        // Geometry-stable skip: regenerate render commands from cached positions
        // without re-running the layout solve. Safe only when a text-only edit
        // (`VisualOnly`) leaves every leaf's box unchanged — verified by
        // `can_reuse_geometry`, which re-measures the leaves and rejects anything
        // the reuse would render wrong.
        let can_reuse_geometry = tree_visual_geometry_stable
            || computed.result().is_some_and(|result| {
                result.can_reuse_geometry(
                    scaled_tree,
                    &cached_measure,
                    viewport_width,
                    viewport_height,
                    1.0,
                )
            });
        if matches!(pending_change, Some(LayoutTreeChange::VisualOnly))
            && can_reuse_geometry
            && let Some(result) = computed.result_mut()
        {
            result.regenerate_commands(scaled_tree);
            computed.refresh_draw_order_projection();
            events::trigger_panel_changed(&mut commands, entity, PanelChangeKind::VisualOnly);
            panel_count += 1;
            continue;
        }

        let engine = LayoutEngine::new(Arc::clone(&cached_measure));
        let result = engine.compute(scaled_tree, viewport_width, viewport_height, 1.0);

        commit_layout_result(&mut computed, &panel_ref, scaled_tree, result, entity);
        events::trigger_panel_changed(
            &mut commands,
            entity,
            PanelChangeKind::from_layout_change(pending_change, had_result),
        );
        panel_count += 1;
    }

    // Zeroes on an empty frame so the `layout` row reads 0 when no panel relaid
    // out, rather than the bare loop overhead.
    perf.compute_ms = if panel_count == 0 {
        0.0
    } else {
        start.elapsed().as_secs_f32() * MILLISECONDS_PER_SECOND
    };
    perf.compute_panels = panel_count;
}

/// Builds the cache-backed text measurer for one layout pass.
///
/// Clones the shared [`ShapedTextCache`] handle (a refcount bump, not a map
/// copy) into the returned `'static` closure. Each lookup hits the shared cache;
/// on a miss it falls back to the parley-backed [`DiegeticTextMeasurer`] and
/// inserts the result back into the shared cache, so measurements computed
/// during layout persist for the renderer instead of being discarded each frame.
fn build_cached_measure(cache: &ShapedTextCache, measurer: &DiegeticTextMeasurer) -> MeasureTextFn {
    let cache_handle = cache.clone();
    let parley_fn = Arc::clone(&measurer.measure_fn);

    Arc::new(move |text: &str, measure: &TextMeasure| {
        if let Some(dims) = cache_handle.get_measurement(text, measure) {
            return dims;
        }
        let dims = parley_fn(text, measure);
        cache_handle.insert_measurement(text, measure, dims);
        dims
    })
}

/// Finalizes a completed layout pass onto the panel.
///
/// Records content bounds (converted to world units) and editable-field
/// records, warning on duplicate field ids.
fn commit_layout_result(
    computed: &mut ComputedDiegeticPanel,
    panel_ref: &DiegeticPanel,
    scaled_tree: &LayoutTree,
    result: LayoutResult,
    entity: Entity,
) {
    if let Some(bounds) = result.content_bounds() {
        let s = panel_ref.points_to_world();
        computed.set_content_size(bounds.width * s, bounds.height * s);
    }

    let (field_records, field_id_conflicts) =
        field::collect_panel_field_records(scaled_tree, &result);
    if !field_id_conflicts.is_empty() {
        bevy::log::warn!(
            target: "bevy_diegetic::ime",
            "panel {entity:?} has duplicate editable field ids: {field_id_conflicts:?}"
        );
    }
    computed.set_result_with_fields(result, field_records, field_id_conflicts);
}

/// Resolves `Fit`-axis world panels to their content bounds.
///
/// Runs after [`compute_panel_layouts`] writes the layout result. For each
/// world panel whose width or height is `Sizing::Fit { min, max }`, reads
/// the computed content bounds (in layout points) and shrinks the panel's
/// physical width / height to match, clamped to `[min, max]`.
///
/// Screen panels resolve their own dynamic sizing earlier in the pipeline
/// via `position_screen_space_panels` + `resolve_screen_axis`, so this
/// system intentionally only touches world panels.
pub(super) fn resolve_world_panel_fit(
    mut panels: Query<(
        Entity,
        &mut DiegeticPanel,
        &ComputedDiegeticPanel,
        &mut LastPanelDimensions,
    )>,
    mut commands: Commands,
) {
    for (entity, mut panel, computed, mut last_dimensions) in &mut panels {
        let (w_sizing, h_sizing) = match panel.coordinate_space() {
            CoordinateSpace::World { width, height } => (*width, *height),
            CoordinateSpace::Screen { .. } => continue,
        };

        if let Some(bounds) = computed.content_bounds() {
            let layout_to_points = panel.layout_unit().to_points();
            if layout_to_points > 0.0 {
                let horizontal_content = bounds.width / layout_to_points;
                let vertical_content = bounds.height / layout_to_points;

                if let Sizing::Fit { min, max } = w_sizing {
                    let clamped = horizontal_content.clamp(min.value, max.value);
                    if (panel.width() - clamped).abs() > PANEL_RESIZE_EPSILON {
                        panel.set_width(clamped);
                    }
                }
                if let Sizing::Fit { min, max } = h_sizing {
                    let clamped = vertical_content.clamp(min.value, max.value);
                    if (panel.height() - clamped).abs() > PANEL_RESIZE_EPSILON {
                        panel.set_height(clamped);
                    }
                }
            }
        }

        events::trigger_panel_dimensions_changed(
            &mut commands,
            entity,
            &panel,
            computed,
            &mut last_dimensions,
        );
    }
}

#[cfg(test)]
#[allow(
    clippy::float_cmp,
    reason = "tests compare exact expected layout values"
)]
#[allow(
    clippy::expect_used,
    reason = "tests should panic on unexpected values"
)]
mod tests {
    use std::sync::Arc;

    use bevy::prelude::*;
    use bevy::window::PrimaryWindow;
    use bevy::window::Window;
    use bevy_kana::ToF32;

    use crate::Anchor;
    use crate::Fit;
    use crate::FitMax;
    use crate::Mm;
    use crate::Percent;
    use crate::Px;
    use crate::TextStyle;
    use crate::cascade::FontUnit;
    use crate::cascade::Resolved;
    use crate::constants::MONOSPACE_WIDTH_RATIO;
    use crate::layout::LayoutBuilder;
    use crate::layout::LayoutTree;
    use crate::layout::RenderCommandKind;
    use crate::layout::TextDimensions;
    use crate::layout::TextMeasure;
    use crate::layout::Unit;
    use crate::panel::ComputedDiegeticPanel;
    use crate::panel::DiegeticPanel;
    use crate::panel::DiegeticPanelCommands;
    use crate::panel::HeadlessLayoutPlugin;
    use crate::panel::PanelChangeKind;
    use crate::panel::PanelChanged;
    use crate::panel::PanelDimensionsChanged;
    use crate::panel::diegetic_panel::ScaledLayoutTreeCache;
    use crate::screen_space::ScreenSpacePlugin;
    use crate::text::DiegeticTextMeasurer;

    fn monospace_measurer() -> DiegeticTextMeasurer {
        DiegeticTextMeasurer {
            measure_fn: Arc::new(|text: &str, measure: &TextMeasure| {
                let char_width = measure.size * MONOSPACE_WIDTH_RATIO;
                let mut max_line_width: f32 = 0.0;
                let mut line_count = 0_u32;
                for line in text.lines() {
                    line_count += 1;
                    let width = line.chars().count().to_f32() * char_width;
                    max_line_width = max_line_width.max(width);
                }
                if line_count == 0 {
                    line_count = 1;
                }
                TextDimensions {
                    width:       max_line_width,
                    height:      measure.size * line_count.to_f32(),
                    line_height: measure.size,
                }
            }),
        }
    }

    fn make_app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.insert_resource(monospace_measurer());
        app.add_plugins(HeadlessLayoutPlugin);
        app
    }

    #[track_caller]
    fn assert_close(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() < 1e-4,
            "expected {expected}, got {actual}",
        );
    }

    fn colored_text_tree(text: &str, color: Color) -> LayoutTree {
        let mut builder = LayoutBuilder::new(100.0, 50.0);
        builder.text((text, TextStyle::new(10.0).with_color(color)));
        builder.build()
    }

    fn first_text_color(computed: &ComputedDiegeticPanel) -> Color {
        let result = computed.result().expect("layout result should exist");
        result
            .commands
            .iter()
            .find_map(|command| {
                if let RenderCommandKind::Text { config, .. } = &command.kind {
                    Some(config.color())
                } else {
                    None
                }
            })
            .expect("panel should produce a text command")
    }

    #[derive(Resource, Default)]
    struct DimensionEventLog(Vec<PanelDimensionsChanged>);

    #[derive(Resource, Default)]
    struct PanelChangeEventLog(Vec<PanelChanged>);

    fn record_dimension_event(
        event: On<PanelDimensionsChanged>,
        mut log: ResMut<DimensionEventLog>,
    ) {
        log.0.push(*event.event());
    }

    fn record_panel_changed_event(event: On<PanelChanged>, mut log: ResMut<PanelChangeEventLog>) {
        log.0.push(*event.event());
    }

    #[test]
    fn world_panel_dimensions_changed_fires_once_for_first_measurement() {
        let mut app = make_app();
        app.init_resource::<DimensionEventLog>();
        app.add_observer(record_dimension_event);
        let entity = app
            .world_mut()
            .spawn(
                DiegeticPanel::world()
                    .size(Fit, Fit)
                    .with_tree(colored_text_tree("Hello", Color::WHITE))
                    .build()
                    .expect("panel should build"),
            )
            .id();

        app.update();

        let log = app.world().resource::<DimensionEventLog>();
        assert_eq!(log.0.len(), 1);
        let event = log.0[0];
        assert_eq!(event.entity, entity);
        assert!(event.previous.is_none());
        let panel = app
            .world()
            .get::<DiegeticPanel>(entity)
            .expect("panel should exist");
        assert_close(event.dimensions.resolved_size.x, panel.width());
        assert_close(event.dimensions.resolved_size.y, panel.height());
        assert_close(
            event.dimensions.resolved_size.x,
            event.dimensions.content_size.x,
        );
        assert_close(
            event.dimensions.resolved_size.y,
            event.dimensions.content_size.y,
        );

        app.update();

        let log = app.world().resource::<DimensionEventLog>();
        assert_eq!(log.0.len(), 1);
    }

    #[test]
    fn queued_visual_only_tree_change_regenerates_commands_without_moving_content() {
        let mut app = make_app();
        let entity = app
            .world_mut()
            .spawn(
                DiegeticPanel::world()
                    .size(Mm(100.0), Mm(50.0))
                    .with_tree(colored_text_tree("Hello", Color::WHITE))
                    .build()
                    .expect("panel should build"),
            )
            .id();

        app.update();

        let before = app
            .world()
            .get::<ComputedDiegeticPanel>(entity)
            .expect("computed panel should exist")
            .content_bounds()
            .expect("content bounds should exist");
        assert_eq!(
            first_text_color(
                app.world()
                    .get::<ComputedDiegeticPanel>(entity)
                    .expect("computed panel should exist")
            ),
            Color::WHITE
        );

        app.world_mut()
            .commands()
            .set_tree(entity, colored_text_tree("Hello", Color::BLACK));
        app.update();

        let computed = app
            .world()
            .get::<ComputedDiegeticPanel>(entity)
            .expect("computed panel should exist");
        let after = computed
            .content_bounds()
            .expect("content bounds should still exist");
        assert_eq!(before, after);
        assert_eq!(first_text_color(computed), Color::BLACK);

        let panel = app
            .world()
            .get::<DiegeticPanel>(entity)
            .expect("panel should exist");
        assert_eq!(panel.tree_revision(), 1);
    }

    #[test]
    fn visual_only_tree_change_fires_panel_changed_but_not_dimensions_changed() {
        let mut app = make_app();
        app.init_resource::<PanelChangeEventLog>();
        app.init_resource::<DimensionEventLog>();
        app.add_observer(record_panel_changed_event);
        app.add_observer(record_dimension_event);

        let entity = app
            .world_mut()
            .spawn(
                DiegeticPanel::world()
                    .size(Mm(100.0), Mm(50.0))
                    .with_tree(colored_text_tree("Hello", Color::WHITE))
                    .build()
                    .expect("panel should build"),
            )
            .id();
        app.update();
        app.world_mut()
            .resource_mut::<PanelChangeEventLog>()
            .0
            .clear();
        app.world_mut()
            .resource_mut::<DimensionEventLog>()
            .0
            .clear();

        app.world_mut()
            .commands()
            .set_tree(entity, colored_text_tree("Hello", Color::BLACK));
        app.update();

        let panel_change_events = &app.world().resource::<PanelChangeEventLog>().0;
        assert_eq!(panel_change_events.len(), 1);
        assert_eq!(panel_change_events[0].entity, entity);
        assert_eq!(panel_change_events[0].kind, PanelChangeKind::VisualOnly);
        assert!(
            app.world().resource::<DimensionEventLog>().0.is_empty(),
            "color-only edits refresh computed output without changing dimensions",
        );
    }

    #[test]
    fn repeated_queued_tree_changes_compose_to_layout_affecting() {
        let mut app = make_app();
        let entity = app
            .world_mut()
            .spawn(
                DiegeticPanel::world()
                    .size(Mm(100.0), Mm(50.0))
                    .with_tree(colored_text_tree("Hi", Color::WHITE))
                    .build()
                    .expect("panel should build"),
            )
            .id();

        app.update();
        let before = app
            .world()
            .get::<ComputedDiegeticPanel>(entity)
            .expect("computed panel should exist")
            .content_bounds()
            .expect("content bounds should exist");

        {
            let mut commands = app.world_mut().commands();
            commands.set_tree(entity, colored_text_tree("Hi", Color::BLACK));
            commands.set_tree(entity, colored_text_tree("Hello", Color::BLACK));
        }
        app.update();

        let computed = app
            .world()
            .get::<ComputedDiegeticPanel>(entity)
            .expect("computed panel should exist");
        let after = computed
            .content_bounds()
            .expect("content bounds should still exist");
        assert!(
            after.width > before.width,
            "layout-affecting text change should recompute content width"
        );
        assert_eq!(first_text_color(computed), Color::BLACK);

        let panel = app
            .world()
            .get::<DiegeticPanel>(entity)
            .expect("panel should exist");
        assert_eq!(panel.tree_revision(), 2);
    }

    #[test]
    fn world_fit_panel_shrinks_to_content_bounds_in_meters() {
        let mut app = make_app();

        let panel = DiegeticPanel::world()
            .size(Fit, Fit)
            .layout(|b| {
                b.text(("Hello", TextStyle::new(Mm(6.0))));
            })
            .build()
            .expect("Fit world panel should build even at zero initial size");

        let entity = app.world_mut().spawn(panel).id();

        app.update();
        app.update();

        let panel = app
            .world()
            .get::<DiegeticPanel>(entity)
            .expect("panel component must exist");

        assert!(
            panel.width() > 0.0,
            "panel.width() = {}, expected > 0 after layout",
            panel.width()
        );
        assert!(
            panel.height() > 0.0,
            "panel.height() = {}, expected > 0 after layout",
            panel.height()
        );

        let horizontal_meters = 5.0 * 6.0 * MONOSPACE_WIDTH_RATIO * 0.001;
        let vertical_meters = 6.0 * 0.001;
        assert!(
            (panel.width() - horizontal_meters).abs() < 0.001,
            "panel.width() = {}, expected ~{horizontal_meters}",
            panel.width()
        );
        assert!(
            (panel.height() - vertical_meters).abs() < 0.001,
            "panel.height() = {}, expected ~{vertical_meters}",
            panel.height()
        );
    }

    #[test]
    fn world_fit_panel_with_explicit_unit_shrinks_in_that_unit() {
        let mut app = make_app();

        let panel = DiegeticPanel::world()
            .size(Fit, FitMax(Mm(1000.0).into()))
            .layout(|b| {
                b.text(("Hello", TextStyle::new(Mm(6.0))));
            })
            .build()
            .expect("Fit/FitMax world panel should build");

        let entity = app.world_mut().spawn(panel).id();
        app.update();
        app.update();

        let panel = app
            .world()
            .get::<DiegeticPanel>(entity)
            .expect("panel component must exist");

        let horizontal_mm = 5.0 * 6.0 * MONOSPACE_WIDTH_RATIO;
        let vertical_mm = 6.0;
        assert!(
            (panel.width() - horizontal_mm).abs() < 0.5,
            "panel.width() = {}, expected ~{horizontal_mm} mm",
            panel.width()
        );
        assert!(
            (panel.height() - vertical_mm).abs() < 0.5,
            "panel.height() = {}, expected ~{vertical_mm} mm",
            panel.height()
        );
    }

    #[test]
    fn world_fitmax_panel_caps_at_max() {
        let mut app = make_app();

        let panel = DiegeticPanel::world()
            .size(FitMax(Mm(20.0).into()), Fit)
            .layout(|b| {
                b.text(("HelloHello", TextStyle::new(Mm(6.0))));
            })
            .build()
            .expect("FitMax world panel should build");

        let entity = app.world_mut().spawn(panel).id();
        app.update();
        app.update();

        let panel = app
            .world()
            .get::<DiegeticPanel>(entity)
            .expect("panel component must exist");

        assert!(
            (panel.width() - 20.0).abs() < 0.5,
            "panel.width() = {}, expected ~20.0 (capped)",
            panel.width()
        );
    }

    #[test]
    fn screen_fit_panel_shrinks_to_content_bounds() {
        let mut app = make_app();
        app.world_mut().spawn((
            Window {
                resolution: (1600_u32, 900_u32).into(),
                ..Default::default()
            },
            PrimaryWindow,
        ));
        app.add_plugins(ScreenSpacePlugin);

        let panel = DiegeticPanel::screen()
            .size(Fit, Fit)
            .layout(|b| {
                b.with(
                    crate::El::new()
                        .width(crate::Sizing::GROW)
                        .height(crate::Sizing::GROW)
                        .padding(crate::Padding::all(8.0)),
                    |b| {
                        b.text(("Hello", TextStyle::new(16.0)));
                    },
                );
            })
            .build()
            .expect("Fit screen panel should build");

        let entity = app.world_mut().spawn(panel).id();

        for _ in 0..5 {
            app.update();
        }

        let panel = app
            .world()
            .get::<DiegeticPanel>(entity)
            .expect("panel component must exist");

        assert!(
            panel.width() > 0.0 && panel.width() < 200.0,
            "panel.width() = {}, expected tight shrink-wrap value",
            panel.width()
        );
        assert!(
            panel.height() > 0.0 && panel.height() < 100.0,
            "panel.height() = {}, expected tight shrink-wrap value",
            panel.height()
        );

        let (ax, ay) = panel.anchor_offsets();
        assert!(
            ax.abs() < 0.01 && ay.abs() < 0.01,
            "TopLeft anchor_offsets = ({ax}, {ay}), expected ~(0, 0)",
        );
    }

    /// A `Fit` screen panel runs its first solve before `resolve_screen_axis`
    /// writes `panel.width()` back, so the viewport arrives zero-area. The root
    /// clip is seeded from the solved root box (content-sized for `Fit`), not the
    /// zero viewport, so an owner-bounds draw line must survive that first solve
    /// rather than clipping against an empty rectangle.
    #[test]
    fn screen_fit_panel_emits_owner_clipped_line_on_first_solve() {
        let mut app = make_app();
        app.world_mut().spawn((
            Window {
                resolution: (1600_u32, 900_u32).into(),
                ..Default::default()
            },
            PrimaryWindow,
        ));
        app.add_plugins(ScreenSpacePlugin);

        let panel = DiegeticPanel::screen()
            .size(Fit, Fit)
            .layout(|b| {
                b.with(
                    crate::El::new()
                        .width(crate::Sizing::fixed(Px(40.0)))
                        .height(crate::Sizing::fixed(Px(12.0)))
                        .draw(crate::PanelDraw::lines([crate::PanelLine::new(
                            crate::PanelPoint::new(
                                crate::PanelCoord::start(0.0),
                                crate::PanelCoord::percent(0.5),
                            ),
                            crate::PanelPoint::new(
                                crate::PanelCoord::end(0.0),
                                crate::PanelCoord::percent(0.5),
                            ),
                        )
                        .width(Px(1.0))
                        .color(Color::WHITE)])),
                    |_| {},
                );
            })
            .build()
            .expect("Fit screen panel with a connector line should build");

        let entity = app.world_mut().spawn(panel).id();

        // One update runs the first solve, where panel.width() is still 0.
        app.update();

        let computed = app
            .world()
            .get::<ComputedDiegeticPanel>(entity)
            .expect("computed panel must exist");
        let shape_commands = computed
            .result()
            .expect("layout result must exist")
            .commands
            .iter()
            .filter(|command| matches!(command.kind, RenderCommandKind::Shapes { .. }))
            .count();
        assert_eq!(
            shape_commands, 1,
            "owner-bounds line must be emitted on the first (zero-viewport) solve",
        );
    }

    #[test]
    fn screen_anchor_offsets_equal_panel_size_for_all_sizing_modes() {
        let mut app = make_app();
        app.world_mut().spawn((
            Window {
                resolution: (1600_u32, 900_u32).into(),
                ..Default::default()
            },
            PrimaryWindow,
        ));
        app.add_plugins(ScreenSpacePlugin);

        let fixed = DiegeticPanel::screen()
            .size(Px(600.0), Px(44.0))
            .anchor(Anchor::BottomRight)
            .layout(|b| {
                b.text(("fixed", TextStyle::new(16.0)));
            })
            .build()
            .expect("fixed screen panel");
        let percent = DiegeticPanel::screen()
            .size(Percent(0.25), Px(200.0))
            .anchor(Anchor::TopRight)
            .layout(|b| {
                b.text(("percent", TextStyle::new(16.0)));
            })
            .build()
            .expect("percent screen panel");
        let fit = DiegeticPanel::screen()
            .size(Fit, Fit)
            .anchor(Anchor::BottomRight)
            .layout(|b| {
                b.text(("fit", TextStyle::new(16.0)));
            })
            .build()
            .expect("fit screen panel");

        let e_fixed = app.world_mut().spawn(fixed).id();
        let e_percent = app.world_mut().spawn(percent).id();
        let e_fit = app.world_mut().spawn(fit).id();

        for _ in 0..5 {
            app.update();
        }

        for (entity, label, anchor) in [
            (e_fixed, "fixed", Anchor::BottomRight),
            (e_percent, "percent", Anchor::TopRight),
            (e_fit, "fit", Anchor::BottomRight),
        ] {
            let panel = app
                .world()
                .get::<DiegeticPanel>(entity)
                .expect("panel component");
            let (ax, ay) = panel.anchor_offsets();
            let (fx, fy) = anchor.offset_fraction();
            let expected_x = panel.width() * fx;
            let expected_y = panel.height() * fy;
            assert!(
                (ax - expected_x).abs() < 0.01,
                "{label}: anchor_offset.x = {ax}, expected {expected_x} \
                 (panel.width={}, fx={fx})",
                panel.width()
            );
            assert!(
                (ay - expected_y).abs() < 0.01,
                "{label}: anchor_offset.y = {ay}, expected {expected_y} \
                 (panel.height={}, fy={fy})",
                panel.height()
            );
        }
    }

    #[test]
    fn screen_window_resize_reuses_scaled_layout_tree_cache() {
        let mut app = make_app();
        let window_entity = app
            .world_mut()
            .spawn((
                Window {
                    resolution: (1600_u32, 900_u32).into(),
                    ..Default::default()
                },
                PrimaryWindow,
            ))
            .id();
        app.add_plugins(ScreenSpacePlugin);

        let panel = DiegeticPanel::screen()
            .size(Percent(0.25), Percent(0.20))
            .layout(|b| {
                b.text(("Resize", TextStyle::new(16.0)));
            })
            .build()
            .expect("percent screen panel should build");

        let panel_entity = app.world_mut().spawn(panel).id();

        app.update();

        let panel = app
            .world()
            .get::<DiegeticPanel>(panel_entity)
            .expect("panel component must exist");
        assert_eq!(panel.width(), 400.0);
        assert_eq!(panel.height(), 180.0);

        let cache = app
            .world()
            .get::<ScaledLayoutTreeCache>(panel_entity)
            .expect("cache component must exist");
        assert_eq!(cache.misses(), 1);
        assert_eq!(cache.hits(), 0);

        {
            let mut window = app
                .world_mut()
                .get_mut::<Window>(window_entity)
                .expect("primary window component must exist");
            window.resolution.set(2000.0, 1000.0);
        }

        app.update();

        let panel = app
            .world()
            .get::<DiegeticPanel>(panel_entity)
            .expect("panel component must exist");
        assert_eq!(panel.width(), 500.0);
        assert_eq!(panel.height(), 200.0);

        let cache = app
            .world()
            .get::<ScaledLayoutTreeCache>(panel_entity)
            .expect("cache component must exist");
        assert_eq!(cache.misses(), 1);
        assert_eq!(cache.hits(), 1);

        let computed = app
            .world()
            .get::<ComputedDiegeticPanel>(panel_entity)
            .expect("computed panel component must exist");
        assert!(computed.result().is_some());
    }

    #[test]
    fn world_fixed_panel_keeps_its_declared_size() {
        let mut app = make_app();

        let panel = DiegeticPanel::world()
            .size(Mm(50.0), Mm(30.0))
            .layout(|b| {
                b.text(("Hi", TextStyle::new(Mm(6.0))));
            })
            .build()
            .expect("fixed-size world panel should build");

        let entity = app.world_mut().spawn(panel).id();
        app.update();
        app.update();

        let panel = app
            .world()
            .get::<DiegeticPanel>(entity)
            .expect("panel component must exist");

        assert_eq!(panel.width(), 50.0);
        assert_eq!(panel.height(), 30.0);
    }

    #[test]
    fn headless_panel_resolves_seeded_font_unit_to_points() {
        let mut app = make_app();

        let panel = DiegeticPanel::world()
            .size(Mm(50.0), Mm(30.0))
            .layout(|b| {
                b.text(("Hi", TextStyle::new(Mm(6.0))));
            })
            .build()
            .expect("headless panel should build");
        let entity = app.world_mut().spawn(panel).id();
        app.update();

        // The panel carries a seeded `Resolved<FontUnit>` from the
        // construction-time `panel_font_unit` (`Points`); `compute_panel_layouts`
        // reads it directly. The `.expect` proves the component is present, so
        // the `defaults.panel_font_unit` fallback in `compute_panel_layouts` is
        // unreached for a panel.
        let resolved = app
            .world()
            .get::<Resolved<FontUnit>>(entity)
            .expect("panel should carry seeded Resolved<FontUnit>");
        assert_eq!(resolved.0.0, Unit::Points);
    }
}
