use std::collections::BTreeMap;

use bevy::prelude::*;
use bevy::window::OnMonitor;
use bevy::window::PrimaryWindow;
use bevy::window::WindowPosition;
use bevy_clerestory::CurrentMonitor;
use bevy_clerestory::ManagedWindow;
use bevy_clerestory::MonitorIdentity;
use bevy_clerestory::MonitorInfo;
use bevy_clerestory::MonitorTopologyRevision;
use bevy_clerestory::Monitors;
use bevy_clerestory::WindowKey;
use bevy_clerestory::WindowRecovery;
use bevy_remote::BrpError;
use bevy_remote::BrpResult;
use bevy_remote::RemotePlugin;
use bevy_remote::error_codes::INVALID_PARAMS;
use bevy_remote::http::RemoteHttpPlugin;
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;

use super::ProbeMonitorIndex;
use super::ProbeStartupMode;
use super::constants::*;
use super::control::CommandReceipt;
use super::control::CommandReceipts;
use super::control::ProbeCommand;
use super::control::ProbeCommandIntent;
use super::setup;
use super::trace::ProbeTrace;

#[derive(Resource)]
pub(super) struct ProbeSession {
    pub(super) run_id:     String,
    pub(super) boot_nonce: String,
    capability:            String,
}

impl ProbeSession {
    pub(super) const fn new(run_id: String, boot_nonce: String, capability: String) -> Self {
        Self {
            run_id,
            boot_nonce,
            capability,
        }
    }
}

#[derive(Deserialize)]
struct AuthenticatedRequest {
    capability: String,
}

#[derive(Deserialize)]
struct RecordRequest {
    capability:     String,
    #[serde(default)]
    after_sequence: u64,
}

trait CapabilityRequest {
    fn capability(&self) -> &str;
}

impl CapabilityRequest for AuthenticatedRequest {
    fn capability(&self) -> &str { &self.capability }
}

impl CapabilityRequest for RecordRequest {
    fn capability(&self) -> &str { &self.capability }
}

#[derive(Deserialize)]
struct CommandRequest {
    capability: String,
    command_id: String,
    command:    ProbeCommand,
}

impl CapabilityRequest for CommandRequest {
    fn capability(&self) -> &str { &self.capability }
}

#[derive(Serialize)]
struct MonitorSnapshot {
    entity:            u64,
    name:              Option<String>,
    identity:          String,
    verified_id:       Option<String>,
    index:             usize,
    scale:             f64,
    physical_position: [i32; 2],
    physical_size:     [u32; 2],
}

impl MonitorSnapshot {
    fn from_monitor(entity: Entity, name: Option<String>, monitor: MonitorInfo) -> Self {
        Self {
            entity: entity.to_bits(),
            name,
            identity: format!("{:?}", monitor.identity),
            verified_id: match monitor.identity {
                MonitorIdentity::Verified(monitor_id) => Some(format!("{monitor_id:?}")),
                MonitorIdentity::Unverified => None,
            },
            index: monitor.index,
            scale: monitor.scale,
            physical_position: [monitor.physical_position.x, monitor.physical_position.y],
            physical_size: [monitor.physical_size.x, monitor.physical_size.y],
        }
    }
}

#[derive(Default, Serialize)]
struct RecoveryCounts {
    accepted:     usize,
    pending:      usize,
    available:    usize,
    restored:     usize,
    mismatch:     usize,
    cancellation: usize,
}

#[derive(Serialize)]
#[serde(rename_all = "kebab-case")]
enum Readiness {
    Ready,
    Pending,
}

impl From<bool> for Readiness {
    fn from(ready: bool) -> Self { if ready { Self::Ready } else { Self::Pending } }
}

#[derive(Serialize)]
#[serde(rename_all = "kebab-case")]
enum Presence {
    Present,
    Absent,
}

impl From<bool> for Presence {
    fn from(present: bool) -> Self { if present { Self::Present } else { Self::Absent } }
}

#[derive(Serialize)]
#[serde(rename_all = "kebab-case")]
enum Focus {
    Focused,
    Unfocused,
}

impl From<bool> for Focus {
    fn from(focused: bool) -> Self {
        if focused {
            Self::Focused
        } else {
            Self::Unfocused
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "kebab-case")]
enum MonitorCoverage {
    Full,
    Partial,
}

impl From<bool> for MonitorCoverage {
    fn from(covers: bool) -> Self { if covers { Self::Full } else { Self::Partial } }
}

#[derive(Serialize)]
#[serde(rename_all = "kebab-case")]
enum NativeFullscreen {
    Fullscreen,
    Windowed,
    Unavailable,
}

impl From<Option<bool>> for NativeFullscreen {
    fn from(state: Option<bool>) -> Self {
        match state {
            Some(true) => Self::Fullscreen,
            Some(false) => Self::Windowed,
            None => Self::Unavailable,
        }
    }
}

#[derive(Serialize)]
struct WindowSnapshot {
    key:                    String,
    entity:                 u64,
    recovery_policy:        Option<String>,
    current_monitor:        Option<MonitorSnapshot>,
    requested_mode:         String,
    effective_mode:         Option<String>,
    position:               String,
    physical_size:          [u32; 2],
    decorated:              Presence,
    focused:                Focus,
    native_fullscreen:      NativeFullscreen,
    covers_current_monitor: MonitorCoverage,
    replacement_count:      usize,
    recovery_counts:        RecoveryCounts,
}

#[derive(Serialize)]
struct ProbeSnapshot {
    schema_version:         u32,
    run_id:                 String,
    boot_nonce:             String,
    ready:                  Readiness,
    startup_mode:           &'static str,
    selected_monitor_index: usize,
    topology_revision:      u64,
    record_cursor:          u64,
    monitors:               Vec<MonitorSnapshot>,
    windows:                Vec<WindowSnapshot>,
    terminal_failure:       Presence,
    command_receipts:       Vec<CommandReceipt>,
}

#[derive(Serialize)]
struct WireRecord {
    run_id:                String,
    boot_nonce:            String,
    sequence:              u64,
    cycle_id:              usize,
    timestamp_unix_micros: u128,
    frame_count:           u32,
    producer:              String,
    kind:                  String,
    fields:                BTreeMap<String, String>,
}

#[derive(Serialize)]
struct RecordResponse {
    schema_version: u32,
    run_id:         String,
    boot_nonce:     String,
    next_cursor:    u64,
    records:        Vec<WireRecord>,
}

pub(super) fn plugin() -> RemotePlugin {
    RemotePlugin::default()
        .with_method_main(PROBE_COMMAND_METHOD, command_handler)
        .with_method_main(PROBE_SNAPSHOT_METHOD, snapshot_handler)
        .with_method_main(PROBE_RECORDS_METHOD, records_handler)
        .with_method_main(PROBE_SHUTDOWN_METHOD, shutdown_handler)
}

pub(super) fn http_plugin(port: u16) -> RemoteHttpPlugin {
    RemoteHttpPlugin::default().with_port(port)
}

fn authenticated<T: for<'de> Deserialize<'de> + CapabilityRequest>(
    params: Option<Value>,
    session: &ProbeSession,
) -> Result<T, BrpError> {
    let params = params.ok_or_else(|| invalid_params("missing request parameters"))?;
    let request: T = serde_json::from_value(params).map_err(invalid_params)?;
    if request.capability() != session.capability {
        return Err(invalid_params("invalid capability"));
    }
    Ok(request)
}

fn snapshot_handler(In(params): In<Option<Value>>, world: &mut World) -> BrpResult {
    let session = world
        .get_resource::<ProbeSession>()
        .ok_or_else(|| BrpError::internal("probe session is unavailable"))?;
    let _: AuthenticatedRequest = authenticated(params, session)?;
    serde_json::to_value(snapshot(world)).map_err(BrpError::internal)
}

fn records_handler(In(params): In<Option<Value>>, world: &mut World) -> BrpResult {
    let session = world
        .get_resource::<ProbeSession>()
        .ok_or_else(|| BrpError::internal("probe session is unavailable"))?;
    let request: RecordRequest = authenticated(params, session)?;
    let trace = world
        .get_resource::<ProbeTrace>()
        .ok_or_else(|| BrpError::internal("probe trace is unavailable"))?;
    let mut cycle_id = 0;
    let mut records = Vec::new();
    for record in trace.records() {
        if record.kind == KIND_MONITOR_DISCONNECTED {
            cycle_id += 1;
        }
        if record.sequence <= request.after_sequence {
            continue;
        }
        records.push(WireRecord {
            run_id: session.run_id.clone(),
            boot_nonce: session.boot_nonce.clone(),
            sequence: record.sequence,
            cycle_id,
            timestamp_unix_micros: record.timestamp_unix_micros,
            frame_count: record.frame_count,
            producer: record.producer,
            kind: record.kind,
            fields: record.fields.into_iter().collect(),
        });
    }
    let next_cursor = records
        .last()
        .map_or(request.after_sequence, |record| record.sequence);
    serde_json::to_value(RecordResponse {
        schema_version: 1,
        run_id: session.run_id.clone(),
        boot_nonce: session.boot_nonce.clone(),
        next_cursor,
        records,
    })
    .map_err(BrpError::internal)
}

fn shutdown_handler(In(params): In<Option<Value>>, world: &mut World) -> BrpResult {
    let session = world
        .get_resource::<ProbeSession>()
        .ok_or_else(|| BrpError::internal("probe session is unavailable"))?;
    let _: AuthenticatedRequest = authenticated(params, session)?;
    world.write_message(AppExit::Success);
    serde_json::to_value(serde_json::json!({ "accepted": true })).map_err(BrpError::internal)
}

fn command_handler(In(params): In<Option<Value>>, world: &mut World) -> BrpResult {
    let session = world
        .get_resource::<ProbeSession>()
        .ok_or_else(|| BrpError::internal("probe session is unavailable"))?;
    let request: CommandRequest = authenticated(params, session)?;
    if request.command_id.is_empty() {
        return Err(invalid_params("command_id must not be empty"));
    }
    if let Some(receipt) = world
        .resource::<CommandReceipts>()
        .0
        .get(&request.command_id)
    {
        return serde_json::to_value(receipt).map_err(BrpError::internal);
    }
    let command_id = request.command_id.clone();
    world.trigger(ProbeCommandIntent {
        command_id: request.command_id,
        command:    request.command,
    });
    let receipt = world
        .resource::<CommandReceipts>()
        .0
        .get(&command_id)
        .ok_or_else(|| BrpError::internal("probe command produced no receipt"))?;
    serde_json::to_value(receipt).map_err(BrpError::internal)
}

fn snapshot(world: &mut World) -> ProbeSnapshot {
    let session = world.resource::<ProbeSession>();
    let run_id = session.run_id.clone();
    let boot_nonce = session.boot_nonce.clone();
    let startup_mode = world.resource::<ProbeStartupMode>().selector();
    let selected_monitor_index = world.resource::<ProbeMonitorIndex>().0;
    let topology_revision = world.resource::<MonitorTopologyRevision>().get();
    let trace = world.resource::<ProbeTrace>().clone();
    let records = trace.records();
    let record_cursor = records.last().map_or(0, |record| record.sequence);
    let ready: Readiness = records
        .iter()
        .any(|record| record.kind == KIND_RECOVERY_READY)
        .into();
    let terminal_failure: Presence = records
        .iter()
        .any(|record| record.kind == KIND_RECOVERY_MISMATCH)
        .into();
    let command_receipts = world
        .resource::<CommandReceipts>()
        .0
        .values()
        .cloned()
        .collect();
    let monitor_values: Vec<_> = world
        .get_resource::<Monitors>()
        .map(|monitors| {
            monitors
                .iter()
                .map(|monitor| (monitor.entity, *monitor.monitor_info))
                .collect()
        })
        .unwrap_or_default();
    let monitors = monitor_values
        .into_iter()
        .map(|(entity, monitor_info)| {
            let name = world
                .get::<bevy::window::Monitor>(entity)
                .and_then(|monitor| monitor.name.clone());
            MonitorSnapshot::from_monitor(entity, name, monitor_info)
        })
        .collect();
    let mut windows_query = world.query::<(
        Entity,
        &Window,
        Option<&CurrentMonitor>,
        Option<&OnMonitor>,
        Option<&PrimaryWindow>,
        Option<&ManagedWindow>,
        Option<&WindowRecovery>,
    )>();
    let windows = windows_query
        .iter(world)
        .filter_map(
            |(entity, window, current_monitor, on_monitor, primary, managed, recovery)| {
                let window_key = setup::canonical_window_key(primary, managed)?;
                Some(window_snapshot(
                    entity,
                    window,
                    current_monitor,
                    on_monitor,
                    recovery,
                    window_key,
                    &records,
                ))
            },
        )
        .collect();
    ProbeSnapshot {
        schema_version: 1,
        run_id,
        boot_nonce,
        ready,
        startup_mode,
        selected_monitor_index,
        topology_revision,
        record_cursor,
        monitors,
        windows,
        terminal_failure,
        command_receipts,
    }
}

fn window_snapshot(
    entity: Entity,
    window: &Window,
    current_monitor: Option<&CurrentMonitor>,
    on_monitor: Option<&OnMonitor>,
    recovery: Option<&WindowRecovery>,
    window_key: WindowKey,
    records: &[super::trace::TraceRecord],
) -> WindowSnapshot {
    let key_debug = format!("{window_key:?}");
    let current_monitor_snapshot = current_monitor.map(|current| {
        MonitorSnapshot::from_monitor(
            on_monitor.map_or(Entity::PLACEHOLDER, |on_monitor| on_monitor.0),
            None,
            current.monitor_info,
        )
    });
    let counts = recovery_counts(records, &key_debug);
    let created = record_count(records, KIND_WINDOW_CREATED, &key_debug);
    let covers_current_monitor: MonitorCoverage = current_monitor
        .is_some_and(|current| {
            window.resolution.physical_size() == current.physical_size
                && match window.position {
                    WindowPosition::At(position) => position == current.physical_position,
                    WindowPosition::Automatic | WindowPosition::Centered(_) => false,
                }
        })
        .into();
    WindowSnapshot {
        key: window_key.to_string(),
        entity: entity.to_bits(),
        recovery_policy: recovery.map(|recovery| format!("{recovery:?}")),
        current_monitor: current_monitor_snapshot,
        requested_mode: format!("{:?}", window.mode),
        effective_mode: current_monitor
            .map(|current| format!("{:?}", current.effective_window_mode)),
        position: format!("{:?}", window.position),
        physical_size: [
            window.resolution.physical_width(),
            window.resolution.physical_height(),
        ],
        decorated: window.decorations.into(),
        focused: window.focused.into(),
        native_fullscreen: native_fullscreen(entity).into(),
        covers_current_monitor,
        replacement_count: created.saturating_sub(1),
        recovery_counts: counts,
    }
}

fn recovery_counts(records: &[super::trace::TraceRecord], window_key: &str) -> RecoveryCounts {
    RecoveryCounts {
        accepted:     record_count(records, KIND_RECOVERY_ACCEPTED, window_key),
        pending:      record_count(records, KIND_RECOVERY_PENDING, window_key),
        available:    record_count(records, KIND_RECOVERY_AVAILABLE, window_key),
        restored:     record_count(records, KIND_RECOVERY_RESTORED, window_key),
        mismatch:     record_count(records, KIND_RECOVERY_MISMATCH, window_key),
        cancellation: record_count(records, KIND_RECOVERY_CANCELLATION_REQUESTED, window_key),
    }
}

fn record_count(records: &[super::trace::TraceRecord], kind: &str, window_key: &str) -> usize {
    records
        .iter()
        .filter(|record| {
            record.kind == kind
                && record
                    .fields
                    .iter()
                    .any(|(name, value)| name == FIELD_WINDOW_KEY && value.contains(window_key))
        })
        .count()
}

fn invalid_params(error: impl ToString) -> BrpError {
    BrpError {
        code:    INVALID_PARAMS,
        message: error.to_string(),
        data:    None,
    }
}

#[cfg(target_os = "macos")]
fn native_fullscreen(entity: Entity) -> Option<bool> {
    use bevy::winit::WINIT_WINDOWS;
    use objc2_app_kit::NSView;
    use objc2_app_kit::NSWindowStyleMask;
    use raw_window_handle::HasWindowHandle;
    use raw_window_handle::RawWindowHandle;

    WINIT_WINDOWS.with_borrow(|winit_windows| {
        let winit_window = winit_windows.get_window(entity)?;
        let handle = winit_window.window_handle().ok()?;
        let RawWindowHandle::AppKit(appkit_handle) = handle.as_raw() else {
            return None;
        };
        // SAFETY: `ns_view` comes from the live winit window handle above.
        let ns_view: &NSView = unsafe { appkit_handle.ns_view.cast().as_ref() };
        let window = ns_view.window()?;
        Some(window.styleMask().contains(NSWindowStyleMask::FullScreen))
    })
}

#[cfg(not(target_os = "macos"))]
const fn native_fullscreen(_entity: Entity) -> Option<bool> { None }
