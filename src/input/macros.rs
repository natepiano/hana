/// Generates a `bevy_enhanced_input` `InputAction` struct.
///
/// # Examples
///
/// ```ignore
/// use bevy_kana::action;
///
/// action!(CameraHome);
/// ```
///
/// Expands to:
///
/// ```ignore
/// #[derive(InputAction)]
/// #[action_output(bool)]
/// pub struct CameraHome;
/// ```
#[macro_export]
macro_rules! action {
    ($(#[$meta:meta])* $action:ident) => {
        $(#[$meta])*
        #[derive(InputAction)]
        #[action_output(bool)]
        pub struct $action;
    };
}

/// Generates a Bevy `Event` struct with `Reflect` support.
///
/// Supports both unit events and events with payload fields.
/// Events generated this way are compatible with the Bevy Remote Protocol's
/// `world.trigger_event`.
///
/// # Examples
///
/// Unit event:
///
/// ```ignore
/// use bevy_kana::event;
///
/// event!(PauseEvent);
/// ```
///
/// Payload event:
///
/// ```ignore
/// use bevy_kana::event;
///
/// event!(ZoomToTarget { entity: Entity });
/// ```
#[macro_export]
macro_rules! event {
    ($(#[$meta:meta])* $event:ident) => {
        $(#[$meta])*
        #[derive(Event, Reflect, Default)]
        #[reflect(Event)]
        pub struct $event;
    };
    ($(#[$meta:meta])* $event:ident { $($field:ident : $ty:ty),+ $(,)? }) => {
        $(#[$meta])*
        #[derive(Event, Reflect)]
        #[reflect(Event)]
        pub struct $event {
            $(pub $field: $ty,)+
        }
    };
}

/// Wires an input action to a command function through an intermediate event.
///
/// Registers two observers:
/// 1. `On<Start<Action>>` triggers the event
/// 2. `On<Event>` runs the command via `run_system_cached`
///
/// The intermediate event decouples keyboard input from command execution,
/// allowing the same command to be invoked by a keybinding, programmatically
/// via `commands.trigger(MyEvent)`, or through the Bevy Remote Protocol's
/// `world.trigger_event`.
///
/// Use with [`action!`] and [`event!`] to generate the action and event structs.
///
/// # Examples
///
/// ```ignore
/// use bevy_kana::action;
/// use bevy_kana::bind_action_system;
/// use bevy_kana::event;
///
/// action!(PauseToggle);
/// event!(PauseEvent);
///
/// fn setup(app: &mut App) {
///     bind_action_system!(app, PauseToggle, PauseEvent, pause_command);
/// }
/// ```
#[macro_export]
macro_rules! bind_action_system {
    ($app:expr, $action:ty, $event:ty, $command:path) => {{
        $app.add_observer(
            |_start: On<bevy_enhanced_input::action::events::Start<$action>>,
             mut commands: Commands| {
                commands.trigger(<$event>::default());
            },
        )
        .add_observer(|_event: On<$event>, mut commands: Commands| {
            commands.run_system_cached($command);
        })
    }};
}

#[cfg(test)]
mod tests {
    use bevy::prelude::*;

    event!(TestEvent);
    event!(TestPayloadEvent { value: u32 });

    #[test]
    fn payload_event_fields() {
        let event = TestPayloadEvent { value: 42 };
        assert_eq!(event.value, 42);
    }

    #[test]
    fn unit_event_defaults() {
        let event = TestEvent;
        assert_eq!(std::mem::size_of_val(&event), 0);
    }
}
