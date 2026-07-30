use std::collections::HashMap;
use std::collections::HashSet;

use bevy::prelude::AppTypeRegistry;
use bevy::prelude::On;
use bevy::prelude::ReflectEvent;
use bevy::prelude::ResMut;
use bevy::prelude::Resource;
use bevy::prelude::World;
use bevy::reflect::PartialReflect;
use bevy::reflect::TypeRegistry;
use bevy_enhanced_input::prelude::ActionValue;
use bevy_enhanced_input::prelude::CustomInput;
use bevy_enhanced_input::prelude::CustomInputs;

use super::Capability;
use super::CommandId;
use super::HoldPhase;
use super::KeymapCommand;
use super::ReflectKeymapCommand;
use crate::Diagnostic;
use crate::DiagnosticKind;

/// A read-only table that resolves declared command IDs to their runtime registrations.
#[derive(Resource)]
pub struct CommandRegistry {
    entries:            HashMap<CommandId, CommandEntry>,
    held_registrations: Vec<(CustomInput, fn(&mut World, CustomInput))>,
}

/// One command's declared metadata, as read from the registry.
pub struct CommandInfo<'registry> {
    /// The command's validated identifier.
    pub id:          &'registry CommandId,
    /// Palette title text.
    pub title:       &'registry str,
    /// Palette description text.
    pub description: &'registry str,
    /// Declared keymap binding capability.
    pub capability:  Capability,
}

impl CommandRegistry {
    /// Builds a command registry from the application's reflected type registrations.
    ///
    /// The returned registry is not inserted into the world — the caller owns that. A bare `App`
    /// is enough to call this, since its world already carries the `AppTypeRegistry` this reads.
    ///
    /// # Errors
    ///
    /// Returns registry-validation diagnostics when a command ID is malformed or duplicated, a
    /// command title or description is empty, or a command event lacks `ReflectEvent` type data.
    pub fn initialize(world: &mut World) -> Result<Self, Vec<Diagnostic>> {
        world.init_resource::<CustomInputs>();
        let app_type_registry = world.resource::<AppTypeRegistry>().clone();
        let command_registry = {
            let type_registry = app_type_registry.read();
            let mut custom_inputs = world.resource_mut::<CustomInputs>();
            Self::build(&type_registry, &mut custom_inputs)?
        };

        command_registry.register_held_observers(world);
        Ok(command_registry)
    }

    /// Builds a command registry from one reflected type registry.
    pub(crate) fn build(
        type_registry: &TypeRegistry,
        custom_inputs: &mut CustomInputs,
    ) -> Result<Self, Vec<Diagnostic>> {
        let declarations = command_declarations(type_registry);
        let validated_declarations = validate_declarations(declarations)?;
        let mut held_registrations = Vec::new();
        let entries = validated_declarations
            .into_iter()
            .map(|declaration| {
                let capability = declaration.command.capability;
                let invocation = Invocation::from_capability(capability, custom_inputs);
                if let Some(custom_input) = invocation.custom_input() {
                    held_registrations
                        .push((custom_input, declaration.command.register_held_observer));
                }
                let command_entry = CommandEntry {
                    title: declaration.command.title,
                    description: declaration.command.description,
                    capability,
                    reflect_event: declaration.reflect_event,
                    invocation,
                };

                (declaration.command_id, command_entry)
            })
            .collect();

        Ok(Self {
            entries,
            held_registrations,
        })
    }

    /// Iterates over declared command metadata in ascending [`CommandId`] order.
    pub fn iter(&self) -> impl Iterator<Item = CommandInfo<'_>> {
        let mut commands = self
            .entries
            .iter()
            .map(|(command_id, entry)| CommandInfo {
                id:          command_id,
                title:       entry.title,
                description: entry.description,
                capability:  entry.capability,
            })
            .collect::<Vec<_>>();
        commands.sort_by(|left, right| left.id.as_str().cmp(right.id.as_str()));
        commands.into_iter()
    }

    /// Returns the authored title for `command_id`.
    #[must_use]
    pub fn title(&self, command_id: &CommandId) -> Option<&str> {
        self.entries.get(command_id).map(|entry| entry.title)
    }

    /// Returns the authored description for `command_id`.
    #[must_use]
    pub fn description(&self, command_id: &CommandId) -> Option<&str> {
        self.entries.get(command_id).map(|entry| entry.description)
    }

    /// Returns the runtime custom input allocated for a held command.
    #[must_use]
    pub fn custom_input(&self, command_id: &CommandId) -> Option<CustomInput> {
        self.entries
            .get(command_id)
            .and_then(|entry| entry.invocation.custom_input())
    }

    /// Returns the declared keymap binding capability for `command_id`.
    #[must_use]
    pub fn capability(&self, command_id: &CommandId) -> Option<Capability> {
        self.entries.get(command_id).map(|entry| entry.capability)
    }

    /// Triggers `event` through the `ReflectEvent` handle cached for `command_id`.
    ///
    /// This resolves the command ID through the registry table and performs no reflection type
    /// lookup on the input path.
    pub fn invoke(
        &self,
        command_id: &CommandId,
        event: &dyn PartialReflect,
        world: &mut World,
        type_registry: &TypeRegistry,
    ) -> Option<()> {
        self.entries
            .get(command_id)
            .map(|entry| entry.reflect_event.trigger(world, event, type_registry))
    }

    fn register_held_observers(&self, world: &mut World) {
        for (custom_input, register_held_observer) in &self.held_registrations {
            register_held_observer(world, *custom_input);
        }
    }
}

pub(crate) fn register_held_observer<T: KeymapCommand>(
    world: &mut World,
    custom_input: CustomInput,
) {
    world.add_observer(
        move |event: On<T>, mut custom_inputs: ResMut<CustomInputs>| {
            let Some(hold_phase) = event.event().hold_phase() else {
                return;
            };

            let input_is_active = custom_inputs
                .get(&custom_input)
                .is_some_and(|action_value| action_value.as_bool());
            match (hold_phase, input_is_active) {
                (HoldPhase::Begin, false) => {
                    custom_inputs.insert(custom_input, ActionValue::Bool(true));
                },
                (HoldPhase::End, true) => {
                    custom_inputs.insert(custom_input, ActionValue::Bool(false));
                },
                (HoldPhase::Begin, true) | (HoldPhase::End, false) => {},
            }
        },
    );
}

pub(crate) struct CommandEntry {
    title:         &'static str,
    description:   &'static str,
    capability:    Capability,
    reflect_event: ReflectEvent,
    invocation:    Invocation,
}

pub(crate) enum Invocation {
    OneShot,
    Held(CustomInput),
    Unremappable,
}

impl Invocation {
    fn from_capability(capability: Capability, custom_inputs: &mut CustomInputs) -> Self {
        match capability {
            Capability::OneShot => Self::OneShot,
            Capability::Held => Self::Held(custom_inputs.register_input()),
            Capability::Unremappable => Self::Unremappable,
        }
    }

    const fn custom_input(&self) -> Option<CustomInput> {
        match self {
            Self::Held(custom_input) => Some(*custom_input),
            Self::OneShot | Self::Unremappable => None,
        }
    }
}

struct CommandDeclaration {
    type_path:     &'static str,
    command:       ReflectKeymapCommand,
    reflect_event: Option<ReflectEvent>,
}

struct ValidatedDeclaration {
    command_id:    CommandId,
    command:       ReflectKeymapCommand,
    reflect_event: ReflectEvent,
}

fn command_declarations(type_registry: &TypeRegistry) -> Vec<CommandDeclaration> {
    let mut declarations = type_registry
        .iter_with_data::<ReflectKeymapCommand>()
        .map(|(registration, command)| CommandDeclaration {
            type_path:     registration.type_info().type_path(),
            command:       command.clone(),
            reflect_event: registration.data::<ReflectEvent>().cloned(),
        })
        .collect::<Vec<_>>();
    declarations.sort_by_key(|declaration| (declaration.command.id, declaration.type_path));
    declarations
}

fn validate_declarations(
    declarations: Vec<CommandDeclaration>,
) -> Result<Vec<ValidatedDeclaration>, Vec<Diagnostic>> {
    let mut command_ids = HashSet::new();
    let mut diagnostics = Vec::new();
    let mut validated_declarations = Vec::new();

    for declaration in declarations {
        let command_id = if let Ok(command_id) = CommandId::try_from(declaration.command.id) {
            Some(command_id)
        } else {
            diagnostics.push(registry_diagnostic(
                DiagnosticKind::InvalidCommandId,
                declaration.command.id,
                format!(
                    "Command event `{}` declares an invalid command ID.",
                    declaration.type_path
                ),
            ));
            None
        };

        let duplicate_id = command_id
            .as_ref()
            .is_some_and(|command_id| !command_ids.insert(command_id.clone()));
        if duplicate_id {
            diagnostics.push(registry_diagnostic(
                DiagnosticKind::DuplicateCommandId,
                declaration.command.id,
                format!(
                    "Command event `{}` duplicates command ID `{}`.",
                    declaration.type_path, declaration.command.id
                ),
            ));
        }

        let missing_title = declaration.command.title.is_empty();
        if missing_title {
            diagnostics.push(registry_diagnostic(
                DiagnosticKind::MissingCommandTitle,
                declaration.command.id,
                format!(
                    "Command ID `{}` has an empty title.",
                    declaration.command.id
                ),
            ));
        }

        let missing_description = declaration.command.description.is_empty();
        if missing_description {
            diagnostics.push(registry_diagnostic(
                DiagnosticKind::MissingCommandDescription,
                declaration.command.id,
                format!(
                    "Command ID `{}` has an empty description.",
                    declaration.command.id
                ),
            ));
        }

        let reflected_event = declaration.reflect_event;
        if reflected_event.is_none() {
            diagnostics.push(registry_diagnostic(
                DiagnosticKind::CommandEventNotReflected,
                declaration.command.id,
                format!(
                    "Command event `{}` is not reflected as an event.",
                    declaration.type_path
                ),
            ));
        }

        if let (Some(command_id), Some(reflect_event)) = (command_id, reflected_event)
            && !duplicate_id
            && !missing_title
            && !missing_description
        {
            validated_declarations.push(ValidatedDeclaration {
                command_id,
                command: declaration.command,
                reflect_event,
            });
        }
    }

    if diagnostics.is_empty() {
        Ok(validated_declarations)
    } else {
        Err(diagnostics)
    }
}

fn registry_diagnostic(kind: DiagnosticKind, command_id: &str, message: String) -> Diagnostic {
    Diagnostic {
        source_path: String::new(),
        byte_range: 0..0,
        line: 0,
        column: 0,
        block_index: 0,
        context: String::new(),
        original_keystroke: String::new(),
        command_id: command_id.to_owned(),
        kind,
        message,
        suggestions: Vec::new(),
    }
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    reason = "tests should panic on unexpected values"
)]
mod tests {
    use std::any::type_name;

    use bevy::prelude::App;
    use bevy::prelude::AppTypeRegistry;
    use bevy::prelude::Event;
    use bevy::prelude::On;
    use bevy::prelude::Reflect;
    use bevy::prelude::ReflectEvent;
    use bevy::prelude::Resource;
    use bevy::reflect::TypeRegistry;
    use bevy_enhanced_input::prelude::ActionValue;
    use bevy_enhanced_input::prelude::CustomInputs;
    use bevy_enhanced_input::prelude::InputAction;

    use super::CommandRegistry;
    use super::Invocation;
    use crate::Capability;
    use crate::CommandId;
    use crate::DiagnosticKind;
    use crate::HoldPhase;
    use crate::KeymapCommand;
    use crate::ReflectKeymapCommand;

    crate::command! {
        action:      RegistryOneShotAction,
        event:       RegistryOneShot,
        id:          "registry::one_shot",
        title:       "Registry One Shot",
        description: "Triggers a one-shot command through the registry.",
    }

    crate::command! {
        held,
        action:      RegistryHeldAction,
        event:       RegistryHeld,
        id:          "registry::held",
        title:       "Registry Held",
        description: "Drives a custom input while the command is held.",
    }

    #[derive(Event, Reflect)]
    #[reflect(Event, KeymapCommand)]
    struct DuplicateFirst;

    impl KeymapCommand for DuplicateFirst {
        const ID: &'static str = "registry::duplicate";
        const TITLE: &'static str = "First Duplicate";
        const DESCRIPTION: &'static str = "The first duplicate command declaration.";
        const CAPABILITY: crate::Capability = crate::Capability::OneShot;

        fn hold_phase(&self) -> Option<HoldPhase> { None }
    }

    #[derive(Event, Reflect)]
    #[reflect(Event, KeymapCommand)]
    struct DuplicateSecond;

    impl KeymapCommand for DuplicateSecond {
        const ID: &'static str = "registry::duplicate";
        const TITLE: &'static str = "Second Duplicate";
        const DESCRIPTION: &'static str = "The second duplicate command declaration.";
        const CAPABILITY: crate::Capability = crate::Capability::OneShot;

        fn hold_phase(&self) -> Option<HoldPhase> { None }
    }

    #[derive(Event, Reflect)]
    #[reflect(Event, KeymapCommand)]
    struct EmptyTitle;

    impl KeymapCommand for EmptyTitle {
        const ID: &'static str = "registry::empty_title";
        const TITLE: &'static str = "";
        const DESCRIPTION: &'static str = "A command declaration with an empty title.";
        const CAPABILITY: crate::Capability = crate::Capability::OneShot;

        fn hold_phase(&self) -> Option<HoldPhase> { None }
    }

    #[derive(Event, Reflect)]
    #[reflect(Event, KeymapCommand)]
    struct EmptyDescription;

    impl KeymapCommand for EmptyDescription {
        const ID: &'static str = "registry::empty_description";
        const TITLE: &'static str = "Empty Description";
        const DESCRIPTION: &'static str = "";
        const CAPABILITY: crate::Capability = crate::Capability::OneShot;

        fn hold_phase(&self) -> Option<HoldPhase> { None }
    }

    #[derive(Event, Reflect)]
    #[reflect(Event, KeymapCommand)]
    struct InvalidId;

    impl KeymapCommand for InvalidId {
        const ID: &'static str = "invalid";
        const TITLE: &'static str = "Invalid ID";
        const DESCRIPTION: &'static str = "A command declaration with malformed ID text.";
        const CAPABILITY: crate::Capability = crate::Capability::OneShot;

        fn hold_phase(&self) -> Option<HoldPhase> { None }
    }

    #[derive(Event, Reflect)]
    #[reflect(KeymapCommand)]
    struct EventNotReflected;

    impl KeymapCommand for EventNotReflected {
        const ID: &'static str = "registry::event_not_reflected";
        const TITLE: &'static str = "Event Not Reflected";
        const DESCRIPTION: &'static str = "A command event without ReflectEvent type data.";
        const CAPABILITY: crate::Capability = crate::Capability::OneShot;

        fn hold_phase(&self) -> Option<HoldPhase> { None }
    }

    #[derive(Resource, Default)]
    struct InvocationCount(usize);

    #[test]
    fn registry_rejects_duplicate_ids() {
        let mut type_registry = TypeRegistry::default();
        type_registry.register::<DuplicateFirst>();
        type_registry.register::<DuplicateSecond>();
        let mut custom_inputs = CustomInputs::default();

        let diagnostics = CommandRegistry::build(&type_registry, &mut custom_inputs)
            .err()
            .expect("duplicate command IDs must be rejected");

        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic.kind == DiagnosticKind::DuplicateCommandId
                && diagnostic.message.contains("registry::duplicate")
                && diagnostic.message.contains(type_name::<DuplicateSecond>())
        }));
    }

    #[test]
    fn registry_rejects_empty_titles_and_descriptions() {
        let mut type_registry = TypeRegistry::default();
        type_registry.register::<EmptyTitle>();
        type_registry.register::<EmptyDescription>();
        let mut custom_inputs = CustomInputs::default();

        let diagnostics = CommandRegistry::build(&type_registry, &mut custom_inputs)
            .err()
            .expect("empty command text must be rejected");

        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic.kind == DiagnosticKind::MissingCommandTitle
                && diagnostic.message.contains(EmptyTitle::ID)
        }));
        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic.kind == DiagnosticKind::MissingCommandDescription
                && diagnostic.message.contains(EmptyDescription::ID)
        }));
    }

    #[test]
    fn registry_rejects_hand_written_invalid_ids_and_unreflected_events() {
        let mut type_registry = TypeRegistry::default();
        type_registry.register::<InvalidId>();
        type_registry.register::<EventNotReflected>();
        let mut custom_inputs = CustomInputs::default();

        let diagnostics = CommandRegistry::build(&type_registry, &mut custom_inputs)
            .err()
            .expect("hand-written command errors must be diagnosed");

        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic.kind == DiagnosticKind::InvalidCommandId
                && diagnostic.message.contains(type_name::<InvalidId>())
        }));
        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic.kind == DiagnosticKind::CommandEventNotReflected
                && diagnostic
                    .message
                    .contains(type_name::<EventNotReflected>())
        }));
    }

    #[test]
    fn registry_invokes_the_cached_reflected_event() {
        let mut app = App::new();
        app.world_mut().init_resource::<InvocationCount>();
        app.world_mut().add_observer(
            |_: On<RegistryOneShot>,
             mut invocation_count: bevy::prelude::ResMut<InvocationCount>| {
                invocation_count.0 += 1;
            },
        );

        let mut type_registry = TypeRegistry::default();
        type_registry.register::<RegistryOneShot>();
        let mut custom_inputs = CustomInputs::default();
        let command_registry = CommandRegistry::build(&type_registry, &mut custom_inputs)
            .expect("a valid command must build a registry");
        let command_id = CommandId::try_from(RegistryOneShot::ID)
            .expect("the command macro validates the command ID");

        assert_eq!(
            command_registry.title(&command_id),
            Some(RegistryOneShot::TITLE)
        );
        assert_eq!(
            command_registry.description(&command_id),
            Some(RegistryOneShot::DESCRIPTION)
        );
        assert_eq!(
            command_registry.invoke(
                &command_id,
                &RegistryOneShot,
                app.world_mut(),
                &type_registry,
            ),
            Some(())
        );
        assert_eq!(app.world().resource::<InvocationCount>().0, 1);
    }

    #[test]
    fn initialize_resolves_registered_commands_and_stable_metadata() {
        let mut app = App::new();
        app.world_mut().insert_resource(AppTypeRegistry::default());
        let app_type_registry = app.world().resource::<AppTypeRegistry>().clone();
        {
            let mut type_registry = app_type_registry.write();
            type_registry.register::<RegistryOneShot>();
            type_registry.register::<RegistryHeld>();
        }

        let command_registry = CommandRegistry::initialize(app.world_mut())
            .expect("registered commands must initialize");
        let command_id = CommandId::try_from(RegistryHeld::ID)
            .expect("the command macro validates the command ID");

        assert_eq!(
            command_registry.title(&command_id),
            Some(RegistryHeld::TITLE)
        );
        assert_eq!(
            command_registry.capability(&command_id),
            Some(Capability::Held)
        );
        assert!(command_registry.custom_input(&command_id).is_some());
        assert_eq!(
            command_registry
                .iter()
                .map(|command| command.id.as_str())
                .collect::<Vec<_>>(),
            ["registry::held", "registry::one_shot"]
        );
    }

    #[test]
    fn held_command_registers_one_custom_input_and_updates_it_from_events() {
        let mut app = App::new();
        app.world_mut().init_resource::<CustomInputs>();
        let mut type_registry = TypeRegistry::default();
        type_registry.register::<RegistryHeld>();
        let command_registry = {
            let mut custom_inputs = app.world_mut().resource_mut::<CustomInputs>();
            CommandRegistry::build(&type_registry, &mut custom_inputs)
                .expect("a held command must build a registry")
        };
        command_registry.register_held_observers(app.world_mut());
        let command_id = CommandId::try_from(RegistryHeld::ID)
            .expect("the command macro validates the command ID");
        let custom_input = command_registry
            .custom_input(&command_id)
            .expect("held commands must have one custom input");

        assert!(matches!(
            command_registry
                .entries
                .get(&command_id)
                .map(|entry| &entry.invocation),
            Some(Invocation::Held(_))
        ));
        assert_eq!(
            app.world().resource::<CustomInputs>().get(&custom_input),
            None
        );

        app.world_mut().trigger(RegistryHeld {
            phase: HoldPhase::End,
        });
        assert_eq!(
            app.world().resource::<CustomInputs>().get(&custom_input),
            None
        );

        assert_eq!(
            command_registry.invoke(
                &command_id,
                &RegistryHeld {
                    phase: HoldPhase::Begin,
                },
                app.world_mut(),
                &type_registry,
            ),
            Some(())
        );
        assert_eq!(
            app.world().resource::<CustomInputs>().get(&custom_input),
            Some(&ActionValue::Bool(true))
        );

        app.world_mut().trigger(RegistryHeld {
            phase: HoldPhase::End,
        });
        assert_eq!(
            app.world().resource::<CustomInputs>().get(&custom_input),
            Some(&ActionValue::Bool(false))
        );

        app.world_mut().trigger(RegistryHeld {
            phase: HoldPhase::End,
        });
        assert_eq!(
            app.world().resource::<CustomInputs>().get(&custom_input),
            Some(&ActionValue::Bool(false))
        );
    }

    #[test]
    fn registry_accepts_commands_with_required_authored_text() {
        let mut type_registry = TypeRegistry::default();
        type_registry.register::<RegistryOneShot>();
        type_registry.register::<RegistryHeld>();
        let mut custom_inputs = CustomInputs::default();

        assert!(CommandRegistry::build(&type_registry, &mut custom_inputs).is_ok());
    }
}
