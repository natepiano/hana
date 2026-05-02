//! Capability: orbit-camera input via `bevy_lagrange::LagrangePlugin`,
//! plus a spawned `OrbitCam` entity tagged with [`FairyDustOrbitCam`] so
//! camera-attached capabilities (e.g. stable-transparency) can find it.

use std::sync::Mutex;

use bevy::prelude::*;
use bevy_lagrange::InputControl;
use bevy_lagrange::LagrangePlugin;
use bevy_lagrange::OrbitCam;
use bevy_lagrange::TrackpadBehavior;
use bevy_lagrange::TrackpadInput;

use crate::ensure_plugin;

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
    ensure_plugin(app, LagrangePlugin);
    let configure = Mutex::new(Some(configure));
    app.add_systems(Startup, move |mut commands: Commands| {
        let mut cam = OrbitCam {
            button_orbit: MouseButton::Middle,
            button_pan: MouseButton::Middle,
            modifier_pan: Some(KeyCode::ShiftLeft),
            input_control: Some(InputControl {
                trackpad: Some(TrackpadInput {
                    behavior:    TrackpadBehavior::BlenderLike {
                        modifier_pan:  Some(KeyCode::ShiftLeft),
                        modifier_zoom: Some(KeyCode::ControlLeft),
                    },
                    sensitivity: 0.5,
                }),
                ..default()
            }),
            ..default()
        };
        let configure_fn = configure.lock().ok().and_then(|mut g| g.take());
        if let Some(f) = configure_fn {
            f(&mut cam);
        }
        commands.spawn((cam, FairyDustOrbitCam));
    });
}
