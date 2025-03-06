mod action;
mod basic;
mod camera;
mod error;
mod oscillating_gizmo;
mod prelude;
mod splash;
mod tokio_runtime;

use bevy::prelude::*;

use crate::action::ActionPlugin;
use crate::basic::BasicPlugin;
use crate::camera::CameraPlugin;
use crate::oscillating_gizmo::OscillatingGizmoPlugin;
use crate::splash::SplashPlugin;
use crate::tokio_runtime::TokioRuntimePlugin;

fn main() {
    trace!("Starting Hana visualization management system");

    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins((
            ActionPlugin,
            BasicPlugin,
            CameraPlugin,
            OscillatingGizmoPlugin,
            SplashPlugin,
            TokioRuntimePlugin,
        ))
        .run();
}
