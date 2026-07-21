//! Causal monitor hotplug probe for primary and managed secondary windows.

mod constants;
mod lifecycle;
mod recovery_trace;
mod setup;
mod trace;
mod window_trace;

use std::env::VarError;
use std::env::var;
use std::fs::remove_file;
use std::io::Error;
use std::io::ErrorKind;
use std::path::PathBuf;
use std::process::id;

use bevy::log::LogPlugin;
use bevy::prelude::*;
use bevy::window::ExitCondition;
use bevy::window::MonitorSelection;
use bevy::window::WindowPosition;
use bevy_clerestory::MonitorConnected;
use bevy_clerestory::MonitorDisconnected;
use bevy_clerestory::MonitorTopologyRevision;
use bevy_clerestory::Platform;
use bevy_clerestory::WindowManagerPlugin;
use constants::DEFAULT_EXTERNAL_MONITOR_INDEX;
use constants::EXIT_AFTER_FRAME_ENVIRONMENT_VARIABLE;
use constants::MONITOR_INDEX_ENVIRONMENT_VARIABLE;
use constants::PERSISTENCE_FILE_PREFIX;
use constants::PRIMARY_WINDOW_TITLE;
use trace::ProbeTrace;

struct HotplugProbePlugin;

impl Plugin for HotplugProbePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<window_trace::WindowBindings>()
            .init_resource::<window_trace::WindowSnapshots>()
            .add_observer(lifecycle::on_primary_window_added)
            .add_observer(lifecycle::on_managed_window_added)
            .add_observer(lifecycle::on_managed_window_removed)
            .add_observer(lifecycle::on_managed_window_despawned)
            .add_observer(lifecycle::on_window_added)
            .add_observer(lifecycle::on_window_removed)
            .add_observer(lifecycle::on_window_despawned)
            .add_observer(lifecycle::on_monitor_added)
            .add_observer(lifecycle::on_monitor_removed)
            .add_observer(lifecycle::on_monitor_despawned)
            .add_observer(lifecycle::on_monitor_link_added)
            .add_observer(lifecycle::on_monitor_link_discarded)
            .add_observer(lifecycle::on_monitor_link_inserted)
            .add_observer(lifecycle::on_monitor_link_removed)
            .add_observer(lifecycle::on_monitor_link_despawned)
            .add_observer(lifecycle::on_has_windows_added)
            .add_observer(lifecycle::on_has_windows_removed)
            .add_observer(lifecycle::on_has_windows_despawned)
            .add_observer(trace::on_monitor_connected)
            .add_observer(trace::on_monitor_disconnected)
            .add_observer(setup::place_and_register_probe_window)
            .add_observer(recovery_trace::on_window_recovery_pending)
            .add_observer(recovery_trace::on_window_recovery_available)
            .add_systems(
                Startup,
                (setup::spawn_secondary_window, setup::trace_probe_session).chain(),
            )
            .add_systems(
                Update,
                (
                    setup::request_probe_window_placement,
                    window_trace::trace_os_window_events,
                    window_trace::trace_internal_window_messages,
                ),
            )
            .add_systems(PostUpdate, window_trace::trace_window_component_changes)
            .add_systems(Last, setup::exit_after_smoke_frame);
    }
}

#[derive(Resource)]
struct ProbeMonitorIndex(usize);

#[derive(Resource)]
struct SmokeExitFrame(u32);

fn optional_environment_value(name: &str) -> std::io::Result<Option<String>> {
    match var(name) {
        Ok(value) => Ok(Some(value)),
        Err(VarError::NotPresent) => Ok(None),
        Err(VarError::NotUnicode(_)) => Err(Error::new(
            ErrorKind::InvalidInput,
            format!("{name} must contain Unicode text"),
        )),
    }
}

fn selected_monitor_index() -> std::io::Result<usize> {
    optional_environment_value(MONITOR_INDEX_ENVIRONMENT_VARIABLE)?.map_or(
        Ok(DEFAULT_EXTERNAL_MONITOR_INDEX),
        |value| {
            value.parse().map_err(|error| {
                Error::new(
                    ErrorKind::InvalidInput,
                    format!("invalid {MONITOR_INDEX_ENVIRONMENT_VARIABLE}: {error}"),
                )
            })
        },
    )
}

fn persistence_path() -> PathBuf {
    std::env::temp_dir().join(format!("{PERSISTENCE_FILE_PREFIX}-{}.ron", id()))
}

fn fresh_persistence_path() -> std::io::Result<PathBuf> {
    let path = persistence_path();
    match remove_file(&path) {
        Ok(()) => {},
        Err(error) if error.kind() == ErrorKind::NotFound => {},
        Err(error) => return Err(error),
    }
    Ok(path)
}

const fn selected_window_position(platform: Platform, monitor_index: usize) -> WindowPosition {
    if platform.position_available() {
        WindowPosition::Centered(MonitorSelection::Index(monitor_index))
    } else {
        WindowPosition::Automatic
    }
}

fn smoke_exit_frame() -> std::io::Result<Option<u32>> {
    optional_environment_value(EXIT_AFTER_FRAME_ENVIRONMENT_VARIABLE)?
        .map(|value| {
            value.parse().map_err(|error| {
                Error::new(
                    ErrorKind::InvalidInput,
                    format!("invalid {EXIT_AFTER_FRAME_ENVIRONMENT_VARIABLE}: {error}"),
                )
            })
        })
        .transpose()
}

fn main() -> std::io::Result<()> {
    let monitor_index = selected_monitor_index()?;
    let smoke_exit_frame = smoke_exit_frame()?;
    let persistence_path = fresh_persistence_path()?;
    let mut app = App::new();
    app.insert_resource(ProbeTrace::default())
        .insert_resource(ProbeMonitorIndex(monitor_index))
        .add_plugins(HotplugProbePlugin)
        .add_plugins(
            DefaultPlugins
                .set(LogPlugin {
                    custom_layer: trace::monitor_probe_layer,
                    ..default()
                })
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: PRIMARY_WINDOW_TITLE.into(),
                        position: WindowPosition::Automatic,
                        ..default()
                    }),
                    exit_condition: ExitCondition::DontExit,
                    ..default()
                }),
        )
        .add_plugins(WindowManagerPlugin::with_path(persistence_path));
    if let Some(frame) = smoke_exit_frame {
        app.insert_resource(SmokeExitFrame(frame));
    }
    app.run();
    Ok(())
}
