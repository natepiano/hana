use bevy::prelude::*;
use bevy::window::Monitor;
use bevy::window::PrimaryWindow;
use bevy::window::WindowPosition;
use bevy_kana::ToI32;
use bevy_kana::ToU32;
use bevy_window_manager::CurrentMonitor;
use bevy_window_manager::ManagedWindow;
use bevy_window_manager::ManagedWindowPersistence;
use bevy_window_manager::Monitors;

use super::constants::ACTUAL_COLUMN_TITLE;
use super::constants::AUTOMATIC_TEXT;
use super::constants::COMPARISON_COLUMN_PADDING;
use super::constants::CURRENT_COLUMN_TITLE;
use super::constants::DEFAULT_COLOR;
use super::constants::EFFECTIVE_MODE_LABEL;
use super::constants::EXPECTED_COLUMN_TITLE;
use super::constants::FONT_SIZE;
use super::constants::LABEL_WIDTH;
use super::constants::MANAGED_WINDOWS_HEADER;
use super::constants::MIN_COMPARISON_COLUMN_WIDTH;
use super::constants::MISMATCH_COLOR;
use super::constants::MISMATCH_WARN_COLOR;
use super::constants::MODE_LABEL;
use super::constants::MONITOR_LABEL;
use super::constants::NO_MANAGED_WINDOWS_TEXT;
use super::constants::NO_RESTORE_DATA_TEXT;
use super::constants::NONE_TEXT;
use super::constants::POSITION_LOGICAL_LABEL;
use super::constants::POSITION_PHYSICAL_LABEL;
use super::constants::RESTORED_COLUMN_TITLE;
use super::constants::SCALE_LABEL;
use super::constants::SECONDARY_WINDOW_NAME_LABEL;
use super::constants::SIZE_LOGICAL_LABEL;
use super::constants::SIZE_PHYSICAL_LABEL;
use super::constants::UNKNOWN_MANAGED_WINDOW_NAME;
use super::constants::VIDEO_MODES_HEADER;
use super::events::CachedMismatchState;
use super::events::CachedRestoredState;
use super::events::MismatchStates;
use super::events::RestoredStates;
use super::input;
use super::state::PrimaryDisplay;
use super::state::SecondaryDisplay;
use super::state::SelectedVideoModes;

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
fn build_comparison_spans(
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
    let col_width = restored_values
        .comparison_width()
        .max(MIN_COMPARISON_COLUMN_WIDTH);
    let layout = if mismatch_state.is_some() {
        ComparisonLayout::WithMismatchColumns
    } else {
        ComparisonLayout::CurrentOnly
    };

    add_restored_header(child_spawner, font, layout, col_width);
    add_position_rows(
        child_spawner,
        font,
        &restored_values,
        current_values,
        mismatch_state,
        col_width,
    );
    add_size_rows(
        child_spawner,
        font,
        &restored_values,
        current_values,
        mismatch_state,
        col_width,
    );
    add_scale_row(
        child_spawner,
        font,
        current_values,
        mismatch_state,
        col_width,
    );
    add_monitor_row(
        child_spawner,
        font,
        &restored_values,
        current_values,
        mismatch_state,
        col_width,
    );
    add_mode_row(
        child_spawner,
        font,
        &restored_values,
        current_values,
        mismatch_state,
        col_width,
    );
}

fn add_restored_header(
    child_spawner: &mut ChildSpawnerCommands,
    font: &TextFont,
    layout: ComparisonLayout,
    col_width: usize,
) {
    let header = if matches!(layout, ComparisonLayout::WithMismatchColumns) {
        format!(
            "{:LABEL_WIDTH$}{:<col_width$}{:<col_width$}{:<col_width$}{}\n",
            "",
            RESTORED_COLUMN_TITLE,
            CURRENT_COLUMN_TITLE,
            EXPECTED_COLUMN_TITLE,
            ACTUAL_COLUMN_TITLE
        )
    } else {
        format!(
            "{:LABEL_WIDTH$}{:<col_width$}{}\n",
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
    col_width: usize,
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
        col_width,
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
        col_width,
    );
}

fn add_size_rows(
    child_spawner: &mut ChildSpawnerCommands,
    font: &TextFont,
    restored_values: &RestoredValues,
    current_values: &CurrentValues,
    mismatch_state: Option<&CachedMismatchState>,
    col_width: usize,
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
        col_width,
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
        col_width,
    );
}

fn add_scale_row(
    child_spawner: &mut ChildSpawnerCommands,
    font: &TextFont,
    current_values: &CurrentValues,
    mismatch_state: Option<&CachedMismatchState>,
    col_width: usize,
) {
    if mismatch_state.is_none() {
        add_span(
            child_spawner,
            font,
            &format!(
                "{:<LABEL_WIDTH$}{:<col_width$}{}\n",
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
    add_row(child_spawner, font, &row, col_width);
}

fn add_monitor_row(
    child_spawner: &mut ChildSpawnerCommands,
    font: &TextFont,
    restored_values: &RestoredValues,
    current_values: &CurrentValues,
    mismatch_state: Option<&CachedMismatchState>,
    col_width: usize,
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
        col_width,
    );
}

fn add_mode_row(
    child_spawner: &mut ChildSpawnerCommands,
    font: &TextFont,
    restored_values: &RestoredValues,
    current_values: &CurrentValues,
    mismatch_state: Option<&CachedMismatchState>,
    col_width: usize,
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
        col_width,
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
    col_width: usize,
) {
    if let Some(mismatch) = row.mismatch.as_ref() {
        add_extended_comparison_row(child_spawner, font, row, mismatch, col_width);
    } else {
        add_standard_comparison_row(child_spawner, font, row, col_width);
    }
}

/// Add a comparison row: label + file value (white) + current value (white or red if mismatch).
fn add_standard_comparison_row(
    child_spawner: &mut ChildSpawnerCommands,
    font: &TextFont,
    row: &ComparisonRow<'_>,
    col_width: usize,
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
            "{label:<LABEL_WIDTH$}{restored:<col_width$}",
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
    col_width: usize,
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
            "{label:<LABEL_WIDTH$}{restored:<col_width$}",
            label = row.label,
            restored = row.restored
        ),
        DEFAULT_COLOR,
    );
    // Current value
    add_span(
        child_spawner,
        font,
        &format!("{current:<col_width$}", current = row.current),
        current_color,
    );
    // Expected value (always white)
    add_span(
        child_spawner,
        font,
        &format!("{expected:<col_width$}", expected = mismatch.expected),
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
fn add_span(child_spawner: &mut ChildSpawnerCommands, font: &TextFont, text: &str, color: Color) {
    child_spawner.spawn((TextSpan(text.to_string()), font.clone(), TextColor(color)));
}

// --- Primary Window Display ---

#[expect(
    clippy::too_many_arguments,
    reason = "Bevy system — each param is a distinct system resource"
)]
pub(crate) fn update_primary_display(
    primary_display: Single<Entity, With<PrimaryDisplay>>,
    window_query: Single<(Entity, &Window, &CurrentMonitor), With<PrimaryWindow>>,
    monitors: Res<Monitors>,
    bevy_monitors: Query<(Entity, &Monitor)>,
    mut selected: ResMut<SelectedVideoModes>,
    persistence: Res<ManagedWindowPersistence>,
    managed_query: Query<(&Window, &ManagedWindow, Option<&CurrentMonitor>)>,
    restored_states: Res<RestoredStates>,
    mismatch_states: Res<MismatchStates>,
    mut commands: Commands,
) {
    let display_entity = *primary_display;
    let (window_entity, window, monitor) = *window_query;

    let restored_state = restored_states.by_entity.get(&window_entity);
    let mismatch_state = mismatch_states.by_entity.get(&window_entity);

    let (video_modes, refresh_rate) = input::get_video_modes_for_monitor(&bevy_monitors, monitor);
    let refresh_display = input::format_refresh_rate(window, refresh_rate);
    let active_mode_idx = input::find_active_video_mode_index(window, &video_modes);
    input::sync_selected_to_active(window, monitor, active_mode_idx, &mut selected);
    let selected_idx = selected.get(monitor.index);
    let video_modes_display =
        input::build_video_modes_display(&video_modes, selected_idx, active_mode_idx);

    let font = TextFont {
        font_size: FONT_SIZE,
        ..default()
    };

    commands.entity(display_entity).despawn_children();
    commands
        .entity(display_entity)
        .with_children(|child_spawner| {
            // Monitor header
            let monitor_row = input::format_monitor_row(monitor, &refresh_display);
            add_span(
                child_spawner,
                &font,
                &format!("{monitor_row}\n\n"),
                DEFAULT_COLOR,
            );

            // Comparison table
            build_comparison_spans(
                child_spawner,
                restored_state,
                mismatch_state,
                window,
                monitor,
                &font,
            );

            // Video modes
            add_span(
                child_spawner,
                &font,
                &format!("{VIDEO_MODES_HEADER}{video_modes_display}\n"),
                DEFAULT_COLOR,
            );

            // Controls
            add_span(
                child_spawner,
                &font,
                &format!(
                    "\nControls:\n\
                 [Enter] Exclusive Fullscreen\n\
                 [B] Borderless Fullscreen\n\
                 [W] Windowed\n\
                 [Space] Spawn managed window\n\
                 [P] Toggle persistence ({persistence:?})\n\
                 [Ctrl+Shift+Backspace] Clear state and quit\n\
                 [Q] Quit\n"
                ),
                DEFAULT_COLOR,
            );

            // Managed windows list
            let mut managed_lines = Vec::new();
            for (managed_window, managed, current_monitor) in &managed_query {
                let monitor = current_monitor.map_or_else(
                    || *monitors.first(),
                    |current_monitor| current_monitor.monitor,
                );
                let position = match managed_window.position {
                    WindowPosition::At(managed_position) => {
                        format!("({}, {})", managed_position.x, managed_position.y)
                    },
                    _ => AUTOMATIC_TEXT.to_string(),
                };
                managed_lines.push(format!(
                    "  {}: position={position} physical={}x{} logical={}x{} {SCALE_LABEL} {} {MONITOR_LABEL} {}\n",
                    managed.name,
                    managed_window.physical_width(),
                    managed_window.physical_height(),
                    managed_window.resolution.width().to_u32(),
                    managed_window.resolution.height().to_u32(),
                    managed_window.resolution.scale_factor(),
                    monitor.index,
                ));
            }
            add_span(child_spawner, &font, MANAGED_WINDOWS_HEADER, DEFAULT_COLOR);
            if managed_lines.is_empty() {
                add_span(child_spawner, &font, NO_MANAGED_WINDOWS_TEXT, DEFAULT_COLOR);
            } else {
                for line in &managed_lines {
                    add_span(child_spawner, &font, line, DEFAULT_COLOR);
                }
            }
        });
}

// --- Secondary Window Displays ---

#[expect(
    clippy::too_many_arguments,
    reason = "Bevy system — each param is a distinct system resource"
)]
pub(crate) fn update_secondary_displays(
    mut displays: Query<(Entity, &SecondaryDisplay)>,
    windows: Query<(&Window, Option<&CurrentMonitor>)>,
    managed_query: Query<&ManagedWindow>,
    monitors: Res<Monitors>,
    bevy_monitors: Query<(Entity, &Monitor)>,
    mut selected: ResMut<SelectedVideoModes>,
    restored_states: Res<RestoredStates>,
    mismatch_states: Res<MismatchStates>,
    mut commands: Commands,
) {
    for (display_entity, display) in &mut displays {
        let Ok((window, current_monitor)) = windows.get(display.0) else {
            continue;
        };
        let monitor_info = current_monitor.copied().unwrap_or_else(|| CurrentMonitor {
            monitor:        *monitors.first(),
            effective_mode: window.mode,
        });

        let name = managed_query
            .get(display.0)
            .map_or(UNKNOWN_MANAGED_WINDOW_NAME, |managed_window| {
                &managed_window.name
            });
        let restored_state = restored_states.by_entity.get(&display.0);
        let mismatch_state = mismatch_states.by_entity.get(&display.0);

        let (video_modes, refresh_rate) =
            input::get_video_modes_for_monitor(&bevy_monitors, &monitor_info);
        let refresh_display = input::format_refresh_rate(window, refresh_rate);
        let active_mode_idx = input::find_active_video_mode_index(window, &video_modes);
        input::sync_selected_to_active(window, &monitor_info, active_mode_idx, &mut selected);
        let selected_idx = selected.get(monitor_info.index);
        let video_modes_display =
            input::build_video_modes_display(&video_modes, selected_idx, active_mode_idx);

        let font = TextFont {
            font_size: FONT_SIZE,
            ..default()
        };

        commands.entity(display_entity).despawn_children();
        commands
            .entity(display_entity)
            .with_children(|child_spawner| {
                // Window name + monitor header
                let monitor_row = input::format_monitor_row(&monitor_info, &refresh_display);
                add_span(
                    child_spawner,
                    &font,
                    &format!("{SECONDARY_WINDOW_NAME_LABEL} {name}\n{monitor_row}\n\n"),
                    DEFAULT_COLOR,
                );

                // Comparison table
                build_comparison_spans(
                    child_spawner,
                    restored_state,
                    mismatch_state,
                    window,
                    &monitor_info,
                    &font,
                );

                // Video modes
                add_span(
                    child_spawner,
                    &font,
                    &format!("{VIDEO_MODES_HEADER}{video_modes_display}\n"),
                    DEFAULT_COLOR,
                );

                // Controls
                add_span(
                    child_spawner,
                    &font,
                    "\nControls:\n\
                 [Enter] Exclusive Fullscreen\n\
                 [B] Borderless Fullscreen\n\
                 [W] Windowed\n\
                 [Space] Spawn managed window\n\
                 [P] Toggle persistence\n\
                 [Ctrl+Shift+Backspace] Clear state and quit\n\
                 [Q] Quit\n",
                    DEFAULT_COLOR,
                );
            });
    }
}
