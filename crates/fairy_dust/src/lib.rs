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

use std::marker::PhantomData;

use bevy::app::Plugins;
use bevy::ecs::schedule::IntoScheduleConfigs;
use bevy::ecs::schedule::ScheduleLabel;
use bevy::ecs::system::ScheduleSystem;
use bevy::log::LogPlugin;
use bevy::prelude::*;
pub use bevy_diegetic::Anchor;
pub use bevy_lagrange::OrbitCam;
pub use camera_control_panel::CameraGuidance;
pub use camera_control_panel::CameraGuidanceRow;
use primitive::PrimitiveConfig;
pub use screen_panels::DescriptionPanel;
pub use screen_panels::TitleBar;
pub use screen_panels::TitleBarControlState;

mod brp_extras;
mod camera_control_panel;
mod lighting;
mod orbit_cam;
mod primitive;
mod save_window_position;
mod screen_panels;
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

/// Builder returned while configuring a simple scene primitive.
///
/// Calling a non-primitive builder method finalizes the primitive and returns
/// to the normal [`SprinkleBuilder`] chain.
pub struct PrimitiveBuilder<S> {
    parent: SprinkleBuilder<S>,
    config: PrimitiveConfig,
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

    /// Enable smart screen-space camera control panels for `OrbitCam` cameras.
    ///
    /// Cameras without an explicit [`CameraGuidance`] component get
    /// [`CameraGuidance::auto()`], so the panel reflects the effective preset
    /// or binding configuration and highlights active interactions.
    #[must_use]
    pub fn with_camera_control_panel(mut self) -> Self {
        camera_control_panel::install(&mut self.app);
        self
    }

    /// Add a reusable key/fill/rim lighting setup for simple example scenes.
    #[must_use]
    pub fn with_studio_lighting(mut self) -> Self {
        lighting::install(&mut self.app);
        self
    }

    /// Starts configuring a reusable ground plane for the example scene.
    #[must_use]
    pub const fn with_ground_plane(self) -> PrimitiveBuilder<S> {
        PrimitiveBuilder {
            parent: self,
            config: PrimitiveConfig::ground_plane(),
        }
    }

    /// Starts configuring a reusable cube for the example scene.
    #[must_use]
    pub const fn with_cube(self) -> PrimitiveBuilder<S> {
        PrimitiveBuilder {
            parent: self,
            config: PrimitiveConfig::cube(),
        }
    }

    /// Spawn a static side panel that describes the example.
    #[must_use]
    pub fn with_description_panel(mut self, panel: DescriptionPanel) -> Self {
        screen_panels::install_description(&mut self.app, panel);
        self
    }

    /// Spawn a compact top-left title bar for example controls.
    #[must_use]
    pub fn with_title_bar(mut self, title_bar: TitleBar) -> Self {
        screen_panels::install_title_bar(&mut self.app, title_bar);
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

    /// Mirror of [`App::insert_resource`].
    #[must_use]
    pub fn insert_resource<R: Resource>(mut self, resource: R) -> Self {
        self.app.insert_resource(resource);
        self
    }

    /// Run the configured app. Mirror of [`App::run`].
    pub fn run(mut self) -> AppExit { self.app.run() }

    /// Escape hatch: borrow the underlying [`App`] for capabilities not yet
    /// surfaced as `with_*` methods.
    pub const fn app_mut(&mut self) -> &mut App { &mut self.app }
}

impl<S> PrimitiveBuilder<S> {
    /// Sets the primitive size.
    ///
    /// For a ground plane this is the square edge length. For a cube this is
    /// the cube edge length.
    #[must_use]
    pub const fn size(mut self, size: f32) -> Self {
        self.config.set_size(size);
        self
    }

    /// Sets the primitive material base color.
    #[must_use]
    pub const fn color(mut self, color: Color) -> Self {
        self.config.set_color(color);
        self
    }

    /// Sets the full primitive material.
    ///
    /// This overrides any color previously configured with [`Self::color`].
    #[must_use]
    pub fn material(mut self, material: StandardMaterial) -> Self {
        self.config = self.config.with_material(material);
        self
    }

    /// Sets the primitive transform.
    #[must_use]
    pub const fn transform(mut self, transform: Transform) -> Self {
        self.config.set_transform(transform);
        self
    }

    /// Finalizes the current primitive and starts configuring a ground plane.
    #[must_use]
    pub fn with_ground_plane(self) -> Self { self.finish().with_ground_plane() }

    /// Finalizes the current primitive and starts configuring a cube.
    #[must_use]
    pub fn with_cube(self) -> Self { self.finish().with_cube() }

    /// Finalizes the current primitive and adds window position persistence.
    #[must_use]
    pub fn with_save_window_position(self) -> SprinkleBuilder<S> {
        self.finish().with_save_window_position()
    }

    /// Finalizes the current primitive and adds BRP extras.
    #[must_use]
    pub fn with_brp_extras(self) -> SprinkleBuilder<S> { self.finish().with_brp_extras() }

    /// Finalizes the current primitive and adds the smart camera control panel.
    #[must_use]
    pub fn with_camera_control_panel(self) -> SprinkleBuilder<S> {
        self.finish().with_camera_control_panel()
    }

    /// Finalizes the current primitive and adds studio lighting.
    #[must_use]
    pub fn with_studio_lighting(self) -> SprinkleBuilder<S> { self.finish().with_studio_lighting() }

    /// Finalizes the current primitive and adds an example description panel.
    #[must_use]
    pub fn with_description_panel(self, panel: DescriptionPanel) -> SprinkleBuilder<S> {
        self.finish().with_description_panel(panel)
    }

    /// Finalizes the current primitive and adds an example title bar.
    #[must_use]
    pub fn with_title_bar(self, title_bar: TitleBar) -> SprinkleBuilder<S> {
        self.finish().with_title_bar(title_bar)
    }

    /// Finalizes the current primitive and mirrors [`App::add_plugins`].
    #[must_use]
    pub fn add_plugins<M>(self, plugins: impl Plugins<M>) -> SprinkleBuilder<S> {
        self.finish().add_plugins(plugins)
    }

    /// Finalizes the current primitive and mirrors [`App::add_systems`].
    #[must_use]
    pub fn add_systems<M>(
        self,
        schedule: impl ScheduleLabel,
        systems: impl IntoScheduleConfigs<ScheduleSystem, M>,
    ) -> SprinkleBuilder<S> {
        self.finish().add_systems(schedule, systems)
    }

    /// Finalizes the current primitive and mirrors [`App::init_resource`].
    #[must_use]
    pub fn init_resource<R: Resource + FromWorld>(self) -> SprinkleBuilder<S> {
        self.finish().init_resource::<R>()
    }

    /// Finalizes the current primitive and mirrors [`App::insert_resource`].
    #[must_use]
    pub fn insert_resource<R: Resource>(self, resource: R) -> SprinkleBuilder<S> {
        self.finish().insert_resource(resource)
    }

    /// Finalizes the current primitive and runs the configured app.
    pub fn run(self) -> AppExit { self.finish().run() }

    fn finish(mut self) -> SprinkleBuilder<S> {
        primitive::install(&mut self.parent.app, self.config);
        self.parent
    }
}

// State transition: `NoOrbitCam` → `WithOrbitCam`.
impl SprinkleBuilder<NoOrbitCam> {
    /// Add `bevy_lagrange::LagrangePlugin` and spawn an `OrbitCam` entity.
    /// The caller's `configure` closure can set `focus`, `radius`, `yaw`,
    /// `pitch`, sensitivity, limits, or other camera behavior fields. Input
    /// uses `OrbitCamPreset::SimpleMouse` unless another input-mode component
    /// is inserted.
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

    /// Add `bevy_lagrange::LagrangePlugin`, spawn an `OrbitCam` entity, and
    /// insert extra camera-side components such as `OrbitCamPreset`,
    /// `OrbitCamBindings`, `OrbitCamManual`, or [`CameraGuidance`].
    pub fn with_orbit_cam_bundle<F, B>(
        mut self,
        configure: F,
        bundle: B,
    ) -> SprinkleBuilder<WithOrbitCam>
    where
        F: FnOnce(&mut OrbitCam) + Send + Sync + 'static,
        B: Bundle + Send + Sync + 'static,
    {
        orbit_cam::install_with_bundle(&mut self.app, configure, bundle);
        SprinkleBuilder {
            app:    self.app,
            _state: PhantomData,
        }
    }
}

impl PrimitiveBuilder<NoOrbitCam> {
    /// Finalizes the current primitive, adds `LagrangePlugin`, and spawns an
    /// `OrbitCam` entity.
    pub fn with_orbit_cam_configured<F>(self, configure: F) -> SprinkleBuilder<WithOrbitCam>
    where
        F: FnOnce(&mut OrbitCam) + Send + Sync + 'static,
    {
        self.finish().with_orbit_cam_configured(configure)
    }

    /// Finalizes the current primitive, adds `LagrangePlugin`, spawns an
    /// `OrbitCam`, and inserts extra camera-side components.
    pub fn with_orbit_cam_bundle<F, B>(
        self,
        configure: F,
        bundle: B,
    ) -> SprinkleBuilder<WithOrbitCam>
    where
        F: FnOnce(&mut OrbitCam) + Send + Sync + 'static,
        B: Bundle + Send + Sync + 'static,
    {
        self.finish().with_orbit_cam_bundle(configure, bundle)
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

impl PrimitiveBuilder<WithOrbitCam> {
    /// Finalizes the current primitive and adds stable transparency to the
    /// spawned `OrbitCam`.
    #[must_use]
    pub fn with_stable_transparency(self) -> SprinkleBuilder<WithOrbitCam> {
        self.finish().with_stable_transparency()
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
