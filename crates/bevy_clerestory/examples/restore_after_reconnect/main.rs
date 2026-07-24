//! End-to-end monitor reconnect consumer with a shared causal trace.

mod constants;
mod control;
mod lifecycle;
mod recovery_trace;
mod remote;
mod setup;
mod trace;
mod window_panel;
mod window_trace;

use std::env::VarError;
use std::env::var;
use std::fs::remove_file;
use std::io::Error;
use std::io::ErrorKind;
use std::path::PathBuf;
use std::process::id;

use bevy::ecs::schedule::common_conditions::not;
use bevy::ecs::schedule::common_conditions::resource_exists;
use bevy::log::LogPlugin;
use bevy::prelude::*;
use bevy::window::ExitCondition;
use bevy::window::MonitorSelection;
use bevy::window::VideoModeSelection;
use bevy::window::WindowMode;
use bevy::window::WindowPosition;
use bevy::window::WindowResolution;
use bevy_clerestory::MonitorConnected;
use bevy_clerestory::MonitorDisconnected;
use bevy_clerestory::MonitorTopologyRevision;
use bevy_clerestory::WindowManagerPlugin;
use constants::DEFAULT_EXTERNAL_MONITOR_INDEX;
use constants::DEFAULT_PROBE_PORT;
use constants::EXIT_AFTER_FRAME_ENVIRONMENT_VARIABLE;
use constants::MONITOR_INDEX_ENVIRONMENT_VARIABLE;
use constants::PERSISTENCE_FILE_PREFIX;
use constants::PRIMARY_WINDOW_TITLE;
use constants::PROBE_BOOT_NONCE_ENVIRONMENT_VARIABLE;
use constants::PROBE_CAPABILITY_ENVIRONMENT_VARIABLE;
use constants::PROBE_PERSISTENCE_PATH_ENVIRONMENT_VARIABLE;
use constants::PROBE_PORT_ENVIRONMENT_VARIABLE;
use constants::PROBE_RUN_ID_ENVIRONMENT_VARIABLE;
use constants::PROBE_WINDOW_HEIGHT;
use constants::PROBE_WINDOW_WIDTH;
use constants::STARTUP_MODE_BORDERLESS;
use constants::STARTUP_MODE_ENVIRONMENT_VARIABLE;
use constants::STARTUP_MODE_EXCLUSIVE;
use constants::STARTUP_MODE_WINDOWED;
use trace::ProbeTrace;

struct HotplugProbePlugin;

impl Plugin for HotplugProbePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<setup::AcceptedWindowKeys>()
            .init_resource::<control::CommandReceipts>()
            .init_resource::<recovery_trace::ApplicationRecoveryCycles>()
            .init_resource::<recovery_trace::PreUnplugReadiness>()
            .init_resource::<window_panel::ProbePanelMaterial>()
            .init_resource::<window_panel::ProbeTarget>()
            .init_resource::<window_trace::WindowBindings>()
            .init_resource::<window_trace::WindowSnapshots>()
            .add_observer(lifecycle::on_primary_window_added)
            .add_observer(lifecycle::on_managed_window_added)
            .add_observer(lifecycle::on_managed_window_removed)
            .add_observer(lifecycle::on_managed_window_despawned)
            .add_observer(lifecycle::on_window_added)
            .add_observer(lifecycle::on_window_removed)
            .add_observer(lifecycle::on_window_despawned)
            .add_observer(window_panel::remove_window_clear_camera)
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
            .add_observer(recovery_trace::on_window_recovery_pending)
            .add_observer(recovery_trace::on_window_recovery_available)
            .add_observer(recovery_trace::on_window_restored)
            .add_observer(recovery_trace::on_window_restore_mismatch)
            .add_observer(recovery_trace::prepare_application_window_restore)
            .add_observer(control::apply_probe_command)
            .add_systems(
                Startup,
                (
                    window_panel::spawn_transparency_camera,
                    setup::spawn_probe_windows,
                    setup::trace_probe_session,
                )
                    .chain(),
            )
            .add_systems(
                PreUpdate,
                (
                    window_panel::attach_window_content,
                    recovery_trace::request_application_window_restore,
                )
                    .chain(),
            )
            .add_systems(
                Update,
                (
                    setup::request_probe_window_placement,
                    (
                        setup::place_and_register_probe_windows,
                        setup::place_and_confirm_unregistered_control,
                    ),
                    (
                        setup::control_automatic_window_mode,
                        setup::cancel_automatic_window_recovery,
                    )
                        .run_if(not(resource_exists::<SmokeExitFrame>)),
                    recovery_trace::record_recovery_readiness,
                    window_trace::trace_os_window_events,
                    window_trace::trace_internal_window_messages,
                )
                    .chain_ignore_deferred(),
            )
            .add_systems(
                PostUpdate,
                (
                    window_trace::trace_window_component_changes,
                    window_panel::refresh_window_panels,
                )
                    .chain(),
            )
            .add_systems(Last, setup::exit_after_smoke_frame);
    }
}

#[derive(Resource)]
struct ProbeMonitorIndex(usize);

/// Deterministic initial `WindowMode` for the managed automatic window,
/// selected once at launch through `CLERESTORY_PROBE_STARTUP_MODE`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Resource)]
enum ProbeStartupMode {
    Windowed,
    Borderless,
    Exclusive,
}

impl ProbeStartupMode {
    /// Initial mode for the managed automatic window. Both fullscreen modes
    /// target the selected probe monitor index; exclusive fullscreen keeps the
    /// monitor's current video mode.
    const fn automatic_window_mode(self, monitor_index: usize) -> WindowMode {
        match self {
            Self::Windowed => WindowMode::Windowed,
            Self::Borderless => {
                WindowMode::BorderlessFullscreen(MonitorSelection::Index(monitor_index))
            },
            Self::Exclusive => WindowMode::Fullscreen(
                MonitorSelection::Index(monitor_index),
                VideoModeSelection::Current,
            ),
        }
    }

    /// Documented `CLERESTORY_PROBE_STARTUP_MODE` spelling for trace records.
    const fn selector(self) -> &'static str {
        match self {
            Self::Windowed => STARTUP_MODE_WINDOWED,
            Self::Borderless => STARTUP_MODE_BORDERLESS,
            Self::Exclusive => STARTUP_MODE_EXCLUSIVE,
        }
    }
}

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

fn parse_startup_mode(value: Option<&str>) -> std::io::Result<ProbeStartupMode> {
    match value {
        None | Some(STARTUP_MODE_WINDOWED) => Ok(ProbeStartupMode::Windowed),
        Some(STARTUP_MODE_BORDERLESS) => Ok(ProbeStartupMode::Borderless),
        Some(STARTUP_MODE_EXCLUSIVE) => Ok(ProbeStartupMode::Exclusive),
        Some(other) => Err(Error::new(
            ErrorKind::InvalidInput,
            format!(
                "invalid {STARTUP_MODE_ENVIRONMENT_VARIABLE}: {other:?} (expected \
                 {STARTUP_MODE_WINDOWED}, {STARTUP_MODE_BORDERLESS}, or {STARTUP_MODE_EXCLUSIVE})"
            ),
        )),
    }
}

fn selected_startup_mode() -> std::io::Result<ProbeStartupMode> {
    parse_startup_mode(optional_environment_value(STARTUP_MODE_ENVIRONMENT_VARIABLE)?.as_deref())
}

fn persistence_path() -> std::io::Result<PathBuf> {
    Ok(
        optional_environment_value(PROBE_PERSISTENCE_PATH_ENVIRONMENT_VARIABLE)?.map_or_else(
            || std::env::temp_dir().join(format!("{PERSISTENCE_FILE_PREFIX}-{}.ron", id())),
            PathBuf::from,
        ),
    )
}

fn probe_port() -> std::io::Result<u16> {
    optional_environment_value(PROBE_PORT_ENVIRONMENT_VARIABLE)?.map_or(
        Ok(DEFAULT_PROBE_PORT),
        |value| {
            value.parse().map_err(|error| {
                Error::new(
                    ErrorKind::InvalidInput,
                    format!("invalid {PROBE_PORT_ENVIRONMENT_VARIABLE}: {error}"),
                )
            })
        },
    )
}

fn probe_session() -> std::io::Result<remote::ProbeSession> {
    let process_id = id();
    let run_id = optional_environment_value(PROBE_RUN_ID_ENVIRONMENT_VARIABLE)?
        .unwrap_or_else(|| format!("manual-{process_id}"));
    let boot_nonce = optional_environment_value(PROBE_BOOT_NONCE_ENVIRONMENT_VARIABLE)?
        .unwrap_or_else(|| format!("boot-{process_id}"));
    let capability = optional_environment_value(PROBE_CAPABILITY_ENVIRONMENT_VARIABLE)?
        .unwrap_or_else(|| format!("local-{process_id}"));
    Ok(remote::ProbeSession::new(run_id, boot_nonce, capability))
}

fn fresh_persistence_path() -> std::io::Result<PathBuf> {
    let path = persistence_path()?;
    match remove_file(&path) {
        Ok(()) => {},
        Err(error) if error.kind() == ErrorKind::NotFound => {},
        Err(error) => return Err(error),
    }
    Ok(path)
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
    let startup_mode = selected_startup_mode()?;
    let smoke_exit_frame = smoke_exit_frame()?;
    let persistence_path = fresh_persistence_path()?;
    let probe_port = probe_port()?;
    let probe_session = probe_session()?;
    let mut app = App::new();
    app.insert_resource(ProbeTrace::default())
        .insert_resource(ProbeMonitorIndex(monitor_index))
        .insert_resource(startup_mode)
        .insert_resource(probe_session)
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
                        resolution: WindowResolution::new(PROBE_WINDOW_WIDTH, PROBE_WINDOW_HEIGHT),
                        ..default()
                    }),
                    exit_condition: ExitCondition::DontExit,
                    ..default()
                }),
        )
        .add_plugins(remote::plugin())
        .add_plugins(remote::http_plugin(probe_port))
        .add_plugins(hana_diegetic::DiegeticUiPlugin)
        .add_plugins(WindowManagerPlugin::with_path(persistence_path));
    if let Some(frame) = smoke_exit_frame {
        app.insert_resource(SmokeExitFrame(frame));
    }
    app.run();
    Ok(())
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    reason = "tests should panic on unexpected values"
)]
mod tests {
    use super::*;

    const SELECTED_STARTUP_MONITOR_INDEX: usize = 2;

    #[test]
    fn startup_mode_selector_parses_each_documented_value_and_defaults_to_windowed() {
        assert_eq!(
            parse_startup_mode(None).expect("absent selector should default"),
            ProbeStartupMode::Windowed,
        );
        assert_eq!(
            parse_startup_mode(Some(STARTUP_MODE_WINDOWED)).expect("windowed should parse"),
            ProbeStartupMode::Windowed,
        );
        assert_eq!(
            parse_startup_mode(Some(STARTUP_MODE_BORDERLESS)).expect("borderless should parse"),
            ProbeStartupMode::Borderless,
        );
        assert_eq!(
            parse_startup_mode(Some(STARTUP_MODE_EXCLUSIVE)).expect("exclusive should parse"),
            ProbeStartupMode::Exclusive,
        );
    }

    #[test]
    fn startup_mode_selector_rejects_unknown_values_naming_the_variable() {
        let error = parse_startup_mode(Some("fullscreen"))
            .expect_err("undocumented selector value should be rejected");
        assert_eq!(error.kind(), ErrorKind::InvalidInput);
        assert!(
            error
                .to_string()
                .contains(STARTUP_MODE_ENVIRONMENT_VARIABLE)
        );
    }

    #[test]
    fn startup_mode_trace_spelling_matches_the_documented_selector_values() {
        assert_eq!(ProbeStartupMode::Windowed.selector(), STARTUP_MODE_WINDOWED);
        assert_eq!(
            ProbeStartupMode::Borderless.selector(),
            STARTUP_MODE_BORDERLESS
        );
        assert_eq!(
            ProbeStartupMode::Exclusive.selector(),
            STARTUP_MODE_EXCLUSIVE
        );
    }

    #[test]
    fn startup_mode_fullscreen_modes_target_the_selected_monitor_index() {
        assert_eq!(
            ProbeStartupMode::Windowed.automatic_window_mode(SELECTED_STARTUP_MONITOR_INDEX),
            WindowMode::Windowed,
        );
        assert_eq!(
            ProbeStartupMode::Borderless.automatic_window_mode(SELECTED_STARTUP_MONITOR_INDEX),
            WindowMode::BorderlessFullscreen(MonitorSelection::Index(
                SELECTED_STARTUP_MONITOR_INDEX
            )),
        );
        assert_eq!(
            ProbeStartupMode::Exclusive.automatic_window_mode(SELECTED_STARTUP_MONITOR_INDEX),
            WindowMode::Fullscreen(
                MonitorSelection::Index(SELECTED_STARTUP_MONITOR_INDEX),
                VideoModeSelection::Current,
            ),
        );
    }
}
