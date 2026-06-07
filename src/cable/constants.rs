//! `ALIGNMENT_FEEDBACK_GUARD` threshold for `on_endpoint_alignment_update`.

// alignment
/// Dot-product threshold above which `on_endpoint_alignment_update` skips
/// writing back to `Transform`. Prevents an infinite recompute cycle of
/// `ComputedCableGeometry` -> `Transform` -> `GlobalTransform`.
pub(super) const ALIGNMENT_FEEDBACK_GUARD: f32 = 0.9999;
