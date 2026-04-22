//! [`CascadeDefaults`] resource — every global default the crate honors —
//! plus the sentinel helper the 2-tier and 3-tier propagation systems share.

use bevy::prelude::*;

use crate::layout::Unit;

/// Global defaults for every cascading attribute the crate honors.
///
/// # Design: why one resource, not several
///
/// Every global default lives here so the type doubles as a discoverability
/// manifest — a new contributor sees the full global surface of the crate
/// by reading one struct. Users setting defaults make one `insert_resource`
/// call instead of several.
///
/// The cost of consolidation: `Res<CascadeDefaults>.is_changed()` only tells
/// us "some field changed," not which. Each cascade's propagate-defaults
/// system therefore uses a `Local<Option<A>>` sentinel via
/// [`should_propagate_defaults`] that projects to the specific field that
/// cascade cares about, so mutations to unrelated fields don't wake unrelated
/// cascades.
///
/// The alternative — one resource per default — would give precise
/// `Res<T>.is_changed()` without any sentinel. At current scale (four
/// cascades, a byte of sentinel per cascade, one equality check per frame)
/// the sentinel cost is trivial. At ~15 cascades a maintainer should
/// re-evaluate.
#[derive(Resource, Clone, Copy, Debug, Reflect)]
#[reflect(Resource)]
pub struct CascadeDefaults {
    /// Global fallback for text alpha mode (both panel text and standalone
    /// world text).
    pub text_alpha:      AlphaMode,
    /// Global fallback for panel-text font unit.
    pub panel_font_unit: Unit,
    /// Global fallback for standalone-world-text font unit.
    pub world_font_unit: Unit,
    /// Default `layout_unit` for newly-built panels. Read at panel
    /// construction; **not** cascade-propagated at runtime.
    pub layout_unit:     Unit,
}

impl Default for CascadeDefaults {
    fn default() -> Self {
        Self {
            text_alpha:      AlphaMode::Blend,
            panel_font_unit: Unit::Points,
            world_font_unit: Unit::Meters,
            layout_unit:     Unit::Meters,
        }
    }
}

/// Sentinel-gated check for whether a propagate-defaults system should run.
///
/// Each propagate-defaults system holds a `Local<Option<A>>` that remembers
/// the last-seen value of its cascade's global default. This helper returns
/// `true` only when the current value differs from the sentinel, and updates
/// the sentinel in place.
///
/// Exists because [`CascadeDefaults`] bundles every global default into one
/// struct. Projecting to the specific field each cascade cares about and
/// comparing against a sentinel gives per-field precision without splitting
/// [`CascadeDefaults`] into per-field resources. See the doc comment on
/// [`CascadeDefaults`] for the full design tradeoff.
pub(super) fn should_propagate_defaults<A: Copy + PartialEq>(
    current: A,
    last_seen: &mut Option<A>,
) -> bool {
    if *last_seen == Some(current) {
        return false;
    }
    *last_seen = Some(current);
    true
}
