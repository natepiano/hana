//! `SprinkleBuilder` impls — state-agnostic, `NoOrbitCam`, and `WithOrbitCam`.

use std::marker::PhantomData;

use bevy::app::Plugins;
use bevy::ecs::schedule::IntoScheduleConfigs;
use bevy::ecs::schedule::ScheduleLabel;
use bevy::ecs::system::ScheduleSystem;
use bevy::prelude::*;
use bevy::window::PresentMode;
use bevy::window::PrimaryWindow;
use bevy::winit::WinitSettings;
use bevy_lagrange::OrbitCam;
use bevy_lagrange::OrbitCamBindings;
use bevy_lagrange::OrbitCamInputMode;
use bevy_lagrange::OrbitCamPreset;

use super::CameraHomeBuilder;
use super::NoOrbitCam;
use super::PrimitiveBuilder;
use super::SprinkleBuilder;
use super::StudioLightingBuilder;
use super::TitleBarBuilder;
use super::WithOrbitCam;
use crate::brp_extras;
use crate::camera_control_panel;
use crate::camera_control_panel::CameraControlPanelBackground;
use crate::camera_control_panel::CameraPresetSwitching;
use crate::camera_home::CameraHomeConfig;
use crate::camera_home::HomeTitleBarControl;
use crate::cube_spin;
use crate::cube_spin::CubeSpinConfig;
use crate::lighting::StudioLightingConfig;
use crate::orbit_cam;
use crate::primitive::PrimitiveConfig;
use crate::restart;
use crate::restart_camera;
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

    /// Uncap the frame rate so a stress example reports its true per-frame
    /// cost. Sets the primary window to [`PresentMode::AutoNoVsync`] and swaps
    /// [`WinitSettings`] to [`continuous`](WinitSettings::continuous).
    ///
    /// The Bevy defaults hide real cost two ways: vsync (`Fifo`) pins frame
    /// time to display-refresh steps (120 / 60 / 40 fps on a 120 Hz panel),
    /// and the default [`WinitSettings::game`] throttles an unfocused window to
    /// 60 Hz reactive-low-power. With both removed, the on-screen overlay and a
    /// background BRP reader both see un-throttled frame time.
    #[must_use]
    pub fn with_perf_mode(mut self) -> Self {
        self.app.insert_resource(WinitSettings::continuous());
        let mut windows = self
            .app
            .world_mut()
            .query_filtered::<&mut Window, With<PrimaryWindow>>();
        if let Ok(mut window) = windows.single_mut(self.app.world_mut()) {
            window.present_mode = PresentMode::AutoNoVsync;
        }
        self
    }

    /// Enable smart screen-space camera control panels for `OrbitCam` cameras.
    ///
    /// Cameras without an explicit [`CameraGuidance`](crate::CameraGuidance) component get
    /// [`CameraGuidance::auto()`](crate::CameraGuidance::auto), so the panel reflects the effective
    /// preset or binding configuration and highlights active interactions.
    #[must_use]
    pub fn with_camera_control_panel(mut self) -> Self {
        camera_control_panel::install(&mut self.app);
        self
    }

    /// Pins the camera to its spawned preset: suppresses the Shift+C cycle and
    /// its entry in the keyboard-shortcut overlay. Pair with
    /// [`with_camera_control_panel`](Self::with_camera_control_panel).
    #[must_use]
    pub fn lock_camera_preset(mut self) -> Self {
        self.app.insert_resource(CameraPresetSwitching::Disabled);
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
            .insert_resource(CameraControlPanelBackground(color));
        self
    }

    /// Adds a marker-scoped cube spin helper.
    #[must_use]
    pub fn with_cube_spin<M: Component>(self) -> Self {
        self.with_cube_spin_config::<M>(CubeSpinConfig::default())
    }

    /// Adds a marker-scoped cube spin helper with a customized configuration.
    #[must_use]
    pub fn with_cube_spin_config<M: Component>(mut self, config: CubeSpinConfig) -> Self {
        cube_spin::install::<M>(&mut self.app, config);
        self
    }

    /// Add a reusable key/fill/rim lighting setup for simple example scenes.
    ///
    /// Returns a [`StudioLightingBuilder`] so the key light position and aim
    /// target can be tweaked before the rig spawns. Methods on the returned
    /// builder are only reachable through this call, so lighting tweaks are
    /// a compile error without `with_studio_lighting`.
    #[must_use]
    pub fn with_studio_lighting(self) -> StudioLightingBuilder<S> {
        StudioLightingBuilder {
            parent: self,
            config: StudioLightingConfig::default(),
        }
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
    /// Wires `H` to an [`bevy_lagrange::AnimateToFit`] of the union of every
    /// [`crate::CameraHomeTarget`] entity using the configured `yaw`/`pitch`. If
    /// a title bar is installed, the `H Home` chip is prepended automatically
    /// and highlights for the duration of the home animation unless disabled
    /// with [`CameraHomeBuilder::without_title_bar_control`].
    #[must_use]
    pub const fn with_camera_home(self) -> CameraHomeBuilder<S> {
        CameraHomeBuilder {
            parent: self,
            config: CameraHomeConfig {
                yaw:               0.0,
                pitch:             0.0,
                duration:          crate::constants::HOME_DEFAULT_DURATION,
                margin:            crate::constants::HOME_DEFAULT_MARGIN,
                title_bar_control: HomeTitleBarControl::Shown,
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
    /// uses `OrbitCamInputMode::Preset(OrbitCamPreset::SimpleMouse)` unless another input mode
    /// is inserted.
    pub fn with_orbit_cam_configured<F>(mut self, configure: F) -> SprinkleBuilder<WithOrbitCam>
    where
        F: FnOnce(&mut OrbitCam) + Send + Sync + 'static,
    {
        orbit_cam::install_with(&mut self.app, configure);
        SprinkleBuilder {
            app:          self.app,
            state_marker: PhantomData,
        }
    }

    /// Add `bevy_lagrange::LagrangePlugin`, spawn an `OrbitCam` entity, and
    /// insert extra camera-side components such as `OrbitCamInputMode` or
    /// [`CameraGuidance`](crate::CameraGuidance).
    pub fn with_orbit_cam<F, B>(mut self, configure: F, bundle: B) -> SprinkleBuilder<WithOrbitCam>
    where
        F: FnOnce(&mut OrbitCam) + Send + Sync + 'static,
        B: Bundle + Send + Sync + 'static,
    {
        orbit_cam::install_with_bundle(&mut self.app, configure, bundle);
        SprinkleBuilder {
            app:          self.app,
            state_marker: PhantomData,
        }
    }

    /// Add `bevy_lagrange::LagrangePlugin`, spawn an `OrbitCam` entity, and
    /// install one built-in input preset.
    pub fn with_orbit_cam_preset<F>(
        self,
        configure: F,
        preset: OrbitCamPreset,
    ) -> SprinkleBuilder<WithOrbitCam>
    where
        F: FnOnce(&mut OrbitCam) + Send + Sync + 'static,
    {
        self.with_orbit_cam(configure, OrbitCamInputMode::Preset(preset))
    }

    /// Add `bevy_lagrange::LagrangePlugin`, spawn an `OrbitCam` entity,
    /// install one built-in input preset, and insert extra camera-side
    /// components.
    pub fn with_orbit_cam_preset_bundle<F, B>(
        self,
        configure: F,
        preset: OrbitCamPreset,
        bundle: B,
    ) -> SprinkleBuilder<WithOrbitCam>
    where
        F: FnOnce(&mut OrbitCam) + Send + Sync + 'static,
        B: Bundle + Send + Sync + 'static,
    {
        self.with_orbit_cam(configure, (OrbitCamInputMode::Preset(preset), bundle))
    }

    /// Add `bevy_lagrange::LagrangePlugin`, spawn an `OrbitCam` entity, and
    /// install app-owned input bindings.
    pub fn with_orbit_cam_bindings<F>(
        self,
        configure: F,
        bindings: OrbitCamBindings,
    ) -> SprinkleBuilder<WithOrbitCam>
    where
        F: FnOnce(&mut OrbitCam) + Send + Sync + 'static,
    {
        self.with_orbit_cam(configure, OrbitCamInputMode::Bindings(bindings))
    }

    /// Add `bevy_lagrange::LagrangePlugin`, spawn an `OrbitCam` entity,
    /// install app-owned input bindings, and insert extra camera-side
    /// components.
    pub fn with_orbit_cam_bindings_bundle<F, B>(
        self,
        configure: F,
        bindings: OrbitCamBindings,
        bundle: B,
    ) -> SprinkleBuilder<WithOrbitCam>
    where
        F: FnOnce(&mut OrbitCam) + Send + Sync + 'static,
        B: Bundle + Send + Sync + 'static,
    {
        self.with_orbit_cam(configure, (OrbitCamInputMode::Bindings(bindings), bundle))
    }

    /// Add `bevy_lagrange::LagrangePlugin` and spawn a manually driven
    /// `OrbitCam` entity.
    pub fn with_orbit_cam_manual<F>(self, configure: F) -> SprinkleBuilder<WithOrbitCam>
    where
        F: FnOnce(&mut OrbitCam) + Send + Sync + 'static,
    {
        self.with_orbit_cam(configure, OrbitCamInputMode::Manual)
    }

    /// Add `bevy_lagrange::LagrangePlugin`, spawn a manually driven `OrbitCam`,
    /// and insert extra camera-side components.
    pub fn with_orbit_cam_manual_bundle<F, B>(
        self,
        configure: F,
        bundle: B,
    ) -> SprinkleBuilder<WithOrbitCam>
    where
        F: FnOnce(&mut OrbitCam) + Send + Sync + 'static,
        B: Bundle + Send + Sync + 'static,
    {
        self.with_orbit_cam(configure, (OrbitCamInputMode::Manual, bundle))
    }
}

// Camera-attached capabilities — only valid after an `OrbitCam` has been
// configured.
impl SprinkleBuilder<WithOrbitCam> {
    /// Capture the current `OrbitCam` pose on hot restart and make the restore
    /// animation available through [`crate::RestoreWindowAnimation`].
    #[must_use]
    pub fn with_restore_camera_on_restart(mut self) -> Self {
        restart_camera::install(&mut self.app);
        self
    }

    /// Insert `bevy_diegetic::StableTransparency` on the spawned `OrbitCam`,
    /// which adds `OrderIndependentTransparencySettings`, sets the camera's
    /// depth texture to `TEXTURE_BINDING`, and forces `Msaa::Off` on the
    /// camera and on every screen-space overlay camera in the app.
    ///
    /// Use this when coplanar `WorldText` shows a view-angle color shift,
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
