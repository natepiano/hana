//! One-shot recovery generations, canonical acceptance, and removal classification.

use std::collections::HashMap;

use bevy::prelude::*;
use bevy::window::ClosingWindow;
use bevy::window::OnMonitor;
use bevy::window::PrimaryWindow;

use super::application_controlled::ApplicationControlledRecoveries;
use super::fallback_and_return::AutomaticRestoreIntents;
use super::fallback_and_return::FallbackAndReturnRecoveries;
#[cfg(feature = "monitor-probe")]
use super::monitor_probe::RecoveryAcceptanceProbeRecord;
use crate::CancelWindowRecovery;
use crate::ManagedWindowPersistence;
use crate::WindowKey;
use crate::managed::ManagedWindowRegistry;
use crate::monitors;
use crate::monitors::CurrentMonitor;
use crate::monitors::MonitorId;
use crate::monitors::MonitorIdentity;
use crate::monitors::MonitorInfo;
use crate::monitors::MonitorTopologyRevision;
use crate::monitors::Monitors;
use crate::persistence::CapturedWindowStates;
use crate::platform::ReturnCapability;
use crate::restore;
use crate::restore::NativeWindowReady;
use crate::restore::RestorePreparation;

/// Selects how Clerestory reports or performs monitor-reconnect recovery.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq, Reflect)]
#[reflect(Component)]
#[type_path = "bevy_clerestory::recovery"]
pub enum WindowRecovery {
    /// Do not create a recovery generation.
    #[default]
    Disabled,
    /// Report target loss and return; the application owns all content decisions.
    ApplicationControlled,
    /// Allow Clerestory to preserve a window shell for automatic return.
    FallbackAndReturn,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct RecoveryGeneration(u64);

#[cfg(test)]
impl RecoveryGeneration {
    pub(crate) const fn from_test_raw(raw: u64) -> Self { Self(raw) }
}

#[derive(Clone, Debug)]
struct PendingRegistration {
    entity:     Entity,
    generation: RecoveryGeneration,
    policy:     WindowRecovery,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum CanonicalWindowRole {
    Primary,
    Managed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PrimaryPresence {
    Present,
    Absent,
}

#[derive(Clone, Debug)]
pub(crate) struct RegisteredWindow {
    pub(crate) generation:    RecoveryGeneration,
    pub(crate) policy:        WindowRecovery,
    pub(crate) role:          CanonicalWindowRole,
    pub(crate) window_key:    WindowKey,
    pub(crate) monitor_id:    MonitorId,
    pub(crate) target:        MonitorInfo,
    pub(crate) entity:        Option<Entity>,
    pub(crate) last_revision: Option<MonitorTopologyRevision>,
}

#[derive(Default, Resource)]
pub(crate) struct RecoveryRegistrations {
    next_generation: u64,
    pending:         HashMap<Entity, PendingRegistration>,
    registered:      HashMap<WindowKey, RegisteredWindow>,
}

#[cfg(test)]
#[derive(Debug, PartialEq)]
pub(crate) struct RecoveryRegistrationSnapshot {
    pub(crate) pending:   usize,
    pub(crate) accepted:  Vec<(WindowKey, MonitorInfo)>,
    pub(crate) generated: u64,
}

impl RecoveryRegistrations {
    pub(super) fn begin(&mut self, entity: Entity, policy: WindowRecovery) {
        let generation = RecoveryGeneration(self.next_generation);
        self.next_generation += 1;
        self.pending.insert(
            entity,
            PendingRegistration {
                entity,
                generation,
                policy,
            },
        );
    }

    pub(super) fn registered_mut(&mut self) -> impl Iterator<Item = &mut RegisteredWindow> {
        self.registered.values_mut()
    }

    #[cfg(test)]
    pub(super) fn registered(&self) -> impl Iterator<Item = &RegisteredWindow> {
        self.registered.values()
    }

    pub(super) fn by_entity_mut(&mut self, entity: Entity) -> Option<&mut RegisteredWindow> {
        self.registered
            .values_mut()
            .find(|registration| registration.entity == Some(entity))
    }

    pub(crate) fn by_key(&self, window_key: &WindowKey) -> Option<&RegisteredWindow> {
        self.registered.get(window_key)
    }

    pub(crate) fn by_key_mut(&mut self, window_key: &WindowKey) -> Option<&mut RegisteredWindow> {
        self.registered.get_mut(window_key)
    }

    fn remove_key(&mut self, window_key: &WindowKey) -> Option<RegisteredWindow> {
        self.registered.remove(window_key)
    }

    fn pending_window_key(
        &self,
        entity: Entity,
        primary: PrimaryPresence,
        managed_window_registry: &ManagedWindowRegistry,
    ) -> Option<WindowKey> {
        if !self.pending.contains_key(&entity) {
            return None;
        }
        canonical_window(entity, primary, managed_window_registry).map(|(window_key, _)| window_key)
    }

    fn remove_pending_key(
        &mut self,
        window_key: &WindowKey,
        managed_window_registry: &ManagedWindowRegistry,
        primary_windows: &Query<(), With<PrimaryWindow>>,
    ) -> Option<Entity> {
        let entity = self.pending.keys().copied().find(|entity| {
            canonical_window(
                *entity,
                if primary_windows.contains(*entity) {
                    PrimaryPresence::Present
                } else {
                    PrimaryPresence::Absent
                },
                managed_window_registry,
            )
            .is_some_and(|(pending_key, _)| pending_key == *window_key)
        })?;
        self.pending.remove(&entity);
        Some(entity)
    }
}

#[cfg(test)]
pub(crate) fn registration_snapshot(world: &World) -> RecoveryRegistrationSnapshot {
    let registrations = world.resource::<RecoveryRegistrations>();
    RecoveryRegistrationSnapshot {
        pending:   registrations.pending.len(),
        accepted:  registrations
            .registered
            .values()
            .map(|registration| (registration.window_key.clone(), registration.target))
            .collect(),
        generated: registrations.next_generation,
    }
}

pub(super) fn on_window_recovery_added(
    add: On<Add, WindowRecovery>,
    recoveries: Query<&WindowRecovery>,
    mut registrations: ResMut<RecoveryRegistrations>,
) {
    let Ok(policy) = recoveries.get(add.entity).copied() else {
        return;
    };
    if policy == WindowRecovery::Disabled {
        return;
    }
    registrations.begin(add.entity, policy);
}

pub(crate) fn canonical_window(
    entity: Entity,
    primary: PrimaryPresence,
    managed_window_registry: &ManagedWindowRegistry,
) -> Option<(WindowKey, CanonicalWindowRole)> {
    match (primary, managed_window_registry.name(entity)) {
        (PrimaryPresence::Present, None) => {
            Some((WindowKey::Primary, CanonicalWindowRole::Primary))
        },
        (PrimaryPresence::Absent, Some(name)) => Some((
            WindowKey::Managed(name.to_string()),
            CanonicalWindowRole::Managed,
        )),
        (PrimaryPresence::Present, Some(_)) | (PrimaryPresence::Absent, None) => None,
    }
}

pub(crate) fn accept_eligible_registrations(
    mut registrations: ResMut<RecoveryRegistrations>,
    mut application_controlled: ResMut<ApplicationControlledRecoveries>,
    mut fallback_and_return: ResMut<FallbackAndReturnRecoveries>,
    mut restore_intents: ResMut<AutomaticRestoreIntents>,
    candidates: Query<(
        &Window,
        &OnMonitor,
        &CurrentMonitor,
        Has<PrimaryWindow>,
        Has<NativeWindowReady>,
        Has<RestorePreparation>,
    )>,
    managed_window_registry: Res<ManagedWindowRegistry>,
    monitors: Res<Monitors>,
    revision: Res<MonitorTopologyRevision>,
    mut captured_window_states: ResMut<CapturedWindowStates>,
    platform: Res<crate::Platform>,
    #[cfg(feature = "monitor-probe")] frame_count: Option<Res<bevy::diagnostic::FrameCount>>,
) {
    let pending: Vec<_> = registrations.pending.values().cloned().collect();
    for pending_registration in pending {
        let Ok((window, on_monitor, current_monitor, primary, native_ready, restoring)) =
            candidates.get(pending_registration.entity)
        else {
            continue;
        };
        if !native_ready || restoring {
            continue;
        }
        let Some((window_key, role)) = canonical_window(
            pending_registration.entity,
            if primary {
                PrimaryPresence::Present
            } else {
                PrimaryPresence::Absent
            },
            &managed_window_registry,
        ) else {
            continue;
        };
        let Some(installed_monitor) =
            monitors::exact_monitor_association(on_monitor, current_monitor, &monitors)
        else {
            continue;
        };
        let MonitorIdentity::Verified(monitor_id) = installed_monitor.identity else {
            continue;
        };
        let Some(placement) = captured_window_states.captured_placement(&window_key) else {
            continue;
        };
        if placement.monitor_snapshot != installed_monitor {
            continue;
        }
        if pending_registration.policy == WindowRecovery::FallbackAndReturn
            && platform.fallback_return_capability(placement.position, &placement.saved_window_mode)
                != ReturnCapability::Supported
        {
            continue;
        }

        captured_window_states.freeze(&window_key);
        let registration = RegisteredWindow {
            generation: pending_registration.generation,
            policy: pending_registration.policy,
            role,
            window_key: window_key.clone(),
            monitor_id,
            target: installed_monitor,
            entity: Some(pending_registration.entity),
            last_revision: Some(*revision),
        };
        registrations.pending.remove(&pending_registration.entity);
        debug!(
            "[accept_eligible_registrations] [{window_key}] accepted generation {:?} for {monitor_id:?}; role={:?} target={:?} retained_window_shell={}",
            registration.generation,
            registration.role,
            registration.target,
            pending_registration.policy == WindowRecovery::FallbackAndReturn,
        );
        if let Some(previous_registration) = registrations.by_key(&window_key).cloned() {
            application_controlled.cancel(&window_key, previous_registration.generation);
            fallback_and_return.cancel(
                &window_key,
                previous_registration.generation,
                &mut restore_intents,
            );
        }
        registrations
            .registered
            .insert(window_key.clone(), registration);
        if pending_registration.policy == WindowRecovery::ApplicationControlled {
            application_controlled.accept(window_key.clone(), pending_registration.generation);
        } else {
            fallback_and_return.accept(
                window_key.clone(),
                pending_registration.generation,
                window.clone(),
            );
        }
        #[cfg(feature = "monitor-probe")]
        RecoveryAcceptanceProbeRecord {
            frame_count:    frame_count
                .as_deref()
                .map_or(0, |frame_count| frame_count.0),
            window_key:     &window_key,
            entity:         pending_registration.entity,
            monitor_entity: on_monitor.0,
            monitor:        installed_monitor,
            policy:         pending_registration.policy,
        }
        .emit();
    }
}

pub(super) fn on_window_removed(
    removed: On<Remove, Window>,
    mut registrations: ResMut<RecoveryRegistrations>,
    mut application_controlled: ResMut<ApplicationControlledRecoveries>,
    mut fallback_and_return: ResMut<FallbackAndReturnRecoveries>,
    mut restore_intents: ResMut<AutomaticRestoreIntents>,
    mut captured_window_states: ResMut<CapturedWindowStates>,
) {
    registrations.pending.remove(&removed.entity);
    let Some(registration) = registrations.by_entity_mut(removed.entity) else {
        return;
    };
    registration.entity = None;
    captured_window_states.freeze(&registration.window_key);
    application_controlled.window_removed(&registration.window_key, registration.generation);
    fallback_and_return.window_removed(
        &registration.window_key,
        registration.generation,
        &mut restore_intents,
    );
}

pub(super) fn on_cancel_window_recovery(
    cancel: On<CancelWindowRecovery>,
    mut commands: Commands,
    mut registrations: ResMut<RecoveryRegistrations>,
    mut application_controlled: ResMut<ApplicationControlledRecoveries>,
    mut fallback_and_return: ResMut<FallbackAndReturnRecoveries>,
    mut restore_intents: ResMut<AutomaticRestoreIntents>,
    managed_window_registry: Res<ManagedWindowRegistry>,
    primary_windows: Query<(), With<PrimaryWindow>>,
    managed_window_persistence: Res<ManagedWindowPersistence>,
    mut captured_window_states: ResMut<CapturedWindowStates>,
) {
    let pending_entity = registrations.remove_pending_key(
        &cancel.window,
        &managed_window_registry,
        &primary_windows,
    );
    let registration = registrations.remove_key(&cancel.window);
    if pending_entity.is_none() && registration.is_none() {
        return;
    }
    if let Some(registration) = &registration {
        application_controlled.cancel(&cancel.window, registration.generation);
        fallback_and_return.cancel(
            &cancel.window,
            registration.generation,
            &mut restore_intents,
        );
    }
    let entity = registration
        .as_ref()
        .and_then(|registration| registration.entity)
        .or(pending_entity);
    captured_window_states.cancel(&cancel.window, entity, &managed_window_persistence);
    if let Some(entity) = entity {
        restore::cancel_restore(&mut commands, entity);
    }
}

pub(super) fn record_os_close_intent(
    closing_windows: Query<Entity, Added<ClosingWindow>>,
    registrations: Res<RecoveryRegistrations>,
    managed_window_registry: Res<ManagedWindowRegistry>,
    primary_windows: Query<(), With<PrimaryWindow>>,
    mut commands: Commands,
) {
    for entity in &closing_windows {
        let Some(window_key) = registrations
            .registered
            .values()
            .find(|registration| registration.entity == Some(entity))
            .map(|registration| registration.window_key.clone())
            .or_else(|| {
                registrations.pending_window_key(
                    entity,
                    if primary_windows.contains(entity) {
                        PrimaryPresence::Present
                    } else {
                        PrimaryPresence::Absent
                    },
                    &managed_window_registry,
                )
            })
        else {
            continue;
        };
        commands.trigger(CancelWindowRecovery { window: window_key });
    }
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "monitor-probe")]
    use bevy::log::tracing_subscriber::Registry;
    #[cfg(feature = "monitor-probe")]
    use bevy::log::tracing_subscriber::prelude::*;
    use bevy::reflect::TypePath;

    use super::*;
    use crate::ManagedWindow;
    use crate::Platform;
    use crate::RestoreWindow;
    use crate::WindowRecoveryAvailable;
    use crate::WindowRecoveryPending;
    use crate::managed::on_managed_window_added;
    use crate::managed::on_managed_window_removed;
    use crate::persistence;
    #[cfg(feature = "monitor-probe")]
    use crate::persistence::CapturedWindowPosition;
    #[cfg(feature = "monitor-probe")]
    use crate::persistence::SavedWindowMode;

    #[cfg(feature = "monitor-probe")]
    mod example_probe {
        pub(super) mod constants {
            pub(crate) const FIELD_MONITOR: &str = "monitor";
            pub(crate) const FIELD_MONITOR_ENTITY: &str = "monitor_entity";
            pub(crate) const FIELD_TOPOLOGY_REVISION: &str = "topology_revision";
            pub(crate) const FIELD_TRANSITION: &str = "transition";
            pub(crate) const KIND_MONITOR_CONNECTED: &str = "monitor-connected";
            pub(crate) const KIND_MONITOR_DISCONNECTED: &str = "monitor-disconnected";
            pub(crate) const KIND_MONITOR_TOPOLOGY: &str = "monitor-topology";
            pub(crate) const KIND_RECOVERY_ACCEPTED: &str = "recovery-accepted";
            pub(crate) const MONITOR_PROBE_TARGET: &str = "bevy_clerestory::monitor_probe";
            pub(crate) const PRODUCER_MONITOR_CONNECTED: &str = "observer::MonitorConnected";
            pub(crate) const PRODUCER_MONITOR_DISCONNECTED: &str = "observer::MonitorDisconnected";
            pub(crate) const RECOVERY_PROBE_TARGET: &str = "bevy_clerestory::recovery_probe";
            pub(crate) const TRACE_FIELD_FRAME_COUNT: &str = "frame_count";
            pub(crate) const TRACE_FIELD_PRODUCER_SCHEDULE: &str = "producer_schedule";
            pub(crate) const TRANSITION_CREATED: &str = "created";
            pub(crate) const TRANSITION_REMOVED: &str = "removed";
        }

        pub(super) mod trace {
            include!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/examples/restore_after_reconnect/trace.rs"
            ));
        }
    }

    fn pending_lifecycle_app() -> App {
        let mut app = App::new();
        app.insert_resource(Monitors::from_test_monitors(std::iter::empty::<(
            Entity,
            MonitorInfo,
        )>()))
        .insert_resource(MonitorTopologyRevision::default())
        .insert_resource(Platform::Windows)
        .insert_resource(ManagedWindowPersistence::RememberAll)
        .init_resource::<ManagedWindowRegistry>()
        .init_resource::<CapturedWindowStates>()
        .add_plugins(crate::recovery::RecoveryPlugin)
        .add_observer(persistence::on_primary_window_removed)
        .add_observer(persistence::on_window_removed)
        .add_observer(on_managed_window_added)
        .add_observer(on_managed_window_removed);
        app
    }

    #[cfg(feature = "monitor-probe")]
    fn trace_field<'a>(
        record: &'a example_probe::trace::TraceRecord,
        name: &str,
    ) -> Option<&'a str> {
        record
            .fields
            .iter()
            .find(|(field_name, _)| field_name == name)
            .map(|(_, value)| value.as_str())
    }

    #[cfg(feature = "monitor-probe")]
    fn assert_acceptance_record(
        record: &example_probe::trace::TraceRecord,
        entity: Entity,
        monitor_entity: Entity,
        monitor: MonitorInfo,
    ) {
        assert_eq!(
            record.producer,
            crate::constants::RECOVERY_ACCEPTANCE_PRODUCER
        );
        assert_eq!(
            record.kind,
            example_probe::constants::KIND_RECOVERY_ACCEPTED
        );
        let entity = format!("{entity:?}");
        let monitor_entity = format!("{monitor_entity:?}");
        let monitor = format!("{monitor:?}");
        assert_eq!(trace_field(record, "window_key"), Some("Primary"));
        assert_eq!(trace_field(record, "window"), Some(entity.as_str()));
        assert_eq!(
            trace_field(record, "monitor_entity"),
            Some(monitor_entity.as_str())
        );
        assert_eq!(trace_field(record, "monitor"), Some(monitor.as_str()));
        assert_eq!(
            trace_field(record, "recovery_policy"),
            Some("ApplicationControlled")
        );
    }

    #[test]
    fn recovery_public_type_paths_are_stable() {
        assert_eq!(
            [
                <WindowRecovery as TypePath>::type_path(),
                <WindowRecoveryPending as TypePath>::type_path(),
                <WindowRecoveryAvailable as TypePath>::type_path(),
                <RestoreWindow as TypePath>::type_path(),
                <CancelWindowRecovery as TypePath>::type_path(),
            ],
            [
                "bevy_clerestory::recovery::WindowRecovery",
                "bevy_clerestory::recovery::WindowRecoveryPending",
                "bevy_clerestory::recovery::WindowRecoveryAvailable",
                "bevy_clerestory::recovery::RestoreWindow",
                "bevy_clerestory::recovery::CancelWindowRecovery",
            ]
        );
    }

    #[test]
    fn disabled_addition_does_not_create_a_generation() {
        let mut app = App::new();
        app.init_resource::<RecoveryRegistrations>()
            .add_observer(on_window_recovery_added);
        let entity = app.world_mut().spawn(WindowRecovery::Disabled).id();
        app.world_mut().flush();

        let registrations = app.world().resource::<RecoveryRegistrations>();
        assert!(registrations.pending.is_empty());
        assert_eq!(registrations.next_generation, 0);
        assert!(app.world().get::<WindowRecovery>(entity).is_some());
    }

    #[test]
    fn mutation_and_removal_keep_the_copied_generation() {
        let mut app = App::new();
        app.init_resource::<RecoveryRegistrations>()
            .add_observer(on_window_recovery_added);
        let entity = app
            .world_mut()
            .spawn(WindowRecovery::ApplicationControlled)
            .id();
        app.world_mut().flush();
        app.world_mut()
            .entity_mut(entity)
            .insert(WindowRecovery::FallbackAndReturn);
        app.world_mut()
            .entity_mut(entity)
            .remove::<WindowRecovery>();
        app.world_mut().flush();

        let registrations = app.world().resource::<RecoveryRegistrations>();
        let pending = registrations.pending.get(&entity);
        assert_eq!(
            pending.map(|pending| pending.policy),
            Some(WindowRecovery::ApplicationControlled)
        );
        assert_eq!(registrations.next_generation, 1);
    }

    #[test]
    fn managed_canonicalization_waits_for_authoritative_deduplication() {
        let entity = Entity::from_bits(9);
        let mut managed_window_registry = ManagedWindowRegistry::default();

        assert_eq!(
            canonical_window(entity, PrimaryPresence::Absent, &managed_window_registry),
            None
        );
        managed_window_registry
            .names
            .insert("secondary-2".to_string());
        managed_window_registry
            .entities
            .insert(entity, "secondary-2".to_string());
        assert_eq!(
            canonical_window(entity, PrimaryPresence::Absent, &managed_window_registry),
            Some((
                WindowKey::Managed("secondary-2".to_string()),
                CanonicalWindowRole::Managed,
            ))
        );
        assert_eq!(
            canonical_window(entity, PrimaryPresence::Present, &managed_window_registry),
            None
        );
    }

    #[test]
    fn cancellation_prevents_rearming_until_another_component_addition() {
        let mut app = App::new();
        app.insert_resource(ManagedWindowPersistence::RememberAll)
            .init_resource::<ManagedWindowRegistry>()
            .init_resource::<CapturedWindowStates>()
            .init_resource::<RecoveryRegistrations>()
            .init_resource::<ApplicationControlledRecoveries>()
            .init_resource::<FallbackAndReturnRecoveries>()
            .init_resource::<AutomaticRestoreIntents>()
            .add_observer(on_window_recovery_added)
            .add_observer(on_cancel_window_recovery);
        let entity = app
            .world_mut()
            .spawn((PrimaryWindow, WindowRecovery::ApplicationControlled))
            .id();
        app.world_mut().flush();
        assert_eq!(
            app.world()
                .resource::<RecoveryRegistrations>()
                .pending
                .len(),
            1
        );

        app.world_mut().trigger(CancelWindowRecovery {
            window: WindowKey::Primary,
        });
        app.world_mut()
            .entity_mut(entity)
            .insert(WindowRecovery::FallbackAndReturn);
        app.world_mut().flush();
        assert!(
            app.world()
                .resource::<RecoveryRegistrations>()
                .pending
                .is_empty()
        );

        app.world_mut()
            .entity_mut(entity)
            .remove::<WindowRecovery>();
        app.world_mut()
            .entity_mut(entity)
            .insert(WindowRecovery::FallbackAndReturn);
        app.world_mut().flush();

        let registrations = app.world().resource::<RecoveryRegistrations>();
        assert_eq!(registrations.pending.len(), 1);
        assert_eq!(registrations.next_generation, 2);
        assert_eq!(
            registrations
                .pending
                .get(&entity)
                .map(|pending| pending.policy),
            Some(WindowRecovery::FallbackAndReturn)
        );
    }

    #[test]
    fn close_intent_cancels_pending_primary_and_authoritative_managed_generations() {
        let mut app = pending_lifecycle_app();
        let primary = app
            .world_mut()
            .spawn((
                Window::default(),
                PrimaryWindow,
                WindowRecovery::ApplicationControlled,
            ))
            .id();
        let managed = app
            .world_mut()
            .spawn((
                Window::default(),
                ManagedWindow {
                    name: "secondary".to_string(),
                },
                WindowRecovery::ApplicationControlled,
            ))
            .id();
        app.world_mut().flush();
        assert_eq!(
            app.world()
                .resource::<RecoveryRegistrations>()
                .pending
                .len(),
            2
        );

        app.world_mut().entity_mut(primary).insert(ClosingWindow);
        app.world_mut().entity_mut(managed).insert(ClosingWindow);
        app.update();

        let registrations = app.world().resource::<RecoveryRegistrations>();
        assert!(registrations.pending.is_empty());
        assert!(registrations.registered.is_empty());
    }

    #[test]
    fn window_removal_and_despawn_before_eligibility_remove_pending_generations() {
        let mut app = pending_lifecycle_app();
        let primary = app
            .world_mut()
            .spawn((
                Window::default(),
                PrimaryWindow,
                WindowRecovery::ApplicationControlled,
            ))
            .id();
        let managed = app
            .world_mut()
            .spawn((
                Window::default(),
                ManagedWindow {
                    name: "secondary".to_string(),
                },
                WindowRecovery::ApplicationControlled,
            ))
            .id();
        app.world_mut().flush();
        assert_eq!(
            app.world()
                .resource::<RecoveryRegistrations>()
                .pending
                .len(),
            2
        );

        app.world_mut().entity_mut(primary).remove::<Window>();
        assert!(app.world_mut().despawn(managed));
        app.world_mut().flush();

        let registrations = app.world().resource::<RecoveryRegistrations>();
        assert!(registrations.pending.is_empty());
        assert!(registrations.registered.is_empty());
        assert!(app.world().get::<Window>(primary).is_none());
        assert!(
            app.world()
                .resource::<ManagedWindowRegistry>()
                .entities
                .is_empty()
        );
    }

    #[cfg(feature = "monitor-probe")]
    #[test]
    fn acceptance_probe_waits_for_core_acceptance_and_emits_the_copied_baseline_once() {
        use bevy::diagnostic::FrameCount;
        use bevy::window::WindowMode;

        use self::example_probe::trace as example_trace;
        use crate::monitors::CurrentMonitor;
        use crate::monitors::MonitorId;
        use crate::persistence::CapturedWindowPlacement;
        use crate::restore::NativeWindowReady;

        let monitor = MonitorInfo {
            identity:          MonitorIdentity::Verified(MonitorId::from_test_raw(17)),
            index:             1,
            scale:             2.0,
            physical_position: IVec2::new(1_920, 0),
            physical_size:     UVec2::new(2_560, 1_440),
        };
        let trace = example_trace::ProbeTrace::default();
        let mut app = App::new();
        let monitor_entity = app.world_mut().spawn_empty().id();
        app.insert_resource(trace.clone())
            .insert_resource(Monitors::from_test_monitors([(monitor_entity, monitor)]))
            .insert_resource(MonitorTopologyRevision::default())
            .insert_resource(Platform::Windows)
            .insert_resource(ManagedWindowPersistence::RememberAll)
            .init_resource::<FrameCount>()
            .init_resource::<ManagedWindowRegistry>()
            .init_resource::<CapturedWindowStates>()
            .add_plugins(crate::recovery::RecoveryPlugin)
            .add_observer(example_trace::on_monitor_connected)
            .add_observer(example_trace::on_monitor_disconnected);
        let entity = app
            .world_mut()
            .spawn((
                Window::default(),
                PrimaryWindow,
                OnMonitor(monitor_entity),
                CurrentMonitor {
                    monitor_info:          monitor,
                    effective_window_mode: WindowMode::Windowed,
                },
                NativeWindowReady,
                WindowRecovery::ApplicationControlled,
            ))
            .id();
        app.world_mut().flush();
        let layer = example_trace::monitor_probe_layer(&mut app);
        assert!(layer.is_some());
        let Some(layer) = layer else {
            return;
        };
        let subscriber = Registry::default().with(layer);

        bevy::log::tracing::subscriber::with_default(subscriber, || {
            app.update();
            assert!(trace.records().is_empty());
            assert_eq!(
                app.world()
                    .resource::<RecoveryRegistrations>()
                    .registered()
                    .count(),
                0
            );

            app.world_mut()
                .entity_mut(entity)
                .insert(WindowRecovery::FallbackAndReturn);
            app.world_mut()
                .resource_mut::<CapturedWindowStates>()
                .promote(
                    WindowKey::Primary,
                    entity,
                    CapturedWindowPlacement {
                        monitor_snapshot:  monitor,
                        position:          CapturedWindowPosition::Restorable {
                            logical_offset: IVec2::new(40, 60),
                        },
                        logical_size:      UVec2::new(800, 600),
                        saved_window_mode: SavedWindowMode::Windowed,
                        captured_scale:    monitor.scale,
                    },
                );
            app.update();
            app.update();
        });

        let records = trace.records();
        assert_eq!(records.len(), 1);
        assert_acceptance_record(&records[0], entity, monitor_entity, monitor);
        assert_eq!(
            app.world()
                .resource::<RecoveryRegistrations>()
                .registered()
                .count(),
            1
        );
    }
}
