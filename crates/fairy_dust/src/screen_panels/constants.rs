//! Constants for the `screen_panels` module.

use bevy::prelude::Color;
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

// title bar
pub(super) const SEPARATOR_HEIGHT: Px = Px(18.0);
pub(super) const SEPARATOR_WIDTH: Px = Px(1.0);
pub(super) const TITLE_BAR_CHILD_GAP: Px = Px(10.0);
pub(super) const TITLE_BAR_DEFAULT_TITLE: &str = "CONTROLS";
