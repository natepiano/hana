//! Constants for the `camera_control_panel` module.

use bevy::prelude::Color;
use bevy_diegetic::Px;

// colors
pub(super) const ACTIVE_COLOR: Color = Color::srgb(1.0, 0.9, 0.25);
pub(super) const HEADER_COLOR: Color = Color::srgb(0.3, 1.0, 0.8);
pub(super) const LABEL_COLOR: Color = Color::srgba(0.6, 0.65, 0.8, 0.85);

// table
/// Sized so the longest action label (`Zoom Out`) shares one column with the
/// shorter labels and the arrows stay aligned.
pub(super) const ACTION_COLUMN_WIDTH: Px = Px(70.0);
pub(super) const GUIDANCE_CHILD_GAP: Px = Px(5.0);
/// Width of the `Normal` / `Slow` speed column, sized so both share one column
/// and the binding column starts at the same x in each block.
pub(super) const SPEED_LABEL_COLUMN_WIDTH: Px = Px(56.0);
pub(super) const TABLE_ACTION_ARROW: &str = "->";
pub(super) const TABLE_COLUMN_GAP: f32 = 8.0;
pub(super) const TABLE_DIVIDER_WIDTH: Px = Px(1.0);
pub(super) const TABLE_GROUP_GAP: f32 = 7.0;
pub(super) const TABLE_ROW_GAP: f32 = 3.0;
