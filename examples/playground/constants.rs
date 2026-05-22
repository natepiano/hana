//! Constants for the playground example, grouped by domain.

use bevy::prelude::*;

// animation
pub(crate) const LIGHT_TRAVEL_CYCLE_SECONDS: f32 =
    LIGHT_TRAVEL_HOLD_END_SECONDS + LIGHT_TRAVEL_SEGMENT_DURATION_SECONDS;
pub(crate) const LIGHT_TRAVEL_FORWARD_END_SECONDS: f32 =
    LIGHT_TRAVEL_HOLD_DURATION_SECONDS + LIGHT_TRAVEL_SEGMENT_DURATION_SECONDS;
pub(crate) const LIGHT_TRAVEL_HOLD_DURATION_SECONDS: f32 = 0.5;
pub(crate) const LIGHT_TRAVEL_HOLD_END_SECONDS: f32 =
    LIGHT_TRAVEL_FORWARD_END_SECONDS + LIGHT_TRAVEL_HOLD_DURATION_SECONDS;
pub(crate) const LIGHT_TRAVEL_SEGMENT_DURATION_SECONDS: f32 = 2.0;

// app
pub(crate) const PLAYGROUND_WINDOW_TITLE: &str = "Playground";

// cable
pub(crate) const DEFAULT_CABLE_RESOLUTION: u32 = 0;
pub(crate) const OBSTACLE_HALF_EXTENTS: Vec3 = Vec3::new(0.8, 0.8, 0.8);
pub(crate) const SLACK_NORMAL: f32 = 1.15;

// camera
pub(crate) const NAVIGATION_DURATION_MS: u64 = 1200;
pub(crate) const ZOOM_DURATION_MS: u64 = 1000;
pub(crate) const ZOOM_MARGIN_GROUND: f32 = 0.05;
pub(crate) const ZOOM_MARGIN_MESH: f32 = 0.15;
pub(crate) const ZOOM_MARGIN_NAVIGATION: f32 = 0.12;

// colors
pub(crate) const CABLE_COLOR: Color = Color::srgb(0.9, 0.5, 0.1);
pub(crate) const DESPAWN_GREEN: Color = Color::srgb(0.3, 0.8, 0.3);
pub(crate) const DESPAWN_RED: Color = Color::srgb(0.8, 0.3, 0.3);
pub(crate) const DETACH_BUMP_BLUE: Color = Color::srgb(0.3, 0.5, 0.9);
pub(crate) const DRAGGABLE_COLOR: Color = Color::srgb(0.2, 0.7, 0.7);
pub(crate) const NAVIGATION_BAR_BACKGROUND: Color = Color::srgba(0.0, 0.0, 0.0, 0.7);
pub(crate) const NAVIGATION_BUTTON_BACKGROUND: Color = Color::srgba(1.0, 1.0, 1.0, 0.15);
pub(crate) const NODE_COLOR: Color = Color::srgba(0.4, 0.6, 0.8, 0.4);
pub(crate) const OBSTACLE_COLOR: Color = Color::srgba(0.8, 0.2, 0.2, 0.25);
pub(crate) const POINT_LIGHT_COLOR: Color = Color::srgb(1.0, 0.95, 0.8);
pub(crate) const SECTION_INFO_BACKGROUND: Color = Color::srgba(0.0, 0.0, 0.0, 0.5);
pub(crate) const TRANSPARENT_TUBE_COLOR: Color = Color::srgba(0.85, 0.55, 0.2, 0.2);

// connector
pub(crate) const CONNECTOR_LANE_AS_SPAWNED_INDEX: usize = 1;
pub(crate) const CONNECTOR_LANE_FIXED_INDEX: usize = 0;
pub(crate) const CONNECTOR_LANE_ROTATING_INDEX: usize = 2;
pub(crate) const CONNECTOR_LANE_Z: [f32; 3] = [1.5, 0.0, -1.5];
pub(crate) const CONNECTOR_MODEL_PATH: &str = "models/power_plug.glb#Scene0";
pub(crate) const CONNECTOR_MODEL_SCALE: f32 = 15.0;

// detach demo
pub(crate) const DETACH_DEMO_ENDPOINT_X_OFFSET: f32 = 2.0;
pub(crate) const DETACH_DEMO_ROW_DESPAWN_INDEX: usize = 2;
pub(crate) const DETACH_DEMO_ROW_FREEZE_INDEX: usize = 0;
pub(crate) const DETACH_DEMO_ROW_SLACK_BUMP_INDEX: usize = 1;
pub(crate) const DETACH_DEMO_ROW_Z: [f32; 3] = [-1.5, 0.0, 1.5];
pub(crate) const DETACH_DEMO_SLACK_BUMP: f32 = 0.35;
pub(crate) const DETACH_DEMO_SPHERE_RINGS: u32 = 16;
pub(crate) const DETACH_DEMO_SPHERE_SECTORS: u32 = 16;

// layout
pub(crate) const ASTAR_SECTION_INDEX: usize = 5;
pub(crate) const CAP_STYLES_SECTION_INDEX: usize = 1;
pub(crate) const CAP_STYLE_RADIUS_MULTIPLIER: f32 = 5.0;
pub(crate) const CAP_STYLE_TUBE_OFFSET: f32 = 0.8;
pub(crate) const CAP_STYLE_TUBE_SPACING: f32 = 2.0;
pub(crate) const CATENARY_SECTION_INDEX: usize = 0;
pub(crate) const CONNECTOR_SECTION_INDEX: usize = 8;
pub(crate) const DETACH_DEMO_SECTION_INDEX: usize = 6;
pub(crate) const DRAGGABLE_CUBE_SIZE: f32 = 0.45;
pub(crate) const ENTITY_ATTACHMENT_SECTION_INDEX: usize = 3;
pub(crate) const GROUND_DEPTH: f32 = 14.0;
pub(crate) const GROUND_WIDTH: f32 = 160.0;
pub(crate) const HUB_SPHERE_RADIUS: f32 = 0.35;
pub(crate) const INSIDE_VIEW_RADIUS_MULTIPLIER: f32 = 25.0;
pub(crate) const INSIDE_VIEW_SECTION_INDEX: usize = 7;
pub(crate) const NODE_CUBE_SIZE: f32 = 0.3;
pub(crate) const NODE_Y: f32 = 2.0;
pub(crate) const RAY_EPSILON: f32 = 1e-6;
pub(crate) const SECTION_COUNT: usize = 9;
pub(crate) const SECTION_SPACING: f32 = 16.0;
pub(crate) const SECTION_TITLES: [&str; SECTION_COUNT] = [
    "Simple Catenary",
    "Cap Styles",
    "Solver Comparison",
    "Entity Attachment",
    "Shared Hub",
    "A* Routing",
    "Detach Policy",
    "Inside View",
    "Connector Model",
];
pub(crate) const SECTION_X: [f32; SECTION_COUNT] = [
    -4.0 * SECTION_SPACING,
    -3.0 * SECTION_SPACING,
    -2.0 * SECTION_SPACING,
    -SECTION_SPACING,
    0.0 * SECTION_SPACING,
    1.0 * SECTION_SPACING,
    2.0 * SECTION_SPACING,
    3.0 * SECTION_SPACING,
    4.0 * SECTION_SPACING,
];
pub(crate) const SECTION_Z: f32 = 0.0;
pub(crate) const SHARED_HUB_SECTION_INDEX: usize = 4;
pub(crate) const SLACK_ADJUSTMENT_STEP: f32 = 0.01;
pub(crate) const SOLVER_COMPARISON_SECTION_INDEX: usize = 2;
pub(crate) const SPAN_HALF_X: f32 = 3.0;

// lighting
pub(crate) const DIRECTIONAL_LIGHT_ILLUMINANCE: f32 = 3000.0;
pub(crate) const POINT_LIGHT_INTENSITY: f32 = 20000.0;
pub(crate) const POINT_LIGHT_RANGE: f32 = 2.0;

// navigation ui
pub(crate) const NAVIGATION_BAR_BORDER_RADIUS: f32 = 6.0;
pub(crate) const NAVIGATION_BAR_BOTTOM: f32 = 16.0;
pub(crate) const NAVIGATION_BAR_CENTER_X_PERCENT: f32 = 50.0;
pub(crate) const NAVIGATION_BAR_COLUMN_GAP: f32 = 12.0;
pub(crate) const NAVIGATION_BAR_HORIZONTAL_OFFSET: f32 = -150.0;
pub(crate) const NAVIGATION_BAR_HORIZONTAL_PADDING: f32 = 12.0;
pub(crate) const NAVIGATION_BAR_VERTICAL_PADDING: f32 = 8.0;
pub(crate) const NAVIGATION_BUTTON_BORDER_RADIUS: f32 = 4.0;
pub(crate) const NAVIGATION_BUTTON_HORIZONTAL_PADDING: f32 = 10.0;
pub(crate) const NAVIGATION_BUTTON_VERTICAL_PADDING: f32 = 4.0;
pub(crate) const NAVIGATION_FONT_SIZE: f32 = 16.0;
pub(crate) const NAVIGATION_LABEL_WIDTH: f32 = 260.0;
pub(crate) const NAVIGATION_NEXT_LABEL: &str = ">";
pub(crate) const NAVIGATION_PREVIOUS_LABEL: &str = "<";

// section bounds
pub(crate) const SECTION_BOUNDS_CENTER_Y_MULTIPLIER: f32 = 0.5;
pub(crate) const SECTION_BOUNDS_COLOR: Color = Color::NONE;
pub(crate) const SECTION_BOUNDS_DEPTH: f32 = 5.0;
pub(crate) const SECTION_BOUNDS_HEIGHT_PADDING: f32 = 2.0;
pub(crate) const SECTION_BOUNDS_SPAN_MULTIPLIER: f32 = 2.0;
pub(crate) const SECTION_BOUNDS_WIDTH_PADDING: f32 = 2.0;

// tube mesh
pub(crate) const JOINT_RADIUS_MULTIPLIER: f32 = 1.5;
pub(crate) const JOINT_SPHERE_SEGMENTS: u32 = 16;
pub(crate) const TUBE_RADIUS: f32 = 0.06;
pub(crate) const TUBE_SIDES: u32 = 32;

// ui
pub(crate) const CAMERA_HELP: &str = "Orbit: Middle-mouse (or trackpad)\n\
                                      Pan: Shift + middle-mouse\n\
                                      Zoom: Scroll wheel (or pinch)";
pub(crate) const DEFAULT_ELBOW_ANGLE_THRESHOLD_DEG: f32 = 25.0;
pub(crate) const DEFAULT_ELBOW_ARM_MULTIPLIER: f32 = 1.0;
pub(crate) const DEFAULT_ELBOW_BEND_RADIUS_MULTIPLIER: f32 = 1.0;
pub(crate) const DEFAULT_ELBOW_MIN_RADIUS_MULTIPLIER: f32 = 0.5;
pub(crate) const DEFAULT_ELBOW_RINGS_PER_RIGHT_ANGLE: u32 = 32;
pub(crate) const ELBOW_ANGLE_THRESHOLD_DEG_SLIDER_MAX: f32 = 90.0;
pub(crate) const ELBOW_ANGLE_THRESHOLD_DEG_SLIDER_MIN: f32 = 1.0;
pub(crate) const ELBOW_ARM_MULTIPLIER_SLIDER_MAX: f32 = 3.0;
pub(crate) const ELBOW_ARM_MULTIPLIER_SLIDER_MIN: f32 = 0.1;
pub(crate) const ELBOW_BEND_RADIUS_MULTIPLIER_SLIDER_MAX: f32 = 20.0;
pub(crate) const ELBOW_BEND_RADIUS_MULTIPLIER_SLIDER_MIN: f32 = 1.0;
pub(crate) const ELBOW_MIN_RADIUS_MULTIPLIER_SLIDER_MAX: f32 = 5.0;
pub(crate) const ELBOW_MIN_RADIUS_MULTIPLIER_SLIDER_MIN: f32 = 0.5;
pub(crate) const ELBOW_RINGS_PER_RIGHT_ANGLE_SLIDER_MAX: u32 = 32;
pub(crate) const ELBOW_RINGS_PER_RIGHT_ANGLE_SLIDER_MIN: u32 = 2;
pub(crate) const JOINT_RADIUS_MULTIPLIER_SLIDER_MAX: f32 = 4.0;
pub(crate) const JOINT_RADIUS_MULTIPLIER_SLIDER_MIN: f32 = 1.0;
pub(crate) const JOINT_SEGMENTS_SLIDER_MAX: u32 = 32;
pub(crate) const JOINT_SEGMENTS_SLIDER_MIN: u32 = 8;
pub(crate) const KEYBOARD_SHORTCUTS_HELP: &str = "D - Debug gizmos\n\
                                                  F - Full scene\n\
                                                  I - Inspector\n\
                                                  +/- Slack";
pub(crate) const OVERLAY_MARGIN: f32 = 12.0;
pub(crate) const SECTION_INFO_CENTER_X_PERCENT: f32 = 50.0;
pub(crate) const SECTION_INFO_LEFT_OFFSET: f32 = -200.0;
pub(crate) const SECTION_INFO_TEXTS: [(usize, &str); 7] = [
    (
        CAP_STYLES_SECTION_INDEX,
        "Round (transparent) / Flat / None\nEnd caps are independent\nEsc - Pause lights",
    ),
    (
        SOLVER_COMPARISON_SECTION_INDEX,
        "Catenary    Linear    Orthogonal",
    ),
    (ENTITY_ATTACHMENT_SECTION_INDEX, "Drag blue boxes"),
    (SHARED_HUB_SECTION_INDEX, "Drag sphere"),
    (
        DETACH_DEMO_SECTION_INDEX,
        "Click green sphere - cable freezes\n\
         Click red sphere - cable disappears\n\
         R - Reset",
    ),
    (INSIDE_VIEW_SECTION_INDEX, "Look, it's a tube!"),
    (
        CONNECTOR_SECTION_INDEX,
        "Front: Fixed (no roll)\nMiddle: AsSpawned (plug keeps its spawn orientation)\nBack: Rotating (follows twist)\nDrag the plugs to compare",
    ),
];
pub(crate) const SECTION_INFO_TOP: f32 = 60.0;
pub(crate) const SECTION_INFO_WIDTH: f32 = 400.0;
pub(crate) const TUBE_RADIUS_SLIDER_MAX: f32 = 0.3;
pub(crate) const TUBE_RADIUS_SLIDER_MIN: f32 = 0.01;
pub(crate) const TUBE_SIDES_SLIDER_MAX: u32 = 64;
pub(crate) const TUBE_SIDES_SLIDER_MIN: u32 = 1;
pub(crate) const UI_FONT_SIZE: f32 = 14.0;
