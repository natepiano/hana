use bevy::prelude::*;
use bevy_enhanced_input::prelude::EnhancedInputPlugin;
use bevy_enhanced_input::prelude::InputContextAppExt;

use crate::input::OrbitCamInputContext;

pub(crate) struct LagrangeEnhancedInputPlugin;

impl Plugin for LagrangeEnhancedInputPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<EnhancedInputPlugin>() {
            app.add_plugins(EnhancedInputPlugin);
        }
        app.add_input_context::<OrbitCamInputContext>();
    }
}
