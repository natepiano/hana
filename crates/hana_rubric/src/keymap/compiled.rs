//! Matcher construction from a semantically resolved keymap.

use std::collections::HashMap;
#[cfg(test)]
use std::time::Duration;
#[cfg(test)]
use std::time::Instant;

use bevy::prelude::Resource;
use bevy::prelude::World;
use bevy_enhanced_input::prelude::CustomInput;

use super::MergedKeymap;
use crate::CommandRegistry;
use crate::KeystrokeSequence;
use crate::ModifierFamily;
use crate::PrimaryTrigger;
use crate::SequenceMatcher;
use crate::command::CommandEntry;
use crate::command::Invocation;
use crate::condition::ConditionHandle;

/// A monotonically increasing keymap replacement identity.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Generation(pub(crate) usize);

/// An opaque index into [`CompiledKeymap`]'s resolved command table.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct CommandHandle(usize);

#[cfg(test)]
impl CommandHandle {
    pub(super) const fn from_index(index: usize) -> Self { Self(index) }
}

/// The global or condition-specific binding table selected by the active application context.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ActiveKeymapScope {
    Global,
    Condition(ConditionHandle),
}

/// One complete set of matchers and command handles for a keymap generation.
#[derive(Resource)]
pub(crate) struct CompiledKeymap {
    pub(super) generation:  Generation,
    pub(super) global:      SequenceMatcher<CommandHandle>,
    pub(super) commands:    Vec<CommandEntry>,
    pub(super) matchers:    HashMap<ConditionHandle, SequenceMatcher<CommandHandle>>,
    modifier_held_bindings: ContextualModifierHeldBindings,
}

impl CompiledKeymap {
    /// Builds a replacement keymap with one resolved matcher for every registered condition.
    #[must_use]
    pub(crate) fn from_merged(
        generation: Generation,
        merged_keymap: &MergedKeymap,
        command_registry: &CommandRegistry,
    ) -> Self {
        let command_entries = command_registry
            .iter_entries()
            .map(|(command_id, command_entry)| (command_id.clone(), command_entry.clone()))
            .collect::<Vec<_>>();
        let command_handles = command_entries
            .iter()
            .enumerate()
            .map(|(index, (command_id, _))| (command_id.clone(), CommandHandle(index)))
            .collect::<HashMap<_, _>>();
        let modifier_held_bindings = ContextualModifierHeldBindings::from_merged(
            merged_keymap,
            &command_handles,
            &command_entries,
        );
        let global = SequenceMatcher::new(merged_keymap.global().iter().filter_map(
            |(keystroke_sequence, command_id)| {
                matcher_entry(keystroke_sequence, command_id, &command_handles)
            },
        ));
        let matchers = merged_keymap
            .conditions
            .iter()
            .map(|(condition_handle, bindings)| {
                let sequences = bindings
                    .iter()
                    .filter_map(|(keystroke_sequence, command_id)| {
                        matcher_entry(keystroke_sequence, command_id, &command_handles)
                    });

                (*condition_handle, SequenceMatcher::new(sequences))
            })
            .collect();
        let commands = command_entries
            .into_iter()
            .map(|(_, command_entry)| command_entry)
            .collect();

        Self {
            generation,
            global,
            commands,
            matchers,
            modifier_held_bindings,
        }
    }

    pub(super) fn modifier_family_held_binding(
        &self,
        active_keymap_scope: ActiveKeymapScope,
        modifier_family: ModifierFamily,
    ) -> ModifierFamilyHeldBinding {
        self.modifier_held_bindings
            .active(active_keymap_scope)
            .for_family(modifier_family)
    }

    pub(crate) fn invocation(&self, command_handle: CommandHandle) -> Option<Invocation> {
        self.commands
            .get(command_handle.0)
            .map(CommandEntry::invocation)
    }

    pub(crate) fn dispatch(&self, command_handle: CommandHandle) -> Option<fn(&mut World)> {
        self.commands
            .get(command_handle.0)
            .map(CommandEntry::dispatch)
    }

    #[cfg(test)]
    pub(crate) const fn generation(&self) -> Generation { self.generation }

    #[cfg(test)]
    pub(crate) fn match_global(
        &mut self,
        keystroke: crate::Keystroke,
        now: Instant,
        timeout: Duration,
    ) -> crate::MatchOutcome<CommandHandle> {
        self.global.match_keystroke(keystroke, now, timeout)
    }
}

/// Pairs a binding with its command handle when a [`SequenceMatcher`] is the path that routes it.
///
/// Only ordinary-key strokes reach a matcher: `route_ordinary_key_presses` is the sole caller of
/// `match_keystroke` and it only ever builds [`PrimaryTrigger::OrdinaryKey`] keystrokes. A
/// modifier-family binding routes through [`ContextualModifierHeldBindings`] instead, so leaving
/// it out here keeps the two paths disjoint rather than seeding the matcher with entries no
/// keystroke can reach.
fn matcher_entry(
    keystroke_sequence: &KeystrokeSequence,
    command_id: &crate::CommandId,
    command_handles: &HashMap<crate::CommandId, CommandHandle>,
) -> Option<(KeystrokeSequence, CommandHandle)> {
    if !routes_ordinary_keys_only(keystroke_sequence) {
        return None;
    }

    command_handles
        .get(command_id)
        .copied()
        .map(|command_handle| (keystroke_sequence.clone(), command_handle))
}

fn routes_ordinary_keys_only(keystroke_sequence: &KeystrokeSequence) -> bool {
    keystroke_sequence
        .iter()
        .all(|keystroke| matches!(keystroke.primary_trigger(), PrimaryTrigger::OrdinaryKey(_)))
}

#[derive(Clone, Copy, Default)]
pub(crate) enum ModifierFamilyHeldBinding {
    #[default]
    Unbound,
    Bound(CustomInput),
}

#[derive(Clone, Copy, Default)]
struct ModifierFamilyHeldBindings {
    control:  ModifierFamilyHeldBinding,
    alt:      ModifierFamilyHeldBinding,
    shift:    ModifierFamilyHeldBinding,
    platform: ModifierFamilyHeldBinding,
}

impl ModifierFamilyHeldBindings {
    fn from_bindings(
        bindings: &[(KeystrokeSequence, crate::CommandId)],
        command_handles: &HashMap<crate::CommandId, CommandHandle>,
        command_entries: &[(crate::CommandId, CommandEntry)],
    ) -> Self {
        let mut modifier_family_held_bindings = Self::default();

        for (keystroke_sequence, command_id) in bindings {
            if keystroke_sequence.len() != 1 {
                continue;
            }
            let PrimaryTrigger::ModifierFamily(modifier_family) =
                keystroke_sequence.first().primary_trigger()
            else {
                continue;
            };
            let Some(command_handle) = command_handles.get(command_id).copied() else {
                continue;
            };
            let Some((_, command_entry)) = command_entries.get(command_handle.0) else {
                continue;
            };
            let Invocation::Held(custom_input) = command_entry.invocation() else {
                continue;
            };

            modifier_family_held_bindings.set(
                modifier_family,
                ModifierFamilyHeldBinding::Bound(custom_input),
            );
        }

        modifier_family_held_bindings
    }

    const fn for_family(self, modifier_family: ModifierFamily) -> ModifierFamilyHeldBinding {
        match modifier_family {
            ModifierFamily::Control => self.control,
            ModifierFamily::Alt => self.alt,
            ModifierFamily::Shift => self.shift,
            ModifierFamily::Platform => self.platform,
        }
    }

    const fn set(
        &mut self,
        modifier_family: ModifierFamily,
        modifier_family_held_binding: ModifierFamilyHeldBinding,
    ) {
        match modifier_family {
            ModifierFamily::Control => self.control = modifier_family_held_binding,
            ModifierFamily::Alt => self.alt = modifier_family_held_binding,
            ModifierFamily::Shift => self.shift = modifier_family_held_binding,
            ModifierFamily::Platform => self.platform = modifier_family_held_binding,
        }
    }
}

#[derive(Default)]
struct ContextualModifierHeldBindings {
    global:     ModifierFamilyHeldBindings,
    conditions: HashMap<ConditionHandle, ModifierFamilyHeldBindings>,
}

impl ContextualModifierHeldBindings {
    fn from_merged(
        merged_keymap: &MergedKeymap,
        command_handles: &HashMap<crate::CommandId, CommandHandle>,
        command_entries: &[(crate::CommandId, CommandEntry)],
    ) -> Self {
        let global = ModifierFamilyHeldBindings::from_bindings(
            merged_keymap.global(),
            command_handles,
            command_entries,
        );
        let conditions = merged_keymap
            .conditions
            .iter()
            .map(|(condition_handle, bindings)| {
                (
                    *condition_handle,
                    ModifierFamilyHeldBindings::from_bindings(
                        bindings,
                        command_handles,
                        command_entries,
                    ),
                )
            })
            .collect();

        Self { global, conditions }
    }

    fn active(&self, active_keymap_scope: ActiveKeymapScope) -> ModifierFamilyHeldBindings {
        match active_keymap_scope {
            ActiveKeymapScope::Condition(condition_handle) => self
                .conditions
                .get(&condition_handle)
                .copied()
                .unwrap_or_default(),
            ActiveKeymapScope::Global => self.global,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;
    use std::time::Instant;

    use bevy::prelude::Event;
    use bevy::prelude::Reflect;
    use bevy::prelude::ReflectEvent;
    use bevy::reflect::TypeRegistry;
    use bevy_enhanced_input::prelude::CustomInputs;

    use super::CompiledKeymap;
    use super::Generation;
    use crate::Capability;
    use crate::CommandRegistry;
    use crate::HoldPhase;
    use crate::KeymapCommand;
    use crate::KeystrokeSequence;
    use crate::MatchOutcome;
    use crate::ReflectKeymapCommand;
    use crate::command::Invocation;
    use crate::condition::ConditionRegistry;
    use crate::keymap::MergedKeymap;

    const DEFAULTS_PATH: &str = "defaults.jsonc";
    const MATCH_TIMEOUT: Duration = Duration::from_secs(1);

    #[derive(Default, Event, Reflect)]
    #[reflect(Event, KeymapCommand)]
    struct CompiledGlobal;

    impl KeymapCommand for CompiledGlobal {
        const ID: &'static str = "compiled::global";
        const TITLE: &'static str = "Compiled Global";
        const DESCRIPTION: &'static str = "A global command used by the compiled keymap test.";
        const CAPABILITY: Capability = Capability::OneShot;

        fn build() -> Self { Self }

        fn hold_phase(&self) -> Option<HoldPhase> { None }
    }

    fn command_registry() -> Result<CommandRegistry, String> {
        let mut type_registry = TypeRegistry::default();
        type_registry.register::<CompiledGlobal>();
        let mut custom_inputs = CustomInputs::default();

        CommandRegistry::build(&type_registry, &mut custom_inputs)
            .map_err(|diagnostics| format!("command registry errors: {diagnostics:?}"))
    }

    #[test]
    fn global_binding_compiles_without_registered_conditions() -> Result<(), String> {
        let command_registry = command_registry()?;
        let condition_registry = ConditionRegistry::default();
        let (merged_keymap, diagnostics) = MergedKeymap::from_sources(
            DEFAULTS_PATH,
            r#"{ "bindings": [{ "bindings": { "g": "compiled::global" } }] }"#,
            None,
            &command_registry,
            &condition_registry,
            &[],
        )
        .map_err(|diagnostics| format!("keymap parse errors: {diagnostics:?}"))?;
        let mut compiled_keymap =
            CompiledKeymap::from_merged(Generation(0), &merged_keymap, &command_registry);
        let keystroke = "g"
            .parse::<KeystrokeSequence>()
            .map_err(|error| format!("invalid test sequence: {error}"))?
            .first();

        assert!(diagnostics.is_empty());
        assert!(compiled_keymap.matchers.is_empty());
        let command_handle =
            match compiled_keymap
                .global
                .match_keystroke(keystroke, Instant::now(), MATCH_TIMEOUT)
            {
                MatchOutcome::Matched(command_handle) => command_handle,
                match_outcome => {
                    return Err(format!("expected a global match, got {match_outcome:?}"));
                },
            };
        assert!(matches!(
            compiled_keymap.invocation(command_handle),
            Some(Invocation::OneShot)
        ));

        Ok(())
    }
}
