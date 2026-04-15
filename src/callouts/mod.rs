//! Callout primitives for annotations.
//!
//! Panel-backed callout lines are the public API. Legacy gizmo helpers
//! remain crate-internal while the typography overlay is migrated.

mod primitives;

pub use primitives::ArrowStyle;
pub use primitives::CalloutCap;
pub use primitives::CalloutLine;
pub(crate) use primitives::draw_dashed_line;
pub(crate) use primitives::draw_dimension_arrow;
pub use primitives::spawn_callout_line;
