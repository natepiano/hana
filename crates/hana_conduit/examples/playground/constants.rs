//! Constants for the playground example, grouped by domain.

use bevy::prelude::*;
use bevy_diegetic::Pt;

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
pub(crate) const EXAMPLE_TITLE: &str = "Cable Playground";

// cable
pub(crate) const DEFAULT_CABLE_RESOLUTION: u32 = 0;
pub(crate) const MIN_TAUT_CABLE_SLACK: f32 = 1.0;
pub(crate) const SLACK_NORMAL: f32 = 1.15;

// cap styles labels
/// Yellow highlight on the Esc line while the tube lights are paused; matches
/// the canonical Fairy Dust active color.
pub(crate) const CAP_STYLE_ESC_ACTIVE_COLOR: Color = NAV_PANEL_ACTIVE_COLOR;
pub(crate) const CAP_STYLE_ESC_TEXT: &str = "Esc - Pause lights";
/// Z of the Esc line, set toward the camera from the section title.
pub(crate) const CAP_STYLE_INFO_Z: f32 = 2.8;
/// Per-cylinder cap-name labels, left to right.
pub(crate) const CAP_STYLE_LABEL_NAMES: [&str; 3] = ["Round", "Flat", "None"];
/// Small font shared by the per-cylinder names and the info lines.
pub(crate) const CAP_STYLE_LABEL_SIZE: f32 = 0.32;
/// X multipliers (times [`CAP_STYLE_TUBE_SPACING`]) of each cylinder center.
pub(crate) const CAP_STYLE_LABEL_X_MULTIPLIERS: [f32; 3] = [-1.5, 0.0, 1.5];
/// Z of the per-cylinder name labels, set just under the hovering cylinders.
pub(crate) const CAP_STYLE_LABEL_Z: f32 = 0.7;
/// Z of the "Cap Styles" title — pulled back toward the cylinders to leave room
/// for the info lines below it.
pub(crate) const CAP_STYLE_TITLE_Z: f32 = 2.0;

// camera
pub(crate) const HOME_PITCH: f32 = 0.45;
pub(crate) const HOME_YAW: f32 = 0.0;
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
pub(crate) const NODE_COLOR: Color = Color::srgba(0.4, 0.6, 0.8, 0.9);
pub(crate) const OBSTACLE_COLOR: Color = Color::srgb(0.8, 0.2, 0.2);
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
/// Ground hint below the "Connector Model" title.
pub(crate) const CONNECTOR_DRAG_HINT_TEXT: &str = "Drag the plugs to compare";
/// Z of the drag hint, set toward the camera from the forward plug.
pub(crate) const CONNECTOR_DRAG_HINT_Z_OFFSET: f32 = 1.1;
/// Standing lane labels left of each fixed endpoint, indexed to match the lane
/// index constants (fixed/as-spawned/rotating = Front/Middle/Back).
pub(crate) const CONNECTOR_LANE_LABELS: [&str; 3] = ["Front", "Middle", "Back"];
/// Per-lane alignment description, shown centered below each lane name.
pub(crate) const CONNECTOR_LANE_DESCRIPTIONS: [&str; 3] = [
    "Fixed (no roll)",
    "AsSpawned (plug keeps its spawn orientation)",
    "Rotating (follows twist)",
];
/// Orange, kept in normal range so the analytic edges stay crisp.
pub(crate) const CONNECTOR_LANE_LABEL_COLOR: Color = Color::srgb(0.95, 0.55, 0.15);
/// Lane name (Front/Middle/Back) height.
pub(crate) const CONNECTOR_LANE_NAME_SIZE: f32 = 0.2;
/// Description height — smaller than the name, centered below it.
pub(crate) const CONNECTOR_LANE_DESC_SIZE: f32 = 0.11;
/// Wrap width (world meters) for the description line.
pub(crate) const CONNECTOR_LANE_DESC_WRAP_WIDTH: f32 = 1.6;
/// Vertical gap (world meters) from the name's baseline to the description top.
pub(crate) const CONNECTOR_LANE_NAME_DESC_GAP: f32 = 0.12;
/// Gap (world meters) left of a lane's fixed endpoint to the label block center.
pub(crate) const CONNECTOR_LANE_LABEL_GAP: f32 = 1.0;
/// Per-lane vertical offset (world meters) so the three label blocks fan apart
/// on screen instead of piling up — the lanes are too close in Z to separate on
/// their own. Indexed to match the lane constants (Front/Middle/Back); Front
/// (nearest, lowest on screen) drops, Back (farthest, highest) rises.
pub(crate) const CONNECTOR_LANE_LABEL_Y_OFFSETS: [f32; 3] = [-0.7, 0.2, 1.1];

// detach demo
pub(crate) const DETACH_DEMO_ENDPOINT_X_OFFSET: f32 = 2.0;
pub(crate) const DETACH_DEMO_ROW_DESPAWN_INDEX: usize = 2;
pub(crate) const DETACH_DEMO_ROW_FREEZE_INDEX: usize = 0;
pub(crate) const DETACH_DEMO_ROW_SLACK_BUMP_INDEX: usize = 1;
pub(crate) const DETACH_DEMO_ROW_Z: [f32; 3] = [-1.5, 0.0, 1.5];
pub(crate) const DETACH_DEMO_SLACK_BUMP: f32 = 0.35;
pub(crate) const DETACH_DEMO_SPHERE_RINGS: u32 = 48;
pub(crate) const DETACH_DEMO_SPHERE_SECTORS: u32 = 48;
/// Per-row sphere caption text, indexed by row (freeze, slack-bump, despawn).
pub(crate) const DETACH_DEMO_LABELS: [&str; 3] = [
    "Click - cable freezes",
    "Click - cable hangs in place",
    "Click - cable disappears",
];
/// Emissive caption colors matching each sphere. Kept just above 1.0 in the
/// dominant channel: bright enough to read as self-lit, low enough that the
/// analytic coverage gradient survives the tonemapper instead of clipping to a
/// hard, aliased near-white edge. Indexed by row (freeze, slack-bump, despawn).
pub(crate) const DETACH_DEMO_LABEL_COLORS: [Color; 3] = [
    Color::linear_rgb(0.35, 1.4, 0.35),
    Color::linear_rgb(0.4, 0.8, 1.6),
    Color::linear_rgb(1.5, 0.45, 0.45),
];
/// Gap (world meters) from a sphere's center to the right edge of its caption,
/// which sits to the left so the cables and sphere stay clear.
pub(crate) const DETACH_DEMO_LABEL_SIDE_GAP: f32 = 0.6;
/// Sphere caption font height (world meters).
pub(crate) const DETACH_DEMO_LABEL_SIZE: f32 = 0.15;
/// Wrap width (world meters) for a sphere caption, breaking it onto two lines.
pub(crate) const DETACH_DEMO_LABEL_WRAP_WIDTH: f32 = 1.6;
/// The "R - Reset" ground line below the Detach Policy title.
pub(crate) const DETACH_R_RESET_ACTIVE_COLOR: Color = NAV_PANEL_ACTIVE_COLOR;
pub(crate) const DETACH_R_RESET_TEXT: &str = "R - Reset";
/// How long the "R - Reset" line flashes yellow after `R` is pressed.
pub(crate) const DETACH_R_RESET_FLASH_SECONDS: f32 = 0.5;
/// Z of the "R - Reset" line, set toward the camera from the section title.
pub(crate) const DETACH_R_RESET_Z: f32 = 4.3;

// face labels
/// Over-bright orange for the box face labels — unlit and HDR so it reads as
/// emissive and blooms. Scaled so its luminance clears the `BLOOM_THRESHOLD`
/// (3.0) the camera bloom uses; the earlier value sat just under it and never
/// glowed. Needs camera HDR + bloom.
pub(crate) const BOX_LABEL_EMISSIVE_COLOR: Color = Color::linear_rgb(12.0, 4.0, 0.4);
pub(crate) const DRAG_FACE_LABEL_TEXT: &str = "Drag Me";
/// Face-label text height as a fraction of the cube edge.
pub(crate) const FACE_LABEL_SIZE_RATIO: f32 = 0.15;
/// Solver-comparison cube-end words, indexed by row (catenary, linear, routed).
pub(crate) const SOLVER_FACE_LABELS: [&str; 3] = ["Catenary", "Linear", "Orthogonal"];
/// Solver-comparison face-label height (world meters).
pub(crate) const SOLVER_FACE_LABEL_SIZE: f32 = 0.09;

// shared hub label
/// Camera-facing "Drag Me" label hovering above the hub sphere, colored to match
/// the sphere.
pub(crate) const HUB_LABEL_COLOR: Color = DRAGGABLE_COLOR;
/// Local height above the hub center at which the label hovers.
pub(crate) const HUB_LABEL_HOVER_Y: f32 = 0.7;
/// Hub label height.
pub(crate) const HUB_LABEL_SIZE: f32 = 0.22;
pub(crate) const HUB_LABEL_TEXT: &str = "Drag Me";

// inside view label
/// Emissive world text centered inside the large tube.
pub(crate) const INSIDE_VIEW_LABEL_SIZE: f32 = 0.28;
pub(crate) const INSIDE_VIEW_LABEL_TEXT: &str = "Look, it's a tube!";

// ground labels
pub(crate) const GROUND_LABEL_COLOR: Color = Color::srgb(0.85, 0.9, 1.0);
pub(crate) const GROUND_LABEL_SIZE: f32 = 0.55;
pub(crate) const GROUND_LABEL_Y: f32 = 0.01;
/// Z offset toward the camera so the label sits in front of the section's mesh.
pub(crate) const GROUND_LABEL_Z: f32 = 3.4;
/// Per-section title Z (relative to `SECTION_Z`). Sections whose geometry the
/// label should sit directly beneath use the geometry's Z; the rest keep the
/// pulled-forward `GROUND_LABEL_Z`. Cap Styles places its own title.
pub(crate) const SECTION_GROUND_LABEL_Z: [f32; SECTION_COUNT] = [
    0.0,            // Simple Catenary — centered under the cable (z = 0)
    GROUND_LABEL_Z, // Cap Styles — unused (own label path)
    1.5,            // Solver Comparison — under the orthogonal (routed) cable row
    0.0,            // Entity Attachment — centered under the cable (z = 0)
    0.0,            // Shared Hub — centered under the sphere (z = 0)
    GROUND_LABEL_Z, // Orthogonal Routing
    GROUND_LABEL_Z, // Detach Policy
    GROUND_LABEL_Z, // Inside View
    GROUND_LABEL_Z, // Connector Model
];

// layout
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
pub(crate) const ORTHOGONAL_ROUTING_SECTION_INDEX: usize = 5;
pub(crate) const RAY_EPSILON: f32 = 1e-6;
pub(crate) const SECTION_COUNT: usize = 9;
pub(crate) const SECTION_SPACING: f32 = 16.0;
pub(crate) const SECTION_TITLES: [&str; SECTION_COUNT] = [
    "Simple Catenary",
    "Cap Styles",
    "Solver Comparison",
    "Entity Attachment",
    "Shared Hub",
    "Orthogonal Routing",
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
pub(crate) const POINT_LIGHT_INTENSITY: f32 = 20000.0;
pub(crate) const POINT_LIGHT_RANGE: f32 = 2.0;

// nav panel
/// Highlight color for the current section row and the last-used arrow; matches
/// the canonical Fairy Dust title-bar active yellow.
pub(crate) const NAV_PANEL_ACTIVE_COLOR: Color = Color::srgb(1.0, 0.9, 0.25);
/// Color for an arrow that cannot navigate further (at the first/last section).
pub(crate) const NAV_PANEL_DISABLED_COLOR: Color = Color::srgb(0.3, 0.32, 0.38);
pub(crate) const NAV_PANEL_HEADER_GAP: f32 = 8.0;
/// "Sections" header font — two points larger than the section rows.
pub(crate) const NAV_PANEL_HEADER_SIZE: Pt = Pt(13.0);
pub(crate) const NAV_PANEL_LEFT_ARROW: &str = "\u{2190}";
/// Color for an unselected, navigable section row or arrow.
pub(crate) const NAV_PANEL_NORMAL_COLOR: Color = Color::srgb(0.62, 0.66, 0.72);
pub(crate) const NAV_PANEL_RIGHT_ARROW: &str = "\u{2192}";
pub(crate) const NAV_PANEL_ROW_GAP: f32 = 2.0;

// section bounds
pub(crate) const SECTION_BOUNDS_CENTER_Y_MULTIPLIER: f32 = 0.5;
pub(crate) const SECTION_BOUNDS_COLOR: Color = Color::NONE;
pub(crate) const SECTION_BOUNDS_DEPTH: f32 = 5.0;
pub(crate) const SECTION_BOUNDS_HEIGHT_PADDING: f32 = 2.0;
pub(crate) const SECTION_BOUNDS_SPAN_MULTIPLIER: f32 = 2.0;
pub(crate) const SECTION_BOUNDS_WIDTH_PADDING: f32 = 2.0;

// title bar
pub(crate) const OVERVIEW_CONTROL: &str = "F Overview";
/// Key hint preceding the two highlightable slack segments.
pub(crate) const SLACK_HINT: &str = "Slack";
pub(crate) const SLACK_MINUS_LABEL: &str = "-";
/// Stable identity for the minus segment, used by the chip-pulse wiring.
pub(crate) const SLACK_MINUS_SEGMENT_ID: &str = "slack-minus";
pub(crate) const SLACK_PLUS_LABEL: &str = "+";
/// Stable identity for the plus segment, used by the chip-pulse wiring.
pub(crate) const SLACK_PLUS_SEGMENT_ID: &str = "slack-plus";
/// How long a slack segment stays highlighted after its key is pressed.
pub(crate) const SLACK_PULSE_SECONDS: f32 = 0.5;

// tube mesh
pub(crate) const TUBE_RADIUS: f32 = 0.06;

// ui
pub(crate) const SECTION_INFO_CENTER_X_PERCENT: f32 = 50.0;
pub(crate) const SECTION_INFO_LEFT_OFFSET: f32 = -200.0;
pub(crate) const SECTION_INFO_TEXTS: [(usize, &str); 0] = [];
pub(crate) const SECTION_INFO_TOP: f32 = 60.0;
pub(crate) const SECTION_INFO_WIDTH: f32 = 400.0;
pub(crate) const UI_FONT_SIZE: f32 = 14.0;
