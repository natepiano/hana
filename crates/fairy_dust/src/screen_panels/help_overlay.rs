//! Keyboard shortcut help overlay for title bars.

use bevy::prelude::*;
use bevy_diegetic::AlignX;
use bevy_diegetic::AlignY;
use bevy_diegetic::Anchor;
use bevy_diegetic::DiegeticPanel;
use bevy_diegetic::Direction;
use bevy_diegetic::El;
use bevy_diegetic::Fit;
use bevy_diegetic::LayoutBuilder;
use bevy_diegetic::LayoutTextStyle;
use bevy_diegetic::LayoutTree;
use bevy_diegetic::Sizing;

use super::constants::BODY_COLOR;
use super::constants::DIVIDER_COLOR;
use super::constants::HELP_CLOSE_HINT_COLUMN_WIDTH;
use super::constants::HELP_CLOSE_HINT_SIZE;
use super::constants::HELP_KEY_COLUMN_WIDTH;
use super::constants::HELP_PANEL_CHILD_GAP;
use super::constants::HELP_ROW_GAP;
use super::constants::HELP_SEPARATOR_HEIGHT;
use super::constants::HELP_TABLE_COLUMN_GAP;
use super::default_inner_background;
use super::screen_panel_frame;
use super::screen_panel_material;
use crate::camera_control_panel::CameraGuidancePanel;
use crate::camera_home::CameraHomeMarker;
use crate::constants::LABEL_SIZE;
use crate::constants::TITLE_COLOR;
use crate::constants::TITLE_SIZE;

const HELP_TITLE: &str = "Keyboard Shortcuts";
const CLOSE_HINT: &str = "Esc to close";
const HOME_AABB_KEYS: &str = "ctrl-shift-A";
const HOME_AABB_LABEL: &str = "Show bounding box for camera home AnimateToFit";
const SCREEN_PANEL_KEYS: &str = "ctrl-shift-L";
const SCREEN_PANEL_LABEL: &str = "Toggle screen space panels off/on";
const CAMERA_PRESET_KEYS: &str = "shift-C";
const CAMERA_PRESET_LABEL: &str = "Cycle through camera presets";

#[derive(Component)]
pub(super) struct KeyboardShortcutHelp;

#[derive(Clone, Copy)]
struct HelpShortcuts {
    home_marker:  bool,
    camera_panel: bool,
}

struct HelpRow {
    keys:  &'static str,
    label: &'static str,
}

pub(super) fn toggle_keyboard_shortcut_help(
    keys: Res<ButtonInput<KeyCode>>,
    mut commands: Commands,
    overlay: Query<Entity, With<KeyboardShortcutHelp>>,
    home_markers: Query<Entity, With<CameraHomeMarker>>,
    camera_panels: Query<Entity, With<CameraGuidancePanel>>,
) {
    let question_mark = question_mark_pressed(&keys);
    let escape = keys.just_pressed(KeyCode::Escape);
    if !question_mark && !escape {
        return;
    }

    let mut help_was_open = false;
    for entity in &overlay {
        commands.entity(entity).despawn();
        help_was_open = true;
    }
    if help_was_open {
        return;
    }
    if escape {
        return;
    }

    spawn_help_overlay(
        &mut commands,
        HelpShortcuts {
            home_marker:  !home_markers.is_empty(),
            camera_panel: !camera_panels.is_empty(),
        },
    );
}

fn question_mark_pressed(keys: &ButtonInput<KeyCode>) -> bool {
    let shift = keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]);
    shift && keys.just_pressed(KeyCode::Slash)
}

fn spawn_help_overlay(commands: &mut Commands, shortcuts: HelpShortcuts) {
    let unlit = screen_panel_material();
    let panel = DiegeticPanel::screen()
        .size(Fit, Fit)
        .anchor(Anchor::Center)
        .material(unlit.clone())
        .text_material(unlit)
        .with_tree(build_help_tree(shortcuts))
        .build();

    match panel {
        Ok(panel) => {
            commands.spawn((KeyboardShortcutHelp, panel, Transform::default()));
        },
        Err(error) => {
            error!("fairy_dust: failed to build keyboard shortcut help: {error}");
        },
    }
}

fn build_help_tree(shortcuts: HelpShortcuts) -> LayoutTree {
    let mut builder = LayoutBuilder::with_root(El::new().width(Sizing::FIT).height(Sizing::FIT));
    build_help_layout(&mut builder, shortcuts);
    builder.build()
}

fn build_help_layout(builder: &mut LayoutBuilder, shortcuts: HelpShortcuts) {
    let title = LayoutTextStyle::new(TITLE_SIZE)
        .with_color(TITLE_COLOR)
        .no_wrap();
    let hint = LayoutTextStyle::new(HELP_CLOSE_HINT_SIZE)
        .with_color(BODY_COLOR)
        .no_wrap();
    let label = LayoutTextStyle::new(LABEL_SIZE)
        .with_color(BODY_COLOR)
        .no_wrap();

    screen_panel_frame(
        builder,
        Sizing::FIT,
        default_inner_background(),
        |builder| {
            builder.with(
                El::new()
                    .width(Sizing::FIT)
                    .height(Sizing::FIT)
                    .direction(Direction::TopToBottom)
                    .child_gap(HELP_PANEL_CHILD_GAP),
                |builder| {
                    build_title_row(builder, &title, &hint);
                    build_separator(builder);
                    build_shortcut_table(builder, shortcuts, &label);
                },
            );
        },
    );
}

fn build_title_row(builder: &mut LayoutBuilder, title: &LayoutTextStyle, hint: &LayoutTextStyle) {
    builder.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::FIT)
            .direction(Direction::LeftToRight)
            .child_gap(HELP_TABLE_COLUMN_GAP)
            .child_align_y(AlignY::Center),
        |builder| {
            builder.with(
                El::new().width(Sizing::GROW).height(Sizing::FIT),
                |builder| {
                    builder.text(HELP_TITLE, title.clone());
                },
            );
            builder.with(
                El::new()
                    .width(Sizing::fixed(HELP_CLOSE_HINT_COLUMN_WIDTH))
                    .height(Sizing::FIT)
                    .child_align_x(AlignX::Right),
                |builder| {
                    builder.text(CLOSE_HINT, hint.clone());
                },
            );
        },
    );
}

fn build_separator(builder: &mut LayoutBuilder) {
    builder.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::fixed(HELP_SEPARATOR_HEIGHT))
            .background(DIVIDER_COLOR),
        |_| {},
    );
}

fn build_shortcut_table(
    builder: &mut LayoutBuilder,
    shortcuts: HelpShortcuts,
    label: &LayoutTextStyle,
) {
    builder.with(
        El::new()
            .width(Sizing::FIT)
            .height(Sizing::FIT)
            .direction(Direction::TopToBottom)
            .child_gap(HELP_ROW_GAP),
        |builder| {
            for row in shortcut_rows(shortcuts) {
                build_shortcut_row(builder, row, label);
            }
        },
    );
}

fn build_shortcut_row(builder: &mut LayoutBuilder, row: HelpRow, label: &LayoutTextStyle) {
    builder.with(
        El::new()
            .width(Sizing::FIT)
            .height(Sizing::FIT)
            .direction(Direction::LeftToRight)
            .child_gap(HELP_TABLE_COLUMN_GAP)
            .child_align_y(AlignY::Center),
        |builder| {
            builder.with(
                El::new()
                    .width(Sizing::fixed(HELP_KEY_COLUMN_WIDTH))
                    .height(Sizing::FIT),
                |builder| {
                    builder.text(row.keys, label.clone());
                },
            );
            builder.with(
                El::new().width(Sizing::FIT).height(Sizing::FIT),
                |builder| {
                    builder.text(row.label, label.clone());
                },
            );
        },
    );
}

fn shortcut_rows(shortcuts: HelpShortcuts) -> Vec<HelpRow> {
    let mut rows = Vec::new();
    if shortcuts.home_marker {
        rows.push(HelpRow {
            keys:  HOME_AABB_KEYS,
            label: HOME_AABB_LABEL,
        });
    }
    rows.push(HelpRow {
        keys:  SCREEN_PANEL_KEYS,
        label: SCREEN_PANEL_LABEL,
    });
    if shortcuts.camera_panel {
        rows.push(HelpRow {
            keys:  CAMERA_PRESET_KEYS,
            label: CAMERA_PRESET_LABEL,
        });
    }
    rows
}
