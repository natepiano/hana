//! `SprinkleBuilder` impls — state-agnostic, `NoOrbitCam`, and `WithOrbitCam`.

use std::marker::PhantomData;

use bevy::app::App;
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
use super::PrimitiveBuilder;
use super::StudioLightingBuilder;
use super::TitleBarBuilder;
use crate::Anchor;
use crate::bloom;
use crate::brp_extras;
use crate::camera_control_panel;
use crate::camera_control_panel::CameraControlPanelBackground;
use crate::camera_control_panel::CameraPresetSwitching;
use crate::camera_home::CameraHomeConfig;
use crate::camera_home::HomeTitleBarControl;
use crate::cube_spin;
use crate::cube_spin::CubeSpinConfig;
use crate::environment_map;
use crate::fold_controls;
use crate::hdr;
use crate::lighting::StudioLightingConfig;
use crate::orbit_cam;
use crate::orbit_cam::OrbitCamPose;
use crate::primitive::PrimitiveConfig;
use crate::restart;
use crate::restart_camera;
use crate::save_window_position;
use crate::screen_panels;
use crate::screen_panels::DescriptionPanel;
use crate::screen_panels::TitleBar;
use crate::shortcuts;
use crate::transparency;
use crate::unclamp;

/// Typestate marker: the builder has not yet spawned an `OrbitCam`.
///
/// Camera-attached capabilities are not defined for `SprinkleBuilder<NoOrbitCam>`,
/// so calling them is a compile error.
pub struct NoOrbitCam;

/// Typestate marker: the builder has spawned an `OrbitCam`.
///
/// Reached via [`SprinkleBuilder::with_orbit_cam_configured`]. Camera-attached
/// capabilities like [`SprinkleBuilder::with_stable_transparency`]
/// become callable in this state.
pub struct WithOrbitCam;

/// Builder returned by [`sprinkle_example`](crate::sprinkle_example). State-agnostic capability
/// methods are defined for any `S`; camera-attached methods are gated by
/// the typestate.
pub struct SprinkleBuilder<S> {
    pub(super) app:          App,
    pub(super) state_marker: PhantomData<S>,
}

// State-agnostic capabilities — available regardless of whether an `OrbitCam`
// has been configured.
impl<S> SprinkleBuilder<S> {
    /// Installs Hana fold playback with the standard `Space` fold,
    /// `Shift+Space` unfold, and `P` play controls.
    #[must_use]
    pub fn with_fold_controls(mut self) -> Self {
        fold_controls::install(&mut self.app);
        self
    }

    /// Add a `bevy_clerestory` `WindowManagerPlugin` so window position
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

    /// Enable HDR output on every camera (current and later-spawned). Required
    /// for over-bright (>1.0) colors to survive a multi-camera diegetic render
    /// chain — any camera left in LDR clamps them at that step.
    #[must_use]
    pub fn with_hdr(mut self) -> Self {
        hdr::install(&mut self.app);
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

    /// Wire a one-shot keyboard shortcut: pressing `key` runs `system` once.
    /// The example writes only a plain Bevy system — no input macros, no input
    /// crate import:
    ///
    /// ```ignore
    /// fairy_dust::sprinkle_example()
    ///     .with_shortcut(KeyCode::KeyL, look_at)
    ///     .run();
    ///
    /// fn look_at(camera: Query<Entity, With<OrbitCam>>, /* ... */) { /* ... */ }
    /// ```
    ///
    /// The shortcut fires only while no modifier is held, so a bare key stays
    /// quiet during `Ctrl`/`Shift`/`Alt`/`Cmd` and Fairy Dust's modifier chords
    /// (`Ctrl+Shift+L` and friends) reach only their own action.
    ///
    /// Reusing a key Fairy Dust already binds bare (`H` home with
    /// [`with_camera_home`](Self::with_camera_home), `P` with
    /// [`with_cube_spin`](Self::with_cube_spin) or
    /// [`with_fold_controls`](Self::with_fold_controls)) fails at startup —
    /// use the matching capability instead of a manual shortcut.
    #[must_use]
    pub fn with_shortcut<Sys, M>(mut self, key: KeyCode, system: Sys) -> Self
    where
        Sys: IntoSystem<(), (), M> + 'static,
    {
        shortcuts::install(&mut self.app);
        let system_id = self.app.world_mut().register_system(system);
        shortcuts::register_press(&mut self.app, key, system_id);
        self
    }

    /// Wire a continuous keyboard shortcut: while `key` is held, `system` runs
    /// every frame. Use this for held motion such as a light brighten/dim or a
    /// log scroll; the `system` reads `Res<Time>` itself and scales by
    /// `delta_secs`.
    ///
    /// Modifier guarding and the reserved-key check match
    /// [`with_shortcut`](Self::with_shortcut); only the firing cadence differs
    /// (every held frame instead of once per press).
    #[must_use]
    pub fn with_held_shortcut<Sys, M>(mut self, key: KeyCode, system: Sys) -> Self
    where
        Sys: IntoSystem<(), (), M> + 'static,
    {
        shortcuts::install(&mut self.app);
        let system_id = self.app.world_mut().register_system(system);
        shortcuts::register_held(&mut self.app, key, system_id);
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
    /// Snaps the camera to the union of every [`crate::CameraHomeTarget`] on
    /// startup, refits it on window resize while still home, and fills empty
    /// Lagrange presets with Fairy Dust's home input (`H` for keyboard-family
    /// presets, Select for gamepads). If a title bar is installed, the `H Home`
    /// chip is prepended automatically unless disabled with
    /// [`CameraHomeBuilder::without_title_bar_control`].
    #[must_use]
    pub const fn with_camera_home(self) -> CameraHomeBuilder<S> {
        CameraHomeBuilder {
            parent: self,
            config: CameraHomeConfig {
                yaw:               0.0,
                pitch:             0.0,
                margin:            crate::constants::HOME_DEFAULT_MARGIN,
                anchor:            Anchor::Center,
                offset_px:         Vec2::ZERO,
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
    pub(crate) const fn new(app: App) -> Self {
        Self {
            app,
            state_marker: PhantomData,
        }
    }

    /// Add `bevy_lagrange::LagrangePlugin` and spawn an `OrbitCam` entity.
    /// The caller's `configure` closure can set `focus`, `radius`, `yaw`,
    /// `pitch`, sensitivity, limits, or other camera behavior fields. Input
    /// uses `OrbitCamPreset::simple_mouse()` unless another input mode is inserted.
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
        preset: impl Into<OrbitCamPreset>,
    ) -> SprinkleBuilder<WithOrbitCam>
    where
        F: FnOnce(&mut OrbitCam) + Send + Sync + 'static,
    {
        self.with_orbit_cam(configure, OrbitCamInputMode::with_preset(preset))
    }

    /// Add `bevy_lagrange::LagrangePlugin`, spawn an `OrbitCam` entity with an
    /// explicit startup pose, and install one built-in input preset.
    pub fn with_orbit_cam_preset_pose(
        mut self,
        pose: OrbitCamPose,
        preset: impl Into<OrbitCamPreset>,
    ) -> SprinkleBuilder<WithOrbitCam> {
        orbit_cam::install_pose_with_bundle(
            &mut self.app,
            pose,
            OrbitCamInputMode::with_preset(preset),
        );
        SprinkleBuilder {
            app:          self.app,
            state_marker: PhantomData,
        }
    }

    /// Add `bevy_lagrange::LagrangePlugin`, spawn an `OrbitCam` entity,
    /// install one built-in input preset, and insert extra camera-side
    /// components.
    pub fn with_orbit_cam_preset_bundle<F, B>(
        self,
        configure: F,
        preset: impl Into<OrbitCamPreset>,
        bundle: B,
    ) -> SprinkleBuilder<WithOrbitCam>
    where
        F: FnOnce(&mut OrbitCam) + Send + Sync + 'static,
        B: Bundle + Send + Sync + 'static,
    {
        self.with_orbit_cam(configure, (OrbitCamInputMode::with_preset(preset), bundle))
    }

    /// Add `bevy_lagrange::LagrangePlugin`, spawn an `OrbitCam` entity with an
    /// explicit startup pose, install one built-in input preset, and insert extra
    /// camera-side components.
    pub fn with_orbit_cam_preset_pose_bundle<B>(
        mut self,
        pose: OrbitCamPose,
        preset: impl Into<OrbitCamPreset>,
        bundle: B,
    ) -> SprinkleBuilder<WithOrbitCam>
    where
        B: Bundle + Send + Sync + 'static,
    {
        orbit_cam::install_pose_with_bundle(
            &mut self.app,
            pose,
            (OrbitCamInputMode::with_preset(preset), bundle),
        );
        SprinkleBuilder {
            app:          self.app,
            state_marker: PhantomData,
        }
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

    /// Add a thresholded bloom to the orbit camera so over-bright (>1.0) colors
    /// glow while normal-range content stays crisp. `Bloom` requires HDR; pair
    /// with [`with_hdr`](Self::with_hdr) so every camera in a multi-camera
    /// render chain keeps the over-bright values.
    #[must_use]
    pub fn with_bloom(mut self) -> Self {
        bloom::install(&mut self.app);
        self
    }

    /// Insert an [`EnvironmentMapLight`] on the orbit camera, lighting PBR
    /// surfaces (panel backgrounds and text glyphs both run `apply_pbr_lighting`)
    /// with diffuse ambient fill and specular reflection from a bundled
    /// `pisa` cathedral HDRI. The cubemaps are embedded, so no runtime
    /// `assets/` directory is required.
    ///
    /// A sharp specular glint only appears on a metallic, low-roughness
    /// surface; on the default rough-dielectric panel material the visible
    /// effect is the diffuse ambient term.
    #[must_use]
    pub fn with_environment_map(mut self) -> Self {
        environment_map::install(&mut self.app);
        self
    }

    /// Clear the example-default pitch and zoom limits on the spawned
    /// `OrbitCam`: the `orbit` angle limit resets to unbounded and the `zoom`
    /// limit drops its ceiling, keeping only a tiny positive floor.
    /// Use to inspect geometry from steep angles or at extreme zoom. Overrides
    /// limits set in the camera `configure` closure.
    #[must_use]
    pub fn unclamped(mut self) -> Self {
        unclamp::install(&mut self.app);
        self
    }
}

#[cfg(test)]
mod tests {
    use bevy_lagrange::OrbitCamBlenderLikePreset;

    use super::NoOrbitCam;
    use super::SprinkleBuilder;
    use super::WithOrbitCam;
    use crate::builder::CameraHomeBuilder;
    use crate::builder::PrimitiveBuilder;
    use crate::builder::StudioLightingBuilder;
    use crate::builder::TitleBarBuilder;

    #[test]
    fn builder_wrappers_accept_typed_preset_payloads() {
        let _: fn(
            SprinkleBuilder<NoOrbitCam>,
            OrbitCamBlenderLikePreset,
        ) -> SprinkleBuilder<WithOrbitCam> = sprinkle_builder_with_preset;
        let _: fn(
            PrimitiveBuilder<NoOrbitCam>,
            OrbitCamBlenderLikePreset,
        ) -> SprinkleBuilder<WithOrbitCam> = primitive_builder_with_preset;
        let _: fn(
            StudioLightingBuilder<NoOrbitCam>,
            OrbitCamBlenderLikePreset,
        ) -> SprinkleBuilder<WithOrbitCam> = studio_lighting_builder_with_preset;
        let _: fn(
            CameraHomeBuilder<NoOrbitCam>,
            OrbitCamBlenderLikePreset,
        ) -> SprinkleBuilder<WithOrbitCam> = camera_home_builder_with_preset;
        let _: fn(
            TitleBarBuilder<NoOrbitCam>,
            OrbitCamBlenderLikePreset,
        ) -> SprinkleBuilder<WithOrbitCam> = title_bar_builder_with_preset;
    }

    fn sprinkle_builder_with_preset(
        builder: SprinkleBuilder<NoOrbitCam>,
        preset: OrbitCamBlenderLikePreset,
    ) -> SprinkleBuilder<WithOrbitCam> {
        builder.with_orbit_cam_preset(|_| {}, preset)
    }

    fn primitive_builder_with_preset(
        builder: PrimitiveBuilder<NoOrbitCam>,
        preset: OrbitCamBlenderLikePreset,
    ) -> SprinkleBuilder<WithOrbitCam> {
        builder.with_orbit_cam_preset(|_| {}, preset)
    }

    fn studio_lighting_builder_with_preset(
        builder: StudioLightingBuilder<NoOrbitCam>,
        preset: OrbitCamBlenderLikePreset,
    ) -> SprinkleBuilder<WithOrbitCam> {
        builder.with_orbit_cam_preset(|_| {}, preset)
    }

    fn camera_home_builder_with_preset(
        builder: CameraHomeBuilder<NoOrbitCam>,
        preset: OrbitCamBlenderLikePreset,
    ) -> SprinkleBuilder<WithOrbitCam> {
        builder.with_orbit_cam_preset(|_| {}, preset)
    }

    fn title_bar_builder_with_preset(
        builder: TitleBarBuilder<NoOrbitCam>,
        preset: OrbitCamBlenderLikePreset,
    ) -> SprinkleBuilder<WithOrbitCam> {
        builder.with_orbit_cam_preset(|_| {}, preset)
    }
}
