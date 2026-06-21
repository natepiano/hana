//! Capability: spawn an `OrbitCam` entity tagged with [`FairyDustOrbitCam`]
//! so camera-attached capabilities (e.g. stable-transparency) can find it.
//! `LagrangePlugin` itself is installed unconditionally by
//! [`crate::sprinkle_example`].

use std::sync::Mutex;

use bevy::prelude::*;
use bevy_lagrange::OrbitCam;
use bevy_lagrange::UpsideDownPolicy;

use crate::constants::EXAMPLE_ORBIT_CAM_PITCH_LIMIT;
use crate::constants::EXAMPLE_ORBIT_CAM_ZOOM_LOWER_LIMIT;
use crate::constants::EXAMPLE_ORBIT_CAM_ZOOM_UPPER_LIMIT;

/// Marker identifying the `OrbitCam` that `fairy_dust` capabilities target.
///
/// [`crate::SprinkleBuilder::with_orbit_cam_configured`],
/// [`crate::SprinkleBuilder::with_orbit_cam`],
/// [`crate::SprinkleBuilder::with_orbit_cam_preset`],
/// [`crate::SprinkleBuilder::with_orbit_cam_bindings`], and
/// [`crate::SprinkleBuilder::with_orbit_cam_manual`] add it for you. An example
/// that spawns its own `OrbitCam` (for instance to set a swapped `axis` inline)
/// attaches it by hand, so [`crate::SprinkleBuilder::with_camera_home`],
/// [`crate::SprinkleBuilder::with_stable_transparency`], and
/// [`crate::SprinkleBuilder::with_restore_camera_on_restart`] still find the
/// camera. Capabilities locate it via `On<Add, FairyDustOrbitCam>` observers or
/// `With<FairyDustOrbitCam>` queries rather than scanning every `OrbitCam` in
/// the world.
///
/// [`crate::SprinkleBuilder::with_camera_control_panel`] does not need this
/// marker — it follows the routed `OrbitCam` directly.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct FairyDustOrbitCam;

/// Applies the canonical example camera limits used by manual `OrbitCam` spawns.
pub fn apply_example_orbit_cam_limits(camera: &mut OrbitCam) {
    camera.pitch_upper_limit = Some(EXAMPLE_ORBIT_CAM_PITCH_LIMIT);
    camera.pitch_lower_limit = Some(-EXAMPLE_ORBIT_CAM_PITCH_LIMIT);
    camera.zoom_upper_limit = Some(EXAMPLE_ORBIT_CAM_ZOOM_UPPER_LIMIT);
    camera.zoom_lower_limit = EXAMPLE_ORBIT_CAM_ZOOM_LOWER_LIMIT;
    camera.upside_down_policy = UpsideDownPolicy::Allow;
}

pub(crate) fn install_with(
    app: &mut App,
    configure: impl FnOnce(&mut OrbitCam) + Send + Sync + 'static,
) {
    install_with_bundle(app, configure, ());
}

pub(crate) fn install_with_bundle<B>(
    app: &mut App,
    configure: impl FnOnce(&mut OrbitCam) + Send + Sync + 'static,
    bundle: B,
) where
    B: Bundle + Send + Sync + 'static,
{
    let configure = Mutex::new(Some(configure));
    let bundle = Mutex::new(Some(bundle));
    app.add_systems(Startup, move |mut commands: Commands| {
        let mut orbit_cam = OrbitCam::default();
        apply_example_orbit_cam_limits(&mut orbit_cam);
        let configure_fn = configure.lock().ok().and_then(|mut g| g.take());
        if let Some(f) = configure_fn {
            f(&mut orbit_cam);
        }
        let mut camera = commands.spawn((orbit_cam, FairyDustOrbitCam));
        if let Some(bundle) = bundle.lock().ok().and_then(|mut g| g.take()) {
            camera.insert(bundle);
        }
    });
}
