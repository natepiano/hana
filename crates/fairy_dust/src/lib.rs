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
use std::time::Duration;

use bevy::app::Plugins;
use bevy::ecs::schedule::IntoScheduleConfigs;
use bevy::ecs::schedule::ScheduleLabel;
use bevy::ecs::system::ScheduleSystem;
use bevy::log::LogPlugin;
use bevy::prelude::*;
pub use bevy_diegetic::Anchor;
use bevy_diegetic::DiegeticUiPlugin;
pub use bevy_lagrange::OrbitCam;
pub use camera_control_panel::CameraGuidance;
pub use camera_control_panel::CameraGuidanceRow;
use camera_home::CameraHomeConfig;
pub use primitive::Face;
use primitive::FaceTextSpec;
use primitive::PrimitiveConfig;
pub use primitive::cube_face_text;
pub use screen_panels::DescriptionPanel;
pub use screen_panels::TitleBar;
pub use screen_panels::TitleBarControlState;

mod brp_extras;
mod camera_control_panel;
mod camera_home;
mod lighting;
mod orbit_cam;
mod primitive;
mod restart;
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

/// Builder returned while configuring a camera "home" pose.
///
/// Calling a non-home builder method finalizes the home registration and
/// returns to the normal [`SprinkleBuilder`] chain.
pub struct CameraHomeBuilder<S> {
    parent: SprinkleBuilder<S>,
    config: CameraHomeConfig,
}

/// Builder returned by [`SprinkleBuilder::with_title_bar`] for wiring chip
/// highlights to event lifecycles. Chip-wiring methods are only reachable
/// through this type, so calling [`Self::wire_chip_to_events`] is a compile
/// error when no title bar has been installed.
///
/// Calling a non-wiring builder method finalizes the title bar configuration
/// and returns to the normal [`SprinkleBuilder`] chain.
pub struct TitleBarBuilder<S> {
    parent: SprinkleBuilder<S>,
}

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
                duration: camera_home::HOME_DEFAULT_DURATION,
                margin: camera_home::HOME_DEFAULT_MARGIN,
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
    /// that a `Ctrl+Shift+R` press handled via [`Self::with_restart_key`]
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

    /// Adds a centered [`bevy_diegetic::WorldText`] label to one face of a
    /// cube primitive. The label inherits the cube's `Transform` as parent,
    /// is sized in world meters by `text_size`, and uses one-sided glyphs.
    ///
    /// Only meaningful for cube primitives; on a ground plane the label is
    /// still attached as a child but its placement uses the cube face math
    /// and may not be what you want.
    #[must_use]
    pub fn face_text(
        mut self,
        face: Face,
        text: impl Into<String>,
        text_size: f32,
        color: Color,
    ) -> Self {
        self.config.push_face_text(FaceTextSpec {
            face,
            text: text.into(),
            text_size,
            color,
        });
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
    pub fn with_title_bar(self, title_bar: TitleBar) -> TitleBarBuilder<S> {
        self.finish().with_title_bar(title_bar)
    }

    /// Finalizes the current primitive and starts configuring a camera home pose.
    #[must_use]
    pub fn with_camera_home(self, transform: Transform) -> CameraHomeBuilder<S> {
        self.finish().with_camera_home(transform)
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

    /// Finalizes the current primitive and mirrors [`App::add_observer`].
    #[must_use]
    pub fn add_observer<E, B, M, I>(self, observer: I) -> SprinkleBuilder<S>
    where
        E: bevy::ecs::event::Event,
        B: Bundle,
        I: bevy::ecs::system::IntoObserverSystem<E, B, M>,
    {
        self.finish().add_observer(observer)
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
    pub fn with_orbit_cam<F, B>(self, configure: F, bundle: B) -> SprinkleBuilder<WithOrbitCam>
    where
        F: FnOnce(&mut OrbitCam) + Send + Sync + 'static,
        B: Bundle + Send + Sync + 'static,
    {
        self.finish().with_orbit_cam(configure, bundle)
    }
}

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
    pub fn with_studio_lighting(self) -> SprinkleBuilder<S> { self.finish().with_studio_lighting() }

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
    pub fn with_orbit_cam_bundle<F, B>(
        self,
        configure: F,
        bundle: B,
    ) -> SprinkleBuilder<WithOrbitCam>
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
    pub fn with_title_bar(self, title_bar: TitleBar) -> TitleBarBuilder<S> {
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
