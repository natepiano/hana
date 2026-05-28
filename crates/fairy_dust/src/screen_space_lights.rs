//! Capability: Ctrl+Shift+L toggles screen-space panel visibility.

use bevy::prelude::*;
use bevy_diegetic::ScreenSpaceCamera;
use bevy_diegetic::ScreenSpaceLight;

#[derive(Resource)]
struct ScreenSpacePanelsEnabled(bool);

impl Default for ScreenSpacePanelsEnabled {
    fn default() -> Self { Self(true) }
}

#[derive(Component)]
struct ScreenSpaceCameraRestore {
    is_active: bool,
}

#[derive(Component)]
struct ScreenSpaceLightRestore {
    illuminance: f32,
}

pub(crate) fn install(app: &mut App) {
    app.init_resource::<ScreenSpacePanelsEnabled>().add_systems(
        Update,
        (toggle_screen_space_panels, apply_screen_space_panels).chain(),
    );
}

fn toggle_screen_space_panels(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut enabled: ResMut<ScreenSpacePanelsEnabled>,
) {
    let ctrl = keyboard.any_pressed([KeyCode::ControlLeft, KeyCode::ControlRight]);
    let shift = keyboard.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]);
    if ctrl && shift && keyboard.just_pressed(KeyCode::KeyL) {
        enabled.0 = !enabled.0;
    }
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
