//! Capability: clear the example-default pitch and zoom limits on the spawned
//! orbit camera so examples can inspect geometry from steep angles and at
//! extreme zoom.
//!
//! Gated behind the `SprinkleBuilder<WithOrbitCam>` typestate — see
//! [`crate::SprinkleBuilder::unclamped`]. Runs as an `On<Add, FairyDustOrbitCam>`
//! observer, so it overrides whatever limits the camera `configure` closure set.

use bevy::prelude::*;
use bevy_lagrange::AnglePairLimit;
use bevy_lagrange::OrbitCam;
use bevy_lagrange::ScalarLimit;

use crate::constants::UNCLAMPED_ZOOM_LOWER_LIMIT;
use crate::orbit_cam::FairyDustOrbitCam;

pub(crate) fn install(app: &mut App) { app.add_observer(unclamp_limits); }

fn unclamp_limits(trigger: On<Add, FairyDustOrbitCam>, mut cameras: Query<&mut OrbitCam>) {
    let Ok(mut camera) = cameras.get_mut(trigger.entity) else {
        return;
    };
    // Clear the per-axis angle limits and drop the zoom ceiling, keeping only a
    // small positive floor so the camera can't pass through the focus.
    *camera.orbit.limit_mut() = AnglePairLimit::default();
    *camera.zoom.limit_mut() = ScalarLimit::Clamp {
        min: UNCLAMPED_ZOOM_LOWER_LIMIT,
        max: f32::INFINITY,
    };
}
