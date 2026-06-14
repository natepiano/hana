//! Constants for cascade resolution.

/// Upper bound on the parent-walk depth.
///
/// The real maximum is ~4 (panel label → panel → root). The cap is set far
/// above that so a legitimate hierarchy never trips it; exceeding it means a
/// malformed `ChildOf` chain (a cycle Bevy did not catch, or a pathologically
/// deep tree), which terminates at the global default with a `warn!` rather
/// than looping forever.
pub(super) const CASCADE_DEPTH_CAP: usize = 64;

/// Default `DrawLayer` cascade value for panel text runs.
///
/// A cascade default only; the draw-order projection (`render::draw_order`)
/// does not read it for ordering.
pub(crate) const DEFAULT_DRAW_LAYER: i8 = 64;
