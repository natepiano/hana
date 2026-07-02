//! Default resources for cascade attributes and non-cascade construction
//! defaults.

use bevy::prelude::*;

use super::resolved::CascadeProperty;
use crate::layout::Unit;

/// Global default for one cascading attribute.
///
/// Each cascade attribute owns one resource with this generic wrapper, so
/// `Res<CascadeDefault<A>>::is_changed()` precisely tracks only that
/// attribute's default. Concrete defaults are implemented beside each
/// `cascade_attr!` declaration.
#[derive(Resource, Clone, Debug, Reflect)]
#[reflect(Resource)]
pub struct CascadeDefault<A: CascadeProperty>(pub A);

/// Non-cascade construction defaults.
///
/// Runtime-propagated cascade defaults are stored in [`CascadeDefault<A>`] resources.
/// The fields here are read when panels are built or seeded and are not
/// propagated by the cascade plugin.
#[derive(Resource, Clone, Copy, Debug, Reflect)]
#[reflect(Resource)]
pub struct PanelDefaults {
    /// Construction-time seed for a panel's `Override<FontUnit>`. Read once by
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
