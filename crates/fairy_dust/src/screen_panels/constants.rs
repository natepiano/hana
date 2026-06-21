//! Constants for the `screen_panels` module.

use bevy::prelude::Color;
use bevy_diegetic::Pt;
use bevy_diegetic::Px;

// body
pub(super) const BODY_COLOR: Color = Color::srgba(0.68, 0.72, 0.82, 0.9);

// control
pub(super) const CONTROL_ACTIVE_COLOR: Color = Color::srgb(1.0, 0.9, 0.25);
pub(super) const CONTROL_INACTIVE_COLOR: Color = Color::srgba(0.68, 0.72, 0.82, 0.9);

// description
pub(super) const DESCRIPTION_CHILD_GAP: Px = Px(6.0);
pub(super) const DESCRIPTION_WIDTH: Px = Px(330.0);

// divider
pub(super) const DIVIDER_COLOR: Color = Color::srgba(0.35, 0.8, 1.0, 0.35);

// help overlay
pub(super) const CAMERA_PRESET_KEYS: &str = "shift-C";
pub(super) const CAMERA_PRESET_LABEL: &str = "Cycle camera presets and panel off";
pub(super) const CLOSE_HINT: &str = "Esc to close";
/// One above the default context priority (0) so the help-close Esc is
/// evaluated first and consumes Esc, keeping a caller's Esc binding (e.g. the
/// showcase pause) from also firing while the overlay is open.
pub(super) const HELP_CLOSE_CONTEXT_PRIORITY: usize = 1;
pub(super) const HELP_CLOSE_HINT_COLUMN_WIDTH: Px = Px(120.0);
pub(super) const HELP_CLOSE_HINT_SIZE: Pt = Pt(9.0);
pub(super) const HELP_KEY_COLUMN_WIDTH: Px = Px(120.0);
pub(super) const HELP_PANEL_CHILD_GAP: Px = Px(10.0);
pub(super) const HELP_ROW_GAP: Px = Px(6.0);
pub(super) const HELP_SEPARATOR_HEIGHT: Px = Px(1.0);
pub(super) const HELP_TABLE_COLUMN_GAP: Px = Px(18.0);
pub(super) const HELP_TITLE: &str = "Keyboard Shortcuts";
pub(super) const HOME_AABB_KEYS: &str = "ctrl-shift-A";
pub(super) const HOME_AABB_LABEL: &str = "Show bounding box for camera home AnimateToFit";
pub(super) const SCREEN_PANEL_KEYS: &str = "ctrl-shift-L";
pub(super) const SCREEN_PANEL_LABEL: &str = "Toggle screen space panels off/on";

// performance
pub(super) const GPU_METER_PANEL_WIDTH_FRACTION: f32 = 0.8;
pub(super) const PANEL_SEPARATOR_COLOR: Color = Color::srgba(0.1, 0.4, 0.6, 0.3);
pub(super) const PANEL_SEPARATOR_THICKNESS: f32 = 1.0;
pub(super) const STATS_DESC_COLOR: Color = Color::srgba(0.60, 0.66, 0.76, 0.68);
pub(super) const STATS_DESC_FONT_SIZE: f32 = 9.0;
pub(super) const STATS_DETAIL_INDENT: f32 = 8.0;
pub(super) const STATS_GROUP_GAP: f32 = 6.0;
pub(super) const STATS_HEADER_FONT_SIZE: f32 = 11.25;
pub(super) const STATS_INTRA_GAP: f32 = 2.0;
pub(super) const STATS_ROW_WIDTH: f32 = 260.0;
pub(super) const STATS_SECTION_FONT_SIZE: f32 = 10.0;
pub(super) const STATS_SECTION_GAP: f32 = 8.0;
pub(super) const STATUS_LABEL_COLOR: Color = Color::srgba(0.7, 0.78, 0.92, 0.85);
pub(super) const STATUS_TEXT_COLOR: Color = Color::srgba(1.0, 1.0, 1.0, 0.9);

// title bar
/// Identity and visible label of the always-present help chip. Highlighted
/// while the keyboard-shortcut overlay is open.
pub(super) const HELP_CONTROL: &str = "?";
pub(super) const SEPARATOR_HEIGHT: Px = Px(18.0);
pub(super) const SEPARATOR_WIDTH: Px = Px(1.0);
pub(super) const TITLE_BAR_CHILD_GAP: Px = Px(10.0);
pub(super) const TITLE_BAR_DEFAULT_TITLE: &str = "CONTROLS";
/// Gap between the words of a segmented control — tighter than
/// `TITLE_BAR_CHILD_GAP` so the segments read as one label.
pub(super) const TITLE_BAR_SEGMENT_GAP: Px = Px(6.0);
