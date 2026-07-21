#![doc = include_str!("../README.md")]
//!
//! # Technical Details
//!
//! ## The Problem
//!
//! On macOS with multiple monitors that have different scale factors (e.g., a Retina display
//! at scale 2.0 and an external monitor at scale 1.0), Bevy's window positioning has issues:
//!
//! 1. **`Window.position` is unreliable at startup**: When a window is created, `Window.position`
//!    is `Automatic` (not `At(position)`), even though winit has placed the window at a specific
//!    physical position.
//!
//! 2. **Scale factor conversion in `changed_windows`**: When you modify `Window.resolution`, Bevy's
//!    `changed_windows` system applies scale factor conversion if `scale_factor !=
//!    cached_scale_factor`. This corrupts the size when moving windows between monitors with
//!    different scale factors.
//!
//! 3. **Timing of scale factor updates**: The `CachedWindow` is updated after winit events are
//!    processed, but our systems run before we receive the `ScaleFactorChanged` event.
//!
//! ## The Solution
//!
//! This plugin uses winit directly to capture the actual window position at startup,
//! compensates for scale factor conversions, restores `Window.position`,
//! `Window.resolution`, visibility, and monitor state across monitors.
//!
//! The plugin automatically hides the window during startup and shows it after positioning
//! is complete, preventing any visual flash at the default position.
//!
//! See the `custom_app_name` example for how to override the `app_name` used in the path
//! (default is to choose the executable name).
//!
//! See the `custom_path` example for how to override the full path to the state file.

mod constants;
mod events;
#[cfg(target_os = "macos")]
mod macos_tabbing_fix;
mod managed;
mod monitors;
mod persistence;
mod platform;
mod restore;
mod restore_window_config;
mod visibility;
#[cfg(all(target_os = "windows", feature = "workaround-winit-4341"))]
mod windows_dpi_fix;
#[cfg(all(target_os = "linux", feature = "workaround-winit-4445"))]
mod x11_position_fix;

use std::path::PathBuf;

use bevy::prelude::*;
use bevy::window::PrimaryWindow;
pub use events::WindowRestoreMismatch;
pub use events::WindowRestored;
pub use managed::ManagedWindow;
pub use managed::ManagedWindowPersistence;
use managed::ManagedWindowRegistry;
use managed::on_managed_window_added;
use managed::on_managed_window_load;
use managed::on_managed_window_removed;
use managed::on_persistence_changed;
pub use monitors::CurrentMonitor;
pub use monitors::LiveMonitor;
pub use monitors::MonitorConnected;
pub use monitors::MonitorDisconnected;
pub use monitors::MonitorId;
pub use monitors::MonitorIdentity;
pub use monitors::MonitorInfo;
use monitors::MonitorPlugin;
pub use monitors::MonitorTopologyRevision;
pub use monitors::Monitors;
use persistence::PersistencePlugin;
pub use persistence::WindowKey;
pub use platform::Platform;
use restore::RestorePlugin;
#[cfg(all(target_os = "linux", feature = "workaround-winit-4445"))]
use restore::has_restoring_windows;
use restore_window_config::RestoreWindowConfig;

#[derive(Clone, Debug, Hash, PartialEq, Eq, SystemSet)]
enum ClerestoryPreStartupSet {
    MonitorsInitialized,
    PersistenceLoaded,
}

/// The main plugin. See module docs for usage.
///
/// Default state file locations:
/// - macOS: `~/Library/Application Support/<executable_name>/windows.ron`
/// - Linux: `~/.config/<executable_name>/windows.ron`
/// - Windows: `C:\Users\<User>\AppData\Roaming\<executable_name>\windows.ron`
///
/// Unit struct version for convenience using `.add_plugins(WindowManagerPlugin)`.
pub struct WindowManagerPlugin;

impl WindowManagerPlugin {
    /// Create a plugin with a custom app name.
    ///
    /// Uses `config_dir()/<app_name>/windows.ron`.
    ///
    /// # Panics
    ///
    /// Panics if the config directory cannot be determined.
    #[must_use]
    #[expect(clippy::expect_used, reason = "fail fast if path cannot be determined")]
    pub fn with_app_name(app_name: impl Into<String>) -> impl Plugin {
        WindowManagerPluginCustomPath {
            path:                       persistence::get_state_path_for_app(&app_name.into())
                .expect("Could not determine state file path"),
            managed_window_persistence: ManagedWindowPersistence::default(),
        }
    }

    /// Create a plugin with a custom state file path.
    #[must_use]
    pub fn with_path(path: impl Into<PathBuf>) -> impl Plugin {
        WindowManagerPluginCustomPath {
            path:                       path.into(),
            managed_window_persistence: ManagedWindowPersistence::default(),
        }
    }

    /// Create a plugin with a specific persistence behavior.
    ///
    /// # Panics
    ///
    /// Panics if the config directory cannot be determined.
    #[must_use]
    #[expect(clippy::expect_used, reason = "fail fast if path cannot be determined")]
    pub fn with_persistence(managed_window_persistence: ManagedWindowPersistence) -> impl Plugin {
        WindowManagerPluginCustomPath {
            path: persistence::get_default_state_path()
                .expect("Could not determine state file path"),
            managed_window_persistence,
        }
    }
}

impl Plugin for WindowManagerPlugin {
    #[expect(clippy::expect_used, reason = "fail fast if path cannot be determined")]
    fn build(&self, app: &mut App) {
        app.add_plugins(WindowManagerPluginCustomPath {
            path:                       persistence::get_default_state_path()
                .expect("Could not determine state file path"),
            managed_window_persistence: ManagedWindowPersistence::default(),
        });
    }
}

/// Plugin variant with a custom state file path.
struct WindowManagerPluginCustomPath {
    path:                       PathBuf,
    managed_window_persistence: ManagedWindowPersistence,
}

impl Plugin for WindowManagerPluginCustomPath {
    fn build(&self, app: &mut App) {
        let path = self.path.clone();
        let managed_window_persistence = self.managed_window_persistence.clone();

        let platform = Platform::detect();
        app.insert_resource(platform);

        // Hide primary window to prevent flash at default position.
        // Two cases to handle:
        // 1. Window already exists (`WindowManagerPlugin` added after `DefaultPlugins`) — hide
        //    immediately
        // 2. Window doesn't exist yet (`WindowManagerPlugin` added before `DefaultPlugins`) — use
        //    observer
        //
        // EXCEPTION: On Linux X11 with frame extent compensation (workaround-winit-4445),
        // we cannot hide the window because the compensation system needs to query
        // `_NET_FRAME_EXTENTS`, which requires the window to be visible/mapped.
        let should_hide = platform.should_hide_on_startup();

        if should_hide {
            let mut query = app
                .world_mut()
                .query_filtered::<&mut Window, With<PrimaryWindow>>();
            if let Some(mut window) = query.iter_mut(app.world_mut()).next() {
                debug!("[build] Window already exists, hiding immediately");
                window.visible = false;
            } else {
                debug!("[build] Window doesn't exist yet, registering observer");
                app.add_observer(visibility::hide_window_on_creation);
            }
        } else {
            debug!("[build] Linux X11: skipping window hide for frame extent compensation");
        }

        #[cfg(target_os = "macos")]
        {
            // App-wide opt-out of automatic window tabbing, before winit creates
            // any OS window. See `macos_tabbing_fix` module docs.
            macos_tabbing_fix::disable_automatic_tabbing();
            app.add_systems(
                Update,
                macos_tabbing_fix::disable_tabbing_on_managed.before(restore::restore_windows),
            );
        }

        #[cfg(all(target_os = "windows", feature = "workaround-winit-4341"))]
        {
            app.add_systems(Startup, windows_dpi_fix::install_dpi_fix);
            app.add_systems(Update, windows_dpi_fix::install_dpi_fix_on_managed);
        }

        app.configure_sets(
            PreStartup,
            (
                ClerestoryPreStartupSet::MonitorsInitialized,
                ClerestoryPreStartupSet::PersistenceLoaded,
            )
                .chain(),
        )
        .add_plugins(MonitorPlugin)
        .add_plugins(PersistencePlugin)
        .add_plugins(RestorePlugin)
        .insert_resource(RestoreWindowConfig { path })
        .insert_resource(managed_window_persistence)
        .init_resource::<ManagedWindowRegistry>()
        .add_observer(on_managed_window_added)
        .add_observer(on_managed_window_removed)
        .add_observer(on_managed_window_load);

        // X11 frame extent compensation (W6 workaround, winit #4445).
        #[cfg(all(target_os = "linux", feature = "workaround-winit-4445"))]
        app.add_systems(
            Update,
            (
                x11_position_fix::compensate_target_position
                    .after(restore::prepare_restore_targets)
                    .before(restore::restore_windows),
                // Re-apply the compensated position once the window is mapped: bevy 0.19
                // can ignore the first `set_outer_position` request while the X11 window is
                // unmapped, while a mapped window's `Window.position` readback matches the
                // requested compensated position plus `X11FrameTop`.
                x11_position_fix::reapply_compensated_position
                    .after(restore::restore_windows)
                    .before(restore::check_restore_settling),
            )
                .run_if(has_restoring_windows)
                .run_if(|p: Res<Platform>| p.is_x11()),
        );

        app.add_systems(
            Update,
            on_persistence_changed
                .run_if(resource_changed::<ManagedWindowPersistence>)
                .after(monitors::update_current_monitor)
                .before(persistence::write_dirty_window_states),
        );
    }
}
