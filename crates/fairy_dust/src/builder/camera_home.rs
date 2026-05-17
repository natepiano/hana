//! `CameraHomeBuilder` impls.

use std::time::Duration;

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
use super::StudioLightingBuilder;
use super::TitleBarBuilder;
use super::WithOrbitCam;
use crate::camera_home;
use crate::screen_panels::DescriptionPanel;
use crate::screen_panels::TitleBar;

impl<S> CameraHomeBuilder<S> {
    /// Sets the home pose yaw in radians.
    #[must_use]
    pub const fn yaw(mut self, yaw: f32) -> Self {
        self.config.yaw = yaw;
        self
    }

    /// Sets the home pose pitch in radians.
    #[must_use]
    pub const fn pitch(mut self, pitch: f32) -> Self {
        self.config.pitch = pitch;
        self
    }

    /// Sets the duration of the `H`-triggered home animation.
    ///
    /// The startup framing is always instant; this only affects subsequent
    /// `H` presses.
    #[must_use]
    pub const fn duration(mut self, duration: Duration) -> Self {
        self.config.duration = duration;
        self
    }

    /// Sets the screen-fraction margin used when framing the home region.
    #[must_use]
    pub const fn margin(mut self, margin: f32) -> Self {
        self.config.margin = margin;
        self
    }

    /// Finalizes the current home registration and starts configuring another.
    #[must_use]
    pub fn with_camera_home(self, transform: Transform) -> Self {
        self.finish().with_camera_home(transform)
    }

    /// Finalizes the current home registration and starts configuring a ground plane.
    #[must_use]
    pub fn with_ground_plane(self) -> PrimitiveBuilder<S> { self.finish().with_ground_plane() }

    /// Finalizes the current home registration and starts configuring a cube.
    #[must_use]
    pub fn with_cube(self) -> PrimitiveBuilder<S> { self.finish().with_cube() }

    /// Finalizes the current home registration and adds window position persistence.
    #[must_use]
    pub fn with_save_window_position(self) -> SprinkleBuilder<S> {
        self.finish().with_save_window_position()
    }

    /// Finalizes the current home registration and adds BRP extras.
    #[must_use]
    pub fn with_brp_extras(self) -> SprinkleBuilder<S> { self.finish().with_brp_extras() }

    /// Finalizes the current home registration and adds the smart camera control panel.
    #[must_use]
    pub fn with_camera_control_panel(self) -> SprinkleBuilder<S> {
        self.finish().with_camera_control_panel()
    }

    /// Finalizes the current home registration and adds studio lighting.
    #[must_use]
    pub fn with_studio_lighting(self) -> StudioLightingBuilder<S> {
        self.finish().with_studio_lighting()
    }

    /// Finalizes the current home registration and adds an example description panel.
    #[must_use]
    pub fn with_description_panel(self, panel: DescriptionPanel) -> SprinkleBuilder<S> {
        self.finish().with_description_panel(panel)
    }

    /// Finalizes the current home registration and adds an example title bar.
    #[must_use]
    pub fn with_title_bar(self, title_bar: TitleBar) -> TitleBarBuilder<S> {
        self.finish().with_title_bar(title_bar)
    }

    /// Finalizes the current home registration and mirrors [`App::add_plugins`].
    #[must_use]
    pub fn add_plugins<M>(self, plugins: impl Plugins<M>) -> SprinkleBuilder<S> {
        self.finish().add_plugins(plugins)
    }

    /// Finalizes the current home registration and mirrors [`App::add_systems`].
    #[must_use]
    pub fn add_systems<M>(
        self,
        schedule: impl ScheduleLabel,
        systems: impl IntoScheduleConfigs<ScheduleSystem, M>,
    ) -> SprinkleBuilder<S> {
        self.finish().add_systems(schedule, systems)
    }

    /// Finalizes the current home registration and mirrors [`App::add_observer`].
    #[must_use]
    pub fn add_observer<E, B, M, I>(self, observer: I) -> SprinkleBuilder<S>
    where
        E: bevy::ecs::event::Event,
        B: Bundle,
        I: bevy::ecs::system::IntoObserverSystem<E, B, M>,
    {
        self.finish().add_observer(observer)
    }

    /// Finalizes the current home registration and mirrors [`App::init_resource`].
    #[must_use]
    pub fn init_resource<R: Resource + FromWorld>(self) -> SprinkleBuilder<S> {
        self.finish().init_resource::<R>()
    }

    /// Finalizes the current home registration and mirrors [`App::insert_resource`].
    #[must_use]
    pub fn insert_resource<R: Resource>(self, resource: R) -> SprinkleBuilder<S> {
        self.finish().insert_resource(resource)
    }

    /// Finalizes the current home registration and runs the configured app.
    pub fn run(self) -> AppExit { self.finish().run() }

    fn finish(mut self) -> SprinkleBuilder<S> {
        camera_home::install(&mut self.parent.app, self.config);
        self.parent
    }
}

impl CameraHomeBuilder<NoOrbitCam> {
    /// Finalizes the current home registration, adds `LagrangePlugin`, and spawns an
    /// `OrbitCam` entity.
    pub fn with_orbit_cam_configured<F>(self, configure: F) -> SprinkleBuilder<WithOrbitCam>
    where
        F: FnOnce(&mut OrbitCam) + Send + Sync + 'static,
    {
        self.finish().with_orbit_cam_configured(configure)
    }

    /// Finalizes the current home registration, adds `LagrangePlugin`, spawns an
    /// `OrbitCam`, and inserts extra camera-side components.
    pub fn with_orbit_cam<F, B>(self, configure: F, bundle: B) -> SprinkleBuilder<WithOrbitCam>
    where
        F: FnOnce(&mut OrbitCam) + Send + Sync + 'static,
        B: Bundle + Send + Sync + 'static,
    {
        self.finish().with_orbit_cam(configure, bundle)
    }
}

impl CameraHomeBuilder<WithOrbitCam> {
    /// Finalizes the current home registration and adds stable transparency to the
    /// spawned `OrbitCam`.
    #[must_use]
    pub fn with_stable_transparency(self) -> SprinkleBuilder<WithOrbitCam> {
        self.finish().with_stable_transparency()
    }
}
