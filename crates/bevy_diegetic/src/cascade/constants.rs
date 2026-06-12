//! Constants for cascade resolution.

/// Upper bound on the parent-walk depth.
///
/// The real maximum is ~4 (panel label → panel → root). The cap is set far
/// above that so a legitimate hierarchy never trips it; exceeding it means a
/// malformed `ChildOf` chain (a cycle Bevy did not catch, or a pathologically
/// deep tree), which terminates at the global default with a `warn!` rather
/// than looping forever.
pub(super) const CASCADE_DEPTH_CAP: usize = 64;

/// Default draw layer for panel text runs.
///
/// Sits above any realistic panel render-command count, so default-layer text
/// composites over every backing layer on both sorted and OIT views
/// (`render::constants::DrawOrdinal` derives both ordering mechanisms from
/// it). Assumes a panel's render commands stay below this count; a panel
/// exceeding it would draw commands over its own default-layer text on
/// sorted views.
pub(crate) const DEFAULT_DRAW_LAYER: i8 = 64;
