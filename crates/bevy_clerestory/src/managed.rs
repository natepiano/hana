//! Managed window types, registry, and lifecycle observers.

use std::collections::HashMap;
use std::collections::HashSet;

use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use super::WindowKey;
use super::constants::DEFAULT_SCALE_FACTOR;
use super::constants::FIRST_DUPLICATE_SUFFIX;
use super::constants::MANAGED_WINDOW_NAME_SEPARATOR;
use super::constants::PRIMARY_MONITOR_INDEX;
use super::constants::PRIMARY_WINDOW_KEY;
use super::monitors::CurrentMonitor;
use super::monitors::Monitors;
use super::persistence::CapturedWindowPlacement;
use super::persistence::CapturedWindowStates;
use super::persistence::PersistedWindowState;
use super::platform::Platform;
use super::restore;
use super::restore::MonitorResolutionSource;
use super::restore::WinitInfo;
use super::restore::X11FrameCompensated;

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
    mut managed_window_registry: ResMut<ManagedWindowRegistry>,
    managed_window_persistence: Res<ManagedWindowPersistence>,
    mut captured_window_states: ResMut<CapturedWindowStates>,
) {
    let entity = remove.entity;
    if let Some(name) = managed_window_registry.entities.remove(&entity) {
        let window_key = WindowKey::Managed(name.clone());
        captured_window_states.unbind(&window_key, entity);
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

/// Observer: hide a managed window on creation and load its saved state.
pub(crate) fn on_managed_window_load(
    add: On<Add, ManagedWindow>,
    mut commands: Commands,
    managed: Query<&ManagedWindow>,
    monitors: Res<Monitors>,
    winit_info: Option<Res<WinitInfo>>,
    mut captured_window_states: ResMut<CapturedWindowStates>,
    mut windows: Query<&mut Window>,
    primary_monitor: Query<&CurrentMonitor, With<PrimaryWindow>>,
    platform: Res<Platform>,
) {
    let entity = add.entity;
    let Ok(managed_window) = managed.get(entity) else {
        return;
    };
    let name = &managed_window.name;

    // `Platform::should_hide_on_startup` keeps Linux X11 windows visible for frame extents.
    if let Ok(mut window) = windows.get_mut(entity)
        && platform.should_hide_on_startup()
    {
        window.visible = false;
    }

    let window_key = WindowKey::Managed((*name).clone());
    let captured_placement = captured_window_states
        .captured_placement(&window_key)
        .cloned();
    let Some(saved_state) = captured_window_states.restore_state(&window_key) else {
        debug!("[on_managed_window_load] No saved state for \"{name}\", showing window");
        if let Ok(mut window) = windows.get_mut(entity) {
            window.visible = true;
        }
        return;
    };
    captured_window_states.bind(&window_key, entity);
    captured_window_states.freeze(&window_key);

    debug!(
        "[on_managed_window_load] Loaded state for \"{name}\": position={:?} logical_size={}x{} monitor_scale={} monitor={} mode={:?}",
        saved_state.logical_position,
        saved_state.logical_width,
        saved_state.logical_height,
        saved_state.scale,
        saved_state.monitor,
        saved_state.saved_window_mode
    );

    let Some(winit_info) = winit_info else {
        debug!("[on_managed_window_load] WinitInfo not available, showing window for \"{name}\"");
        if let Ok(mut window) = windows.get_mut(entity) {
            window.visible = true;
        }
        return;
    };

    if monitors.is_empty() {
        debug!("[on_managed_window_load] No monitors available, showing window for \"{name}\"");
        if let Ok(mut window) = windows.get_mut(entity) {
            window.visible = true;
        }
        return;
    }

    // The window will be created on the focused window's monitor (the primary window's
    // monitor), so use that scale as starting_scale for scale factor compensation.
    let primary_scale = primary_monitor
        .iter()
        .next()
        .map_or(DEFAULT_SCALE_FACTOR, |current_monitor| {
            current_monitor.scale
        });

    restore_managed_window(
        entity,
        &saved_state,
        &monitors,
        &winit_info,
        &mut commands,
        primary_scale,
        *platform,
        captured_placement.as_ref(),
    );
}

/// Compute the target position for a managed window from saved state.
///
/// Inserts a `TargetPosition` component but does NOT modify `Window.position` or
/// `Window.resolution`. The actual restore is deferred to `restore_windows`, which
/// gates on the winit window existing (via `WINIT_WINDOWS`). This ensures
/// `create_windows` → `set_scale_factor_and_apply_to_physical_size()` runs first,
/// preventing the physical size from being doubled on high-DPI displays.
fn restore_managed_window(
    entity: Entity,
    saved_window_state: &PersistedWindowState,
    monitors: &Monitors,
    winit_info: &WinitInfo,
    commands: &mut Commands,
    primary_scale: f64,
    platform: Platform,
    captured_placement: Option<&CapturedWindowPlacement>,
) {
    let ManagedRestoreTarget {
        monitor_info,
        logical_position,
        monitor_resolution_source,
        prepared_position,
    } = resolve_managed_restore_target(saved_window_state, monitors, captured_placement);
    if matches!(
        monitor_resolution_source,
        restore::MonitorResolutionSource::FallbackToPrimary
    ) {
        warn!(
            "[restore_managed_window] Target monitor unavailable, falling back to monitor {PRIMARY_MONITOR_INDEX}",
        );
    }

    let physical_decoration = winit_info.physical_decoration();

    // The window is created on the focused window's monitor (the primary window's monitor)
    // without explicit positioning. Its starting scale matches the primary monitor, not the
    // target monitor.
    let mut target_position = restore::compute_target_position(
        saved_window_state,
        monitor_info,
        logical_position,
        physical_decoration,
        primary_scale,
        platform,
    );
    if let PreparedPosition::Retained(physical_position) = prepared_position {
        target_position.physical_position = physical_position;
    }

    debug!(
        "[restore_managed_window] saved_position={:?} clamped_position={:?} target_scale={} logical={}x{} physical={}x{} monitor={} monitor_position=({},{}) monitor_size=({},{})",
        saved_window_state.logical_position,
        target_position.physical_position,
        target_position.target_scale,
        target_position.logical_size.x,
        target_position.logical_size.y,
        target_position.physical_size.x,
        target_position.physical_size.y,
        target_position.monitor_index,
        monitor_info.physical_position.x,
        monitor_info.physical_position.y,
        monitor_info.physical_size.x,
        monitor_info.physical_size.y,
    );

    let is_fullscreen = saved_window_state.saved_window_mode.is_fullscreen();
    commands.entity(entity).insert(target_position);

    // Insert `X11FrameCompensated` for platforms that don't need compensation.
    // For fullscreen modes, skip frame compensation — frame extents are irrelevant
    // and delaying restore gives the compositor time to revert position changes.
    if is_fullscreen || !platform.needs_frame_compensation() {
        commands.entity(entity).insert(X11FrameCompensated);
    }
}

struct ManagedRestoreTarget<'a> {
    monitor_info:              &'a super::monitors::MonitorInfo,
    logical_position:          Option<(i32, i32)>,
    monitor_resolution_source: MonitorResolutionSource,
    prepared_position:         PreparedPosition,
}

enum PreparedPosition {
    Computed,
    Retained(Option<IVec2>),
}

fn resolve_managed_restore_target<'a>(
    saved_window_state: &PersistedWindowState,
    monitors: &'a Monitors,
    captured_placement: Option<&CapturedWindowPlacement>,
) -> ManagedRestoreTarget<'a> {
    let Some(captured_placement) = captured_placement else {
        let resolved_monitor = restore::resolve_target_monitor_and_position(
            saved_window_state.monitor,
            saved_window_state.logical_position,
            monitors,
        );
        return ManagedRestoreTarget {
            monitor_info:              resolved_monitor.monitor_info,
            logical_position:          resolved_monitor.logical_position,
            monitor_resolution_source: resolved_monitor.monitor_resolution_source,
            prepared_position:         PreparedPosition::Computed,
        };
    };

    match captured_placement.monitor_snapshot.identity {
        super::monitors::MonitorIdentity::Verified(id) => monitors.by_id(id).map_or_else(
            || ManagedRestoreTarget {
                monitor_info:              monitors.first(),
                logical_position:          None,
                monitor_resolution_source: MonitorResolutionSource::FallbackToPrimary,
                prepared_position:         PreparedPosition::Computed,
            },
            |monitor_info| ManagedRestoreTarget {
                monitor_info,
                logical_position: saved_window_state.logical_position,
                monitor_resolution_source: MonitorResolutionSource::Requested,
                prepared_position: PreparedPosition::Retained(
                    captured_placement.rebased_physical_position(monitor_info),
                ),
            },
        ),
        super::monitors::MonitorIdentity::Unverified => {
            let resolved_monitor = restore::resolve_target_monitor_and_position(
                saved_window_state.monitor,
                saved_window_state.logical_position,
                monitors,
            );
            let prepared_position = if matches!(
                &resolved_monitor.monitor_resolution_source,
                MonitorResolutionSource::Requested
            ) {
                PreparedPosition::Retained(
                    captured_placement.rebased_physical_position(resolved_monitor.monitor_info),
                )
            } else {
                PreparedPosition::Computed
            };
            ManagedRestoreTarget {
                monitor_info: resolved_monitor.monitor_info,
                logical_position: resolved_monitor.logical_position,
                monitor_resolution_source: resolved_monitor.monitor_resolution_source,
                prepared_position,
            }
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::monitors::MonitorId;
    use crate::monitors::MonitorIdentity;
    use crate::monitors::MonitorInfo;
    use crate::persistence::CapturedWindowPosition;
    use crate::persistence::SavedWindowMode;
    use crate::restore::TargetPosition;

    const CAPTURED_OFFSET: IVec2 = IVec2::new(100, 50);
    const TARGET_ID: MonitorId = MonitorId::from_test_raw(7);
    const ABSENT_ID: MonitorId = MonitorId::from_test_raw(8);

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
            .spawn((Window::default(), ManagedWindow { name: name.clone() }))
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
        let mut app = App::new();
        app.insert_resource(monitors)
            .insert_resource(WinitInfo::default())
            .insert_resource(Platform::Windows)
            .init_resource::<CapturedWindowStates>()
            .add_observer(on_managed_window_load);
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
                ManagedWindow {
                    name: "secondary".to_string(),
                },
            ))
            .id();
        app.world_mut().flush();
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
    }
}
