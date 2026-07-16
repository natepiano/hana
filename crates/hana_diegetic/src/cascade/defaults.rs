//! Default resources for cascade attributes and non-cascade construction
//! defaults.

use bevy::prelude::*;

use crate::layout::Unit;

/// Non-cascade construction defaults.
///
/// Runtime-propagated cascade defaults are stored in
/// [`CascadeDefault<A>`](bevy_kana::CascadeDefault) resources.
/// The fields here are read when panels are built or seeded and are not
/// propagated by the cascade plugin.
#[derive(Resource, Clone, Copy, Debug, Reflect)]
#[reflect(Resource)]
pub struct PanelDefaults {
    /// Construction-time seed for a panel's `Cascade<FontUnit>`. Read once by
    /// the panel authoring bridge when a panel has no explicit `font_unit`;
    /// **not** a cascade global and **not** propagated at runtime.
    pub panel_font_unit: Unit,
    /// Default `layout_unit` for newly-built panels. Read at panel
    /// construction; **not** cascade-propagated at runtime.
    pub layout_unit:     Unit,
}

impl Default for PanelDefaults {
    fn default() -> Self {
        Self {
            panel_font_unit: Unit::Points,
            layout_unit:     Unit::Meters,
        }
    }
}
