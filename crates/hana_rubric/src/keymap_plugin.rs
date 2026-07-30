//! Keymap plugin configuration and system ordering.

use bevy::app::Plugin;
use bevy::ecs::schedule::SystemSet;
use bevy::prelude::Resource;
use bevy::prelude::States;

use crate::KeymapContext;
use crate::condition::ResourceContextPlugin;
use crate::condition::StateContextPlugin;

/// Registers the application's keymap context enum.
///
/// Construct this with [`Self::new`] and choose exactly one context source with
/// [`Self::for_context`] or [`Self::for_state_context`].
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
}

impl Default for KeymapPlugin {
    fn default() -> Self { Self::new() }
}

/// Keymap-system ordering points exposed to application context derivation systems.
#[derive(Clone, Debug, Eq, Hash, PartialEq, SystemSet)]
pub enum KeymapSystems {
    /// Synchronizes the application's changed context into [`crate::ActiveCondition`].
    UpdateActiveCondition,
    /// Routes input through the compiled keymap.
    Route,
}
