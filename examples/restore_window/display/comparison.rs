use bevy::prelude::*;
use bevy::window::WindowPosition;
use bevy_kana::ToI32;
use bevy_kana::ToU32;
use bevy_window_manager::CurrentMonitor;

use super::super::constants::ACTUAL_COLUMN_TITLE;
use super::super::constants::AUTOMATIC_TEXT;
use super::super::constants::COMPARISON_COLUMN_PADDING;
use super::super::constants::CURRENT_COLUMN_TITLE;
use super::super::constants::DEFAULT_COLOR;
use super::super::constants::EFFECTIVE_MODE_LABEL;
use super::super::constants::EXPECTED_COLUMN_TITLE;
use super::super::constants::LABEL_WIDTH;
use super::super::constants::MIN_COMPARISON_COLUMN_WIDTH;
use super::super::constants::MISMATCH_COLOR;
use super::super::constants::MISMATCH_WARN_COLOR;
use super::super::constants::MODE_LABEL;
use super::super::constants::MONITOR_LABEL;
use super::super::constants::NO_RESTORE_DATA_TEXT;
use super::super::constants::NONE_TEXT;
use super::super::constants::POSITION_LOGICAL_LABEL;
use super::super::constants::POSITION_PHYSICAL_LABEL;
use super::super::constants::RESTORED_COLUMN_TITLE;
use super::super::constants::SCALE_LABEL;
use super::super::constants::SIZE_LOGICAL_LABEL;
use super::super::constants::SIZE_PHYSICAL_LABEL;
use super::super::events::CachedMismatchState;
use super::super::events::CachedRestoredState;

struct CurrentValues {
    physical_position: String,
    logical_position:  String,
    physical_size:     String,
    logical_size:      String,
    scale:             String,
    monitor:           String,
    mode:              String,
}

struct ComparisonMismatch {
    expected: String,
    actual:   String,
}

struct ComparisonRow<'a> {
    label:    &'a str,
    restored: String,
    current:  String,
    mismatch: Option<ComparisonMismatch>,
}

#[derive(Clone, Copy)]
enum ComparisonLayout {
    CurrentOnly,
    WithMismatchColumns,
}

struct RestoredValues {
    physical_position: String,
    logical_position:  String,
    physical_size:     String,
    logical_size:      String,
    monitor:           String,
    mode:              String,
}

impl From<&CachedRestoredState> for RestoredValues {
    fn from(cached_restored_state: &CachedRestoredState) -> Self {
        let physical_size = cached_restored_state.physical_size;
        let logical_size = cached_restored_state.logical_size;
        Self {
            physical_position: cached_restored_state.physical_position.map_or_else(
                || NONE_TEXT.to_string(),
                |position| format!("({}, {})", position.x, position.y),
            ),
            logical_position:  cached_restored_state.logical_position.map_or_else(
                || NONE_TEXT.to_string(),
                |position| format!("({}, {})", position.x, position.y),
            ),
            physical_size:     format!("{}x{}", physical_size.x, physical_size.y),
            logical_size:      format!("{}x{}", logical_size.x, logical_size.y),
            monitor:           cached_restored_state.monitor.to_string(),
            mode:              format!("{:?}", cached_restored_state.mode),
        }
    }
}

impl RestoredValues {
    fn comparison_width(&self) -> usize {
        [
            self.physical_position.len(),
            self.logical_position.len(),
            self.physical_size.len(),
            self.logical_size.len(),
            self.monitor.len(),
            self.mode.len(),
        ]
        .into_iter()
        .max()
        .unwrap_or(0)
            + COMPARISON_COLUMN_PADDING
    }
}

/// Build comparison spans (restored vs current) for a window and add them as `TextSpan` children.
pub(super) fn build_comparison_spans(
    child_spawner: &mut ChildSpawnerCommands,
    restored_state: Option<&CachedRestoredState>,
    mismatch_state: Option<&CachedMismatchState>,
    window: &Window,
    monitor: &CurrentMonitor,
    font: &TextFont,
) {
    let effective_mode = monitor.effective_mode;
    let scale = window.resolution.scale_factor();

    let current_values = CurrentValues {
        physical_position: match window.position {
            WindowPosition::At(position) => format!("({}, {})", position.x, position.y),
            _ => AUTOMATIC_TEXT.to_string(),
        },
        logical_position:  match window.position {
            WindowPosition::At(position) => {
                let logical_x = (f64::from(position.x) / f64::from(scale)).round().to_i32();
                let logical_y = (f64::from(position.y) / f64::from(scale)).round().to_i32();
                format!("({logical_x}, {logical_y})")
            },
            _ => AUTOMATIC_TEXT.to_string(),
        },
        physical_size:     format!("{}x{}", window.physical_width(), window.physical_height()),
        logical_size:      format!(
            "{}x{}",
            window.resolution.width().to_u32(),
            window.resolution.height().to_u32()
        ),
        scale:             format!("{scale}"),
        monitor:           format!("{}", monitor.index),
        mode:              format!("{effective_mode:?}"),
    };

    if let Some(cached_restored_state) = restored_state {
        build_restored_spans(
            child_spawner,
            cached_restored_state,
            mismatch_state,
            &current_values,
            font,
        );
    } else {
        build_current_only_spans(child_spawner, &current_values, font);
    }

    add_span(
        child_spawner,
        font,
        &format!("\n{EFFECTIVE_MODE_LABEL} {effective_mode:?}\n"),
        DEFAULT_COLOR,
    );
}

/// Render comparison rows when restore data is available.
fn build_restored_spans(
    child_spawner: &mut ChildSpawnerCommands,
    cached_restored_state: &CachedRestoredState,
    mismatch_state: Option<&CachedMismatchState>,
    current_values: &CurrentValues,
    font: &TextFont,
) {
    let restored_values = RestoredValues::from(cached_restored_state);
    let column_width = restored_values
        .comparison_width()
        .max(MIN_COMPARISON_COLUMN_WIDTH);
    let layout = if mismatch_state.is_some() {
        ComparisonLayout::WithMismatchColumns
    } else {
        ComparisonLayout::CurrentOnly
    };

    add_restored_header(child_spawner, font, layout, column_width);
    add_position_rows(
        child_spawner,
        font,
        &restored_values,
        current_values,
        mismatch_state,
        column_width,
    );
    add_size_rows(
        child_spawner,
        font,
        &restored_values,
        current_values,
        mismatch_state,
        column_width,
    );
    add_scale_row(
        child_spawner,
        font,
        current_values,
        mismatch_state,
        column_width,
    );
    add_monitor_row(
        child_spawner,
        font,
        &restored_values,
        current_values,
        mismatch_state,
        column_width,
    );
    add_mode_row(
        child_spawner,
        font,
        &restored_values,
        current_values,
        mismatch_state,
        column_width,
    );
}

fn add_restored_header(
    child_spawner: &mut ChildSpawnerCommands,
    font: &TextFont,
    layout: ComparisonLayout,
    column_width: usize,
) {
    let header = if matches!(layout, ComparisonLayout::WithMismatchColumns) {
        format!(
            "{:LABEL_WIDTH$}{:<column_width$}{:<column_width$}{:<column_width$}{}\n",
            "",
            RESTORED_COLUMN_TITLE,
            CURRENT_COLUMN_TITLE,
            EXPECTED_COLUMN_TITLE,
            ACTUAL_COLUMN_TITLE
        )
    } else {
        format!(
            "{:LABEL_WIDTH$}{:<column_width$}{}\n",
            "", RESTORED_COLUMN_TITLE, CURRENT_COLUMN_TITLE
        )
    };
    add_span(child_spawner, font, &header, DEFAULT_COLOR);
}

fn add_position_rows(
    child_spawner: &mut ChildSpawnerCommands,
    font: &TextFont,
    restored_values: &RestoredValues,
    current_values: &CurrentValues,
    mismatch_state: Option<&CachedMismatchState>,
    column_width: usize,
) {
    add_row(
        child_spawner,
        font,
        &ComparisonRow {
            label:    POSITION_PHYSICAL_LABEL,
            restored: restored_values.physical_position.clone(),
            current:  current_values.physical_position.clone(),
            mismatch: mismatch_state.map(|mismatch| ComparisonMismatch {
                expected: mismatch.physical_position.expected.map_or_else(
                    || NONE_TEXT.to_string(),
                    |position| format!("({}, {})", position.x, position.y),
                ),
                actual:   mismatch.physical_position.actual.map_or_else(
                    || NONE_TEXT.to_string(),
                    |position| format!("({}, {})", position.x, position.y),
                ),
            }),
        },
        column_width,
    );
    add_row(
        child_spawner,
        font,
        &ComparisonRow {
            label:    POSITION_LOGICAL_LABEL,
            restored: restored_values.logical_position.clone(),
            current:  current_values.logical_position.clone(),
            mismatch: mismatch_state.map(|mismatch| ComparisonMismatch {
                expected: mismatch.logical_position.expected.map_or_else(
                    || NONE_TEXT.to_string(),
                    |position| format!("({}, {})", position.x, position.y),
                ),
                actual:   mismatch.logical_position.actual.map_or_else(
                    || NONE_TEXT.to_string(),
                    |position| format!("({}, {})", position.x, position.y),
                ),
            }),
        },
        column_width,
    );
}

fn add_size_rows(
    child_spawner: &mut ChildSpawnerCommands,
    font: &TextFont,
    restored_values: &RestoredValues,
    current_values: &CurrentValues,
    mismatch_state: Option<&CachedMismatchState>,
    column_width: usize,
) {
    add_row(
        child_spawner,
        font,
        &ComparisonRow {
            label:    SIZE_PHYSICAL_LABEL,
            restored: restored_values.physical_size.clone(),
            current:  current_values.physical_size.clone(),
            mismatch: mismatch_state.map(|mismatch| ComparisonMismatch {
                expected: format!(
                    "{}x{}",
                    mismatch.physical_size.expected.x, mismatch.physical_size.expected.y
                ),
                actual:   format!(
                    "{}x{}",
                    mismatch.physical_size.actual.x, mismatch.physical_size.actual.y
                ),
            }),
        },
        column_width,
    );
    add_row(
        child_spawner,
        font,
        &ComparisonRow {
            label:    SIZE_LOGICAL_LABEL,
            restored: restored_values.logical_size.clone(),
            current:  current_values.logical_size.clone(),
            mismatch: mismatch_state.map(|mismatch| ComparisonMismatch {
                expected: format!(
                    "{}x{}",
                    mismatch.logical_size.expected.x, mismatch.logical_size.expected.y
                ),
                actual:   format!(
                    "{}x{}",
                    mismatch.logical_size.actual.x, mismatch.logical_size.actual.y
                ),
            }),
        },
        column_width,
    );
}

fn add_scale_row(
    child_spawner: &mut ChildSpawnerCommands,
    font: &TextFont,
    current_values: &CurrentValues,
    mismatch_state: Option<&CachedMismatchState>,
    column_width: usize,
) {
    if mismatch_state.is_none() {
        add_span(
            child_spawner,
            font,
            &format!(
                "{:<LABEL_WIDTH$}{:<column_width$}{}\n",
                SCALE_LABEL, "", current_values.scale
            ),
            DEFAULT_COLOR,
        );
        return;
    }

    let row = ComparisonRow {
        label:    SCALE_LABEL,
        restored: String::new(),
        current:  current_values.scale.clone(),
        mismatch: mismatch_state.map(|mismatch| ComparisonMismatch {
            expected: mismatch.scale.expected.to_string(),
            actual:   mismatch.scale.actual.to_string(),
        }),
    };
    add_row(child_spawner, font, &row, column_width);
}

fn add_monitor_row(
    child_spawner: &mut ChildSpawnerCommands,
    font: &TextFont,
    restored_values: &RestoredValues,
    current_values: &CurrentValues,
    mismatch_state: Option<&CachedMismatchState>,
    column_width: usize,
) {
    add_row(
        child_spawner,
        font,
        &ComparisonRow {
            label:    MONITOR_LABEL,
            restored: restored_values.monitor.clone(),
            current:  current_values.monitor.clone(),
            mismatch: mismatch_state.map(|mismatch| ComparisonMismatch {
                expected: mismatch.monitor.expected.to_string(),
                actual:   mismatch.monitor.actual.to_string(),
            }),
        },
        column_width,
    );
}

fn add_mode_row(
    child_spawner: &mut ChildSpawnerCommands,
    font: &TextFont,
    restored_values: &RestoredValues,
    current_values: &CurrentValues,
    mismatch_state: Option<&CachedMismatchState>,
    column_width: usize,
) {
    add_row(
        child_spawner,
        font,
        &ComparisonRow {
            label:    MODE_LABEL,
            restored: restored_values.mode.clone(),
            current:  current_values.mode.clone(),
            mismatch: mismatch_state.map(|mismatch| ComparisonMismatch {
                expected: format!("{:?}", mismatch.mode.expected),
                actual:   format!("{:?}", mismatch.mode.actual),
            }),
        },
        column_width,
    );
}

/// Render current-only values when no restore data exists.
fn build_current_only_spans(
    child_spawner: &mut ChildSpawnerCommands,
    current_values: &CurrentValues,
    font: &TextFont,
) {
    add_span(child_spawner, font, NO_RESTORE_DATA_TEXT, MISMATCH_COLOR);
    add_span(
        child_spawner,
        font,
        &format!(
            "{:<LABEL_WIDTH$}{}\n",
            POSITION_PHYSICAL_LABEL, current_values.physical_position
        ),
        DEFAULT_COLOR,
    );
    add_span(
        child_spawner,
        font,
        &format!(
            "{:<LABEL_WIDTH$}{}\n",
            POSITION_LOGICAL_LABEL, current_values.logical_position
        ),
        DEFAULT_COLOR,
    );
    add_span(
        child_spawner,
        font,
        &format!(
            "{:<LABEL_WIDTH$}{}\n",
            SIZE_PHYSICAL_LABEL, current_values.physical_size
        ),
        DEFAULT_COLOR,
    );
    add_span(
        child_spawner,
        font,
        &format!(
            "{:<LABEL_WIDTH$}{}\n",
            SIZE_LOGICAL_LABEL, current_values.logical_size
        ),
        DEFAULT_COLOR,
    );
    add_span(
        child_spawner,
        font,
        &format!("{:<LABEL_WIDTH$}{}\n", SCALE_LABEL, current_values.scale),
        DEFAULT_COLOR,
    );
    add_span(
        child_spawner,
        font,
        &format!(
            "{:<LABEL_WIDTH$}{}\n",
            MONITOR_LABEL, current_values.monitor
        ),
        DEFAULT_COLOR,
    );
    add_span(
        child_spawner,
        font,
        &format!("{:<LABEL_WIDTH$}{}\n", MODE_LABEL, current_values.mode),
        DEFAULT_COLOR,
    );
}

/// Add a comparison row, dispatching to 3-column or 5-column layout based on mismatch data.
fn add_row(
    child_spawner: &mut ChildSpawnerCommands,
    font: &TextFont,
    row: &ComparisonRow<'_>,
    column_width: usize,
) {
    if let Some(mismatch) = row.mismatch.as_ref() {
        add_extended_comparison_row(child_spawner, font, row, mismatch, column_width);
    } else {
        add_standard_comparison_row(child_spawner, font, row, column_width);
    }
}

/// Add a comparison row: label + file value (white) + current value (white or red if mismatch).
fn add_standard_comparison_row(
    child_spawner: &mut ChildSpawnerCommands,
    font: &TextFont,
    row: &ComparisonRow<'_>,
    column_width: usize,
) {
    let color = if row.restored == row.current {
        DEFAULT_COLOR
    } else {
        MISMATCH_COLOR
    };

    // Label + file value (always white)
    add_span(
        child_spawner,
        font,
        &format!(
            "{label:<LABEL_WIDTH$}{restored:<column_width$}",
            label = row.label,
            restored = row.restored
        ),
        DEFAULT_COLOR,
    );
    // Current value (colored)
    add_span(child_spawner, font, &format!("{}\n", row.current), color);
}

/// Add a 5-column comparison row: label + restored + current + expected + actual.
/// Expected/actual columns use warning color when they differ.
fn add_extended_comparison_row(
    child_spawner: &mut ChildSpawnerCommands,
    font: &TextFont,
    row: &ComparisonRow<'_>,
    mismatch: &ComparisonMismatch,
    column_width: usize,
) {
    let current_color = if row.restored == row.current {
        DEFAULT_COLOR
    } else {
        MISMATCH_COLOR
    };
    let mismatch_color = if mismatch.expected == mismatch.actual {
        DEFAULT_COLOR
    } else {
        MISMATCH_WARN_COLOR
    };

    // Label + restored value (always white)
    add_span(
        child_spawner,
        font,
        &format!(
            "{label:<LABEL_WIDTH$}{restored:<column_width$}",
            label = row.label,
            restored = row.restored
        ),
        DEFAULT_COLOR,
    );
    // Current value
    add_span(
        child_spawner,
        font,
        &format!("{current:<column_width$}", current = row.current),
        current_color,
    );
    // Expected value (always white)
    add_span(
        child_spawner,
        font,
        &format!("{expected:<column_width$}", expected = mismatch.expected),
        DEFAULT_COLOR,
    );
    // Actual value (warning color if mismatch)
    add_span(
        child_spawner,
        font,
        &format!("{}\n", mismatch.actual),
        mismatch_color,
    );
}

/// Add a single `TextSpan` child.
pub(super) fn add_span(
    child_spawner: &mut ChildSpawnerCommands,
    font: &TextFont,
    text: &str,
    color: Color,
) {
    child_spawner.spawn((TextSpan(text.to_string()), font.clone(), TextColor(color)));
}
