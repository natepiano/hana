//! Capability: Shift+C cycles the routed `OrbitCam` between the `SimpleMouse`
//! and `BlenderLike` presets.
//!
//! Wired through `bevy_enhanced_input` using the `bevy_kana` macros
//! (`action!`, `event!`, `bind_action_system!`), modeled on [`crate::restart`].
//! Installed alongside the camera control panel, so every panel example gains
//! the switch. The bound system only acts when the routed camera is in
//! [`OrbitCamInputMode::Preset`], leaving `Bindings`/`Manual` cameras untouched.

use bevy::prelude::*;
use bevy_enhanced_input::prelude::*;
use bevy_kana::action;
use bevy_kana::bind_action_system;
use bevy_kana::event;
use bevy_lagrange::OrbitCamInputMode;
use bevy_lagrange::OrbitCamPreset;
use bevy_lagrange::ResolvedOrbitCamInputRoute;

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

fn cycle_preset(route: Res<ResolvedOrbitCamInputRoute>, mut modes: Query<&mut OrbitCamInputMode>) {
    let Some(camera) = route.routed_camera() else {
        return;
    };
    let Ok(mut mode) = modes.get_mut(camera) else {
        return;
    };
    let OrbitCamInputMode::Preset(preset) = *mode else {
        return;
    };
    *mode = OrbitCamInputMode::Preset(next_preset(preset));
}

const fn next_preset(preset: OrbitCamPreset) -> OrbitCamPreset {
    match preset {
        OrbitCamPreset::SimpleMouse => OrbitCamPreset::BlenderLike,
        _ => OrbitCamPreset::SimpleMouse,
    }
}
