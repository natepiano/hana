//! Workspace example helper for `bevy_hana`.
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

use std::marker::PhantomData;

use bevy::app::Plugins;
use bevy::ecs::schedule::IntoScheduleConfigs;
use bevy::ecs::schedule::ScheduleLabel;
use bevy::ecs::system::ScheduleSystem;
use bevy::log::LogPlugin;
use bevy::prelude::*;
pub use bevy_lagrange::OrbitCam;

mod brp_extras;
mod camera_control_panel;
mod orbit_cam;
mod save_window_position;
mod transparency;

/// Default `tracing` filter applied by [`sprinkle_example`].
///
/// Quiets the most common chatty crates (`wgpu`, `naga`) while leaving the
/// rest at `info` so example-side `info!`/`warn!` calls remain visible.
pub const LOG_FILTER: &str = "info,wgpu=error,naga=error,bevy_winit=warn,bevy_render=warn";

/// Typestate marker: the builder has not yet spawned an `OrbitCam`.
///
/// Camera-attached capabilities are not defined for `SprinkleBuilder<NoOrbitCam>`,
/// so calling them is a compile error.
pub struct NoOrbitCam;

/// Typestate marker: the builder has spawned an `OrbitCam`.
///
/// Reached via [`SprinkleBuilder::with_orbit_cam_configured`]. Camera-attached
/// capabilities like [`SprinkleBuilder::with_stable_transparency`] become
/// callable in this state.
pub struct WithOrbitCam;

/// Builder returned by [`sprinkle_example`]. State-agnostic capability
/// methods are defined for any `S`; camera-attached methods are gated by
/// the typestate.
pub struct SprinkleBuilder<S> {
    app:    App,
    _state: PhantomData<S>,
}

/// Construct a fresh [`SprinkleBuilder`] with `DefaultPlugins` configured
/// for a quiet log filter. Chain capability methods, then call `.run()`.
#[must_use]
pub fn sprinkle_example() -> SprinkleBuilder<NoOrbitCam> {
    let mut app = App::new();
    app.add_plugins(DefaultPlugins.set(LogPlugin {
        filter: LOG_FILTER.to_string(),
        ..LogPlugin::default()
    }));
    SprinkleBuilder {
        app,
        _state: PhantomData,
    }
}

// State-agnostic capabilities — available regardless of whether an `OrbitCam`
// has been configured.
impl<S> SprinkleBuilder<S> {
    /// Add a `bevy_window_manager` `WindowManagerPlugin` so window position
    /// and size are persisted across runs.
    #[must_use]
    pub fn with_save_window_position(mut self) -> Self {
        save_window_position::install(&mut self.app);
        self
    }

    /// Add a `bevy_brp_extras` `BrpExtrasPlugin` configured to display the
    /// BRP port in the window title when the port is non-default.
    #[must_use]
    pub fn with_brp_extras(mut self) -> Self {
        brp_extras::install(&mut self.app);
        self
    }

    /// Spawn a screen-space panel anchored bottom-right that documents
    /// `bevy_lagrange::OrbitCam` mouse and trackpad controls.
    ///
    /// Pulls in `DiegeticUiPlugin` and `MeshPickingPlugin` if not already
    /// present.
    #[must_use]
    pub fn with_camera_control_panel(mut self) -> Self {
        camera_control_panel::install(&mut self.app);
        self
    }

    /// Mirror of [`App::add_plugins`].
    #[must_use]
    pub fn add_plugins<M>(mut self, plugins: impl Plugins<M>) -> Self {
        self.app.add_plugins(plugins);
        self
    }

    /// Mirror of [`App::add_systems`].
    #[must_use]
    pub fn add_systems<M>(
        mut self,
        schedule: impl ScheduleLabel,
        systems: impl IntoScheduleConfigs<ScheduleSystem, M>,
    ) -> Self {
        self.app.add_systems(schedule, systems);
        self
    }

    /// Mirror of [`App::init_resource`].
    #[must_use]
    pub fn init_resource<R: Resource + FromWorld>(mut self) -> Self {
        self.app.init_resource::<R>();
        self
    }

    /// Run the configured app. Mirror of [`App::run`].
    pub fn run(mut self) -> AppExit { self.app.run() }

    /// Escape hatch: borrow the underlying [`App`] for capabilities not yet
    /// surfaced as `with_*` methods.
    pub const fn app_mut(&mut self) -> &mut App { &mut self.app }
}

// State transition: `NoOrbitCam` → `WithOrbitCam`.
impl SprinkleBuilder<NoOrbitCam> {
    /// Add `bevy_lagrange::LagrangePlugin` and spawn an `OrbitCam` entity.
    /// Defaults to MMB orbit, Shift+MMB pan, scroll zoom, and Blender-like
    /// trackpad input (Shift+scroll pan, Ctrl+scroll zoom, pinch zoom). The
    /// caller's `configure` closure runs after defaults so it can set
    /// `focus`, `radius`, `yaw`, `pitch`, or override the default buttons.
    pub fn with_orbit_cam_configured<F>(mut self, configure: F) -> SprinkleBuilder<WithOrbitCam>
    where
        F: FnOnce(&mut OrbitCam) + Send + Sync + 'static,
    {
        orbit_cam::install_with(&mut self.app, configure);
        SprinkleBuilder {
            app:    self.app,
            _state: PhantomData,
        }
    }
}

// Camera-attached capabilities — only valid after an `OrbitCam` has been
// configured.
impl SprinkleBuilder<WithOrbitCam> {
    /// Insert `bevy_diegetic::StableTransparency` on the spawned `OrbitCam`,
    /// which adds `OrderIndependentTransparencySettings`, sets the camera's
    /// depth texture to `TEXTURE_BINDING`, and forces `Msaa::Off` on the
    /// camera and on every screen-space overlay camera in the app.
    ///
    /// Use this when coplanar `WorldText` shows view-angle shading shifts,
    /// when you need animated alpha fades on text, or when you need correct
    /// depth compositing of text with other translucent primitives. Pair
    /// with `AlphaMode::Blend` on text. Inert without `DiegeticUiPlugin`,
    /// which is added deduplicated.
    #[must_use]
    pub fn with_stable_transparency(mut self) -> Self {
        transparency::install(&mut self.app);
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
