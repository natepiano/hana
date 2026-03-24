//! Callout primitives for gizmo-based annotations.
//!
//! Provides reusable drawing helpers for dimension arrows, dashed lines,
//! and other annotation elements used by the typography overlay and
//! future callout systems.

mod primitives;

pub use primitives::ARROWHEAD_SIZE;
pub use primitives::draw_dashed_line;
pub use primitives::draw_dimension_arrow;
