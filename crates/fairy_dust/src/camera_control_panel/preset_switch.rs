//! Capability: Shift+C cycles the routed `OrbitCam` through `SimpleMouse`,
//! `BlenderLike`, and a hidden camera-control panel state.
//!
//! Wired through `bevy_enhanced_input` using the `bevy_kana` macros
//! (`action!`, `event!`, `bind_action_system!`), modeled on [`crate::restart`].
//! Installed alongside the camera control panel, so every panel example gains
//! the switch. The bound system only acts when the routed camera is in
//! [`OrbitCamInputMode::Preset`], leaving `Bindings`/`Manual` cameras untouched.
//! The hidden panel state leaves the camera on its current preset; the next
//! Shift+C restores the panel and returns to `SimpleMouse`.

use bevy::prelude::*;
use bevy_enhanced_input::prelude::*;
use bevy_kana::action;
use bevy_kana::bind_action_system;
use bevy_kana::event;
use bevy_lagrange::OrbitCamInputMode;
use bevy_lagrange::OrbitCamPreset;
use bevy_lagrange::OrbitCamPresetKind;
use bevy_lagrange::ResolvedOrbitCamInputRoute;

use super::CameraGuidancePanel;
use super::CameraGuidanceRevealPending;
use crate::ensure_plugin;

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
    Preset(OrbitCamPreset),
    Off,
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
    route: Res<ResolvedOrbitCamInputRoute>,
    mut modes: Query<&mut OrbitCamInputMode>,
    mut panels: Query<(Entity, &mut Visibility), With<CameraGuidancePanel>>,
) {
    let Some(camera) = route.routed_camera() else {
        return;
    };
    let Ok(mut mode) = modes.get_mut(camera) else {
        return;
    };
    let OrbitCamInputMode::Preset(preset) = &*mode else {
        return;
    };
    let Ok((panel_entity, mut panel_visibility)) = panels.single_mut() else {
        return;
    };

    match next_cycle_entry(preset.kind(), *panel_visibility) {
        PresetCycleEntry::Preset(preset) => {
            *mode = OrbitCamInputMode::with_preset(preset);
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
        PresetCycleEntry::Off => {
            *panel_visibility = Visibility::Hidden;
            commands
                .entity(panel_entity)
                .remove::<CameraGuidanceRevealPending>();
        },
    }
}

fn next_cycle_entry(
    preset_kind: OrbitCamPresetKind,
    panel_visibility: Visibility,
) -> PresetCycleEntry {
    if matches!(panel_visibility, Visibility::Hidden) {
        return PresetCycleEntry::Preset(OrbitCamPreset::simple_mouse());
    }
    match preset_kind {
        OrbitCamPresetKind::SimpleMouse => PresetCycleEntry::Preset(OrbitCamPreset::blender_like()),
        OrbitCamPresetKind::BlenderLike => PresetCycleEntry::Off,
        _ => PresetCycleEntry::Preset(OrbitCamPreset::simple_mouse()),
    }
}

#[cfg(test)]
mod tests {
    use bevy::camera::RenderTarget;
    use bevy::window::WindowRef;
    use bevy_lagrange::CameraInputRoutingConfig;
    use bevy_lagrange::LagrangePlugin;
    use bevy_lagrange::OrbitCam;
    use bevy_lagrange::OrbitCamInputModeDescriptor;
    use bevy_lagrange::OrbitCamInputModeDraft;

    use super::*;

    fn camera_preset_kind(app: &App, camera: Entity) -> Option<OrbitCamPresetKind> {
        match app.world().get::<OrbitCamInputMode>(camera)? {
            OrbitCamInputMode::Preset(preset) => Some(preset.kind()),
            _ => None,
        }
    }

    #[test]
    fn visible_panel_cycles_simple_to_blender() {
        assert_eq!(
            next_cycle_entry(OrbitCamPreset::simple_mouse().kind(), Visibility::Inherited),
            PresetCycleEntry::Preset(OrbitCamPreset::blender_like())
        );
    }

    #[test]
    fn visible_panel_cycles_blender_to_off() {
        assert_eq!(
            next_cycle_entry(OrbitCamPreset::blender_like().kind(), Visibility::Inherited),
            PresetCycleEntry::Off
        );
    }

    #[test]
    fn hidden_panel_cycles_back_to_simple_mouse() {
        assert_eq!(
            next_cycle_entry(OrbitCamPreset::blender_like().kind(), Visibility::Hidden),
            PresetCycleEntry::Preset(OrbitCamPreset::simple_mouse())
        );
    }

    #[test]
    fn preset_cycle_does_not_reapply_stale_descriptor() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, LagrangePlugin));
        app.finish();
        let camera = app
            .world_mut()
            .spawn((
                OrbitCam::default(),
                Camera::default(),
                RenderTarget::Window(WindowRef::Primary),
                OrbitCamInputModeDescriptor {
                    mode: OrbitCamInputModeDraft::from_preset(&OrbitCamPreset::simple_mouse()),
                },
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
                .resource::<ResolvedOrbitCamInputRoute>()
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
            camera_preset_kind(&app, camera),
            Some(OrbitCamPresetKind::BlenderLike)
        );
    }
}
