//! Shared screen-space instrumentation panels for examples.

use bevy::prelude::*;
use bevy_diegetic::AlignX;
use bevy_diegetic::AlignY;
use bevy_diegetic::Anchor;
use bevy_diegetic::DiegeticPanel;
use bevy_diegetic::El;
use bevy_diegetic::Fit;
use bevy_diegetic::GlyphShadowMode;
use bevy_diegetic::LayoutBuilder;
use bevy_diegetic::LayoutTree;
use bevy_diegetic::PanelBuildError;
use bevy_diegetic::Percent;
use bevy_diegetic::Sizing;
use bevy_diegetic::TextStyle;

use super::screen_panel_frame;
use super::screen_panel_material;
use crate::DEFAULT_PANEL_BACKGROUND;

const STATS_HEADER_FONT_SIZE: f32 = 15.0;
const STATS_DESC_FONT_SIZE: f32 = 9.0;
const STATS_DESC_COLOR: Color = Color::srgba(0.60, 0.66, 0.76, 0.68);
const STATS_ROW_WIDTH: f32 = 260.0;
const STATS_INTRA_GAP: f32 = 2.0;
const STATS_GROUP_GAP: f32 = 6.0;
const STATUS_TEXT_COLOR: Color = Color::srgba(1.0, 1.0, 1.0, 0.9);
const STATUS_LABEL_COLOR: Color = Color::srgba(0.7, 0.78, 0.92, 0.85);
const PANEL_SEPARATOR_COLOR: Color = Color::srgba(0.1, 0.4, 0.6, 0.3);
const PANEL_SEPARATOR_THICKNESS: f32 = 1.0;
const GPU_METER_PANEL_WIDTH_FRACTION: f32 = 0.8;

/// One label/value group in a reusable instrumentation stats panel.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct StatsPanelRow {
    /// Left-side row label.
    pub label:   String,
    /// Right-side current value.
    pub value:   String,
    /// Optional explanatory detail lines below the label/value row.
    pub details: Vec<String>,
}

impl StatsPanelRow {
    /// Creates a stats row with no detail lines.
    #[must_use]
    pub fn new(label: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            label:   label.into(),
            value:   value.into(),
            details: Vec::new(),
        }
    }

    /// Adds one detail line below the label/value row.
    #[must_use]
    pub fn detail(mut self, detail: impl Into<String>) -> Self {
        self.details.push(detail.into());
        self
    }

    /// Adds all detail lines below the label/value row.
    #[must_use]
    pub fn details(mut self, details: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.details.extend(details.into_iter().map(Into::into));
        self
    }
}

/// Creates the standard top-right diegetic stats panel.
///
/// The caller owns the counters and update cadence; this helper owns only the
/// Fairy Dust screen-panel styling and row layout.
///
/// # Errors
///
/// Returns [`PanelBuildError`] if the generated screen-space
/// [`DiegeticPanel`] fails layout validation.
pub fn diegetic_stats_panel(rows: &[StatsPanelRow]) -> Result<DiegeticPanel, PanelBuildError> {
    let unlit = screen_panel_material();
    DiegeticPanel::screen()
        .size(Fit, Fit)
        .anchor(Anchor::TopRight)
        .material(unlit.clone())
        .text_material(unlit)
        .with_tree(diegetic_stats_tree(rows))
        .build()
}

/// Creates a bottom-left panel for a frame-time/FPS readout tree.
///
/// The tree is supplied by the example because different examples expose
/// different timing rows.
///
/// # Errors
///
/// Returns [`PanelBuildError`] if the generated screen-space
/// [`DiegeticPanel`] fails layout validation.
pub fn fps_stats_panel(tree: LayoutTree) -> Result<DiegeticPanel, PanelBuildError> {
    let unlit = screen_panel_material();
    DiegeticPanel::screen()
        .size(Fit, Fit)
        .anchor(Anchor::TopLeft)
        .screen_position(0.0, 0.0)
        .material(unlit.clone())
        .text_material(unlit)
        .with_tree(tree)
        .build()
}

/// Creates a bottom-left wide panel for a GPU timing meter tree.
///
/// The caller supplies the meter contents because the exact lane model is
/// example-specific.
///
/// # Errors
///
/// Returns [`PanelBuildError`] if the generated screen-space
/// [`DiegeticPanel`] fails layout validation.
pub fn gpu_meter_panel(tree: LayoutTree) -> Result<DiegeticPanel, PanelBuildError> {
    let unlit = screen_panel_material();
    DiegeticPanel::screen()
        .size(Percent(GPU_METER_PANEL_WIDTH_FRACTION), Fit)
        .anchor(Anchor::BottomLeft)
        .material(unlit.clone())
        .text_material(unlit)
        .with_tree(tree)
        .build()
}

/// Builds the reusable row-group tree used by [`diegetic_stats_panel`].
#[must_use]
pub fn diegetic_stats_tree(rows: &[StatsPanelRow]) -> LayoutTree {
    let mut builder = LayoutBuilder::with_root(El::new().width(Sizing::FIT).height(Sizing::FIT));
    screen_panel_frame(
        &mut builder,
        Sizing::FIT,
        Sizing::FIT,
        DEFAULT_PANEL_BACKGROUND,
        |builder| {
            builder.with(
                El::column()
                    .width(Sizing::FIT)
                    .height(Sizing::FIT)
                    .gap(STATS_GROUP_GAP),
                |builder| {
                    let last = rows.len().saturating_sub(1);
                    for (index, row) in rows.iter().enumerate() {
                        stats_group(builder, row, index == last);
                    }
                },
            );
        },
    );
    builder.build()
}

fn stats_header_label_style() -> TextStyle {
    TextStyle::new(STATS_HEADER_FONT_SIZE)
        .with_color(STATUS_LABEL_COLOR)
        .with_shadow_mode(GlyphShadowMode::None)
}

fn stats_header_value_style() -> TextStyle {
    TextStyle::new(STATS_HEADER_FONT_SIZE)
        .with_color(STATUS_TEXT_COLOR)
        .with_shadow_mode(GlyphShadowMode::None)
}

fn stats_desc_style() -> TextStyle {
    TextStyle::new(STATS_DESC_FONT_SIZE)
        .with_color(STATS_DESC_COLOR)
        .with_shadow_mode(GlyphShadowMode::None)
}

fn stats_group(builder: &mut LayoutBuilder, row: &StatsPanelRow, last: bool) {
    builder.with(
        El::column()
            .width(Sizing::fixed(STATS_ROW_WIDTH))
            .height(Sizing::FIT)
            .gap(STATS_INTRA_GAP),
        |builder| {
            builder.with(
                El::row()
                    .width(Sizing::fixed(STATS_ROW_WIDTH))
                    .height(Sizing::FIT)
                    .alignment(AlignX::Left, AlignY::Center),
                |builder| {
                    builder.with(
                        El::new()
                            .width(Sizing::GROW)
                            .height(Sizing::FIT)
                            .alignment(AlignX::Left, AlignY::Center),
                        |builder| {
                            builder.text(&row.label, stats_header_label_style());
                        },
                    );
                    builder.with(
                        El::new()
                            .width(Sizing::FIT)
                            .height(Sizing::FIT)
                            .alignment(AlignX::Right, AlignY::Center),
                        |builder| {
                            builder.text(&row.value, stats_header_value_style());
                        },
                    );
                },
            );
            for detail in &row.details {
                builder.with(
                    El::new()
                        .width(Sizing::fixed(STATS_ROW_WIDTH))
                        .height(Sizing::FIT)
                        .alignment(AlignX::Left, AlignY::Top),
                    |builder| {
                        builder.text(detail, stats_desc_style());
                    },
                );
            }
            if !last {
                builder.with(
                    El::new()
                        .width(Sizing::fixed(STATS_ROW_WIDTH))
                        .height(Sizing::fixed(PANEL_SEPARATOR_THICKNESS))
                        .background(PANEL_SEPARATOR_COLOR),
                    |_builder| {},
                );
            }
        },
    );
}
