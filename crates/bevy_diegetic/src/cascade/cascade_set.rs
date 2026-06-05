//! Public [`CascadeSet`] — the system set every propagation system belongs to.

use bevy::prelude::*;

/// Public system-set handle for `bevy_diegetic` cascade propagation.
///
/// Users schedule `.after(CascadeSet::Propagate)` to guarantee they observe
/// propagated values within the same frame.
#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CascadeSet {
    /// All systems that keep per-entity
    /// `Resolved` values current. One propagation
    /// system per cascade attribute re-resolves a node when its own
    /// `Override<A>` changes or is removed, its `ChildOf` changes, or
    /// `CascadeDefault<A>` changes — fanning ancestor
    /// changes down through `Children`. After this set runs in [`Update`],
    /// every `Resolved` on an affected entity reflects current sources.
    Propagate,
}
