use bevy::prelude::*;
use leafwing_input_manager::prelude::*;

/// ToggleActive allows us to do something cool - we can use it like the bevy
/// input_toggle_active but it works with leafwing_input_manager input_map
/// entries so we can have simple syntax for toggling systems as a run condition
/// as follows:
///
/// ```
/// .add_systems(Update, my_system.run_if(toggle_active(false, Action::AABBs)))
/// ```
/// cool, huh? it uses a Local<ToggleState> to keep track of, well, the toggle state
#[allow(dead_code)]
pub fn toggle_active<A: Actionlike>(
    default: bool,
    action: A,
) -> impl Fn(Res<ActionState<A>>, Local<ToggleState>) -> bool {
    move |action_state: Res<ActionState<A>>, mut state: Local<ToggleState>| {
        if action_state.just_pressed(&action) {
            state.state = !state.state;
        }

        if state.state {
            !default
        } else {
            default
        }
    }
}

#[allow(dead_code)]
#[derive(Default)]
pub struct ToggleState {
    pub state: bool,
}

/// use just_pressed as a run condition
/// for systems that are invoked by a just_pressed action
///
/// ```
/// .add_systems(Update, my_system.run_if(just_pressed(Action::MyAction)))
/// ```
///
/// normally we would do something like this
/// query for  action_state: Res<ActionState<PlayerAction>> in the fn signature
///
/// and then check it in the code:
///  if action_state.pressed(&PlayerAction::Jump) {
///   println!("Jumping!");
/// }
///
/// doing it in the run condition is probably not technically faster but it's more obvious that
/// the ActionState is gating the entire system and you don't have to take up an argument
/// and surround your code with an if statement in the simple case where the system
/// is only invoked by a key press
pub fn just_pressed<A: Actionlike>(action: A) -> impl Fn(Res<ActionState<A>>) -> bool {
    move |action_state: Res<ActionState<A>>| action_state.just_pressed(&action)
}
