//! [`compute_panel_layouts`] — recomputes layout for changed panels, plus
//! [`resolve_world_panel_fit`] — shrinks `Fit`-axis world panels to their
//! content bounds after layout runs.

use std::sync::Arc;
use std::sync::Mutex;
use std::sync::PoisonError;
use std::time::Instant;

use bevy::prelude::*;

use super::constants::PANEL_RESIZE_EPSILON;
use super::coordinate_space::CoordinateSpace;
use super::diegetic_panel::ComputedDiegeticPanel;
use super::diegetic_panel::DiegeticPanel;
use super::diegetic_panel::DiegeticPanelChangeClassification;
use super::diegetic_panel::PanelFontUnit;
use super::diegetic_panel::ScaledLayoutTreeCache;
use super::perf::DiegeticPerfStats;
use crate::cascade::CascadeDefaults;
use crate::cascade::CascadeTarget;
use crate::cascade::Resolved;
use crate::constants::MILLISECONDS_PER_SECOND;
use crate::layout::LayoutEngine;
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
    panel_font_units: Query<&Resolved<PanelFontUnit>>,
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
        let font_unit = panel_font_units.get(entity).map_or_else(
            |_| PanelFontUnit::global_default(&defaults).0,
            |resolved| resolved.0.0,
        );
        let layout_to_points = layout_unit.to_points();
        let font_to_points = font_unit.to_points();

        let pending_change = tree_change.take();
        if matches!(pending_change, Some(LayoutTreeChange::Identical))
            && computed.result().is_some()
        {
            continue;
        }

        let scaled_tree = scaled_tree_cache.get_or_update(
            panel_ref.tree(),
            panel_ref.tree_revision(),
            layout_to_points,
            font_to_points,
        );

        if matches!(pending_change, Some(LayoutTreeChange::VisualOnly))
            && let Some(result) = computed.result_mut()
        {
            result.regenerate_commands(scaled_tree);
            panel_count += 1;
            continue;
        }

        let engine = LayoutEngine::new(Arc::clone(&cached_measure));
        let result = engine.compute(
            scaled_tree,
            panel_ref.width() * layout_to_points,
            panel_ref.height() * layout_to_points,
            1.0,
        );

        if let Some(bounds) = result.content_bounds() {
            let s = panel_ref.points_to_world();
            computed.set_content_size(bounds.width * s, bounds.height * s);
        }

        computed.set_result(result);
        panel_count += 1;
    }

    perf.compute_ms = if panel_count == 0 {
        0.0
    } else {
        start.elapsed().as_secs_f32() * MILLISECONDS_PER_SECOND
    };
    perf.compute_panels = panel_count;
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
    mut panels: Query<(&mut DiegeticPanel, &ComputedDiegeticPanel)>,
) {
    for (mut panel, computed) in &mut panels {
        let (w_sizing, h_sizing) = match panel.coordinate_space() {
            CoordinateSpace::World { width, height } => (*width, *height),
            CoordinateSpace::Screen { .. } => continue,
        };
        let Some(bounds) = computed.content_bounds() else {
            continue;
        };
        let layout_to_points = panel.layout_unit().to_points();
        if layout_to_points <= 0.0 {
            continue;
        }
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
    use crate::LayoutTextStyle;
    use crate::Mm;
    use crate::Percent;
    use crate::Px;
    use crate::constants::MONOSPACE_WIDTH_RATIO;
    use crate::layout::LayoutBuilder;
    use crate::layout::LayoutTree;
    use crate::layout::RenderCommandKind;
    use crate::layout::TextDimensions;
    use crate::layout::TextMeasure;
    use crate::panel::ComputedDiegeticPanel;
    use crate::panel::DiegeticPanel;
    use crate::panel::DiegeticPanelCommands;
    use crate::panel::HeadlessLayoutPlugin;
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

    fn colored_text_tree(text: &str, color: Color) -> LayoutTree {
        let mut builder = LayoutBuilder::new(100.0, 50.0);
        builder.text(text, LayoutTextStyle::new(10.0).with_color(color));
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
                b.text("Hello", LayoutTextStyle::new(Mm(6.0)));
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
                b.text("Hello", LayoutTextStyle::new(Mm(6.0)));
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
                b.text("HelloHello", LayoutTextStyle::new(Mm(6.0)));
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
                        b.text("Hello", LayoutTextStyle::new(16.0));
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
                b.text("fixed", LayoutTextStyle::new(16.0));
            })
            .build()
            .expect("fixed screen panel");
        let percent = DiegeticPanel::screen()
            .size(Percent(0.25), Px(200.0))
            .anchor(Anchor::TopRight)
            .layout(|b| {
                b.text("percent", LayoutTextStyle::new(16.0));
            })
            .build()
            .expect("percent screen panel");
        let fit = DiegeticPanel::screen()
            .size(Fit, Fit)
            .anchor(Anchor::BottomRight)
            .layout(|b| {
                b.text("fit", LayoutTextStyle::new(16.0));
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
                b.text("Resize", LayoutTextStyle::new(16.0));
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
                b.text("Hi", LayoutTextStyle::new(Mm(6.0)));
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
}
