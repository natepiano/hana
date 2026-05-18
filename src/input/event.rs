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

#[cfg(test)]
mod tests {
    use std::mem::size_of_val;

    use bevy::prelude::*;

    // expectations
    const EMPTY_EVENT_SIZE: usize = 0;
    const PAYLOAD_VALUE: u32 = 42;

    crate::event!(TestEvent);
    crate::event!(TestPayloadEvent { value: u32 });

    #[test]
    fn payload_event_fields() {
        let event = TestPayloadEvent {
            value: PAYLOAD_VALUE,
        };
        assert_eq!(event.value, PAYLOAD_VALUE);
    }

    #[test]
    fn unit_event_defaults() {
        let event = TestEvent;
        assert_eq!(size_of_val(&event), EMPTY_EVENT_SIZE);
    }
}
