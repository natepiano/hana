//! Workspace example helper for `bevy_hana`.
//!
//! Use [`sprinkle_example`] to construct an [`App`] preloaded with `DefaultPlugins`
//! configured for a quiet log filter, then chain [`FairyDustExt`] capability methods
//! to opt into specific dev conveniences:
//!
//! ```ignore
//! fairy_dust::sprinkle_example()
//!     .with_orbit_cam()
//!     .with_save_window_position()
//!     .with_brp_extras()
//!     .with_camera_control_panel()
//!     .add_systems(Startup, setup)
//!     .run();
//! ```
//!
//! Each capability is opt-in. Capabilities that need shared infrastructure
//! (for example a `DiegeticUiPlugin` for HUD panels) ensure the required
//! plugin is registered exactly once, regardless of how many capabilities
//! pull it in.

use bevy::log::LogPlugin;
use bevy::prelude::*;

mod brp_extras;
mod camera_control_panel;
mod orbit_cam;
mod save_window_position;

/// Default `tracing` filter applied by [`sprinkle_example`].
///
/// Quiets the most common chatty crates (`wgpu`, `naga`) while leaving the
/// rest at `info` so example-side `info!`/`warn!` calls remain visible.
pub const LOG_FILTER: &str = "info,wgpu=error,naga=error,bevy_winit=warn,bevy_render=warn";

/// Construct a fresh [`App`] with `DefaultPlugins` configured for a quiet
/// log filter.
///
/// Nothing else is added. Chain [`FairyDustExt`] methods to opt into specific
/// capabilities, then call `.run()` to start the app.
pub fn sprinkle_example() -> App {
    let mut app = App::new();
    app.add_plugins(DefaultPlugins.set(LogPlugin {
        filter: LOG_FILTER.to_string(),
        ..LogPlugin::default()
    }));
    app
}

/// Chainable builder methods on [`App`] that opt into individual `fairy_dust`
/// capabilities.
///
/// Each method:
/// - adds whatever plugins the capability requires (deduplicated via [`App::is_plugin_added`], so
///   adding the same plugin from multiple capabilities — or alongside an explicit `add_plugins`
///   call in the example — is safe);
/// - returns `&mut Self` so calls compose with native [`App`] methods.
pub trait FairyDustExt {
    /// Add a `bevy_lagrange` `LagrangePlugin` and configure orbit-camera input.
    fn with_orbit_cam(&mut self) -> &mut Self;

    /// Add a `bevy_window_manager` `WindowManagerPlugin` so window position
    /// and size are persisted across runs.
    fn with_save_window_position(&mut self) -> &mut Self;

    /// Add a `bevy_brp_extras` `BrpExtrasPlugin` configured to display the
    /// BRP port in the window title when the port is non-default.
    fn with_brp_extras(&mut self) -> &mut Self;

    /// Spawn a screen-space panel anchored bottom-right that documents
    /// `bevy_lagrange::OrbitCam` mouse and trackpad controls.
    ///
    /// Pulls in `DiegeticUiPlugin` and `MeshPickingPlugin` if not already
    /// present. Pair with [`FairyDustExt::with_orbit_cam`] for the
    /// described controls to actually do anything.
    fn with_camera_control_panel(&mut self) -> &mut Self;
}

impl FairyDustExt for App {
    fn with_orbit_cam(&mut self) -> &mut Self {
        orbit_cam::install(self);
        self
    }

    fn with_save_window_position(&mut self) -> &mut Self {
        save_window_position::install(self);
        self
    }

    fn with_brp_extras(&mut self) -> &mut Self {
        brp_extras::install(self);
        self
    }

    fn with_camera_control_panel(&mut self) -> &mut Self {
        camera_control_panel::install(self);
        self
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
