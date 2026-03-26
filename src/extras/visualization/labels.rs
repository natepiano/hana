use bevy::prelude::*;
use bevy::ui::UiTargetCamera;

use super::super::fit::Edge;
use super::super::support::ScreenSpaceBounds;
use super::screen_space;

/// Font size used for all debug labels.
const LABEL_FONT_SIZE: f32 = 11.0;
/// Pixel offset used to keep labels off line endpoints and screen edges.
const LABEL_PIXEL_OFFSET: f32 = 8.0;

/// Component marking margin percentage labels, scoped to a specific camera entity.
#[derive(Component, Reflect)]
#[reflect(Component)]
pub struct MarginLabel {
    pub edge:   Edge,
    pub camera: Entity,
}

/// Parameters for creating or updating a margin label.
pub struct MarginLabelParams {
    pub camera:        Entity,
    pub edge:          Edge,
    pub text:          String,
    pub color:         Color,
    pub screen_pos:    Vec2,
    pub viewport_size: Vec2,
}

/// Component marking the "screen space bounds" label, scoped to a specific camera entity.
#[derive(Component, Reflect)]
#[reflect(Component)]
pub struct BoundsLabel {
    pub camera: Entity,
}

/// Calculates the viewport pixel position for a margin label, offset by a fixed
/// number of pixels from the screen-edge endpoint of the margin line.
pub fn calculate_label_pixel_position(
    edge: Edge,
    bounds: &ScreenSpaceBounds,
    viewport_size: Vec2,
) -> Vec2 {
    let (screen_x, screen_y) = screen_space::screen_edge_center(bounds, edge);
    let px = screen_space::norm_to_viewport(
        screen_x,
        screen_y,
        bounds.half_extent_x,
        bounds.half_extent_y,
        viewport_size,
    );

    // Left/Right labels sit above the horizontal line;
    // Top/Bottom labels sit beside the vertical line with pixel offsets.
    let above_line = px.y - LABEL_FONT_SIZE - LABEL_PIXEL_OFFSET;

    match edge {
        Edge::Left => Vec2::new(LABEL_PIXEL_OFFSET, above_line),
        Edge::Right => Vec2::new(viewport_size.x - LABEL_PIXEL_OFFSET, above_line),
        Edge::Top => Vec2::new(px.x + LABEL_PIXEL_OFFSET, LABEL_PIXEL_OFFSET),
        Edge::Bottom => Vec2::new(
            px.x + LABEL_PIXEL_OFFSET,
            viewport_size.y - LABEL_PIXEL_OFFSET,
        ),
    }
}

/// Returns the final viewport position for the "screen space bounds" label.
pub fn bounds_label_position(upper_left: Vec2) -> Vec2 {
    Vec2::new(
        upper_left.x + LABEL_PIXEL_OFFSET,
        upper_left.y - LABEL_FONT_SIZE - LABEL_PIXEL_OFFSET,
    )
}

/// Applies anchored placement for a margin label node based on edge semantics.
fn apply_margin_label_anchor(node: &mut Node, edge: Edge, screen_pos: Vec2, viewport_size: Vec2) {
    match edge {
        Edge::Left | Edge::Top => {
            node.left = Val::Px(screen_pos.x);
            node.top = Val::Px(screen_pos.y);
            node.right = Val::Auto;
            node.bottom = Val::Auto;
        },
        Edge::Right => {
            node.right = Val::Px(viewport_size.x - screen_pos.x);
            node.top = Val::Px(screen_pos.y);
            node.left = Val::Auto;
            node.bottom = Val::Auto;
        },
        Edge::Bottom => {
            node.left = Val::Px(screen_pos.x);
            node.bottom = Val::Px(viewport_size.y - screen_pos.y);
            node.right = Val::Auto;
            node.top = Val::Auto;
        },
    }
}

/// Builds an anchored node for a new margin label.
fn margin_label_node(edge: Edge, screen_pos: Vec2, viewport_size: Vec2) -> Node {
    let mut node = Node {
        position_type: PositionType::Absolute,
        ..default()
    };
    apply_margin_label_anchor(&mut node, edge, screen_pos, viewport_size);
    node
}

/// Updates an existing margin label or creates a new one.
pub fn update_or_create_margin_label(
    commands: &mut Commands,
    label_query: &mut Query<(Entity, &MarginLabel, &mut Text, &mut Node, &mut TextColor)>,
    params: MarginLabelParams,
) {
    let mut found = false;
    for (_, label, mut label_text, mut node, mut text_color) in label_query {
        if label.camera == params.camera && label.edge == params.edge {
            (**label_text).clone_from(&params.text);
            text_color.0 = params.color;
            apply_margin_label_anchor(
                &mut node,
                params.edge,
                params.screen_pos,
                params.viewport_size,
            );
            found = true;
            break;
        }
    }

    if !found {
        commands.spawn((
            Text::new(params.text),
            TextFont {
                font_size: LABEL_FONT_SIZE,
                ..default()
            },
            TextColor(params.color),
            margin_label_node(params.edge, params.screen_pos, params.viewport_size),
            MarginLabel {
                edge:   params.edge,
                camera: params.camera,
            },
            UiTargetCamera(params.camera),
        ));
    }
}

/// Updates an existing bounds label position or creates a new one.
pub fn update_or_create_bounds_label(
    commands: &mut Commands,
    bounds_query: &mut Query<(Entity, &BoundsLabel, &mut Node), Without<MarginLabel>>,
    camera: Entity,
    screen_pos: Vec2,
) {
    let mut found = false;
    for (_, label, mut node) in bounds_query.iter_mut() {
        if label.camera == camera {
            node.left = Val::Px(screen_pos.x);
            node.top = Val::Px(screen_pos.y);
            found = true;
            break;
        }
    }

    if !found {
        commands.spawn((
            Text::new("screen space bounds"),
            TextFont {
                font_size: LABEL_FONT_SIZE,
                ..default()
            },
            TextColor(Color::srgb(1.0, 1.0, 0.0)),
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(screen_pos.x),
                top: Val::Px(screen_pos.y),
                ..default()
            },
            BoundsLabel { camera },
            UiTargetCamera(camera),
        ));
    }
}
