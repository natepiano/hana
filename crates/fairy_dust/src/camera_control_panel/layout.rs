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

use super::constants::ACTION_COLUMN_MIN_WIDTH;
use super::constants::ACTION_COLUMN_SLOW_WIDTH;
use super::constants::ACTIVE_COLOR;
use super::constants::GUIDANCE_CHILD_GAP;
use super::constants::HEADER_COLOR;
use super::constants::LABEL_COLOR;
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

fn build_guidance_table(
    builder: &mut LayoutBuilder,
    snapshot: &CameraGuidanceSnapshot,
    display: CameraGuidanceDisplay,
    label: &LayoutTextStyle,
    active: &LayoutTextStyle,
) {
    // A single shared action-column width keeps the arrows aligned and the
    // action labels left-aligned; widen it only when a slow variant is present.
    let action_width = if snapshot
        .rows
        .iter()
        .any(|row| row.speed() == ControlSpeed::Slow)
    {
        ACTION_COLUMN_SLOW_WIDTH
    } else {
        ACTION_COLUMN_MIN_WIDTH
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
            for (kind, speed) in [
                (OrbitCamInteractionKind::Orbit, ControlSpeed::Normal),
                (OrbitCamInteractionKind::Orbit, ControlSpeed::Slow),
                (OrbitCamInteractionKind::Pan, ControlSpeed::Normal),
                (OrbitCamInteractionKind::Pan, ControlSpeed::Slow),
                (OrbitCamInteractionKind::Zoom, ControlSpeed::Normal),
                (OrbitCamInteractionKind::Zoom, ControlSpeed::Slow),
            ] {
                build_guidance_group(
                    builder,
                    snapshot,
                    (kind, speed),
                    display,
                    action_width,
                    label,
                    active,
                );
            }
        },
    );
}

fn build_guidance_group(
    builder: &mut LayoutBuilder,
    snapshot: &CameraGuidanceSnapshot,
    group: (OrbitCamInteractionKind, ControlSpeed),
    display: CameraGuidanceDisplay,
    action_width: Px,
    label: &LayoutTextStyle,
    active: &LayoutTextStyle,
) {
    let (kind, speed) = group;
    let active_sources = display.sources(kind);
    // Only highlight when the live interaction's speed matches this group, so
    // engaging the slow variant lights "Orbit Slow" without also lighting "Orbit".
    let speed_matches = display.speed(kind) == speed;
    let rows = snapshot
        .rows
        .iter()
        .filter(|row| row.kind() == kind && row.speed() == speed)
        .collect::<Vec<_>>();
    if rows.is_empty() {
        return;
    }

    let group_active = speed_matches
        && rows
            .iter()
            .any(|row| snapshot::row_active(row, active_sources));
    let action_style = if group_active { active } else { label };

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
                        let binding_style =
                            if speed_matches && snapshot::row_active(row, active_sources) {
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
                    .width(Sizing::fit_min(action_width))
                    .height(Sizing::FIT),
                |builder| {
                    builder.text(snapshot::group_label(kind, speed), action_style.clone());
                },
            );
        },
    );
}
