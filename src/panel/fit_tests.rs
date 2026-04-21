//! Integration-style tests for `Fit` resolution on world and screen
//! panels (B‴ step 9).
//!
//! Spawns a world panel with `.size(Fit, Fit)` in a minimal Bevy `App`,
//! runs `compute_panel_layouts` + `resolve_world_panel_fit`, and verifies
//! that `panel.width()` / `panel.height()` shrink to the content bounds
//! on the second tick.

#![allow(
    clippy::float_cmp,
    reason = "tests compare exact expected layout values"
)]
#![allow(
    clippy::expect_used,
    reason = "tests should panic on unexpected values"
)]

use std::sync::Arc;

use bevy::prelude::*;
use bevy_kana::ToF32;

use super::DiegeticPanel;
use super::HeadlessLayoutPlugin;
use crate::Anchor;
use crate::Fit;
use crate::FitMax;
use crate::LayoutTextStyle;
use crate::Mm;
use crate::constants::MONOSPACE_WIDTH_RATIO;
use crate::layout::TextDimensions;
use crate::layout::TextMeasure;
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

#[test]
fn world_fit_panel_shrinks_to_content_bounds_in_meters() {
    let mut app = make_app();

    // `.size(Fit, Fit)` with both axes `AnyUnit` falls back to the mode
    // default — `Unit::Meters` for world panels. The engine and our
    // monospace measurer operate on the internal point values regardless
    // of layout unit, so expect results in meters: 18 mm → 0.018 m,
    // 6 mm → 0.006 m.
    let panel = DiegeticPanel::world()
        .size(Fit, Fit)
        .layout(|b| {
            b.text("Hello", LayoutTextStyle::new(Mm(6.0)));
        })
        .build()
        .expect("Fit world panel should build even at zero initial size");

    let entity = app.world_mut().spawn(panel).id();

    // Tick 1: layout runs against panel.width/height = 0 (Fit root), so the
    // engine resolves the root to content bounds. `resolve_world_panel_fit`
    // then pulls those content bounds back into panel.width/height.
    app.update();
    // Tick 2: stable state — content bounds should equal panel dimensions.
    app.update();

    let panel = app
        .world()
        .get::<DiegeticPanel>(entity)
        .expect("panel component must exist");

    // The panel should have shrunk from its initial 0 × 0 placeholder.
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

    // 5 chars * 6 mm * 0.6 = 18 mm → 0.018 m in the Meters layout unit.
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

    // `.size(Fit, FitMax(Mm(..)))`: the `FitMax(Mm)` side fixes the layout
    // unit to millimetres via `CompatibleUnits` = `(AnyUnit, Millimeters)`.
    // That means panel.width() / height() now report in mm.
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

    // Text width = 10 chars * 6 * 0.6 = 36 mm; cap width at 20 mm.
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
    // Mirror the text_alpha camera-help structure: `.size(Fit, Fit)` with
    // GROW wrapper children that bottom out in text. If the screen
    // resolve + engine pass + change-detection loop doesn't converge to a
    // sensible non-zero size, this test fails.
    use bevy::window::PrimaryWindow;
    use bevy::window::Window;

    let mut app = make_app();
    // position_screen_space_panels needs a primary window.
    app.world_mut().spawn((
        Window {
            resolution: (1600_u32, 900_u32).into(),
            ..Default::default()
        },
        PrimaryWindow,
    ));
    // And the ScreenSpacePlugin, so `position_screen_space_panels` runs.
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

    // Several ticks to give the feedback loop (panel.width → engine →
    // content_bounds → resolve_screen_axis → panel.width) time to settle.
    for _ in 0..5 {
        app.update();
    }

    let panel = app
        .world()
        .get::<DiegeticPanel>(entity)
        .expect("panel component must exist");

    // Shrinks to content — far smaller than the 1600×900 window.
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

    // Panel defaults to Anchor::TopLeft (fraction 0,0), so both axes
    // of anchor_offsets should be ~0. A non-zero value would indicate
    // the screen-mode short-circuit isn't honoring anchor fractions.
    let (ax, ay) = panel.anchor_offsets();
    assert!(
        ax.abs() < 0.01 && ay.abs() < 0.01,
        "TopLeft anchor_offsets = ({ax}, {ay}), expected ~(0, 0)",
    );
}

#[test]
fn screen_anchor_offsets_equal_panel_size_for_all_sizing_modes() {
    // Regression test: the renderer uses `anchor_offsets` to place panel
    // content relative to its anchor point. For screen panels under the
    // ortho camera (1 px = 1 world unit), anchor offsets must equal
    // `panel.size × anchor_fraction` regardless of whether axes are
    // Fixed, Percent, or Fit — otherwise panels render at the wrong
    // position (e.g. camera panel floating far off-screen after the
    // `world_height` freeze only applied to fully-fixed panels).
    use bevy::window::PrimaryWindow;
    use bevy::window::Window;

    use crate::Fit;
    use crate::Percent;
    use crate::Px;

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
