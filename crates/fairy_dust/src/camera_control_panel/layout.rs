//! Diegetic layout-tree builders for the camera control panel.

use bevy::prelude::*;
use bevy_diegetic::AlignY;
use bevy_diegetic::Border;
use bevy_diegetic::Direction;
use bevy_diegetic::El;
use bevy_diegetic::LayoutBuilder;
use bevy_diegetic::LayoutTextStyle;
use bevy_diegetic::LayoutTree;
use bevy_diegetic::Px;
use bevy_diegetic::Sizing;
use bevy_lagrange::ControlSpeed;
use bevy_lagrange::OrbitCamInteractionKind;
use bevy_lagrange::ZoomDirection;

use super::constants::ACTION_COLUMN_WIDTH;
use super::constants::ACTIVE_COLOR;
use super::constants::GUIDANCE_CHILD_GAP;
use super::constants::HEADER_COLOR;
use super::constants::LABEL_COLOR;
use super::constants::SPEED_LABEL_COLUMN_WIDTH;
use super::constants::TABLE_ACTION_ARROW;
use super::constants::TABLE_COLUMN_GAP;
use super::constants::TABLE_DIVIDER_WIDTH;
use super::constants::TABLE_GROUP_GAP;
use super::constants::TABLE_ROW_GAP;
use super::display::CameraGuidanceDisplay;
use super::snapshot;
use super::snapshot::CameraGuidanceSnapshot;
use crate::constants::BORDER_DIM;
use crate::constants::LABEL_SIZE;
use crate::constants::TITLE_COLOR;
use crate::constants::TITLE_SIZE;
use crate::screen_panels;

pub(super) fn build_guidance_tree(
    snapshot: &CameraGuidanceSnapshot,
    display: CameraGuidanceDisplay,
    background: Color,
) -> LayoutTree {
    let mut builder = LayoutBuilder::with_root(El::new().width(Sizing::FIT).height(Sizing::FIT));
    build_guidance_layout(&mut builder, snapshot, display, background);
    builder.build()
}

fn build_guidance_layout(
    builder: &mut LayoutBuilder,
    snapshot: &CameraGuidanceSnapshot,
    display: CameraGuidanceDisplay,
    background: Color,
) {
    let title = LayoutTextStyle::new(TITLE_SIZE)
        .with_color(TITLE_COLOR)
        .no_wrap();
    let header = LayoutTextStyle::new(LABEL_SIZE)
        .with_color(HEADER_COLOR)
        .no_wrap();
    let label = LayoutTextStyle::new(LABEL_SIZE)
        .with_color(LABEL_COLOR)
        .no_wrap();
    let active = LayoutTextStyle::new(LABEL_SIZE)
        .with_color(ACTIVE_COLOR)
        .no_wrap();

    screen_panels::screen_panel_frame(builder, Sizing::FIT, Sizing::FIT, background, |builder| {
        builder.with(
            El::new()
                .width(Sizing::FIT)
                .height(Sizing::FIT)
                .direction(Direction::TopToBottom)
                .child_gap(GUIDANCE_CHILD_GAP),
            |builder| {
                builder.text(format!("CAMERA: {}", snapshot.camera_label), title.clone());
                builder.text(
                    format!("{}: {}", snapshot.mode_label, snapshot.mode_value),
                    header.clone(),
                );
                build_guidance_table(builder, snapshot, display, &label, &active);
            },
        );
    });
}

/// Whether a speed block renders the `Normal` / `Slow` label column.
#[derive(Clone, Copy)]
enum SpeedColumn {
    Shown,
    Hidden,
}

fn build_guidance_table(
    builder: &mut LayoutBuilder,
    snapshot: &CameraGuidanceSnapshot,
    display: CameraGuidanceDisplay,
    label: &LayoutTextStyle,
    active: &LayoutTextStyle,
) {
    let speeds: Vec<ControlSpeed> = [ControlSpeed::Normal, ControlSpeed::Slow]
        .into_iter()
        .filter(|&speed| snapshot.rows.iter().any(|row| row.speed() == speed))
        .collect();
    // Single-speed presets (mouse, keyboard) drop the speed column entirely.
    let speed_column = if speeds.len() > 1 {
        SpeedColumn::Shown
    } else {
        SpeedColumn::Hidden
    };

    builder.with(
        El::new()
            .width(Sizing::FIT)
            .height(Sizing::FIT)
            .direction(Direction::TopToBottom)
            .child_gap(Px(TABLE_GROUP_GAP))
            .border(
                Border::new()
                    .between_children(TABLE_DIVIDER_WIDTH)
                    .color(BORDER_DIM),
            ),
        |builder| {
            for speed in speeds {
                build_speed_block(
                    builder,
                    snapshot,
                    speed,
                    speed_column,
                    display,
                    label,
                    active,
                );
            }
        },
    );
}

fn build_speed_block(
    builder: &mut LayoutBuilder,
    snapshot: &CameraGuidanceSnapshot,
    speed: ControlSpeed,
    speed_column: SpeedColumn,
    display: CameraGuidanceDisplay,
    label: &LayoutTextStyle,
    active: &LayoutTextStyle,
) {
    builder.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::FIT)
            .direction(Direction::LeftToRight)
            .child_gap(Px(TABLE_COLUMN_GAP))
            .child_align_y(AlignY::Center),
        |builder| {
            if matches!(speed_column, SpeedColumn::Shown) {
                let block_active = snapshot.rows.iter().any(|row| {
                    row.speed() == speed
                        && display.speed(row.kind()) == Some(speed)
                        && snapshot::row_active(
                            row,
                            display.sources(row.kind()),
                            display.zoom_direction(),
                        )
                });
                let speed_style = if block_active { active } else { label };
                builder.with(
                    El::new()
                        .width(Sizing::fit_min(SPEED_LABEL_COLUMN_WIDTH))
                        .height(Sizing::FIT),
                    |builder| {
                        builder.text(snapshot::speed_label(speed), speed_style.clone());
                    },
                );
            }
            builder.with(action_rows_element(speed_column), |builder| {
                for (kind, direction) in [
                    (OrbitCamInteractionKind::Orbit, None),
                    (OrbitCamInteractionKind::Pan, None),
                    (OrbitCamInteractionKind::Zoom, Some(ZoomDirection::In)),
                    (OrbitCamInteractionKind::Zoom, Some(ZoomDirection::Out)),
                    (OrbitCamInteractionKind::Zoom, None),
                ] {
                    build_action_row(
                        builder,
                        snapshot,
                        (kind, speed, direction),
                        display,
                        label,
                        active,
                    );
                }
            });
        },
    );
}

/// The action-row column element. Single-speed presets (`SimpleMouse`,
/// `BlenderLike`) divide their rows with a border line — Orbit / Pan / Zoom In /
/// Zoom Out — while multi-speed presets keep the rows gap-separated and rely on
/// the divider between their `Normal` / `Slow` blocks instead.
fn action_rows_element(speed_column: SpeedColumn) -> El {
    let element = El::new()
        .width(Sizing::GROW)
        .height(Sizing::FIT)
        .direction(Direction::TopToBottom)
        .child_gap(Px(TABLE_ROW_GAP));
    match speed_column {
        SpeedColumn::Hidden => element.border(
            Border::new()
                .between_children(TABLE_DIVIDER_WIDTH)
                .color(BORDER_DIM),
        ),
        SpeedColumn::Shown => element,
    }
}

fn build_action_row(
    builder: &mut LayoutBuilder,
    snapshot: &CameraGuidanceSnapshot,
    group: (OrbitCamInteractionKind, ControlSpeed, Option<ZoomDirection>),
    display: CameraGuidanceDisplay,
    label: &LayoutTextStyle,
    active: &LayoutTextStyle,
) {
    let (kind, speed, direction) = group;
    let active_sources = display.sources(kind);
    let live_zoom_direction = display.zoom_direction();
    // `speed_matches` gates highlight so the slow row stays dim at normal speed.
    let speed_matches = display.speed(kind) == Some(speed);
    let rows = snapshot
        .rows
        .iter()
        .filter(|row| {
            row.kind() == kind && row.speed() == speed && row.zoom_direction() == direction
        })
        .collect::<Vec<_>>();
    if rows.is_empty() {
        return;
    }

    let action_active = speed_matches
        && rows
            .iter()
            .any(|row| snapshot::row_active(row, active_sources, live_zoom_direction));
    let action_style = if action_active { active } else { label };

    builder.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::FIT)
            .direction(Direction::LeftToRight)
            .child_gap(Px(TABLE_COLUMN_GAP))
            .child_align_y(AlignY::Center),
        |builder| {
            builder.with(
                El::new()
                    .width(Sizing::GROW)
                    .height(Sizing::FIT)
                    .direction(Direction::TopToBottom)
                    .child_gap(Px(TABLE_ROW_GAP)),
                |builder| {
                    for row in rows {
                        let binding_style = if speed_matches
                            && snapshot::row_active(row, active_sources, live_zoom_direction)
                        {
                            active
                        } else {
                            label
                        };
                        builder.text(row.label(), binding_style.clone());
                    }
                },
            );
            builder.text(TABLE_ACTION_ARROW, action_style.clone());
            builder.with(
                El::new()
                    .width(Sizing::fit_min(ACTION_COLUMN_WIDTH))
                    .height(Sizing::FIT),
                |builder| {
                    builder.text(
                        snapshot::action_label(kind, direction),
                        action_style.clone(),
                    );
                },
            );
        },
    );
}
