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
