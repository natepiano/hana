//! Capability: orbit-camera input via `bevy_lagrange::LagrangePlugin`,
//! plus a spawned `OrbitCam` entity with OIT + MSAA-off (the combination
//! every `bevy_hana` example uses).

use std::sync::Mutex;

use bevy::core_pipeline::oit::OrderIndependentTransparencySettings;
use bevy::prelude::*;
use bevy::render::view::Msaa;
use bevy_lagrange::InputControl;
use bevy_lagrange::LagrangePlugin;
use bevy_lagrange::OrbitCam;
use bevy_lagrange::TrackpadBehavior;
use bevy_lagrange::TrackpadInput;

use crate::ensure_plugin;

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
        commands.spawn((
            cam,
            OrderIndependentTransparencySettings::default(),
            Msaa::Off,
        ));
    });
}
