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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum ScreenSpacePanelsVisibility {
    #[default]
    Shown,
    Hidden,
}

impl ScreenSpacePanelsVisibility {
    const fn toggled(self) -> Self {
        match self {
            Self::Shown => Self::Hidden,
            Self::Hidden => Self::Shown,
        }
    }
}

#[derive(Resource, Default)]
struct ScreenSpacePanelsEnabled(ScreenSpacePanelsVisibility);

#[derive(Component)]
struct ScreenSpacePanelsContext;

action!(ToggleScreenSpacePanels);
event!(ToggleScreenSpacePanelsEvent);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ScreenSpaceCameraState {
    Active,
    Inactive,
}

impl From<bool> for ScreenSpaceCameraState {
    fn from(is_active: bool) -> Self {
        if is_active {
            Self::Active
        } else {
            Self::Inactive
        }
    }
}

impl From<ScreenSpaceCameraState> for bool {
    fn from(state: ScreenSpaceCameraState) -> Self {
        matches!(state, ScreenSpaceCameraState::Active)
    }
}

#[derive(Component)]
struct ScreenSpaceCameraRestore {
    state: ScreenSpaceCameraState,
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
    enabled.0 = enabled.0.toggled();
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
        if enabled.0 == ScreenSpacePanelsVisibility::Shown {
            if let Some(restore) = restore {
                camera.is_active = restore.state.into();
                commands.entity(entity).remove::<ScreenSpaceCameraRestore>();
            }
        } else {
            if restore.is_none() {
                commands.entity(entity).insert(ScreenSpaceCameraRestore {
                    state: camera.is_active.into(),
                });
            }
            camera.is_active = false;
        }
    }

    for (entity, mut light, restore) in &mut lights {
        if enabled.0 == ScreenSpacePanelsVisibility::Shown {
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
