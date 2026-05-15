//! Capability: spawn an `OrbitCam` entity tagged with [`FairyDustOrbitCam`]
//! so camera-attached capabilities (e.g. stable-transparency) can find it.
//! `LagrangePlugin` itself is installed unconditionally by
//! [`crate::sprinkle_example`].

use std::sync::Mutex;

use bevy::prelude::*;
use bevy_lagrange::OrbitCam;

/// Marker on the `OrbitCam` entity spawned by
/// [`crate::SprinkleBuilder::with_orbit_cam_configured`]. Other capabilities use this to find the
/// camera (e.g. via `On<Add, FairyDustOrbitCam>` observers) rather than scanning every
/// `OrbitCam` in the world.
#[derive(Component)]
pub(crate) struct FairyDustOrbitCam;

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
        let mut cam = OrbitCam::default();
        let configure_fn = configure.lock().ok().and_then(|mut g| g.take());
        if let Some(f) = configure_fn {
            f(&mut cam);
        }
        let mut camera = commands.spawn((cam, FairyDustOrbitCam));
        if let Some(bundle) = bundle.lock().ok().and_then(|mut g| g.take()) {
            camera.insert(bundle);
        }
    });
}
