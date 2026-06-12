use bevy::prelude::Component;

use crate::PanelFieldId;
use crate::layout::BoundingBox;

/// Layout payload for a panel-text run (a
/// [`TextContent`](crate::TextContent) entity).
///
/// Stores the layout-computed bounding box and panel scale factors needed to
/// build panel-local glyph meshes.
#[derive(Component, Clone, Debug)]
pub struct PanelTextLayout {
    /// Panel-local id of the source text run, plus the line ordinal within that
    /// run (`0` for an unwrapped run). Together they form the reconcile reuse
    /// key, replacing the former positional `(element_idx, command_index)` pair
    /// so a named run survives a sibling reorder.
    pub id:          PanelFieldId,
    /// Line ordinal of this command within its run (`0`-based), so a wrapped
    /// multi-line run reuses each line stably.
    pub line_index:  usize,
    /// Index of the source element in the layout tree.
    pub element_idx: usize,
    /// Geometry draw slot recorded when this text command was emitted (the
    /// slot the next geometry command occupies). Drives the run's non-OIT
    /// depth nudge so the run draws above the geometry emitted before it.
    pub draw_slot:   usize,
    /// Layout-computed position and size in layout coordinates.
    pub bounds:      BoundingBox,
    /// X scale: points to meters.
    pub scale_x:     f32,
    /// Y scale: points to meters.
    pub scale_y:     f32,
    /// `Anchor` X offset in world units.
    pub anchor_x:    f32,
    /// `Anchor` Y offset in world units.
    pub anchor_y:    f32,
    /// Active clip rect in layout coordinates, or `None` if unclipped.
    pub clip_rect:   Option<BoundingBox>,
}

impl PanelTextLayout {
    /// Bit-equality over the layout fields a panel-text glyph mesh depends on,
    /// used to gate per-run rebuilds.
    ///
    /// Compares `bounds`, `scale_x`, `scale_y`, `anchor_x`, `anchor_y`, and
    /// `clip_rect` via `to_bits`. Excludes the reuse-identity fields
    /// (`id`/`line_index`/`element_idx`/`draw_slot`), constant within a
    /// reused slot.
    pub(super) fn gating_eq(&self, other: &Self) -> bool {
        let Self {
            bounds,
            scale_x,
            scale_y,
            anchor_x,
            anchor_y,
            clip_rect,
            id: _,
            line_index: _,
            element_idx: _,
            draw_slot: _,
        } = self;

        bbox_bits(bounds) == bbox_bits(&other.bounds)
            && scale_x.to_bits() == other.scale_x.to_bits()
            && scale_y.to_bits() == other.scale_y.to_bits()
            && anchor_x.to_bits() == other.anchor_x.to_bits()
            && anchor_y.to_bits() == other.anchor_y.to_bits()
            && clip_rect.as_ref().map(bbox_bits) == other.clip_rect.as_ref().map(bbox_bits)
    }
}

/// Returns a [`BoundingBox`]'s four floats as raw bits for exact comparison.
const fn bbox_bits(bounds: &BoundingBox) -> [u32; 4] {
    [
        bounds.x.to_bits(),
        bounds.y.to_bits(),
        bounds.width.to_bits(),
        bounds.height.to_bits(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bbox(x: f32, y: f32, width: f32, height: f32) -> BoundingBox {
        BoundingBox {
            x,
            y,
            width,
            height,
        }
    }

    fn sample_layout() -> PanelTextLayout {
        PanelTextLayout {
            id:          PanelFieldId::named("sample"),
            line_index:  0,
            element_idx: 0,
            draw_slot:   0,
            bounds:      bbox(1.0, 2.0, 30.0, 12.0),
            scale_x:     0.5,
            scale_y:     0.5,
            anchor_x:    0.0,
            anchor_y:    0.0,
            clip_rect:   None,
        }
    }

    #[test]
    fn gating_eq_true_for_identical() {
        let layout = sample_layout();
        assert!(layout.gating_eq(&layout.clone()));
    }

    #[test]
    fn gating_eq_detects_bounds_change() {
        let base = sample_layout();
        let mut widened = sample_layout();
        widened.bounds.width = 40.0;
        assert!(!base.gating_eq(&widened));
    }

    #[test]
    fn gating_eq_ignores_reuse_key() {
        // element_idx/draw_slot form the reuse key, constant within a slot.
        let base = sample_layout();
        let mut rekeyed = sample_layout();
        rekeyed.element_idx = 7;
        rekeyed.draw_slot = 9;
        assert!(base.gating_eq(&rekeyed));
    }

    #[test]
    fn gating_eq_detects_clip_rect_presence() {
        let base = sample_layout();
        let mut clipped = sample_layout();
        clipped.clip_rect = Some(bbox(0.0, 0.0, 100.0, 100.0));
        assert!(!base.gating_eq(&clipped));
    }

    #[test]
    fn gating_eq_distinguishes_signed_zero() {
        // to_bits, not ==: +0.0 and -0.0 are distinct bit patterns.
        let mut positive = sample_layout();
        let mut negative = sample_layout();
        positive.bounds.x = 0.0;
        negative.bounds.x = -0.0;
        assert!(!positive.gating_eq(&negative));
    }
}
