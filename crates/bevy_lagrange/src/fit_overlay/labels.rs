use bevy::prelude::*;
use bevy::ui::UiTargetCamera;
use bevy_kana::ScreenPosition;

use super::constants::BOUNDS_LABEL_COLOR;
use super::constants::BOUNDS_LABEL_TEXT;
use super::constants::LABEL_FONT_SIZE;
use super::constants::LABEL_PIXEL_OFFSET;
use super::screen_space;
use crate::fit::Edge;
use crate::projection::ScreenSpaceBounds;

/// Component marking margin percentage labels, scoped to a specific camera entity.
#[derive(Component, Reflect)]
#[reflect(Component)]
pub(super) struct MarginLabel {
    pub(super) edge:   Edge,
    pub(super) camera: Entity,
}

/// Parameters for creating or updating a margin label.
pub(super) struct MarginLabelParameters {
    pub(super) camera:          Entity,
    pub(super) ui_camera:       Entity,
    pub(super) edge:            Edge,
    pub(super) text:            String,
    pub(super) color:           Color,
    pub(super) screen_position: ScreenPosition,
    pub(super) viewport_size:   Vec2,
}

/// Component marking the "screen space bounds" label, scoped to a specific camera entity.
#[derive(Component, Reflect)]
#[reflect(Component)]
pub(super) struct BoundsLabel {
    pub(super) camera: Entity,
}

/// Calculates the viewport pixel position for a margin label, offset by a fixed
/// number of pixels from the screen-edge endpoint of the margin line.
pub(super) fn calculate_label_pixel_position(
    edge: Edge,
    bounds: &ScreenSpaceBounds,
    viewport_size: Vec2,
) -> ScreenPosition {
    let (screen_x, screen_y) = screen_space::screen_edge_center(bounds, edge);
    let pixel_position = screen_space::norm_to_viewport(
        screen_x,
        screen_y,
        bounds.half_extent_x,
        bounds.half_extent_y,
        viewport_size,
    );

    // Left/Right labels sit above the horizontal line;
    // Top/Bottom labels sit beside the vertical line with pixel offsets.
    let above_line = pixel_position.y - LABEL_FONT_SIZE - LABEL_PIXEL_OFFSET;

    match edge {
        Edge::Left => ScreenPosition::new(LABEL_PIXEL_OFFSET, above_line),
        Edge::Right => ScreenPosition::new(viewport_size.x - LABEL_PIXEL_OFFSET, above_line),
        Edge::Top => ScreenPosition::new(pixel_position.x + LABEL_PIXEL_OFFSET, LABEL_PIXEL_OFFSET),
        Edge::Bottom => ScreenPosition::new(
            pixel_position.x + LABEL_PIXEL_OFFSET,
            viewport_size.y - LABEL_PIXEL_OFFSET,
        ),
    }
}

/// Returns the final viewport position for the "screen space bounds" label.
pub(super) fn bounds_label_position(upper_left: ScreenPosition) -> ScreenPosition {
    ScreenPosition::new(
        upper_left.x + LABEL_PIXEL_OFFSET,
        upper_left.y - LABEL_FONT_SIZE - LABEL_PIXEL_OFFSET,
    )
}

/// Applies anchored placement for a margin label node based on edge semantics.
fn apply_margin_label_anchor(
    node: &mut Node,
    edge: Edge,
    screen_position: ScreenPosition,
    viewport_size: Vec2,
) {
    match edge {
        Edge::Left | Edge::Top => {
            node.left = Val::Px(screen_position.x);
            node.top = Val::Px(screen_position.y);
            node.right = Val::Auto;
            node.bottom = Val::Auto;
        },
        Edge::Right => {
            node.right = Val::Px(viewport_size.x - screen_position.x);
            node.top = Val::Px(screen_position.y);
            node.left = Val::Auto;
            node.bottom = Val::Auto;
        },
        Edge::Bottom => {
            node.left = Val::Px(screen_position.x);
            node.bottom = Val::Px(viewport_size.y - screen_position.y);
            node.right = Val::Auto;
            node.top = Val::Auto;
        },
    }
}

/// Builds an anchored node for a new margin label.
fn margin_label_node(edge: Edge, screen_position: ScreenPosition, viewport_size: Vec2) -> Node {
    let mut node = Node {
        position_type: PositionType::Absolute,
        ..default()
    };
    apply_margin_label_anchor(&mut node, edge, screen_position, viewport_size);
    node
}

/// Updates an existing margin label or creates a new one.
pub(super) fn update_or_create_margin_label(
    commands: &mut Commands,
    label_query: &mut Query<(
        Entity,
        &MarginLabel,
        &mut Text,
        &mut Node,
        &mut TextColor,
        &mut UiTargetCamera,
    )>,
    parameters: MarginLabelParameters,
) {
    let existing = label_query.iter_mut().find(|(_, label, _, _, _, _)| {
        label.camera == parameters.camera && label.edge == parameters.edge
    });

    if let Some((_, _, mut label_text, mut node, mut text_color, mut ui_camera)) = existing {
        (**label_text).clone_from(&parameters.text);
        text_color.0 = parameters.color;
        ui_camera.0 = parameters.ui_camera;
        apply_margin_label_anchor(
            &mut node,
            parameters.edge,
            parameters.screen_position,
            parameters.viewport_size,
        );
    } else {
        commands.spawn((
            Text::new(parameters.text),
            TextFont {
                font_size: FontSize::Px(LABEL_FONT_SIZE),
                ..default()
            },
            TextColor(parameters.color),
            margin_label_node(
                parameters.edge,
                parameters.screen_position,
                parameters.viewport_size,
            ),
            MarginLabel {
                edge:   parameters.edge,
                camera: parameters.camera,
            },
            UiTargetCamera(parameters.ui_camera),
        ));
    }
}

/// Updates an existing bounds label position or creates a new one.
pub(super) fn update_or_create_bounds_label(
    commands: &mut Commands,
    bounds_query: &mut Query<
        (Entity, &BoundsLabel, &mut Node, &mut UiTargetCamera),
        Without<MarginLabel>,
    >,
    camera: Entity,
    ui_camera: Entity,
    screen_position: ScreenPosition,
) {
    let existing = bounds_query
        .iter_mut()
        .find(|(_, label, _, _)| label.camera == camera);

    if let Some((_, _, mut node, mut target_camera)) = existing {
        node.left = Val::Px(screen_position.x);
        node.top = Val::Px(screen_position.y);
        target_camera.0 = ui_camera;
    } else {
        commands.spawn((
            Text::new(BOUNDS_LABEL_TEXT),
            TextFont {
                font_size: FontSize::Px(LABEL_FONT_SIZE),
                ..default()
            },
            TextColor(BOUNDS_LABEL_COLOR),
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(screen_position.x),
                top: Val::Px(screen_position.y),
                ..default()
            },
            BoundsLabel { camera },
            UiTargetCamera(ui_camera),
        ));
    }
}
