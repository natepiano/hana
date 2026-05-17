//! `SprinkleBuilder` impls — state-agnostic, `NoOrbitCam`, and `WithOrbitCam`.

use std::marker::PhantomData;

use bevy::app::Plugins;
use bevy::ecs::schedule::IntoScheduleConfigs;
use bevy::ecs::schedule::ScheduleLabel;
use bevy::ecs::system::ScheduleSystem;
use bevy::prelude::*;
use bevy_lagrange::OrbitCam;

use super::CameraHomeBuilder;
use super::NoOrbitCam;
use super::PrimitiveBuilder;
use super::SprinkleBuilder;
use super::TitleBarBuilder;
use super::WithOrbitCam;
use crate::brp_extras;
use crate::camera_control_panel;
use crate::camera_home::CameraHomeConfig;
use crate::lighting;
use crate::orbit_cam;
use crate::primitive::PrimitiveConfig;
use crate::restart;
use crate::save_window_position;
use crate::screen_panels;
use crate::screen_panels::DescriptionPanel;
use crate::screen_panels::TitleBar;
use crate::transparency;

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

    /// Overrides the inner background color of the camera control panel.
    /// Pair with [`with_camera_control_panel`](Self::with_camera_control_panel).
    /// Use [`DEFAULT_PANEL_BACKGROUND`](crate::DEFAULT_PANEL_BACKGROUND) and
    /// [`Color::with_alpha`] to tweak only the opacity:
    /// `.with_camera_control_panel_background_color(DEFAULT_PANEL_BACKGROUND.with_alpha(0.85))`.
    #[must_use]
    pub fn with_camera_control_panel_background_color(mut self, color: Color) -> Self {
        self.app
            .insert_resource(camera_control_panel::CameraControlPanelBackground(color));
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
            parent:  self,
            config:  PrimitiveConfig::ground_plane(),
            inserts: Vec::new(),
        }
    }

    /// Starts configuring a reusable cube for the example scene.
    #[must_use]
    pub const fn with_cube(self) -> PrimitiveBuilder<S> {
        PrimitiveBuilder {
            parent:  self,
            config:  PrimitiveConfig::cube(),
            inserts: Vec::new(),
        }
    }

    /// Spawn a static side panel that describes the example.
    #[must_use]
    pub fn with_description_panel(mut self, panel: DescriptionPanel) -> Self {
        screen_panels::install_description(&mut self.app, panel);
        self
    }

    /// Spawn a compact top-left title bar for example controls and switch to
    /// a [`TitleBarBuilder`] so chip highlights can be wired to event
    /// lifecycles.
    #[must_use]
    pub fn with_title_bar(mut self, title_bar: TitleBar) -> TitleBarBuilder<S> {
        screen_panels::install_title_bar(&mut self.app, title_bar);
        TitleBarBuilder { parent: self }
    }

    /// Begin configuring a generalized camera "home" pose.
    ///
    /// Spawns an invisible cube at the given [`Transform`] (its `scale` defines
    /// the framed volume) and wires `H` to an [`bevy_lagrange::AnimateToFit`]
    /// of that region using the configured `yaw`/`pitch`. If a title bar is
    /// installed, the `H Home` chip is prepended automatically and highlights
    /// for the duration of the home animation.
    #[must_use]
    pub const fn with_camera_home(self, transform: Transform) -> CameraHomeBuilder<S> {
        CameraHomeBuilder {
            parent: self,
            config: CameraHomeConfig {
                transform,
                yaw: 0.0,
                pitch: 0.0,
                duration: crate::constants::HOME_DEFAULT_DURATION,
                margin: crate::constants::HOME_DEFAULT_MARGIN,
            },
        }
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

    /// Mirror of [`App::add_observer`].
    #[must_use]
    pub fn add_observer<E, B, M, I>(mut self, observer: I) -> Self
    where
        E: bevy::ecs::event::Event,
        B: Bundle,
        I: bevy::ecs::system::IntoObserverSystem<E, B, M>,
    {
        self.app.add_observer(observer);
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

    /// Run the configured app. Mirror of [`App::run`], with the exception
    /// that a `Ctrl+Shift+R` press handled via `with_restart_key`
    /// will re-exec the current binary before this method returns.
    pub fn run(mut self) -> AppExit {
        let exit = self.app.run();
        restart::perform_restart_if_requested();
        exit
    }

    /// Escape hatch: borrow the underlying [`App`] for capabilities not yet
    /// surfaced as `with_*` methods.
    pub const fn app_mut(&mut self) -> &mut App { &mut self.app }
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
    pub fn with_orbit_cam<F, B>(mut self, configure: F, bundle: B) -> SprinkleBuilder<WithOrbitCam>
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
