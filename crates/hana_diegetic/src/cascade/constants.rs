//! Constants for cascade resolution.

/// Maximum stack size for a single `CascadeProperty` value.
///
/// Cascaded values are cloned during propagation. This budget keeps cascade
/// attributes as cheap handles or small value wrappers, not owned render data.
pub(super) const CASCADE_ATTRIBUTE_BYTES: usize = 32;

/// Upper bound on the parent-walk depth.
///
/// The real maximum is ~4 (panel label → panel → root). The cap is set far
/// above that so a legitimate hierarchy never trips it; exceeding it means a
/// malformed `ChildOf` chain (a cycle Bevy did not catch, or a pathologically
/// deep tree), which terminates at the global default with a `warn!` rather
/// than looping forever.
pub(super) const CASCADE_DEPTH_CAP: usize = 64;
