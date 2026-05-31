//! Capability: Ctrl+Shift+L toggles screen-space panel visibility.
//!
//! Wired through `bevy_enhanced_input` using the `bevy_kana` macros, modeled on
//! [`crate::restart`].

use bevy::prelude::*;
use bevy_diegetic::ScreenSpaceCamera;
use bevy_diegetic::ScreenSpaceLight;
use bevy_enhanced_input::prelude::*;
use bevy_kana::action;
use bevy_kana::bind_action_system;
use bevy_kana::event;

use crate::ensure_plugin;

#[derive(Resource)]
struct ScreenSpacePanelsEnabled(bool);

impl Default for ScreenSpacePanelsEnabled {
    fn default() -> Self { Self(true) }
}

#[derive(Component)]
struct ScreenSpacePanelsContext;

action!(ToggleScreenSpacePanels);
event!(ToggleScreenSpacePanelsEvent);

#[derive(Component)]
struct ScreenSpaceCameraRestore {
    is_active: bool,
}

#[derive(Component)]
struct ScreenSpaceLightRestore {
    illuminance: f32,
}

pub(crate) fn install(app: &mut App) {
    ensure_plugin(app, EnhancedInputPlugin);
    app.init_resource::<ScreenSpacePanelsEnabled>();
    app.add_input_context::<ScreenSpacePanelsContext>();
    app.add_systems(Startup, spawn_toggle_action);
    app.add_systems(Update, apply_screen_space_panels);
    bind_action_system!(
        app,
        ToggleScreenSpacePanels,
        ToggleScreenSpacePanelsEvent,
        toggle_screen_space_panels
    );
}

fn spawn_toggle_action(mut commands: Commands) {
    commands.spawn((
        ScreenSpacePanelsContext,
        actions!(ScreenSpacePanelsContext[
            (
                Action::<ToggleScreenSpacePanels>::new(),
                bindings![KeyCode::KeyL.with_mod_keys(ModKeys::CONTROL | ModKeys::SHIFT)],
            ),
        ]),
    ));
}

fn toggle_screen_space_panels(mut enabled: ResMut<ScreenSpacePanelsEnabled>) {
    enabled.0 = !enabled.0;
}

fn apply_screen_space_panels(
    enabled: Res<ScreenSpacePanelsEnabled>,
    mut commands: Commands,
    mut cameras: Query<
        (Entity, &mut Camera, Option<&ScreenSpaceCameraRestore>),
        With<ScreenSpaceCamera>,
    >,
    mut lights: Query<
        (
            Entity,
            &mut DirectionalLight,
            Option<&ScreenSpaceLightRestore>,
        ),
        With<ScreenSpaceLight>,
    >,
) {
    for (entity, mut camera, restore) in &mut cameras {
        if enabled.0 {
            if let Some(restore) = restore {
                camera.is_active = restore.is_active;
                commands.entity(entity).remove::<ScreenSpaceCameraRestore>();
            }
        } else {
            if restore.is_none() {
                commands.entity(entity).insert(ScreenSpaceCameraRestore {
                    is_active: camera.is_active,
                });
            }
            camera.is_active = false;
        }
    }

    for (entity, mut light, restore) in &mut lights {
        if enabled.0 {
            if let Some(restore) = restore {
                light.illuminance = restore.illuminance;
                commands.entity(entity).remove::<ScreenSpaceLightRestore>();
            }
        } else {
            if restore.is_none() {
                commands.entity(entity).insert(ScreenSpaceLightRestore {
                    illuminance: light.illuminance,
                });
            }
            light.illuminance = 0.0;
        }
    }
}
