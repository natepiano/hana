use bevy::diagnostic::FrameCount;
use bevy::prelude::*;
use bevy::window::OnMonitor;
use bevy::window::PrimaryWindow;
use bevy_clerestory::CancelWindowRecovery;
use bevy_clerestory::CurrentMonitor;
use bevy_clerestory::ManagedWindow;
use bevy_clerestory::MonitorIdentity;
use bevy_clerestory::MonitorInfo;
use bevy_clerestory::Platform;
use bevy_clerestory::RestoreWindow;
use bevy_clerestory::WindowKey;
use bevy_clerestory::WindowRecovery;
use bevy_clerestory::WindowRecoveryAvailable;
use bevy_clerestory::WindowRecoveryPending;
use bevy_clerestory::WindowRestoreMismatch;
use bevy_clerestory::WindowRestored;

use super::ProbeMonitorIndex;
use super::constants::*;
use super::setup;
use super::setup::AcceptedWindowKeys;
use super::setup::ControlPlacementConfirmed;
use super::setup::ControlPlacementState;
use super::trace::ProbeTrace;

#[derive(Default, Resource)]
pub(super) struct ApplicationRecoveryCycles {
    pending_count: usize,
}

#[derive(Component)]
pub(super) struct PendingApplicationRestore {
    window_key: WindowKey,
    monitor:    MonitorInfo,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum ReadinessState {
    #[default]
    Waiting,
    Recorded,
}

#[derive(Default, Resource)]
pub(super) struct PreUnplugReadiness {
    readiness_state: ReadinessState,
}

#[derive(Clone, Copy)]
struct RecoveryAssociation {
    acceptance_count:  usize,
    current_monitor:   CurrentMonitor,
    entity:            Entity,
    installed_monitor: MonitorInfo,
    on_monitor:        Entity,
    reason:            Option<&'static str>,
    window_recovery:   WindowRecovery,
}

fn field(name: &str, value: impl std::fmt::Debug) -> (String, String) {
    (name.into(), format!("{value:?}"))
}

fn trace_record_count(trace: &ProbeTrace, kind: &str, fields: &[(&str, &str)]) -> usize {
    trace
        .records()
        .iter()
        .filter(|record| {
            record.kind == kind
                && fields.iter().all(|(expected_name, expected_value)| {
                    record
                        .fields
                        .iter()
                        .any(|(name, value)| name == expected_name && value == expected_value)
                })
        })
        .count()
}

fn expected_window_keys() -> [WindowKey; 3] {
    [
        WindowKey::Primary,
        WindowKey::Managed(AUTOMATIC_WINDOW_KEY.into()),
        WindowKey::Managed(APPLICATION_WINDOW_KEY.into()),
    ]
}

const fn recovery_unarmed_reason(
    platform: Platform,
    identity: MonitorIdentity,
    window_recovery: WindowRecovery,
) -> Option<&'static str> {
    match (identity, platform.is_wayland(), window_recovery) {
        (MonitorIdentity::Unverified, _, _) => Some(VALUE_UNARMED_UNVERIFIED),
        (MonitorIdentity::Verified(_), true, WindowRecovery::FallbackAndReturn) => {
            Some(VALUE_UNARMED_WAYLAND_WINDOWED)
        },
        (MonitorIdentity::Verified(_), _, _) => None,
    }
}

fn recovery_association(
    window_key: &WindowKey,
    monitor_index: usize,
    platform: Platform,
    accepted_window_keys: &AcceptedWindowKeys,
    monitors: &setup::ProbeMonitors,
    trace: &ProbeTrace,
    windows: &Query<(
        Entity,
        &OnMonitor,
        &CurrentMonitor,
        Option<&WindowRecovery>,
        Option<&PrimaryWindow>,
        Option<&ManagedWindow>,
    )>,
) -> Option<RecoveryAssociation> {
    if !accepted_window_keys.0.contains(window_key) {
        return None;
    }
    let window_recovery = setup::recovery_policy(window_key)?;
    let (entity, on_monitor, current_monitor, installed_recovery, _, _) =
        windows
            .iter()
            .find(|(_, _, _, _, primary_window, managed_window)| {
                setup::canonical_window_key(*primary_window, *managed_window).as_ref()
                    == Some(window_key)
            })?;
    if installed_recovery.copied() != Some(window_recovery) {
        return None;
    }
    let installed = monitors.by_entity(on_monitor.0)?;
    if installed.index != monitor_index || current_monitor.monitor_info != installed {
        return None;
    }
    let window_key_value = format!("{window_key:?}");
    let monitor_value = format!("{installed:?}");
    let acceptance_count = trace_record_count(
        trace,
        KIND_RECOVERY_ACCEPTED,
        &[
            (FIELD_WINDOW_KEY, &window_key_value),
            (FIELD_MONITOR, &monitor_value),
        ],
    );
    let reason = recovery_unarmed_reason(platform, installed.identity, window_recovery);
    match reason {
        Some(_) if acceptance_count != 0 => return None,
        None if acceptance_count != 1 => return None,
        Some(_) | None => {},
    }
    Some(RecoveryAssociation {
        acceptance_count,
        current_monitor: *current_monitor,
        entity,
        installed_monitor: installed,
        on_monitor: on_monitor.0,
        reason,
        window_recovery,
    })
}

fn control_association_state(
    monitor_index: usize,
    monitors: &setup::ProbeMonitors,
    controls: &Query<&OnMonitor, With<ControlPlacementConfirmed>>,
) -> ControlPlacementState {
    let mut associations = controls.iter().filter(|on_monitor| {
        monitors
            .by_entity(on_monitor.0)
            .is_some_and(|monitor| monitor.index == monitor_index)
    });
    if associations.next().is_some() && associations.next().is_none() {
        ControlPlacementState::Confirmed
    } else {
        ControlPlacementState::AwaitingConfirmation
    }
}

fn ready_recovery_associations(
    associations: Option<Vec<(WindowKey, RecoveryAssociation)>>,
    control_placement_state: ControlPlacementState,
) -> Option<Vec<(WindowKey, RecoveryAssociation)>> {
    match (associations, control_placement_state) {
        (Some(associations), ControlPlacementState::Confirmed) => Some(associations),
        (None, _) | (Some(_), ControlPlacementState::AwaitingConfirmation) => None,
    }
}

pub(super) fn record_recovery_readiness(
    monitor_index: Res<ProbeMonitorIndex>,
    platform: Res<Platform>,
    accepted_window_keys: Res<AcceptedWindowKeys>,
    monitors: setup::ProbeMonitors,
    windows: Query<(
        Entity,
        &OnMonitor,
        &CurrentMonitor,
        Option<&WindowRecovery>,
        Option<&PrimaryWindow>,
        Option<&ManagedWindow>,
    )>,
    controls: Query<&OnMonitor, With<ControlPlacementConfirmed>>,
    mut readiness: ResMut<PreUnplugReadiness>,
    trace: Res<ProbeTrace>,
    frame_count: Res<FrameCount>,
) {
    if readiness.readiness_state == ReadinessState::Recorded
        || trace_record_count(&trace, KIND_RECOVERY_PENDING, &[]) != 0
    {
        return;
    }
    let expected_window_keys = expected_window_keys();
    let associations = expected_window_keys
        .iter()
        .map(|window_key| {
            recovery_association(
                window_key,
                monitor_index.0,
                *platform,
                &accepted_window_keys,
                &monitors,
                &trace,
                &windows,
            )
            .map(|association| (window_key.clone(), association))
        })
        .collect::<Option<Vec<_>>>();
    let Some(associations) = ready_recovery_associations(
        associations,
        control_association_state(monitor_index.0, &monitors, &controls),
    ) else {
        return;
    };

    for (window_key, association) in &associations {
        trace.record(
            frame_count.0,
            PRODUCER_RECOVERY_READY,
            KIND_PRE_UNPLUG_ASSOCIATION,
            vec![
                field(FIELD_WINDOW_KEY, window_key),
                field(FIELD_WINDOW, association.entity),
                field(FIELD_MONITOR_ENTITY, association.on_monitor),
                field(FIELD_CURRENT_MONITOR, association.current_monitor),
                field(FIELD_MONITOR, association.installed_monitor),
                field(FIELD_RECOVERY_POLICY, association.window_recovery),
                field(FIELD_ACCEPTED_RECORDS, association.acceptance_count),
                field(
                    FIELD_ARMING_STATE,
                    association.reason.map_or(VALUE_ARMED, |_| VALUE_UNARMED),
                ),
                field(FIELD_RECOVERY_REASON, association.reason),
            ],
        );
        if association.reason.is_some() {
            trace.record(
                frame_count.0,
                PRODUCER_RECOVERY_READY,
                KIND_RECOVERY_UNARMED,
                vec![
                    field(FIELD_WINDOW_KEY, window_key),
                    field(FIELD_RECOVERY_POLICY, association.window_recovery),
                    field(FIELD_RECOVERY_REASON, association.reason),
                ],
            );
        }
    }
    trace.record(
        frame_count.0,
        PRODUCER_RECOVERY_READY,
        KIND_RECOVERY_READY,
        vec![
            field(FIELD_SELECTED_MONITOR_INDEX, monitor_index.0),
            field(FIELD_WINDOW_KEY, expected_window_keys),
        ],
    );
    readiness.readiness_state = ReadinessState::Recorded;
}

fn handle_application_recovery_pending(
    window_key: &WindowKey,
    recovery_cycles: &mut ApplicationRecoveryCycles,
    windows: &Query<(Entity, &ManagedWindow)>,
    commands: &mut Commands,
    trace: &ProbeTrace,
    frame_count: u32,
) {
    if window_key != &WindowKey::Managed(APPLICATION_WINDOW_KEY.into()) {
        return;
    }
    recovery_cycles.pending_count += 1;
    for (entity, managed_window) in windows.iter() {
        if managed_window.name == APPLICATION_WINDOW_KEY {
            commands.entity(entity).despawn();
        }
    }
    if recovery_cycles.pending_count == SECOND_RECOVERY_CYCLE {
        commands.trigger(CancelWindowRecovery {
            window: window_key.clone(),
        });
        trace.record(
            frame_count,
            PRODUCER_APPLICATION_RECOVERY_CANCELLATION_REQUESTED,
            KIND_RECOVERY_CANCELLATION_REQUESTED,
            vec![
                field(FIELD_WINDOW_KEY, window_key),
                field(FIELD_RECOVERY_CYCLE, recovery_cycles.pending_count),
            ],
        );
    }
}

pub(super) fn on_window_recovery_pending(
    event: On<WindowRecoveryPending>,
    mut recovery_cycles: ResMut<ApplicationRecoveryCycles>,
    windows: Query<(Entity, &ManagedWindow)>,
    mut commands: Commands,
    trace: Res<ProbeTrace>,
    frame_count: Res<FrameCount>,
) {
    trace.record(
        frame_count.0,
        PRODUCER_RECOVERY_PENDING,
        KIND_RECOVERY_PENDING,
        vec![
            field(FIELD_WINDOW_KEY, &event.window_key),
            field(FIELD_MONITOR, event.monitor_id),
        ],
    );
    handle_application_recovery_pending(
        &event.window_key,
        &mut recovery_cycles,
        &windows,
        &mut commands,
        &trace,
        frame_count.0,
    );
}

pub(super) fn prepare_application_window_restore(
    event: On<WindowRecoveryAvailable>,
    windows: Query<(Entity, &ManagedWindow)>,
    mut commands: Commands,
) {
    if event.window_key != WindowKey::Managed(APPLICATION_WINDOW_KEY.into()) {
        return;
    }
    let entity = windows
        .iter()
        .find(|(_, managed_window)| managed_window.name == APPLICATION_WINDOW_KEY)
        .map_or_else(
            || {
                commands
                    .spawn((
                        setup::probe_window(APPLICATION_WINDOW_TITLE, WindowPosition::Automatic),
                        ManagedWindow {
                            name: APPLICATION_WINDOW_KEY.into(),
                        },
                    ))
                    .id()
            },
            |(entity, _)| entity,
        );
    commands.entity(entity).insert(PendingApplicationRestore {
        window_key: event.window_key.clone(),
        monitor:    event.monitor,
    });
}

pub(super) fn request_application_window_restore(
    pending: Query<(Entity, &PendingApplicationRestore), With<setup::ProbeContentAttached>>,
    mut commands: Commands,
    trace: Res<ProbeTrace>,
    frame_count: Res<FrameCount>,
) {
    for (entity, pending_restore) in &pending {
        commands
            .entity(entity)
            .remove::<PendingApplicationRestore>();
        commands.trigger(RestoreWindow { entity });
        trace.record(
            frame_count.0,
            PRODUCER_RECOVERY_RESTORE_REQUESTED,
            KIND_RECOVERY_RESTORE_REQUESTED,
            vec![
                field(FIELD_WINDOW_KEY, &pending_restore.window_key),
                field(FIELD_WINDOW, entity),
                field(FIELD_MONITOR, pending_restore.monitor),
            ],
        );
    }
}

pub(super) fn on_window_recovery_available(
    event: On<WindowRecoveryAvailable>,
    trace: Res<ProbeTrace>,
    frame_count_resource: Res<FrameCount>,
) {
    trace.record(
        frame_count_resource.0,
        PRODUCER_RECOVERY_AVAILABLE,
        KIND_RECOVERY_AVAILABLE,
        vec![
            field(FIELD_WINDOW_KEY, &event.window_key),
            field(FIELD_MONITOR, event.monitor),
        ],
    );
}

pub(super) fn on_window_restored(
    event: On<WindowRestored>,
    trace: Res<ProbeTrace>,
    frame_count_resource: Res<FrameCount>,
) {
    trace.record(
        frame_count_resource.0,
        PRODUCER_RECOVERY_RESTORED,
        KIND_RECOVERY_RESTORED,
        vec![
            field(FIELD_WINDOW_KEY, &event.window_key),
            field(FIELD_WINDOW, event.entity),
            field(FIELD_WINDOW_POSITION, event.physical_position),
            field(FIELD_WINDOW_SIZE, event.physical_size),
            field(FIELD_WINDOW_MODE, event.window_mode),
            field(FIELD_MONITOR, event.monitor_index),
        ],
    );
}

pub(super) fn on_window_restore_mismatch(
    event: On<WindowRestoreMismatch>,
    trace: Res<ProbeTrace>,
    frame_count_resource: Res<FrameCount>,
) {
    trace.record(
        frame_count_resource.0,
        PRODUCER_RECOVERY_MISMATCH,
        KIND_RECOVERY_MISMATCH,
        vec![
            field(FIELD_WINDOW_KEY, &event.window_key),
            field(FIELD_WINDOW, event.entity),
            field(
                FIELD_EXPECTED,
                (
                    event.expected_physical_position,
                    event.expected_physical_size,
                    event.expected_window_mode,
                    event.expected_monitor,
                    event.expected_scale,
                ),
            ),
            field(
                FIELD_ACTUAL,
                (
                    event.actual_physical_position,
                    event.actual_physical_size,
                    event.actual_window_mode,
                    event.actual_monitor,
                    event.actual_scale,
                ),
            ),
        ],
    );
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    reason = "tests should panic on unexpected values"
)]
mod tests {
    use bevy::reflect::tuple_struct::DynamicTupleStruct;
    use bevy::window::WindowMode;
    use bevy_clerestory::MonitorId;

    use super::*;

    const ACCEPTED_RECORDS_PER_KEY: usize = 1;
    const RECOVERY_MONITOR_ID_RAW: u64 = 7;
    const RETURNED_MONITOR_INDEX: usize = 2;
    const RETURNED_MONITOR_SIZE: UVec2 = UVec2::new(1_920, 1_080);
    const RETURNED_MONITOR_SCALE: f64 = 1.0;

    #[derive(Default, Resource)]
    struct ConsumerRequests {
        cancellations: Vec<WindowKey>,
        restores:      Vec<Entity>,
    }

    fn returned_monitor() -> MonitorInfo {
        MonitorInfo {
            identity:          MonitorIdentity::Unverified,
            index:             RETURNED_MONITOR_INDEX,
            scale:             RETURNED_MONITOR_SCALE,
            physical_position: IVec2::ZERO,
            physical_size:     RETURNED_MONITOR_SIZE,
        }
    }

    fn recovery_monitor_id() -> MonitorId {
        let mut reflected_monitor_id = DynamicTupleStruct::default();
        reflected_monitor_id.insert(RECOVERY_MONITOR_ID_RAW);
        MonitorId::from_reflect(&reflected_monitor_id)
            .expect("reflected monitor identifier should be constructible")
    }

    fn record_recovery_acceptance(
        trace: &ProbeTrace,
        window_key: &WindowKey,
        window: Entity,
        monitor_entity: Entity,
        monitor: MonitorInfo,
        recovery_policy: WindowRecovery,
    ) {
        trace.record(
            0,
            RECOVERY_PROBE_TARGET,
            KIND_RECOVERY_ACCEPTED,
            vec![
                field(FIELD_WINDOW_KEY, window_key),
                field(FIELD_WINDOW, window),
                field(FIELD_MONITOR_ENTITY, monitor_entity),
                field(FIELD_MONITOR, monitor),
                field(FIELD_RECOVERY_POLICY, recovery_policy),
            ],
        );
    }

    struct ReadinessFixtures {
        accepted_windows: [(WindowKey, Entity, WindowRecovery); 3],
        control:          Entity,
    }

    fn spawn_readiness_fixtures(
        app: &mut App,
        monitors: setup::tests::ProbeTestMonitors,
        current_monitor: CurrentMonitor,
    ) -> ReadinessFixtures {
        let primary = app
            .world_mut()
            .spawn((
                Window::default(),
                PrimaryWindow,
                OnMonitor(monitors.selected_entity),
                current_monitor,
                WindowRecovery::FallbackAndReturn,
            ))
            .id();
        let automatic = app
            .world_mut()
            .spawn((
                Window::default(),
                ManagedWindow {
                    name: AUTOMATIC_WINDOW_KEY.into(),
                },
                OnMonitor(monitors.selected_entity),
                current_monitor,
                WindowRecovery::FallbackAndReturn,
            ))
            .id();
        let application = app
            .world_mut()
            .spawn((
                Window::default(),
                ManagedWindow {
                    name: APPLICATION_WINDOW_KEY.into(),
                },
                OnMonitor(monitors.selected_entity),
                current_monitor,
                WindowRecovery::ApplicationControlled,
            ))
            .id();
        let control = app
            .world_mut()
            .spawn((
                Window::default(),
                setup::UnregisteredControl,
                OnMonitor(monitors.selected_entity),
            ))
            .id();
        ReadinessFixtures {
            accepted_windows: [
                (
                    WindowKey::Primary,
                    primary,
                    WindowRecovery::FallbackAndReturn,
                ),
                (
                    WindowKey::Managed(AUTOMATIC_WINDOW_KEY.into()),
                    automatic,
                    WindowRecovery::FallbackAndReturn,
                ),
                (
                    WindowKey::Managed(APPLICATION_WINDOW_KEY.into()),
                    application,
                    WindowRecovery::ApplicationControlled,
                ),
            ],
            control,
        }
    }

    fn install_recovery_acceptance_records(
        app: &mut App,
        monitors: setup::tests::ProbeTestMonitors,
        accepted_windows: &[(WindowKey, Entity, WindowRecovery)],
    ) -> ProbeTrace {
        let trace = app.world().resource::<ProbeTrace>().clone();
        for (window_key, window, recovery_policy) in accepted_windows {
            app.world_mut()
                .resource_mut::<AcceptedWindowKeys>()
                .0
                .insert(window_key.clone());
            record_recovery_acceptance(
                &trace,
                window_key,
                *window,
                monitors.selected_entity,
                monitors.selected_monitor,
                *recovery_policy,
            );
        }
        trace
    }

    fn assert_recovery_acceptance_records(
        trace: &ProbeTrace,
        monitor: MonitorInfo,
        accepted_windows: &[(WindowKey, Entity, WindowRecovery)],
    ) {
        let monitor_value = format!("{monitor:?}");
        for (window_key, _, _) in accepted_windows {
            let window_key_value = format!("{window_key:?}");
            assert_eq!(
                trace_record_count(
                    trace,
                    KIND_RECOVERY_ACCEPTED,
                    &[
                        (FIELD_WINDOW_KEY, &window_key_value),
                        (FIELD_MONITOR, &monitor_value),
                    ],
                ),
                ACCEPTED_RECORDS_PER_KEY,
            );
        }
        assert_eq!(
            trace_record_count(trace, KIND_RECOVERY_ACCEPTED, &[]),
            accepted_windows.len(),
        );
    }

    fn assert_armed_readiness_records(
        trace: &ProbeTrace,
        monitors: setup::tests::ProbeTestMonitors,
        current_monitor: CurrentMonitor,
        accepted_windows: &[(WindowKey, Entity, WindowRecovery)],
    ) {
        let monitor_value = format!("{:?}", monitors.selected_monitor);
        let monitor_entity_value = format!("{:?}", monitors.selected_entity);
        let current_monitor_value = format!("{current_monitor:?}");
        let accepted_records_value = format!("{ACCEPTED_RECORDS_PER_KEY:?}");
        let armed_value = format!("{VALUE_ARMED:?}");
        for (window_key, _, _) in accepted_windows {
            let window_key_value = format!("{window_key:?}");
            assert_eq!(
                trace_record_count(
                    trace,
                    KIND_PRE_UNPLUG_ASSOCIATION,
                    &[
                        (FIELD_WINDOW_KEY, &window_key_value),
                        (FIELD_MONITOR_ENTITY, &monitor_entity_value),
                        (FIELD_CURRENT_MONITOR, &current_monitor_value),
                        (FIELD_MONITOR, &monitor_value),
                        (FIELD_ACCEPTED_RECORDS, &accepted_records_value),
                        (FIELD_ARMING_STATE, &armed_value),
                    ],
                ),
                1,
            );
        }
        assert_eq!(
            trace_record_count(trace, KIND_PRE_UNPLUG_ASSOCIATION, &[]),
            accepted_windows.len(),
        );
        assert_eq!(trace_record_count(trace, KIND_RECOVERY_READY, &[]), 1);
        assert_eq!(
            trace_record_count(trace, KIND_RECOVERY_ACCEPTED, &[]),
            accepted_windows.len(),
        );
        assert_eq!(trace_record_count(trace, KIND_RECOVERY_UNARMED, &[]), 0);
    }

    fn record_cancellation_request(
        event: On<CancelWindowRecovery>,
        mut requests: ResMut<ConsumerRequests>,
    ) {
        requests.cancellations.push(event.window.clone());
    }

    fn record_restore_request(
        event: On<RestoreWindow>,
        content: Query<(), With<setup::ProbeContentAttached>>,
        mut requests: ResMut<ConsumerRequests>,
    ) {
        assert!(content.contains(event.entity));
        requests.restores.push(event.entity);
    }

    fn consumer_app() -> App {
        let mut app = App::new();
        app.init_resource::<FrameCount>()
            .init_resource::<ApplicationRecoveryCycles>()
            .init_resource::<ConsumerRequests>()
            .insert_resource(ProbeTrace::default())
            .add_observer(on_window_recovery_pending)
            .add_observer(prepare_application_window_restore)
            .add_observer(record_cancellation_request)
            .add_observer(record_restore_request)
            .add_systems(
                PreUpdate,
                (
                    setup::attach_managed_content,
                    request_application_window_restore,
                )
                    .chain(),
            );
        app
    }

    #[test]
    fn armed_production_readiness_waits_for_control_confirmation_then_records_once() {
        let (mut app, monitors) = setup::tests::production_system_app();
        assert!(matches!(
            monitors.selected_monitor.identity,
            MonitorIdentity::Verified(_)
        ));
        let current_monitor = CurrentMonitor {
            monitor_info:          monitors.selected_monitor,
            effective_window_mode: WindowMode::Windowed,
        };
        let fixtures = spawn_readiness_fixtures(&mut app, monitors, current_monitor);
        let trace =
            install_recovery_acceptance_records(&mut app, monitors, &fixtures.accepted_windows);
        assert_recovery_acceptance_records(
            &trace,
            monitors.selected_monitor,
            &fixtures.accepted_windows,
        );
        assert!(
            app.world()
                .get::<ControlPlacementConfirmed>(fixtures.control)
                .is_none()
        );
        assert_eq!(trace_record_count(&trace, KIND_RECOVERY_READY, &[]), 0,);

        app.update();

        assert!(
            app.world()
                .get::<ControlPlacementConfirmed>(fixtures.control)
                .is_some()
        );
        assert_eq!(trace_record_count(&trace, KIND_RECOVERY_READY, &[]), 0,);
        assert_eq!(
            trace_record_count(&trace, KIND_PRE_UNPLUG_ASSOCIATION, &[]),
            0,
        );
        let monitor_entity_value = format!("{:?}", monitors.selected_entity);
        let monitor_value = format!("{:?}", monitors.selected_monitor);
        assert_eq!(
            trace_record_count(
                &trace,
                KIND_CONTROL_ASSOCIATION_CONFIRMED,
                &[
                    (FIELD_MONITOR_ENTITY, &monitor_entity_value),
                    (FIELD_MONITOR, &monitor_value),
                ]
            ),
            1
        );

        app.update();
        app.update();

        assert_armed_readiness_records(
            &trace,
            monitors,
            current_monitor,
            &fixtures.accepted_windows,
        );
    }

    #[test]
    fn first_application_restore_request_waits_for_replacement_content() {
        let mut app = consumer_app();
        let trace = app.world().resource::<ProbeTrace>().clone();
        app.world_mut().trigger(WindowRecoveryAvailable {
            window_key: WindowKey::Managed(APPLICATION_WINDOW_KEY.into()),
            monitor:    returned_monitor(),
        });
        assert!(
            app.world()
                .resource::<ConsumerRequests>()
                .restores
                .is_empty()
        );

        app.update();

        let requests = app.world().resource::<ConsumerRequests>();
        assert_eq!(requests.restores.len(), 1);
        let records = trace.records();
        let content_index = records
            .iter()
            .position(|record| record.kind == KIND_CONTENT_ATTACHED);
        let restore_index = records
            .iter()
            .position(|record| record.kind == KIND_RECOVERY_RESTORE_REQUESTED);
        assert!(matches!(
            (content_index, restore_index),
            (Some(content_index), Some(restore_index)) if content_index < restore_index
        ));
    }

    #[test]
    fn second_application_pending_records_before_queuing_cancellation() {
        let mut app = consumer_app();
        let application_window_key = WindowKey::Managed(APPLICATION_WINDOW_KEY.into());
        for recovery_cycle in 0..SECOND_RECOVERY_CYCLE {
            let application_window = app
                .world_mut()
                .spawn((
                    Window::default(),
                    ManagedWindow {
                        name: APPLICATION_WINDOW_KEY.into(),
                    },
                ))
                .id();
            app.world_mut().trigger(WindowRecoveryPending {
                window_key: application_window_key.clone(),
                monitor_id: recovery_monitor_id(),
            });
            app.world_mut().flush();
            assert!(app.world().get_entity(application_window).is_err());
            assert_eq!(
                app.world()
                    .resource::<ConsumerRequests>()
                    .cancellations
                    .len(),
                usize::from(recovery_cycle + 1 == SECOND_RECOVERY_CYCLE),
            );
        }

        let requests = app.world().resource::<ConsumerRequests>();
        assert_eq!(requests.cancellations, [application_window_key]);
        assert!(requests.restores.is_empty());
        let records = app.world().resource::<ProbeTrace>().records();
        let pending_records: Vec<_> = records
            .iter()
            .filter(|record| record.kind == KIND_RECOVERY_PENDING)
            .collect();
        let cancellation_record = records
            .iter()
            .find(|record| record.kind == KIND_RECOVERY_CANCELLATION_REQUESTED);
        assert_eq!(pending_records.len(), SECOND_RECOVERY_CYCLE);
        assert!(matches!(
            (pending_records.last(), cancellation_record),
            (Some(pending), Some(cancellation)) if pending.sequence < cancellation.sequence
        ));
    }
}
