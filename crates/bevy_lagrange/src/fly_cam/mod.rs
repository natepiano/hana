//! Free-flight `FlyCam` camera kind.

use bevy::prelude::*;
use bevy_enhanced_input::prelude::InputContextAppExt;

use crate::input::FlyCamInputContext;

/// Registers `FlyCam` systems and its enhanced-input context.
pub(super) struct FlyCamPlugin;

impl Plugin for FlyCamPlugin {
    fn build(&self, app: &mut App) { app.add_input_context::<FlyCamInputContext>(); }
}

/// Tags an entity as a free-flight camera that can turn and translate freely.
#[derive(Component, Reflect, Default, Debug)]
#[reflect(Component)]
#[require(FlyCamInputContext)]
pub struct FlyCam {}
