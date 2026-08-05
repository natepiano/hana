use std::time::Instant;

use bevy::ecs::world::World;
use bevy::input::ButtonInput;
use bevy::input::keyboard::KeyCode;
use bevy_enhanced_input::prelude::CustomInput;
use bevy_enhanced_input::prelude::CustomInputs;

use super::held::ActiveMatcher;
use super::held::CustomInputTransition;
use super::held::HeldChordPhysicalOwnership;
use super::held::KeyboardHandover;
use super::held::KeymapRuntime;
use super::held::PhysicalSourceReleaseProgress;
use super::key_edge;
use super::key_edge::OrdinaryKeyRoutingState;
use super::key_edge::PhysicalKeyRole;
use super::key_edge::PrimaryTriggerOwnership;
use crate::ActiveCondition;
use crate::ActiveConditionState;
use crate::DeferredMatch;
use crate::MatchOutcome;
use crate::Modifiers;
use crate::SequenceMatcher;
use crate::TimeoutOutcome;
use crate::command::Invocation;
use crate::keymap::ActiveKeymapScope;
use crate::keymap::CommandHandle;
use crate::keymap::CompiledKeymap;
use crate::keymap::KeystrokeRouting;
use crate::keymap::ModifierFamilyHeldBinding;
use crate::keymap::constants::SEQUENCE_TIMEOUT;

pub(crate) fn cancel_pending_sequences(world: &mut World) {
    let Some(mut compiled_keymap) = world.get_resource_mut::<CompiledKeymap>() else {
        return;
    };

    compiled_keymap.global.cancel_pending();
    for sequence_matcher in compiled_keymap.matchers.values_mut() {
        sequence_matcher.cancel_pending();
    }
}

pub(crate) fn reset_physical_input(world: &mut World) {
    cancel_pending_sequences(world);
    world.init_resource::<KeymapRuntime>();
    world.init_resource::<CustomInputs>();

    let pressed = world
        .get_resource::<ButtonInput<KeyCode>>()
        .map(|keys| keys.get_pressed().copied().collect::<Vec<_>>())
        .unwrap_or_default();
    release_all_physical_sources(world);
    world
        .resource_mut::<KeymapRuntime>()
        .inhibit(pressed.into_iter());
}

pub(crate) fn route_input(world: &mut World) {
    if !world.contains_resource::<CompiledKeymap>() {
        return;
    }
    let active_keymap_scope = match world.get_resource::<ActiveCondition>() {
        Some(active_condition) => match active_condition.state() {
            ActiveConditionState::GlobalRouting => ActiveKeymapScope::Global,
            ActiveConditionState::ResolvedCondition { handle, .. }
                if handle.is_registry_issued() =>
            {
                ActiveKeymapScope::Condition(*handle)
            },
            // A handle the registry never issued reaches this resource only through a reflected
            // reconstruction of `ActiveCondition`, whose ignored handle field comes from
            // `ConditionHandle::default`. Routing on it would select the first registered
            // condition regardless of the name the resource reports.
            ActiveConditionState::AwaitingContext
            | ActiveConditionState::ResolvedCondition { .. } => return,
        },
        None => ActiveKeymapScope::Global,
    };
    world.init_resource::<KeymapRuntime>();
    world.init_resource::<CustomInputs>();
    let keystroke_routing = world
        .get_resource::<KeystrokeRouting>()
        .cloned()
        .unwrap_or_default();
    if matches!(
        world
            .resource_mut::<KeymapRuntime>()
            .observe_routing(&keystroke_routing),
        KeyboardHandover::Crossed
    ) {
        reset_physical_input(world);
    }

    let routed_commands =
        synchronize_and_resolve_timeout(world, active_keymap_scope, &keystroke_routing);
    dispatch_all(world, routed_commands);
    route_releases(world);
    route_presses(world, active_keymap_scope, &keystroke_routing);
}

fn synchronize_and_resolve_timeout(
    world: &mut World,
    active_keymap_scope: ActiveKeymapScope,
    keystroke_routing: &KeystrokeRouting,
) -> RoutedCommands {
    let reset_required = world.resource_scope::<CompiledKeymap, _>(|world, mut compiled_keymap| {
        let mut keymap_runtime = world.resource_mut::<KeymapRuntime>();
        let generation_changed = keymap_runtime
            .generation()
            .is_some_and(|generation| generation != compiled_keymap.generation);
        let condition_changed = keymap_runtime.condition_changed(active_keymap_scope);

        if condition_changed
            && !generation_changed
            && let Some(sequence_matcher) =
                previous_matcher(&mut compiled_keymap, keymap_runtime.active_matcher())
        {
            sequence_matcher.cancel_pending();
        }
        keymap_runtime.update_generation(compiled_keymap.generation, active_keymap_scope);
        generation_changed || condition_changed
    });

    if reset_required {
        let pressed = world
            .get_resource::<ButtonInput<KeyCode>>()
            .map(|pressed| pressed.get_pressed().copied().collect::<Vec<_>>())
            .unwrap_or_default();
        release_all_physical_sources(world);
        world
            .resource_mut::<KeymapRuntime>()
            .inhibit(pressed.into_iter());
    }

    world.resource_scope::<CompiledKeymap, _>(|world, mut compiled_keymap| {
        let now = world.resource::<KeymapRuntime>().now();
        let timeout_outcome = matcher(&mut compiled_keymap, active_keymap_scope)
            .map_or(TimeoutOutcome::NoPendingSequence, |sequence_matcher| {
                sequence_matcher.resolve_timeout(now, SEQUENCE_TIMEOUT)
            });

        match timeout_outcome {
            TimeoutOutcome::Resolved(command_handle) => RoutedCommands::from(routed_command(
                &compiled_keymap,
                command_handle,
                keystroke_routing,
            )),
            TimeoutOutcome::DiscardedPartialPrefix
            | TimeoutOutcome::AwaitingKeystroke
            | TimeoutOutcome::NoPendingSequence => RoutedCommands::default(),
        }
    })
}

fn route_releases(world: &mut World) {
    clear_processed_keycodes(world);
    while let Some(key) = next_released_key(world) {
        let pressed_modifiers = world
            .get_resource::<ButtonInput<KeyCode>>()
            .map(Modifiers::from_pressed)
            .unwrap_or_default();
        let custom_input_transition = {
            let mut keymap_runtime = world.resource_mut::<KeymapRuntime>();
            keymap_runtime.mark_processed(key);
            keymap_runtime.release_inhibition(key);
            keymap_runtime.release_key(key)
        };
        write_custom_input_transition(world, custom_input_transition);
        release_chords_missing_modifiers(world, pressed_modifiers);
    }
}

fn route_presses(
    world: &mut World,
    active_keymap_scope: ActiveKeymapScope,
    keystroke_routing: &KeystrokeRouting,
) {
    clear_processed_keycodes(world);
    let primary_trigger_ownership = world.get_resource::<ButtonInput<KeyCode>>().map_or(
        PrimaryTriggerOwnership::Unclaimed,
        PrimaryTriggerOwnership::from,
    );

    match primary_trigger_ownership {
        PrimaryTriggerOwnership::Unclaimed => {},
        PrimaryTriggerOwnership::ModifierFamilies => {
            activate_modifier_family_held_bindings(world, active_keymap_scope, keystroke_routing);
        },
        PrimaryTriggerOwnership::OrdinaryKeys(ordinary_key_routing_state) => {
            suspend_modifier_family_held_bindings(world);
            route_ordinary_key_presses(world, active_keymap_scope, keystroke_routing);
            match ordinary_key_routing_state {
                OrdinaryKeyRoutingState::Held => {},
                OrdinaryKeyRoutingState::PressEdgesOnly => {
                    activate_modifier_family_held_bindings(
                        world,
                        active_keymap_scope,
                        keystroke_routing,
                    );
                },
            }
        },
    }
}

fn route_ordinary_key_presses(
    world: &mut World,
    active_keymap_scope: ActiveKeymapScope,
    keystroke_routing: &KeystrokeRouting,
) {
    while let Some(key) = next_pressed_key(world) {
        world.resource_mut::<KeymapRuntime>().mark_processed(key);
        if world.resource::<KeymapRuntime>().is_inhibited(key) {
            continue;
        }
        let PhysicalKeyRole::OrdinaryKey(ordinary_key) = PhysicalKeyRole::from(key) else {
            continue;
        };

        let Some(keystroke) = world
            .get_resource::<ButtonInput<KeyCode>>()
            .map(|pressed| key_edge::keystroke(pressed, ordinary_key))
        else {
            continue;
        };
        let held_chord_physical_ownership =
            HeldChordPhysicalOwnership::new(key, keystroke.modifiers());
        let routed_commands =
            route_keystroke(world, active_keymap_scope, keystroke, keystroke_routing);
        claim_held_chords(world, routed_commands, held_chord_physical_ownership);
        release_physical_if_no_longer_pressed(world, key);
        dispatch_all(world, routed_commands);
    }
}

/// Activates the held custom input of every hold-to-act command the pressed chord matched.
fn claim_held_chords(
    world: &mut World,
    routed_commands: RoutedCommands,
    held_chord_physical_ownership: HeldChordPhysicalOwnership,
) {
    claim_held_chord(world, routed_commands.first, held_chord_physical_ownership);
    claim_held_chord(world, routed_commands.second, held_chord_physical_ownership);
}

fn claim_held_chord(
    world: &mut World,
    routed_command: RoutedCommand,
    held_chord_physical_ownership: HeldChordPhysicalOwnership,
) {
    let RoutedCommand::HoldChord(custom_input) = routed_command else {
        return;
    };
    let custom_input_transition = world
        .resource_mut::<KeymapRuntime>()
        .activate_ordinary_chord(held_chord_physical_ownership, custom_input);
    write_custom_input_transition(world, custom_input_transition);
}

fn release_physical_if_no_longer_pressed(world: &mut World, key: KeyCode) {
    let key_remains_pressed = world
        .get_resource::<ButtonInput<KeyCode>>()
        .is_some_and(|key_input| key_input.pressed(key));
    let pressed_modifiers = world
        .get_resource::<ButtonInput<KeyCode>>()
        .map(Modifiers::from_pressed)
        .unwrap_or_default();
    if !key_remains_pressed {
        let custom_input_transition = world.resource_mut::<KeymapRuntime>().release_key(key);
        write_custom_input_transition(world, custom_input_transition);
    }
    release_chords_missing_modifiers(world, pressed_modifiers);
}

/// Activates the bare-modifier held bindings of the keymap, unless a text field
/// owns the keyboard — a held modifier is never the command that closes a field,
/// so no exemption reaches this path.
fn activate_modifier_family_held_bindings(
    world: &mut World,
    active_keymap_scope: ActiveKeymapScope,
    keystroke_routing: &KeystrokeRouting,
) {
    if matches!(keystroke_routing, KeystrokeRouting::TextEntry { .. }) {
        return;
    }
    for key in key_edge::PHYSICAL_MODIFIER_KEYS {
        let is_pressed = world
            .get_resource::<ButtonInput<KeyCode>>()
            .is_some_and(|pressed| pressed.pressed(key));
        if !is_pressed || world.resource::<KeymapRuntime>().is_inhibited(key) {
            continue;
        }
        let PhysicalKeyRole::ModifierFamily(modifier_family) = PhysicalKeyRole::from(key) else {
            continue;
        };
        let modifier_family_held_binding = world
            .resource::<CompiledKeymap>()
            .modifier_family_held_binding(active_keymap_scope, modifier_family);
        let ModifierFamilyHeldBinding::Bound(custom_input) = modifier_family_held_binding else {
            continue;
        };

        let custom_input_transition = world
            .resource_mut::<KeymapRuntime>()
            .activate_modifier_family(key, custom_input);
        write_custom_input_transition(world, custom_input_transition);
    }
}

fn suspend_modifier_family_held_bindings(world: &mut World) {
    for key in key_edge::PHYSICAL_MODIFIER_KEYS {
        let custom_input_transition = world.resource_mut::<KeymapRuntime>().release_key(key);
        write_custom_input_transition(world, custom_input_transition);
    }
}

fn clear_processed_keycodes(world: &mut World) {
    world
        .resource_mut::<KeymapRuntime>()
        .clear_processed_keycodes();
}

fn next_pressed_key(world: &World) -> Option<KeyCode> {
    let keymap_runtime = world.get_resource::<KeymapRuntime>()?;
    world
        .get_resource::<ButtonInput<KeyCode>>()?
        .get_just_pressed()
        .copied()
        .find(|key| !keymap_runtime.is_processed(*key))
}

fn next_released_key(world: &World) -> Option<KeyCode> {
    let keymap_runtime = world.get_resource::<KeymapRuntime>()?;
    world
        .get_resource::<ButtonInput<KeyCode>>()?
        .get_just_released()
        .copied()
        .find(|key| !keymap_runtime.is_processed(*key))
}

fn route_keystroke(
    world: &mut World,
    active_keymap_scope: ActiveKeymapScope,
    keystroke: crate::Keystroke,
    keystroke_routing: &KeystrokeRouting,
) -> RoutedCommands {
    world.resource_scope::<CompiledKeymap, _>(|world, mut compiled_keymap| {
        let now = world.resource::<KeymapRuntime>().now();
        let Some(match_outcome) =
            matcher(&mut compiled_keymap, active_keymap_scope).map(|sequence_matcher| {
                sequence_matcher.match_keystroke(keystroke, now, SEQUENCE_TIMEOUT)
            })
        else {
            return RoutedCommands::default();
        };
        let mut routed_commands = RoutedCommands::default();
        route_match_outcome(
            &mut compiled_keymap,
            active_keymap_scope,
            now,
            match_outcome,
            keystroke_routing,
            &mut routed_commands,
        );

        routed_commands
    })
}

fn route_match_outcome(
    compiled_keymap: &mut CompiledKeymap,
    active_keymap_scope: ActiveKeymapScope,
    now: Instant,
    match_outcome: MatchOutcome<CommandHandle>,
    keystroke_routing: &KeystrokeRouting,
    routed_commands: &mut RoutedCommands,
) {
    match match_outcome {
        MatchOutcome::Matched(command_handle) => {
            routed_commands.push(routed_command(
                compiled_keymap,
                command_handle,
                keystroke_routing,
            ));
        },
        MatchOutcome::Reprocess {
            deferred_match,
            keystroke,
        } => {
            if let DeferredMatch::Fire(command_handle) = deferred_match {
                routed_commands.push(routed_command(
                    compiled_keymap,
                    command_handle,
                    keystroke_routing,
                ));
            }
            if let Some(MatchOutcome::Matched(command_handle)) =
                matcher(compiled_keymap, active_keymap_scope).map(|sequence_matcher| {
                    sequence_matcher.match_keystroke(keystroke, now, SEQUENCE_TIMEOUT)
                })
            {
                routed_commands.push(routed_command(
                    compiled_keymap,
                    command_handle,
                    keystroke_routing,
                ));
            }
        },
        MatchOutcome::Deferred(_) | MatchOutcome::NoMatch | MatchOutcome::Pending => {},
    }
}

/// Resolves what a matched keystroke does, which is nothing at all while a text
/// field owns the keyboard and the matched command is not one it exempts.
fn routed_command(
    compiled_keymap: &CompiledKeymap,
    command_handle: CommandHandle,
    keystroke_routing: &KeystrokeRouting,
) -> RoutedCommand {
    if !compiled_keymap
        .command_id(command_handle)
        .is_some_and(|command_id| keystroke_routing.routes(command_id))
    {
        return RoutedCommand::Nothing;
    }
    match compiled_keymap.invocation(command_handle) {
        Some(Invocation::Held(custom_input)) => RoutedCommand::HoldChord(custom_input),
        Some(Invocation::OneShot | Invocation::Unremappable) => compiled_keymap
            .dispatch(command_handle)
            .map_or(RoutedCommand::Nothing, RoutedCommand::Dispatch),
        None => RoutedCommand::Nothing,
    }
}

fn write_custom_input_transition(
    world: &mut World,
    custom_input_transition: CustomInputTransition,
) {
    custom_input_transition.write_to(&mut world.resource_mut::<CustomInputs>());
}

fn release_all_physical_sources(world: &mut World) {
    loop {
        let physical_source_release_progress = world
            .resource_mut::<KeymapRuntime>()
            .release_one_physical_source();
        match physical_source_release_progress {
            PhysicalSourceReleaseProgress::ReleasedOne(custom_input_transition) => {
                write_custom_input_transition(world, custom_input_transition);
            },
            PhysicalSourceReleaseProgress::Complete => break,
        }
    }
}

fn release_chords_missing_modifiers(world: &mut World, pressed_modifiers: Modifiers) {
    loop {
        let physical_source_release_progress = world
            .resource_mut::<KeymapRuntime>()
            .release_one_chord_missing_modifiers(pressed_modifiers);
        match physical_source_release_progress {
            PhysicalSourceReleaseProgress::ReleasedOne(custom_input_transition) => {
                write_custom_input_transition(world, custom_input_transition);
            },
            PhysicalSourceReleaseProgress::Complete => break,
        }
    }
}

fn matcher(
    compiled_keymap: &mut CompiledKeymap,
    active_keymap_scope: ActiveKeymapScope,
) -> Option<&mut SequenceMatcher<CommandHandle>> {
    match active_keymap_scope {
        ActiveKeymapScope::Global => Some(&mut compiled_keymap.global),
        ActiveKeymapScope::Condition(condition_handle) => {
            compiled_keymap.matchers.get_mut(&condition_handle)
        },
    }
}

fn previous_matcher(
    compiled_keymap: &mut CompiledKeymap,
    active_matcher: ActiveMatcher,
) -> Option<&mut SequenceMatcher<CommandHandle>> {
    match active_matcher {
        ActiveMatcher::Uninitialized => None,
        ActiveMatcher::Global => Some(&mut compiled_keymap.global),
        ActiveMatcher::Condition(condition_handle) => {
            compiled_keymap.matchers.get_mut(&condition_handle)
        },
    }
}

fn dispatch_all(world: &mut World, routed_commands: RoutedCommands) {
    dispatch_one(world, routed_commands.first);
    dispatch_one(world, routed_commands.second);
}

fn dispatch_one(world: &mut World, routed_command: RoutedCommand) {
    match routed_command {
        RoutedCommand::Dispatch(dispatch) => dispatch(world),
        RoutedCommand::HoldChord(_) | RoutedCommand::Nothing => {},
    }
}

/// What a matched keystroke resolves to before the runtime acts on it.
///
/// Routing decides this from the compiled keymap alone; the caller then decides which halves it
/// honors, so the sequence-timeout path can drop a [`RoutedCommand::HoldChord`] that has no
/// physical key to own it.
#[derive(Clone, Copy, Default)]
enum RoutedCommand {
    #[default]
    Nothing,
    Dispatch(fn(&mut World)),
    HoldChord(CustomInput),
}

/// The commands one keystroke can resolve to.
///
/// A keystroke yields at most two: a sequence prefix that a longer sequence just abandoned, plus
/// the command the reprocessed keystroke matches on its own.
#[derive(Clone, Copy, Default)]
struct RoutedCommands {
    first:  RoutedCommand,
    second: RoutedCommand,
}

impl RoutedCommands {
    const fn push(&mut self, routed_command: RoutedCommand) {
        if matches!(routed_command, RoutedCommand::Nothing) {
            return;
        }
        if matches!(self.first, RoutedCommand::Nothing) {
            self.first = routed_command;
        } else {
            self.second = routed_command;
        }
    }
}

impl From<RoutedCommand> for RoutedCommands {
    fn from(routed_command: RoutedCommand) -> Self {
        let mut routed_commands = Self::default();
        routed_commands.push(routed_command);
        routed_commands
    }
}

#[cfg(test)]
#[allow(
    dead_code,
    reason = "runtime command declarations generate action marker types used through the registry"
)]
mod tests {
    use std::path::PathBuf;
    use std::time::Instant;

    use bevy::ecs::schedule::IntoScheduleConfigs;
    use bevy::ecs::spawn::SpawnRelated;
    use bevy::ecs::spawn::SpawnWith;
    use bevy::input::ButtonInput;
    use bevy::input::keyboard::KeyCode;
    use bevy::prelude::App;
    use bevy::prelude::Component;
    use bevy::prelude::Entity;
    use bevy::prelude::Event;
    use bevy::prelude::On;
    use bevy::prelude::PreUpdate;
    use bevy::prelude::Reflect;
    use bevy::prelude::ReflectEvent;
    use bevy::prelude::ResMut;
    use bevy::prelude::Resource;
    use bevy::prelude::With;
    use bevy::reflect::FromReflect;
    use bevy::reflect::TypeRegistry;
    use bevy::reflect::enums::DynamicEnum;
    use bevy::reflect::enums::DynamicVariant;
    use bevy::reflect::structs::DynamicStruct;
    use bevy::time::TimePlugin;
    use bevy_enhanced_input::bindings;
    use bevy_enhanced_input::prelude::Action;
    use bevy_enhanced_input::prelude::ActionSpawner;
    use bevy_enhanced_input::prelude::ActionValue;
    use bevy_enhanced_input::prelude::Actions;
    use bevy_enhanced_input::prelude::Binding;
    use bevy_enhanced_input::prelude::Complete;
    use bevy_enhanced_input::prelude::CustomInput;
    use bevy_enhanced_input::prelude::CustomInputs;
    use bevy_enhanced_input::prelude::EnhancedInputPlugin;
    use bevy_enhanced_input::prelude::InputAction;
    use bevy_enhanced_input::prelude::InputContextAppExt;
    use bevy_enhanced_input::prelude::Start;
    use strum::AsRefStr;
    use strum::EnumIter;
    use strum::EnumMessage;

    use super::KeymapRuntime;
    use super::route_input;
    use crate::ActiveCondition;
    use crate::CommandId;
    use crate::CommandRegistry;
    use crate::DiagnosticOrigin;
    use crate::HoldPhase;
    use crate::KeymapCommand;
    use crate::KeymapPlugin;
    use crate::KeymapSystems;
    use crate::KeystrokeSequence;
    use crate::ReflectKeymapCommand;
    use crate::SequenceMatcher;
    use crate::command::Invocation;
    use crate::condition::ConditionLookup;
    use crate::condition::ConditionName;
    use crate::condition::ConditionRegistry;
    use crate::keymap::CommandHandle;
    use crate::keymap::CompiledKeymap;
    use crate::keymap::Generation;
    use crate::keymap::KeyboardOwner;
    use crate::keymap::KeystrokeRouting;
    use crate::keymap::MergedKeymap;
    use crate::keymap::merged::UserKeymap;

    const DEFAULTS_PATH: &str = "runtime-defaults.jsonc";
    const FIRST_GENERATION: Generation = Generation(1);
    const SECOND_GENERATION: Generation = Generation(2);

    fn defaults_keymap_file() -> DiagnosticOrigin {
        DiagnosticOrigin::KeymapFile(PathBuf::from(DEFAULTS_PATH))
    }

    crate::command! {
        action:      RuntimeOneShotAction,
        event:       RuntimeOneShot,
        id:          "runtime::one_shot",
        title:       "Runtime One Shot",
        description: "Dispatches one event through the keymap runtime.",
    }

    #[derive(AsRefStr, Clone, Copy, EnumIter, EnumMessage, Eq, PartialEq, Resource)]
    #[strum(serialize_all = "snake_case")]
    enum SecondaryRuntimeContext {
        #[strum(message = "Routes a second runtime context when it is present.")]
        Secondary,
    }

    #[derive(AsRefStr, Clone, Copy, EnumIter, EnumMessage, Eq, PartialEq, Resource)]
    #[strum(serialize_all = "snake_case")]
    enum RuntimeContext {
        #[strum(message = "Routes while the runtime is flying.")]
        Flying,
        #[strum(message = "Routes while the runtime is paused.")]
        Paused,
    }

    #[derive(AsRefStr, Clone, Copy, EnumIter, EnumMessage, Eq, PartialEq, Resource)]
    #[strum(serialize_all = "snake_case")]
    enum UninitializedRuntimeContext {
        MissingDescription,
    }

    #[derive(Component)]
    struct RuntimeInputContext;

    crate::command! {
        action:      RuntimeTwoStrokeAction,
        event:       RuntimeTwoStroke,
        id:          "runtime::two_stroke",
        title:       "Runtime Two Stroke",
        description: "Dispatches after the second keymap stroke.",
    }

    crate::command! {
        held,
        action:      RuntimeHeldAction,
        event:       RuntimeHeld,
        id:          "runtime::held",
        title:       "Runtime Held",
        description: "Writes a custom input while the matched key is pressed.",
    }

    crate::command! {
        held,
        action:      RuntimeShiftHeldAction,
        event:       RuntimeShiftHeld,
        id:          "runtime::shift_held",
        title:       "Runtime Shift Held",
        description: "Writes a second custom input while its modifier family is pressed.",
    }

    crate::command! {
        held,
        action:      RuntimeAltHeldAction,
        event:       RuntimeAltHeld,
        id:          "runtime::alt_held",
        title:       "Runtime Alt Held",
        description: "Writes a third custom input while its modifier family is pressed.",
    }

    crate::command! {
        action:      RuntimeUnremappableAction,
        event:       RuntimeUnremappable,
        id:          "runtime::unremappable",
        title:       "Runtime Unremappable",
        description: "Dispatches through an opaque compiled command handle.",
        capability:  Unremappable,
    }

    #[derive(Debug, Default, Eq, PartialEq, Resource)]
    struct DispatchCounts {
        one_shot:     usize,
        two_stroke:   usize,
        unremappable: usize,
    }

    #[derive(Debug, Default, Eq, PartialEq, Resource)]
    struct HeldTransitionCounts {
        started:   usize,
        completed: usize,
    }

    #[test]
    fn single_stroke_dispatches_its_semantic_event() -> Result<(), String> {
        let mut app = runtime_app();
        insert_compiled(
            &mut app,
            bindings(&[("g", RuntimeOneShot::ID)]),
            FIRST_GENERATION,
        )?;

        press(&mut app, KeyCode::KeyG);

        assert_eq!(app.world().resource::<DispatchCounts>().one_shot, 1);
        Ok(())
    }

    #[test]
    fn same_frame_press_and_release_dispatches_one_shot() -> Result<(), String> {
        let mut app = runtime_app();
        insert_compiled(
            &mut app,
            bindings(&[("g", RuntimeOneShot::ID)]),
            FIRST_GENERATION,
        )?;
        {
            let mut key_input = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
            key_input.press(KeyCode::KeyG);
            key_input.release(KeyCode::KeyG);
        }

        route_input(app.world_mut());

        assert_eq!(app.world().resource::<DispatchCounts>().one_shot, 1);
        Ok(())
    }

    #[test]
    fn missing_compiled_keymap_returns_without_routing() {
        let mut app = runtime_app();

        route_input(app.world_mut());

        assert_eq!(app.world().resource::<DispatchCounts>().one_shot, 0);
    }

    #[test]
    fn two_stroke_sequence_dispatches_on_its_second_stroke() -> Result<(), String> {
        let mut app = runtime_app();
        insert_compiled(
            &mut app,
            bindings(&[("g h", RuntimeTwoStroke::ID)]),
            FIRST_GENERATION,
        )?;

        press(&mut app, KeyCode::KeyG);
        release(&mut app, KeyCode::KeyG);
        assert_eq!(app.world().resource::<DispatchCounts>().two_stroke, 0);
        press(&mut app, KeyCode::KeyH);

        assert_eq!(app.world().resource::<DispatchCounts>().two_stroke, 1);
        Ok(())
    }

    #[test]
    fn deferred_short_binding_dispatches_when_the_runtime_clock_reaches_the_timeout()
    -> Result<(), String> {
        let mut app = runtime_app();
        insert_compiled(
            &mut app,
            bindings(&[("g", RuntimeOneShot::ID), ("g h", RuntimeTwoStroke::ID)]),
            FIRST_GENERATION,
        )?;
        let now = Instant::now();
        app.world_mut()
            .resource_mut::<KeymapRuntime>()
            .set_test_clock(now);

        press(&mut app, KeyCode::KeyG);
        release(&mut app, KeyCode::KeyG);
        assert!(app.world().resource::<CompiledKeymap>().global.is_pending());

        app.world_mut()
            .resource_mut::<KeymapRuntime>()
            .set_test_clock(now + crate::keymap::constants::SEQUENCE_TIMEOUT);
        route_input(app.world_mut());

        assert_eq!(app.world().resource::<DispatchCounts>().one_shot, 1);
        assert!(!app.world().resource::<CompiledKeymap>().global.is_pending());
        Ok(())
    }

    #[test]
    fn held_binding_writes_its_custom_input_on_press_and_release() -> Result<(), String> {
        let mut app = runtime_app();
        insert_compiled(
            &mut app,
            bindings(&[("g", RuntimeHeld::ID)]),
            FIRST_GENERATION,
        )?;
        let custom_input = held_custom_input(&app)?;

        press(&mut app, KeyCode::KeyG);
        assert_eq!(
            app.world().resource::<CustomInputs>().get(&custom_input),
            Some(&ActionValue::Bool(true))
        );
        release(&mut app, KeyCode::KeyG);

        assert_eq!(
            app.world().resource::<CustomInputs>().get(&custom_input),
            Some(&ActionValue::Bool(false))
        );
        Ok(())
    }

    #[test]
    fn a_key_released_while_a_text_field_owns_the_keyboard_does_not_stay_held() -> Result<(), String>
    {
        let mut app = runtime_app();
        insert_compiled(
            &mut app,
            bindings(&[("g", RuntimeHeld::ID)]),
            FIRST_GENERATION,
        )?;
        let custom_input = held_custom_input(&app)?;

        press(&mut app, KeyCode::KeyG);
        assert_eq!(
            app.world().resource::<CustomInputs>().get(&custom_input),
            Some(&ActionValue::Bool(true))
        );

        app.world_mut()
            .insert_resource(KeystrokeRouting::text_entry(query_field(), []));
        route_input(app.world_mut());
        release(&mut app, KeyCode::KeyG);
        app.world_mut()
            .insert_resource(KeystrokeRouting::EveryBinding);
        route_input(app.world_mut());

        assert_eq!(
            app.world().resource::<CustomInputs>().get(&custom_input),
            Some(&ActionValue::Bool(false))
        );

        press(&mut app, KeyCode::KeyG);
        assert_eq!(
            app.world().resource::<CustomInputs>().get(&custom_input),
            Some(&ActionValue::Bool(true))
        );
        release(&mut app, KeyCode::KeyG);
        assert_eq!(
            app.world().resource::<CustomInputs>().get(&custom_input),
            Some(&ActionValue::Bool(false))
        );
        Ok(())
    }

    #[test]
    fn a_held_command_goes_false_at_the_handover_to_a_text_field() -> Result<(), String> {
        let mut app = runtime_app();
        insert_compiled(
            &mut app,
            bindings(&[("g", RuntimeHeld::ID)]),
            FIRST_GENERATION,
        )?;
        let custom_input = held_custom_input(&app)?;

        press(&mut app, KeyCode::KeyG);
        assert_eq!(
            app.world().resource::<CustomInputs>().get(&custom_input),
            Some(&ActionValue::Bool(true))
        );

        app.world_mut()
            .insert_resource(KeystrokeRouting::text_entry(query_field(), []));
        route_input(app.world_mut());

        assert_eq!(
            app.world().resource::<CustomInputs>().get(&custom_input),
            Some(&ActionValue::Bool(false))
        );
        Ok(())
    }

    /// The other direction of the same handover: an exempt hold-to-act command
    /// is the one held input a text field can leave active, so handing the
    /// keyboard back is where it goes false.
    #[test]
    fn a_held_command_goes_false_at_the_handover_back_to_the_keymap() -> Result<(), String> {
        let mut app = runtime_app();
        insert_compiled(
            &mut app,
            bindings(&[("g", RuntimeHeld::ID)]),
            FIRST_GENERATION,
        )?;
        let custom_input = held_custom_input(&app)?;
        app.world_mut()
            .insert_resource(KeystrokeRouting::text_entry(
                query_field(),
                [CommandId::declared::<RuntimeHeld>()],
            ));
        route_input(app.world_mut());

        press(&mut app, KeyCode::KeyG);
        assert_eq!(
            app.world().resource::<CustomInputs>().get(&custom_input),
            Some(&ActionValue::Bool(true))
        );

        app.world_mut()
            .insert_resource(KeystrokeRouting::EveryBinding);
        route_input(app.world_mut());

        assert_eq!(
            app.world().resource::<CustomInputs>().get(&custom_input),
            Some(&ActionValue::Bool(false))
        );
        Ok(())
    }

    #[test]
    fn a_pending_sequence_is_cancelled_at_the_handover_to_a_text_field() -> Result<(), String> {
        let mut app = runtime_app();
        insert_compiled(
            &mut app,
            bindings(&[("g h", RuntimeTwoStroke::ID)]),
            FIRST_GENERATION,
        )?;

        press(&mut app, KeyCode::KeyG);
        release(&mut app, KeyCode::KeyG);
        assert_eq!(app.world().resource::<DispatchCounts>().two_stroke, 0);

        app.world_mut()
            .insert_resource(KeystrokeRouting::text_entry(
                query_field(),
                [CommandId::declared::<RuntimeTwoStroke>()],
            ));
        route_input(app.world_mut());
        press(&mut app, KeyCode::KeyH);
        release(&mut app, KeyCode::KeyH);

        assert_eq!(app.world().resource::<DispatchCounts>().two_stroke, 0);
        Ok(())
    }

    /// The other direction of the same handover: an exempt multi-stroke command
    /// can leave a sequence pending while the field owns the keyboard, and
    /// handing the keyboard back cancels it.
    #[test]
    fn a_pending_sequence_is_cancelled_at_the_handover_back_to_the_keymap() -> Result<(), String> {
        let mut app = runtime_app();
        insert_compiled(
            &mut app,
            bindings(&[("g h", RuntimeTwoStroke::ID)]),
            FIRST_GENERATION,
        )?;
        app.world_mut()
            .insert_resource(KeystrokeRouting::text_entry(
                query_field(),
                [CommandId::declared::<RuntimeTwoStroke>()],
            ));
        route_input(app.world_mut());

        press(&mut app, KeyCode::KeyG);
        release(&mut app, KeyCode::KeyG);
        assert!(app.world().resource::<CompiledKeymap>().global.is_pending());

        app.world_mut()
            .insert_resource(KeystrokeRouting::EveryBinding);
        route_input(app.world_mut());
        press(&mut app, KeyCode::KeyH);
        release(&mut app, KeyCode::KeyH);

        assert!(!app.world().resource::<CompiledKeymap>().global.is_pending());
        assert_eq!(app.world().resource::<DispatchCounts>().two_stroke, 0);
        Ok(())
    }

    /// The other direction of the same handover. The inhibition is asserted on
    /// [`KeymapRuntime`] directly because a text field suppresses the
    /// bare-modifier held bindings outright, so nothing downstream of it can
    /// tell an inhibited key from a suppressed one.
    #[test]
    fn a_key_down_at_the_handover_to_a_text_field_is_inhibited() -> Result<(), String> {
        let mut app = runtime_app();
        insert_compiled(
            &mut app,
            bindings(&[("ctrl", RuntimeHeld::ID)]),
            FIRST_GENERATION,
        )?;
        let custom_input = held_custom_input(&app)?;

        press(&mut app, KeyCode::ControlLeft);
        assert_eq!(
            app.world().resource::<CustomInputs>().get(&custom_input),
            Some(&ActionValue::Bool(true))
        );

        app.world_mut()
            .insert_resource(KeystrokeRouting::text_entry(query_field(), []));
        route_input(app.world_mut());

        assert!(
            app.world()
                .resource::<KeymapRuntime>()
                .is_inhibited(KeyCode::ControlLeft)
        );
        assert_eq!(
            app.world().resource::<CustomInputs>().get(&custom_input),
            Some(&ActionValue::Bool(false))
        );
        Ok(())
    }

    #[test]
    fn a_key_down_at_the_handover_back_to_the_keymap_stays_inhibited_until_released()
    -> Result<(), String> {
        let mut app = runtime_app();
        insert_compiled(
            &mut app,
            bindings(&[("ctrl", RuntimeHeld::ID)]),
            FIRST_GENERATION,
        )?;
        let custom_input = held_custom_input(&app)?;
        app.world_mut()
            .insert_resource(KeystrokeRouting::text_entry(query_field(), []));
        route_input(app.world_mut());

        press(&mut app, KeyCode::ControlLeft);
        assert_ne!(
            app.world().resource::<CustomInputs>().get(&custom_input),
            Some(&ActionValue::Bool(true))
        );

        app.world_mut()
            .insert_resource(KeystrokeRouting::EveryBinding);
        route_input(app.world_mut());
        route_input(app.world_mut());
        assert_ne!(
            app.world().resource::<CustomInputs>().get(&custom_input),
            Some(&ActionValue::Bool(true))
        );

        release(&mut app, KeyCode::ControlLeft);
        press(&mut app, KeyCode::ControlLeft);

        assert_eq!(
            app.world().resource::<CustomInputs>().get(&custom_input),
            Some(&ActionValue::Bool(true))
        );
        Ok(())
    }

    #[test]
    fn text_entry_routes_the_commands_it_exempts_and_no_others() -> Result<(), String> {
        let mut app = runtime_app();
        insert_compiled(
            &mut app,
            bindings(&[("g", RuntimeOneShot::ID), ("h", RuntimeTwoStroke::ID)]),
            FIRST_GENERATION,
        )?;
        app.world_mut()
            .insert_resource(KeystrokeRouting::text_entry(
                query_field(),
                [CommandId::declared::<RuntimeOneShot>()],
            ));
        route_input(app.world_mut());

        press(&mut app, KeyCode::KeyG);
        release(&mut app, KeyCode::KeyG);
        press(&mut app, KeyCode::KeyH);
        release(&mut app, KeyCode::KeyH);

        let dispatch_counts = app.world().resource::<DispatchCounts>();
        assert_eq!(dispatch_counts.one_shot, 1);
        assert_eq!(dispatch_counts.two_stroke, 0);
        Ok(())
    }

    #[test]
    fn same_frame_press_and_release_leaves_held_input_inactive() -> Result<(), String> {
        let mut app = runtime_app();
        insert_compiled(
            &mut app,
            bindings(&[("g", RuntimeHeld::ID)]),
            FIRST_GENERATION,
        )?;
        let custom_input = held_custom_input(&app)?;
        {
            let mut key_input = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
            key_input.press(KeyCode::KeyG);
            key_input.release(KeyCode::KeyG);
        }

        route_input(app.world_mut());

        assert_eq!(
            app.world().resource::<CustomInputs>().get(&custom_input),
            Some(&ActionValue::Bool(false))
        );
        Ok(())
    }

    #[test]
    fn modifier_family_binding_counts_left_and_right_shift_as_one_hold() -> Result<(), String> {
        let mut app = runtime_app();
        insert_compiled(
            &mut app,
            bindings(&[("shift", RuntimeHeld::ID)]),
            FIRST_GENERATION,
        )?;
        let custom_input = held_custom_input(&app)?;
        spawn_held_action(&mut app, custom_input)?;

        press(&mut app, KeyCode::ShiftLeft);
        app.update();
        assert_eq!(
            app.world().resource::<CustomInputs>().get(&custom_input),
            Some(&ActionValue::Bool(true))
        );
        press(&mut app, KeyCode::ShiftRight);
        release(&mut app, KeyCode::ShiftLeft);
        app.update();
        assert_eq!(
            app.world().resource::<CustomInputs>().get(&custom_input),
            Some(&ActionValue::Bool(true))
        );
        release(&mut app, KeyCode::ShiftRight);
        app.update();

        assert_eq!(
            app.world().resource::<CustomInputs>().get(&custom_input),
            Some(&ActionValue::Bool(false))
        );
        assert_eq!(
            *app.world().resource::<HeldTransitionCounts>(),
            HeldTransitionCounts {
                started:   1,
                completed: 1,
            }
        );
        Ok(())
    }

    #[test]
    fn shifted_key_suspends_its_bare_hold_until_the_key_is_released() -> Result<(), String> {
        let mut app = runtime_app();
        insert_compiled(
            &mut app,
            bindings(&[("shift", RuntimeHeld::ID), ("shift-f", RuntimeOneShot::ID)]),
            FIRST_GENERATION,
        )?;
        let custom_input = held_custom_input(&app)?;

        press(&mut app, KeyCode::ShiftLeft);
        assert_eq!(
            app.world().resource::<CustomInputs>().get(&custom_input),
            Some(&ActionValue::Bool(true))
        );
        press(&mut app, KeyCode::KeyF);

        assert_eq!(app.world().resource::<DispatchCounts>().one_shot, 1);
        assert_eq!(
            app.world().resource::<CustomInputs>().get(&custom_input),
            Some(&ActionValue::Bool(false))
        );
        release(&mut app, KeyCode::KeyF);
        assert_eq!(
            app.world().resource::<CustomInputs>().get(&custom_input),
            Some(&ActionValue::Bool(true))
        );
        release(&mut app, KeyCode::ShiftLeft);
        assert_eq!(
            app.world().resource::<CustomInputs>().get(&custom_input),
            Some(&ActionValue::Bool(false))
        );
        Ok(())
    }

    #[test]
    fn unroutable_key_press_leaves_a_bare_modifier_hold_active() -> Result<(), String> {
        let mut app = runtime_app();
        insert_compiled(
            &mut app,
            bindings(&[("shift", RuntimeHeld::ID)]),
            FIRST_GENERATION,
        )?;
        let custom_input = held_custom_input(&app)?;
        spawn_held_action(&mut app, custom_input)?;

        press(&mut app, KeyCode::ShiftLeft);
        assert_eq!(
            app.world().resource::<CustomInputs>().get(&custom_input),
            Some(&ActionValue::Bool(true))
        );
        press(&mut app, KeyCode::AudioVolumeUp);
        assert_eq!(
            app.world().resource::<CustomInputs>().get(&custom_input),
            Some(&ActionValue::Bool(true))
        );
        app.update();

        assert_eq!(
            app.world().resource::<CustomInputs>().get(&custom_input),
            Some(&ActionValue::Bool(true))
        );
        assert_eq!(
            *app.world().resource::<HeldTransitionCounts>(),
            HeldTransitionCounts {
                started:   1,
                completed: 0,
            }
        );
        Ok(())
    }

    #[test]
    fn same_frame_shift_and_key_press_never_activates_the_bare_hold() -> Result<(), String> {
        let mut app = runtime_app();
        insert_compiled(
            &mut app,
            bindings(&[("shift", RuntimeHeld::ID), ("shift-f", RuntimeOneShot::ID)]),
            FIRST_GENERATION,
        )?;
        let custom_input = held_custom_input(&app)?;
        spawn_held_action(&mut app, custom_input)?;
        {
            let mut pressed = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
            pressed.press(KeyCode::ShiftLeft);
            pressed.press(KeyCode::KeyF);
        }

        route_input(app.world_mut());
        {
            let mut pressed = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
            pressed.clear_just_pressed(KeyCode::ShiftLeft);
            pressed.clear_just_pressed(KeyCode::KeyF);
        }
        app.update();

        assert_eq!(app.world().resource::<DispatchCounts>().one_shot, 1);
        assert_eq!(
            app.world().resource::<CustomInputs>().get(&custom_input),
            None
        );
        assert_eq!(
            *app.world().resource::<HeldTransitionCounts>(),
            HeldTransitionCounts::default()
        );
        Ok(())
    }

    #[test]
    fn modified_held_chord_ends_when_primary_key_is_released_first() -> Result<(), String> {
        let mut app = runtime_app();
        insert_compiled(
            &mut app,
            bindings(&[("shift-f", RuntimeHeld::ID)]),
            FIRST_GENERATION,
        )?;
        let custom_input = held_custom_input(&app)?;

        press(&mut app, KeyCode::ShiftLeft);
        press(&mut app, KeyCode::KeyF);
        assert_eq!(
            app.world().resource::<CustomInputs>().get(&custom_input),
            Some(&ActionValue::Bool(true))
        );

        release(&mut app, KeyCode::KeyF);

        assert_eq!(
            app.world().resource::<CustomInputs>().get(&custom_input),
            Some(&ActionValue::Bool(false))
        );
        assert!(
            app.world()
                .resource::<ButtonInput<KeyCode>>()
                .pressed(KeyCode::ShiftLeft)
        );
        Ok(())
    }

    #[test]
    fn modified_held_chord_ends_when_required_modifier_is_released_first() -> Result<(), String> {
        let mut app = runtime_app();
        insert_compiled(
            &mut app,
            bindings(&[("shift-f", RuntimeHeld::ID)]),
            FIRST_GENERATION,
        )?;
        let custom_input = held_custom_input(&app)?;

        press(&mut app, KeyCode::ShiftLeft);
        press(&mut app, KeyCode::KeyF);
        assert_eq!(
            app.world().resource::<CustomInputs>().get(&custom_input),
            Some(&ActionValue::Bool(true))
        );

        release(&mut app, KeyCode::ShiftLeft);
        route_input(app.world_mut());

        assert_eq!(
            app.world().resource::<CustomInputs>().get(&custom_input),
            Some(&ActionValue::Bool(false))
        );
        assert!(
            app.world()
                .resource::<ButtonInput<KeyCode>>()
                .pressed(KeyCode::KeyF)
        );
        Ok(())
    }

    #[test]
    fn event_source_release_keeps_a_physical_held_binding_active() -> Result<(), String> {
        let mut app = runtime_app();
        insert_compiled(
            &mut app,
            bindings(&[("g", RuntimeHeld::ID)]),
            FIRST_GENERATION,
        )?;
        let custom_input = held_custom_input(&app)?;

        press(&mut app, KeyCode::KeyG);
        app.world_mut().trigger(RuntimeHeld {
            phase: HoldPhase::Begin,
        });
        app.world_mut().trigger(RuntimeHeld {
            phase: HoldPhase::End,
        });

        assert_eq!(
            app.world().resource::<CustomInputs>().get(&custom_input),
            Some(&ActionValue::Bool(true))
        );
        release(&mut app, KeyCode::KeyG);
        assert_eq!(
            app.world().resource::<CustomInputs>().get(&custom_input),
            Some(&ActionValue::Bool(false))
        );
        Ok(())
    }

    #[test]
    fn event_source_remains_active_across_a_generation_change() -> Result<(), String> {
        let mut app = runtime_app();
        let command_registry = command_registry(&mut app)?;
        let first = compile(
            bindings(&[("g", RuntimeHeld::ID)]),
            &command_registry,
            FIRST_GENERATION,
        )?;
        let second = compile(
            bindings(&[("h", RuntimeHeld::ID)]),
            &command_registry,
            SECOND_GENERATION,
        )?;
        app.world_mut().insert_resource(first);
        app.world_mut().init_resource::<KeymapRuntime>();
        let custom_input = held_custom_input(&app)?;
        route_input(app.world_mut());

        app.world_mut().trigger(RuntimeHeld {
            phase: HoldPhase::Begin,
        });
        app.world_mut().insert_resource(second);
        route_input(app.world_mut());

        assert_eq!(
            app.world().resource::<CustomInputs>().get(&custom_input),
            Some(&ActionValue::Bool(true))
        );
        Ok(())
    }

    #[test]
    fn held_binding_drives_one_start_and_complete_through_enhanced_input() -> Result<(), String> {
        let mut app = runtime_app();
        insert_compiled(
            &mut app,
            bindings(&[("g", RuntimeHeld::ID)]),
            FIRST_GENERATION,
        )?;
        let custom_input = held_custom_input(&app)?;
        spawn_held_action(&mut app, custom_input)?;

        press(&mut app, KeyCode::KeyG);
        app.update();
        release(&mut app, KeyCode::KeyG);
        app.update();

        assert_eq!(
            *app.world().resource::<HeldTransitionCounts>(),
            HeldTransitionCounts {
                started:   1,
                completed: 1,
            }
        );
        Ok(())
    }

    #[test]
    fn remapping_an_unpressed_held_binding_changes_its_source_without_replacing_its_action()
    -> Result<(), String> {
        let mut app = runtime_app();
        let command_registry = command_registry(&mut app)?;
        let first = compile(
            bindings(&[("g", RuntimeHeld::ID)]),
            &command_registry,
            FIRST_GENERATION,
        )?;
        let second = compile(
            bindings(&[("h", RuntimeHeld::ID)]),
            &command_registry,
            SECOND_GENERATION,
        )?;
        app.world_mut().insert_resource(first);
        app.world_mut().init_resource::<KeymapRuntime>();
        let custom_input = held_custom_input(&app)?;
        let action_entity = spawn_held_action(&mut app, custom_input)?;

        press(&mut app, KeyCode::KeyG);
        release(&mut app, KeyCode::KeyG);
        app.world_mut().insert_resource(second);
        press(&mut app, KeyCode::KeyG);
        assert_eq!(
            app.world().resource::<CustomInputs>().get(&custom_input),
            Some(&ActionValue::Bool(false))
        );
        release(&mut app, KeyCode::KeyG);
        press(&mut app, KeyCode::KeyH);

        assert_eq!(
            app.world().resource::<CustomInputs>().get(&custom_input),
            Some(&ActionValue::Bool(true))
        );
        assert!(app.world().get_entity(action_entity).is_ok());
        Ok(())
    }

    #[test]
    fn context_change_cancels_the_previous_matcher_pending_sequence() -> Result<(), String> {
        let mut app = runtime_app();
        app.insert_resource(RuntimeContext::Flying)
            .add_plugins(KeymapPlugin::new().for_context::<RuntimeContext>());
        app.update();
        let command_registry = command_registry(&mut app)?;
        let condition_registry = app.world().resource::<ConditionRegistry>();
        let compiled_keymap = compile_with_conditions(
            r#"{
                "bindings": [
                    { "context": "flying", "bindings": {
                        "g": "runtime::one_shot",
                        "g h": "runtime::two_stroke"
                    }}
                ]
            }"#,
            &command_registry,
            condition_registry,
            FIRST_GENERATION,
        )?;
        let ConditionLookup::Registered { handle: flying, .. } =
            condition_registry.lookup("flying")
        else {
            return Err(String::from("runtime context did not register flying"));
        };
        app.world_mut().insert_resource(compiled_keymap);

        press(&mut app, KeyCode::KeyG);
        release(&mut app, KeyCode::KeyG);
        assert!(
            app.world()
                .resource::<CompiledKeymap>()
                .matchers
                .get(&flying)
                .is_some_and(SequenceMatcher::is_pending)
        );

        *app.world_mut().resource_mut::<RuntimeContext>() = RuntimeContext::Paused;
        app.update();

        assert_eq!(app.world().resource::<DispatchCounts>().one_shot, 0);
        assert!(
            !app.world()
                .resource::<CompiledKeymap>()
                .matchers
                .get(&flying)
                .is_some_and(SequenceMatcher::is_pending)
        );
        Ok(())
    }

    #[test]
    fn uninitialized_context_routing_returns_without_dispatching() -> Result<(), String> {
        let mut app = runtime_app();
        app.add_plugins(KeymapPlugin::new().for_context::<UninitializedRuntimeContext>());
        insert_compiled(
            &mut app,
            bindings(&[("g", RuntimeOneShot::ID)]),
            FIRST_GENERATION,
        )?;

        press(&mut app, KeyCode::KeyG);

        assert_eq!(app.world().resource::<DispatchCounts>().one_shot, 0);
        Ok(())
    }

    #[test]
    fn preupdate_context_changes_route_held_input_before_enhanced_input_updates()
    -> Result<(), String> {
        let mut app = runtime_app();
        app.insert_resource(RuntimeContext::Flying)
            .add_plugins(KeymapPlugin::new().for_context::<RuntimeContext>());
        app.update();
        let command_registry = command_registry(&mut app)?;
        let condition_registry = app.world().resource::<ConditionRegistry>();
        let compiled_keymap = compile_with_conditions(
            r#"{
                "bindings": [
                    { "context": "paused", "bindings": { "g": "runtime::held" }}
                ]
            }"#,
            &command_registry,
            condition_registry,
            FIRST_GENERATION,
        )?;
        let custom_input = held_custom_input_from_compiled(&compiled_keymap)?;
        spawn_held_action(&mut app, custom_input)?;
        app.world_mut().insert_resource(compiled_keymap);
        app.add_systems(
            PreUpdate,
            pause_runtime_context.before(KeymapSystems::UpdateActiveCondition),
        );
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyG);

        app.update();

        assert_eq!(
            app.world().resource::<CustomInputs>().get(&custom_input),
            Some(&ActionValue::Bool(true))
        );
        assert_eq!(app.world().resource::<HeldTransitionCounts>().started, 1);
        Ok(())
    }

    #[test]
    fn generation_change_inhibits_already_pressed_keys_until_release() -> Result<(), String> {
        let mut app = runtime_app();
        insert_compiled(
            &mut app,
            bindings(&[("g", RuntimeOneShot::ID)]),
            FIRST_GENERATION,
        )?;

        route_input(app.world_mut());
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyG);
        let command_registry = command_registry(&mut app)?;
        let replacement = compile(
            bindings(&[("g", RuntimeOneShot::ID)]),
            &command_registry,
            SECOND_GENERATION,
        )?;
        app.world_mut().insert_resource(replacement);
        route_input(app.world_mut());
        assert_eq!(app.world().resource::<DispatchCounts>().one_shot, 0);
        assert!(
            app.world()
                .resource::<KeymapRuntime>()
                .is_inhibited(KeyCode::KeyG)
        );
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .clear_just_pressed(KeyCode::KeyG);

        release(&mut app, KeyCode::KeyG);
        press(&mut app, KeyCode::KeyG);
        assert_eq!(app.world().resource::<DispatchCounts>().one_shot, 1);
        Ok(())
    }

    #[test]
    fn recovery_cancellation_clears_a_pending_global_sequence() -> Result<(), String> {
        let mut app = runtime_app();
        insert_compiled(
            &mut app,
            bindings(&[("g", RuntimeOneShot::ID), ("g h", RuntimeTwoStroke::ID)]),
            FIRST_GENERATION,
        )?;

        press(&mut app, KeyCode::KeyG);
        release(&mut app, KeyCode::KeyG);
        assert!(app.world().resource::<CompiledKeymap>().global.is_pending());

        crate::cancel_pending_sequences(app.world_mut());

        assert!(!app.world().resource::<CompiledKeymap>().global.is_pending());
        press(&mut app, KeyCode::KeyH);
        assert_eq!(app.world().resource::<DispatchCounts>().two_stroke, 0);
        Ok(())
    }

    #[test]
    fn physical_input_reset_cancels_sequences_and_refreshes_held_state() -> Result<(), String> {
        let mut app = runtime_app();
        insert_compiled(
            &mut app,
            bindings(&[("g", RuntimeHeld::ID), ("h j", RuntimeTwoStroke::ID)]),
            FIRST_GENERATION,
        )?;
        let custom_input = held_custom_input(&app)?;

        press(&mut app, KeyCode::KeyG);
        press(&mut app, KeyCode::KeyH);
        release(&mut app, KeyCode::KeyH);
        assert!(app.world().resource::<CompiledKeymap>().global.is_pending());
        assert_eq!(
            app.world().resource::<CustomInputs>().get(&custom_input),
            Some(&ActionValue::Bool(true))
        );

        crate::reset_physical_input(app.world_mut());

        assert!(!app.world().resource::<CompiledKeymap>().global.is_pending());
        assert_eq!(
            app.world().resource::<CustomInputs>().get(&custom_input),
            Some(&ActionValue::Bool(false))
        );
        assert!(
            app.world()
                .resource::<KeymapRuntime>()
                .is_inhibited(KeyCode::KeyG)
        );
        press(&mut app, KeyCode::KeyJ);
        assert_eq!(app.world().resource::<DispatchCounts>().two_stroke, 0);

        app.world_mut().trigger(RuntimeHeld {
            phase: HoldPhase::Begin,
        });
        crate::reset_physical_input(app.world_mut());
        assert_eq!(
            app.world().resource::<CustomInputs>().get(&custom_input),
            Some(&ActionValue::Bool(true))
        );

        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .release(KeyCode::KeyG);
        crate::reset_physical_input(app.world_mut());
        assert!(
            !app.world()
                .resource::<KeymapRuntime>()
                .is_inhibited(KeyCode::KeyG)
        );
        app.world_mut().trigger(RuntimeHeld {
            phase: HoldPhase::End,
        });
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .clear_just_released(KeyCode::KeyG);
        press(&mut app, KeyCode::KeyG);

        assert_eq!(
            app.world().resource::<CustomInputs>().get(&custom_input),
            Some(&ActionValue::Bool(true))
        );
        Ok(())
    }

    #[test]
    fn keymap_plugin_routes_global_bindings_without_a_context_plugin() -> Result<(), String> {
        let mut app = runtime_app();
        app.add_plugins(KeymapPlugin::new());
        insert_compiled(
            &mut app,
            bindings(&[("g", RuntimeOneShot::ID)]),
            FIRST_GENERATION,
        )?;
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyG);

        app.update();

        assert_eq!(app.world().resource::<DispatchCounts>().one_shot, 1);
        Ok(())
    }

    #[test]
    fn keymap_plugin_and_two_context_plugins_route_one_press_once() -> Result<(), String> {
        let mut app = runtime_app();
        app.insert_resource(RuntimeContext::Flying)
            .add_plugins(KeymapPlugin::new().for_context::<RuntimeContext>())
            .add_plugins(KeymapPlugin::new().for_context::<SecondaryRuntimeContext>())
            .add_plugins(KeymapPlugin::new());
        let command_registry = command_registry(&mut app)?;
        let condition_registry = app.world().resource::<ConditionRegistry>();
        let compiled_keymap = compile_with_conditions(
            &bindings(&[("g", RuntimeOneShot::ID)]),
            &command_registry,
            condition_registry,
            FIRST_GENERATION,
        )?;
        app.world_mut().insert_resource(compiled_keymap);
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyG);

        app.update();

        assert_eq!(app.world().resource::<DispatchCounts>().one_shot, 1);
        Ok(())
    }

    #[test]
    fn modifier_edges_leave_pending_sequences_armed() -> Result<(), String> {
        let mut app = runtime_app();
        insert_compiled(
            &mut app,
            bindings(&[("g", RuntimeOneShot::ID), ("g h", RuntimeTwoStroke::ID)]),
            FIRST_GENERATION,
        )?;

        press(&mut app, KeyCode::KeyG);
        release(&mut app, KeyCode::KeyG);
        press(&mut app, KeyCode::ShiftLeft);

        assert!(app.world().resource::<CompiledKeymap>().global.is_pending());
        assert_eq!(app.world().resource::<DispatchCounts>().one_shot, 0);
        Ok(())
    }

    #[test]
    fn physical_super_prevents_a_bare_binding_from_dispatching() -> Result<(), String> {
        let mut app = runtime_app();
        insert_compiled(
            &mut app,
            bindings(&[("g", RuntimeOneShot::ID)]),
            FIRST_GENERATION,
        )?;

        press(&mut app, KeyCode::SuperLeft);
        press(&mut app, KeyCode::KeyG);

        assert_eq!(app.world().resource::<DispatchCounts>().one_shot, 0);
        Ok(())
    }

    #[test]
    fn unremappable_entries_dispatch_through_the_compiled_function_pointer() -> Result<(), String> {
        let mut app = runtime_app();
        insert_compiled(
            &mut app,
            bindings(&[("g", RuntimeOneShot::ID)]),
            FIRST_GENERATION,
        )?;
        let command_handle = app
            .world()
            .resource::<CompiledKeymap>()
            .commands
            .iter()
            .position(|(_, command_entry)| {
                matches!(command_entry.invocation(), Invocation::Unremappable)
            })
            .map(CommandHandle::from_index)
            .ok_or_else(|| "runtime registry did not compile its unremappable entry".to_owned())?;
        let sequence = "u"
            .parse::<KeystrokeSequence>()
            .map_err(|error| format!("invalid unremappable test sequence: {error}"))?;
        app.world_mut().resource_mut::<CompiledKeymap>().global =
            SequenceMatcher::new([(sequence, command_handle)]);

        press(&mut app, KeyCode::KeyU);

        assert_eq!(app.world().resource::<DispatchCounts>().unremappable, 1);
        Ok(())
    }

    #[test]
    fn steady_state_held_routing_does_not_allocate() -> Result<(), String> {
        let mut app = runtime_app();
        insert_compiled(
            &mut app,
            bindings(&[("g", RuntimeHeld::ID)]),
            FIRST_GENERATION,
        )?;

        press(&mut app, KeyCode::KeyG);
        release(&mut app, KeyCode::KeyG);
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyG);
        let allocations_before = crate::TEST_ALLOCATOR.allocation_count();
        route_input(app.world_mut());
        let allocations_after = crate::TEST_ALLOCATOR.allocation_count();

        assert_eq!(allocations_after - allocations_before, 0);
        Ok(())
    }

    /// The warm-up presses and releases the same four keys together so every routing structure is
    /// already populated, and takes one empty [`World::resource_scope`] because Bevy's own
    /// first-touch cost lands there; the measured pass then reports only what routing allocates
    /// while it activates all four keys at once.
    #[test]
    fn simultaneous_modifier_family_held_routing_does_not_allocate() -> Result<(), String> {
        const MODIFIER_KEYS: [KeyCode; 4] = [
            KeyCode::ControlLeft,
            KeyCode::ControlRight,
            KeyCode::ShiftLeft,
            KeyCode::AltLeft,
        ];

        let mut app = runtime_app();
        insert_compiled_for_modifier_family_holds(
            &mut app,
            bindings(&[
                ("ctrl", RuntimeHeld::ID),
                ("shift", RuntimeShiftHeld::ID),
                ("alt", RuntimeAltHeld::ID),
            ]),
            FIRST_GENERATION,
        )?;

        press_together(&mut app, MODIFIER_KEYS);
        release_together(&mut app, MODIFIER_KEYS);
        for key in MODIFIER_KEYS {
            app.world_mut()
                .resource_mut::<ButtonInput<KeyCode>>()
                .press(key);
        }
        app.world_mut()
            .resource_scope::<CompiledKeymap, _>(|_world, _compiled_keymap| {});

        let allocations_before = crate::TEST_ALLOCATOR.allocation_count();
        route_input(app.world_mut());
        let allocations_after = crate::TEST_ALLOCATOR.allocation_count();

        assert_eq!(allocations_after - allocations_before, 0);
        Ok(())
    }

    #[test]
    fn same_frame_held_tap_does_not_allocate() -> Result<(), String> {
        let mut app = runtime_app();
        insert_compiled(
            &mut app,
            bindings(&[("g", RuntimeHeld::ID)]),
            FIRST_GENERATION,
        )?;
        let custom_input = held_custom_input(&app)?;

        press(&mut app, KeyCode::KeyG);
        release(&mut app, KeyCode::KeyG);
        {
            let mut key_input = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
            key_input.press(KeyCode::KeyG);
            key_input.release(KeyCode::KeyG);
        }
        let allocations_before = crate::TEST_ALLOCATOR.allocation_count();
        route_input(app.world_mut());
        let allocations_after = crate::TEST_ALLOCATOR.allocation_count();

        assert_eq!(allocations_after - allocations_before, 0);
        assert_eq!(
            app.world().resource::<CustomInputs>().get(&custom_input),
            Some(&ActionValue::Bool(false))
        );
        Ok(())
    }

    /// A remote client can rebuild `ActiveCondition` from a name alone, which leaves the ignored
    /// handle field at `ConditionHandle::default`. Routing must refuse that handle instead of
    /// selecting whichever condition registered first — here `flying`, the condition that binds
    /// `g`.
    #[test]
    fn a_reflected_active_condition_does_not_route_through_the_first_condition()
    -> Result<(), String> {
        let mut app = runtime_app();
        app.insert_resource(RuntimeContext::Flying)
            .add_plugins(KeymapPlugin::new().for_context::<RuntimeContext>());
        app.update();
        let command_registry = command_registry(&mut app)?;
        let condition_registry = app.world().resource::<ConditionRegistry>();
        let compiled_keymap = compile_with_conditions(
            r#"{
                "bindings": [
                    { "context": "flying", "bindings": { "g": "runtime::one_shot" }}
                ]
            }"#,
            &command_registry,
            condition_registry,
            FIRST_GENERATION,
        )?;
        let ConditionLookup::Registered { handle: flying, .. } =
            condition_registry.lookup("flying")
        else {
            return Err(String::from("runtime context did not register flying"));
        };
        app.world_mut().insert_resource(compiled_keymap);
        app.world_mut()
            .insert_resource(reflected_active_condition("flying")?);

        press(&mut app, KeyCode::KeyG);

        assert_eq!(app.world().resource::<DispatchCounts>().one_shot, 0);

        release(&mut app, KeyCode::KeyG);
        app.world_mut()
            .resource_mut::<ActiveCondition>()
            .resolve(flying, &ConditionName::new("flying"));
        press(&mut app, KeyCode::KeyG);

        assert_eq!(app.world().resource::<DispatchCounts>().one_shot, 1);
        Ok(())
    }

    /// Rebuilds `ActiveCondition` the way a remote `insert_resource` does: through reflection,
    /// carrying only the condition name.
    fn reflected_active_condition(condition_name: &str) -> Result<ActiveCondition, String> {
        let mut resolved_condition = DynamicStruct::default();
        resolved_condition.insert("name", ConditionName::new(condition_name));
        let mut active_condition_state = DynamicEnum::default();
        active_condition_state.set_variant(
            "ResolvedCondition",
            DynamicVariant::Struct(resolved_condition),
        );
        let mut reflected_active_condition = DynamicStruct::default();
        reflected_active_condition.insert("state", active_condition_state);

        ActiveCondition::from_reflect(&reflected_active_condition)
            .ok_or_else(|| String::from("ActiveCondition did not rebuild through reflection"))
    }

    /// The text field the handover tests hand the keyboard to.
    struct QueryField;

    fn query_field() -> KeyboardOwner { KeyboardOwner::of::<QueryField>() }

    fn runtime_app() -> App {
        let mut app = App::new();
        app.init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<CustomInputs>()
            .init_resource::<DispatchCounts>()
            .init_resource::<HeldTransitionCounts>();
        app.world_mut().add_observer(
            |_: On<RuntimeOneShot>, mut dispatch_counts: ResMut<DispatchCounts>| {
                dispatch_counts.one_shot += 1;
            },
        );
        app.world_mut().add_observer(
            |_: On<RuntimeTwoStroke>, mut dispatch_counts: ResMut<DispatchCounts>| {
                dispatch_counts.two_stroke += 1;
            },
        );
        app.world_mut().add_observer(
            |_: On<RuntimeUnremappable>, mut dispatch_counts: ResMut<DispatchCounts>| {
                dispatch_counts.unremappable += 1;
            },
        );
        app.world_mut().add_observer(
            |_: On<Start<RuntimeHeldAction>>,
             mut transition_counts: ResMut<HeldTransitionCounts>| {
                transition_counts.started += 1;
            },
        );
        app.world_mut().add_observer(
            |_: On<Complete<RuntimeHeldAction>>,
             mut transition_counts: ResMut<HeldTransitionCounts>| {
                transition_counts.completed += 1;
            },
        );
        app
    }

    fn insert_compiled(
        app: &mut App,
        source: String,
        generation: Generation,
    ) -> Result<(), String> {
        let command_registry = command_registry(app)?;

        insert_compiled_for_registry(app, command_registry, source, generation)
    }

    /// Compiles against a registry holding one held command per modifier family, so several
    /// distinct custom inputs can be active at once.
    fn insert_compiled_for_modifier_family_holds(
        app: &mut App,
        source: String,
        generation: Generation,
    ) -> Result<(), String> {
        let mut type_registry = TypeRegistry::default();
        type_registry.register::<RuntimeAltHeld>();
        type_registry.register::<RuntimeHeld>();
        type_registry.register::<RuntimeShiftHeld>();
        let command_registry = built_command_registry(app, &type_registry)?;

        insert_compiled_for_registry(app, command_registry, source, generation)
    }

    fn insert_compiled_for_registry(
        app: &mut App,
        command_registry: CommandRegistry,
        source: String,
        generation: Generation,
    ) -> Result<(), String> {
        let compiled_keymap = compile(source, &command_registry, generation)?;
        app.world_mut().insert_resource(compiled_keymap);
        app.world_mut().init_resource::<KeymapRuntime>();
        Ok(())
    }

    fn command_registry(app: &mut App) -> Result<CommandRegistry, String> {
        let mut type_registry = TypeRegistry::default();
        type_registry.register::<RuntimeHeld>();
        type_registry.register::<RuntimeOneShot>();
        type_registry.register::<RuntimeTwoStroke>();
        type_registry.register::<RuntimeUnremappable>();

        built_command_registry(app, &type_registry)
    }

    fn built_command_registry(
        app: &mut App,
        type_registry: &TypeRegistry,
    ) -> Result<CommandRegistry, String> {
        let command_registry = {
            let mut custom_inputs = app.world_mut().resource_mut::<CustomInputs>();
            CommandRegistry::build(type_registry, &mut custom_inputs).map_err(|diagnostics| {
                format!("runtime command registry errors: {diagnostics:?}")
            })?
        };
        command_registry.register_held_observers(app.world_mut());

        Ok(command_registry)
    }

    fn compile(
        source: String,
        command_registry: &CommandRegistry,
        generation: Generation,
    ) -> Result<CompiledKeymap, String> {
        let condition_registry = ConditionRegistry::default();
        let (merged_keymap, diagnostics) = MergedKeymap::from_sources(
            &defaults_keymap_file(),
            &source,
            &UserKeymap::DefaultsOnly,
            command_registry,
            &condition_registry,
            &[],
        )
        .map_err(|diagnostics| format!("runtime keymap errors: {diagnostics:?}"))?;
        if !diagnostics.is_empty() {
            return Err(format!("runtime keymap diagnostics: {diagnostics:?}"));
        }

        Ok(merged_keymap.compile(generation, command_registry))
    }

    fn compile_with_conditions(
        source: &str,
        command_registry: &CommandRegistry,
        condition_registry: &ConditionRegistry,
        generation: Generation,
    ) -> Result<CompiledKeymap, String> {
        let (merged_keymap, diagnostics) = MergedKeymap::from_sources(
            &defaults_keymap_file(),
            source,
            &UserKeymap::DefaultsOnly,
            command_registry,
            condition_registry,
            &[],
        )
        .map_err(|diagnostics| format!("runtime keymap errors: {diagnostics:?}"))?;
        if !diagnostics.is_empty() {
            return Err(format!("runtime keymap diagnostics: {diagnostics:?}"));
        }

        Ok(merged_keymap.compile(generation, command_registry))
    }

    fn bindings(entries: &[(&str, &str)]) -> String {
        let bindings = entries
            .iter()
            .map(|(keystroke, command_id)| format!(r#""{keystroke}": "{command_id}""#))
            .collect::<Vec<_>>()
            .join(", ");

        format!(r#"{{ "bindings": [{{ "bindings": {{ {bindings} }} }}] }}"#)
    }

    fn held_custom_input(app: &App) -> Result<CustomInput, String> {
        held_custom_input_from_compiled(app.world().resource::<CompiledKeymap>())
    }

    fn held_custom_input_from_compiled(
        compiled_keymap: &CompiledKeymap,
    ) -> Result<CustomInput, String> {
        compiled_keymap
            .commands
            .iter()
            .find_map(|(_, command_entry)| match command_entry.invocation() {
                Invocation::Held(custom_input) => Some(custom_input),
                Invocation::OneShot | Invocation::Unremappable => None,
            })
            .ok_or_else(|| "runtime keymap has no held custom input".to_owned())
    }

    fn spawn_held_action(app: &mut App, custom_input: CustomInput) -> Result<Entity, String> {
        app.add_plugins(TimePlugin);
        app.add_plugins(EnhancedInputPlugin);
        app.add_input_context::<RuntimeInputContext>();
        app.finish();
        app.world_mut().spawn((
            RuntimeInputContext,
            Actions::<RuntimeInputContext>::spawn(SpawnWith(
                move |action_spawner: &mut ActionSpawner<RuntimeInputContext>| {
                    action_spawner.spawn((
                        Action::<RuntimeHeldAction>::new(),
                        bindings![Binding::Custom(custom_input)],
                    ));
                },
            )),
        ));
        app.world_mut().flush();
        app.update();

        let world = app.world_mut();
        let mut query = world.query_filtered::<Entity, With<Action<RuntimeHeldAction>>>();
        query
            .single(world)
            .map_err(|_| "runtime held action was not spawned".to_owned())
    }

    fn press(app: &mut App, key: KeyCode) {
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(key);
        route_input(app.world_mut());
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .clear_just_pressed(key);
    }

    fn release(app: &mut App, key: KeyCode) {
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .release(key);
        route_input(app.world_mut());
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .clear_just_released(key);
    }

    fn press_together<const COUNT: usize>(app: &mut App, keys: [KeyCode; COUNT]) {
        for key in keys {
            app.world_mut()
                .resource_mut::<ButtonInput<KeyCode>>()
                .press(key);
        }
        route_input(app.world_mut());
        for key in keys {
            app.world_mut()
                .resource_mut::<ButtonInput<KeyCode>>()
                .clear_just_pressed(key);
        }
    }

    fn release_together<const COUNT: usize>(app: &mut App, keys: [KeyCode; COUNT]) {
        for key in keys {
            app.world_mut()
                .resource_mut::<ButtonInput<KeyCode>>()
                .release(key);
        }
        route_input(app.world_mut());
        for key in keys {
            app.world_mut()
                .resource_mut::<ButtonInput<KeyCode>>()
                .clear_just_released(key);
        }
    }

    fn pause_runtime_context(mut runtime_context: ResMut<RuntimeContext>) {
        *runtime_context = RuntimeContext::Paused;
    }
}
