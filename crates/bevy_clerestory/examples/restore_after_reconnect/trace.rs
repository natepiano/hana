use std::sync::Arc;
use std::sync::Mutex;
use std::time::Instant;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

use bevy::diagnostic::FrameCount;
use bevy::log::BoxedLayer;
use bevy::log::tracing::Event;
use bevy::log::tracing::field::Field;
use bevy::log::tracing::field::Visit;
use bevy::log::tracing_subscriber::Layer;
use bevy::log::tracing_subscriber::filter::FilterFn;
use bevy::log::tracing_subscriber::layer::Context;
use bevy::log::tracing_subscriber::registry::Registry;
use bevy::prelude::*;

use super::constants::*;
use crate::MonitorConnected;
use crate::MonitorDisconnected;
use crate::MonitorTopologyRevision;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TraceRecord {
    pub(crate) sequence:              u64,
    pub(crate) timestamp_unix_micros: u128,
    pub(crate) frame_count:           u32,
    pub(crate) producer:              String,
    pub(crate) kind:                  String,
    pub(crate) fields:                Vec<(String, String)>,
}

impl TraceRecord {
    fn line(&self) -> String {
        let fields = self
            .fields
            .iter()
            .map(|(name, value)| format!("{name}={value}"))
            .collect::<Vec<_>>()
            .join(" ");
        format!(
            "clerestory_probe sequence={} timestamp_unix_micros={} frame_count={} producer={:?} kind={:?} {}",
            self.sequence,
            self.timestamp_unix_micros,
            self.frame_count,
            self.producer,
            self.kind,
            fields,
        )
    }
}

struct TraceState {
    next_sequence: u64,
    instant:       Instant,
    unix_micros:   u128,
    records:       Vec<TraceRecord>,
}

impl Default for TraceState {
    fn default() -> Self {
        let started_unix_micros = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |duration| duration.as_micros());
        Self {
            next_sequence: 1,
            instant:       Instant::now(),
            unix_micros:   started_unix_micros,
            records:       Vec::new(),
        }
    }
}

/// Shared sequence and timestamp authority for every probe producer.
#[derive(Clone, Default, Resource)]
pub(crate) struct ProbeTrace(Arc<Mutex<TraceState>>);

impl ProbeTrace {
    pub(super) fn record(
        &self,
        frame_count: u32,
        producer: impl Into<String>,
        kind: impl Into<String>,
        fields: Vec<(String, String)>,
    ) {
        let mut state = match self.0.lock() {
            Ok(state) => state,
            Err(poisoned) => poisoned.into_inner(),
        };
        let record = TraceRecord {
            sequence: state.next_sequence,
            timestamp_unix_micros: state.unix_micros + state.instant.elapsed().as_micros(),
            frame_count,
            producer: producer.into(),
            kind: kind.into(),
            fields,
        };
        state.next_sequence += 1;
        println!("{}", record.line());
        state.records.push(record);
    }

    #[cfg(test)]
    pub(crate) fn records(&self) -> Vec<TraceRecord> {
        match self.0.lock() {
            Ok(state) => state.records.clone(),
            Err(poisoned) => poisoned.into_inner().records.clone(),
        }
    }
}

fn field(name: &str, value: impl std::fmt::Debug) -> (String, String) {
    (name.into(), format!("{value:?}"))
}

pub(crate) fn on_monitor_connected(
    event: On<MonitorConnected>,
    revision: Res<MonitorTopologyRevision>,
    trace: Res<ProbeTrace>,
    frame_count_resource: Res<FrameCount>,
) {
    trace.record(
        frame_count_resource.0,
        PRODUCER_MONITOR_CONNECTED,
        KIND_MONITOR_CONNECTED,
        vec![
            field(FIELD_MONITOR_ENTITY, event.entity),
            field(FIELD_MONITOR, event.monitor),
            field(FIELD_TOPOLOGY_REVISION, revision.get()),
            field(FIELD_TRANSITION, TRANSITION_CREATED),
        ],
    );
}

pub(crate) fn on_monitor_disconnected(
    event: On<MonitorDisconnected>,
    revision: Res<MonitorTopologyRevision>,
    trace: Res<ProbeTrace>,
    frame_count_resource: Res<FrameCount>,
) {
    trace.record(
        frame_count_resource.0,
        PRODUCER_MONITOR_DISCONNECTED,
        KIND_MONITOR_DISCONNECTED,
        vec![
            field(FIELD_MONITOR_ENTITY, event.former_entity),
            field(FIELD_MONITOR, event.monitor),
            field(FIELD_TOPOLOGY_REVISION, revision.get()),
            field(FIELD_TRANSITION, TRANSITION_REMOVED),
        ],
    );
}

#[derive(Default)]
struct MonitorProbeFields {
    frame_count: Option<u32>,
    producer:    Option<String>,
    fields:      Vec<(String, String)>,
}

impl Visit for MonitorProbeFields {
    fn record_u64(&mut self, field: &Field, value: u64) {
        if field.name() == TRACE_FIELD_FRAME_COUNT {
            self.frame_count = u32::try_from(value).ok();
        }
        self.fields.push((field.name().into(), value.to_string()));
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == TRACE_FIELD_PRODUCER_SCHEDULE {
            self.producer = Some(value.into());
        }
        self.fields
            .push((field.name().into(), format!("{value:?}")));
    }

    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        self.fields
            .push((field.name().into(), format!("{value:?}")));
    }
}

struct MonitorProbeLayer {
    trace: ProbeTrace,
}

impl Layer<Registry> for MonitorProbeLayer {
    fn on_event(&self, event: &Event<'_>, _context: Context<'_, Registry>) {
        let mut fields = MonitorProbeFields::default();
        event.record(&mut fields);
        self.trace.record(
            fields.frame_count.unwrap_or_default(),
            fields
                .producer
                .unwrap_or_else(|| event.metadata().target().into()),
            KIND_MONITOR_TOPOLOGY,
            fields.fields,
        );
    }
}

fn monitor_target(metadata: &bevy::log::tracing::Metadata<'_>) -> bool {
    metadata.target() == MONITOR_PROBE_TARGET
}

pub(crate) fn monitor_probe_layer(app: &mut App) -> Option<BoxedLayer> {
    let trace = app.world_mut().get_resource::<ProbeTrace>()?.clone();
    Some(Box::new(
        MonitorProbeLayer { trace }.with_filter(FilterFn::new(monitor_target)),
    ))
}
