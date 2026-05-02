//! Capability: insert `bevy_diegetic::StableTransparency` on the spawned
//! orbit camera so the proper OIT + `Msaa::Off` configuration is applied
//! (mitigates view-angle shading shifts on coplanar `WorldText`).
//!
//! Gated behind the `SprinkleBuilder<WithOrbitCam>` typestate — see
//! [`crate::SprinkleBuilder::with_stable_transparency`]. Pulls in
//! `DiegeticUiPlugin` deduplicated, since `StableTransparency` is inert
//! without it.

use bevy::prelude::*;
use bevy_diegetic::DiegeticUiPlugin;
use bevy_diegetic::StableTransparency;

use crate::ensure_plugin;
use crate::orbit_cam::FairyDustOrbitCam;

pub(crate) fn install(app: &mut App) {
    ensure_plugin(app, DiegeticUiPlugin);
    app.add_observer(insert_stable_transparency);
}

fn insert_stable_transparency(trigger: On<Add, FairyDustOrbitCam>, mut commands: Commands) {
    commands.entity(trigger.entity).insert(StableTransparency);
}
