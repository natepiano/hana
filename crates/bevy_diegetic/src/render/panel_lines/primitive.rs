//! Stable source identity for batched panel-line primitives.

use bevy::prelude::Entity;

use crate::layout::PanelLinePrimitiveKey;

/// Stable cross-panel source identity for one line primitive record.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(super) struct PanelLineRenderKey {
    /// Panel entity that owns the primitive source.
    pub panel:  Entity,
    /// Stable primitive key inside the panel's resolved command stream.
    pub source: PanelLinePrimitiveKey,
}
