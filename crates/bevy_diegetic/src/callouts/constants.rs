//! Constants for callout primitives.

/// Default shaft thickness of a [`CalloutLine`](super::CalloutLine), in
/// world meters. Tuned to read clearly at the meter-scale viewing
/// distances typical of diegetic panels without dominating the scene.
pub(super) const DEFAULT_LINE_THICKNESS: f32 = 0.002;

/// Default radius/extent of an end cap (arrow, circle, square, diamond)
/// on a [`CalloutLine`](super::CalloutLine), in world meters. Roughly 4×
/// the default shaft thickness so caps are clearly distinguishable from
/// the line.
pub(super) const DEFAULT_CAP_SIZE: f32 = 0.008;

/// Multiplier applied to a line's `thickness` to derive the half-height
/// of the SDF mesh's *hidden* anti-aliasing band (the band beyond the
/// visible edge that still receives SDF coverage). Empirically tuned —
/// values much smaller produce visible edge clipping; much larger waste
/// fragment work on transparent pixels.
pub(super) const HIDDEN_HALF_HEIGHT_MULTIPLIER: f32 = 4.0;
