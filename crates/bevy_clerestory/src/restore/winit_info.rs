use bevy::ecs::system::NonSendMarker;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy::window::WindowMode;
use bevy::window::WindowPosition;
use bevy::winit::WINIT_WINDOWS;

use super::NativeWindowReady;
use super::RestorePreparation;
use super::target_position::TargetPosition;
use crate::Platform;
use crate::WindowKey;
use crate::monitors::CurrentMonitor;
use crate::monitors::Monitors;
use crate::persistence::CapturedWindowStates;

/// Window decoration dimensions (title bar, borders).
struct WindowDecoration {
    physical_width:  u32,
    physical_height: u32,
}

/// Information from winit captured at startup.
#[derive(Resource)]
pub(crate) struct WinitInfo {
    window_decoration: WindowDecoration,
}

impl WinitInfo {
    /// Get window decoration dimensions as a `UVec2`.
    #[must_use]
    pub(crate) const fn physical_decoration(&self) -> UVec2 {
        UVec2::new(
            self.window_decoration.physical_width,
            self.window_decoration.physical_height,
        )
    }
}

#[cfg(test)]
impl Default for WinitInfo {
    fn default() -> Self {
        Self {
            window_decoration: WindowDecoration {
                physical_width:  0,
                physical_height: 0,
            },
        }
    }
}

/// Token indicating X11 frame extent compensation is complete (W6 workaround).
///
/// This component gates `restore_windows` - the restore system cannot process
/// a window until this token exists on the entity. On Linux X11 with W6 workaround
/// enabled, this ensures frame extents are queried and position is compensated
/// before restore proceeds. On other platforms/configurations, the token is
/// inserted immediately during `prepare_restore_targets` since no compensation is needed.
#[derive(Component)]
pub(crate) struct X11FrameCompensated;

/// Populate `WinitInfo` resource from winit (decoration and starting monitor).
///
/// # Panics
///
/// Panics if no monitors are available (e.g., laptop lid closed at startup).
/// Window management requires at least one monitor to function.
pub(crate) fn init_winit_info(
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
            let physical_outer_size = winit_window.outer_size();
            let physical_inner_size = winit_window.inner_size();
            let physical_decoration = WindowDecoration {
                physical_width: physical_outer_size
                    .width
                    .saturating_sub(physical_inner_size.width),
                physical_height: physical_outer_size
                    .height
                    .saturating_sub(physical_inner_size.height),
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
                    let monitor_info =
                        monitors.at(physical_monitor_position.x, physical_monitor_position.y);
                    debug!(
                        "[init_winit_info] current_monitor() position=({}, {}) -> index={:?}",
                        physical_monitor_position.x,
                        physical_monitor_position.y,
                        monitor_info.map(|monitor| monitor.index)
                    );
                    monitor_info.copied()
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

            commands.entity(*window_entity).insert((
                CurrentMonitor {
                    monitor_info:          starting_monitor,
                    effective_window_mode: WindowMode::Windowed,
                },
                NativeWindowReady,
            ));

            commands.insert_resource(WinitInfo {
                window_decoration: physical_decoration,
            });
        }
    });
}

/// Queue primary startup restoration from the one-read captured-state authority.
pub(crate) fn queue_primary_restore(
    mut commands: Commands,
    window_entity: Single<Entity, With<PrimaryWindow>>,
    mut captured_window_states: ResMut<CapturedWindowStates>,
) {
    if !captured_window_states.bind_and_freeze(&WindowKey::Primary, *window_entity) {
        debug!("[queue_primary_restore] No saved bevy_clerestory state, showing window");
        commands.queue(|world: &mut World| {
            let mut query = world.query_filtered::<&mut Window, With<PrimaryWindow>>();
            if let Some(mut window) = query.iter_mut(world).next() {
                window.visible = true;
            }
        });
        return;
    }

    commands
        .entity(*window_entity)
        .insert(RestorePreparation::startup(WindowKey::Primary));
}

/// Move the primary window to the target monitor for fullscreen restore on X11.
///
/// Body is platform-neutral Bevy code; only the `add_systems` registration in
/// `lib.rs` is gated to Linux. The early `is_wayland` check makes the system
/// inert on Wayland; non-Linux platforms never schedule it at all.
pub(crate) fn move_to_target_monitor(
    mut window: Single<&mut Window, With<PrimaryWindow>>,
    targets: Query<&TargetPosition, With<PrimaryWindow>>,
    platform: Res<Platform>,
) {
    if !platform.is_x11() {
        return;
    }

    let Ok(target_position) = targets.single() else {
        return;
    };

    if !target_position.saved_window_mode.is_fullscreen() {
        return;
    }

    if let Some(position) = target_position.physical_position {
        debug!("[move_to_target_monitor] X11 fullscreen: setting position={position:?}");
        window.position = WindowPosition::At(position);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::monitors::MonitorIdentity;
    use crate::monitors::MonitorInfo;
    use crate::persistence::CapturedWindowPlacement;
    use crate::persistence::CapturedWindowPosition;
    use crate::persistence::SavedWindowMode;

    #[test]
    fn primary_startup_protects_saved_placement_when_restore_is_queued() {
        let original_placement = CapturedWindowPlacement {
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
        };
        let mut app = App::new();
        app.init_resource::<CapturedWindowStates>()
            .add_systems(PreStartup, queue_primary_restore);
        let entity = app
            .world_mut()
            .spawn((Window::default(), PrimaryWindow))
            .id();
        app.world_mut()
            .resource_mut::<CapturedWindowStates>()
            .promote(WindowKey::Primary, entity, original_placement.clone());

        app.world_mut().run_schedule(PreStartup);

        assert!(app.world().get::<RestorePreparation>(entity).is_some());
        let mut captured_window_states = app.world_mut().resource_mut::<CapturedWindowStates>();
        assert!(captured_window_states.is_bound_to(&WindowKey::Primary, entity));
        captured_window_states.capture(
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
                position:          CapturedWindowPosition::CompositorControlled,
                logical_size:      UVec2::new(1_024, 768),
                saved_window_mode: SavedWindowMode::Windowed,
                captured_scale:    1.0,
            },
        );
        assert_eq!(
            captured_window_states.captured_placement(&WindowKey::Primary),
            Some(&original_placement)
        );
    }
}
