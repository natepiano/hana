//! Diegetic layout-tree builders for the camera control panel.

use bevy::prelude::*;
use bevy_diegetic::AlignY;
use bevy_diegetic::ChildDivider;
use bevy_diegetic::Column;
use bevy_diegetic::El;
use bevy_diegetic::GlyphShadowMode;
use bevy_diegetic::LayoutBuilder;
use bevy_diegetic::LayoutTree;
use bevy_diegetic::PanelDraw;
use bevy_diegetic::Px;
use bevy_diegetic::Sizing;
use bevy_diegetic::TextStyle;
use bevy_lagrange::ControlSpeed;
use bevy_lagrange::OrbitCamInteractionKind;
use bevy_lagrange::ZoomDirection;

use super::constants::ACTION_COLUMN_WIDTH;
use super::constants::ACTIVE_COLOR;
use super::constants::CONNECTOR_CAP_SIZE;
use super::constants::CONNECTOR_LEVEL_EPSILON;
use super::constants::CONNECTOR_LINE_WIDTH;
use super::constants::FEEDER_CELL_MIN;
use super::constants::FEEDER_START_GAP;
use super::constants::GUIDANCE_CHILD_GAP;
use super::constants::HEADER_COLOR;
use super::constants::LABEL_COLOR;
use super::constants::LABEL_LINE_HEIGHT;
use super::constants::SPACER_WIDTH;
use super::constants::SPEED_LABEL_COLUMN_WIDTH;
use super::constants::TABLE_COLUMN_GAP;
use super::constants::TABLE_DIVIDER_WIDTH;
use super::constants::TABLE_GROUP_GAP;
use super::constants::TABLE_ROW_GAP;
use super::constants::TRUNK_END_GAP;
use super::display::CameraGuidanceDisplay;
use super::snapshot;
use super::snapshot::CameraGuidanceSnapshot;
use crate::connector;
use crate::connector::ConnectorColors;
use crate::connector::SpacerLayout;
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
    let title = TextStyle::new(TITLE_SIZE)
        .with_color(TITLE_COLOR)
        .no_wrap()
        .with_shadow_mode(GlyphShadowMode::None);
    let header = TextStyle::new(LABEL_SIZE)
        .with_color(HEADER_COLOR)
        .no_wrap()
        .with_shadow_mode(GlyphShadowMode::None);
    let label = TextStyle::new(LABEL_SIZE)
        .with_color(LABEL_COLOR)
        .no_wrap()
        .with_shadow_mode(GlyphShadowMode::None);
    let active = TextStyle::new(LABEL_SIZE)
        .with_color(ACTIVE_COLOR)
        .no_wrap()
        .with_shadow_mode(GlyphShadowMode::None);

    screen_panels::screen_panel_frame(builder, Sizing::FIT, Sizing::FIT, background, |builder| {
        builder.with(
            El::column()
                .width(Sizing::FIT)
                .height(Sizing::FIT)
                .gap(GUIDANCE_CHILD_GAP),
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
    label: &TextStyle,
    active: &TextStyle,
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
        El::column()
            .width(Sizing::FIT)
            .height(Sizing::FIT)
            .gap(Px(TABLE_GROUP_GAP))
            .child_divider(ChildDivider::new(TABLE_DIVIDER_WIDTH, BORDER_DIM)),
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
    label: &TextStyle,
    active: &TextStyle,
) {
    builder.with(
        El::row()
            .width(Sizing::GROW)
            .height(Sizing::FIT)
            .gap(Px(TABLE_COLUMN_GAP))
            .align_y(AlignY::Center),
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
fn action_rows_element(speed_column: SpeedColumn) -> El<Column> {
    let element = El::column()
        .width(Sizing::GROW)
        .height(Sizing::FIT)
        .gap(Px(TABLE_GROUP_GAP));
    match speed_column {
        SpeedColumn::Hidden => {
            element.child_divider(ChildDivider::new(TABLE_DIVIDER_WIDTH, BORDER_DIM))
        },
        SpeedColumn::Shown => element,
    }
}

fn build_action_row(
    builder: &mut LayoutBuilder,
    snapshot: &CameraGuidanceSnapshot,
    group: (OrbitCamInteractionKind, ControlSpeed, Option<ZoomDirection>),
    display: CameraGuidanceDisplay,
    label: &TextStyle,
    active: &TextStyle,
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

    let word_active = rows
        .iter()
        .map(|row| speed_matches && snapshot::row_active(row, active_sources, live_zoom_direction))
        .collect::<Vec<_>>();
    let group_active = word_active.iter().any(|&active| active);
    let action_style = if group_active { active } else { label };
    let spacer_layout = SpacerLayout {
        word_count:        rows.len(),
        label_line_height: LABEL_LINE_HEIGHT,
        row_gap:           Px(TABLE_ROW_GAP),
        line_width:        CONNECTOR_LINE_WIDTH,
        cap_size:          CONNECTOR_CAP_SIZE,
        level_epsilon:     CONNECTOR_LEVEL_EPSILON,
        trunk_end_gap:     TRUNK_END_GAP,
        colors:            ConnectorColors {
            active: ACTIVE_COLOR,
            idle:   Color::NONE,
        },
    };
    let spacer_lines = connector::spacer_lines(spacer_layout, &word_active, group_active);

    builder.with(
        El::row()
            .width(Sizing::GROW)
            .height(Sizing::FIT)
            .gap(Px(0.0))
            .align_y(AlignY::Center),
        |builder| {
            builder.with(
                El::column()
                    .width(Sizing::GROW)
                    .height(Sizing::FIT)
                    .gap(Px(TABLE_ROW_GAP)),
                |builder| {
                    for (row, &is_active) in rows.iter().zip(&word_active) {
                        let binding_style = if is_active { active } else { label };
                        let connector_color = if is_active { ACTIVE_COLOR } else { Color::NONE };
                        builder.with(
                            El::row()
                                .width(Sizing::GROW)
                                .height(Sizing::fixed(LABEL_LINE_HEIGHT))
                                .align_y(AlignY::Center),
                            |builder| {
                                builder.text(row.label(), binding_style.clone());
                                builder.with(
                                    El::new()
                                        .width(Sizing::grow_min(FEEDER_CELL_MIN))
                                        .height(Sizing::GROW)
                                        .draw(PanelDraw::lines([connector::feeder_line(
                                            FEEDER_START_GAP,
                                            CONNECTOR_LINE_WIDTH,
                                            connector_color,
                                        )])),
                                    |_| {},
                                );
                            },
                        );
                    }
                },
            );
            builder.with(
                El::new()
                    .width(Sizing::fixed(SPACER_WIDTH))
                    .height(Sizing::GROW)
                    .draw(PanelDraw::lines(spacer_lines)),
                |_| {},
            );
            builder.with(
                El::new()
                    .width(Sizing::fixed(ACTION_COLUMN_WIDTH))
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
