//! Workspace example helper for `bevy_hana`.
//!
//!
//! Use [`sprinkle_example`] to construct a [`SprinkleBuilder`] preloaded with
//! `DefaultPlugins` configured for a quiet log filter, then chain capability
//! methods to opt into specific dev conveniences:
//!
//! ```ignore
//! fairy_dust::sprinkle_example()
//!     .with_orbit_cam_configured(|cam| { cam.radius = Some(5.0); })
//!     .with_stable_transparency()        // only callable after with_orbit_cam_*
//!     .with_save_window_position()
//!     .with_brp_extras()
//!     .with_camera_control_panel()
//!     .add_systems(Startup, setup)
//!     .run();
//! ```
//!
//! ## Typestate
//!
//! The builder is parameterized by a state marker (`NoOrbitCam` / `WithOrbitCam`).
//! Methods that act on the spawned `OrbitCam` entity (currently
//! [`SprinkleBuilder::with_stable_transparency`]) are only defined on
//! `SprinkleBuilder<WithOrbitCam>`, so calling them before
//! [`SprinkleBuilder::with_orbit_cam_configured`] is a compile error.
//!
//! ## Plugin deduplication
//!
//! Capabilities that share infrastructure (for example a `DiegeticUiPlugin` for
//! HUD panels) ensure the required plugin is registered exactly once via
//! [`ensure_plugin`], regardless of how many capabilities pull it in.

mod brp_extras;
mod builder;
mod camera_control_panel;
mod camera_home;
mod constants;
mod lighting;
mod orbit_cam;
mod primitive;
mod restart;
mod save_window_position;
mod screen_panels;
mod transparency;

use std::marker::PhantomData;

use bevy::log::LogPlugin;
use bevy::prelude::*;
pub use bevy_diegetic::Anchor;
use bevy_diegetic::DiegeticUiPlugin;
pub use bevy_lagrange::OrbitCam;
pub use builder::CameraHomeBuilder;
pub use builder::NoOrbitCam;
pub use builder::PrimitiveBuilder;
pub use builder::SprinkleBuilder;
pub use builder::TitleBarBuilder;
pub use builder::WithOrbitCam;
pub use camera_control_panel::CameraGuidance;
pub use camera_control_panel::CameraGuidanceRow;
pub use camera_control_panel::SourceVisibility;
pub use constants::LOG_FILTER;
pub use primitive::Face;
pub use primitive::cube_face_text;
pub use screen_panels::DescriptionPanel;
pub use screen_panels::TitleBar;

/// Construct a fresh [`SprinkleBuilder`] with `DefaultPlugins` configured
/// for a quiet log filter. Chain capability methods, then call `.run()`.
///
/// [`bevy_diegetic::DiegeticUiPlugin`] is registered unconditionally so any
/// example can spawn `WorldText` or `DiegeticPanel` without an explicit
/// `add_plugins` call.
///
/// The Ctrl+Shift+R hot-restart shortcut is wired up unconditionally — when
/// pressed, the example process re-execs itself via a trampoline so source
/// changes picked up by a parallel `cargo build` take effect immediately.
/// If this process was spawned as the trampoline, this function never
/// returns — it sleeps so the parent is fully reaped, then `exec`s the
/// same binary without the trampoline marker.
#[must_use]
pub fn sprinkle_example() -> SprinkleBuilder<NoOrbitCam> {
    restart::handle_trampoline_if_active();
    let mut app = App::new();
    app.add_plugins(DefaultPlugins.set(LogPlugin {
        filter: LOG_FILTER.to_string(),
        ..LogPlugin::default()
    }));
    ensure_plugin(&mut app, DiegeticUiPlugin);
    restart::install(&mut app);
    SprinkleBuilder {
        app,
        _state: PhantomData,
    }
}

/// Add `plugin` to `app` if no plugin of the same type is already registered.
///
/// Bevy panics on duplicate plugin registration, so capabilities that share
/// infrastructure (for instance multiple HUD capabilities both needing
/// `DiegeticUiPlugin`) route their plugin adds through this helper.
pub(crate) fn ensure_plugin<P: Plugin>(app: &mut App, plugin: P) {
    if !app.is_plugin_added::<P>() {
        app.add_plugins(plugin);
    }
}
