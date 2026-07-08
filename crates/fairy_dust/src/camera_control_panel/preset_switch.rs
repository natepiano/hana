//! Capability: Shift+C cycles the routed camera through `OrbitCam` presets,
//! a `FreeCam` preset, and a hidden camera-control panel state.
//!
//! Wired through `bevy_enhanced_input` using the `bevy_kana` macros
//! (`action!`, `event!`, `bind_action_system!`), modeled on [`crate::restart`].
//! Installed alongside the camera control panel, so every panel example gains
//! the switch. The bound system only acts when the camera is in a built-in
//! preset mode, leaving `Bindings`/`Manual` cameras untouched. The hidden panel
//! state leaves the camera on its current preset; the next Shift+C restores the
//! panel and returns to `SimpleMouse`.

use bevy::prelude::*;
use bevy_enhanced_input::prelude::*;
use bevy_kana::action;
use bevy_kana::bind_action_system;
use bevy_kana::event;
use bevy_lagrange::CameraBasis;
use bevy_lagrange::CameraHomePending;
use bevy_lagrange::FreeCam;
use bevy_lagrange::FreeCamHomePose;
use bevy_lagrange::FreeCamInput;
use bevy_lagrange::FreeCamInputContext;
use bevy_lagrange::FreeCamInputMode;
use bevy_lagrange::FreeCamLookPitch;
use bevy_lagrange::FreeCamPreset;
use bevy_lagrange::FreeCamPresetKind;
use bevy_lagrange::OrbitCam;
use bevy_lagrange::OrbitCamHomePose;
use bevy_lagrange::OrbitCamInput;
use bevy_lagrange::OrbitCamInputContext;
use bevy_lagrange::OrbitCamInputMode;
use bevy_lagrange::OrbitCamPreset;
use bevy_lagrange::OrbitCamPresetKind;
use bevy_lagrange::ResolvedCameraInputRoute;

use super::CameraGuidancePanel;
use super::CameraGuidanceRevealPending;
use super::free_settings;
use super::free_settings::FreeCamLookPitchPreference;
use crate::ensure_plugin;
use crate::orbit_cam;

/// Whether the camera control panel's Shift+C preset cycling is wired up.
///
/// Defaults to [`Enabled`](Self::Enabled); the `lock_camera_preset` builder
/// method sets [`Disabled`](Self::Disabled) to pin the camera to one preset.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum CameraPresetSwitching {
    /// Shift+C cycles presets.
    #[default]
    Enabled,
    /// Preset switching is suppressed.
    Disabled,
}

#[derive(Clone, Debug, PartialEq)]
enum PresetCycleEntry {
    OrbitPreset(OrbitCamPreset),
    FreePreset(FreeCamPreset),
    Off,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CameraCycleMode {
    OrbitPreset(OrbitCamPresetKind),
    FreePreset(FreeCamPresetKind),
}

#[derive(Component)]
struct FairyDustPresetContext;

action!(CyclePreset);
event!(CyclePresetEvent);

pub(super) fn install(app: &mut App) {
    ensure_plugin(app, EnhancedInputPlugin);
    app.init_resource::<CameraPresetSwitching>();
    app.add_input_context::<FairyDustPresetContext>();
    app.add_systems(Startup, spawn_preset_action);
    bind_action_system!(app, CyclePreset, CyclePresetEvent, cycle_preset);
}

fn spawn_preset_action(mut commands: Commands, switching: Res<CameraPresetSwitching>) {
    if *switching == CameraPresetSwitching::Disabled {
        return;
    }
    commands.spawn((
        FairyDustPresetContext,
        actions!(FairyDustPresetContext[
            (
                Action::<CyclePreset>::new(),
                bindings![KeyCode::KeyC.with_mod_keys(ModKeys::SHIFT)],
            ),
        ]),
    ));
}

fn cycle_preset(
    mut commands: Commands,
    route: Res<ResolvedCameraInputRoute>,
    modes: Query<(
        Option<&OrbitCamInputMode>,
        Option<&FreeCamInputMode>,
        Option<&OrbitCamHomePose>,
        Option<&CameraBasis>,
        Option<&Projection>,
    )>,
    orbit_cameras: Query<Entity, With<OrbitCamInputMode>>,
    free_cameras: Query<Entity, With<FreeCamInputMode>>,
    mut panels: Query<(Entity, &CameraGuidancePanel, &mut Visibility)>,
    preference: Option<Res<FreeCamLookPitchPreference>>,
) {
    let Ok((panel_entity, panel_marker, mut panel_visibility)) = panels.single_mut() else {
        return;
    };
    let Some(camera) = active_cycle_camera(&route, panel_marker, &orbit_cameras, &free_cameras)
    else {
        return;
    };
    let Ok((orbit_mode, free_mode, orbit_home, basis, projection)) = modes.get(camera) else {
        return;
    };
    let Some(mode) = camera_cycle_mode(orbit_mode, free_mode) else {
        return;
    };

    match next_cycle_entry(mode, *panel_visibility) {
        PresetCycleEntry::OrbitPreset(preset) => {
            switch_to_orbit_preset(&mut commands, camera, preset);
            if matches!(*panel_visibility, Visibility::Hidden) {
                commands
                    .entity(panel_entity)
                    .insert(CameraGuidanceRevealPending);
            } else {
                *panel_visibility = Visibility::Inherited;
                commands
                    .entity(panel_entity)
                    .remove::<CameraGuidanceRevealPending>();
            }
        },
        PresetCycleEntry::FreePreset(preset) => {
            let look_pitch = preference.as_ref().map_or_else(
                || FreeCamLookPitchPreference::default().look_pitch(),
                |preference| preference.look_pitch(),
            );
            let free_home = orbit_home.zip(projection).map(|(home, projection)| {
                FreeCamHomePose::from_orbit_home(
                    *home,
                    basis.copied().unwrap_or_default(),
                    projection,
                )
            });
            switch_to_free_preset(&mut commands, camera, preset, look_pitch, free_home);
            *panel_visibility = Visibility::Inherited;
            commands
                .entity(panel_entity)
                .remove::<CameraGuidanceRevealPending>();
        },
        PresetCycleEntry::Off => {
            *panel_visibility = Visibility::Hidden;
            commands
                .entity(panel_entity)
                .remove::<CameraGuidanceRevealPending>();
        },
    }
}

fn active_cycle_camera(
    route: &ResolvedCameraInputRoute,
    panel_marker: &CameraGuidancePanel,
    orbit_cameras: &Query<Entity, With<OrbitCamInputMode>>,
    free_cameras: &Query<Entity, With<FreeCamInputMode>>,
) -> Option<Entity> {
    route
        .routed_camera()
        .or(panel_marker.bound_camera)
        .or_else(|| orbit_cameras.iter().min())
        .or_else(|| free_cameras.iter().min())
}

const fn camera_cycle_mode(
    orbit_mode: Option<&OrbitCamInputMode>,
    free_mode: Option<&FreeCamInputMode>,
) -> Option<CameraCycleMode> {
    match (orbit_mode, free_mode) {
        (Some(OrbitCamInputMode::Preset(preset)), _) => {
            Some(CameraCycleMode::OrbitPreset(preset.kind()))
        },
        (_, Some(FreeCamInputMode::Preset(preset))) => {
            Some(CameraCycleMode::FreePreset(preset.kind()))
        },
        _ => None,
    }
}

fn next_cycle_entry(mode: CameraCycleMode, panel_visibility: Visibility) -> PresetCycleEntry {
    if matches!(panel_visibility, Visibility::Hidden) {
        return PresetCycleEntry::OrbitPreset(OrbitCamPreset::simple_mouse());
    }
    match mode {
        CameraCycleMode::OrbitPreset(OrbitCamPresetKind::SimpleMouse) => {
            PresetCycleEntry::OrbitPreset(OrbitCamPreset::blender_like())
        },
        CameraCycleMode::OrbitPreset(OrbitCamPresetKind::BlenderLike) => {
            PresetCycleEntry::FreePreset(FreeCamPreset::keyboard_mouse())
        },
        CameraCycleMode::FreePreset(FreeCamPresetKind::KeyboardMouse) => PresetCycleEntry::Off,
        CameraCycleMode::FreePreset(_) | CameraCycleMode::OrbitPreset(_) => {
            PresetCycleEntry::OrbitPreset(OrbitCamPreset::simple_mouse())
        },
    }
}

fn switch_to_orbit_preset(commands: &mut Commands, camera: Entity, preset: OrbitCamPreset) {
    let mut orbit_cam = OrbitCam::default();
    orbit_cam::apply_example_orbit_cam_limits(&mut orbit_cam);
    commands
        .entity(camera)
        .remove::<FreeCam>()
        .remove::<FreeCamInput>()
        .remove::<FreeCamInputContext>()
        .remove::<FreeCamInputMode>()
        .remove::<CameraHomePending>()
        .insert((orbit_cam, OrbitCamInputMode::with_preset(preset)));
}

fn switch_to_free_preset(
    commands: &mut Commands,
    camera: Entity,
    preset: FreeCamPreset,
    look_pitch: FreeCamLookPitch,
    home: Option<FreeCamHomePose>,
) {
    let preset = free_settings::preset_with_look_pitch(preset, look_pitch);
    let mut entity = commands.entity(camera);
    entity
        .remove::<OrbitCam>()
        .remove::<OrbitCamInput>()
        .remove::<OrbitCamInputContext>()
        .remove::<OrbitCamInputMode>()
        .remove::<CameraHomePending>()
        .insert((FreeCam::default(), FreeCamInputMode::with_preset(preset)));
    if let Some(home) = home {
        entity.insert(home);
    }
}

#[cfg(test)]
mod tests {
    use bevy::camera::RenderTarget;
    use bevy::ecs::system::RunSystemOnce;
    use bevy::window::WindowRef;
    use bevy_enhanced_input::prelude::Binding;
    use bevy_lagrange::AnimationEnd;
    use bevy_lagrange::AnimationReason;
    use bevy_lagrange::AnimationSource;
    use bevy_lagrange::CameraBasis;
    use bevy_lagrange::CameraHomePending;
    use bevy_lagrange::CameraInputPhase;
    use bevy_lagrange::CameraInputRoutingConfig;
    use bevy_lagrange::Focus;
    use bevy_lagrange::FreeCam;
    use bevy_lagrange::FreeCamHomePose;
    use bevy_lagrange::FreeCamInputMode;
    use bevy_lagrange::FreeCamLookPitch;
    use bevy_lagrange::FreeCamPresetKind;
    use bevy_lagrange::InputGain;
    use bevy_lagrange::LagrangePlugin;
    use bevy_lagrange::LookAngles;
    use bevy_lagrange::OrbitAngles;
    use bevy_lagrange::OrbitCam;
    use bevy_lagrange::OrbitCamBlenderLikePreset;
    use bevy_lagrange::OrbitCamHomePose;
    use bevy_lagrange::OrbitCamInputGain;
    use bevy_lagrange::OrbitCamSimpleMousePreset;
    use bevy_lagrange::Position;
    use bevy_lagrange::Radius;
    use bevy_lagrange::Roll;

    use super::*;
    use crate::Anchor;
    use crate::camera_home;
    use crate::camera_home::CameraHomeConfig;
    use crate::camera_home::HomeTitleBarControl;
    use crate::constants::HOME_KEY;

    const TUNED_SENSITIVITY: f32 = 0.5;

    fn camera_preset_kind(app: &App, camera: Entity) -> Option<OrbitCamPresetKind> {
        match app.world().get::<OrbitCamInputMode>(camera)? {
            OrbitCamInputMode::Preset(preset) => Some(preset.kind()),
            _ => None,
        }
    }

    fn free_camera_preset_kind(app: &App, camera: Entity) -> Option<FreeCamPresetKind> {
        match app.world().get::<FreeCamInputMode>(camera)? {
            FreeCamInputMode::Preset(preset) => Some(preset.kind()),
            _ => None,
        }
    }

    fn free_camera_look_pitch(app: &App, camera: Entity) -> Option<FreeCamLookPitch> {
        match app.world().get::<FreeCamInputMode>(camera)? {
            FreeCamInputMode::Preset(FreeCamPreset::KeyboardMouse(preset)) => {
                Some(preset.look_pitch())
            },
            FreeCamInputMode::Bindings(bindings) => Some(bindings.look_pitch()),
            _ => None,
        }
    }

    fn orbit_home_bindings(app: &App, camera: Entity) -> Result<Vec<Binding>, String> {
        let Some(OrbitCamInputMode::Preset(preset)) = app.world().get::<OrbitCamInputMode>(camera)
        else {
            return Err("expected OrbitCam preset mode".to_string());
        };
        preset
            .to_bindings()
            .map(|bindings| bindings.home().to_vec())
            .map_err(|error| error.to_string())
    }

    fn test_camera_home_config() -> CameraHomeConfig {
        CameraHomeConfig {
            yaw:               0.0,
            pitch:             0.0,
            margin:            0.0,
            anchor:            Anchor::Center,
            offset_px:         Vec2::ZERO,
            title_bar_control: HomeTitleBarControl::Hidden,
        }
    }

    fn tuned_blender_like_preset() -> OrbitCamBlenderLikePreset {
        let disabled = InputGain::DISABLED.0;
        OrbitCamBlenderLikePreset::default()
            .mouse_input_gain(OrbitCamInputGain::uniform(disabled))
            .smooth_scroll_input_gain(OrbitCamInputGain::uniform(disabled))
    }

    #[test]
    fn visible_panel_cycles_simple_to_blender() {
        assert_eq!(
            next_cycle_entry(
                CameraCycleMode::OrbitPreset(OrbitCamPreset::simple_mouse().kind()),
                Visibility::Inherited
            ),
            PresetCycleEntry::OrbitPreset(OrbitCamPreset::blender_like())
        );
    }

    #[test]
    fn visible_panel_cycles_blender_to_free_cam() {
        assert_eq!(
            next_cycle_entry(
                CameraCycleMode::OrbitPreset(OrbitCamPreset::blender_like().kind()),
                Visibility::Inherited
            ),
            PresetCycleEntry::FreePreset(FreeCamPreset::keyboard_mouse())
        );
    }

    #[test]
    fn visible_panel_cycles_free_cam_to_off() {
        assert_eq!(
            next_cycle_entry(
                CameraCycleMode::FreePreset(FreeCamPreset::keyboard_mouse().kind()),
                Visibility::Inherited
            ),
            PresetCycleEntry::Off
        );
    }

    #[test]
    fn hidden_panel_cycles_back_to_simple_mouse() {
        assert_eq!(
            next_cycle_entry(
                CameraCycleMode::OrbitPreset(OrbitCamPreset::blender_like().kind()),
                Visibility::Hidden
            ),
            PresetCycleEntry::OrbitPreset(OrbitCamPreset::simple_mouse())
        );
    }

    #[test]
    fn explicit_cycle_constructs_default_target_preset() {
        let tuned = OrbitCamSimpleMousePreset::default()
            .mouse_input_gain(OrbitCamInputGain::uniform(TUNED_SENSITIVITY));

        assert_eq!(
            next_cycle_entry(
                CameraCycleMode::OrbitPreset(OrbitCamPreset::from(tuned).kind()),
                Visibility::Inherited
            ),
            PresetCycleEntry::OrbitPreset(OrbitCamPreset::blender_like())
        );
    }

    #[test]
    fn tuned_blender_like_cycles_to_free_cam() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, LagrangePlugin));
        app.finish();
        let tuned = tuned_blender_like_preset();
        let camera = app
            .world_mut()
            .spawn((
                OrbitCam::default(),
                Camera::default(),
                RenderTarget::Window(WindowRef::Primary),
                OrbitCamInputMode::with_preset(tuned),
            ))
            .id();
        let panel = app
            .world_mut()
            .spawn((
                CameraGuidancePanel { bound_camera: None },
                Visibility::Inherited,
            ))
            .id();
        app.insert_resource(CameraInputRoutingConfig::explicit(camera));
        app.update();

        app.add_systems(Update, cycle_preset);
        app.update();

        assert_eq!(
            free_camera_preset_kind(&app, camera),
            Some(FreeCamPresetKind::KeyboardMouse)
        );
        assert_eq!(
            free_camera_look_pitch(&app, camera),
            Some(FreeCamLookPitch::Inverted)
        );
        assert_eq!(
            app.world().get::<Visibility>(panel),
            Some(&Visibility::Inherited)
        );
    }

    #[test]
    fn preset_cycle_advances_from_simple_mouse() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, LagrangePlugin));
        app.finish();
        let camera = app
            .world_mut()
            .spawn((
                OrbitCam::default(),
                Camera::default(),
                RenderTarget::Window(WindowRef::Primary),
                OrbitCamInputMode::with_preset(OrbitCamPreset::simple_mouse()),
            ))
            .id();
        app.world_mut().spawn((
            CameraGuidancePanel { bound_camera: None },
            Visibility::Inherited,
        ));
        app.insert_resource(CameraInputRoutingConfig::explicit(camera));
        app.update();

        assert_eq!(
            app.world()
                .resource::<ResolvedCameraInputRoute>()
                .routed_camera(),
            Some(camera)
        );
        assert_eq!(
            camera_preset_kind(&app, camera),
            Some(OrbitCamPresetKind::SimpleMouse)
        );

        app.add_systems(Update, cycle_preset);
        app.update();
        assert_eq!(
            camera_preset_kind(&app, camera),
            Some(OrbitCamPresetKind::BlenderLike)
        );

        app.update();
        assert_eq!(
            free_camera_preset_kind(&app, camera),
            Some(FreeCamPresetKind::KeyboardMouse)
        );
    }

    #[test]
    fn cycled_orbit_preset_is_filled_when_camera_home_is_enabled() -> Result<(), String> {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, LagrangePlugin))
            .insert_resource(test_camera_home_config())
            .add_systems(
                PreUpdate,
                camera_home::fill_camera_home_presets.before(CameraInputPhase::PreInput),
            );
        app.finish();
        let camera = app
            .world_mut()
            .spawn((
                OrbitCam::default(),
                Camera::default(),
                RenderTarget::Window(WindowRef::Primary),
                OrbitCamInputMode::with_preset(OrbitCamPreset::simple_mouse()),
            ))
            .id();
        app.world_mut().spawn((
            CameraGuidancePanel { bound_camera: None },
            Visibility::Inherited,
        ));
        app.insert_resource(CameraInputRoutingConfig::explicit(camera));
        app.update();

        app.world_mut()
            .run_system_once(cycle_preset)
            .map_err(|error| error.to_string())?;
        app.update();

        assert_eq!(
            camera_preset_kind(&app, camera),
            Some(OrbitCamPresetKind::BlenderLike)
        );
        assert_eq!(
            orbit_home_bindings(&app, camera)?,
            vec![Binding::from(HOME_KEY)]
        );
        Ok(())
    }

    #[test]
    fn cycling_to_free_cam_keeps_orbit_home_as_fixed_free_home() -> Result<(), String> {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, LagrangePlugin));
        app.finish();
        let orbit_home = OrbitCamHomePose {
            orbit: OrbitAngles {
                yaw:   0.25,
                pitch: -0.125,
            },
            pan:   Focus(Vec3::new(1.0, 2.0, 3.0)),
            zoom:  Radius(7.0),
        };
        let camera = app
            .world_mut()
            .spawn((
                OrbitCam::default(),
                Camera::default(),
                RenderTarget::Window(WindowRef::Primary),
                OrbitCamInputMode::with_preset(OrbitCamPreset::blender_like()),
                orbit_home,
            ))
            .id();
        app.world_mut().spawn((
            CameraGuidancePanel { bound_camera: None },
            Visibility::Inherited,
        ));
        app.insert_resource(CameraInputRoutingConfig::explicit(camera));
        app.update();

        app.world_mut()
            .run_system_once(cycle_preset)
            .map_err(|error| error.to_string())?;
        app.update();

        let projection = app
            .world()
            .get::<Projection>(camera)
            .ok_or("camera should have a projection")?;
        let expected_home =
            FreeCamHomePose::from_orbit_home(orbit_home, CameraBasis::Y_UP, projection);
        assert_eq!(
            free_camera_preset_kind(&app, camera),
            Some(FreeCamPresetKind::KeyboardMouse)
        );
        assert_eq!(
            app.world().get::<FreeCamHomePose>(camera),
            Some(&expected_home)
        );
        assert!(app.world().get::<CameraHomePending>(camera).is_none());

        let target = app.world_mut().spawn_empty().id();
        {
            let mut free_cam = app
                .world_mut()
                .get_mut::<FreeCam>(camera)
                .ok_or("camera should have switched to FreeCam")?;
            free_cam
                .translate
                .snap_to(Position(Vec3::new(50.0, 60.0, 70.0)));
            free_cam.look.snap_to(LookAngles {
                yaw:   1.5,
                pitch: 0.75,
            });
            free_cam.roll.snap_to(Roll(0.5));
        }
        app.world_mut().trigger(AnimationEnd {
            camera,
            source: AnimationSource::ZoomToFit,
            target: Some(target),
            reason: AnimationReason::Completed,
        });
        app.world_mut().flush();

        assert_eq!(
            app.world().get::<FreeCamHomePose>(camera),
            Some(&expected_home)
        );
        Ok(())
    }
}
