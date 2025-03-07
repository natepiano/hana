use bevy::prelude::*;
use leafwing_input_manager::prelude::*;

use super::Action;

pub struct ActionPlugin;

impl Plugin for ActionPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(InputManagerPlugin::<Action>::default())
            .init_resource::<ActionState<Action>>()
            .insert_resource(Action::input_map());
    }
}
