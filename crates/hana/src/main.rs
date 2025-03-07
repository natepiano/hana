mod action;
mod basic_viz;
mod camera;
mod error;
mod error_handling;
mod oscillating_gizmo;
mod splash;

use bevy::prelude::*;
use hana_async::AsyncRuntimePlugin;
use hana_viz::HanaVizPlugin;

use crate::action::ActionPlugin;
use crate::basic_viz::BasicVizPlugin;
use crate::camera::CameraPlugin;
use crate::error_handling::ErrorHandlingPlugin;
use crate::oscillating_gizmo::OscillatingGizmoPlugin;
use crate::splash::SplashPlugin;

fn main() {
    trace!("Starting Hana visualization management system");

    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins((
            ActionPlugin,
            AsyncRuntimePlugin,
            BasicVizPlugin,
            CameraPlugin,
            ErrorHandlingPlugin,
            HanaVizPlugin,
            OscillatingGizmoPlugin,
            SplashPlugin,
        ))
        .run();
}
