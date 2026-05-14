//! `TitleBarBuilder` impls.

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
use crate::screen_panels::DescriptionPanel;
use crate::screen_panels::TitleBar;
use crate::screen_panels::TitleBarControlState;

impl<S> TitleBarBuilder<S> {
    /// Toggles `chip` active on `Begin` and inactive on `End` for any event
    /// of those types. Use this when only one chip cares about the event.
    /// For multi-chip discrimination (e.g. `AnimationBegin` shared by
    /// `LookAt` and `LookAtAndZoomToFit`), use
    /// [`Self::wire_chip_to_events_filtered`] instead.
    #[must_use]
    pub fn wire_chip_to_events<Begin, End>(mut self, chip: impl Into<String>) -> Self
    where
        Begin: bevy::ecs::event::Event,
        End: bevy::ecs::event::Event,
    {
        let chip = chip.into();
        let activate = chip.clone();
        self.parent.app.add_observer(
            move |_: On<Begin>, mut bars: Query<&mut TitleBarControlState>| {
                for mut bar in &mut bars {
                    bar.set_active(&activate, true);
                }
            },
        );
        let deactivate = chip;
        self.parent.app.add_observer(
            move |_: On<End>, mut bars: Query<&mut TitleBarControlState>| {
                for mut bar in &mut bars {
                    bar.set_active(&deactivate, false);
                }
            },
        );
        self
    }

    /// Like [`Self::wire_chip_to_events`], but each filter decides whether a
    /// given event applies to this chip. Return `false` to ignore.
    #[must_use]
    pub fn wire_chip_to_events_filtered<Begin, End, FStart, FEnd>(
        mut self,
        chip: impl Into<String>,
        start_filter: FStart,
        end_filter: FEnd,
    ) -> Self
    where
        Begin: bevy::ecs::event::Event,
        End: bevy::ecs::event::Event,
        FStart: Fn(&Begin) -> bool + Send + Sync + 'static,
        FEnd: Fn(&End) -> bool + Send + Sync + 'static,
    {
        let chip = chip.into();
        let activate = chip.clone();
        self.parent.app.add_observer(
            move |trigger: On<Begin>, mut bars: Query<&mut TitleBarControlState>| {
                if !start_filter(&trigger) {
                    return;
                }
                for mut bar in &mut bars {
                    bar.set_active(&activate, true);
                }
            },
        );
        let deactivate = chip;
        self.parent.app.add_observer(
            move |trigger: On<End>, mut bars: Query<&mut TitleBarControlState>| {
                if !end_filter(&trigger) {
                    return;
                }
                for mut bar in &mut bars {
                    bar.set_active(&deactivate, false);
                }
            },
        );
        self
    }

    fn finish(self) -> SprinkleBuilder<S> { self.parent }

    /// Finalizes the title bar and starts configuring a ground plane.
    #[must_use]
    pub fn with_ground_plane(self) -> PrimitiveBuilder<S> { self.finish().with_ground_plane() }

    /// Finalizes the title bar and starts configuring a cube.
    #[must_use]
    pub fn with_cube(self) -> PrimitiveBuilder<S> { self.finish().with_cube() }

    /// Finalizes the title bar and adds window position persistence.
    #[must_use]
    pub fn with_save_window_position(self) -> SprinkleBuilder<S> {
        self.finish().with_save_window_position()
    }

    /// Finalizes the title bar and adds BRP extras.
    #[must_use]
    pub fn with_brp_extras(self) -> SprinkleBuilder<S> { self.finish().with_brp_extras() }

    /// Finalizes the title bar and adds the smart camera control panel.
    #[must_use]
    pub fn with_camera_control_panel(self) -> SprinkleBuilder<S> {
        self.finish().with_camera_control_panel()
    }

    /// Finalizes the title bar and adds studio lighting.
    #[must_use]
    pub fn with_studio_lighting(self) -> SprinkleBuilder<S> { self.finish().with_studio_lighting() }

    /// Finalizes the title bar and adds an example description panel.
    #[must_use]
    pub fn with_description_panel(self, panel: DescriptionPanel) -> SprinkleBuilder<S> {
        self.finish().with_description_panel(panel)
    }

    /// Finalizes the title bar and installs another title bar.
    #[must_use]
    pub fn with_title_bar(self, title_bar: TitleBar) -> Self {
        self.finish().with_title_bar(title_bar)
    }

    /// Finalizes the title bar and starts configuring a camera home pose.
    #[must_use]
    pub fn with_camera_home(self, transform: Transform) -> CameraHomeBuilder<S> {
        self.finish().with_camera_home(transform)
    }

    /// Finalizes the title bar and mirrors [`App::add_plugins`].
    #[must_use]
    pub fn add_plugins<M>(self, plugins: impl Plugins<M>) -> SprinkleBuilder<S> {
        self.finish().add_plugins(plugins)
    }

    /// Finalizes the title bar and mirrors [`App::add_systems`].
    #[must_use]
    pub fn add_systems<M>(
        self,
        schedule: impl ScheduleLabel,
        systems: impl IntoScheduleConfigs<ScheduleSystem, M>,
    ) -> SprinkleBuilder<S> {
        self.finish().add_systems(schedule, systems)
    }

    /// Finalizes the title bar and mirrors [`App::add_observer`].
    #[must_use]
    pub fn add_observer<E, B, M, I>(self, observer: I) -> SprinkleBuilder<S>
    where
        E: bevy::ecs::event::Event,
        B: Bundle,
        I: bevy::ecs::system::IntoObserverSystem<E, B, M>,
    {
        self.finish().add_observer(observer)
    }

    /// Finalizes the title bar and mirrors [`App::init_resource`].
    #[must_use]
    pub fn init_resource<R: Resource + FromWorld>(self) -> SprinkleBuilder<S> {
        self.finish().init_resource::<R>()
    }

    /// Finalizes the title bar and mirrors [`App::insert_resource`].
    #[must_use]
    pub fn insert_resource<R: Resource>(self, resource: R) -> SprinkleBuilder<S> {
        self.finish().insert_resource(resource)
    }

    /// Finalizes the title bar and runs the configured app.
    pub fn run(self) -> AppExit { self.finish().run() }
}

impl TitleBarBuilder<NoOrbitCam> {
    /// Finalizes the title bar, adds `LagrangePlugin`, and spawns an `OrbitCam`.
    pub fn with_orbit_cam_configured<F>(self, configure: F) -> SprinkleBuilder<WithOrbitCam>
    where
        F: FnOnce(&mut OrbitCam) + Send + Sync + 'static,
    {
        self.finish().with_orbit_cam_configured(configure)
    }

    /// Finalizes the title bar, adds `LagrangePlugin`, spawns an `OrbitCam`,
    /// and inserts extra camera-side components.
    pub fn with_orbit_cam<F, B>(self, configure: F, bundle: B) -> SprinkleBuilder<WithOrbitCam>
    where
        F: FnOnce(&mut OrbitCam) + Send + Sync + 'static,
        B: Bundle + Send + Sync + 'static,
    {
        self.finish().with_orbit_cam(configure, bundle)
    }
}

impl TitleBarBuilder<WithOrbitCam> {
    /// Finalizes the title bar and adds stable transparency to the spawned
    /// `OrbitCam`.
    #[must_use]
    pub fn with_stable_transparency(self) -> SprinkleBuilder<WithOrbitCam> {
        self.finish().with_stable_transparency()
    }
}
