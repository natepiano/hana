use std::collections::HashMap;
use std::collections::HashSet;
use std::time::Instant;

use bevy::ecs::prelude::Resource;
use bevy::input::keyboard::KeyCode;
use bevy_enhanced_input::prelude::ActionValue;
use bevy_enhanced_input::prelude::CustomInput;
use bevy_enhanced_input::prelude::CustomInputs;

use crate::condition::ConditionHandle;
use crate::keymap::Generation;

#[derive(Resource)]
pub(crate) struct KeymapRuntime {
    clock:              Clock,
    generation:         Option<Generation>,
    active_matcher:     ActiveMatcher,
    held_sources:       HashMap<CustomInput, HeldSources>,
    physical_sources:   HashMap<KeyCode, CustomInput>,
    pending_inputs:     Vec<(CustomInput, bool)>,
    inhibited:          HashSet<KeyCode>,
    processed_keycodes: HashSet<KeyCode>,
}

impl Default for KeymapRuntime {
    fn default() -> Self {
        Self {
            clock:              Clock::System,
            generation:         None,
            active_matcher:     ActiveMatcher::Uninitialized,
            held_sources:       HashMap::with_capacity(1),
            physical_sources:   HashMap::with_capacity(1),
            pending_inputs:     Vec::with_capacity(1),
            inhibited:          HashSet::with_capacity(1),
            processed_keycodes: HashSet::with_capacity(1),
        }
    }
}

impl KeymapRuntime {
    pub(super) fn now(&self) -> Instant { self.clock.now() }

    pub(super) const fn generation(&self) -> Option<Generation> { self.generation }

    pub(super) const fn active_matcher(&self) -> ActiveMatcher { self.active_matcher }

    pub(super) fn condition_changed(&self, active_condition: Option<ConditionHandle>) -> bool {
        match self.active_matcher {
            ActiveMatcher::Uninitialized => false,
            ActiveMatcher::Global => active_condition.is_some(),
            ActiveMatcher::Condition(previous) => Some(previous) != active_condition,
        }
    }

    pub(super) fn update_generation(
        &mut self,
        generation: Generation,
        active_condition: Option<ConditionHandle>,
    ) {
        self.generation = Some(generation);
        self.active_matcher = ActiveMatcher::from(active_condition);
    }

    pub(super) fn inhibit(&mut self, pressed: impl Iterator<Item = KeyCode>) {
        self.inhibited.clear();
        self.inhibited.extend(pressed);
    }

    pub(super) fn is_inhibited(&self, key: KeyCode) -> bool { self.inhibited.contains(&key) }

    pub(super) fn release_inhibition(&mut self, key: KeyCode) { self.inhibited.remove(&key); }

    pub(super) fn clear_processed_keycodes(&mut self) { self.processed_keycodes.clear(); }

    pub(super) fn is_processed(&self, key: KeyCode) -> bool {
        self.processed_keycodes.contains(&key)
    }

    pub(super) fn mark_processed(&mut self, key: KeyCode) { self.processed_keycodes.insert(key); }

    pub(super) fn activate_physical(&mut self, key: KeyCode, custom_input: CustomInput) {
        if self.physical_sources.contains_key(&key) {
            return;
        }

        let (was_active, is_active) = {
            let held_sources = self.held_sources.entry(custom_input).or_default();
            let was_active = held_sources.is_active();
            held_sources.physical += 1;
            (was_active, held_sources.is_active())
        };
        self.physical_sources.insert(key, custom_input);
        self.record_custom_input_if_changed(custom_input, was_active, is_active);
    }

    pub(super) fn release_physical(&mut self, key: KeyCode) {
        let Some(custom_input) = self.physical_sources.remove(&key) else {
            return;
        };
        let Some((was_active, is_active)) =
            self.held_sources
                .get_mut(&custom_input)
                .map(|held_sources| {
                    let was_active = held_sources.is_active();
                    held_sources.physical -= 1;
                    (was_active, held_sources.is_active())
                })
        else {
            return;
        };
        self.record_custom_input_if_changed(custom_input, was_active, is_active);
        if !is_active {
            self.held_sources.remove(&custom_input);
        }
    }

    pub(super) fn clear_physical(&mut self) {
        self.physical_sources.clear();
        let mut input_changes = Vec::new();
        self.held_sources.retain(|custom_input, held_sources| {
            let was_active = held_sources.is_active();
            held_sources.physical = 0;
            let is_active = held_sources.is_active();
            if was_active != is_active {
                input_changes.push((*custom_input, is_active));
            }
            is_active
        });
        self.pending_inputs.extend(input_changes);
    }

    pub(super) fn set_event_source(
        &mut self,
        custom_input: CustomInput,
        is_active: bool,
        custom_inputs: &mut CustomInputs,
    ) {
        if is_active {
            let held_sources = self.held_sources.entry(custom_input).or_default();
            let was_active = held_sources.is_active();
            held_sources.event = 1;
            write_custom_input_if_changed(
                custom_inputs,
                custom_input,
                was_active,
                held_sources.is_active(),
            );
            return;
        }

        let Some(held_sources) = self.held_sources.get_mut(&custom_input) else {
            custom_inputs.insert(custom_input, ActionValue::Bool(false));
            return;
        };
        let was_active = held_sources.is_active();
        held_sources.event = 0;
        let input_is_active = held_sources.is_active();
        write_custom_input_if_changed(custom_inputs, custom_input, was_active, input_is_active);
        if !input_is_active {
            self.held_sources.remove(&custom_input);
        }
    }

    pub(super) fn take_pending_inputs(&mut self) -> Vec<(CustomInput, bool)> {
        std::mem::take(&mut self.pending_inputs)
    }

    pub(super) fn restore_pending_inputs(&mut self, mut pending_inputs: Vec<(CustomInput, bool)>) {
        pending_inputs.clear();
        self.pending_inputs = pending_inputs;
    }

    #[cfg(test)]
    pub(super) const fn set_test_clock(&mut self, now: Instant) { self.clock = Clock::Test(now); }

    fn record_custom_input_if_changed(
        &mut self,
        custom_input: CustomInput,
        was_active: bool,
        is_active: bool,
    ) {
        if was_active != is_active {
            self.pending_inputs.push((custom_input, is_active));
        }
    }
}

#[derive(Clone, Copy)]
pub(super) enum ActiveMatcher {
    Uninitialized,
    Global,
    Condition(ConditionHandle),
}

impl From<Option<ConditionHandle>> for ActiveMatcher {
    fn from(condition_handle: Option<ConditionHandle>) -> Self {
        match condition_handle {
            Some(condition_handle) => Self::Condition(condition_handle),
            None => Self::Global,
        }
    }
}

#[derive(Clone, Copy)]
enum Clock {
    System,
    #[cfg(test)]
    Test(Instant),
}

impl Clock {
    fn now(self) -> Instant {
        match self {
            Self::System => Instant::now(),
            #[cfg(test)]
            Self::Test(now) => now,
        }
    }
}

#[derive(Default)]
struct HeldSources {
    event:    usize,
    physical: usize,
}

impl HeldSources {
    const fn is_active(&self) -> bool { self.event + self.physical != 0 }
}

fn write_custom_input_if_changed(
    custom_inputs: &mut CustomInputs,
    custom_input: CustomInput,
    was_active: bool,
    is_active: bool,
) {
    if was_active != is_active {
        custom_inputs.insert(custom_input, ActionValue::Bool(is_active));
    }
}
