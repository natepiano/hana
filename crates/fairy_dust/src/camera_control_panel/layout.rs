//! Diegetic layout-tree builders for the camera control panel.

use bevy::prelude::*;
use bevy_diegetic::AlignX;
use bevy_diegetic::AlignY;
use bevy_diegetic::Border;
use bevy_diegetic::CornerRadius;
use bevy_diegetic::Direction;
use bevy_diegetic::El;
use bevy_diegetic::LayoutBuilder;
use bevy_diegetic::LayoutTextStyle;
use bevy_diegetic::LayoutTree;
use bevy_diegetic::Padding;
use bevy_diegetic::Px;
use bevy_diegetic::Sizing;
use bevy_diegetic::default_panel_material;
use bevy_lagrange::OrbitCamInteractionKind;

use super::config::SourceVisibility;
use super::constants::ACTION_COLUMN_MIN_WIDTH;
use super::constants::ACTIVE_COLOR;
use super::constants::GUIDANCE_CHILD_GAP;
use super::constants::HEADER_COLOR;
use super::constants::HEADER_SIZE;
use super::constants::LABEL_COLOR;
use super::constants::LABEL_SIZE;
use super::constants::SOURCE_COLOR;
use super::constants::TABLE_ACTION_ARROW;
use super::constants::TABLE_COLUMN_GAP;
use super::constants::TABLE_DIVIDER_WIDTH;
use super::constants::TABLE_GROUP_GAP;
use super::constants::TABLE_ROW_GAP;
use super::display::CameraGuidanceDisplay;
use super::snapshot;
use super::snapshot::CameraGuidanceSnapshot;
use crate::constants::BORDER;
use crate::constants::BORDER_ACCENT;
use crate::constants::BORDER_DIM;
use crate::constants::FRAME_PAD;
use crate::constants::INNER_BG;
use crate::constants::INNER_BORDER_WIDTH;
use crate::constants::INNER_PAD;
use crate::constants::INNER_RADIUS;
use crate::constants::RADIUS;
use crate::constants::TITLE_COLOR;
use crate::constants::TITLE_SIZE;

pub(super) fn unlit_panel_material() -> StandardMaterial {
    StandardMaterial {
        unlit: true,
        ..default_panel_material()
    }
}

pub(super) fn build_guidance_tree(
    snapshot: &CameraGuidanceSnapshot,
    display: CameraGuidanceDisplay,
) -> LayoutTree {
    let mut builder = LayoutBuilder::with_root(El::new().width(Sizing::FIT).height(Sizing::FIT));
    build_guidance_layout(&mut builder, snapshot, display);
    builder.build()
}

fn build_guidance_layout(
    builder: &mut LayoutBuilder,
    snapshot: &CameraGuidanceSnapshot,
    display: CameraGuidanceDisplay,
) {
    let title = LayoutTextStyle::new(TITLE_SIZE)
        .with_color(TITLE_COLOR)
        .no_wrap();
    let header = LayoutTextStyle::new(HEADER_SIZE)
        .with_color(HEADER_COLOR)
        .no_wrap();
    let label = LayoutTextStyle::new(LABEL_SIZE)
        .with_color(LABEL_COLOR)
        .no_wrap();
    let active = LayoutTextStyle::new(LABEL_SIZE)
        .with_color(ACTIVE_COLOR)
        .no_wrap();
    let source = LayoutTextStyle::new(LABEL_SIZE)
        .with_color(SOURCE_COLOR)
        .no_wrap();

    builder.with(
        El::new()
            .width(Sizing::FIT)
            .height(Sizing::FIT)
            .padding(Padding::all(FRAME_PAD))
            .corner_radius(CornerRadius::all(RADIUS))
            .border(Border::all(BORDER, BORDER_ACCENT)),
        |builder| {
            builder.with(
                El::new()
                    .width(Sizing::FIT)
                    .height(Sizing::FIT)
                    .direction(Direction::TopToBottom)
                    .padding(Padding::all(INNER_PAD))
                    .child_gap(GUIDANCE_CHILD_GAP)
                    .corner_radius(CornerRadius::all(INNER_RADIUS))
                    .background(INNER_BG)
                    .border(Border::all(INNER_BORDER_WIDTH, BORDER_DIM)),
                |builder| {
                    builder.text(format!("CAMERA: {}", snapshot.camera_label), title.clone());
                    builder.text(
                        format!("{}: {}", snapshot.mode_label, snapshot.mode_value),
                        header.clone(),
                    );
                    build_guidance_table(builder, snapshot, display, &label, &active);
                    if snapshot.source_visibility == SourceVisibility::Visible {
                        builder.with(
                            El::new()
                                .width(Sizing::GROW)
                                .height(Sizing::FIT)
                                .child_align_x(AlignX::Center),
                            |builder| {
                                builder.text(snapshot::source_label(display.all_sources()), source);
                            },
                        );
                    }
                },
            );
        },
    );
}

fn build_guidance_table(
    builder: &mut LayoutBuilder,
    snapshot: &CameraGuidanceSnapshot,
    display: CameraGuidanceDisplay,
    label: &LayoutTextStyle,
    active: &LayoutTextStyle,
) {
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
            for kind in [
                OrbitCamInteractionKind::Orbit,
                OrbitCamInteractionKind::Pan,
                OrbitCamInteractionKind::Zoom,
            ] {
                build_guidance_group(builder, snapshot, kind, display, label, active);
            }
        },
    );
}

fn build_guidance_group(
    builder: &mut LayoutBuilder,
    snapshot: &CameraGuidanceSnapshot,
    kind: OrbitCamInteractionKind,
    display: CameraGuidanceDisplay,
    label: &LayoutTextStyle,
    active: &LayoutTextStyle,
) {
    let active_sources = display.sources(kind);
    let rows = snapshot
        .rows
        .iter()
        .filter(|row| row.kind() == kind)
        .collect::<Vec<_>>();
    if rows.is_empty() {
        return;
    }

    let group_active = rows
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
                        let binding_style = if snapshot::row_active(row, active_sources) {
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
                    .width(Sizing::fit_min(ACTION_COLUMN_MIN_WIDTH))
                    .height(Sizing::FIT),
                |builder| {
                    builder.text(snapshot::kind_label(kind), action_style.clone());
                },
            );
        },
    );
}
