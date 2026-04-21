//! Public [`CascadeSet`] — the system set every propagation system lives in.

use bevy::prelude::*;

/// Public system-set handle for `bevy_diegetic` cascade propagation.
///
/// Users schedule `.after(CascadeSet::Propagate)` to guarantee they observe
/// propagated values within the same frame.
#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CascadeSet {
    /// All systems that propagate cascade changes into per-entity
    /// [`Resolved`](crate::cascade::Resolved) values. Covers both tier-2
    /// propagation (panel override mutations, detected via
    /// `Changed<A::PanelOverride>`) and tier-3 propagation (global-default
    /// mutations, detected via [`CascadeDefaults`](super::CascadeDefaults)).
    /// After this set runs in [`Update`], every `Resolved` on affected
    /// entities reflects current sources.
    Propagate,
}
