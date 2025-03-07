mod action;
mod basic;
mod basic_viz;
mod camera;
mod error;
mod error_handling;
mod oscillating_gizmo;
mod prelude;
mod splash;
mod tokio_runtime;

use bevy::prelude::*;
use hana_async::AsyncRuntimePlugin;
use hana_viz::HanaVizPlugin;

use crate::action::ActionPlugin;
use crate::basic::BasicPlugin;
use crate::basic_viz::BasicVizPlugin;
use crate::camera::CameraPlugin;
use crate::error_handling::ErrorHandlingPlugin;
use crate::oscillating_gizmo::OscillatingGizmoPlugin;
use crate::splash::SplashPlugin;
use crate::tokio_runtime::TokioRuntimePlugin;

fn main() {
    trace!("Starting Hana visualization management system");

    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins((
            ActionPlugin,
            AsyncRuntimePlugin,
            BasicPlugin,
            BasicVizPlugin,
            CameraPlugin,
            ErrorHandlingPlugin,
            HanaVizPlugin,
            OscillatingGizmoPlugin,
            SplashPlugin,
            TokioRuntimePlugin,
        ))
        .run();
}
