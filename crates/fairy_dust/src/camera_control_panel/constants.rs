//! Constants for the `camera_control_panel` module.

use bevy::prelude::Color;
use bevy_diegetic::Px;

// colors
pub(super) const ACTIVE_COLOR: Color = Color::srgb(1.0, 0.9, 0.25);
pub(super) const HEADER_COLOR: Color = Color::srgb(0.3, 1.0, 0.8);
pub(super) const LABEL_COLOR: Color = Color::srgba(0.6, 0.65, 0.8, 0.85);
pub(super) const SOURCE_COLOR: Color = Color::srgba(0.35, 0.8, 1.0, 0.95);

// hold
pub(super) const SOURCE_HOLD_SECONDS: f32 = 0.15;

// table
pub(super) const ACTION_COLUMN_MIN_WIDTH: Px = Px(46.0);
pub(super) const GUIDANCE_CHILD_GAP: Px = Px(5.0);
pub(super) const TABLE_ACTION_ARROW: &str = "->";
pub(super) const TABLE_COLUMN_GAP: f32 = 8.0;
pub(super) const TABLE_DIVIDER_WIDTH: Px = Px(1.0);
pub(super) const TABLE_GROUP_GAP: f32 = 7.0;
pub(super) const TABLE_ROW_GAP: f32 = 3.0;
