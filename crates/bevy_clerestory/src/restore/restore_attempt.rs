//! Shared restore preparation and retained restore origin.

use std::time::Duration;

use bevy::prelude::*;
use bevy::time::Virtual;
use bevy::window::OnMonitor;
use bevy::window::PrimaryWindow;

use super::target_position;
use super::target_position::MonitorResolutionSource;
use super::target_position::PreparedWindowPosition;
use super::target_position::RestoreDiagnostics;
use super::target_position::TargetPosition;
use super::winit_info::WinitInfo;
use super::winit_info::X11FrameCompensated;
use crate::ManagedWindow;
use crate::Platform;
use crate::WindowKey;
use crate::WindowRestoreMismatch;
use crate::WindowRestored;
#[cfg(test)]
use crate::constants::SCALE_FACTOR_EPSILON;
use crate::constants::SETTLE_TIMEOUT_SECS;
use crate::managed::ManagedWindowRegistry;
use crate::monitors;
use crate::monitors::CurrentMonitor;
use crate::monitors::MonitorIdentity;
use crate::monitors::MonitorInfo;
use crate::monitors::MonitorTopologyRevision;
use crate::monitors::Monitors;
use crate::persistence::CapturedPlacement;
use crate::persistence::CapturedWindowPlacement;
use crate::persistence::CapturedWindowStates;
use crate::persistence::PersistedWindowState;
use crate::persistence::RebasedCapturedPosition;
use crate::recovery::ApplicationControlledRecoveries;
use crate::recovery::AutomaticRestoreIntent;
use crate::recovery::AutomaticRestoreIntents;
use crate::recovery::CanonicalWindowRole;
use crate::recovery::ExplicitRestoreRequests;
use crate::recovery::FallbackAndReturnRecoveries;
use crate::recovery::PrimaryPresence;
use crate::recovery::RecoveryGeneration;
use crate::recovery::RecoveryRegistrations;
use crate::recovery::WindowRecovery;
use crate::recovery::canonical_window;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Reflect)]
pub(crate) struct RestoreAttemptId(u64);

#[cfg(test)]
impl RestoreAttemptId {
    pub(crate) const fn from_test_raw(raw: u64) -> Self { Self(raw) }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RestoreAttempt {
    id:                RestoreAttemptId,
    window_key:        WindowKey,
    entity:            Entity,
    generation:        RecoveryGeneration,
    expected_monitor:  crate::MonitorId,
    topology_revision: MonitorTopologyRevision,
    deadline:          Duration,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RestoreDisposition {
    Succeeded,
    Failed,
}

#[derive(Clone, Copy)]
enum RestoreAttemptIdState {
    Next(u64),
    Exhausted,
}

#[derive(Resource)]
pub(crate) struct RestoreAttemptIds {
    state: RestoreAttemptIdState,
}

impl Default for RestoreAttemptIds {
    fn default() -> Self {
        Self {
            state: RestoreAttemptIdState::Next(0),
        }
    }
}

impl RestoreAttemptIds {
    const fn allocate(&mut self) -> Option<RestoreAttemptId> {
        let RestoreAttemptIdState::Next(next) = self.state else {
            return None;
        };
        self.state = match next.checked_add(1) {
            Some(following) => RestoreAttemptIdState::Next(following),
            None => RestoreAttemptIdState::Exhausted,
        };
        Some(RestoreAttemptId(next))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum RestoreOrigin {
    Startup { window_key: WindowKey },
    Recovery(RestoreAttempt),
}

/// Marks that `init_winit_info` or an accepted `OnMonitor` association established
/// native-window readiness.
#[derive(Component)]
pub(crate) struct NativeWindowReady;

#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub(crate) struct RestorePreparation {
    origin: RestoreOrigin,
}

impl RestorePreparation {
    #[must_use]
    pub(crate) const fn startup(window_key: WindowKey) -> Self {
        Self {
            origin: RestoreOrigin::Startup { window_key },
        }
    }

    #[must_use]
    const fn recovery(restore_attempt: RestoreAttempt) -> Self {
        Self {
            origin: RestoreOrigin::Recovery(restore_attempt),
        }
    }

    #[must_use]
    pub(crate) const fn window_key(&self) -> &WindowKey {
        match &self.origin {
            RestoreOrigin::Startup { window_key } => window_key,
            RestoreOrigin::Recovery(restore_attempt) => &restore_attempt.window_key,
        }
    }

    #[must_use]
    pub(crate) const fn attempt_id(&self) -> Option<RestoreAttemptId> {
        match &self.origin {
            RestoreOrigin::Startup { .. } => None,
            RestoreOrigin::Recovery(restore_attempt) => Some(restore_attempt.id),
        }
    }

    #[must_use]
    pub(crate) const fn recovery_attempt(&self) -> Option<&RestoreAttempt> {
        match &self.origin {
            RestoreOrigin::Startup { .. } => None,
            RestoreOrigin::Recovery(restore_attempt) => Some(restore_attempt),
        }
    }

    const fn origin(&self) -> &RestoreOrigin { &self.origin }
}

/// Accept or clear native readiness when Bevy changes a window's monitor association.
pub(crate) fn mark_native_window_ready(
    insert: On<Insert, OnMonitor>,
    windows: Query<(&Window, &OnMonitor), Or<(With<PrimaryWindow>, With<ManagedWindow>)>>,
    monitors: Res<Monitors>,
    mut commands: Commands,
) {
    let Ok((window, on_monitor)) = windows.get(insert.entity) else {
        return;
    };
    let mut entity = commands.entity(insert.entity);
    if let Some(current_monitor) =
        monitors::current_monitor_from_association(window, on_monitor, &monitors)
    {
        entity.insert((current_monitor, NativeWindowReady));
    } else {
        entity.remove::<(CurrentMonitor, NativeWindowReady)>();
    }
}

pub(crate) fn clear_native_window_ready(remove: On<Remove, OnMonitor>, mut commands: Commands) {
    commands
        .entity(remove.entity)
        .try_remove::<(CurrentMonitor, NativeWindowReady)>();
}

#[cfg(all(target_os = "linux", feature = "workaround-winit-4445"))]
type RestoreAttemptComponents = (
    RestorePreparation,
    TargetPosition,
    X11FrameCompensated,
    crate::x11_position_fix::X11FrameTop,
);

#[cfg(not(all(target_os = "linux", feature = "workaround-winit-4445")))]
type RestoreAttemptComponents = (RestorePreparation, TargetPosition, X11FrameCompensated);

pub(crate) fn cancel_restore(commands: &mut Commands, entity: Entity) {
    commands.entity(entity).queue(|mut entity: EntityWorldMut| {
        entity.remove::<RestoreAttemptComponents>();
        if let Some(mut window) = entity.get_mut::<Window>() {
            window.visible = true;
        }
    });
}

fn restore_deadline(time: &Time<Virtual>) -> Duration {
    time.elapsed() + Duration::from_secs_f32(SETTLE_TIMEOUT_SECS)
}

fn canonical_request(
    entity: Entity,
    primary_windows: &Query<(), With<PrimaryWindow>>,
    managed_window_registry: &ManagedWindowRegistry,
) -> Option<(WindowKey, CanonicalWindowRole)> {
    canonical_window(
        entity,
        if primary_windows.contains(entity) {
            PrimaryPresence::Present
        } else {
            PrimaryPresence::Absent
        },
        managed_window_registry,
    )
}

pub(crate) fn accept_explicit_restore_requests(
    mut commands: Commands,
    mut requests: ResMut<ExplicitRestoreRequests>,
    windows: Query<(), With<Window>>,
    primary_windows: Query<(), With<PrimaryWindow>>,
    preparations: Query<(), With<RestorePreparation>>,
    managed_window_registry: Res<ManagedWindowRegistry>,
    monitors: Res<Monitors>,
    revision: Res<MonitorTopologyRevision>,
    time: Res<Time<Virtual>>,
    mut ids: ResMut<RestoreAttemptIds>,
    mut registrations: ResMut<RecoveryRegistrations>,
    mut recoveries: ResMut<ApplicationControlledRecoveries>,
    mut captured_window_states: ResMut<CapturedWindowStates>,
) {
    let entities: Vec<_> = requests.drain().collect();
    for entity in entities {
        if !windows.contains(entity) || preparations.contains(entity) {
            continue;
        }
        let Some((window_key, role)) =
            canonical_request(entity, &primary_windows, &managed_window_registry)
        else {
            continue;
        };
        let Some(registration) = registrations.by_key(&window_key).cloned() else {
            continue;
        };
        let binding_matches = match registration.entity {
            Some(bound_entity) => {
                bound_entity == entity && captured_window_states.is_bound_to(&window_key, entity)
            },
            None => captured_window_states.placement(&window_key).is_some(),
        };
        if registration.policy != WindowRecovery::ApplicationControlled
            || registration.role != role
            || !binding_matches
            || monitors.by_id(registration.monitor_id).is_none()
            || !recoveries.can_begin_restore(&window_key, registration.generation)
        {
            continue;
        }
        let Some(id) = ids.allocate() else {
            continue;
        };
        let restore_attempt = RestoreAttempt {
            id,
            window_key: window_key.clone(),
            entity,
            generation: registration.generation,
            expected_monitor: registration.monitor_id,
            topology_revision: *revision,
            deadline: restore_deadline(&time),
        };
        if registration.entity.is_none() {
            if !captured_window_states.bind_and_freeze(&window_key, entity) {
                continue;
            }
            let Some(registration) = registrations.by_key_mut(&window_key) else {
                continue;
            };
            registration.entity = Some(entity);
        }
        if !recoveries.begin_restore(&window_key, registration.generation) {
            continue;
        }
        commands
            .entity(entity)
            .insert(RestorePreparation::recovery(restore_attempt));
    }
}

pub(crate) fn accept_automatic_restore_intents(
    mut commands: Commands,
    windows: Query<(), With<Window>>,
    preparations: Query<(), With<RestorePreparation>>,
    monitors: Res<Monitors>,
    revision: Res<MonitorTopologyRevision>,
    time: Res<Time<Virtual>>,
    mut ids: ResMut<RestoreAttemptIds>,
    registrations: Res<RecoveryRegistrations>,
    mut recoveries: ResMut<FallbackAndReturnRecoveries>,
    mut restore_intents: ResMut<AutomaticRestoreIntents>,
    captured_window_states: Res<CapturedWindowStates>,
) {
    let pending: Vec<(WindowKey, AutomaticRestoreIntent)> = restore_intents
        .pending()
        .map(|(window_key, intent)| (window_key.clone(), intent.clone()))
        .collect();
    for (window_key, intent) in pending {
        let Some(entity) = intent.entity else {
            continue;
        };
        if !windows.contains(entity) || preparations.contains(entity) {
            continue;
        }
        let Some(registration) = registrations.by_key(&window_key).cloned() else {
            continue;
        };
        let MonitorIdentity::Verified(expected_monitor) = intent.monitor.identity else {
            continue;
        };
        if registration.policy != WindowRecovery::FallbackAndReturn
            || registration.generation != intent.generation
            || registration.entity != Some(entity)
            || registration.monitor_id != expected_monitor
            || intent.revision != *revision
            || monitors.by_id(expected_monitor) != Some(&intent.monitor)
            || !captured_window_states.is_bound_to(&window_key, entity)
            || !recoveries.can_begin_restore(&window_key, intent.generation)
        {
            continue;
        }
        let Some(id) = ids.allocate() else {
            continue;
        };
        let restore_attempt = RestoreAttempt {
            id,
            window_key: window_key.clone(),
            entity,
            generation: intent.generation,
            expected_monitor,
            topology_revision: intent.revision,
            deadline: restore_deadline(&time),
        };
        if !recoveries.begin_restore(&window_key, intent.generation) {
            continue;
        }
        restore_intents.consume(&window_key, intent.generation);
        commands
            .entity(entity)
            .insert(RestorePreparation::recovery(restore_attempt));
    }
}

fn restore_attempt_is_current(
    restore_attempt: &RestoreAttempt,
    entity: Entity,
    registrations: &RecoveryRegistrations,
    monitors: &Monitors,
    revision: MonitorTopologyRevision,
) -> bool {
    restore_attempt.entity == entity
        && restore_attempt.topology_revision == revision
        && monitors.by_id(restore_attempt.expected_monitor).is_some()
        && registrations
            .by_key(&restore_attempt.window_key)
            .is_some_and(|registration| {
                registration.entity == Some(entity)
                    && registration.generation == restore_attempt.generation
                    && registration.monitor_id == restore_attempt.expected_monitor
            })
}

fn finish_recovery_lifecycle(
    restore_attempt: &RestoreAttempt,
    disposition: RestoreDisposition,
    registrations: &RecoveryRegistrations,
    application_controlled: &mut ApplicationControlledRecoveries,
    fallback_and_return: &mut FallbackAndReturnRecoveries,
) -> bool {
    let Some(registration) = registrations.by_key(&restore_attempt.window_key) else {
        return false;
    };
    match registration.policy {
        WindowRecovery::ApplicationControlled => application_controlled.finish_restore(
            &restore_attempt.window_key,
            restore_attempt.generation,
            disposition,
        ),
        WindowRecovery::FallbackAndReturn => fallback_and_return.finish_restore(
            &restore_attempt.window_key,
            restore_attempt.generation,
            restore_attempt.entity,
            disposition,
        ),
        WindowRecovery::Disabled => false,
    }
}

pub(crate) fn reject_stale_restore_attempts(
    mut commands: Commands,
    preparations: Query<(Entity, &RestorePreparation)>,
    registrations: Res<RecoveryRegistrations>,
    monitors: Res<Monitors>,
    revision: Res<MonitorTopologyRevision>,
    mut application_controlled: ResMut<ApplicationControlledRecoveries>,
    mut fallback_and_return: ResMut<FallbackAndReturnRecoveries>,
) {
    for (entity, restore_preparation) in &preparations {
        let RestoreOrigin::Recovery(restore_attempt) = restore_preparation.origin() else {
            continue;
        };
        if restore_attempt_is_current(
            restore_attempt,
            entity,
            &registrations,
            &monitors,
            *revision,
        ) {
            continue;
        }
        finish_recovery_lifecycle(
            restore_attempt,
            RestoreDisposition::Failed,
            &registrations,
            &mut application_controlled,
            &mut fallback_and_return,
        );
        cancel_restore(&mut commands, entity);
    }
}

#[derive(Clone)]
pub(crate) enum RuntimeRestoreOutcome {
    Restored(WindowRestored),
    Mismatch(WindowRestoreMismatch),
}

#[derive(EntityEvent)]
pub(crate) struct RuntimeRestoreCompletion {
    entity:          Entity,
    restore_attempt: RestoreAttempt,
    outcome:         RuntimeRestoreOutcome,
}

impl RuntimeRestoreCompletion {
    pub(crate) const fn new(
        restore_attempt: RestoreAttempt,
        outcome: RuntimeRestoreOutcome,
    ) -> Self {
        Self {
            entity: restore_attempt.entity,
            restore_attempt,
            outcome,
        }
    }
}

pub(crate) fn validate_runtime_restore_completion(
    completion: On<RuntimeRestoreCompletion>,
    preparations: Query<&RestorePreparation>,
    registrations: Res<RecoveryRegistrations>,
    monitors: Res<Monitors>,
    revision: Res<MonitorTopologyRevision>,
    mut application_controlled: ResMut<ApplicationControlledRecoveries>,
    mut fallback_and_return: ResMut<FallbackAndReturnRecoveries>,
    mut commands: Commands,
) {
    let Ok(restore_preparation) = preparations.get(completion.entity) else {
        return;
    };
    if restore_preparation.origin() != &RestoreOrigin::Recovery(completion.restore_attempt.clone())
        || !restore_attempt_is_current(
            &completion.restore_attempt,
            completion.entity,
            &registrations,
            &monitors,
            *revision,
        )
    {
        return;
    }
    let disposition = match completion.outcome {
        RuntimeRestoreOutcome::Restored(_) => RestoreDisposition::Succeeded,
        RuntimeRestoreOutcome::Mismatch(_) => RestoreDisposition::Failed,
    };
    if !finish_recovery_lifecycle(
        &completion.restore_attempt,
        disposition,
        &registrations,
        &mut application_controlled,
        &mut fallback_and_return,
    ) {
        cancel_restore(&mut commands, completion.entity);
        return;
    }
    match completion.outcome.clone() {
        RuntimeRestoreOutcome::Restored(restored) => commands.trigger(restored),
        RuntimeRestoreOutcome::Mismatch(mismatch) => commands.trigger(mismatch),
    }
    cancel_restore(&mut commands, completion.entity);
}

struct RestoreTargetBuilder<'a> {
    source:              &'a CapturedPlacement,
    monitors:            &'a Monitors,
    recovery_monitor:    Option<&'a MonitorInfo>,
    physical_decoration: UVec2,
    starting_scale:      f64,
    platform:            Platform,
}

struct PreparedRestore {
    target_position:           TargetPosition,
    monitor_resolution_source: MonitorResolutionSource,
}

impl RestoreTargetBuilder<'_> {
    fn build(&self) -> PreparedRestore {
        match self.source {
            CapturedPlacement::PersistedOnly(persisted_window_state) => {
                self.build_persisted(persisted_window_state)
            },
            CapturedPlacement::Captured(captured_window_placement) => {
                self.build_captured(captured_window_placement)
            },
        }
    }

    fn build_persisted(&self, persisted_window_state: &PersistedWindowState) -> PreparedRestore {
        let resolved_monitor = target_position::resolve_target_monitor_and_position(
            persisted_window_state.monitor,
            persisted_window_state.logical_position,
            self.monitors,
        );
        let prepared_window_position = match (
            resolved_monitor.monitor_resolution_source,
            resolved_monitor.logical_position,
        ) {
            (MonitorResolutionSource::FallbackToPrimary, _) => {
                PreparedWindowPosition::TargetUnavailable
            },
            (MonitorResolutionSource::Requested, Some((x, y))) => {
                PreparedWindowPosition::PersistedCoordinate(IVec2::new(x, y))
            },
            (MonitorResolutionSource::Requested, None) => {
                PreparedWindowPosition::PersistedWithoutCoordinate
            },
        };
        let target_position = target_position::compute_target_position(
            persisted_window_state,
            resolved_monitor.monitor_info,
            prepared_window_position,
            self.physical_decoration,
            self.starting_scale,
            self.platform,
        );
        PreparedRestore {
            target_position,
            monitor_resolution_source: resolved_monitor.monitor_resolution_source,
        }
    }

    fn build_captured(
        &self,
        captured_window_placement: &CapturedWindowPlacement,
    ) -> PreparedRestore {
        let persisted_window_state = captured_window_placement.project("");
        let (monitor_info, monitor_resolution_source) =
            self.resolve_captured_monitor(captured_window_placement, &persisted_window_state);
        let prepared_window_position = if matches!(
            monitor_resolution_source,
            MonitorResolutionSource::FallbackToPrimary
        ) {
            PreparedWindowPosition::TargetUnavailable
        } else {
            match captured_window_placement.rebased_position(monitor_info) {
                RebasedCapturedPosition::Restorable {
                    physical_position,
                    logical_position,
                } => PreparedWindowPosition::CapturedRestorable {
                    physical_position,
                    logical_position,
                },
                RebasedCapturedPosition::CompositorControlled => {
                    PreparedWindowPosition::CompositorControlled
                },
            }
        };
        let target_position = target_position::compute_target_position(
            &persisted_window_state,
            monitor_info,
            prepared_window_position,
            self.physical_decoration,
            self.starting_scale,
            self.platform,
        );
        PreparedRestore {
            target_position,
            monitor_resolution_source,
        }
    }

    fn resolve_captured_monitor<'a>(
        &'a self,
        captured_window_placement: &CapturedWindowPlacement,
        persisted_window_state: &PersistedWindowState,
    ) -> (&'a MonitorInfo, MonitorResolutionSource) {
        if let Some(recovery_monitor) = self.recovery_monitor {
            return (recovery_monitor, MonitorResolutionSource::Requested);
        }
        match captured_window_placement.monitor_snapshot.identity {
            MonitorIdentity::Verified(monitor_id) => self.monitors.by_id(monitor_id).map_or_else(
                || {
                    (
                        self.monitors.first(),
                        MonitorResolutionSource::FallbackToPrimary,
                    )
                },
                |monitor_info| (monitor_info, MonitorResolutionSource::Requested),
            ),
            MonitorIdentity::Unverified => {
                let resolved_monitor = target_position::resolve_target_monitor_and_position(
                    persisted_window_state.monitor,
                    persisted_window_state.logical_position,
                    self.monitors,
                );
                (
                    resolved_monitor.monitor_info,
                    resolved_monitor.monitor_resolution_source,
                )
            },
        }
    }
}

pub(crate) fn prepare_restore_targets(
    mut commands: Commands,
    preparations: Query<
        (Entity, &RestorePreparation, &CurrentMonitor),
        (
            With<Window>,
            With<NativeWindowReady>,
            Without<TargetPosition>,
        ),
    >,
    monitors: Res<Monitors>,
    winit_info: Option<Res<WinitInfo>>,
    captured_window_states: Res<CapturedWindowStates>,
    platform: Res<Platform>,
) {
    let Some(winit_info) = winit_info else {
        return;
    };
    if monitors.is_empty() {
        return;
    }

    for (entity, restore_preparation, current_monitor) in &preparations {
        let window_key = restore_preparation.window_key();
        let recovery_monitor = match restore_preparation.origin() {
            RestoreOrigin::Startup { .. } => None,
            RestoreOrigin::Recovery(restore_attempt) => {
                if restore_attempt.entity != entity {
                    continue;
                }
                let Some(monitor) = monitors.by_id(restore_attempt.expected_monitor) else {
                    continue;
                };
                Some(monitor)
            },
        };
        let Some(prepared_restore) = captured_window_states.placement(window_key).map(|source| {
            RestoreTargetBuilder {
                source,
                monitors: &monitors,
                recovery_monitor,
                physical_decoration: winit_info.physical_decoration(),
                starting_scale: current_monitor.scale,
                platform: *platform,
            }
            .build()
        }) else {
            commands
                .entity(entity)
                .remove::<RestorePreparation>()
                .queue(move |mut entity: EntityWorldMut| {
                    if let Some(mut window) = entity.get_mut::<Window>() {
                        window.visible = true;
                    }
                });
            continue;
        };

        if matches!(
            prepared_restore.monitor_resolution_source,
            MonitorResolutionSource::FallbackToPrimary
        ) {
            warn!(
                "[prepare_restore_targets] [{window_key}] target monitor unavailable, falling back to the primary monitor without a retained coordinate"
            );
        }

        let target_position = prepared_restore.target_position;
        let is_fullscreen = target_position.saved_window_mode.is_fullscreen();
        let restore_diagnostics = RestoreDiagnostics {
            starting_monitor_index: current_monitor.index,
            starting_scale:         current_monitor.scale,
            target_scale:           target_position.target_scale,
            monitor_scale_strategy: target_position.monitor_scale_strategy,
        };
        commands
            .entity(entity)
            .insert((target_position, restore_diagnostics));

        if is_fullscreen || !platform.needs_frame_compensation() {
            commands.entity(entity).insert(X11FrameCompensated);
        }

        #[cfg(all(target_os = "windows", feature = "workaround-winit-3124"))]
        if is_fullscreen {
            commands.queue(move |world: &mut World| {
                if let Some(mut window) = world.get_mut::<Window>(entity) {
                    window.visible = true;
                }
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use bevy::time::TimePlugin;
    use bevy::time::TimeUpdateStrategy;
    use bevy::window::OnMonitor;
    use bevy::window::PrimaryWindow;
    use bevy::window::WindowMode;
    use bevy::window::WindowPosition;
    use bevy::window::WindowScaleFactorChanged;
    use bevy_kana::ToF32;

    use super::*;
    use crate::CancelWindowRecovery;
    use crate::ManagedWindow;
    use crate::ManagedWindowPersistence;
    use crate::WindowRecovery;
    use crate::WindowRestoreMismatch;
    use crate::WindowRestored;
    use crate::managed::ManagedWindowRegistry;
    use crate::managed::on_managed_window_added;
    use crate::managed::on_managed_window_load;
    use crate::managed::on_managed_window_removed;
    use crate::monitors::InjectedCurrentMonitorSource;
    use crate::monitors::MonitorId;
    use crate::monitors::MonitorTopologyRevision;
    use crate::monitors::NativeQueryActivity;
    use crate::persistence::CapturedWindowPosition;
    use crate::persistence::PersistencePlugin;
    use crate::persistence::SavedWindowMode;
    use crate::recovery::RecoveryGeneration;
    use crate::recovery::RecoveryPlugin;
    use crate::restore::RestorePlugin;
    use crate::restore::restore_windows;
    use crate::restore::settle_state::SettleState;
    use crate::restore::target_position::InjectedWinitWindows;
    use crate::restore::target_position::MonitorScaleStrategy;
    use crate::restore::target_position::WindowRestoreState;
    use crate::restore_window_config::RestoreWindowConfig;

    const CAPTURED_OFFSET: IVec2 = IVec2::new(100, 50);
    const STALE_SENTINEL_POSITION: IVec2 = IVec2::new(-432, 765);
    const STALE_SENTINEL_SIZE: UVec2 = UVec2::new(321, 234);
    const TARGET_ID: MonitorId = MonitorId::from_test_raw(7);
    const WRONG_SCALE_DELTA: f64 = 0.5;

    #[derive(Clone, Copy)]
    enum StartupReadiness {
        Waiting,
        Associated,
    }

    #[derive(Debug, PartialEq)]
    struct TargetSnapshot {
        physical_position:      Option<IVec2>,
        logical_position:       Option<IVec2>,
        physical_size:          UVec2,
        logical_size:           UVec2,
        target_scale:           f64,
        starting_scale:         f64,
        monitor_scale_strategy: MonitorScaleStrategy,
        saved_window_mode:      SavedWindowMode,
        monitor_index:          usize,
    }

    #[derive(Debug, PartialEq)]
    struct WindowSnapshot {
        position:      WindowPosition,
        physical_size: UVec2,
        window_mode:   WindowMode,
        visible:       bool,
    }

    #[derive(Default, Resource)]
    struct RestoreOutcomeCounts {
        restored:   usize,
        mismatched: usize,
    }

    fn record_restored(_: On<WindowRestored>, mut outcomes: ResMut<RestoreOutcomeCounts>) {
        outcomes.restored += 1;
    }

    fn record_mismatch(_: On<WindowRestoreMismatch>, mut outcomes: ResMut<RestoreOutcomeCounts>) {
        outcomes.mismatched += 1;
    }

    impl From<&TargetPosition> for TargetSnapshot {
        fn from(target_position: &TargetPosition) -> Self {
            Self {
                physical_position:      target_position.physical_position,
                logical_position:       target_position.logical_position,
                physical_size:          target_position.physical_size,
                logical_size:           target_position.logical_size,
                target_scale:           target_position.target_scale,
                starting_scale:         target_position.starting_scale,
                monitor_scale_strategy: target_position.monitor_scale_strategy,
                saved_window_mode:      target_position.saved_window_mode.clone(),
                monitor_index:          target_position.monitor_index,
            }
        }
    }

    impl From<&Window> for WindowSnapshot {
        fn from(window: &Window) -> Self {
            Self {
                position:      window.position,
                physical_size: UVec2::new(window.physical_width(), window.physical_height()),
                window_mode:   window.mode,
                visible:       window.visible,
            }
        }
    }

    fn monitor(
        identity: MonitorIdentity,
        index: usize,
        scale: f64,
        physical_position: IVec2,
    ) -> MonitorInfo {
        MonitorInfo {
            identity,
            index,
            scale,
            physical_position,
            physical_size: UVec2::new(1_920, 1_080),
        }
    }

    fn monitors_with_returned_target() -> Monitors {
        monitors_with_returned_target_entities(Entity::from_bits(1), Entity::from_bits(2))
    }

    fn monitors_with_returned_target_entities(
        starting_monitor_entity: Entity,
        target_monitor_entity: Entity,
    ) -> Monitors {
        Monitors::from_test_monitors([
            (
                starting_monitor_entity,
                monitor(MonitorIdentity::Unverified, 0, 1.0, IVec2::ZERO),
            ),
            (
                target_monitor_entity,
                monitor(
                    MonitorIdentity::Verified(TARGET_ID),
                    2,
                    2.0,
                    IVec2::new(2_000, -200),
                ),
            ),
        ])
    }

    fn captured_placement(
        monitor_identity: MonitorIdentity,
        monitor_index: usize,
        position: CapturedWindowPosition,
        saved_window_mode: SavedWindowMode,
    ) -> CapturedWindowPlacement {
        CapturedWindowPlacement {
            monitor_snapshot: monitor(monitor_identity, monitor_index, 1.0, IVec2::new(-1_920, 0)),
            position,
            logical_size: UVec2::new(800, 600),
            saved_window_mode,
            captured_scale: 1.0,
        }
    }

    fn build_target_for_synthetic_runtime(
        captured_window_placement: &CapturedWindowPlacement,
        monitors: &Monitors,
    ) -> TargetPosition {
        RestoreTargetBuilder {
            source: &CapturedPlacement::Captured(captured_window_placement.clone()),
            monitors,
            recovery_monitor: None,
            physical_decoration: UVec2::ZERO,
            starting_scale: 1.0,
            platform: Platform::Windows,
        }
        .build()
        .target_position
    }

    fn captured_startup_app(
        captured_window_placement: CapturedWindowPlacement,
        startup_readiness: StartupReadiness,
    ) -> (App, Entity, Entity) {
        let mut app = App::new();
        let readiness_monitor_entity = app.world_mut().spawn_empty().id();
        let target_monitor_entity = app.world_mut().spawn_empty().id();
        let monitors =
            monitors_with_returned_target_entities(readiness_monitor_entity, target_monitor_entity);
        app.insert_resource(monitors)
            .insert_resource(WinitInfo::default())
            .insert_resource(Platform::Windows)
            .init_resource::<CapturedWindowStates>()
            .init_resource::<InjectedCurrentMonitorSource>()
            .add_observer(monitors::install_current_monitor_from_association)
            .add_observer(mark_native_window_ready)
            .add_observer(clear_native_window_ready)
            .add_systems(
                Update,
                (monitors::update_current_monitor, prepare_restore_targets).chain(),
            );
        let entity = app
            .world_mut()
            .spawn((
                Window::default(),
                PrimaryWindow,
                RestorePreparation::startup(WindowKey::Primary),
            ))
            .id();
        app.world_mut()
            .resource_mut::<CapturedWindowStates>()
            .promote(WindowKey::Primary, entity, captured_window_placement);
        app.world_mut()
            .resource_mut::<CapturedWindowStates>()
            .bind_and_freeze(&WindowKey::Primary, entity);
        if matches!(startup_readiness, StartupReadiness::Associated) {
            app.world_mut()
                .entity_mut(entity)
                .insert(OnMonitor(readiness_monitor_entity));
            app.world_mut().flush();
        }
        (app, entity, readiness_monitor_entity)
    }

    struct RuntimeTestApp {
        app:             App,
        window:          Entity,
        window_key:      WindowKey,
        target_entity:   Entity,
        fallback_entity: Entity,
        target:          MonitorInfo,
        fallback:        MonitorInfo,
        placement:       CapturedWindowPlacement,
        _state_file:     tempfile::NamedTempFile,
    }

    fn runtime_test_app(window_recovery: WindowRecovery) -> RuntimeTestApp {
        runtime_test_app_for(window_recovery, WindowKey::Primary)
    }

    fn configure_runtime_update_schedule(app: &mut App) {
        app.configure_sets(
            Update,
            (
                crate::ClerestoryUpdateSet::MonitorTopology,
                crate::ClerestoryUpdateSet::RecoveryTopology,
                crate::ClerestoryUpdateSet::CurrentMonitor,
                crate::ClerestoryUpdateSet::RecoveryWindow,
                crate::ClerestoryUpdateSet::RestorePreparation,
                crate::ClerestoryUpdateSet::X11Compensation,
                crate::ClerestoryUpdateSet::RestoreApplication,
                crate::ClerestoryUpdateSet::RestoreSettling,
                crate::ClerestoryUpdateSet::Persistence,
            )
                .chain(),
        )
        .add_systems(
            Update,
            (monitors::update_current_monitor, ApplyDeferred)
                .chain()
                .in_set(crate::ClerestoryUpdateSet::CurrentMonitor),
        );
    }

    #[expect(
        clippy::expect_used,
        reason = "runtime persistence tests require a writable temporary state file"
    )]
    fn runtime_test_app_for(
        window_recovery: WindowRecovery,
        window_key: WindowKey,
    ) -> RuntimeTestApp {
        let mut app = App::new();
        let state_file = tempfile::NamedTempFile::new()
            .expect("runtime persistence state file should be available");
        let target_entity = app.world_mut().spawn_empty().id();
        let fallback_entity = app.world_mut().spawn_empty().id();
        let target = monitor(
            MonitorIdentity::Verified(TARGET_ID),
            2,
            2.0,
            IVec2::new(2_000, -200),
        );
        let fallback = monitor(MonitorIdentity::Unverified, 0, 1.0, IVec2::ZERO);
        let placement = CapturedWindowPlacement {
            monitor_snapshot:  target,
            position:          CapturedWindowPosition::Restorable {
                logical_offset: CAPTURED_OFFSET,
            },
            logical_size:      UVec2::new(800, 600),
            saved_window_mode: SavedWindowMode::Windowed,
            captured_scale:    target.scale,
        };
        app.insert_resource(Monitors::from_test_monitors([
            (fallback_entity, fallback),
            (target_entity, target),
        ]))
        .insert_resource(MonitorTopologyRevision::default())
        .insert_resource(Platform::Windows)
        .insert_resource(RestoreWindowConfig {
            path: state_file.path().to_path_buf(),
        })
        .insert_resource(ManagedWindowPersistence::RememberAll)
        .insert_resource(WinitInfo::default())
        .init_resource::<ManagedWindowRegistry>()
        .init_resource::<CapturedWindowStates>()
        .init_resource::<InjectedCurrentMonitorSource>()
        .init_resource::<RestoreOutcomeCounts>()
        .add_message::<WindowScaleFactorChanged>()
        .add_plugins(TimePlugin)
        .add_plugins(RecoveryPlugin)
        .add_plugins(PersistencePlugin)
        .add_plugins(RestorePlugin)
        .add_observer(monitors::install_current_monitor_from_association)
        .add_observer(on_managed_window_added)
        .add_observer(on_managed_window_removed)
        .add_observer(on_managed_window_load)
        .add_observer(record_restored)
        .add_observer(record_mismatch);
        configure_runtime_update_schedule(&mut app);
        let window = app
            .world_mut()
            .spawn((
                Window::default(),
                OnMonitor(target_entity),
                CurrentMonitor {
                    monitor_info:          target,
                    effective_window_mode: WindowMode::Windowed,
                },
                NativeWindowReady,
            ))
            .id();
        match &window_key {
            WindowKey::Primary => {
                app.world_mut().entity_mut(window).insert(PrimaryWindow);
            },
            WindowKey::Managed(name) => {
                app.world_mut()
                    .entity_mut(window)
                    .insert(ManagedWindow { name: name.clone() });
            },
        }
        app.world_mut().flush();
        app.update();
        app.world_mut()
            .resource_mut::<CapturedWindowStates>()
            .promote(window_key.clone(), window, placement.clone());
        app.world_mut().entity_mut(window).insert(window_recovery);
        app.world_mut().flush();
        app.update();
        RuntimeTestApp {
            app,
            window,
            window_key,
            target_entity,
            fallback_entity,
            target,
            fallback,
            placement,
            _state_file: state_file,
        }
    }

    fn install_runtime_topology(
        test_app: &mut RuntimeTestApp,
        raw_revision: u64,
        monitors: impl IntoIterator<Item = (Entity, MonitorInfo)>,
    ) {
        test_app
            .app
            .insert_resource(Monitors::from_test_monitors(monitors));
        test_app
            .app
            .insert_resource(MonitorTopologyRevision::from_test_raw(raw_revision));
    }

    fn settle_automatic_on_fallback(test_app: &mut RuntimeTestApp) {
        {
            let mut entity = test_app.app.world_mut().entity_mut(test_app.window);
            let Some(mut window) = entity.get_mut::<Window>() else {
                return;
            };
            window.position =
                WindowPosition::At(test_app.fallback.physical_position + CAPTURED_OFFSET);
        }
        test_app
            .app
            .world_mut()
            .entity_mut(test_app.window)
            .insert(CurrentMonitor {
                monitor_info:          test_app.fallback,
                effective_window_mode: WindowMode::Windowed,
            });
        install_runtime_topology(test_app, 1, [(test_app.fallback_entity, test_app.fallback)]);
        test_app.app.update();
        test_app
            .app
            .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f32(
                crate::constants::SETTLE_STABILITY_SECS,
            )));
        test_app.app.update();
        test_app
            .app
            .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::ZERO));
    }

    fn return_target(test_app: &mut RuntimeTestApp, raw_revision: u64) {
        install_runtime_topology(
            test_app,
            raw_revision,
            [
                (test_app.fallback_entity, test_app.fallback),
                (test_app.target_entity, test_app.target),
            ],
        );
        test_app.app.update();
    }

    #[test]
    fn startup_and_synthetic_runtime_use_equivalent_target_computation() {
        let captured_window_placement = captured_placement(
            MonitorIdentity::Verified(TARGET_ID),
            1,
            CapturedWindowPosition::Restorable {
                logical_offset: CAPTURED_OFFSET,
            },
            SavedWindowMode::Windowed,
        );
        let synthetic_runtime_target = build_target_for_synthetic_runtime(
            &captured_window_placement,
            &monitors_with_returned_target(),
        );
        let (mut app, entity, readiness_monitor_entity) =
            captured_startup_app(captured_window_placement, StartupReadiness::Associated);

        app.update();

        assert_eq!(
            app.world()
                .get::<OnMonitor>(entity)
                .map(|on_monitor| on_monitor.0),
            Some(readiness_monitor_entity)
        );
        let startup_target = app.world().get::<TargetPosition>(entity);
        assert_eq!(
            startup_target.map(TargetSnapshot::from),
            Some(TargetSnapshot::from(&synthetic_runtime_target))
        );
        assert_eq!(
            startup_target.and_then(|target_position| target_position.physical_position),
            Some(IVec2::new(2_200, -100))
        );
        let captured_window_states = app.world().resource::<CapturedWindowStates>();
        assert!(matches!(
            captured_window_states.placement(&WindowKey::Primary),
            Some(CapturedPlacement::Captured(_))
        ));
        assert_eq!(captured_window_states.activity().file_reads, 0);
        assert_eq!(captured_window_states.activity().projections, 0);
        assert_eq!(captured_window_states.activity().writes, 0);
    }

    #[test]
    fn surviving_automatic_request_uses_startup_target_computation() {
        let mut test_app = runtime_test_app(WindowRecovery::FallbackAndReturn);
        let expected = build_target_for_synthetic_runtime(
            &test_app.placement,
            test_app.app.world().resource::<Monitors>(),
        );

        settle_automatic_on_fallback(&mut test_app);
        return_target(&mut test_app, 2);

        let target_position = test_app.app.world().get::<TargetPosition>(test_app.window);
        assert_eq!(
            target_position.map(TargetSnapshot::from),
            Some(TargetSnapshot::from(&expected)),
        );
        assert!(
            test_app
                .app
                .world()
                .get::<RestorePreparation>(test_app.window)
                .is_some_and(|preparation| preparation.recovery_attempt().is_some())
        );
        assert!(
            test_app
                .app
                .world()
                .resource::<AutomaticRestoreIntents>()
                .pending()
                .next()
                .is_none()
        );
    }

    #[test]
    fn entityless_automatic_intent_remains_pending_without_a_target_position() {
        let mut test_app = runtime_test_app(WindowRecovery::FallbackAndReturn);
        settle_automatic_on_fallback(&mut test_app);
        test_app
            .app
            .world_mut()
            .entity_mut(test_app.window)
            .remove::<Window>();
        test_app.app.world_mut().flush();

        return_target(&mut test_app, 2);

        let intent = test_app
            .app
            .world()
            .resource::<AutomaticRestoreIntents>()
            .pending()
            .next()
            .map(|(_, intent)| intent.clone());
        assert!(intent.is_some_and(|intent| intent.entity.is_none()));
        assert!(
            test_app
                .app
                .world()
                .get::<RestorePreparation>(test_app.window)
                .is_none()
        );
        assert!(
            test_app
                .app
                .world()
                .get::<TargetPosition>(test_app.window)
                .is_none()
        );
    }

    #[test]
    fn automatic_intent_waits_while_its_exact_target_is_absent() {
        let mut test_app = runtime_test_app(WindowRecovery::FallbackAndReturn);
        settle_automatic_on_fallback(&mut test_app);
        let generation = test_app
            .app
            .world()
            .resource::<RecoveryRegistrations>()
            .by_key(&test_app.window_key)
            .map(|registration| registration.generation);
        let Some(generation) = generation else {
            return;
        };
        test_app
            .app
            .world_mut()
            .resource_mut::<AutomaticRestoreIntents>()
            .enqueue(
                test_app.window_key.clone(),
                generation,
                Some(test_app.window),
                test_app.target,
                MonitorTopologyRevision::from_test_raw(1),
            );

        test_app.app.update();

        assert!(
            test_app
                .app
                .world()
                .resource::<AutomaticRestoreIntents>()
                .pending()
                .next()
                .is_some()
        );
        assert!(
            test_app
                .app
                .world()
                .get::<RestorePreparation>(test_app.window)
                .is_none()
        );
        assert!(
            test_app
                .app
                .world()
                .get::<TargetPosition>(test_app.window)
                .is_none()
        );
    }

    fn prepare_application_replacement_target(test_app: &mut RuntimeTestApp) {
        install_runtime_topology(test_app, 1, [(test_app.fallback_entity, test_app.fallback)]);
        test_app.app.update();
        match &test_app.window_key {
            WindowKey::Primary => {
                test_app
                    .app
                    .world_mut()
                    .entity_mut(test_app.window)
                    .remove::<Window>();
            },
            WindowKey::Managed(_) => {
                assert!(test_app.app.world_mut().despawn(test_app.window));
            },
        }
        test_app.app.world_mut().flush();
        return_target(test_app, 2);
        test_app
            .app
            .world_mut()
            .resource_mut::<InjectedCurrentMonitorSource>()
            .reset_activity();
    }

    fn spawn_application_replacement(test_app: &mut RuntimeTestApp) -> Entity {
        let replacement = test_app.app.world_mut().spawn(Window::default()).id();
        match &test_app.window_key {
            WindowKey::Primary => {
                test_app
                    .app
                    .world_mut()
                    .entity_mut(replacement)
                    .insert(PrimaryWindow);
            },
            WindowKey::Managed(name) => {
                test_app
                    .app
                    .world_mut()
                    .entity_mut(replacement)
                    .insert(ManagedWindow { name: name.clone() });
            },
        }
        test_app.app.world_mut().flush();
        replacement
    }

    fn request_application_restore(test_app: &mut RuntimeTestApp, replacement: Entity) {
        test_app.app.world_mut().trigger(crate::RestoreWindow {
            entity: replacement,
        });
        test_app
            .app
            .world_mut()
            .entity_mut(replacement)
            .insert(OnMonitor(test_app.fallback_entity));
        test_app.app.world_mut().flush();
        test_app.app.update();
    }

    fn prepare_application_replacement(test_app: &mut RuntimeTestApp) -> Entity {
        prepare_application_replacement_target(test_app);
        let replacement = spawn_application_replacement(test_app);
        request_application_restore(test_app, replacement);
        replacement
    }

    #[test]
    fn application_replacement_uses_repaired_association_and_startup_target() {
        let mut test_app = runtime_test_app(WindowRecovery::ApplicationControlled);
        let expected = build_target_for_synthetic_runtime(
            &test_app.placement,
            test_app.app.world().resource::<Monitors>(),
        );

        let replacement = prepare_application_replacement(&mut test_app);

        assert_eq!(
            test_app
                .app
                .world()
                .get::<TargetPosition>(replacement)
                .map(TargetSnapshot::from),
            Some(TargetSnapshot::from(&expected)),
        );
        assert_eq!(
            test_app
                .app
                .world()
                .resource::<InjectedCurrentMonitorSource>()
                .activity(),
            NativeQueryActivity {
                window_map:       0,
                monitor_metadata: 0,
            },
        );
        assert_eq!(
            test_app
                .app
                .world()
                .resource::<CapturedWindowStates>()
                .live_entity(&test_app.window_key),
            Some(replacement),
        );
    }

    #[test]
    fn managed_application_replacement_enters_runtime_attempt_in_production_order() {
        let window_key = WindowKey::Managed("secondary".to_string());
        let mut test_app =
            runtime_test_app_for(WindowRecovery::ApplicationControlled, window_key.clone());
        prepare_application_replacement_target(&mut test_app);

        let replacement = spawn_application_replacement(&mut test_app);

        assert_eq!(
            test_app
                .app
                .world()
                .resource::<ManagedWindowRegistry>()
                .name(replacement),
            Some("secondary")
        );
        assert!(
            test_app
                .app
                .world()
                .get::<RestorePreparation>(replacement)
                .is_none()
        );
        assert_eq!(
            test_app
                .app
                .world()
                .resource::<CapturedWindowStates>()
                .live_entity(&window_key),
            None
        );

        request_application_restore(&mut test_app, replacement);

        let restore_attempt = test_app
            .app
            .world()
            .get::<RestorePreparation>(replacement)
            .and_then(RestorePreparation::recovery_attempt);
        assert!(restore_attempt.is_some());
        assert_eq!(
            restore_attempt.map(|attempt| (&attempt.window_key, attempt.entity)),
            Some((&window_key, replacement))
        );
        assert_eq!(
            test_app
                .app
                .world()
                .resource::<CapturedWindowStates>()
                .live_entity(&window_key),
            Some(replacement)
        );
        assert_eq!(
            test_app
                .app
                .world()
                .resource::<RecoveryRegistrations>()
                .by_key(&window_key)
                .and_then(|registration| registration.entity),
            Some(replacement)
        );
    }

    fn accepted_restore_attempt(
        test_app: &RuntimeTestApp,
        entity: Entity,
    ) -> Option<RestoreAttempt> {
        test_app
            .app
            .world()
            .get::<RestorePreparation>(entity)
            .and_then(RestorePreparation::recovery_attempt)
            .cloned()
    }

    fn restored_from_target(
        test_app: &RuntimeTestApp,
        entity: Entity,
        target: &TargetSnapshot,
    ) -> WindowRestored {
        WindowRestored {
            entity,
            window_key: test_app.window_key.clone(),
            physical_position: target.physical_position,
            logical_position: target.logical_position,
            physical_size: target.physical_size,
            logical_size: target.logical_size,
            window_mode: target
                .saved_window_mode
                .to_window_mode(target.monitor_index),
            monitor_index: target.monitor_index,
        }
    }

    fn runtime_is_restoring(test_app: &RuntimeTestApp, attempt: &RestoreAttempt) -> bool {
        test_app
            .app
            .world()
            .resource::<ApplicationControlledRecoveries>()
            .is_restoring(&test_app.window_key, attempt.generation)
    }

    fn stage_matching_settle(
        test_app: &mut RuntimeTestApp,
        entity: Entity,
        target: &TargetSnapshot,
    ) -> bool {
        {
            let mut entity_mut = test_app.app.world_mut().entity_mut(entity);
            let Some(mut window) = entity_mut.get_mut::<Window>() else {
                return false;
            };
            window.position = target
                .physical_position
                .map_or(WindowPosition::Automatic, WindowPosition::At);
            window
                .resolution
                .set_physical_resolution(target.physical_size.x, target.physical_size.y);
            window
                .resolution
                .set_scale_factor(target.target_scale.to_f32());
            window.mode = target
                .saved_window_mode
                .to_window_mode(target.monitor_index);
        }
        test_app
            .app
            .world_mut()
            .entity_mut(entity)
            .insert(OnMonitor(test_app.target_entity));
        test_app.app.world_mut().flush();
        let Some(mut target_position) = test_app.app.world_mut().get_mut::<TargetPosition>(entity)
        else {
            return false;
        };
        target_position.settle_state = Some(SettleState::new());
        true
    }

    fn restore_outcome_counts(test_app: &RuntimeTestApp) -> (usize, usize) {
        let outcomes = test_app.app.world().resource::<RestoreOutcomeCounts>();
        (outcomes.restored, outcomes.mismatched)
    }

    fn assert_finalized_runtime_success(
        test_app: &RuntimeTestApp,
        entity: Entity,
        restore_attempt: &RestoreAttempt,
    ) {
        assert!(
            test_app
                .app
                .world()
                .resource::<ApplicationControlledRecoveries>()
                .is_healthy(&test_app.window_key, restore_attempt.generation)
        );
        assert!(
            test_app
                .app
                .world()
                .get::<RestorePreparation>(entity)
                .is_none()
        );
        assert!(test_app.app.world().get::<TargetPosition>(entity).is_none());
        assert!(
            test_app
                .app
                .world()
                .get::<RestoreDiagnostics>(entity)
                .is_some()
        );
        assert!(
            test_app
                .app
                .world()
                .get::<X11FrameCompensated>(entity)
                .is_none()
        );
        let states = test_app.app.world().resource::<CapturedWindowStates>();
        let activity = states.activity();
        assert_eq!((activity.projections, activity.writes), (1, 1));
        assert_eq!(states.live_entity(&test_app.window_key), Some(entity));
    }

    fn stage_stale_application_sentinel(test_app: &mut RuntimeTestApp) -> Option<WindowSnapshot> {
        {
            let mut target_position = test_app
                .app
                .world_mut()
                .get_mut::<TargetPosition>(test_app.window)?;
            assert_ne!(
                (
                    target_position.physical_position,
                    target_position.physical_size
                ),
                (Some(STALE_SENTINEL_POSITION), STALE_SENTINEL_SIZE)
            );
            target_position.monitor_scale_strategy = MonitorScaleStrategy::ApplyUnchanged;
        }
        test_app.app.init_resource::<InjectedWinitWindows>();
        test_app
            .app
            .world_mut()
            .resource_mut::<InjectedWinitWindows>()
            .insert(test_app.window);
        let mut window = test_app
            .app
            .world_mut()
            .get_mut::<Window>(test_app.window)?;
        window.position = WindowPosition::At(STALE_SENTINEL_POSITION);
        window
            .resolution
            .set_physical_resolution(STALE_SENTINEL_SIZE.x, STALE_SENTINEL_SIZE.y);
        window.mode = WindowMode::Windowed;
        window.visible = true;
        Some(WindowSnapshot::from(&*window))
    }

    fn assert_stale_attempt_rejected(test_app: &RuntimeTestApp, window_snapshot: WindowSnapshot) {
        assert_eq!(
            test_app
                .app
                .world()
                .get::<Window>(test_app.window)
                .map(WindowSnapshot::from),
            Some(window_snapshot)
        );
        assert!(
            test_app
                .app
                .world()
                .get::<RestorePreparation>(test_app.window)
                .is_none()
        );
        assert!(
            test_app
                .app
                .world()
                .get::<TargetPosition>(test_app.window)
                .is_none()
        );
        assert!(
            test_app
                .app
                .world()
                .get::<X11FrameCompensated>(test_app.window)
                .is_none()
        );
        assert_eq!(restore_outcome_counts(test_app), (0, 0));
    }

    #[test]
    fn settling_validates_generation_then_publishes_and_projects_once() {
        let mut test_app = runtime_test_app(WindowRecovery::ApplicationControlled);
        let replacement = prepare_application_replacement(&mut test_app);
        let restore_attempt = accepted_restore_attempt(&test_app, replacement);
        let target = test_app
            .app
            .world()
            .get::<TargetPosition>(replacement)
            .map(TargetSnapshot::from);
        assert!(restore_attempt.is_some());
        assert!(target.is_some());
        if let (Some(restore_attempt), Some(target)) = (restore_attempt, target) {
            let mut stale_generation_attempt = restore_attempt.clone();
            stale_generation_attempt.generation = RecoveryGeneration::from_test_raw(u64::MAX);
            let stale_completion = restored_from_target(&test_app, replacement, &target);

            test_app
                .app
                .world_mut()
                .trigger(RuntimeRestoreCompletion::new(
                    stale_generation_attempt,
                    RuntimeRestoreOutcome::Restored(stale_completion),
                ));
            test_app.app.world_mut().flush();

            assert!(runtime_is_restoring(&test_app, &restore_attempt));
            assert_eq!(restore_outcome_counts(&test_app), (0, 0));
            assert!(
                test_app
                    .app
                    .world()
                    .get::<RestorePreparation>(replacement)
                    .is_some()
            );

            assert!(stage_matching_settle(&mut test_app, replacement, &target));
            test_app
                .app
                .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::ZERO));
            test_app.app.update();
            test_app
                .app
                .world_mut()
                .resource_mut::<CapturedWindowStates>()
                .reset_activity();
            assert_eq!(
                test_app
                    .app
                    .world()
                    .resource::<CapturedWindowStates>()
                    .activity()
                    .projections,
                0
            );
            assert!(runtime_is_restoring(&test_app, &restore_attempt));

            test_app
                .app
                .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f32(
                    crate::constants::SETTLE_STABILITY_SECS,
                )));
            test_app.app.update();
            assert_eq!(restore_outcome_counts(&test_app), (1, 0));
            test_app
                .app
                .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::ZERO));
            test_app.app.update();

            assert_eq!(restore_outcome_counts(&test_app), (1, 0));
            assert_finalized_runtime_success(&test_app, replacement, &restore_attempt);
        }
    }

    #[test]
    fn stale_topology_revision_cannot_reach_restore_application() {
        let mut test_app = runtime_test_app(WindowRecovery::FallbackAndReturn);
        settle_automatic_on_fallback(&mut test_app);
        return_target(&mut test_app, 2);
        assert!(
            test_app
                .app
                .world()
                .get::<TargetPosition>(test_app.window)
                .is_some()
        );
        let window_snapshot = stage_stale_application_sentinel(&mut test_app);
        assert!(window_snapshot.is_some());
        if let Some(window_snapshot) = window_snapshot {
            let fallback = (test_app.fallback_entity, test_app.fallback);
            let target = (test_app.target_entity, test_app.target);
            install_runtime_topology(&mut test_app, 3, [fallback, target]);

            test_app.app.update();

            assert_stale_attempt_rejected(&test_app, window_snapshot);
        }
    }

    #[test]
    fn replaced_expected_monitor_cannot_reach_restore_application() {
        let mut test_app = runtime_test_app(WindowRecovery::FallbackAndReturn);
        settle_automatic_on_fallback(&mut test_app);
        return_target(&mut test_app, 2);
        assert!(
            test_app
                .app
                .world()
                .get::<TargetPosition>(test_app.window)
                .is_some()
        );
        let window_snapshot = stage_stale_application_sentinel(&mut test_app);
        assert!(window_snapshot.is_some());
        if let Some(window_snapshot) = window_snapshot {
            let replacement_monitor = MonitorInfo {
                identity: MonitorIdentity::Verified(MonitorId::from_test_raw(8)),
                ..test_app.target
            };
            let fallback = (test_app.fallback_entity, test_app.fallback);
            let replacement = (test_app.target_entity, replacement_monitor);
            install_runtime_topology(&mut test_app, 2, [fallback, replacement]);

            test_app.app.update();

            assert_stale_attempt_rejected(&test_app, window_snapshot);
        }
    }

    #[test]
    fn restore_attempt_ids_exhaust_without_reuse() {
        let mut ids = RestoreAttemptIds {
            state: RestoreAttemptIdState::Next(u64::MAX),
        };

        assert_eq!(ids.allocate(), Some(RestoreAttemptId(u64::MAX)));
        assert_eq!(ids.allocate(), None);
    }

    struct ConcurrentScaleRestores {
        app:              App,
        recovery_entity:  Entity,
        startup_entity:   Entity,
        unrelated_entity: Entity,
        recovery_attempt: RestoreAttempt,
        other_attempt_id: RestoreAttemptId,
        starting_scale:   f64,
        target_scale:     f64,
    }

    #[derive(Clone, Copy)]
    enum ScaleMessagePath {
        HigherToLower,
        WindowsCompensateSizeOnly,
    }

    impl ScaleMessagePath {
        const fn platform(self) -> Platform {
            match self {
                Self::HigherToLower => Platform::MacOs,
                Self::WindowsCompensateSizeOnly => Platform::Windows,
            }
        }

        const fn waiting_strategy(
            self,
            attempt_id: Option<RestoreAttemptId>,
        ) -> MonitorScaleStrategy {
            let state = WindowRestoreState::WaitingForScaleChange { attempt_id };
            match self {
                Self::HigherToLower => MonitorScaleStrategy::HigherToLower(state),
                Self::WindowsCompensateSizeOnly => MonitorScaleStrategy::CompensateSizeOnly(state),
            }
        }

        fn is_waiting(
            self,
            strategy: MonitorScaleStrategy,
            expected_attempt: Option<RestoreAttemptId>,
        ) -> bool {
            match (self, strategy) {
                (
                    Self::HigherToLower,
                    MonitorScaleStrategy::HigherToLower(
                        WindowRestoreState::WaitingForScaleChange { attempt_id },
                    ),
                )
                | (
                    Self::WindowsCompensateSizeOnly,
                    MonitorScaleStrategy::CompensateSizeOnly(
                        WindowRestoreState::WaitingForScaleChange { attempt_id },
                    ),
                ) => attempt_id == expected_attempt,
                _ => false,
            }
        }
    }

    fn waiting_scale_target(
        scale_message_path: ScaleMessagePath,
        attempt_id: Option<RestoreAttemptId>,
    ) -> TargetPosition {
        let mut target = build_target_for_synthetic_runtime(
            &captured_placement(
                MonitorIdentity::Verified(TARGET_ID),
                2,
                CapturedWindowPosition::Restorable {
                    logical_offset: CAPTURED_OFFSET,
                },
                SavedWindowMode::Windowed,
            ),
            &monitors_with_returned_target(),
        );
        target.monitor_scale_strategy = scale_message_path.waiting_strategy(attempt_id);
        target
    }

    fn concurrent_scale_restores(scale_message_path: ScaleMessagePath) -> ConcurrentScaleRestores {
        let mut app = App::new();
        app.insert_resource(scale_message_path.platform())
            .init_resource::<InjectedWinitWindows>()
            .add_message::<WindowScaleFactorChanged>()
            .add_systems(Update, restore_windows);
        let recovery_entity = app.world_mut().spawn_empty().id();
        let startup_entity = app.world_mut().spawn_empty().id();
        let unrelated_entity = app.world_mut().spawn_empty().id();
        let recovery_attempt = RestoreAttempt {
            id:                RestoreAttemptId::from_test_raw(21),
            window_key:        WindowKey::Primary,
            entity:            recovery_entity,
            generation:        RecoveryGeneration::from_test_raw(34),
            expected_monitor:  TARGET_ID,
            topology_revision: MonitorTopologyRevision::default(),
            deadline:          Duration::from_secs(2),
        };
        let other_attempt_id = RestoreAttemptId::from_test_raw(22);
        let recovery_target = waiting_scale_target(scale_message_path, Some(recovery_attempt.id));
        let startup_target = waiting_scale_target(scale_message_path, None);
        let starting_scale = recovery_target.starting_scale;
        let target_scale = recovery_target.target_scale;
        assert!((starting_scale - target_scale).abs() > SCALE_FACTOR_EPSILON);
        assert!((startup_target.starting_scale - starting_scale).abs() <= SCALE_FACTOR_EPSILON);
        let mut recovery_window = Window::default();
        recovery_window
            .resolution
            .set_scale_factor(target_scale.to_f32());
        let mut startup_window = Window::default();
        startup_window
            .resolution
            .set_scale_factor(target_scale.to_f32());
        app.world_mut().entity_mut(recovery_entity).insert((
            recovery_window,
            RestorePreparation::recovery(recovery_attempt.clone()),
            recovery_target,
            X11FrameCompensated,
        ));
        app.world_mut().entity_mut(startup_entity).insert((
            startup_window,
            RestorePreparation::startup(WindowKey::Managed("startup".to_string())),
            startup_target,
            X11FrameCompensated,
        ));
        {
            let mut injected_windows = app.world_mut().resource_mut::<InjectedWinitWindows>();
            injected_windows.insert(recovery_entity);
            injected_windows.insert(startup_entity);
        }
        ConcurrentScaleRestores {
            app,
            recovery_entity,
            startup_entity,
            unrelated_entity,
            recovery_attempt,
            other_attempt_id,
            starting_scale,
            target_scale,
        }
    }

    fn send_scale_change(app: &mut App, entity: Entity, scale_factor: f64) {
        app.world_mut().write_message(WindowScaleFactorChanged {
            window: entity,
            scale_factor,
        });
        app.update();
    }

    fn waiting_for_attempt(
        app: &App,
        entity: Entity,
        scale_message_path: ScaleMessagePath,
        expected_attempt: Option<RestoreAttemptId>,
        expected_starting_scale: f64,
    ) -> bool {
        app.world()
            .get::<TargetPosition>(entity)
            .is_some_and(|target| {
                (target.starting_scale - expected_starting_scale).abs() <= SCALE_FACTOR_EPSILON
                    && scale_message_path
                        .is_waiting(target.monitor_scale_strategy, expected_attempt)
            })
    }

    fn set_waiting_attempt(
        app: &mut App,
        entity: Entity,
        scale_message_path: ScaleMessagePath,
        attempt_id: Option<RestoreAttemptId>,
    ) {
        let target = app.world_mut().get_mut::<TargetPosition>(entity);
        assert!(target.is_some());
        if let Some(mut target) = target {
            target.monitor_scale_strategy = scale_message_path.waiting_strategy(attempt_id);
        }
    }

    fn set_live_scale(app: &mut App, entity: Entity, scale: f64) {
        let window = app.world_mut().get_mut::<Window>(entity);
        assert!(window.is_some());
        if let Some(mut window) = window {
            window.resolution.set_scale_factor(scale.to_f32());
        }
    }

    fn settle_started(app: &App, entity: Entity) -> bool {
        app.world()
            .get::<TargetPosition>(entity)
            .is_some_and(|target| target.settle_state.is_some())
    }

    #[test]
    fn scale_message_advances_only_matching_concurrent_restore() {
        for scale_message_path in [
            ScaleMessagePath::HigherToLower,
            ScaleMessagePath::WindowsCompensateSizeOnly,
        ] {
            let ConcurrentScaleRestores {
                mut app,
                recovery_entity,
                startup_entity,
                unrelated_entity,
                recovery_attempt,
                other_attempt_id,
                starting_scale,
                target_scale,
            } = concurrent_scale_restores(scale_message_path);

            app.update();
            assert!(waiting_for_attempt(
                &app,
                recovery_entity,
                scale_message_path,
                Some(recovery_attempt.id),
                starting_scale
            ));
            assert!(waiting_for_attempt(
                &app,
                startup_entity,
                scale_message_path,
                None,
                starting_scale
            ));

            set_waiting_attempt(
                &mut app,
                recovery_entity,
                scale_message_path,
                Some(other_attempt_id),
            );
            send_scale_change(&mut app, recovery_entity, target_scale);
            assert!(waiting_for_attempt(
                &app,
                recovery_entity,
                scale_message_path,
                Some(other_attempt_id),
                starting_scale
            ));

            set_waiting_attempt(
                &mut app,
                recovery_entity,
                scale_message_path,
                Some(recovery_attempt.id),
            );
            send_scale_change(&mut app, recovery_entity, target_scale + WRONG_SCALE_DELTA);
            assert!(waiting_for_attempt(
                &app,
                recovery_entity,
                scale_message_path,
                Some(recovery_attempt.id),
                starting_scale
            ));

            set_live_scale(&mut app, recovery_entity, target_scale - WRONG_SCALE_DELTA);
            send_scale_change(&mut app, recovery_entity, target_scale);
            assert!(waiting_for_attempt(
                &app,
                recovery_entity,
                scale_message_path,
                Some(recovery_attempt.id),
                starting_scale
            ));

            set_live_scale(&mut app, recovery_entity, target_scale);
            send_scale_change(&mut app, unrelated_entity, target_scale);
            assert!(waiting_for_attempt(
                &app,
                recovery_entity,
                scale_message_path,
                Some(recovery_attempt.id),
                starting_scale
            ));
            assert!(waiting_for_attempt(
                &app,
                startup_entity,
                scale_message_path,
                None,
                starting_scale
            ));

            send_scale_change(&mut app, recovery_entity, target_scale);

            assert!(settle_started(&app, recovery_entity));
            assert!(waiting_for_attempt(
                &app,
                startup_entity,
                scale_message_path,
                None,
                starting_scale
            ));
            assert!(!settle_started(&app, startup_entity));

            send_scale_change(&mut app, startup_entity, target_scale);

            assert!(settle_started(&app, startup_entity));
        }
    }

    #[test]
    fn preparation_waits_for_native_window_readiness() {
        let captured_window_placement = captured_placement(
            MonitorIdentity::Verified(TARGET_ID),
            1,
            CapturedWindowPosition::Restorable {
                logical_offset: CAPTURED_OFFSET,
            },
            SavedWindowMode::Windowed,
        );
        let (mut app, entity, readiness_monitor_entity) =
            captured_startup_app(captured_window_placement, StartupReadiness::Waiting);

        app.update();
        app.update();

        assert!(app.world().get::<TargetPosition>(entity).is_none());
        assert!(app.world().get::<NativeWindowReady>(entity).is_none());
        assert_eq!(
            app.world()
                .resource::<InjectedCurrentMonitorSource>()
                .activity(),
            NativeQueryActivity {
                window_map:       0,
                monitor_metadata: 0,
            }
        );

        app.world_mut()
            .entity_mut(entity)
            .insert(OnMonitor(readiness_monitor_entity));
        app.world_mut().flush();

        assert!(app.world().get::<NativeWindowReady>(entity).is_some());
        assert!(app.world().get::<CurrentMonitor>(entity).is_some());
        app.update();

        assert!(app.world().get::<TargetPosition>(entity).is_some());
        assert_eq!(
            app.world()
                .resource::<InjectedCurrentMonitorSource>()
                .activity(),
            NativeQueryActivity {
                window_map:       0,
                monitor_metadata: 0,
            }
        );
    }

    fn assert_cancelled_runtime_attempt(
        test_app: &RuntimeTestApp,
        entity: Entity,
        restore_attempt: &RestoreAttempt,
    ) {
        assert!(test_app.app.world().get_entity(entity).is_ok());
        assert_eq!(
            test_app
                .app
                .world()
                .get::<Window>(entity)
                .map(|window| window.visible),
            Some(true)
        );
        assert!(
            test_app
                .app
                .world()
                .get::<RestorePreparation>(entity)
                .is_none()
        );
        assert!(test_app.app.world().get::<TargetPosition>(entity).is_none());
        assert!(
            test_app
                .app
                .world()
                .get::<X11FrameCompensated>(entity)
                .is_none()
        );
        assert!(
            test_app
                .app
                .world()
                .get::<RestoreDiagnostics>(entity)
                .is_some()
        );
        assert!(!runtime_is_restoring(test_app, restore_attempt));
        assert!(
            test_app
                .app
                .world()
                .resource::<RecoveryRegistrations>()
                .by_key(&test_app.window_key)
                .is_none()
        );
    }

    #[test]
    fn recovery_cancellation_finishes_a_hidden_live_window_and_removes_the_restore_attempt() {
        let mut test_app = runtime_test_app(WindowRecovery::ApplicationControlled);
        let replacement = prepare_application_replacement(&mut test_app);
        let restore_attempt = accepted_restore_attempt(&test_app, replacement);
        assert!(restore_attempt.is_some());
        if let Some(restore_attempt) = restore_attempt {
            let target_position = test_app
                .app
                .world_mut()
                .get_mut::<TargetPosition>(replacement);
            assert!(target_position.is_some());
            if let Some(mut target_position) = target_position {
                target_position.settle_state = Some(SettleState::new());
            }
            let window = test_app.app.world_mut().get_mut::<Window>(replacement);
            assert!(window.is_some());
            if let Some(mut window) = window {
                window.visible = false;
            }
            assert!(runtime_is_restoring(&test_app, &restore_attempt));
            assert!(
                test_app
                    .app
                    .world()
                    .get::<X11FrameCompensated>(replacement)
                    .is_some()
            );

            test_app.app.world_mut().trigger(CancelWindowRecovery {
                window: test_app.window_key.clone(),
            });
            test_app.app.world_mut().flush();

            assert_cancelled_runtime_attempt(&test_app, replacement, &restore_attempt);

            test_app
                .app
                .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f32(
                    crate::constants::SETTLE_TIMEOUT_SECS * 2.0,
                )));
            test_app.app.update();
            test_app.app.update();

            assert_cancelled_runtime_attempt(&test_app, replacement, &restore_attempt);
            assert_eq!(restore_outcome_counts(&test_app), (0, 0));
        }
    }

    #[test]
    fn rejected_monitor_association_clears_stale_readiness() {
        let captured_window_placement = captured_placement(
            MonitorIdentity::Verified(TARGET_ID),
            1,
            CapturedWindowPosition::Restorable {
                logical_offset: CAPTURED_OFFSET,
            },
            SavedWindowMode::Windowed,
        );
        let (mut app, entity, _) =
            captured_startup_app(captured_window_placement, StartupReadiness::Associated);
        assert!(app.world().get::<NativeWindowReady>(entity).is_some());
        assert!(app.world().get::<CurrentMonitor>(entity).is_some());

        let unresolved_monitor_entity = app.world_mut().spawn_empty().id();
        app.world_mut()
            .entity_mut(entity)
            .insert(OnMonitor(unresolved_monitor_entity));
        app.world_mut().flush();

        assert!(app.world().get::<NativeWindowReady>(entity).is_none());
        assert!(app.world().get::<CurrentMonitor>(entity).is_none());
        app.update();
        assert!(app.world().get::<TargetPosition>(entity).is_none());
        assert_eq!(
            app.world()
                .resource::<InjectedCurrentMonitorSource>()
                .activity(),
            NativeQueryActivity {
                window_map:       0,
                monitor_metadata: 0,
            }
        );
    }

    #[test]
    fn removed_monitor_association_clears_stale_readiness() {
        let captured_window_placement = captured_placement(
            MonitorIdentity::Verified(TARGET_ID),
            1,
            CapturedWindowPosition::Restorable {
                logical_offset: CAPTURED_OFFSET,
            },
            SavedWindowMode::Windowed,
        );
        let (mut app, entity, _) =
            captured_startup_app(captured_window_placement, StartupReadiness::Associated);
        assert!(app.world().get::<NativeWindowReady>(entity).is_some());
        assert!(app.world().get::<CurrentMonitor>(entity).is_some());

        app.world_mut().entity_mut(entity).remove::<OnMonitor>();
        app.world_mut().flush();

        assert!(app.world().get::<NativeWindowReady>(entity).is_none());
        assert!(app.world().get::<CurrentMonitor>(entity).is_none());
    }

    #[test]
    fn compositor_controlled_capture_never_prepares_a_coordinate() {
        let captured_window_placement = captured_placement(
            MonitorIdentity::Verified(TARGET_ID),
            1,
            CapturedWindowPosition::CompositorControlled,
            SavedWindowMode::Windowed,
        );
        let target_position = build_target_for_synthetic_runtime(
            &captured_window_placement,
            &monitors_with_returned_target(),
        );

        assert_eq!(target_position.monitor_index, 2);
        assert_eq!(target_position.physical_position, None);
        assert_eq!(target_position.logical_position, None);
    }

    #[test]
    fn x11_fullscreen_prestartup_flushes_target_before_monitor_move() {
        let monitors = monitors_with_returned_target();
        let starting_monitor = *monitors.first();
        let captured_window_placement = captured_placement(
            MonitorIdentity::Verified(TARGET_ID),
            1,
            CapturedWindowPosition::Restorable {
                logical_offset: CAPTURED_OFFSET,
            },
            SavedWindowMode::BorderlessFullscreen,
        );
        let mut app = App::new();
        app.insert_resource(monitors)
            .insert_resource(WinitInfo::default())
            .insert_resource(Platform::X11)
            .init_resource::<CapturedWindowStates>()
            .add_plugins(RestorePlugin);
        let entity = app
            .world_mut()
            .spawn((
                Window::default(),
                PrimaryWindow,
                CurrentMonitor {
                    monitor_info:          starting_monitor,
                    effective_window_mode: WindowMode::Windowed,
                },
                NativeWindowReady,
            ))
            .id();
        app.world_mut()
            .resource_mut::<CapturedWindowStates>()
            .promote(WindowKey::Primary, entity, captured_window_placement);

        app.world_mut().run_schedule(PreStartup);

        let target_position = app.world().get::<TargetPosition>(entity);
        assert_eq!(
            target_position.and_then(|target_position| target_position.physical_position),
            Some(IVec2::new(2_200, -100))
        );
        assert_eq!(
            app.world()
                .get::<Window>(entity)
                .map(|window| window.position),
            Some(WindowPosition::At(IVec2::new(2_200, -100)))
        );
        assert!(app.world().get::<X11FrameCompensated>(entity).is_some());
    }

    #[test]
    fn persisted_adapter_fallback_is_coordinate_free() {
        let persisted_window_state = PersistedWindowState {
            logical_position:  Some((1_000, 200)),
            logical_width:     800,
            logical_height:    600,
            scale:             1.0,
            monitor:           4,
            saved_window_mode: SavedWindowMode::Windowed,
            app_name:          "test".to_string(),
        };
        let monitors = Monitors::from_test_monitors([(
            Entity::from_bits(1),
            monitor(MonitorIdentity::Unverified, 0, 2.0, IVec2::ZERO),
        )]);
        let source = CapturedPlacement::PersistedOnly(persisted_window_state);
        let prepared_restore = RestoreTargetBuilder {
            source:              &source,
            monitors:            &monitors,
            recovery_monitor:    None,
            physical_decoration: UVec2::ZERO,
            starting_scale:      1.0,
            platform:            Platform::Windows,
        }
        .build();

        assert_eq!(
            prepared_restore.monitor_resolution_source,
            MonitorResolutionSource::FallbackToPrimary
        );
        assert_eq!(prepared_restore.target_position.monitor_index, 0);
        assert_eq!(prepared_restore.target_position.physical_position, None);
    }

    #[test]
    fn primary_preparation_reads_seeded_state_without_another_file_read() {
        let mut captured_window_states = CapturedWindowStates::default();
        captured_window_states.seed(HashMap::from([(
            WindowKey::Primary,
            PersistedWindowState {
                logical_position:  Some((10, 20)),
                logical_width:     800,
                logical_height:    600,
                scale:             1.0,
                monitor:           0,
                saved_window_mode: SavedWindowMode::Windowed,
                app_name:          "test".to_string(),
            },
        )]));

        assert!(matches!(
            captured_window_states.placement(&WindowKey::Primary),
            Some(CapturedPlacement::PersistedOnly(_))
        ));
        assert_eq!(captured_window_states.activity().file_reads, 0);
    }
}
