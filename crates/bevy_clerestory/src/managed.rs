//! Managed window types, registry, and lifecycle observers.

use std::collections::HashMap;
use std::collections::HashSet;

use bevy::prelude::*;
use bevy::window::OnMonitor;
use bevy::window::PrimaryWindow;

use super::WindowKey;
use super::constants::FIRST_DUPLICATE_SUFFIX;
use super::constants::MANAGED_WINDOW_NAME_SEPARATOR;
use super::constants::PRIMARY_WINDOW_KEY;
use super::monitors;
use super::monitors::Monitors;
use super::persistence::CapturedWindowStates;
use super::platform::Platform;
use super::recovery::CanonicalWindowRole;
use super::recovery::RecoveryRegistrations;
use super::recovery::WindowRecovery;
use super::restore::NativeWindowReady;
use super::restore::RestorePreparation;

/// Marks a window entity as managed by the window manager plugin.
///
/// Add this component to any secondary window entity to opt into automatic
/// save/restore behavior. The primary window is always managed automatically
/// using the key `"primary"` in the state file.
///
/// Each managed window must have a unique `name`. Duplicate names
/// will cause a panic.
///
/// # Example
///
/// ```ignore
/// commands.spawn((
///     Window { title: "Inspector".into(), ..default() },
///     ManagedWindow { name: "inspector".into() },
/// ));
/// ```
#[derive(Component, Clone, Reflect)]
#[reflect(Component)]
pub struct ManagedWindow {
    /// Unique name used as the key in the state file.
    pub name: String,
}

/// Controls what happens to saved state when a managed window is despawned.
///
/// Set as a resource on the app to control persistence behavior for all windows.
#[derive(Resource, Default, Clone, Debug, PartialEq, Eq, Reflect)]
#[reflect(Resource)]
pub enum ManagedWindowPersistence {
    /// Default: saved state persists even if window is closed during the session.
    /// All windows ever opened are remembered in the state file.
    #[default]
    RememberAll,
    /// Only windows open at time of save are persisted.
    /// Closing a window removes its entry from the state file.
    ActiveOnly,
}

/// Internal registry to track managed window names and detect duplicates.
#[derive(Resource, Default)]
pub(crate) struct ManagedWindowRegistry {
    /// Set of registered window names (for duplicate detection).
    pub(crate) names:    HashSet<String>,
    /// Map from entity to window name (for cleanup on removal).
    pub(crate) entities: HashMap<Entity, String>,
}

impl ManagedWindowRegistry {
    #[must_use]
    pub(crate) fn name(&self, entity: Entity) -> Option<&str> {
        self.entities.get(&entity).map(String::as_str)
    }
}

/// Register and deduplicate a `ManagedWindow` name.
pub(crate) fn on_managed_window_added(
    add: On<Add, ManagedWindow>,
    mut managed: Query<&mut ManagedWindow>,
    mut managed_window_registry: ResMut<ManagedWindowRegistry>,
    primary_query: Query<(), With<PrimaryWindow>>,
) {
    let entity = add.entity;
    let Ok(mut managed_window) = managed.get_mut(entity) else {
        return;
    };
    let name = managed_window.name.clone();

    // Primary window is managed automatically — reject explicit `ManagedWindow` on it
    if primary_query.get(entity).is_ok() {
        warn!(
            "[on_managed_window_added] `ManagedWindow` cannot be added to the primary window (entity {entity:?}). \
             The primary window is managed automatically under the key \"{key}\".",
            key = PRIMARY_WINDOW_KEY,
        );
        return;
    }

    let unique_name = if managed_window_registry.names.contains(&name) {
        debug_assert!(false, "Duplicate ManagedWindow name: \"{name}\"");
        let mut suffix = FIRST_DUPLICATE_SUFFIX;
        loop {
            let candidate = format!("{name}{MANAGED_WINDOW_NAME_SEPARATOR}{suffix}");
            if !managed_window_registry.names.contains(&candidate) {
                break candidate;
            }
            suffix += 1;
        }
    } else {
        name.clone()
    };

    if unique_name != name {
        warn!(
            "[on_managed_window_added] Duplicate ManagedWindow name: \"{name}\" — renamed to \"{unique_name}\" for entity {entity:?}"
        );
        managed_window.name.clone_from(&unique_name);
    }

    managed_window_registry.names.insert(unique_name.clone());
    managed_window_registry
        .entities
        .insert(entity, unique_name.clone());
    debug!(
        "[on_managed_window_added] Registered managed window \"{unique_name}\" on entity {entity:?}"
    );
}

/// Observer: unregister a `ManagedWindow` name when removed, and update state file if `ActiveOnly`.
pub(crate) fn on_managed_window_removed(
    remove: On<Remove, ManagedWindow>,
    mut commands: Commands,
    mut managed_window_registry: ResMut<ManagedWindowRegistry>,
    managed_window_persistence: Res<ManagedWindowPersistence>,
    mut captured_window_states: ResMut<CapturedWindowStates>,
    primary_windows: Query<(), With<PrimaryWindow>>,
) {
    let entity = remove.entity;
    if !primary_windows.contains(entity) {
        commands
            .entity(entity)
            .try_remove::<(monitors::CurrentMonitor, NativeWindowReady)>();
    }
    if let Some(name) = managed_window_registry.entities.remove(&entity) {
        let window_key = WindowKey::Managed(name.clone());
        captured_window_states.deactivate(&window_key, entity, &managed_window_persistence);
        captured_window_states.apply_policy(&managed_window_persistence);

        managed_window_registry.names.remove(&name);
        debug!(
            "[on_managed_window_removed] Unregistered managed window \"{name}\" from entity {entity:?}"
        );
    }
}

/// Apply the current retention policy to captured entries.
pub(crate) fn on_persistence_changed(
    managed_window_persistence: Res<ManagedWindowPersistence>,
    mut captured_window_states: ResMut<CapturedWindowStates>,
) {
    captured_window_states.apply_policy(&managed_window_persistence);
}

/// Queue managed startup restoration after checking canonical ownership.
pub(crate) fn on_managed_window_load(
    add: On<Add, ManagedWindow>,
    mut commands: Commands,
    managed: Query<&ManagedWindow>,
    mut captured_window_states: ResMut<CapturedWindowStates>,
    mut windows: Query<(&mut Window, Option<&OnMonitor>)>,
    monitors: Res<Monitors>,
    platform: Res<Platform>,
    recovery_registrations: Option<Res<RecoveryRegistrations>>,
) {
    let entity = add.entity;
    let Ok(managed_window) = managed.get(entity) else {
        return;
    };
    let name = &managed_window.name;
    let window_key = WindowKey::Managed((*name).clone());
    let Ok((mut window, on_monitor)) = windows.get_mut(entity) else {
        return;
    };

    let current_monitor = on_monitor.and_then(|on_monitor| {
        monitors::current_monitor_from_association(&window, on_monitor, &monitors)
    });
    let mut entity_commands = commands.entity(entity);
    if let Some(current_monitor) = current_monitor {
        entity_commands.insert((current_monitor, NativeWindowReady));
    } else {
        entity_commands.remove::<(monitors::CurrentMonitor, NativeWindowReady)>();
    }

    if captured_window_states.is_bound_to(&window_key, entity) {
        debug!(
            "[on_managed_window_load] Bypassing automatic startup restore for canonically bound window \"{name}\""
        );
        return;
    }

    let is_application_controlled_replacement = recovery_registrations
        .as_deref()
        .and_then(|registrations| registrations.by_key(&window_key))
        .is_some_and(|registration| {
            registration.policy == WindowRecovery::ApplicationControlled
                && registration.role == CanonicalWindowRole::Managed
                && registration.entity.is_none()
        });
    if is_application_controlled_replacement {
        debug!(
            "[on_managed_window_load] Deferring managed replacement \"{name}\" to explicit restore acceptance"
        );
        return;
    }

    // `Platform::should_hide_on_startup` keeps Linux X11 windows visible for frame extents.
    if platform.should_hide_on_startup() {
        window.visible = false;
    }

    if !captured_window_states.bind_and_freeze(&window_key, entity) {
        debug!("[on_managed_window_load] No saved state for \"{name}\", showing window");
        window.visible = true;
        return;
    }

    commands
        .entity(entity)
        .insert(RestorePreparation::startup(window_key));
}

#[cfg(test)]
mod tests {
    use bevy::window::WindowMode;

    use super::*;
    use crate::monitors;
    use crate::monitors::CurrentMonitor;
    use crate::monitors::InjectedCurrentMonitorSource;
    use crate::monitors::MonitorId;
    use crate::monitors::MonitorIdentity;
    use crate::monitors::MonitorInfo;
    use crate::monitors::Monitors;
    use crate::monitors::NativeQueryActivity;
    use crate::persistence::CapturedWindowPlacement;
    use crate::persistence::CapturedWindowPosition;
    use crate::persistence::PersistedWindowState;
    use crate::persistence::SavedWindowMode;
    use crate::restore;
    use crate::restore::TargetPosition;
    use crate::restore::WinitInfo;

    const CAPTURED_OFFSET: IVec2 = IVec2::new(100, 50);
    const TARGET_ID: MonitorId = MonitorId::from_test_raw(7);
    const ABSENT_ID: MonitorId = MonitorId::from_test_raw(8);

    #[derive(Default, Resource)]
    struct RestoreQueueActivity {
        additions: usize,
    }

    fn count_restore_queue(
        _added: On<Add, RestorePreparation>,
        mut activity: ResMut<RestoreQueueActivity>,
    ) {
        activity.additions += 1;
    }

    fn persisted() -> PersistedWindowState {
        PersistedWindowState {
            logical_position:  Some((10, 20)),
            logical_width:     800,
            logical_height:    600,
            scale:             1.0,
            monitor:           0,
            saved_window_mode: SavedWindowMode::Windowed,
            app_name:          "test".to_string(),
        }
    }

    fn managed_removal_app(
        managed_window_persistence: ManagedWindowPersistence,
    ) -> (App, Entity, WindowKey) {
        let mut app = App::new();
        app.insert_resource(managed_window_persistence)
            .init_resource::<ManagedWindowRegistry>()
            .init_resource::<CapturedWindowStates>()
            .add_observer(on_managed_window_removed);
        let name = "secondary".to_string();
        let window_key = WindowKey::Managed(name.clone());
        let entity = app
            .world_mut()
            .spawn((
                Window::default(),
                CurrentMonitor {
                    monitor_info:          monitor(
                        MonitorIdentity::Unverified,
                        0,
                        1.0,
                        IVec2::ZERO,
                    ),
                    effective_window_mode: WindowMode::Windowed,
                },
                NativeWindowReady,
                ManagedWindow { name: name.clone() },
            ))
            .id();
        {
            let mut registry = app.world_mut().resource_mut::<ManagedWindowRegistry>();
            registry.names.insert(name.clone());
            registry.entities.insert(entity, name);
        }
        {
            let mut states = app.world_mut().resource_mut::<CapturedWindowStates>();
            states.seed(HashMap::from([(window_key.clone(), persisted())]));
            states.bind(&window_key, entity);
        }
        (app, entity, window_key)
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

    fn captured_placement(
        identity: MonitorIdentity,
        index: usize,
        scale: f64,
        physical_position: IVec2,
        position: CapturedWindowPosition,
    ) -> CapturedWindowPlacement {
        CapturedWindowPlacement {
            monitor_snapshot: monitor(identity, index, scale, physical_position),
            position,
            logical_size: UVec2::new(800, 600),
            saved_window_mode: SavedWindowMode::Windowed,
            captured_scale: scale,
        }
    }

    fn prepare_captured_managed_window(
        monitors: Monitors,
        captured_placement: CapturedWindowPlacement,
    ) -> (App, Entity) {
        let starting_monitor = *monitors.first();
        let mut app = App::new();
        let starting_monitor_entity = app.world_mut().spawn_empty().id();
        let installed_monitors = std::iter::once((starting_monitor_entity, starting_monitor))
            .chain(monitors.iter().skip(1).map(|monitor| {
                let entity = app.world_mut().spawn_empty().id();
                (entity, *monitor.monitor_info)
            }));
        let monitors = Monitors::from_test_monitors(installed_monitors);
        app.insert_resource(monitors)
            .insert_resource(WinitInfo::default())
            .insert_resource(Platform::Windows)
            .init_resource::<CapturedWindowStates>()
            .add_observer(on_managed_window_load)
            .add_systems(Update, restore::prepare_restore_targets);
        let previous_entity = app.world_mut().spawn_empty().id();
        app.world_mut()
            .resource_mut::<CapturedWindowStates>()
            .promote(
                WindowKey::Managed("secondary".to_string()),
                previous_entity,
                captured_placement,
            );

        let entity = app
            .world_mut()
            .spawn((
                Window::default(),
                CurrentMonitor {
                    monitor_info:          starting_monitor,
                    effective_window_mode: WindowMode::Windowed,
                },
                OnMonitor(starting_monitor_entity),
                NativeWindowReady,
                ManagedWindow {
                    name: "secondary".to_string(),
                },
            ))
            .id();
        app.world_mut().flush();
        app.update();
        (app, entity)
    }

    #[test]
    fn captured_verified_reopen_prepares_returned_identity_at_its_current_index() {
        let returned_origin = IVec2::new(2_000, -200);
        let monitors = Monitors::from_test_monitors([
            (
                Entity::from_bits(1),
                monitor(MonitorIdentity::Unverified, 0, 1.0, IVec2::ZERO),
            ),
            (
                Entity::from_bits(2),
                monitor(MonitorIdentity::Unverified, 1, 1.0, IVec2::new(-1_920, 0)),
            ),
            (
                Entity::from_bits(3),
                monitor(
                    MonitorIdentity::Verified(TARGET_ID),
                    2,
                    2.0,
                    returned_origin,
                ),
            ),
        ]);
        let captured = captured_placement(
            MonitorIdentity::Verified(TARGET_ID),
            1,
            1.0,
            IVec2::new(-1_920, 0),
            CapturedWindowPosition::Restorable {
                logical_offset: CAPTURED_OFFSET,
            },
        );

        let (app, entity) = prepare_captured_managed_window(monitors, captured);
        let target = app.world().get::<TargetPosition>(entity);

        assert_eq!(target.map(|target| target.monitor_index), Some(2));
        assert_eq!(target.map(|target| target.target_scale), Some(2.0));
        assert_eq!(
            target.and_then(|target| target.physical_position),
            Some(returned_origin + CAPTURED_OFFSET * 2)
        );
        assert_eq!(
            target.map(|target| target.physical_size),
            Some(UVec2::new(1_600, 1_200))
        );
    }

    #[test]
    fn captured_verified_reopen_falls_back_when_identity_is_absent() {
        let monitors = Monitors::from_test_monitors([
            (
                Entity::from_bits(1),
                monitor(MonitorIdentity::Unverified, 0, 2.0, IVec2::ZERO),
            ),
            (
                Entity::from_bits(2),
                monitor(MonitorIdentity::Unverified, 1, 1.0, IVec2::new(-1_920, 0)),
            ),
        ]);
        let captured = captured_placement(
            MonitorIdentity::Verified(ABSENT_ID),
            1,
            1.0,
            IVec2::new(-1_920, 0),
            CapturedWindowPosition::Restorable {
                logical_offset: CAPTURED_OFFSET,
            },
        );

        let (app, entity) = prepare_captured_managed_window(monitors, captured);
        let target = app.world().get::<TargetPosition>(entity);

        assert_eq!(target.map(|target| target.monitor_index), Some(0));
        assert_eq!(target.map(|target| target.target_scale), Some(2.0));
        assert_eq!(target.and_then(|target| target.physical_position), None);
    }

    #[test]
    fn captured_unverified_reopen_retains_index_based_targeting() {
        let indexed_origin = IVec2::new(-2_560, 300);
        let monitors = Monitors::from_test_monitors([
            (
                Entity::from_bits(1),
                monitor(MonitorIdentity::Unverified, 0, 1.0, IVec2::ZERO),
            ),
            (
                Entity::from_bits(2),
                monitor(MonitorIdentity::Unverified, 1, 2.0, indexed_origin),
            ),
        ]);
        let captured = captured_placement(
            MonitorIdentity::Unverified,
            1,
            1.0,
            IVec2::new(-1_920, 0),
            CapturedWindowPosition::Restorable {
                logical_offset: CAPTURED_OFFSET,
            },
        );

        let (app, entity) = prepare_captured_managed_window(monitors, captured);
        let target = app.world().get::<TargetPosition>(entity);

        assert_eq!(target.map(|target| target.monitor_index), Some(1));
        assert_eq!(
            target.and_then(|target| target.physical_position),
            Some(indexed_origin + CAPTURED_OFFSET * 2)
        );
    }

    #[test]
    fn captured_unverified_reopen_falls_back_without_retained_coordinate() {
        let monitors = Monitors::from_test_monitors([(
            Entity::from_bits(1),
            monitor(MonitorIdentity::Unverified, 0, 2.0, IVec2::ZERO),
        )]);
        let captured = captured_placement(
            MonitorIdentity::Unverified,
            1,
            1.0,
            IVec2::new(-1_920, 0),
            CapturedWindowPosition::Restorable {
                logical_offset: CAPTURED_OFFSET,
            },
        );

        let (app, entity) = prepare_captured_managed_window(monitors, captured);
        let target = app.world().get::<TargetPosition>(entity);

        assert_eq!(target.map(|target| target.monitor_index), Some(0));
        assert_eq!(target.map(|target| target.target_scale), Some(2.0));
        assert_eq!(target.and_then(|target| target.physical_position), None);
    }

    #[test]
    fn captured_compositor_controlled_reopen_has_no_coordinate() {
        let monitors = Monitors::from_test_monitors([
            (
                Entity::from_bits(1),
                monitor(MonitorIdentity::Unverified, 0, 1.0, IVec2::ZERO),
            ),
            (
                Entity::from_bits(2),
                monitor(
                    MonitorIdentity::Verified(TARGET_ID),
                    2,
                    2.0,
                    IVec2::new(2_000, -200),
                ),
            ),
        ]);
        let captured = captured_placement(
            MonitorIdentity::Verified(TARGET_ID),
            1,
            1.0,
            IVec2::new(-1_920, 0),
            CapturedWindowPosition::CompositorControlled,
        );

        let (app, entity) = prepare_captured_managed_window(monitors, captured);
        let target = app.world().get::<TargetPosition>(entity);

        assert_eq!(target.map(|target| target.monitor_index), Some(2));
        assert_eq!(target.and_then(|target| target.physical_position), None);
    }

    #[test]
    fn canonically_bound_replacement_bypasses_automatic_startup_restore() {
        let name = "secondary".to_string();
        let window_key = WindowKey::Managed(name.clone());
        let mut app = App::new();
        app.insert_resource(Platform::Windows)
            .init_resource::<CapturedWindowStates>()
            .add_observer(on_managed_window_load);
        let monitor_entity = app.world_mut().spawn_empty().id();
        app.insert_resource(Monitors::from_test_monitors([(
            monitor_entity,
            monitor(MonitorIdentity::Unverified, 0, 1.0, IVec2::ZERO),
        )]));
        let entity = app
            .world_mut()
            .spawn((Window::default(), OnMonitor(monitor_entity)))
            .id();
        {
            let mut captured_window_states = app.world_mut().resource_mut::<CapturedWindowStates>();
            captured_window_states.seed(HashMap::from([(window_key.clone(), persisted())]));
            captured_window_states.bind(&window_key, entity);
        }

        app.world_mut()
            .entity_mut(entity)
            .insert(ManagedWindow { name });
        app.world_mut().flush();

        assert!(app.world().get::<RestorePreparation>(entity).is_none());
        assert!(app.world().get::<NativeWindowReady>(entity).is_some());
        assert_eq!(
            app.world()
                .get::<Window>(entity)
                .map(|window| window.visible),
            Some(true)
        );
    }

    #[test]
    fn late_managed_opt_in_uses_existing_monitor_association_without_native_queries() {
        let name = "secondary".to_string();
        let window_key = WindowKey::Managed(name.clone());
        let mut app = App::new();
        let primary_monitor_entity = app.world_mut().spawn_empty().id();
        let associated_monitor_entity = app.world_mut().spawn_empty().id();
        let associated_monitor = monitor(
            MonitorIdentity::Verified(TARGET_ID),
            1,
            2.0,
            IVec2::new(2_000, -200),
        );
        app.insert_resource(Monitors::from_test_monitors([
            (
                primary_monitor_entity,
                monitor(MonitorIdentity::Unverified, 0, 1.0, IVec2::ZERO),
            ),
            (associated_monitor_entity, associated_monitor),
        ]))
        .insert_resource(WinitInfo::default())
        .insert_resource(Platform::Windows)
        .init_resource::<CapturedWindowStates>()
        .init_resource::<InjectedCurrentMonitorSource>()
        .init_resource::<RestoreQueueActivity>()
        .add_observer(monitors::install_current_monitor_from_association)
        .add_observer(on_managed_window_load)
        .add_observer(count_restore_queue)
        .add_systems(
            Update,
            (
                monitors::update_current_monitor,
                restore::prepare_restore_targets,
            )
                .chain(),
        );
        let entity = app
            .world_mut()
            .spawn((Window::default(), OnMonitor(associated_monitor_entity)))
            .id();
        app.world_mut().flush();
        assert!(app.world().get::<CurrentMonitor>(entity).is_none());

        let previous_entity = app.world_mut().spawn_empty().id();
        app.world_mut()
            .resource_mut::<CapturedWindowStates>()
            .promote(
                window_key,
                previous_entity,
                captured_placement(
                    MonitorIdentity::Verified(TARGET_ID),
                    1,
                    1.0,
                    IVec2::new(-1_920, 0),
                    CapturedWindowPosition::Restorable {
                        logical_offset: CAPTURED_OFFSET,
                    },
                ),
            );

        app.world_mut()
            .entity_mut(entity)
            .insert(ManagedWindow { name });
        app.world_mut().flush();

        let current_monitor = app.world().get::<CurrentMonitor>(entity);
        assert_eq!(
            current_monitor.map(|current_monitor| current_monitor.monitor_info),
            Some(associated_monitor)
        );
        assert!(app.world().get::<NativeWindowReady>(entity).is_some());
        assert!(app.world().get::<RestorePreparation>(entity).is_some());
        assert_eq!(app.world().resource::<RestoreQueueActivity>().additions, 1);

        app.update();
        app.update();

        assert!(app.world().get::<TargetPosition>(entity).is_some());
        assert_eq!(app.world().resource::<RestoreQueueActivity>().additions, 1);
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
    fn late_managed_opt_in_rejects_stale_unresolved_association() {
        let name = "secondary".to_string();
        let window_key = WindowKey::Managed(name.clone());
        let mut app = App::new();
        let installed_monitor_entity = app.world_mut().spawn_empty().id();
        let unresolved_monitor_entity = app.world_mut().spawn_empty().id();
        let installed_monitor = monitor(MonitorIdentity::Unverified, 0, 1.0, IVec2::ZERO);
        app.insert_resource(Monitors::from_test_monitors([(
            installed_monitor_entity,
            installed_monitor,
        )]))
        .insert_resource(Platform::Windows)
        .init_resource::<CapturedWindowStates>()
        .add_observer(on_managed_window_load);
        let previous_entity = app.world_mut().spawn_empty().id();
        app.world_mut()
            .resource_mut::<CapturedWindowStates>()
            .promote(
                window_key,
                previous_entity,
                captured_placement(
                    MonitorIdentity::Unverified,
                    0,
                    1.0,
                    IVec2::ZERO,
                    CapturedWindowPosition::Restorable {
                        logical_offset: CAPTURED_OFFSET,
                    },
                ),
            );
        let entity = app
            .world_mut()
            .spawn((
                Window::default(),
                OnMonitor(unresolved_monitor_entity),
                CurrentMonitor {
                    monitor_info:          installed_monitor,
                    effective_window_mode: WindowMode::Windowed,
                },
                NativeWindowReady,
            ))
            .id();

        app.world_mut()
            .entity_mut(entity)
            .insert(ManagedWindow { name });
        app.world_mut().flush();

        assert!(app.world().get::<RestorePreparation>(entity).is_some());
        assert!(app.world().get::<NativeWindowReady>(entity).is_none());
        assert!(app.world().get::<CurrentMonitor>(entity).is_none());
    }

    #[test]
    fn managed_startup_protects_saved_placement_when_restore_is_queued() {
        let name = "secondary".to_string();
        let window_key = WindowKey::Managed(name.clone());
        let original_placement = captured_placement(
            MonitorIdentity::Unverified,
            0,
            1.0,
            IVec2::ZERO,
            CapturedWindowPosition::Restorable {
                logical_offset: IVec2::new(10, 20),
            },
        );
        let mut app = App::new();
        app.insert_resource(Monitors::from_test_monitors([(
            Entity::from_bits(1),
            monitor(MonitorIdentity::Unverified, 0, 1.0, IVec2::ZERO),
        )]))
        .insert_resource(Platform::Windows)
        .init_resource::<CapturedWindowStates>()
        .add_observer(on_managed_window_load);
        let previous_entity = app.world_mut().spawn_empty().id();
        let entity = app.world_mut().spawn(Window::default()).id();
        app.world_mut()
            .resource_mut::<CapturedWindowStates>()
            .promote(
                window_key.clone(),
                previous_entity,
                original_placement.clone(),
            );
        app.world_mut()
            .entity_mut(entity)
            .insert(ManagedWindow { name });
        app.world_mut().flush();

        assert!(app.world().get::<RestorePreparation>(entity).is_some());
        let mut captured_window_states = app.world_mut().resource_mut::<CapturedWindowStates>();
        assert!(captured_window_states.is_bound_to(&window_key, entity));
        captured_window_states.apply_policy(&ManagedWindowPersistence::ActiveOnly);
        assert!(captured_window_states.is_bound_to(&window_key, entity));
        assert!(captured_window_states.entry(&window_key).is_some());
        captured_window_states.capture(
            window_key.clone(),
            entity,
            captured_placement(
                MonitorIdentity::Unverified,
                0,
                1.0,
                IVec2::ZERO,
                CapturedWindowPosition::CompositorControlled,
            ),
        );
        assert_eq!(
            captured_window_states.captured_placement(&window_key),
            Some(&original_placement)
        );
    }

    #[test]
    fn managed_removal_remembers_absent_state_under_remember_all() {
        let (mut app, entity, window_key) =
            managed_removal_app(ManagedWindowPersistence::RememberAll);

        app.world_mut().entity_mut(entity).remove::<ManagedWindow>();
        app.world_mut().flush();

        let entry = app
            .world()
            .resource::<CapturedWindowStates>()
            .entry(&window_key);
        assert!(entry.is_some());
        assert_eq!(entry.and_then(|entry| entry.live), None);
        assert!(app.world().get::<NativeWindowReady>(entity).is_none());
        assert!(app.world().get::<CurrentMonitor>(entity).is_none());
    }

    #[test]
    fn managed_removal_prunes_active_only_without_unbinding_primary() {
        let (mut app, entity, managed_key) =
            managed_removal_app(ManagedWindowPersistence::ActiveOnly);
        {
            let mut states = app.world_mut().resource_mut::<CapturedWindowStates>();
            states.promote(
                WindowKey::Primary,
                entity,
                CapturedWindowPlacement {
                    monitor_snapshot:  MonitorInfo {
                        identity:          MonitorIdentity::Unverified,
                        index:             0,
                        scale:             1.0,
                        physical_position: IVec2::ZERO,
                        physical_size:     UVec2::new(1_920, 1_080),
                    },
                    position:          CapturedWindowPosition::Restorable {
                        logical_offset: IVec2::new(10, 20),
                    },
                    logical_size:      UVec2::new(800, 600),
                    saved_window_mode: SavedWindowMode::Windowed,
                    captured_scale:    1.0,
                },
            );
        }
        app.world_mut().entity_mut(entity).insert(PrimaryWindow);

        app.world_mut().entity_mut(entity).remove::<ManagedWindow>();
        app.world_mut().flush();

        let states = app.world().resource::<CapturedWindowStates>();
        assert!(states.entry(&managed_key).is_none());
        assert_eq!(states.live_entity(&WindowKey::Primary), Some(entity));
        assert!(app.world().get::<NativeWindowReady>(entity).is_some());
        assert!(app.world().get::<CurrentMonitor>(entity).is_some());
    }
}
