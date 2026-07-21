//! Application-controlled recovery phases and factual topology transitions.

use std::collections::HashMap;

use bevy::prelude::*;

use super::registration::RecoveryGeneration;
use super::registration::RecoveryRegistrations;
use super::registration::WindowRecovery;
use crate::RestoreWindow;
use crate::WindowKey;
use crate::WindowRecoveryAvailable;
use crate::WindowRecoveryPending;
use crate::WindowRestoreMismatch;
use crate::WindowRestored;
use crate::monitors::MonitorId;
use crate::monitors::MonitorInfo;
use crate::monitors::MonitorTopologyRevision;
use crate::monitors::Monitors;
use crate::persistence::CapturedWindowStates;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ApplicationControlledPhase {
    Healthy,
    RemovalPending,
    TargetAbsent,
    TargetAvailable,
    Restoring,
    RetryableFailure,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct ApplicationControlledRecovery {
    generation:   RecoveryGeneration,
    phase:        ApplicationControlledPhase,
    notification: Option<ApplicationControlledNotification>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum ApplicationControlledNotification {
    Pending(MonitorId),
    Available(MonitorInfo),
}

#[derive(Default, Resource)]
pub(super) struct ApplicationControlledRecoveries {
    entries: HashMap<WindowKey, ApplicationControlledRecovery>,
}

impl ApplicationControlledRecoveries {
    pub(super) fn accept(&mut self, window_key: WindowKey, generation: RecoveryGeneration) {
        self.entries.insert(
            window_key,
            ApplicationControlledRecovery {
                generation,
                phase: ApplicationControlledPhase::Healthy,
                notification: None,
            },
        );
    }

    pub(super) fn window_removed(
        &mut self,
        window_key: &WindowKey,
        generation: RecoveryGeneration,
    ) {
        if let Some(recovery) = self.entries.get_mut(window_key)
            && recovery.generation == generation
            && recovery.phase == ApplicationControlledPhase::Healthy
        {
            recovery.phase = ApplicationControlledPhase::RemovalPending;
        }
    }

    pub(super) fn cancel(&mut self, window_key: &WindowKey, generation: RecoveryGeneration) {
        if self
            .entries
            .get(window_key)
            .is_some_and(|recovery| recovery.generation == generation)
        {
            self.entries.remove(window_key);
        }
    }
}

pub(super) fn evaluate_topology(
    revision: Res<MonitorTopologyRevision>,
    monitors: Res<Monitors>,
    mut registrations: ResMut<RecoveryRegistrations>,
    mut recoveries: ResMut<ApplicationControlledRecoveries>,
    mut captured_window_states: ResMut<CapturedWindowStates>,
) {
    for registration in registrations.registered_mut() {
        if registration.policy != WindowRecovery::ApplicationControlled
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

        if let Some(monitor) = monitors.by_id(registration.monitor_id).copied() {
            if matches!(
                recovery.phase,
                ApplicationControlledPhase::TargetAbsent
                    | ApplicationControlledPhase::RetryableFailure
            ) {
                recovery.phase = ApplicationControlledPhase::TargetAvailable;
                recovery.notification = Some(ApplicationControlledNotification::Available(monitor));
            }
        } else if matches!(
            recovery.phase,
            ApplicationControlledPhase::Healthy
                | ApplicationControlledPhase::RemovalPending
                | ApplicationControlledPhase::TargetAvailable
        ) {
            captured_window_states.freeze(&registration.window_key);
            recovery.phase = ApplicationControlledPhase::TargetAbsent;
            recovery.notification = Some(ApplicationControlledNotification::Pending(
                registration.monitor_id,
            ));
        }
    }
}

pub(super) fn emit_topology_notifications(
    registrations: Res<RecoveryRegistrations>,
    mut recoveries: ResMut<ApplicationControlledRecoveries>,
    mut commands: Commands,
) {
    for (window_key, recovery) in &mut recoveries.entries {
        let Some(registration) = registrations.by_key(window_key) else {
            continue;
        };
        if registration.generation != recovery.generation {
            continue;
        }
        match recovery.notification.take() {
            Some(ApplicationControlledNotification::Pending(monitor_id)) => {
                commands.trigger(WindowRecoveryPending {
                    window_key: window_key.clone(),
                    monitor_id,
                });
            },
            Some(ApplicationControlledNotification::Available(monitor)) => {
                commands.trigger(WindowRecoveryAvailable {
                    window_key: window_key.clone(),
                    monitor,
                });
            },
            None => {},
        }
    }
}

pub(super) fn on_restore_window(
    restore: On<RestoreWindow>,
    mut registrations: ResMut<RecoveryRegistrations>,
    mut recoveries: ResMut<ApplicationControlledRecoveries>,
) {
    let Some(registration) = registrations.by_entity_mut(restore.entity) else {
        return;
    };
    if registration.policy != WindowRecovery::ApplicationControlled {
        return;
    }
    let Some(recovery) = recoveries.entries.get_mut(&registration.window_key) else {
        return;
    };
    if recovery.generation == registration.generation
        && matches!(
            recovery.phase,
            ApplicationControlledPhase::TargetAvailable
                | ApplicationControlledPhase::RetryableFailure
        )
    {
        recovery.phase = ApplicationControlledPhase::Restoring;
    }
}

pub(super) fn on_window_restored(
    restored: On<WindowRestored>,
    registrations: Res<RecoveryRegistrations>,
    mut recoveries: ResMut<ApplicationControlledRecoveries>,
) {
    let Some(registration) = registrations
        .registered()
        .find(|registration| registration.entity == Some(restored.entity))
    else {
        return;
    };
    if let Some(recovery) = recoveries.entries.get_mut(&registration.window_key)
        && recovery.generation == registration.generation
        && recovery.phase == ApplicationControlledPhase::Restoring
    {
        recovery.phase = ApplicationControlledPhase::Healthy;
    }
}

pub(super) fn on_window_restore_mismatch(
    mismatch: On<WindowRestoreMismatch>,
    mut registrations: ResMut<RecoveryRegistrations>,
    mut recoveries: ResMut<ApplicationControlledRecoveries>,
) {
    let Some(registration) = registrations.by_entity_mut(mismatch.entity) else {
        return;
    };
    if let Some(recovery) = recoveries.entries.get_mut(&registration.window_key)
        && recovery.generation == registration.generation
        && recovery.phase == ApplicationControlledPhase::Restoring
    {
        recovery.phase = ApplicationControlledPhase::RetryableFailure;
    }
}

#[cfg(test)]
mod tests {
    use bevy::window::ClosingWindow;
    use bevy::window::OnMonitor;
    use bevy::window::PrimaryWindow;
    use bevy::window::WindowMode;
    use bevy_kana::ToI32;

    use super::*;
    use crate::ManagedWindow;
    use crate::ManagedWindowPersistence;
    use crate::Platform;
    use crate::WindowRecovery;
    use crate::managed::ManagedWindowRegistry;
    use crate::managed::on_managed_window_added;
    use crate::managed::on_managed_window_removed;
    use crate::monitors::CurrentMonitor;
    use crate::monitors::MonitorId;
    use crate::monitors::MonitorIdentity;
    use crate::monitors::MonitorInfo;
    use crate::persistence;
    use crate::persistence::CapturedWindowPlacement;
    use crate::persistence::CapturedWindowPosition;
    use crate::persistence::SavedWindowMode;
    use crate::recovery::RecoveryPlugin;
    use crate::restore::NativeWindowReady;

    const CAPTURE_OFFSET: IVec2 = IVec2::new(30, 40);
    const MONITOR_PHYSICAL_SIZE: UVec2 = UVec2::new(1_920, 1_080);
    const MONITOR_WIDTH: i32 = 1_920;
    const TARGET_ID: MonitorId = MonitorId::from_test_raw(41);
    const WINDOW_LOGICAL_SIZE: UVec2 = UVec2::new(800, 600);

    #[derive(Default, Resource)]
    struct RecoveryFacts {
        pending:   Vec<(WindowKey, MonitorId)>,
        available: Vec<(WindowKey, MonitorInfo)>,
    }

    fn record_pending(event: On<WindowRecoveryPending>, mut facts: ResMut<RecoveryFacts>) {
        facts
            .pending
            .push((event.window_key.clone(), event.monitor_id));
    }

    fn record_available(event: On<WindowRecoveryAvailable>, mut facts: ResMut<RecoveryFacts>) {
        facts
            .available
            .push((event.window_key.clone(), event.monitor));
    }

    fn monitor_info(identity: MonitorIdentity, index: usize) -> MonitorInfo {
        MonitorInfo {
            identity,
            index,
            scale: 1.0,
            physical_position: IVec2::new(index.to_i32() * MONITOR_WIDTH, 0),
            physical_size: MONITOR_PHYSICAL_SIZE,
        }
    }

    fn placement(monitor_snapshot: MonitorInfo) -> CapturedWindowPlacement {
        placement_with(
            monitor_snapshot,
            CapturedWindowPosition::Restorable {
                logical_offset: CAPTURE_OFFSET,
            },
            SavedWindowMode::Windowed,
        )
    }

    fn placement_with(
        monitor_snapshot: MonitorInfo,
        position: CapturedWindowPosition,
        saved_window_mode: SavedWindowMode,
    ) -> CapturedWindowPlacement {
        CapturedWindowPlacement {
            monitor_snapshot,
            position,
            logical_size: WINDOW_LOGICAL_SIZE,
            saved_window_mode,
            captured_scale: monitor_snapshot.scale,
        }
    }

    fn recovery_app(
        identity: MonitorIdentity,
        managed_window_persistence: ManagedWindowPersistence,
    ) -> (App, Entity, Entity) {
        recovery_app_with(
            identity,
            managed_window_persistence,
            WindowRecovery::ApplicationControlled,
            CapturedWindowPosition::Restorable {
                logical_offset: CAPTURE_OFFSET,
            },
            SavedWindowMode::Windowed,
            Platform::Windows,
        )
    }

    fn recovery_app_with(
        identity: MonitorIdentity,
        managed_window_persistence: ManagedWindowPersistence,
        window_recovery: WindowRecovery,
        position: CapturedWindowPosition,
        saved_window_mode: SavedWindowMode,
        platform: Platform,
    ) -> (App, Entity, Entity) {
        let mut app = App::new();
        let monitor_entity = app.world_mut().spawn_empty().id();
        let installed_monitor = monitor_info(identity, 0);
        app.insert_resource(Monitors::from_test_monitors([(
            monitor_entity,
            installed_monitor,
        )]))
        .insert_resource(MonitorTopologyRevision::default())
        .insert_resource(platform)
        .insert_resource(managed_window_persistence)
        .init_resource::<ManagedWindowRegistry>()
        .init_resource::<CapturedWindowStates>()
        .init_resource::<RecoveryFacts>()
        .add_plugins(RecoveryPlugin)
        .add_observer(persistence::on_primary_window_removed)
        .add_observer(persistence::on_window_removed)
        .add_observer(on_managed_window_added)
        .add_observer(on_managed_window_removed)
        .add_observer(record_pending)
        .add_observer(record_available);
        let entity = app
            .world_mut()
            .spawn((
                Window::default(),
                PrimaryWindow,
                OnMonitor(monitor_entity),
                CurrentMonitor {
                    monitor_info:          installed_monitor,
                    effective_window_mode: WindowMode::Windowed,
                },
                NativeWindowReady,
            ))
            .id();
        app.world_mut()
            .resource_mut::<CapturedWindowStates>()
            .promote(
                WindowKey::Primary,
                entity,
                placement_with(installed_monitor, position, saved_window_mode),
            );
        app.world_mut().entity_mut(entity).insert(window_recovery);
        app.world_mut().flush();
        app.update();
        (app, entity, monitor_entity)
    }

    fn install_topology(
        app: &mut App,
        revision: u64,
        monitors: impl IntoIterator<Item = (Entity, MonitorInfo)>,
    ) {
        app.insert_resource(Monitors::from_test_monitors(monitors));
        app.insert_resource(MonitorTopologyRevision::from_test_raw(revision));
    }

    #[test]
    fn first_eligible_baseline_evaluates_revision_zero_once() {
        let (mut app, _, _) = recovery_app(
            MonitorIdentity::Verified(TARGET_ID),
            ManagedWindowPersistence::RememberAll,
        );

        app.update();

        let registrations = app.world().resource::<RecoveryRegistrations>();
        assert_eq!(registrations.registered().count(), 1);
        let facts = app.world().resource::<RecoveryFacts>();
        assert!(facts.pending.is_empty());
        assert!(facts.available.is_empty());
    }

    #[test]
    fn unverified_and_stale_associations_wait_for_exact_verified_target() {
        let (mut app, entity, monitor_entity) = recovery_app(
            MonitorIdentity::Unverified,
            ManagedWindowPersistence::RememberAll,
        );
        assert_eq!(
            app.world()
                .resource::<RecoveryRegistrations>()
                .registered()
                .count(),
            0
        );

        let verified_monitor = monitor_info(MonitorIdentity::Verified(TARGET_ID), 0);
        install_topology(&mut app, 1, [(monitor_entity, verified_monitor)]);
        app.world_mut()
            .resource_mut::<CapturedWindowStates>()
            .promote(WindowKey::Primary, entity, placement(verified_monitor));
        app.update();
        assert_eq!(
            app.world()
                .resource::<RecoveryRegistrations>()
                .registered()
                .count(),
            0
        );

        app.world_mut().entity_mut(entity).insert(CurrentMonitor {
            monitor_info:          verified_monitor,
            effective_window_mode: WindowMode::Windowed,
        });
        app.update();

        assert_eq!(
            app.world()
                .resource::<RecoveryRegistrations>()
                .registered()
                .count(),
            1
        );
    }

    #[test]
    fn automatic_registration_requires_a_supported_return_mechanism() {
        let (unsupported, _, _) = recovery_app_with(
            MonitorIdentity::Verified(TARGET_ID),
            ManagedWindowPersistence::RememberAll,
            WindowRecovery::FallbackAndReturn,
            CapturedWindowPosition::CompositorControlled,
            SavedWindowMode::Windowed,
            Platform::Wayland,
        );
        assert_eq!(
            unsupported
                .world()
                .resource::<RecoveryRegistrations>()
                .registered()
                .count(),
            0
        );

        let (supported, _, _) = recovery_app_with(
            MonitorIdentity::Verified(TARGET_ID),
            ManagedWindowPersistence::RememberAll,
            WindowRecovery::FallbackAndReturn,
            CapturedWindowPosition::CompositorControlled,
            SavedWindowMode::BorderlessFullscreen,
            Platform::Wayland,
        );
        assert_eq!(
            supported
                .world()
                .resource::<RecoveryRegistrations>()
                .registered()
                .count(),
            1
        );

        let (unverified, _, _) = recovery_app_with(
            MonitorIdentity::Unverified,
            ManagedWindowPersistence::RememberAll,
            WindowRecovery::FallbackAndReturn,
            CapturedWindowPosition::Restorable {
                logical_offset: IVec2::ZERO,
            },
            SavedWindowMode::Windowed,
            Platform::Windows,
        );
        assert_eq!(
            unverified
                .world()
                .resource::<RecoveryRegistrations>()
                .registered()
                .count(),
            0
        );
    }

    #[test]
    fn identity_only_loss_and_return_emit_one_fact_each() {
        let (mut app, entity, monitor_entity) = recovery_app(
            MonitorIdentity::Verified(TARGET_ID),
            ManagedWindowPersistence::RememberAll,
        );
        install_topology(
            &mut app,
            1,
            [(monitor_entity, monitor_info(MonitorIdentity::Unverified, 0))],
        );
        app.update();
        let original = placement(monitor_info(MonitorIdentity::Verified(TARGET_ID), 0));
        let fallback = placement(monitor_info(MonitorIdentity::Unverified, 0));
        app.world_mut()
            .resource_mut::<CapturedWindowStates>()
            .capture(WindowKey::Primary, entity, fallback);
        assert_eq!(
            app.world()
                .resource::<CapturedWindowStates>()
                .captured_placement(&WindowKey::Primary),
            Some(&original)
        );
        app.update();

        let returned_monitor = monitor_info(MonitorIdentity::Verified(TARGET_ID), 2);
        install_topology(&mut app, 2, [(monitor_entity, returned_monitor)]);
        app.update();
        app.update();

        let facts = app.world().resource::<RecoveryFacts>();
        assert_eq!(facts.pending, vec![(WindowKey::Primary, TARGET_ID)]);
        assert_eq!(
            facts.available,
            vec![(WindowKey::Primary, returned_monitor)]
        );
    }

    #[test]
    fn coalesced_replacement_revision_with_same_identity_emits_no_lifecycle_fact() {
        let (mut app, _, _) = recovery_app(
            MonitorIdentity::Verified(TARGET_ID),
            ManagedWindowPersistence::RememberAll,
        );
        let replacement_entity = app.world_mut().spawn_empty().id();
        install_topology(
            &mut app,
            1,
            [(
                replacement_entity,
                monitor_info(MonitorIdentity::Verified(TARGET_ID), 1),
            )],
        );
        app.update();

        let facts = app.world().resource::<RecoveryFacts>();
        assert!(facts.pending.is_empty());
        assert!(facts.available.is_empty());
    }

    #[test]
    fn linked_window_despawn_stays_frozen_until_absent_topology_is_classified() {
        for persistence in [
            ManagedWindowPersistence::RememberAll,
            ManagedWindowPersistence::ActiveOnly,
        ] {
            let (mut app, primary_entity, monitor_entity) =
                recovery_app(MonitorIdentity::Verified(TARGET_ID), persistence);
            let managed_key = WindowKey::Managed("secondary".to_string());
            let installed_monitor = monitor_info(MonitorIdentity::Verified(TARGET_ID), 0);
            let managed_entity = app
                .world_mut()
                .spawn((
                    Window::default(),
                    ManagedWindow {
                        name: "secondary".to_string(),
                    },
                    OnMonitor(monitor_entity),
                    CurrentMonitor {
                        monitor_info:          installed_monitor,
                        effective_window_mode: WindowMode::Windowed,
                    },
                    NativeWindowReady,
                ))
                .id();
            app.world_mut()
                .resource_mut::<CapturedWindowStates>()
                .promote(
                    managed_key.clone(),
                    managed_entity,
                    placement(installed_monitor),
                );
            app.world_mut()
                .entity_mut(managed_entity)
                .insert(WindowRecovery::ApplicationControlled);
            app.world_mut().flush();
            app.update();
            assert_eq!(
                app.world()
                    .resource::<RecoveryRegistrations>()
                    .registered()
                    .count(),
                2
            );

            assert!(app.world_mut().despawn(primary_entity));
            assert!(app.world_mut().despawn(managed_entity));
            app.world_mut().flush();

            for window_key in [&WindowKey::Primary, &managed_key] {
                let entry = app
                    .world()
                    .resource::<CapturedWindowStates>()
                    .entry(window_key);
                assert!(entry.is_some());
                assert_eq!(entry.and_then(|entry| entry.live), None);
                assert_eq!(
                    entry.map(|entry| entry.persistence),
                    Some(crate::persistence::PersistenceWriteState::Frozen)
                );
                assert_eq!(
                    app.world()
                        .resource::<CapturedWindowStates>()
                        .captured_placement(window_key),
                    Some(&placement(installed_monitor))
                );
            }
            assert!(
                app.world()
                    .resource::<ManagedWindowRegistry>()
                    .entities
                    .is_empty()
            );
            let registrations = app.world().resource::<RecoveryRegistrations>();
            assert_eq!(registrations.registered().count(), 2);
            assert!(
                registrations
                    .registered()
                    .all(|registration| registration.entity.is_none())
            );

            install_topology(&mut app, 1, []);
            app.update();
            let pending = &app.world().resource::<RecoveryFacts>().pending;
            assert_eq!(pending.len(), 2);
            assert!(pending.contains(&(WindowKey::Primary, TARGET_ID)));
            assert!(pending.contains(&(managed_key, TARGET_ID)));
        }
    }

    #[test]
    fn absent_cancellation_applies_each_persistence_mode_with_zero_monitors() {
        for persistence in [
            ManagedWindowPersistence::RememberAll,
            ManagedWindowPersistence::ActiveOnly,
        ] {
            let (mut app, entity, _) =
                recovery_app(MonitorIdentity::Verified(TARGET_ID), persistence.clone());
            app.world_mut().entity_mut(entity).remove::<Window>();
            app.world_mut().flush();
            install_topology(&mut app, 1, []);
            app.update();

            app.world_mut().trigger(crate::CancelWindowRecovery {
                window: WindowKey::Primary,
            });

            let states = app.world().resource::<CapturedWindowStates>();
            match persistence {
                ManagedWindowPersistence::RememberAll => {
                    assert!(states.persisted(&WindowKey::Primary).is_some());
                },
                ManagedWindowPersistence::ActiveOnly => {
                    assert!(states.entry(&WindowKey::Primary).is_none());
                },
            }
            assert_eq!(
                app.world()
                    .resource::<RecoveryRegistrations>()
                    .registered()
                    .count(),
                0
            );
        }
    }

    #[test]
    fn close_intent_wins_topology_loss_while_declined_close_does_not() {
        let (mut close_app, entity, _) = recovery_app(
            MonitorIdentity::Verified(TARGET_ID),
            ManagedWindowPersistence::RememberAll,
        );
        close_app
            .world_mut()
            .entity_mut(entity)
            .insert(ClosingWindow);
        install_topology(&mut close_app, 1, []);
        close_app.update();
        assert!(
            close_app
                .world()
                .resource::<RecoveryFacts>()
                .pending
                .is_empty()
        );

        let (mut declined_app, _, _) = recovery_app(
            MonitorIdentity::Verified(TARGET_ID),
            ManagedWindowPersistence::RememberAll,
        );
        install_topology(&mut declined_app, 1, []);
        declined_app.update();
        assert_eq!(
            declined_app.world().resource::<RecoveryFacts>().pending,
            vec![(WindowKey::Primary, TARGET_ID)]
        );
    }

    #[test]
    fn target_absence_survives_entity_removal_and_emits_one_available_fact_on_return() {
        let (mut app, entity, _) = recovery_app(
            MonitorIdentity::Verified(TARGET_ID),
            ManagedWindowPersistence::RememberAll,
        );
        install_topology(&mut app, 1, []);
        app.update();
        assert_eq!(
            app.world().resource::<RecoveryFacts>().pending,
            vec![(WindowKey::Primary, TARGET_ID)]
        );

        assert!(app.world_mut().despawn(entity));
        app.world_mut().flush();
        let returned_entity = app.world_mut().spawn_empty().id();
        let returned_monitor = monitor_info(MonitorIdentity::Verified(TARGET_ID), 1);
        install_topology(&mut app, 2, [(returned_entity, returned_monitor)]);
        app.update();
        app.update();

        let facts = app.world().resource::<RecoveryFacts>();
        assert_eq!(
            facts.available,
            vec![(WindowKey::Primary, returned_monitor)]
        );
    }
}
