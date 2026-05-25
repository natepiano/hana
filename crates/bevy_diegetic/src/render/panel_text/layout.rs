use bevy::prelude::Component;

use crate::layout::BoundingBox;

/// Layout payload for a panel-text child (a [`WorldText`](crate::WorldText)
/// entity also marked [`PanelChild`](crate::render::world_text::PanelChild)).
///
/// Stores the layout-computed bounding box and panel scale factors needed to
/// build panel-local glyph meshes.
#[derive(Component, Clone, Debug)]
pub struct PanelTextLayout {
    /// Index of the source element in the layout tree.
    pub element_idx:   usize,
    /// Index of the render command that produced this text child.
    /// Used for Z-offset layering in Geometry mode.
    pub command_index: usize,
    /// Layout-computed position and size in layout coordinates.
    pub bounds:        BoundingBox,
    /// X scale: points to meters.
    pub scale_x:       f32,
    /// Y scale: points to meters.
    pub scale_y:       f32,
    /// `Anchor` X offset in world units.
    pub anchor_x:      f32,
    /// `Anchor` Y offset in world units.
    pub anchor_y:      f32,
    /// Active clip rect in layout coordinates, or `None` if unclipped.
    pub clip_rect:     Option<BoundingBox>,
}
