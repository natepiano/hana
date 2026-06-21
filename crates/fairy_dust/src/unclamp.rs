//! Capability: clear the example-default pitch and zoom limits on the spawned
//! orbit camera so examples can inspect geometry from steep angles and at
//! extreme zoom.
//!
//! Gated behind the `SprinkleBuilder<WithOrbitCam>` typestate — see
//! [`crate::SprinkleBuilder::unclamped`]. Runs as an `On<Add, FairyDustOrbitCam>`
//! observer, so it overrides whatever limits the camera `configure` closure set.

use bevy::prelude::*;
use bevy_lagrange::OrbitCam;

use crate::constants::UNCLAMPED_ZOOM_LOWER_LIMIT;
use crate::orbit_cam::FairyDustOrbitCam;

pub(crate) fn install(app: &mut App) { app.add_observer(unclamp_limits); }

fn unclamp_limits(trigger: On<Add, FairyDustOrbitCam>, mut cameras: Query<&mut OrbitCam>) {
    let Ok(mut camera) = cameras.get_mut(trigger.entity) else {
        return;
    };
    camera.pitch_upper_limit = None;
    camera.pitch_lower_limit = None;
    camera.zoom_upper_limit = None;
    camera.zoom_lower_limit = UNCLAMPED_ZOOM_LOWER_LIMIT;
}
