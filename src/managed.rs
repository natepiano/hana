//! Managed window types, registry, and lifecycle observers.

use std::collections::HashMap;
use std::collections::HashSet;

use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_kana::ToI32;
use bevy_kana::ToU32;

use super::WindowKey;
use super::config::RestoreWindowConfig;
use super::constants::DEFAULT_SCALE_FACTOR;
use super::constants::FIRST_DUPLICATE_SUFFIX;
use super::constants::MANAGED_WINDOW_NAME_SEPARATOR;
use super::constants::PRIMARY_MONITOR_INDEX;
use super::constants::PRIMARY_WINDOW_KEY;
use super::monitors::CurrentMonitor;
use super::monitors::Monitors;
use super::persistence;
use super::persistence::SavedWindowMode;
use super::persistence::WindowState;
use super::platform::Platform;
use super::restore;
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

/// Observer: register a `ManagedWindow` name, deduplicate if needed, and save initial state if
/// needed.
pub(crate) fn on_managed_window_added(
    add: On<Add, ManagedWindow>,
    mut managed: Query<&mut ManagedWindow>,
    mut registry: ResMut<ManagedWindowRegistry>,
    config: Res<RestoreWindowConfig>,
    monitors: Res<Monitors>,
    windows: Query<&Window>,
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

    let unique_name = if registry.names.contains(&name) {
        debug_assert!(false, "Duplicate ManagedWindow name: \"{name}\"");
        let mut suffix = FIRST_DUPLICATE_SUFFIX;
        loop {
            let candidate = format!("{name}{MANAGED_WINDOW_NAME_SEPARATOR}{suffix}");
            if !registry.names.contains(&candidate) {
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

    registry.names.insert(unique_name.clone());
    registry.entities.insert(entity, unique_name.clone());
    debug!(
        "[on_managed_window_added] Registered managed window \"{unique_name}\" on entity {entity:?}"
    );

    // If no saved state exists for this window, save its current position/size immediately
    let existing = persistence::load_all_states(&config.path);
    let already_saved = existing
        .as_ref()
        .is_some_and(|states| states.contains_key(&WindowKey::Managed(unique_name.clone())));

    if !already_saved && let Ok(window) = windows.get(entity) {
        let monitor_info = match window.position {
            WindowPosition::At(physical_position) => *monitors.monitor_for_window(
                physical_position,
                window.physical_width(),
                window.physical_height(),
            ),
            _ => *monitors.first(),
        };
        let logical_position = match window.position {
            WindowPosition::At(physical_position) => {
                let logical_x = (f64::from(physical_position.x) / monitor_info.scale)
                    .round()
                    .to_i32();
                let logical_y = (f64::from(physical_position.y) / monitor_info.scale)
                    .round()
                    .to_i32();
                Some((logical_x, logical_y))
            },
            _ => None,
        };
        let window_state = WindowState {
            logical_position,
            logical_width: window.width().to_u32(),
            logical_height: window.height().to_u32(),
            scale: monitor_info.scale,
            monitor: monitor_info.index,
            saved_window_mode: SavedWindowMode::Windowed,
            app_name: String::new(),
        };

        let mut states = existing.unwrap_or_default();
        states.insert(WindowKey::Managed(unique_name.clone()), window_state);
        persistence::save_all_states(&config.path, &states);
        debug!("[on_managed_window_added] Saved initial state for \"{unique_name}\"");
    }
}

/// Observer: unregister a `ManagedWindow` name when removed, and update state file if `ActiveOnly`.
pub(crate) fn on_managed_window_removed(
    remove: On<Remove, ManagedWindow>,
    mut registry: ResMut<ManagedWindowRegistry>,
    config: Res<RestoreWindowConfig>,
    persistence: Res<ManagedWindowPersistence>,
    monitors: Res<Monitors>,
    all_windows: Query<
        (
            Entity,
            &Window,
            Option<&CurrentMonitor>,
            Option<&ManagedWindow>,
        ),
        Or<(With<PrimaryWindow>, With<ManagedWindow>)>,
    >,
    primary_query: Query<(), With<PrimaryWindow>>,
) {
    let entity = remove.entity;
    if let Some(name) = registry.entities.remove(&entity) {
        // If `ActiveOnly`, rebuild state from all remaining active windows.
        // The removed entity's `ManagedWindow` is being removed, so the query
        // naturally excludes it — but guard against it just in case.
        if *persistence == ManagedWindowPersistence::ActiveOnly {
            persistence::save_active_window_state(
                &config,
                &monitors,
                &all_windows,
                &primary_query,
                Some(entity),
            );
            debug!(
                "[on_managed_window_removed] Rebuilt state file without \"{name}\" (ActiveOnly)"
            );
        }

        registry.names.remove(&name);
        debug!(
            "[on_managed_window_removed] Unregistered managed window \"{name}\" from entity {entity:?}"
        );
    }
}

/// When `ManagedWindowPersistence` switches to `ActiveOnly`, immediately rebuild the state
/// file from the currently-active windows so that any previously-remembered-but-closed
/// window entries are pruned.
pub(crate) fn on_persistence_changed(
    persistence: Res<ManagedWindowPersistence>,
    config: Res<RestoreWindowConfig>,
    monitors: Res<Monitors>,
    all_windows: Query<
        (
            Entity,
            &Window,
            Option<&CurrentMonitor>,
            Option<&ManagedWindow>,
        ),
        Or<(With<PrimaryWindow>, With<ManagedWindow>)>,
    >,
    primary_query: Query<(), With<PrimaryWindow>>,
) {
    if *persistence == ManagedWindowPersistence::ActiveOnly {
        persistence::save_active_window_state(
            &config,
            &monitors,
            &all_windows,
            &primary_query,
            None,
        );
        debug!("[on_persistence_changed] Rebuilt state file for ActiveOnly mode");
    }
}

/// Observer: hide a managed window on creation and load its saved state.
pub(crate) fn on_managed_window_load(
    add: On<Add, ManagedWindow>,
    mut commands: Commands,
    managed: Query<&ManagedWindow>,
    monitors: Res<Monitors>,
    winit_info: Option<Res<WinitInfo>>,
    config: Res<RestoreWindowConfig>,
    mut windows: Query<&mut Window>,
    primary_monitor: Query<&CurrentMonitor, With<PrimaryWindow>>,
    platform: Res<Platform>,
) {
    let entity = add.entity;
    let Ok(managed_window) = managed.get(entity) else {
        return;
    };
    let name = &managed_window.name;

    // Hide window during restore (on Linux X11 with frame extent compensation, don't hide)
    if let Ok(mut window) = windows.get_mut(entity)
        && platform.should_hide_on_startup()
    {
        window.visible = false;
    }

    // Check the startup snapshot — not the file, which may have been modified by
    // `on_managed_window_added` saving initial state for brand-new windows.
    let window_key = WindowKey::Managed((*name).clone());
    let Some(saved_state) = config.loaded_states.get(&window_key).cloned() else {
        debug!("[on_managed_window_load] No saved state for \"{name}\", showing window");
        if let Ok(mut window) = windows.get_mut(entity) {
            window.visible = true;
        }
        return;
    };

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
    saved_window_state: &WindowState,
    monitors: &Monitors,
    winit_info: &WinitInfo,
    commands: &mut Commands,
    primary_scale: f64,
    platform: Platform,
) {
    let resolved_monitor = restore::resolve_target_monitor_and_position(
        saved_window_state.monitor,
        saved_window_state.logical_position,
        monitors,
    );
    if matches!(
        resolved_monitor.monitor_resolution_source,
        restore::MonitorResolutionSource::FallbackToPrimary
    ) {
        warn!(
            "[restore_managed_window] Target monitor {} not found, falling back to monitor {PRIMARY_MONITOR_INDEX}",
            saved_window_state.monitor,
        );
    }

    let physical_decoration = winit_info.physical_decoration();

    // The window is created on the focused window's monitor (the primary window's monitor)
    // without explicit positioning. Its starting scale matches the primary monitor, not the
    // target monitor.
    let target_position = restore::compute_target_position(
        saved_window_state,
        resolved_monitor.monitor_info,
        resolved_monitor.logical_position,
        physical_decoration,
        primary_scale,
        platform,
    );

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
        resolved_monitor.monitor_info.physical_position.x,
        resolved_monitor.monitor_info.physical_position.y,
        resolved_monitor.monitor_info.physical_size.x,
        resolved_monitor.monitor_info.physical_size.y,
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
