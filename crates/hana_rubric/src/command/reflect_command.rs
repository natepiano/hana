use bevy::prelude::Reflect;
use bevy::reflect::FromType;

use super::Capability;

/// Metadata declaration for a semantic event that can be invoked from a keymap.
///
/// `command!` implements `KeymapCommand` for generated event types. Downstream crates can also
/// implement this trait for their own reflected semantic events.
pub trait KeymapCommand: Reflect {
    /// Command identifier text that satisfies [`CommandId::is_valid`](super::CommandId::is_valid).
    const ID: &'static str;
    /// Palette title text for this command.
    const TITLE: &'static str;
    /// Palette description text for this command.
    const DESCRIPTION: &'static str;
    /// Keymap binding capability declared for this command.
    const CAPABILITY: Capability;
}

/// Reflection type data retained for each [`KeymapCommand`] event registration.
#[derive(Clone, Debug)]
pub struct ReflectKeymapCommand {
    /// Validated command identifier text.
    pub id:          &'static str,
    /// Palette title text.
    pub title:       &'static str,
    /// Palette description text.
    pub description: &'static str,
    /// Keymap binding capability.
    pub capability:  Capability,
}

impl<T: KeymapCommand> FromType<T> for ReflectKeymapCommand {
    fn from_type() -> Self {
        Self {
            id:          T::ID,
            title:       T::TITLE,
            description: T::DESCRIPTION,
            capability:  T::CAPABILITY,
        }
    }
}

/// Declares an input action, semantic event, and reflected keymap-command metadata.
///
/// The invocation module must import `InputAction`, `Event`, `Reflect`, `ReflectEvent`, and
/// `ReflectKeymapCommand` unqualified. The wrapped `action!` and `event!` macros emit bare derive
/// names, and the `Reflect` derive resolves `ReflectEvent` and `ReflectKeymapCommand` at the
/// invocation site.
///
/// ```ignore
/// use bevy::prelude::Event;
/// use bevy::prelude::Reflect;
/// use bevy::prelude::ReflectEvent;
/// use bevy_enhanced_input::prelude::InputAction;
/// use hana_rubric::command;
/// use hana_rubric::ReflectKeymapCommand;
///
/// command! {
///     action:      CameraHome,
///     event:       CameraHomeEvent,
///     id:          "camera::home",
///     title:       "Reset Camera to Home",
///     description: "Return the camera to its default framing.",
/// }
/// ```
///
/// The expansion reaches `action!` and `event!` through `hana_rubric`'s `$crate` re-exports, so
/// consumers need only `hana_rubric` rather than a direct `bevy_kana` dependency.
#[macro_export]
macro_rules! command {
    (
        action: $action:ident,
        event: $event:ident,
        id: $id:literal,
        title: $title:literal,
        description: $description:literal,
        capability: $capability:ident $(,)?
    ) => {
        $crate::command!(
            @declare
            $action,
            $event,
            $id,
            $title,
            $description,
            $crate::Capability::$capability
        );
    };
    (
        action: $action:ident,
        event: $event:ident,
        id: $id:literal,
        title: $title:literal,
        description: $description:literal $(,)?
    ) => {
        $crate::command!(
            @declare
            $action,
            $event,
            $id,
            $title,
            $description,
            $crate::Capability::OneShot
        );
    };
    (@declare $action:ident, $event:ident, $id:literal, $title:literal, $description:literal, $capability:path) => {
        $crate::action!($action);
        $crate::event!($event, reflect: KeymapCommand);

        const _: () = assert!($crate::CommandId::is_valid($id));

        impl $crate::KeymapCommand for $event {
            const ID:          &'static str = $id;
            const TITLE:       &'static str = $title;
            const DESCRIPTION: &'static str = $description;
            const CAPABILITY:  $crate::Capability = $capability;
        }
    };
}

#[cfg(test)]
mod tests {
    use std::any::TypeId;

    use bevy::prelude::Event;
    use bevy::prelude::Reflect;
    use bevy::prelude::ReflectEvent;
    use bevy::reflect::GetTypeRegistration;
    use bevy::reflect::TypeRegistry;
    use bevy_enhanced_input::prelude::InputAction;

    use super::Capability;
    use super::ReflectKeymapCommand;

    crate::command! {
        action:      CameraHome,
        event:       CameraHomeEvent,
        id:          "camera::home",
        title:       "Reset Camera to Home",
        description: "Return the camera to its default framing.",
    }

    crate::command! {
        action:      CameraFocusHeld,
        event:       CameraFocusHeldEvent,
        id:          "camera::focus_held",
        title:       "Focus Camera",
        description: "Focus the camera while the key is held.",
        capability: Held,
    }

    crate::command! {
        action:      RecoveryPalette,
        event:       RecoveryPaletteEvent,
        id:          "recovery::palette",
        title:       "Open Recovery Palette",
        description: "Open the permanently registered recovery palette.",
        capability: Unremappable,
    }

    #[test]
    fn command_macro_declares_input_action_and_event() {
        assert_input_action::<CameraHome>();
        assert_event::<CameraHomeEvent>();
    }

    #[test]
    fn command_metadata_registers_declared_text_and_default_capability() {
        let mut type_registry = TypeRegistry::default();
        type_registry.register::<CameraHomeEvent>();

        let command =
            type_registry.get_type_data::<ReflectKeymapCommand>(TypeId::of::<CameraHomeEvent>());

        assert_eq!(command.map(|command| command.id), Some("camera::home"));
        assert_eq!(
            command.map(|command| command.title),
            Some("Reset Camera to Home")
        );
        assert_eq!(
            command.map(|command| command.description),
            Some("Return the camera to its default framing.")
        );
        assert_eq!(
            command.map(|command| command.capability),
            Some(Capability::OneShot)
        );
    }

    #[test]
    fn held_capability_registers_with_the_event() {
        assert_eq!(
            registered_capability::<CameraFocusHeldEvent>(),
            Some(Capability::Held)
        );
    }

    #[test]
    fn unremappable_capability_registers_with_the_event() {
        assert_eq!(
            registered_capability::<RecoveryPaletteEvent>(),
            Some(Capability::Unremappable)
        );
    }

    fn assert_input_action<T: InputAction>() {}

    fn assert_event<T: Event>() {}

    fn registered_capability<T: GetTypeRegistration>() -> Option<Capability> {
        let mut type_registry = TypeRegistry::default();
        type_registry.register::<T>();
        type_registry
            .get_type_data::<ReflectKeymapCommand>(TypeId::of::<T>())
            .map(|command| command.capability)
    }
}
