//! Shared styling constants for `ui/` panel layouts.

use bevy::prelude::Color;
use bevy_diegetic::Pt;
use bevy_diegetic::Px;

pub(crate) const RADIUS: Px = Px(12.0);
pub(crate) const FRAME_PAD: Px = Px(2.0);
pub(crate) const BORDER: Px = Px(2.0);
pub(crate) const INSET: Px = Px(FRAME_PAD.0 + BORDER.0);
pub(crate) const INNER_RADIUS: Px = Px(RADIUS.0 - INSET.0);
pub(crate) const INNER_PAD: Px = Px(10.0);
pub(crate) const INNER_BORDER_WIDTH: Px = Px(1.0);

pub(crate) const TITLE_SIZE: Pt = Pt(14.0);

pub(crate) const INNER_BG: Color = Color::srgba(0.02, 0.03, 0.07, 0.50);
pub(crate) const BORDER_ACCENT: Color = Color::srgba(0.15, 0.7, 0.9, 0.5);
pub(crate) const BORDER_DIM: Color = Color::srgba(0.1, 0.4, 0.6, 0.3);
pub(crate) const TITLE_COLOR: Color = Color::srgb(0.9, 0.95, 1.0);
