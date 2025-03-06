use bevy::prelude::*;
use leafwing_input_manager::prelude::*;
use strum::{EnumIter, IntoEnumIterator};

pub struct ActionPlugin;

impl Plugin for ActionPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(InputManagerPlugin::<Action>::default())
            .init_resource::<ActionState<Action>>()
            .insert_resource(Action::global_input_map());
    }
}

/// An action is a Hana behavior that can be controlled by the user via key commands
///
/// in a bevy system, you can ask for the Action
/// ```rust, ignore
/// fn my_system(user_input: Res<ActionState<Action>>) {
///    if user_input.pressed(&Action::Debug) {
///   }
/// }
/// ```
#[derive(Actionlike, EnumIter, Reflect, PartialEq, Eq, Clone, Copy, Hash, Debug)]
pub enum Action {
    Ping,
    Start,
    Shutdown,
    ToggleText,
}

impl Action {
    pub fn global_input_map() -> InputMap<Self> {
        fn insert_shift_input(
            input_map: InputMap<Action>,
            action: Action,
            key: KeyCode,
        ) -> InputMap<Action> {
            input_map.with_one_to_many(
                action,
                [
                    ButtonlikeChord::new([KeyCode::ShiftLeft]).with(key),
                    ButtonlikeChord::new([KeyCode::ShiftRight]).with(key),
                ],
            )
        }

        // while fold accumulates each pass - we just do an insert each time as a
        // statement and then return the map at the end of each iteration so the
        // accumulation works
        Self::iter().fold(InputMap::default(), |input_map, action| match action {
            Self::Ping => input_map.with(action, KeyCode::KeyP),
            Self::Start => input_map.with(action, KeyCode::F1),
            Self::Shutdown => insert_shift_input(input_map, action, KeyCode::F1),
            Self::ToggleText => input_map.with(action, KeyCode::F2),
        })
    }
}
