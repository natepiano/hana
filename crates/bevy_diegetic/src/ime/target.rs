//! Semantic IME session targets.

use bevy::prelude::Entity;

use super::PanelFieldId;

/// Semantic backing target for an IME session.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ImeTarget {
    /// Editable field authored on a world-space diegetic panel.
    WorldPanelField {
        /// Panel entity that owns the field id.
        panel:    Entity,
        /// Panel-local semantic field id.
        field_id: PanelFieldId,
    },
    /// Editable field authored on a screen-space diegetic panel.
    ScreenPanelField {
        /// Panel entity that owns the field id.
        panel:    Entity,
        /// Panel-local semantic field id.
        field_id: PanelFieldId,
    },
    /// Caller-owned session with app-specific backing state.
    AppOwned {
        /// Entity that owns the app-side field state.
        owner:    Entity,
        /// Owner-local semantic field id.
        field_id: PanelFieldId,
    },
}
