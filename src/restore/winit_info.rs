use bevy::ecs::system::NonSendMarker;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy::window::WindowMode;
use bevy::window::WindowPosition;
use bevy::winit::WINIT_WINDOWS;

use super::target_position;
use super::target_position::MonitorResolutionSource;
use super::target_position::TargetPosition;
use crate::Platform;
use crate::WindowKey;
use crate::config::RestoreWindowConfig;
use crate::constants::DEFAULT_SCALE_FACTOR;
use crate::monitors::CurrentMonitor;
use crate::monitors::Monitors;
use crate::persistence;
#[cfg(all(target_os = "windows", feature = "workaround-winit-3124"))]
use crate::persistence::SavedWindowMode;

/// Window decoration dimensions (title bar, borders).
struct WindowDecoration {
    physical_width:  u32,
    physical_height: u32,
}

/// Information from winit captured at startup.
#[derive(Resource)]
pub struct WinitInfo {
    starting_monitor_index: usize,
    window_decoration:      WindowDecoration,
}

impl WinitInfo {
    /// Get window decoration dimensions as a `UVec2`.
    #[must_use]
    pub const fn physical_decoration(&self) -> UVec2 {
        UVec2::new(
            self.window_decoration.physical_width,
            self.window_decoration.physical_height,
        )
    }
}

/// Token indicating X11 frame extent compensation is complete (W6 workaround).
///
/// This component gates `restore_windows` - the restore system cannot process
/// a window until this token exists on the entity. On Linux X11 with W6 workaround
/// enabled, this ensures frame extents are queried and position is compensated
/// before restore proceeds. On other platforms/configurations, the token is
/// inserted immediately during `load_target_position` since no compensation is needed.
#[derive(Component)]
pub struct X11FrameCompensated;

/// Populate `WinitInfo` resource from winit (decoration and starting monitor).
///
/// # Panics
///
/// Panics if no monitors are available (e.g., laptop lid closed at startup).
/// Window management requires at least one monitor to function.
pub fn init_winit_info(
    mut commands: Commands,
    window_entity: Single<Entity, With<PrimaryWindow>>,
    monitors: Res<Monitors>,
    _: NonSendMarker,
) {
    assert!(
        !monitors.is_empty(),
        "No monitors available - cannot initialize window manager without a display"
    );

    WINIT_WINDOWS.with(|winit_windows| {
        let winit_windows = winit_windows.borrow();
        if let Some(winit_window) = winit_windows.get_window(*window_entity) {
            let outer = winit_window.outer_size();
            let inner = winit_window.inner_size();
            let physical_decoration = WindowDecoration {
                physical_width:  outer.width.saturating_sub(inner.width),
                physical_height: outer.height.saturating_sub(inner.height),
            };

            let physical_position = winit_window.outer_position().map_or(
                IVec2::ZERO,
                |position| IVec2::new(position.x, position.y),
            );

            debug!(
                "[init_winit_info] outer_position={physical_position:?} platform={:?}",
                Platform::detect()
            );

            let starting_monitor = winit_window
                .current_monitor()
                .and_then(|current_monitor| {
                    let physical_monitor_position = current_monitor.position();
                    let info = monitors.at(physical_monitor_position.x, physical_monitor_position.y);
                    debug!(
                        "[init_winit_info] current_monitor() position=({}, {}) -> index={:?}",
                        physical_monitor_position.x,
                        physical_monitor_position.y,
                        info.map(|monitor| monitor.index)
                    );
                    info.copied()
                })
                .unwrap_or_else(|| {
                    debug!(
                        "[init_winit_info] current_monitor() unavailable, falling back to closest_to({}, {})",
                        physical_position.x,
                        physical_position.y
                    );
                    *monitors.closest_to(physical_position.x, physical_position.y)
                });
            let starting_monitor_index = starting_monitor.index;

            debug!(
                "[init_winit_info] decoration={}x{} position=({}, {}) starting_monitor={starting_monitor_index}",
                physical_decoration.physical_width,
                physical_decoration.physical_height,
                physical_position.x,
                physical_position.y,
            );

            commands.entity(*window_entity).insert(CurrentMonitor {
                monitor:        starting_monitor,
                effective_mode: WindowMode::Windowed,
            });

            commands.insert_resource(WinitInfo {
                starting_monitor_index,
                window_decoration: physical_decoration,
            });
        }
    });
}

/// Load saved window state and insert `TargetPosition` on the primary window entity.
pub fn load_target_position(
    mut commands: Commands,
    window_entity: Single<Entity, With<PrimaryWindow>>,
    monitors: Res<Monitors>,
    winit_info: Res<WinitInfo>,
    mut config: ResMut<RestoreWindowConfig>,
    platform: Res<Platform>,
) {
    if let Some(all_states) = persistence::load_all_states(&config.path) {
        config.loaded_states = all_states;
    }

    let Some(state) = config.loaded_states.get(&WindowKey::Primary).cloned() else {
        debug!("[load_target_position] No saved bevy_window_manager state, showing window");
        commands.queue(|world: &mut World| {
            let mut query = world.query_filtered::<&mut Window, With<PrimaryWindow>>();
            if let Some(mut window) = query.iter_mut(world).next() {
                window.visible = true;
            }
        });
        return;
    };

    debug!(
        "[load_target_position] Loaded state: position={:?} logical_size={}x{} monitor_scale={} monitor_index={} mode={:?}",
        state.logical_position,
        state.logical_width,
        state.logical_height,
        state.scale,
        state.monitor,
        state.mode
    );

    let starting_monitor_index = winit_info.starting_monitor_index;
    let starting_scale = monitors
        .by_index(starting_monitor_index)
        .map_or(DEFAULT_SCALE_FACTOR, |monitor| monitor.scale);

    let resolved = target_position::resolve_target_monitor_and_position(
        state.monitor,
        state.logical_position,
        &monitors,
    );
    if matches!(resolved.source, MonitorResolutionSource::FallbackToPrimary) {
        warn!(
            "[load_target_position] Target monitor {} not found, falling back to monitor 0",
            state.monitor
        );
    }

    let target = target_position::compute_target_position(
        &state,
        resolved.info,
        resolved.logical_position,
        winit_info.physical_decoration(),
        starting_scale,
        *platform,
    );

    debug!(
        "[load_target_position] Starting monitor={starting_monitor_index} scale={starting_scale}, Target monitor={} scale={}, strategy={:?}, position={:?}",
        target.monitor_index, target.target_scale, target.scale_strategy, target.physical_position
    );

    #[cfg(all(target_os = "windows", feature = "workaround-winit-3124"))]
    if matches!(state.mode, SavedWindowMode::Fullscreen { .. }) {
        debug!(
            "[load_target_position] Windows exclusive fullscreen: showing window for surface creation"
        );
        commands.queue(|world: &mut World| {
            let mut query = world.query_filtered::<&mut Window, With<PrimaryWindow>>();
            if let Some(mut window) = query.iter_mut(world).next() {
                window.visible = true;
            }
        });
    }

    let entity = *window_entity;
    let is_fullscreen = state.mode.is_fullscreen();
    commands.entity(entity).insert(target);

    if is_fullscreen || !platform.needs_frame_compensation() {
        commands.entity(entity).insert(X11FrameCompensated);
    }
}

/// Move the primary window to the target monitor for fullscreen restore on X11.
///
/// Body is platform-neutral Bevy code; only the `add_systems` registration in
/// `lib.rs` is gated to Linux. The early `is_wayland` check makes the system
/// inert on Wayland; non-Linux platforms never schedule it at all.
pub fn move_to_target_monitor(
    mut window: Single<&mut Window, With<PrimaryWindow>>,
    targets: Query<&TargetPosition, With<PrimaryWindow>>,
    platform: Res<Platform>,
) {
    if !platform.is_x11() {
        return;
    }

    let Ok(target) = targets.single() else {
        return;
    };

    if !target.mode.is_fullscreen() {
        return;
    }

    if let Some(position) = target.physical_position {
        debug!("[move_to_target_monitor] X11 fullscreen: setting position={position:?}");
        window.position = WindowPosition::At(position);
    }
}
