//! Constants for cascade resolution.

/// Upper bound on the parent-walk depth.
///
/// The real maximum is ~4 (panel label → panel → root). The cap is set far
/// above that so a legitimate hierarchy never trips it; exceeding it means a
/// malformed `ChildOf` chain (a cycle Bevy did not catch, or a pathologically
/// deep tree), which terminates at the global default with a `warn!` rather
/// than looping forever.
pub(super) const CASCADE_DEPTH_CAP: usize = 64;
