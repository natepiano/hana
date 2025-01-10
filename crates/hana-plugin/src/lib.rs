use bevy::prelude::*;

#[derive(Debug, thiserror::Error)]
pub enum PluginError {
    #[error("Plugin creation failed: {0}")]
    CreationError(String),
}

/// Trait that must be implemented by Hana visualization plugins
pub trait HanaPlugin: Send + Sync {
    /// Returns a Bevy plugin that will be added to the Hana application
    fn create_bevy_plugin(&self) -> Box<dyn Plugin>;
}

/// Factory trait for creating plugin instances
pub trait PluginFactory {
    fn create() -> Box<dyn HanaPlugin>;
}
