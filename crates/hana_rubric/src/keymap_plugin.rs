//! Keymap plugin configuration and system ordering.

use bevy::app::App;
use bevy::app::Plugin;
use bevy::ecs::schedule::IntoScheduleConfigs;
use bevy::ecs::schedule::SystemSet;
use bevy::input::InputSystems;
use bevy::prelude::PreUpdate;
use bevy::prelude::Resource;
use bevy::prelude::States;
use bevy_enhanced_input::prelude::EnhancedInputSystems;

use crate::ActiveCondition;
use crate::KeymapContext;
use crate::condition::ConditionRegistry;
use crate::condition::ResourceContextPlugin;
use crate::condition::StateContextPlugin;

/// Registers the application's keymap context enum.
///
/// Add this plugin directly for global-only bindings, or construct it with [`Self::new`] and
/// choose a context source with [`Self::for_context`] or [`Self::for_state_context`].
pub struct KeymapPlugin;

impl KeymapPlugin {
    /// Starts a keymap plugin configuration.
    #[must_use]
    pub const fn new() -> Self { Self }

    /// Registers a resource-backed context enum and synchronizes its active condition.
    #[must_use]
    pub fn for_context<C>(self) -> impl Plugin
    where
        C: KeymapContext + Resource,
    {
        ResourceContextPlugin::<C>::new()
    }

    /// Registers a state-backed context enum and synchronizes its active condition.
    #[must_use]
    pub fn for_state_context<C>(self) -> impl Plugin
    where
        C: KeymapContext + States,
    {
        StateContextPlugin::<C>::new()
    }

    pub(crate) fn install_runtime(app: &mut App) {
        if app.world().contains_resource::<KeymapRuntimeInstalled>() {
            return;
        }

        app.init_resource::<KeymapRuntimeInstalled>()
            .init_resource::<ConditionRegistry>()
            .init_resource::<ActiveCondition>()
            .configure_sets(
                PreUpdate,
                (KeymapSystems::UpdateActiveCondition, KeymapSystems::Route).chain(),
            )
            .add_systems(
                PreUpdate,
                crate::keymap::route_input
                    .in_set(KeymapSystems::Route)
                    .after(InputSystems)
                    .before(EnhancedInputSystems::Update),
            );
        app.world_mut()
            .resource_mut::<ActiveCondition>()
            .enable_global();
    }
}

impl Default for KeymapPlugin {
    fn default() -> Self { Self::new() }
}

impl Plugin for KeymapPlugin {
    fn build(&self, app: &mut App) { Self::install_runtime(app); }
}

#[derive(Default, Resource)]
struct KeymapRuntimeInstalled;

/// Keymap-system ordering points exposed to application context derivation systems.
#[derive(Clone, Debug, Eq, Hash, PartialEq, SystemSet)]
pub enum KeymapSystems {
    /// Synchronizes the application's changed context into [`crate::ActiveCondition`].
    UpdateActiveCondition,
    /// Routes input through the compiled keymap.
    Route,
}
