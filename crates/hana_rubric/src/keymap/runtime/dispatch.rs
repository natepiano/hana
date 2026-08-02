use bevy::ecs::world::World;
use bevy::input::ButtonInput;
use bevy::input::keyboard::KeyCode;
use bevy_enhanced_input::prelude::ActionValue;
use bevy_enhanced_input::prelude::CustomInputs;

use super::held::ActiveMatcher;
use super::held::KeymapRuntime;
use super::key_edge;
use crate::ActiveCondition;
use crate::MatchOutcome;
use crate::SequenceMatcher;
use crate::command::Invocation;
use crate::condition::ConditionHandle;
use crate::keymap::CommandHandle;
use crate::keymap::CompiledKeymap;
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
    {
        let mut keymap_runtime = world.resource_mut::<KeymapRuntime>();
        keymap_runtime.clear_physical();
        keymap_runtime.inhibit(pressed.into_iter());
    }
    flush_custom_inputs(world);
}

pub(crate) fn route_input(world: &mut World) {
    if !world.contains_resource::<CompiledKeymap>() {
        return;
    }
    let active_condition = match world.get_resource::<ActiveCondition>() {
        Some(active_condition) if !active_condition.is_initialized() => return,
        Some(active_condition) => active_condition.handle(),
        None => None,
    };
    world.init_resource::<KeymapRuntime>();
    world.init_resource::<CustomInputs>();

    let dispatches = synchronize_and_resolve_timeout(world, active_condition);
    flush_custom_inputs(world);
    dispatch_all(world, dispatches);
    route_releases(world);
    route_presses(world);
}

fn synchronize_and_resolve_timeout(
    world: &mut World,
    active_condition: Option<ConditionHandle>,
) -> Dispatches {
    let reset_required = world.resource_scope::<CompiledKeymap, _>(|world, mut compiled_keymap| {
        let mut keymap_runtime = world.resource_mut::<KeymapRuntime>();
        let generation_changed = keymap_runtime
            .generation()
            .is_some_and(|generation| generation != compiled_keymap.generation);
        let condition_changed = keymap_runtime.condition_changed(active_condition);

        if condition_changed
            && !generation_changed
            && let Some(sequence_matcher) =
                previous_matcher(&mut compiled_keymap, keymap_runtime.active_matcher())
        {
            sequence_matcher.cancel_pending();
        }
        keymap_runtime.update_generation(compiled_keymap.generation, active_condition);
        generation_changed || condition_changed
    });

    if reset_required {
        let pressed = world
            .get_resource::<ButtonInput<KeyCode>>()
            .map(|pressed| pressed.get_pressed().copied().collect::<Vec<_>>())
            .unwrap_or_default();
        let mut keymap_runtime = world.resource_mut::<KeymapRuntime>();
        keymap_runtime.clear_physical();
        keymap_runtime.inhibit(pressed.into_iter());
    }

    world.resource_scope::<CompiledKeymap, _>(|world, mut compiled_keymap| {
        let mut keymap_runtime = world.resource_mut::<KeymapRuntime>();
        let now = keymap_runtime.now();
        matcher(&mut compiled_keymap, active_condition)
            .filter(|sequence_matcher| sequence_matcher.is_pending())
            .and_then(|sequence_matcher| sequence_matcher.resolve_timeout(now, SEQUENCE_TIMEOUT))
            .and_then(|command_handle| {
                dispatch_for_handle(&mut keymap_runtime, &compiled_keymap, command_handle, None)
            })
            .into()
    })
}

fn route_releases(world: &mut World) {
    clear_processed_keycodes(world);
    while let Some(key) = next_released_key(world) {
        let mut keymap_runtime = world.resource_mut::<KeymapRuntime>();
        keymap_runtime.mark_processed(key);
        keymap_runtime.release_inhibition(key);
        keymap_runtime.release_physical(key);
    }
    flush_custom_inputs(world);
}

fn route_presses(world: &mut World) {
    clear_processed_keycodes(world);
    while let Some(key) = next_pressed_key(world) {
        world.resource_mut::<KeymapRuntime>().mark_processed(key);
        if key_edge::is_modifier(key) || world.resource::<KeymapRuntime>().is_inhibited(key) {
            continue;
        }

        let Some(keystroke) = world
            .get_resource::<ButtonInput<KeyCode>>()
            .map(|pressed| key_edge::keystroke(pressed, key))
        else {
            continue;
        };
        let active_condition = world
            .get_resource::<ActiveCondition>()
            .and_then(ActiveCondition::handle);
        let dispatches = route_keystroke(world, active_condition, keystroke, key);
        flush_custom_inputs(world);
        dispatch_all(world, dispatches);
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
    active_condition: Option<ConditionHandle>,
    keystroke: crate::Keystroke,
    key: KeyCode,
) -> Dispatches {
    world.resource_scope::<CompiledKeymap, _>(|world, mut compiled_keymap| {
        let mut keymap_runtime = world.resource_mut::<KeymapRuntime>();
        let now = keymap_runtime.now();
        let Some(match_outcome) =
            matcher(&mut compiled_keymap, active_condition).map(|sequence_matcher| {
                sequence_matcher.match_keystroke(keystroke, now, SEQUENCE_TIMEOUT)
            })
        else {
            return Dispatches::default();
        };
        let mut dispatches = Dispatches::default();
        route_match_outcome(
            &mut keymap_runtime,
            &mut compiled_keymap,
            active_condition,
            Some(key),
            match_outcome,
            &mut dispatches,
        );
        dispatches
    })
}

fn route_match_outcome(
    keymap_runtime: &mut KeymapRuntime,
    compiled_keymap: &mut CompiledKeymap,
    active_condition: Option<ConditionHandle>,
    key: Option<KeyCode>,
    match_outcome: MatchOutcome<CommandHandle>,
    dispatches: &mut Dispatches,
) {
    match match_outcome {
        MatchOutcome::Matched(command_handle) => dispatches.push(dispatch_for_handle(
            keymap_runtime,
            compiled_keymap,
            command_handle,
            key,
        )),
        MatchOutcome::Reprocess {
            deferred,
            keystroke,
        } => {
            if let Some(command_handle) = deferred {
                dispatches.push(dispatch_for_handle(
                    keymap_runtime,
                    compiled_keymap,
                    command_handle,
                    key,
                ));
            }
            if let Some(MatchOutcome::Matched(command_handle)) =
                matcher(compiled_keymap, active_condition).map(|sequence_matcher| {
                    sequence_matcher.match_keystroke(
                        keystroke,
                        keymap_runtime.now(),
                        SEQUENCE_TIMEOUT,
                    )
                })
            {
                dispatches.push(dispatch_for_handle(
                    keymap_runtime,
                    compiled_keymap,
                    command_handle,
                    key,
                ));
            }
        },
        MatchOutcome::Deferred(_) | MatchOutcome::NoMatch | MatchOutcome::Pending => {},
    }
}

fn dispatch_for_handle(
    keymap_runtime: &mut KeymapRuntime,
    compiled_keymap: &CompiledKeymap,
    command_handle: CommandHandle,
    key: Option<KeyCode>,
) -> Option<fn(&mut World)> {
    match compiled_keymap.invocation(command_handle) {
        Some(Invocation::Held(custom_input)) => {
            if let Some(key) = key {
                keymap_runtime.activate_physical(key, custom_input);
            }
            None
        },
        Some(Invocation::OneShot | Invocation::Unremappable) => {
            compiled_keymap.dispatch(command_handle)
        },
        None => None,
    }
}

fn flush_custom_inputs(world: &mut World) {
    let pending_inputs = world.resource_mut::<KeymapRuntime>().take_pending_inputs();
    if !pending_inputs.is_empty() {
        let mut custom_inputs = world.resource_mut::<CustomInputs>();
        for (custom_input, is_active) in &pending_inputs {
            custom_inputs.insert(*custom_input, ActionValue::Bool(*is_active));
        }
    }
    world
        .resource_mut::<KeymapRuntime>()
        .restore_pending_inputs(pending_inputs);
}

fn matcher(
    compiled_keymap: &mut CompiledKeymap,
    active_condition: Option<ConditionHandle>,
) -> Option<&mut SequenceMatcher<CommandHandle>> {
    match active_condition {
        Some(condition_handle) => compiled_keymap.matchers.get_mut(&condition_handle),
        None => Some(&mut compiled_keymap.global),
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

fn dispatch_all(world: &mut World, dispatches: Dispatches) {
    if let Some(dispatch) = dispatches.first {
        dispatch(world);
    }
    if let Some(dispatch) = dispatches.second {
        dispatch(world);
    }
}

#[derive(Default)]
struct Dispatches {
    first:  Option<fn(&mut World)>,
    second: Option<fn(&mut World)>,
}

impl Dispatches {
    fn push(&mut self, dispatch: Option<fn(&mut World)>) {
        let Some(dispatch) = dispatch else {
            return;
        };
        if self.first.is_none() {
            self.first = Some(dispatch);
        } else {
            self.second = Some(dispatch);
        }
    }
}

impl From<Option<fn(&mut World)>> for Dispatches {
    fn from(dispatch: Option<fn(&mut World)>) -> Self {
        let mut dispatches = Self::default();
        dispatches.push(dispatch);
        dispatches
    }
}

#[cfg(test)]
#[allow(
    dead_code,
    reason = "runtime command declarations generate action marker types used through the registry"
)]
mod tests {
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
    use bevy::reflect::TypeRegistry;
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
    use crate::CommandRegistry;
    use crate::HoldPhase;
    use crate::KeymapCommand;
    use crate::KeymapPlugin;
    use crate::KeymapSystems;
    use crate::KeystrokeSequence;
    use crate::ReflectKeymapCommand;
    use crate::SequenceMatcher;
    use crate::command::Invocation;
    use crate::condition::ConditionRegistry;
    use crate::keymap::CommandHandle;
    use crate::keymap::CompiledKeymap;
    use crate::keymap::Generation;
    use crate::keymap::MergedKeymap;

    const DEFAULTS_PATH: &str = "runtime-defaults.jsonc";
    const FIRST_GENERATION: Generation = Generation(1);
    const SECOND_GENERATION: Generation = Generation(2);

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
        let flying = condition_registry
            .resolve("flying")
            .ok_or_else(|| "runtime context did not resolve flying".to_owned())?;
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
            .position(|command_entry| {
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
        let command_registry = {
            let mut custom_inputs = app.world_mut().resource_mut::<CustomInputs>();
            CommandRegistry::build(&type_registry, &mut custom_inputs).map_err(|diagnostics| {
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
            DEFAULTS_PATH,
            &source,
            None,
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
            DEFAULTS_PATH,
            source,
            None,
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
            .find_map(|command_entry| match command_entry.invocation() {
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

    fn pause_runtime_context(mut runtime_context: ResMut<RuntimeContext>) {
        *runtime_context = RuntimeContext::Paused;
    }
}
