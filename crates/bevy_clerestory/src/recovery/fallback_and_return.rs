//! Automatic fallback settling, intervention, and return intent.

use std::collections::HashMap;
use std::time::Duration;

use bevy::prelude::*;
use bevy::time::Virtual;
use bevy::window::WindowPosition;
use bevy_kana::ToU32;

use super::registration::RecoveryGeneration;
use super::registration::RecoveryRegistrations;
use super::registration::WindowRecovery;
use crate::WindowKey;
use crate::WindowRecoveryPending;
use crate::constants::SETTLE_STABILITY_SECS;
use crate::monitors::CurrentMonitor;
use crate::monitors::MonitorId;
use crate::monitors::MonitorIdentity;
use crate::monitors::MonitorInfo;
use crate::monitors::MonitorTopologyRevision;
use crate::monitors::Monitors;
use crate::persistence::CapturedWindowPlacement;
use crate::persistence::CapturedWindowPosition;
use crate::persistence::CapturedWindowStates;
use crate::persistence::SavedWindowMode;
use crate::platform::ReturnCapability;
use crate::restore::RestoreDisposition;

#[derive(Clone, Debug, PartialEq, Eq)]
enum ObservedPosition {
    Restorable(IVec2),
    CompositorControlled,
}

#[derive(Clone, Debug, PartialEq)]
struct FallbackObservation {
    monitor_entity:    Option<Entity>,
    monitor_snapshot:  MonitorInfo,
    position:          ObservedPosition,
    logical_size:      UVec2,
    saved_window_mode: SavedWindowMode,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FallbackMonitorPresence {
    Installed,
    Missing,
}

impl FallbackObservation {
    const fn intervention_projection(&self) -> InterventionProjection<'_> {
        InterventionProjection {
            position:          &self.position,
            logical_size:      self.logical_size,
            saved_window_mode: &self.saved_window_mode,
        }
    }

    fn monitor_presence(&self, monitors: &Monitors) -> FallbackMonitorPresence {
        if let Some(monitor_entity) = self.monitor_entity {
            return if monitors
                .iter()
                .any(|monitor| monitor.entity == monitor_entity)
            {
                FallbackMonitorPresence::Installed
            } else {
                FallbackMonitorPresence::Missing
            };
        }

        match self.monitor_snapshot.identity {
            MonitorIdentity::Verified(monitor_id) => monitors
                .by_id(monitor_id)
                .map_or(FallbackMonitorPresence::Missing, |_| {
                    FallbackMonitorPresence::Installed
                }),
            MonitorIdentity::Unverified => monitors
                .iter()
                .find(|monitor| monitor.monitor_info == &self.monitor_snapshot)
                .map_or(FallbackMonitorPresence::Missing, |_| {
                    FallbackMonitorPresence::Installed
                }),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
struct InterventionProjection<'a> {
    position:          &'a ObservedPosition,
    logical_size:      UVec2,
    saved_window_mode: &'a SavedWindowMode,
}

#[derive(Clone, Debug, PartialEq)]
struct FallbackSettling {
    return_intent: ReturnIntent,
    observation:   Option<FallbackObservation>,
    stable_for:    Duration,
}

#[derive(Clone, Debug, PartialEq)]
enum ReturnIntent {
    Active,
    Cleared(FallbackObservation),
}

#[derive(Clone, Debug, PartialEq)]
enum FallbackAndReturnPhase {
    Healthy,
    RemovalPending,
    FallbackSettling(FallbackSettling),
    OnFallback(FallbackObservation),
    Restoring,
    MissingLiveWindow(ReturnIntent),
    RetryableFailure(FallbackObservation),
}

#[derive(Clone, Debug)]
struct FallbackAndReturnRecovery {
    generation:              RecoveryGeneration,
    phase:                   FallbackAndReturnPhase,
    notification:            Option<MonitorId>,
    window_shell:            Window,
    fallback_before_restore: Option<FallbackObservation>,
    #[cfg(test)]
    topology_evaluations:    usize,
}

#[derive(Default, Resource)]
pub(crate) struct FallbackAndReturnRecoveries {
    entries: HashMap<WindowKey, FallbackAndReturnRecovery>,
}

impl FallbackAndReturnRecoveries {
    pub(super) fn accept(
        &mut self,
        window_key: WindowKey,
        generation: RecoveryGeneration,
        window_shell: Window,
    ) {
        let recovery = FallbackAndReturnRecovery {
            generation,
            phase: FallbackAndReturnPhase::Healthy,
            notification: None,
            window_shell,
            fallback_before_restore: None,
            #[cfg(test)]
            topology_evaluations: 0,
        };
        debug!(
            "[FallbackAndReturnRecoveries::accept] [{window_key}] retained window shell with mode {:?}",
            recovery.window_shell.mode,
        );
        self.entries.insert(window_key, recovery);
    }

    pub(super) fn window_removed(
        &mut self,
        window_key: &WindowKey,
        generation: RecoveryGeneration,
        restore_intents: &mut AutomaticRestoreIntents,
    ) {
        let Some(recovery) = self.entries.get_mut(window_key) else {
            return;
        };
        if recovery.generation != generation {
            return;
        }

        recovery.phase = match &recovery.phase {
            FallbackAndReturnPhase::Healthy => FallbackAndReturnPhase::RemovalPending,
            FallbackAndReturnPhase::RetryableFailure(observation) => {
                FallbackAndReturnPhase::MissingLiveWindow(ReturnIntent::Cleared(
                    observation.clone(),
                ))
            },
            FallbackAndReturnPhase::FallbackSettling(settling) => {
                FallbackAndReturnPhase::MissingLiveWindow(settling.return_intent.clone())
            },
            FallbackAndReturnPhase::MissingLiveWindow(return_intent) => {
                FallbackAndReturnPhase::MissingLiveWindow(return_intent.clone())
            },
            FallbackAndReturnPhase::RemovalPending
            | FallbackAndReturnPhase::OnFallback(_)
            | FallbackAndReturnPhase::Restoring => {
                FallbackAndReturnPhase::MissingLiveWindow(ReturnIntent::Active)
            },
        };
        restore_intents.mark_missing(window_key, generation);
    }

    pub(super) fn cancel(
        &mut self,
        window_key: &WindowKey,
        generation: RecoveryGeneration,
        restore_intents: &mut AutomaticRestoreIntents,
    ) {
        if self
            .entries
            .get(window_key)
            .is_some_and(|recovery| recovery.generation == generation)
        {
            self.entries.remove(window_key);
        }
        restore_intents.clear(window_key, generation);
    }

    pub(crate) fn can_begin_restore(
        &self,
        window_key: &WindowKey,
        generation: RecoveryGeneration,
    ) -> bool {
        self.entries.get(window_key).is_some_and(|recovery| {
            recovery.generation == generation
                && matches!(
                    recovery.phase,
                    FallbackAndReturnPhase::RemovalPending
                        | FallbackAndReturnPhase::FallbackSettling(FallbackSettling {
                            return_intent: ReturnIntent::Active,
                            ..
                        })
                        | FallbackAndReturnPhase::OnFallback(_)
                        | FallbackAndReturnPhase::MissingLiveWindow(ReturnIntent::Active)
                )
        })
    }

    pub(crate) fn begin_restore(
        &mut self,
        window_key: &WindowKey,
        generation: RecoveryGeneration,
    ) -> bool {
        let Some(recovery) = self.entries.get_mut(window_key) else {
            return false;
        };
        if recovery.generation != generation {
            return false;
        }
        recovery.fallback_before_restore = match &recovery.phase {
            FallbackAndReturnPhase::OnFallback(observation)
            | FallbackAndReturnPhase::RetryableFailure(observation) => Some(observation.clone()),
            FallbackAndReturnPhase::FallbackSettling(settling) => settling.observation.clone(),
            FallbackAndReturnPhase::Healthy
            | FallbackAndReturnPhase::RemovalPending
            | FallbackAndReturnPhase::Restoring
            | FallbackAndReturnPhase::MissingLiveWindow(_) => None,
        };
        if !matches!(
            recovery.phase,
            FallbackAndReturnPhase::RemovalPending
                | FallbackAndReturnPhase::FallbackSettling(FallbackSettling {
                    return_intent: ReturnIntent::Active,
                    ..
                })
                | FallbackAndReturnPhase::OnFallback(_)
                | FallbackAndReturnPhase::MissingLiveWindow(ReturnIntent::Active)
        ) {
            recovery.fallback_before_restore = None;
            return false;
        }
        recovery.phase = FallbackAndReturnPhase::Restoring;
        true
    }

    pub(crate) fn finish_restore(
        &mut self,
        window_key: &WindowKey,
        generation: RecoveryGeneration,
        entity: Entity,
        disposition: RestoreDisposition,
    ) -> bool {
        let Some(recovery) = self.entries.get_mut(window_key) else {
            return false;
        };
        if recovery.generation != generation || recovery.phase != FallbackAndReturnPhase::Restoring
        {
            return false;
        }
        recovery.phase = match disposition {
            RestoreDisposition::Succeeded => FallbackAndReturnPhase::Healthy,
            RestoreDisposition::Failed => recovery.fallback_before_restore.take().map_or_else(
                || fallback_phase(Some(entity), ReturnIntent::Active),
                FallbackAndReturnPhase::RetryableFailure,
            ),
        };
        recovery.fallback_before_restore = None;
        true
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct AutomaticRestoreIntent {
    pub(crate) generation: RecoveryGeneration,
    pub(crate) entity:     Option<Entity>,
    pub(crate) monitor:    MonitorInfo,
    pub(crate) revision:   MonitorTopologyRevision,
}

#[derive(Default, Resource)]
pub(crate) struct AutomaticRestoreIntents {
    entries: HashMap<WindowKey, AutomaticRestoreIntent>,
}

impl AutomaticRestoreIntents {
    pub(crate) fn enqueue(
        &mut self,
        window_key: WindowKey,
        generation: RecoveryGeneration,
        entity: Option<Entity>,
        monitor: MonitorInfo,
        revision: MonitorTopologyRevision,
    ) {
        self.entries
            .entry(window_key)
            .or_insert(AutomaticRestoreIntent {
                generation,
                entity,
                monitor,
                revision,
            });
    }

    fn mark_missing(&mut self, window_key: &WindowKey, generation: RecoveryGeneration) {
        if let Some(intent) = self.entries.get_mut(window_key)
            && intent.generation == generation
        {
            intent.entity = None;
        }
    }

    fn clear(&mut self, window_key: &WindowKey, generation: RecoveryGeneration) {
        if self
            .entries
            .get(window_key)
            .is_some_and(|intent| intent.generation == generation)
        {
            self.entries.remove(window_key);
        }
    }

    pub(crate) fn pending(&self) -> impl Iterator<Item = (&WindowKey, &AutomaticRestoreIntent)> {
        self.entries.iter()
    }

    pub(crate) fn consume(&mut self, window_key: &WindowKey, generation: RecoveryGeneration) {
        self.clear(window_key, generation);
    }

    #[cfg(test)]
    fn get(&self, window_key: &WindowKey) -> Option<&AutomaticRestoreIntent> {
        self.entries.get(window_key)
    }
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FallbackAndReturnPhaseSnapshot {
    Healthy,
    RemovalPending,
    FallbackSettling,
    OnFallback,
    Restoring,
    MissingLiveWindow,
    RetryableFailure,
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct AutomaticRestoreIntentSnapshot {
    pub(crate) entity:   Option<Entity>,
    pub(crate) monitor:  MonitorInfo,
    pub(crate) revision: MonitorTopologyRevision,
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct FallbackAndReturnSnapshot {
    pub(crate) phase:            FallbackAndReturnPhaseSnapshot,
    pub(crate) fallback_monitor: Option<MonitorInfo>,
    pub(crate) intent:           Option<AutomaticRestoreIntentSnapshot>,
    pub(crate) intent_count:     usize,
}

#[cfg(test)]
pub(crate) fn fallback_and_return_snapshot(
    world: &World,
    window_key: &WindowKey,
) -> Option<FallbackAndReturnSnapshot> {
    let recovery = world
        .resource::<FallbackAndReturnRecoveries>()
        .entries
        .get(window_key)?;
    let (mut phase, fallback_monitor) = match &recovery.phase {
        FallbackAndReturnPhase::Healthy => (FallbackAndReturnPhaseSnapshot::Healthy, None),
        FallbackAndReturnPhase::RemovalPending => {
            (FallbackAndReturnPhaseSnapshot::RemovalPending, None)
        },
        FallbackAndReturnPhase::FallbackSettling(settling) => {
            let fallback_monitor = settling
                .observation
                .as_ref()
                .or(match &settling.return_intent {
                    ReturnIntent::Active => None,
                    ReturnIntent::Cleared(observation) => Some(observation),
                })
                .map(|observation| observation.monitor_snapshot);
            (
                FallbackAndReturnPhaseSnapshot::FallbackSettling,
                fallback_monitor,
            )
        },
        FallbackAndReturnPhase::OnFallback(observation) => (
            FallbackAndReturnPhaseSnapshot::OnFallback,
            Some(observation.monitor_snapshot),
        ),
        FallbackAndReturnPhase::Restoring => (FallbackAndReturnPhaseSnapshot::Restoring, None),
        FallbackAndReturnPhase::MissingLiveWindow(return_intent) => (
            FallbackAndReturnPhaseSnapshot::MissingLiveWindow,
            match return_intent {
                ReturnIntent::Active => None,
                ReturnIntent::Cleared(observation) => Some(observation.monitor_snapshot),
            },
        ),
        FallbackAndReturnPhase::RetryableFailure(observation) => (
            FallbackAndReturnPhaseSnapshot::RetryableFailure,
            Some(observation.monitor_snapshot),
        ),
    };
    let restore_intents = world.resource::<AutomaticRestoreIntents>();
    let intent =
        restore_intents
            .entries
            .get(window_key)
            .map(|intent| AutomaticRestoreIntentSnapshot {
                entity:   intent.entity,
                monitor:  intent.monitor,
                revision: intent.revision,
            });
    if phase == FallbackAndReturnPhaseSnapshot::MissingLiveWindow && intent.is_some() {
        phase = FallbackAndReturnPhaseSnapshot::Restoring;
    }
    Some(FallbackAndReturnSnapshot {
        phase,
        fallback_monitor,
        intent,
        intent_count: restore_intents.entries.len(),
    })
}

const fn fallback_phase(
    live_entity: Option<Entity>,
    return_intent: ReturnIntent,
) -> FallbackAndReturnPhase {
    match live_entity {
        Some(_) => FallbackAndReturnPhase::FallbackSettling(FallbackSettling {
            return_intent,
            observation: None,
            stable_for: Duration::ZERO,
        }),
        None => FallbackAndReturnPhase::MissingLiveWindow(return_intent),
    }
}

pub(super) fn evaluate_topology(
    revision: Res<MonitorTopologyRevision>,
    monitors: Res<Monitors>,
    live_windows: Query<(), With<Window>>,
    mut registrations: ResMut<RecoveryRegistrations>,
    mut recoveries: ResMut<FallbackAndReturnRecoveries>,
    mut restore_intents: ResMut<AutomaticRestoreIntents>,
    mut captured_window_states: ResMut<CapturedWindowStates>,
) {
    for registration in registrations.registered_mut() {
        if registration.policy != WindowRecovery::FallbackAndReturn
            || registration.last_revision == Some(*revision)
        {
            continue;
        }
        registration.last_revision = Some(*revision);
        let Some(recovery) = recoveries.entries.get_mut(&registration.window_key) else {
            continue;
        };
        if recovery.generation != registration.generation {
            continue;
        }
        #[cfg(test)]
        {
            recovery.topology_evaluations += 1;
        }

        let live_entity = registration
            .entity
            .filter(|entity| live_windows.contains(*entity));
        let target_monitor = monitors.by_id(registration.monitor_id).copied();
        let lost_retryable_fallback = match &recovery.phase {
            FallbackAndReturnPhase::RetryableFailure(observation)
                if observation.monitor_presence(&monitors) == FallbackMonitorPresence::Missing =>
            {
                Some(observation.clone())
            },
            FallbackAndReturnPhase::Healthy
            | FallbackAndReturnPhase::RemovalPending
            | FallbackAndReturnPhase::FallbackSettling(_)
            | FallbackAndReturnPhase::OnFallback(_)
            | FallbackAndReturnPhase::Restoring
            | FallbackAndReturnPhase::MissingLiveWindow(_)
            | FallbackAndReturnPhase::RetryableFailure(_) => None,
        };
        if let Some(observation) = lost_retryable_fallback {
            captured_window_states.freeze(&registration.window_key);
            recovery.phase = fallback_phase(live_entity, ReturnIntent::Cleared(observation));
            continue;
        }

        match target_monitor {
            Some(monitor) => match recovery.phase.clone() {
                FallbackAndReturnPhase::RemovalPending
                | FallbackAndReturnPhase::FallbackSettling(FallbackSettling {
                    return_intent: ReturnIntent::Active,
                    ..
                })
                | FallbackAndReturnPhase::OnFallback(_)
                | FallbackAndReturnPhase::MissingLiveWindow(ReturnIntent::Active) => {
                    restore_intents.enqueue(
                        registration.window_key.clone(),
                        registration.generation,
                        live_entity,
                        monitor,
                        *revision,
                    );
                },
                FallbackAndReturnPhase::Healthy
                | FallbackAndReturnPhase::Restoring
                | FallbackAndReturnPhase::RetryableFailure(_)
                | FallbackAndReturnPhase::FallbackSettling(FallbackSettling {
                    return_intent: ReturnIntent::Cleared(_),
                    ..
                })
                | FallbackAndReturnPhase::MissingLiveWindow(ReturnIntent::Cleared(_)) => {},
            },
            None => match recovery.phase.clone() {
                FallbackAndReturnPhase::Healthy | FallbackAndReturnPhase::RemovalPending => {
                    captured_window_states.freeze(&registration.window_key);
                    recovery.notification = Some(registration.monitor_id);
                    recovery.phase = fallback_phase(live_entity, ReturnIntent::Active);
                },
                FallbackAndReturnPhase::Restoring => {
                    restore_intents.clear(&registration.window_key, registration.generation);
                    recovery.phase = fallback_phase(live_entity, ReturnIntent::Active);
                },
                FallbackAndReturnPhase::OnFallback(observation)
                    if observation.monitor_presence(&monitors)
                        == FallbackMonitorPresence::Missing =>
                {
                    recovery.phase = fallback_phase(live_entity, ReturnIntent::Active);
                },
                FallbackAndReturnPhase::MissingLiveWindow(_) => {
                    restore_intents.clear(&registration.window_key, registration.generation);
                },
                FallbackAndReturnPhase::FallbackSettling(_)
                | FallbackAndReturnPhase::OnFallback(_)
                | FallbackAndReturnPhase::RetryableFailure(_) => {},
            },
        }
    }
}

fn observe_fallback(
    window: &Window,
    current_monitor: &CurrentMonitor,
    captured_position: CapturedWindowPosition,
    monitors: &Monitors,
) -> Option<FallbackObservation> {
    let position = match captured_position {
        CapturedWindowPosition::Restorable { .. } => match window.position {
            WindowPosition::At(physical_position) => {
                ObservedPosition::Restorable(physical_position)
            },
            WindowPosition::Automatic | WindowPosition::Centered(_) => return None,
        },
        CapturedWindowPosition::CompositorControlled => ObservedPosition::CompositorControlled,
    };
    let mut matching_monitors = monitors
        .iter()
        .filter(|monitor| monitor.monitor_info == &current_monitor.monitor_info);
    let monitor_entity = matching_monitors
        .next()
        .and_then(|monitor| matching_monitors.next().is_none().then_some(monitor.entity));
    Some(FallbackObservation {
        monitor_entity,
        monitor_snapshot: current_monitor.monitor_info,
        position,
        logical_size: UVec2::new(
            window.resolution.width().to_u32(),
            window.resolution.height().to_u32(),
        ),
        saved_window_mode: (&current_monitor.effective_window_mode).into(),
    })
}

fn captured_fallback(
    window: &Window,
    current_monitor: &CurrentMonitor,
    observation: &FallbackObservation,
    platform: crate::Platform,
) -> CapturedWindowPlacement {
    let physical_position = match observation.position {
        ObservedPosition::Restorable(physical_position) => Some(physical_position),
        ObservedPosition::CompositorControlled => None,
    };
    CapturedWindowPlacement::capture(window, current_monitor, physical_position, platform)
}

fn adopt_user_placement(
    window_key: &WindowKey,
    entity: Entity,
    window: &Window,
    current_monitor: &CurrentMonitor,
    observation: FallbackObservation,
    platform: crate::Platform,
    revision: MonitorTopologyRevision,
    generation: RecoveryGeneration,
    registrations: &mut RecoveryRegistrations,
    restore_intents: &mut AutomaticRestoreIntents,
    captured_window_states: &mut CapturedWindowStates,
) -> Option<FallbackAndReturnPhase> {
    let placement = captured_fallback(window, current_monitor, &observation, platform);
    captured_window_states
        .adopt_intervention(window_key, entity, placement.clone())
        .mutation()?;
    restore_intents.clear(window_key, generation);
    let capability =
        platform.fallback_return_capability(placement.position, &placement.saved_window_mode);
    if let (MonitorIdentity::Verified(monitor_id), ReturnCapability::Supported) =
        (placement.monitor_snapshot.identity, capability)
    {
        if let Some(registration) = registrations.by_key_mut(window_key) {
            registration.monitor_id = monitor_id;
            registration.target = placement.monitor_snapshot;
            registration.last_revision = Some(revision);
        }
        Some(FallbackAndReturnPhase::Healthy)
    } else {
        Some(FallbackAndReturnPhase::RetryableFailure(observation))
    }
}

pub(crate) fn advance_fallback_windows(
    time: Option<Res<Time<Virtual>>>,
    platform: Res<crate::Platform>,
    revision: Res<MonitorTopologyRevision>,
    monitors: Res<Monitors>,
    windows: Query<(&Window, &CurrentMonitor)>,
    mut registrations: ResMut<RecoveryRegistrations>,
    mut recoveries: ResMut<FallbackAndReturnRecoveries>,
    mut restore_intents: ResMut<AutomaticRestoreIntents>,
    mut captured_window_states: ResMut<CapturedWindowStates>,
) {
    let delta = time.as_deref().map_or(Duration::ZERO, Time::delta);
    let window_keys: Vec<_> = recoveries.entries.keys().cloned().collect();
    for window_key in window_keys {
        let Some(registration) = registrations.by_key(&window_key).cloned() else {
            continue;
        };
        let Some(entity) = registration.entity else {
            continue;
        };
        let Ok((window, current_monitor)) = windows.get(entity) else {
            continue;
        };
        let Some(captured_position) = captured_window_states
            .captured_placement(&window_key)
            .map(|placement| placement.position)
        else {
            continue;
        };
        let Some(observation) =
            observe_fallback(window, current_monitor, captured_position, &monitors)
        else {
            continue;
        };
        let Some(recovery) = recoveries.entries.get_mut(&window_key) else {
            continue;
        };
        if recovery.generation != registration.generation {
            continue;
        }

        match &mut recovery.phase {
            FallbackAndReturnPhase::FallbackSettling(settling) => {
                if settling.observation.as_ref() == Some(&observation) {
                    settling.stable_for += delta;
                } else {
                    settling.observation = Some(observation.clone());
                    settling.stable_for = Duration::ZERO;
                }
                if settling.stable_for >= Duration::from_secs_f32(SETTLE_STABILITY_SECS) {
                    recovery.phase = match &settling.return_intent {
                        ReturnIntent::Active => FallbackAndReturnPhase::OnFallback(observation),
                        ReturnIntent::Cleared(_) => {
                            captured_window_states.suppress_current_capture(&window_key, entity);
                            FallbackAndReturnPhase::RetryableFailure(observation)
                        },
                    };
                }
            },
            FallbackAndReturnPhase::OnFallback(previous)
                if previous.intervention_projection() == observation.intervention_projection() =>
            {
                if *previous != observation {
                    *previous = observation;
                }
            },
            FallbackAndReturnPhase::RetryableFailure(previous)
                if previous.intervention_projection() == observation.intervention_projection() =>
            {
                if *previous != observation {
                    *previous = observation;
                    captured_window_states.suppress_current_capture(&window_key, entity);
                }
            },
            FallbackAndReturnPhase::OnFallback(_) | FallbackAndReturnPhase::RetryableFailure(_) => {
                if let Some(next_phase) = adopt_user_placement(
                    &window_key,
                    entity,
                    window,
                    current_monitor,
                    observation,
                    *platform,
                    *revision,
                    registration.generation,
                    &mut registrations,
                    &mut restore_intents,
                    &mut captured_window_states,
                ) {
                    recovery.phase = next_phase;
                }
            },
            FallbackAndReturnPhase::Healthy
            | FallbackAndReturnPhase::RemovalPending
            | FallbackAndReturnPhase::Restoring
            | FallbackAndReturnPhase::MissingLiveWindow(_) => {},
        }
    }
}

pub(super) fn emit_pending_notifications(
    registrations: Res<RecoveryRegistrations>,
    mut recoveries: ResMut<FallbackAndReturnRecoveries>,
    mut commands: Commands,
) {
    for (window_key, recovery) in &mut recoveries.entries {
        let Some(registration) = registrations.by_key(window_key) else {
            continue;
        };
        if registration.generation != recovery.generation {
            continue;
        }
        if let Some(monitor_id) = recovery.notification.take() {
            commands.trigger(WindowRecoveryPending {
                window_key: window_key.clone(),
                monitor_id,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use bevy::time::TimePlugin;
    use bevy::time::TimeUpdateStrategy;
    use bevy::window::MonitorSelection;
    use bevy::window::OnMonitor;
    use bevy::window::PrimaryWindow;
    use bevy::window::WindowMode;
    use bevy_kana::ToF32;
    use tempfile::NamedTempFile;

    use super::*;
    use crate::CancelWindowRecovery;
    use crate::ClerestoryUpdateSet;
    use crate::ManagedWindow;
    use crate::ManagedWindowPersistence;
    use crate::Platform;
    use crate::WindowRestoreMismatch;
    use crate::WindowRestored;
    use crate::managed;
    use crate::managed::ManagedWindowRegistry;
    use crate::persistence;
    use crate::persistence::CapturedPlacement;
    use crate::persistence::PersistencePlugin;
    use crate::persistence::PersistenceWriteState;
    use crate::recovery;
    use crate::recovery::RecoveryPlugin;
    use crate::restore::NativeWindowReady;
    use crate::restore::TargetPosition;
    use crate::restore_window_config::RestoreWindowConfig;

    const ADOPTED_POSITION_OFFSET: IVec2 = IVec2::new(50, 60);
    const FALLBACK_ID: MonitorId = MonitorId::from_test_raw(22);
    const FALLBACK_IDENTITY_REVISION: u64 = 2;
    const FALLBACK_INDEX: usize = 1;
    const FALLBACK_LOSS_REVISION: u64 = 3;
    const FALLBACK_REPLACEMENT_ID: MonitorId = MonitorId::from_test_raw(33);
    const LOGICAL_OFFSET: IVec2 = IVec2::new(30, 40);
    const LOGICAL_SIZE: UVec2 = UVec2::new(800, 600);
    const LOSS_REVISION: u64 = 1;
    const MANAGED_WINDOW_NAME: &str = "secondary";
    const MONITOR_SCALE_FACTOR: f64 = 1.0;
    const MOVED_LOGICAL_SIZE: UVec2 = UVec2::new(900, 700);
    const OS_RELOCATED_LOGICAL_SIZE: UVec2 = UVec2::new(1_000, 800);
    const PHYSICAL_SIZE: UVec2 = UVec2::new(1_920, 1_080);
    const REARRANGED_TARGET_OFFSET: IVec2 = IVec2::new(20, 30);
    const SETTLE_PROBE_SECS: f32 = SETTLE_STABILITY_SECS / 2.0;
    const TARGET_ID: MonitorId = MonitorId::from_test_raw(11);
    const TARGET_INDEX: usize = 0;
    const TARGET_POSITION: IVec2 = IVec2::new(100, 80);
    const TARGET_REAPPEAR_REVISION: u64 = 4;
    const TARGET_RETURN_REVISION: u64 = 2;
    const UNVERIFIED_FALLBACK_OFFSET: IVec2 = IVec2::new(40, 50);

    #[derive(Clone, Copy, Debug)]
    enum TestWindowRole {
        Primary,
        Managed,
    }

    impl TestWindowRole {
        fn window_key(self) -> WindowKey {
            match self {
                Self::Primary => WindowKey::Primary,
                Self::Managed => WindowKey::Managed(MANAGED_WINDOW_NAME.to_string()),
            }
        }
    }

    #[derive(Default, Resource)]
    struct RecoveryFacts {
        pending:    Vec<(WindowKey, MonitorId)>,
        restored:   usize,
        mismatched: usize,
    }

    struct RecoveryTestApp {
        app:             App,
        window:          Entity,
        target_entity:   Entity,
        fallback_entity: Entity,
        window_key:      WindowKey,
        target:          MonitorInfo,
        fallback:        MonitorInfo,
    }

    fn monitor(identity: MonitorIdentity, index: usize, physical_position: IVec2) -> MonitorInfo {
        MonitorInfo {
            identity,
            index,
            scale: MONITOR_SCALE_FACTOR,
            physical_position,
            physical_size: PHYSICAL_SIZE,
        }
    }

    fn placement(
        monitor_snapshot: MonitorInfo,
        position: CapturedWindowPosition,
        saved_window_mode: SavedWindowMode,
    ) -> CapturedWindowPlacement {
        CapturedWindowPlacement {
            monitor_snapshot,
            position,
            logical_size: LOGICAL_SIZE,
            saved_window_mode,
            captured_scale: monitor_snapshot.scale,
        }
    }

    fn record_pending(event: On<WindowRecoveryPending>, mut facts: ResMut<RecoveryFacts>) {
        facts
            .pending
            .push((event.window_key.clone(), event.monitor_id));
    }

    fn record_restored(_: On<WindowRestored>, mut facts: ResMut<RecoveryFacts>) {
        facts.restored += 1;
    }

    fn record_mismatch(_: On<WindowRestoreMismatch>, mut facts: ResMut<RecoveryFacts>) {
        facts.mismatched += 1;
    }

    fn recovery_app(role: TestWindowRole) -> RecoveryTestApp {
        recovery_app_with(
            role,
            Platform::Windows,
            MonitorIdentity::Verified(TARGET_ID),
            CapturedWindowPosition::Restorable {
                logical_offset: LOGICAL_OFFSET,
            },
            SavedWindowMode::Windowed,
        )
    }

    fn recovery_app_with(
        role: TestWindowRole,
        platform: Platform,
        target_identity: MonitorIdentity,
        position: CapturedWindowPosition,
        saved_window_mode: SavedWindowMode,
    ) -> RecoveryTestApp {
        recovery_app_configured(
            role,
            platform,
            target_identity,
            position,
            saved_window_mode,
            None,
        )
    }

    fn recovery_app_with_persistence(
        role: TestWindowRole,
        platform: Platform,
        target_identity: MonitorIdentity,
        position: CapturedWindowPosition,
        saved_window_mode: SavedWindowMode,
        state_file: &Path,
    ) -> RecoveryTestApp {
        recovery_app_configured(
            role,
            platform,
            target_identity,
            position,
            saved_window_mode,
            Some(state_file),
        )
    }

    fn configure_recovery_test_app(app: &mut App, state_file: Option<&Path>) {
        app.configure_sets(
            Update,
            (
                ClerestoryUpdateSet::MonitorTopology,
                ClerestoryUpdateSet::RecoveryTopology,
                ClerestoryUpdateSet::CurrentMonitor,
                ClerestoryUpdateSet::RecoveryWindow,
                ClerestoryUpdateSet::RestorePreparation,
                ClerestoryUpdateSet::X11Compensation,
                ClerestoryUpdateSet::RestoreApplication,
                ClerestoryUpdateSet::RestoreSettling,
                ClerestoryUpdateSet::Persistence,
            )
                .chain(),
        )
        .add_plugins(RecoveryPlugin)
        .add_observer(persistence::on_primary_window_removed)
        .add_observer(persistence::on_window_removed)
        .add_observer(managed::on_managed_window_added)
        .add_observer(managed::on_managed_window_removed)
        .add_observer(record_pending)
        .add_observer(record_restored)
        .add_observer(record_mismatch);
        if let Some(state_file) = state_file {
            app.insert_resource(RestoreWindowConfig {
                path: state_file.to_path_buf(),
            })
            .add_plugins(PersistencePlugin);
            app.world_mut()
                .resource_mut::<CapturedWindowStates>()
                .seed(HashMap::new());
        }
    }

    fn recovery_app_configured(
        role: TestWindowRole,
        platform: Platform,
        target_identity: MonitorIdentity,
        position: CapturedWindowPosition,
        saved_window_mode: SavedWindowMode,
        state_file: Option<&Path>,
    ) -> RecoveryTestApp {
        let mut app = App::new();
        app.add_plugins(TimePlugin);
        let target_entity = app.world_mut().spawn_empty().id();
        let fallback_entity = app.world_mut().spawn_empty().id();
        let target = monitor(target_identity, TARGET_INDEX, IVec2::ZERO);
        let fallback = monitor(
            MonitorIdentity::Verified(FALLBACK_ID),
            FALLBACK_INDEX,
            IVec2::new(PHYSICAL_SIZE.x.cast_signed(), 0),
        );
        app.insert_resource(Monitors::from_test_monitors([
            (target_entity, target),
            (fallback_entity, fallback),
        ]))
        .insert_resource(MonitorTopologyRevision::default())
        .insert_resource(platform)
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::ZERO))
        .insert_resource(ManagedWindowPersistence::RememberAll)
        .init_resource::<ManagedWindowRegistry>()
        .init_resource::<CapturedWindowStates>()
        .init_resource::<RecoveryFacts>();
        configure_recovery_test_app(&mut app, state_file);

        let mut window = Window {
            position: WindowPosition::At(TARGET_POSITION),
            ..default()
        };
        window
            .resolution
            .set(LOGICAL_SIZE.x.to_f32(), LOGICAL_SIZE.y.to_f32());
        let window = match role {
            TestWindowRole::Primary => app
                .world_mut()
                .spawn((
                    window,
                    PrimaryWindow,
                    OnMonitor(target_entity),
                    CurrentMonitor {
                        monitor_info:          target,
                        effective_window_mode: WindowMode::Windowed,
                    },
                    NativeWindowReady,
                    WindowRecovery::FallbackAndReturn,
                ))
                .id(),
            TestWindowRole::Managed => app
                .world_mut()
                .spawn((
                    window,
                    ManagedWindow {
                        name: MANAGED_WINDOW_NAME.to_string(),
                    },
                    OnMonitor(target_entity),
                    CurrentMonitor {
                        monitor_info:          target,
                        effective_window_mode: WindowMode::Windowed,
                    },
                    NativeWindowReady,
                    WindowRecovery::FallbackAndReturn,
                ))
                .id(),
        };
        app.world_mut().flush();
        let window_key = role.window_key();
        app.world_mut()
            .resource_mut::<CapturedWindowStates>()
            .promote(
                window_key.clone(),
                window,
                placement(target, position, saved_window_mode),
            );
        app.update();

        RecoveryTestApp {
            app,
            window,
            target_entity,
            fallback_entity,
            window_key,
            target,
            fallback,
        }
    }

    fn install_topology(
        test_app: &mut RecoveryTestApp,
        revision: u64,
        monitors: impl IntoIterator<Item = (Entity, MonitorInfo)>,
    ) {
        test_app
            .app
            .insert_resource(Monitors::from_test_monitors(monitors));
        test_app
            .app
            .insert_resource(MonitorTopologyRevision::from_test_raw(revision));
    }

    fn move_to_fallback(test_app: &mut RecoveryTestApp) {
        let mut entity = test_app.app.world_mut().entity_mut(test_app.window);
        {
            let window = entity.get_mut::<Window>();
            let Some(mut window) = window else {
                return;
            };
            window.position = WindowPosition::At(test_app.fallback.physical_position + IVec2::ONE);
        }
        entity.insert(CurrentMonitor {
            monitor_info:          test_app.fallback,
            effective_window_mode: WindowMode::Windowed,
        });
    }

    fn lose_target(test_app: &mut RecoveryTestApp, revision: u64) {
        move_to_fallback(test_app);
        let fallback = (test_app.fallback_entity, test_app.fallback);
        install_topology(test_app, revision, [fallback]);
        test_app.app.update();
    }

    fn advance(test_app: &mut RecoveryTestApp, duration: Duration) {
        test_app
            .app
            .insert_resource(TimeUpdateStrategy::ManualDuration(duration));
        test_app.app.update();
        test_app
            .app
            .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::ZERO));
    }

    fn phase(test_app: &RecoveryTestApp) -> &FallbackAndReturnPhase {
        &test_app
            .app
            .world()
            .resource::<FallbackAndReturnRecoveries>()
            .entries[&test_app.window_key]
            .phase
    }

    fn topology_evaluations(test_app: &RecoveryTestApp) -> usize {
        test_app
            .app
            .world()
            .resource::<FallbackAndReturnRecoveries>()
            .entries[&test_app.window_key]
            .topology_evaluations
    }

    fn registered_target(test_app: &RecoveryTestApp) -> Option<(MonitorId, MonitorInfo)> {
        test_app
            .app
            .world()
            .resource::<RecoveryRegistrations>()
            .by_key(&test_app.window_key)
            .map(|registration| (registration.monitor_id, registration.target))
    }

    fn assert_captured_placement_state(
        test_app: &RecoveryTestApp,
        expected_placement: &CapturedWindowPlacement,
        expected_write_state: PersistenceWriteState,
    ) {
        let states = test_app.app.world().resource::<CapturedWindowStates>();
        assert_eq!(
            states.captured_placement(&test_app.window_key),
            Some(expected_placement),
        );
        assert_eq!(
            states
                .entry(&test_app.window_key)
                .map(|state| state.persistence),
            Some(expected_write_state),
        );
    }

    fn seed_restore_intent(test_app: &mut RecoveryTestApp) -> Option<AutomaticRestoreIntent> {
        let source = test_app
            .app
            .world()
            .resource::<RecoveryRegistrations>()
            .by_key(&test_app.window_key)
            .map(|registration| (registration.generation, registration.target));
        if let Some((generation, target)) = source {
            test_app
                .app
                .world_mut()
                .resource_mut::<AutomaticRestoreIntents>()
                .enqueue(
                    test_app.window_key.clone(),
                    generation,
                    Some(test_app.window),
                    target,
                    MonitorTopologyRevision::from_test_raw(LOSS_REVISION),
                );
        }
        test_app
            .app
            .world()
            .resource::<AutomaticRestoreIntents>()
            .get(&test_app.window_key)
            .cloned()
    }

    fn enter_restoring_with_intent(test_app: &mut RecoveryTestApp) {
        lose_target(test_app, LOSS_REVISION);
        let target = (test_app.target_entity, test_app.target);
        let fallback = (test_app.fallback_entity, test_app.fallback);
        install_topology(test_app, TARGET_RETURN_REVISION, [target, fallback]);
        test_app.app.update();

        let generation = test_app
            .app
            .world()
            .resource::<RecoveryRegistrations>()
            .by_key(&test_app.window_key)
            .map(|registration| registration.generation);
        assert!(matches!(
            phase(test_app),
            FallbackAndReturnPhase::FallbackSettling(_)
        ));
        assert_eq!(
            test_app
                .app
                .world()
                .resource::<AutomaticRestoreIntents>()
                .get(&test_app.window_key)
                .map(|intent| intent.generation),
            generation,
        );
    }

    fn accept_replacement_registration(test_app: &mut RecoveryTestApp, policy: WindowRecovery) {
        test_app
            .app
            .world_mut()
            .entity_mut(test_app.window)
            .insert(CurrentMonitor {
                monitor_info:          test_app.target,
                effective_window_mode: WindowMode::Windowed,
            });
        test_app
            .app
            .world_mut()
            .entity_mut(test_app.window)
            .remove::<WindowRecovery>();
        test_app.app.world_mut().flush();
        test_app
            .app
            .world_mut()
            .entity_mut(test_app.window)
            .insert(policy);
        test_app.app.world_mut().flush();
        test_app.app.update();
    }

    fn settle_on_fallback(test_app: &mut RecoveryTestApp) {
        lose_target(test_app, LOSS_REVISION);
        advance(test_app, Duration::from_secs_f32(SETTLE_STABILITY_SECS));
        assert!(
            matches!(phase(test_app), FallbackAndReturnPhase::OnFallback(_)),
            "unexpected phase: {:?}",
            phase(test_app),
        );
    }

    fn retryable_failure_app() -> RecoveryTestApp { retryable_failure_app_configured(None) }

    fn retryable_failure_app_with_persistence(state_file: &Path) -> RecoveryTestApp {
        retryable_failure_app_configured(Some(state_file))
    }

    fn retryable_failure_app_configured(state_file: Option<&Path>) -> RecoveryTestApp {
        let mut test_app = state_file.map_or_else(
            || {
                recovery_app_with(
                    TestWindowRole::Primary,
                    Platform::Wayland,
                    MonitorIdentity::Verified(TARGET_ID),
                    CapturedWindowPosition::CompositorControlled,
                    SavedWindowMode::BorderlessFullscreen,
                )
            },
            |state_file| {
                recovery_app_with_persistence(
                    TestWindowRole::Primary,
                    Platform::Wayland,
                    MonitorIdentity::Verified(TARGET_ID),
                    CapturedWindowPosition::CompositorControlled,
                    SavedWindowMode::BorderlessFullscreen,
                    state_file,
                )
            },
        );
        move_to_fallback(&mut test_app);
        test_app
            .app
            .world_mut()
            .entity_mut(test_app.window)
            .insert(CurrentMonitor {
                monitor_info:          test_app.fallback,
                effective_window_mode: WindowMode::BorderlessFullscreen(MonitorSelection::Index(
                    test_app.fallback.index,
                )),
            });
        let fallback = (test_app.fallback_entity, test_app.fallback);
        install_topology(&mut test_app, LOSS_REVISION, [fallback]);
        test_app.app.update();
        advance(
            &mut test_app,
            Duration::from_secs_f32(SETTLE_STABILITY_SECS),
        );

        test_app
            .app
            .world_mut()
            .entity_mut(test_app.window)
            .insert(CurrentMonitor {
                monitor_info:          test_app.fallback,
                effective_window_mode: WindowMode::Windowed,
            });
        test_app.app.update();
        assert!(matches!(
            phase(&test_app),
            FallbackAndReturnPhase::RetryableFailure(_)
        ));
        test_app
    }

    fn fallback_observation() -> FallbackObservation {
        FallbackObservation {
            monitor_entity:    None,
            monitor_snapshot:  monitor(
                MonitorIdentity::Verified(FALLBACK_ID),
                FALLBACK_INDEX,
                IVec2::new(PHYSICAL_SIZE.x.cast_signed(), 0),
            ),
            position:          ObservedPosition::Restorable(IVec2::ONE),
            logical_size:      LOGICAL_SIZE,
            saved_window_mode: SavedWindowMode::Windowed,
        }
    }

    #[test]
    fn primary_and_managed_loss_enter_fallback_settling() {
        for role in [TestWindowRole::Primary, TestWindowRole::Managed] {
            let mut test_app = recovery_app(role);
            lose_target(&mut test_app, LOSS_REVISION);

            assert!(matches!(
                phase(&test_app),
                FallbackAndReturnPhase::FallbackSettling(_)
            ));
            assert_eq!(
                test_app.app.world().resource::<RecoveryFacts>().pending,
                [(test_app.window_key.clone(), TARGET_ID)]
            );
        }
    }

    #[test]
    fn settling_resets_after_geometry_changes() {
        let mut test_app = recovery_app(TestWindowRole::Primary);
        lose_target(&mut test_app, LOSS_REVISION);
        advance(&mut test_app, Duration::from_secs_f32(SETTLE_PROBE_SECS));

        if let Some(mut window) = test_app.app.world_mut().get_mut::<Window>(test_app.window) {
            window
                .resolution
                .set(MOVED_LOGICAL_SIZE.x.to_f32(), MOVED_LOGICAL_SIZE.y.to_f32());
        }
        advance(
            &mut test_app,
            Duration::from_secs_f32(SETTLE_STABILITY_SECS),
        );
        assert!(matches!(
            phase(&test_app),
            FallbackAndReturnPhase::FallbackSettling(_)
        ));

        advance(
            &mut test_app,
            Duration::from_secs_f32(SETTLE_STABILITY_SECS),
        );
        assert!(matches!(
            phase(&test_app),
            FallbackAndReturnPhase::OnFallback(_)
        ));
    }

    #[test]
    fn target_before_settle_queues_one_internal_intent_only() {
        let mut test_app = recovery_app(TestWindowRole::Primary);
        lose_target(&mut test_app, LOSS_REVISION);
        let target_position = test_app
            .app
            .world()
            .get::<Window>(test_app.window)
            .map(|window| window.position);

        let target = (test_app.target_entity, test_app.target);
        let fallback = (test_app.fallback_entity, test_app.fallback);
        install_topology(&mut test_app, TARGET_RETURN_REVISION, [target, fallback]);
        test_app.app.update();
        test_app.app.update();

        let intents = test_app.app.world().resource::<AutomaticRestoreIntents>();
        let intent = intents.get(&test_app.window_key);
        assert!(intent.is_some());
        assert_eq!(
            intent.map(|intent| intent.entity),
            Some(Some(test_app.window))
        );
        assert!(
            test_app
                .app
                .world()
                .get::<TargetPosition>(test_app.window)
                .is_none()
        );
        assert_eq!(
            test_app
                .app
                .world()
                .get::<Window>(test_app.window)
                .map(|window| window.position),
            target_position,
        );
        let facts = test_app.app.world().resource::<RecoveryFacts>();
        assert_eq!((facts.restored, facts.mismatched), (0, 0));
    }

    #[test]
    fn accepted_generation_replacement_clears_stale_intent_before_later_return() {
        let mut test_app = recovery_app(TestWindowRole::Primary);
        enter_restoring_with_intent(&mut test_app);
        let previous_generation = test_app
            .app
            .world()
            .resource::<RecoveryRegistrations>()
            .by_key(&test_app.window_key)
            .map(|registration| registration.generation);

        accept_replacement_registration(&mut test_app, WindowRecovery::FallbackAndReturn);

        let registrations = test_app.app.world().resource::<RecoveryRegistrations>();
        let replacement_generation = registrations
            .by_key(&test_app.window_key)
            .map(|registration| registration.generation);
        assert_ne!(replacement_generation, previous_generation);
        assert_eq!(registrations.registered().count(), 1);
        assert_eq!(
            registrations
                .by_key(&test_app.window_key)
                .map(|registration| registration.policy),
            Some(WindowRecovery::FallbackAndReturn),
        );
        assert_eq!(
            recovery::registration_snapshot(test_app.app.world()).pending,
            0
        );
        let recoveries = test_app
            .app
            .world()
            .resource::<FallbackAndReturnRecoveries>();
        assert!(matches!(
            recoveries.entries.get(&test_app.window_key),
            Some(FallbackAndReturnRecovery {
                generation,
                phase: FallbackAndReturnPhase::Healthy,
                ..
            }) if Some(*generation) == replacement_generation
        ));
        assert!(
            test_app
                .app
                .world()
                .resource::<AutomaticRestoreIntents>()
                .get(&test_app.window_key)
                .is_none()
        );

        move_to_fallback(&mut test_app);
        let fallback = (test_app.fallback_entity, test_app.fallback);
        install_topology(&mut test_app, FALLBACK_LOSS_REVISION, [fallback]);
        test_app.app.update();
        let target = (test_app.target_entity, test_app.target);
        let fallback = (test_app.fallback_entity, test_app.fallback);
        install_topology(&mut test_app, TARGET_REAPPEAR_REVISION, [target, fallback]);
        test_app.app.update();

        assert_eq!(
            test_app
                .app
                .world()
                .resource::<AutomaticRestoreIntents>()
                .get(&test_app.window_key)
                .map(|intent| intent.generation),
            replacement_generation,
        );
    }

    #[test]
    fn policy_switch_retires_the_previous_automatic_lifecycle() {
        let mut test_app = recovery_app(TestWindowRole::Primary);
        enter_restoring_with_intent(&mut test_app);
        let pending_before_switch = test_app
            .app
            .world()
            .resource::<RecoveryFacts>()
            .pending
            .len();

        accept_replacement_registration(&mut test_app, WindowRecovery::ApplicationControlled);

        let registrations = test_app.app.world().resource::<RecoveryRegistrations>();
        assert_eq!(registrations.registered().count(), 1);
        assert_eq!(
            registrations
                .by_key(&test_app.window_key)
                .map(|registration| registration.policy),
            Some(WindowRecovery::ApplicationControlled),
        );
        assert_eq!(
            recovery::registration_snapshot(test_app.app.world()).pending,
            0
        );
        assert!(
            !test_app
                .app
                .world()
                .resource::<FallbackAndReturnRecoveries>()
                .entries
                .contains_key(&test_app.window_key)
        );
        assert!(
            test_app
                .app
                .world()
                .resource::<AutomaticRestoreIntents>()
                .get(&test_app.window_key)
                .is_none()
        );

        move_to_fallback(&mut test_app);
        let fallback = (test_app.fallback_entity, test_app.fallback);
        install_topology(&mut test_app, FALLBACK_LOSS_REVISION, [fallback]);
        test_app.app.update();
        assert_eq!(
            test_app
                .app
                .world()
                .resource::<RecoveryFacts>()
                .pending
                .len(),
            pending_before_switch + 1,
        );
    }

    #[test]
    fn intervention_adopts_placement_and_rearms_for_verified_target() {
        let mut test_app = recovery_app(TestWindowRole::Primary);
        settle_on_fallback(&mut test_app);
        let evaluations_after_loss = topology_evaluations(&test_app);
        let adopted_position = test_app.fallback.physical_position + ADOPTED_POSITION_OFFSET;
        if let Some(mut window) = test_app.app.world_mut().get_mut::<Window>(test_app.window) {
            window.position = WindowPosition::At(adopted_position);
            window
                .resolution
                .set(MOVED_LOGICAL_SIZE.x.to_f32(), MOVED_LOGICAL_SIZE.y.to_f32());
        }
        test_app.app.update();

        assert!(matches!(phase(&test_app), FallbackAndReturnPhase::Healthy));
        let registrations = test_app.app.world().resource::<RecoveryRegistrations>();
        assert_eq!(
            registrations
                .by_key(&test_app.window_key)
                .map(|registration| registration.monitor_id),
            Some(FALLBACK_ID),
        );
        let states = test_app.app.world().resource::<CapturedWindowStates>();
        let entry = states.entry(&test_app.window_key);
        assert_eq!(
            entry.map(|entry| entry.persistence),
            Some(PersistenceWriteState::Writable),
        );
        assert_eq!(
            states
                .captured_placement(&test_app.window_key)
                .map(|placement| placement.logical_size),
            Some(MOVED_LOGICAL_SIZE),
        );

        test_app.app.update();
        test_app.app.update();
        assert_eq!(topology_evaluations(&test_app), evaluations_after_loss);
        assert!(matches!(phase(&test_app), FallbackAndReturnPhase::Healthy));
        assert_eq!(
            test_app
                .app
                .world()
                .resource::<RecoveryRegistrations>()
                .by_key(&test_app.window_key)
                .map(|registration| registration.last_revision),
            Some(Some(MonitorTopologyRevision::from_test_raw(LOSS_REVISION))),
        );
        assert!(
            test_app
                .app
                .world()
                .resource::<AutomaticRestoreIntents>()
                .get(&test_app.window_key)
                .is_none()
        );
    }

    #[test]
    fn intervention_with_wrong_live_binding_preserves_recovery_state() {
        let mut test_app = recovery_app(TestWindowRole::Primary);
        settle_on_fallback(&mut test_app);
        let original_intent = seed_restore_intent(&mut test_app);
        assert!(original_intent.is_some());
        let wrong_entity = test_app.app.world_mut().spawn_empty().id();
        test_app
            .app
            .world_mut()
            .resource_mut::<CapturedWindowStates>()
            .bind(&test_app.window_key, wrong_entity);
        let original_phase = phase(&test_app).clone();
        let original_registration = test_app
            .app
            .world()
            .resource::<RecoveryRegistrations>()
            .by_key(&test_app.window_key)
            .map(|registration| {
                (
                    registration.monitor_id,
                    registration.target,
                    registration.last_revision,
                )
            });
        let original_state = test_app
            .app
            .world()
            .resource::<CapturedWindowStates>()
            .entry(&test_app.window_key)
            .cloned();
        let original_facts = {
            let facts = test_app.app.world().resource::<RecoveryFacts>();
            (facts.pending.clone(), facts.restored, facts.mismatched)
        };

        let adopted_position = test_app.fallback.physical_position + ADOPTED_POSITION_OFFSET;
        if let Some(mut window) = test_app.app.world_mut().get_mut::<Window>(test_app.window) {
            window.position = WindowPosition::At(adopted_position);
            window
                .resolution
                .set(MOVED_LOGICAL_SIZE.x.to_f32(), MOVED_LOGICAL_SIZE.y.to_f32());
        }
        test_app.app.update();

        assert_eq!(phase(&test_app), &original_phase);
        assert_eq!(
            test_app
                .app
                .world()
                .resource::<RecoveryRegistrations>()
                .by_key(&test_app.window_key)
                .map(|registration| {
                    (
                        registration.monitor_id,
                        registration.target,
                        registration.last_revision,
                    )
                }),
            original_registration,
        );
        assert_eq!(
            test_app
                .app
                .world()
                .resource::<CapturedWindowStates>()
                .entry(&test_app.window_key),
            original_state.as_ref(),
        );
        assert_eq!(
            test_app
                .app
                .world()
                .resource::<AutomaticRestoreIntents>()
                .get(&test_app.window_key),
            original_intent.as_ref(),
        );
        let facts = test_app.app.world().resource::<RecoveryFacts>();
        assert_eq!(
            (facts.pending.clone(), facts.restored, facts.mismatched),
            original_facts,
        );
    }

    #[test]
    fn identity_only_change_refreshes_fallback_without_intervention() {
        let mut test_app = recovery_app(TestWindowRole::Primary);
        settle_on_fallback(&mut test_app);
        let original_placement = test_app
            .app
            .world()
            .resource::<CapturedWindowStates>()
            .captured_placement(&test_app.window_key)
            .cloned();
        let original_registration = registered_target(&test_app);
        let original_intent = seed_restore_intent(&mut test_app);
        assert!(original_intent.is_some());

        let identity_only_fallback = MonitorInfo {
            identity: MonitorIdentity::Verified(FALLBACK_REPLACEMENT_ID),
            ..test_app.fallback
        };
        test_app
            .app
            .world_mut()
            .entity_mut(test_app.window)
            .insert(CurrentMonitor {
                monitor_info:          identity_only_fallback,
                effective_window_mode: WindowMode::Windowed,
            });
        let fallback_entity = test_app.fallback_entity;
        install_topology(
            &mut test_app,
            FALLBACK_IDENTITY_REVISION,
            [(fallback_entity, identity_only_fallback)],
        );
        test_app.app.update();

        assert!(matches!(
            phase(&test_app),
            FallbackAndReturnPhase::OnFallback(observation)
                if observation.monitor_snapshot == identity_only_fallback
        ));
        assert_eq!(
            test_app
                .app
                .world()
                .resource::<CapturedWindowStates>()
                .captured_placement(&test_app.window_key),
            original_placement.as_ref(),
        );
        assert_eq!(
            test_app
                .app
                .world()
                .resource::<AutomaticRestoreIntents>()
                .get(&test_app.window_key),
            original_intent.as_ref(),
        );
        assert_eq!(registered_target(&test_app), original_registration);
        assert_eq!(
            test_app
                .app
                .world()
                .resource::<RecoveryRegistrations>()
                .by_key(&test_app.window_key)
                .and_then(|registration| registration.last_revision),
            Some(MonitorTopologyRevision::from_test_raw(
                FALLBACK_IDENTITY_REVISION,
            )),
        );

        install_topology(&mut test_app, FALLBACK_LOSS_REVISION, []);
        test_app.app.update();

        assert!(matches!(
            phase(&test_app),
            FallbackAndReturnPhase::FallbackSettling(_)
        ));
        assert_eq!(
            test_app
                .app
                .world()
                .resource::<CapturedWindowStates>()
                .captured_placement(&test_app.window_key),
            original_placement.as_ref(),
        );
        assert_eq!(
            test_app
                .app
                .world()
                .resource::<AutomaticRestoreIntents>()
                .get(&test_app.window_key),
            original_intent.as_ref(),
        );
    }

    #[test]
    fn wayland_windowed_intervention_stays_unarmed_until_borderless() {
        let mut test_app = retryable_failure_app();

        test_app
            .app
            .world_mut()
            .entity_mut(test_app.window)
            .insert(CurrentMonitor {
                monitor_info:          test_app.fallback,
                effective_window_mode: WindowMode::BorderlessFullscreen(MonitorSelection::Index(
                    test_app.fallback.index,
                )),
            });
        test_app.app.update();
        assert!(matches!(phase(&test_app), FallbackAndReturnPhase::Healthy));
    }

    #[test]
    fn retryable_fallback_loss_precedes_simultaneous_obsolete_target_return() {
        let mut surviving = retryable_failure_app();
        surviving
            .app
            .world_mut()
            .entity_mut(surviving.window)
            .insert(CurrentMonitor {
                monitor_info:          surviving.target,
                effective_window_mode: WindowMode::Windowed,
            });
        let target = (surviving.target_entity, surviving.target);
        install_topology(&mut surviving, TARGET_RETURN_REVISION, [target]);
        surviving.app.update();

        assert!(matches!(
            phase(&surviving),
            FallbackAndReturnPhase::FallbackSettling(_)
        ));
        assert!(
            surviving
                .app
                .world()
                .resource::<AutomaticRestoreIntents>()
                .get(&surviving.window_key)
                .is_none()
        );
        advance(
            &mut surviving,
            Duration::from_secs_f32(SETTLE_STABILITY_SECS),
        );
        assert!(
            matches!(
                phase(&surviving),
                FallbackAndReturnPhase::RetryableFailure(_)
            ),
            "unexpected phase: {:?}",
            phase(&surviving)
        );

        let target = (surviving.target_entity, surviving.target);
        install_topology(&mut surviving, TARGET_REAPPEAR_REVISION, [target]);
        surviving.app.update();
        assert!(matches!(
            phase(&surviving),
            FallbackAndReturnPhase::RetryableFailure(_)
        ));
        assert!(
            surviving
                .app
                .world()
                .resource::<AutomaticRestoreIntents>()
                .get(&surviving.window_key)
                .is_none()
        );

        let mut deleted = retryable_failure_app();
        deleted
            .app
            .world_mut()
            .entity_mut(deleted.window)
            .remove::<Window>();
        deleted.app.world_mut().flush();
        assert!(matches!(
            phase(&deleted),
            FallbackAndReturnPhase::MissingLiveWindow(ReturnIntent::Cleared(_))
        ));
        let target = (deleted.target_entity, deleted.target);
        install_topology(&mut deleted, TARGET_RETURN_REVISION, [target]);
        deleted.app.update();

        assert!(matches!(
            phase(&deleted),
            FallbackAndReturnPhase::MissingLiveWindow(ReturnIntent::Cleared(_))
        ));
        assert!(
            deleted
                .app
                .world()
                .resource::<AutomaticRestoreIntents>()
                .get(&deleted.window_key)
                .is_none()
        );
    }

    #[test]
    fn retryable_fallback_loss_preserves_adopted_placement_through_persistence_settling()
    -> Result<(), String> {
        let state_file = NamedTempFile::new().map_err(|error| error.to_string())?;
        let mut test_app = retryable_failure_app_with_persistence(state_file.path());
        let adopted_placement = test_app
            .app
            .world()
            .resource::<CapturedWindowStates>()
            .captured_placement(&test_app.window_key)
            .cloned()
            .ok_or_else(|| "retryable failure should retain adopted placement".to_string())?;
        let replacement_entity = test_app.app.world_mut().spawn_empty().id();
        let replacement = MonitorInfo {
            identity: MonitorIdentity::Verified(FALLBACK_REPLACEMENT_ID),
            ..test_app.fallback
        };
        if let Some(mut window) = test_app.app.world_mut().get_mut::<Window>(test_app.window) {
            window.position =
                WindowPosition::At(replacement.physical_position + ADOPTED_POSITION_OFFSET);
            window.resolution.set(
                OS_RELOCATED_LOGICAL_SIZE.x.to_f32(),
                OS_RELOCATED_LOGICAL_SIZE.y.to_f32(),
            );
        }
        test_app
            .app
            .world_mut()
            .entity_mut(test_app.window)
            .insert(CurrentMonitor {
                monitor_info:          replacement,
                effective_window_mode: WindowMode::Windowed,
            });
        install_topology(
            &mut test_app,
            FALLBACK_LOSS_REVISION,
            [(replacement_entity, replacement)],
        );
        test_app.app.update();

        assert!(matches!(
            phase(&test_app),
            FallbackAndReturnPhase::FallbackSettling(FallbackSettling {
                return_intent: ReturnIntent::Cleared(_),
                ..
            })
        ));
        assert_captured_placement_state(
            &test_app,
            &adopted_placement,
            PersistenceWriteState::Frozen,
        );

        advance(
            &mut test_app,
            Duration::from_secs_f32(SETTLE_STABILITY_SECS),
        );

        assert!(matches!(
            phase(&test_app),
            FallbackAndReturnPhase::RetryableFailure(observation)
                if observation.monitor_snapshot == replacement
        ));
        assert_captured_placement_state(
            &test_app,
            &adopted_placement,
            PersistenceWriteState::Writable,
        );

        if let Some(mut window) = test_app.app.world_mut().get_mut::<Window>(test_app.window) {
            window
                .resolution
                .set(MOVED_LOGICAL_SIZE.x.to_f32(), MOVED_LOGICAL_SIZE.y.to_f32());
        }
        test_app.app.update();

        assert!(matches!(
            phase(&test_app),
            FallbackAndReturnPhase::RetryableFailure(_)
        ));
        let states = test_app.app.world().resource::<CapturedWindowStates>();
        assert_eq!(
            states
                .captured_placement(&test_app.window_key)
                .map(|placement| (
                    placement.monitor_snapshot,
                    placement.logical_size,
                    placement.saved_window_mode.clone(),
                )),
            Some((replacement, MOVED_LOGICAL_SIZE, SavedWindowMode::Windowed)),
        );
        assert_eq!(
            states
                .entry(&test_app.window_key)
                .map(|state| state.persistence),
            Some(PersistenceWriteState::Writable),
        );
        Ok(())
    }

    #[test]
    fn retryable_identity_refresh_preserves_capture_then_accepts_intervention() -> Result<(), String>
    {
        let state_file = NamedTempFile::new().map_err(|error| error.to_string())?;
        let mut test_app = retryable_failure_app_with_persistence(state_file.path());
        let adopted_placement = test_app
            .app
            .world()
            .resource::<CapturedWindowStates>()
            .captured_placement(&test_app.window_key)
            .cloned()
            .ok_or_else(|| "retryable failure should retain adopted placement".to_string())?;
        let identity_only_fallback = MonitorInfo {
            identity: MonitorIdentity::Verified(FALLBACK_REPLACEMENT_ID),
            ..test_app.fallback
        };
        test_app
            .app
            .world_mut()
            .entity_mut(test_app.window)
            .insert(CurrentMonitor {
                monitor_info:          identity_only_fallback,
                effective_window_mode: WindowMode::Windowed,
            });
        let fallback_entity = test_app.fallback_entity;
        install_topology(
            &mut test_app,
            FALLBACK_IDENTITY_REVISION,
            [(fallback_entity, identity_only_fallback)],
        );
        test_app.app.update();

        assert!(matches!(
            phase(&test_app),
            FallbackAndReturnPhase::RetryableFailure(observation)
                if observation.monitor_entity == Some(test_app.fallback_entity)
                    && observation.monitor_snapshot == identity_only_fallback
        ));
        let states = test_app.app.world().resource::<CapturedWindowStates>();
        assert_eq!(
            states.captured_placement(&test_app.window_key),
            Some(&adopted_placement),
        );
        assert_eq!(
            states
                .entry(&test_app.window_key)
                .map(|state| state.persistence),
            Some(PersistenceWriteState::Writable),
        );

        if let Some(mut window) = test_app.app.world_mut().get_mut::<Window>(test_app.window) {
            window
                .resolution
                .set(MOVED_LOGICAL_SIZE.x.to_f32(), MOVED_LOGICAL_SIZE.y.to_f32());
        }
        test_app
            .app
            .world_mut()
            .entity_mut(test_app.window)
            .insert(CurrentMonitor {
                monitor_info:          identity_only_fallback,
                effective_window_mode: WindowMode::BorderlessFullscreen(MonitorSelection::Index(
                    identity_only_fallback.index,
                )),
            });
        test_app.app.update();

        assert!(matches!(phase(&test_app), FallbackAndReturnPhase::Healthy));
        let states = test_app.app.world().resource::<CapturedWindowStates>();
        assert_eq!(
            states
                .captured_placement(&test_app.window_key)
                .map(|placement| (
                    placement.monitor_snapshot,
                    placement.logical_size,
                    placement.saved_window_mode.clone(),
                )),
            Some((
                identity_only_fallback,
                MOVED_LOGICAL_SIZE,
                SavedWindowMode::BorderlessFullscreen,
            )),
        );
        assert_eq!(
            states
                .entry(&test_app.window_key)
                .map(|state| state.persistence),
            Some(PersistenceWriteState::Writable),
        );

        let adopted_placement = states
            .captured_placement(&test_app.window_key)
            .cloned()
            .ok_or_else(|| "intervention should replace the captured placement".to_string())?;
        install_topology(&mut test_app, FALLBACK_LOSS_REVISION, []);
        test_app.app.update();

        assert!(matches!(
            phase(&test_app),
            FallbackAndReturnPhase::FallbackSettling(_)
        ));
        assert_captured_placement_state(
            &test_app,
            &adopted_placement,
            PersistenceWriteState::Frozen,
        );
        Ok(())
    }

    #[test]
    fn zero_displays_and_different_identity_do_not_queue_return() {
        let mut test_app = recovery_app(TestWindowRole::Primary);
        move_to_fallback(&mut test_app);
        install_topology(&mut test_app, LOSS_REVISION, []);
        test_app.app.update();
        assert!(
            test_app
                .app
                .world()
                .resource::<AutomaticRestoreIntents>()
                .get(&test_app.window_key)
                .is_none()
        );

        let fallback = (test_app.fallback_entity, test_app.fallback);
        install_topology(&mut test_app, TARGET_RETURN_REVISION, [fallback]);
        test_app.app.update();
        assert!(
            test_app
                .app
                .world()
                .resource::<AutomaticRestoreIntents>()
                .get(&test_app.window_key)
                .is_none()
        );
        assert_eq!(
            test_app
                .app
                .world()
                .resource::<RecoveryFacts>()
                .pending
                .len(),
            1,
        );
    }

    #[test]
    fn fallback_monitor_loss_enters_missing_live_window() {
        let mut test_app = recovery_app(TestWindowRole::Primary);
        settle_on_fallback(&mut test_app);
        test_app
            .app
            .world_mut()
            .entity_mut(test_app.window)
            .remove::<Window>();
        test_app.app.world_mut().flush();
        install_topology(&mut test_app, TARGET_RETURN_REVISION, []);
        test_app.app.update();

        assert!(matches!(
            phase(&test_app),
            FallbackAndReturnPhase::MissingLiveWindow(_)
        ));
    }

    #[test]
    fn unverified_fallback_disappearance_restarts_settling() {
        let mut test_app = recovery_app(TestWindowRole::Primary);
        test_app.fallback.identity = MonitorIdentity::Unverified;
        settle_on_fallback(&mut test_app);
        let original_state = test_app
            .app
            .world()
            .resource::<CapturedWindowStates>()
            .entry(&test_app.window_key)
            .cloned();
        assert_eq!(
            original_state.as_ref().map(|state| state.persistence),
            Some(PersistenceWriteState::Frozen),
        );

        let different_entity = test_app.app.world_mut().spawn_empty().id();
        let different_unverified = MonitorInfo {
            physical_position: test_app.fallback.physical_position + UNVERIFIED_FALLBACK_OFFSET,
            ..test_app.fallback
        };
        install_topology(
            &mut test_app,
            TARGET_RETURN_REVISION,
            [(different_entity, different_unverified)],
        );
        test_app.app.update();

        assert!(matches!(
            phase(&test_app),
            FallbackAndReturnPhase::FallbackSettling(_)
        ));
        assert_eq!(
            test_app
                .app
                .world()
                .resource::<CapturedWindowStates>()
                .entry(&test_app.window_key),
            original_state.as_ref(),
        );
    }

    #[test]
    fn fallback_monitor_replacement_restarts_settling_without_adoption() {
        let mut test_app = recovery_app(TestWindowRole::Primary);
        settle_on_fallback(&mut test_app);
        let original_placement = test_app
            .app
            .world()
            .resource::<CapturedWindowStates>()
            .captured_placement(&test_app.window_key)
            .cloned();
        let original_target = test_app
            .app
            .world()
            .resource::<RecoveryRegistrations>()
            .by_key(&test_app.window_key)
            .map(|registration| (registration.monitor_id, registration.target));
        let original_intent = seed_restore_intent(&mut test_app);
        assert!(original_intent.is_some());

        let replacement_entity = test_app.app.world_mut().spawn_empty().id();
        let replacement = MonitorInfo {
            identity: MonitorIdentity::Verified(FALLBACK_REPLACEMENT_ID),
            ..test_app.fallback
        };
        test_app
            .app
            .world_mut()
            .entity_mut(test_app.window)
            .insert(CurrentMonitor {
                monitor_info:          replacement,
                effective_window_mode: WindowMode::Windowed,
            });
        install_topology(
            &mut test_app,
            TARGET_RETURN_REVISION,
            [(replacement_entity, replacement)],
        );
        test_app.app.update();

        assert!(matches!(
            phase(&test_app),
            FallbackAndReturnPhase::FallbackSettling(_)
        ));
        advance(
            &mut test_app,
            Duration::from_secs_f32(SETTLE_STABILITY_SECS),
        );
        assert!(matches!(
            phase(&test_app),
            FallbackAndReturnPhase::OnFallback(observation)
                if observation.monitor_snapshot.identity
                    == MonitorIdentity::Verified(FALLBACK_REPLACEMENT_ID)
        ));
        assert_eq!(
            test_app
                .app
                .world()
                .resource::<CapturedWindowStates>()
                .captured_placement(&test_app.window_key),
            original_placement.as_ref(),
        );
        assert_eq!(
            test_app
                .app
                .world()
                .resource::<RecoveryRegistrations>()
                .by_key(&test_app.window_key)
                .map(|registration| (registration.monitor_id, registration.target)),
            original_target,
        );
        assert_eq!(
            test_app
                .app
                .world()
                .resource::<AutomaticRestoreIntents>()
                .get(&test_app.window_key),
            original_intent.as_ref(),
        );
    }

    #[test]
    fn cancellation_and_removal_cover_every_automatic_phase() {
        let observation = fallback_observation();
        let phases = [
            FallbackAndReturnPhase::Healthy,
            FallbackAndReturnPhase::RemovalPending,
            FallbackAndReturnPhase::FallbackSettling(FallbackSettling {
                return_intent: ReturnIntent::Active,
                observation:   None,
                stable_for:    Duration::ZERO,
            }),
            FallbackAndReturnPhase::OnFallback(observation.clone()),
            FallbackAndReturnPhase::Restoring,
            FallbackAndReturnPhase::MissingLiveWindow(ReturnIntent::Active),
            FallbackAndReturnPhase::RetryableFailure(observation),
        ];

        for automatic_phase in phases {
            let mut cancelled = recovery_app(TestWindowRole::Primary);
            if let Some(recovery) = cancelled
                .app
                .world_mut()
                .resource_mut::<FallbackAndReturnRecoveries>()
                .entries
                .get_mut(&cancelled.window_key)
            {
                recovery.phase = automatic_phase.clone();
            }
            cancelled.app.world_mut().trigger(CancelWindowRecovery {
                window: cancelled.window_key.clone(),
            });
            assert!(
                !cancelled
                    .app
                    .world()
                    .resource::<FallbackAndReturnRecoveries>()
                    .entries
                    .contains_key(&cancelled.window_key)
            );

            let mut removed = recovery_app(TestWindowRole::Primary);
            if let Some(recovery) = removed
                .app
                .world_mut()
                .resource_mut::<FallbackAndReturnRecoveries>()
                .entries
                .get_mut(&removed.window_key)
            {
                recovery.phase = automatic_phase;
            }
            removed
                .app
                .world_mut()
                .entity_mut(removed.window)
                .remove::<Window>();
            removed.app.world_mut().flush();
            assert!(matches!(
                phase(&removed),
                FallbackAndReturnPhase::RemovalPending
                    | FallbackAndReturnPhase::MissingLiveWindow(_)
            ));
        }
    }

    #[test]
    fn linked_deletion_before_topology_reconnects_without_fallback_observation() {
        for role in [TestWindowRole::Primary, TestWindowRole::Managed] {
            let mut test_app = recovery_app(role);
            let original = test_app
                .app
                .world()
                .resource::<CapturedWindowStates>()
                .captured_placement(&test_app.window_key)
                .cloned();
            test_app
                .app
                .world_mut()
                .entity_mut(test_app.window)
                .remove::<Window>();
            test_app.app.world_mut().flush();
            assert!(matches!(
                phase(&test_app),
                FallbackAndReturnPhase::RemovalPending
            ));

            let fallback = (test_app.fallback_entity, test_app.fallback);
            install_topology(&mut test_app, LOSS_REVISION, [fallback]);
            test_app.app.update();
            assert!(matches!(
                phase(&test_app),
                FallbackAndReturnPhase::MissingLiveWindow(_)
            ));

            let target = (test_app.target_entity, test_app.target);
            let fallback = (test_app.fallback_entity, test_app.fallback);
            install_topology(&mut test_app, TARGET_RETURN_REVISION, [target, fallback]);
            test_app.app.update();
            let intent = test_app
                .app
                .world()
                .resource::<AutomaticRestoreIntents>()
                .get(&test_app.window_key);
            assert_eq!(intent.map(|intent| intent.entity), Some(None));
            assert_eq!(
                test_app
                    .app
                    .world()
                    .resource::<CapturedWindowStates>()
                    .captured_placement(&test_app.window_key),
                original.as_ref(),
            );
        }
    }

    #[test]
    fn missing_window_target_loss_replaces_stale_restore_intent_on_return() {
        let mut test_app = recovery_app(TestWindowRole::Primary);
        settle_on_fallback(&mut test_app);
        let target = (test_app.target_entity, test_app.target);
        let fallback = (test_app.fallback_entity, test_app.fallback);
        install_topology(&mut test_app, TARGET_RETURN_REVISION, [target, fallback]);
        test_app.app.update();
        assert!(matches!(
            phase(&test_app),
            FallbackAndReturnPhase::OnFallback(_)
        ));

        test_app
            .app
            .world_mut()
            .entity_mut(test_app.window)
            .remove::<Window>();
        test_app.app.world_mut().flush();
        assert_eq!(
            test_app
                .app
                .world()
                .resource::<AutomaticRestoreIntents>()
                .get(&test_app.window_key)
                .map(|intent| intent.entity),
            Some(None),
        );

        let fallback = (test_app.fallback_entity, test_app.fallback);
        install_topology(&mut test_app, FALLBACK_LOSS_REVISION, [fallback]);
        test_app.app.update();
        assert!(
            test_app
                .app
                .world()
                .resource::<AutomaticRestoreIntents>()
                .get(&test_app.window_key)
                .is_none()
        );

        let returned_target = MonitorInfo {
            physical_position: test_app.target.physical_position + REARRANGED_TARGET_OFFSET,
            ..test_app.target
        };
        let target = (test_app.target_entity, returned_target);
        let fallback = (test_app.fallback_entity, test_app.fallback);
        install_topology(&mut test_app, TARGET_REAPPEAR_REVISION, [target, fallback]);
        test_app.app.update();

        assert!(matches!(
            phase(&test_app),
            FallbackAndReturnPhase::MissingLiveWindow(_)
        ));
        let intent = test_app
            .app
            .world()
            .resource::<AutomaticRestoreIntents>()
            .get(&test_app.window_key)
            .cloned();
        assert_eq!(intent.as_ref().map(|intent| intent.entity), Some(None));
        assert_eq!(
            intent.as_ref().map(|intent| intent.monitor),
            Some(returned_target),
        );
        assert_eq!(
            intent.map(|intent| intent.revision),
            Some(MonitorTopologyRevision::from_test_raw(
                TARGET_REAPPEAR_REVISION,
            )),
        );
    }

    #[test]
    fn identity_only_and_replacement_revisions_each_evaluate_once() {
        let mut test_app = recovery_app(TestWindowRole::Primary);
        move_to_fallback(&mut test_app);
        let identity_only_target = monitor(MonitorIdentity::Unverified, TARGET_INDEX, IVec2::ZERO);
        let target_entity = test_app.target_entity;
        let fallback = (test_app.fallback_entity, test_app.fallback);
        install_topology(
            &mut test_app,
            LOSS_REVISION,
            [(target_entity, identity_only_target), fallback],
        );
        test_app.app.update();
        test_app.app.update();
        let evaluations = test_app
            .app
            .world()
            .resource::<FallbackAndReturnRecoveries>()
            .entries[&test_app.window_key]
            .topology_evaluations;
        assert_eq!(evaluations, 1);

        let replacement_entity = test_app.app.world_mut().spawn_empty().id();
        let evaluations_before_replacement = topology_evaluations(&test_app);
        let target = test_app.target;
        let fallback = (test_app.fallback_entity, test_app.fallback);
        install_topology(
            &mut test_app,
            TARGET_RETURN_REVISION,
            [(replacement_entity, target), fallback],
        );
        test_app.app.update();
        test_app.app.update();
        let recovery = &test_app
            .app
            .world()
            .resource::<FallbackAndReturnRecoveries>()
            .entries[&test_app.window_key];
        assert_eq!(
            recovery.topology_evaluations,
            evaluations_before_replacement + 1,
        );
        assert!(matches!(
            recovery.phase,
            FallbackAndReturnPhase::FallbackSettling(_)
        ));
        assert_eq!(
            test_app
                .app
                .world()
                .resource::<AutomaticRestoreIntents>()
                .entries
                .len(),
            1,
        );
    }

    #[test]
    fn rearrangement_only_revision_has_no_recovery_transition() {
        let mut test_app = recovery_app(TestWindowRole::Primary);
        let original_placement = test_app
            .app
            .world()
            .resource::<CapturedWindowStates>()
            .captured_placement(&test_app.window_key)
            .cloned();
        let rearranged_target = MonitorInfo {
            physical_position: test_app.target.physical_position + REARRANGED_TARGET_OFFSET,
            ..test_app.target
        };
        let target_entity = test_app.target_entity;
        let fallback = (test_app.fallback_entity, test_app.fallback);
        install_topology(
            &mut test_app,
            LOSS_REVISION,
            [(target_entity, rearranged_target), fallback],
        );
        test_app.app.update();
        test_app.app.update();

        assert_eq!(topology_evaluations(&test_app), 1);
        assert!(matches!(phase(&test_app), FallbackAndReturnPhase::Healthy));
        assert_eq!(
            test_app
                .app
                .world()
                .resource::<CapturedWindowStates>()
                .captured_placement(&test_app.window_key),
            original_placement.as_ref(),
        );
        assert!(
            test_app
                .app
                .world()
                .resource::<AutomaticRestoreIntents>()
                .get(&test_app.window_key)
                .is_none()
        );
        assert!(
            test_app
                .app
                .world()
                .resource::<RecoveryFacts>()
                .pending
                .is_empty()
        );
    }

    #[test]
    fn repeated_reconnect_updates_do_not_repeat_recovery_transition() {
        let mut test_app = recovery_app(TestWindowRole::Primary);
        settle_on_fallback(&mut test_app);
        let evaluations_before_reconnect = topology_evaluations(&test_app);
        let target = (test_app.target_entity, test_app.target);
        let fallback = (test_app.fallback_entity, test_app.fallback);
        install_topology(&mut test_app, TARGET_RETURN_REVISION, [target, fallback]);
        test_app.app.update();
        let reconnect_intent = test_app
            .app
            .world()
            .resource::<AutomaticRestoreIntents>()
            .get(&test_app.window_key)
            .cloned();

        test_app.app.update();
        test_app.app.update();

        assert_eq!(
            topology_evaluations(&test_app),
            evaluations_before_reconnect + 1,
        );
        assert!(matches!(
            phase(&test_app),
            FallbackAndReturnPhase::OnFallback(_)
        ));
        assert_eq!(
            test_app
                .app
                .world()
                .resource::<AutomaticRestoreIntents>()
                .get(&test_app.window_key),
            reconnect_intent.as_ref(),
        );
        assert!(reconnect_intent.is_some());
    }

    #[test]
    fn combined_disconnect_connect_revision_evaluates_once() {
        let mut test_app = recovery_app(TestWindowRole::Primary);
        let replacement_entity = test_app.app.world_mut().spawn_empty().id();
        let replacement = MonitorInfo {
            identity: MonitorIdentity::Verified(FALLBACK_REPLACEMENT_ID),
            ..test_app.fallback
        };
        {
            let mut entity = test_app.app.world_mut().entity_mut(test_app.window);
            if let Some(mut window) = entity.get_mut::<Window>() {
                window.position = WindowPosition::At(replacement.physical_position + IVec2::ONE);
            }
            entity.insert(CurrentMonitor {
                monitor_info:          replacement,
                effective_window_mode: WindowMode::Windowed,
            });
        }
        install_topology(
            &mut test_app,
            LOSS_REVISION,
            [(replacement_entity, replacement)],
        );
        test_app.app.update();
        test_app.app.update();

        assert_eq!(topology_evaluations(&test_app), 1);
        assert!(matches!(
            phase(&test_app),
            FallbackAndReturnPhase::FallbackSettling(_)
        ));
        assert_eq!(
            test_app.app.world().resource::<RecoveryFacts>().pending,
            [(test_app.window_key.clone(), TARGET_ID)],
        );
        assert!(
            test_app
                .app
                .world()
                .resource::<AutomaticRestoreIntents>()
                .get(&test_app.window_key)
                .is_none()
        );
    }

    #[test]
    fn unverified_registration_remains_pending() {
        let test_app = recovery_app_with(
            TestWindowRole::Primary,
            Platform::Windows,
            MonitorIdentity::Unverified,
            CapturedWindowPosition::Restorable {
                logical_offset: LOGICAL_OFFSET,
            },
            SavedWindowMode::Windowed,
        );

        assert!(
            !test_app
                .app
                .world()
                .resource::<FallbackAndReturnRecoveries>()
                .entries
                .contains_key(&test_app.window_key)
        );
        assert_eq!(
            recovery::registration_snapshot(test_app.app.world()).pending,
            1,
        );
    }

    #[test]
    fn frozen_intent_is_not_replaced_by_initial_fallback() {
        let mut test_app = recovery_app(TestWindowRole::Primary);
        let original = test_app
            .app
            .world()
            .resource::<CapturedWindowStates>()
            .captured_placement(&test_app.window_key)
            .cloned();
        lose_target(&mut test_app, LOSS_REVISION);
        test_app.app.update();

        let states = test_app.app.world().resource::<CapturedWindowStates>();
        assert_eq!(
            states.captured_placement(&test_app.window_key),
            original.as_ref(),
        );
        assert!(matches!(
            states
                .entry(&test_app.window_key)
                .map(|entry| &entry.placement),
            Some(CapturedPlacement::Captured(_))
        ));
    }
}
