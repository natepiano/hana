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

use super::constants::GPU_METER_PANEL_WIDTH_FRACTION;
use super::constants::PANEL_SEPARATOR_COLOR;
use super::constants::PANEL_SEPARATOR_THICKNESS;
use super::constants::STATS_DESC_COLOR;
use super::constants::STATS_DESC_FONT_SIZE;
use super::constants::STATS_DETAIL_INDENT;
use super::constants::STATS_GROUP_GAP;
use super::constants::STATS_HEADER_FONT_SIZE;
use super::constants::STATS_INTRA_GAP;
use super::constants::STATS_ROW_WIDTH;
use super::constants::STATS_SECTION_FONT_SIZE;
use super::constants::STATS_SECTION_GAP;
use super::constants::STATUS_LABEL_COLOR;
use super::constants::STATUS_TEXT_COLOR;
use super::screen_panel_frame;
use super::screen_panel_material;
use crate::DEFAULT_PANEL_BACKGROUND;

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

/// One named group of related [`StatsPanelRow`] values.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct StatsPanelSection {
    /// Optional section title shown above this section's rows.
    pub title: String,
    /// Rows displayed inside this section.
    pub rows:  Vec<StatsPanelRow>,
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

impl StatsPanelSection {
    /// Creates a section from a title and rows.
    #[must_use]
    pub fn new(title: impl Into<String>, rows: impl IntoIterator<Item = StatsPanelRow>) -> Self {
        Self {
            title: title.into(),
            rows:  rows.into_iter().collect(),
        }
    }

    /// Creates an untitled section from rows.
    #[must_use]
    pub fn untitled(rows: impl IntoIterator<Item = StatsPanelRow>) -> Self {
        Self {
            title: String::new(),
            rows:  rows.into_iter().collect(),
        }
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
    diegetic_stats_sections_panel(&[StatsPanelSection::untitled(rows.iter().cloned())])
}

/// Creates the standard top-right diegetic stats panel from named sections.
///
/// The caller owns the counters and update cadence; this helper owns only the
/// Fairy Dust screen-panel styling and sectioned row layout.
///
/// # Errors
///
/// Returns [`PanelBuildError`] if the generated screen-space
/// [`DiegeticPanel`] fails layout validation.
pub fn diegetic_stats_sections_panel(
    sections: &[StatsPanelSection],
) -> Result<DiegeticPanel, PanelBuildError> {
    let unlit = screen_panel_material();
    DiegeticPanel::screen()
        .size(Fit, Fit)
        .anchor(Anchor::TopRight)
        .material(unlit.clone())
        .text_material(unlit)
        .with_tree(diegetic_stats_sections_tree(sections))
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
    diegetic_stats_sections_tree(&[StatsPanelSection::untitled(rows.iter().cloned())])
}

/// Builds the reusable sectioned row tree used by
/// [`diegetic_stats_sections_panel`].
#[must_use]
pub fn diegetic_stats_sections_tree(sections: &[StatsPanelSection]) -> LayoutTree {
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
                    .gap(STATS_SECTION_GAP),
                |builder| {
                    let last = sections.len().saturating_sub(1);
                    for (index, section) in sections.iter().enumerate() {
                        stats_section(builder, section, index == last);
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

fn stats_section_label_style() -> TextStyle {
    TextStyle::new(STATS_SECTION_FONT_SIZE)
        .bold()
        .with_color(STATUS_LABEL_COLOR)
        .with_shadow_mode(GlyphShadowMode::None)
}

fn stats_section(builder: &mut LayoutBuilder, section: &StatsPanelSection, last: bool) {
    builder.with(
        El::column()
            .width(Sizing::fixed(STATS_ROW_WIDTH))
            .height(Sizing::FIT)
            .gap(STATS_GROUP_GAP),
        |builder| {
            if !section.title.is_empty() {
                builder.with(
                    El::new()
                        .width(Sizing::fixed(STATS_ROW_WIDTH))
                        .height(Sizing::FIT)
                        .alignment(AlignX::Left, AlignY::Center),
                    |builder| {
                        builder.text(&section.title, stats_section_label_style());
                    },
                );
            }
            for row in &section.rows {
                stats_group(builder, row);
            }
            if !last {
                stats_separator(builder);
            }
        },
    );
}

fn stats_group(builder: &mut LayoutBuilder, row: &StatsPanelRow) {
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
                        .padding(bevy_diegetic::Padding::new(
                            STATS_DETAIL_INDENT,
                            0.0,
                            0.0,
                            0.0,
                        ))
                        .alignment(AlignX::Left, AlignY::Top),
                    |builder| {
                        builder.text(detail, stats_desc_style());
                    },
                );
            }
        },
    );
}

fn stats_separator(builder: &mut LayoutBuilder) {
    builder.with(
        El::new()
            .width(Sizing::fixed(STATS_ROW_WIDTH))
            .height(Sizing::fixed(PANEL_SEPARATOR_THICKNESS))
            .background(PANEL_SEPARATOR_COLOR),
        |_builder| {},
    );
}
