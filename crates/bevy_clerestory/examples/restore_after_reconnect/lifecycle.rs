use bevy::diagnostic::FrameCount;
use bevy::ecs::relationship::RelationshipTarget;
use bevy::ecs::system::NonSendMarker;
use bevy::prelude::*;
use bevy::window::HasWindows;
use bevy::window::Monitor;
use bevy::window::OnMonitor;
use bevy::window::PrimaryWindow;
use bevy_clerestory::ManagedWindow;
use bevy_clerestory::Monitors;
use bevy_clerestory::WindowKey;

use super::constants::*;
use super::trace::ProbeTrace;
use super::window_trace::WindowBindings;
use super::window_trace::native_monitor_fields;
use super::window_trace::window_key;

fn frame_count(frame_count: Option<Res<FrameCount>>) -> u32 {
    frame_count.map_or(0, |frame_count| frame_count.0)
}

fn field(name: &str, value: impl std::fmt::Debug) -> (String, String) {
    (name.into(), format!("{value:?}"))
}

fn lifecycle_producer(phase: &str, component: &str) -> String {
    format!("lifecycle::On<{phase}, {component}>")
}

fn record_component(
    trace: &ProbeTrace,
    frame_count: u32,
    producer: &str,
    component: &str,
    phase: &str,
    entity: Entity,
    mut fields: Vec<(String, String)>,
) {
    fields.extend([
        field(FIELD_COMPONENT, component),
        field(FIELD_PHASE, phase),
        field(FIELD_ENTITY, entity),
    ]);
    trace.record(frame_count, producer, KIND_COMPONENT_LIFECYCLE, fields);
}

pub(super) fn on_primary_window_added(
    add: On<Add, PrimaryWindow>,
    mut bindings: ResMut<WindowBindings>,
) {
    bindings.0.insert(add.entity, WindowKey::Primary);
}

pub(super) fn on_managed_window_added(
    add: On<Add, ManagedWindow>,
    managed_windows: Query<&ManagedWindow>,
    mut bindings: ResMut<WindowBindings>,
    trace: Res<ProbeTrace>,
    frame_count_resource: Option<Res<FrameCount>>,
) {
    let Ok(managed_window) = managed_windows.get(add.entity) else {
        return;
    };
    let window_key = WindowKey::Managed(managed_window.name.clone());
    bindings.0.insert(add.entity, window_key.clone());
    record_component(
        &trace,
        frame_count(frame_count_resource),
        &lifecycle_producer(PHASE_ADD, COMPONENT_MANAGED_WINDOW),
        COMPONENT_MANAGED_WINDOW,
        PHASE_ADD,
        add.entity,
        vec![field(FIELD_WINDOW_KEY, window_key)],
    );
}

pub(super) fn on_managed_window_removed(
    remove: On<Remove, ManagedWindow>,
    bindings: Res<WindowBindings>,
    trace: Res<ProbeTrace>,
    frame_count_resource: Option<Res<FrameCount>>,
) {
    record_component(
        &trace,
        frame_count(frame_count_resource),
        &lifecycle_producer(PHASE_REMOVE, COMPONENT_MANAGED_WINDOW),
        COMPONENT_MANAGED_WINDOW,
        PHASE_REMOVE,
        remove.entity,
        vec![field(FIELD_WINDOW_KEY, bindings.0.get(&remove.entity))],
    );
}

pub(super) fn on_managed_window_despawned(
    despawn: On<Despawn, ManagedWindow>,
    bindings: Res<WindowBindings>,
    trace: Res<ProbeTrace>,
    frame_count_resource: Option<Res<FrameCount>>,
) {
    record_component(
        &trace,
        frame_count(frame_count_resource),
        &lifecycle_producer(PHASE_DESPAWN, COMPONENT_MANAGED_WINDOW),
        COMPONENT_MANAGED_WINDOW,
        PHASE_DESPAWN,
        despawn.entity,
        vec![field(FIELD_WINDOW_KEY, bindings.0.get(&despawn.entity))],
    );
}

pub(super) fn on_window_added(
    add: On<Add, Window>,
    windows: Query<&Window>,
    trace: Res<ProbeTrace>,
    frame_count_resource: Option<Res<FrameCount>>,
) {
    let fields = windows.get(add.entity).map_or_else(
        |_| Vec::new(),
        |window| {
            vec![
                field(FIELD_WINDOW_POSITION, window.position),
                field(FIELD_WINDOW_SIZE, window.resolution.physical_size()),
                field(FIELD_WINDOW_MODE, window.mode),
            ]
        },
    );
    record_component(
        &trace,
        frame_count(frame_count_resource),
        &lifecycle_producer(PHASE_ADD, COMPONENT_WINDOW),
        COMPONENT_WINDOW,
        PHASE_ADD,
        add.entity,
        fields,
    );
}

pub(super) fn on_window_removed(
    remove: On<Remove, Window>,
    bindings: Res<WindowBindings>,
    trace: Res<ProbeTrace>,
    frame_count_resource: Option<Res<FrameCount>>,
) {
    record_component(
        &trace,
        frame_count(frame_count_resource),
        &lifecycle_producer(PHASE_REMOVE, COMPONENT_WINDOW),
        COMPONENT_WINDOW,
        PHASE_REMOVE,
        remove.entity,
        vec![field(FIELD_WINDOW_KEY, bindings.0.get(&remove.entity))],
    );
}

pub(super) fn on_window_despawned(
    despawn: On<Despawn, Window>,
    bindings: Res<WindowBindings>,
    trace: Res<ProbeTrace>,
    frame_count_resource: Option<Res<FrameCount>>,
) {
    trace.record(
        frame_count(frame_count_resource),
        lifecycle_producer(PHASE_DESPAWN, COMPONENT_WINDOW),
        KIND_ENTITY_REMOVAL,
        vec![
            field(FIELD_COMPONENT, COMPONENT_WINDOW),
            field(FIELD_ENTITY, despawn.entity),
            field(FIELD_WINDOW_KEY, bindings.0.get(&despawn.entity)),
        ],
    );
}

pub(super) fn on_monitor_added(
    add: On<Add, Monitor>,
    monitors: Query<&Monitor>,
    trace: Res<ProbeTrace>,
    frame_count_resource: Option<Res<FrameCount>>,
) {
    let fields = monitors.get(add.entity).map_or_else(
        |_| Vec::new(),
        |monitor| vec![field(FIELD_MONITOR, monitor)],
    );
    record_component(
        &trace,
        frame_count(frame_count_resource),
        &lifecycle_producer(PHASE_ADD, COMPONENT_MONITOR),
        COMPONENT_MONITOR,
        PHASE_ADD,
        add.entity,
        fields,
    );
}

pub(super) fn on_monitor_removed(
    remove: On<Remove, Monitor>,
    monitors: Query<&Monitor>,
    trace: Res<ProbeTrace>,
    frame_count_resource: Option<Res<FrameCount>>,
) {
    let fields = monitors.get(remove.entity).map_or_else(
        |_| Vec::new(),
        |monitor| vec![field(FIELD_MONITOR, monitor)],
    );
    record_component(
        &trace,
        frame_count(frame_count_resource),
        &lifecycle_producer(PHASE_REMOVE, COMPONENT_MONITOR),
        COMPONENT_MONITOR,
        PHASE_REMOVE,
        remove.entity,
        fields,
    );
}

pub(super) fn on_monitor_despawned(
    despawn: On<Despawn, Monitor>,
    trace: Res<ProbeTrace>,
    frame_count_resource: Option<Res<FrameCount>>,
) {
    trace.record(
        frame_count(frame_count_resource),
        lifecycle_producer(PHASE_DESPAWN, COMPONENT_MONITOR),
        KIND_ENTITY_REMOVAL,
        vec![
            field(FIELD_COMPONENT, COMPONENT_MONITOR),
            field(FIELD_ENTITY, despawn.entity),
        ],
    );
}

pub(super) fn on_monitor_link_added(
    add: On<Add, OnMonitor>,
    links: Query<&OnMonitor>,
    bindings: Res<WindowBindings>,
    windows: Query<(Has<PrimaryWindow>, Option<&ManagedWindow>)>,
    installed_monitors: Option<Res<Monitors>>,
    winit_monitors: Res<bevy::winit::WinitMonitors>,
    trace: Res<ProbeTrace>,
    frame_count_resource: Option<Res<FrameCount>>,
    _: NonSendMarker,
) {
    let mut fields = vec![
        field(FIELD_WINDOW, add.entity),
        field(
            FIELD_WINDOW_KEY,
            window_key(add.entity, &bindings, &windows),
        ),
        field(
            FIELD_MONITOR_ENTITY,
            links.get(add.entity).ok().map(|link| link.0),
        ),
    ];
    fields.extend(native_monitor_fields(
        add.entity,
        installed_monitors.as_deref(),
        &winit_monitors,
    ));
    record_component(
        &trace,
        frame_count(frame_count_resource),
        &lifecycle_producer(PHASE_ADD, COMPONENT_ON_MONITOR),
        COMPONENT_ON_MONITOR,
        PHASE_ADD,
        add.entity,
        fields,
    );
}

pub(super) fn on_monitor_link_discarded(
    discard: On<Discard, OnMonitor>,
    links: Query<&OnMonitor>,
    bindings: Res<WindowBindings>,
    windows: Query<(Has<PrimaryWindow>, Option<&ManagedWindow>)>,
    installed_monitors: Option<Res<Monitors>>,
    winit_monitors: Res<bevy::winit::WinitMonitors>,
    trace: Res<ProbeTrace>,
    frame_count_resource: Option<Res<FrameCount>>,
    _: NonSendMarker,
) {
    let mut fields = vec![
        field(FIELD_WINDOW, discard.entity),
        field(
            FIELD_WINDOW_KEY,
            window_key(discard.entity, &bindings, &windows),
        ),
        field(
            FIELD_MONITOR_ENTITY,
            links.get(discard.entity).ok().map(|link| link.0),
        ),
    ];
    fields.extend(native_monitor_fields(
        discard.entity,
        installed_monitors.as_deref(),
        &winit_monitors,
    ));
    record_component(
        &trace,
        frame_count(frame_count_resource),
        &lifecycle_producer(PHASE_DISCARD, COMPONENT_ON_MONITOR),
        COMPONENT_ON_MONITOR,
        PHASE_DISCARD,
        discard.entity,
        fields,
    );
}

pub(super) fn on_monitor_link_inserted(
    insert: On<Insert, OnMonitor>,
    links: Query<&OnMonitor>,
    bindings: Res<WindowBindings>,
    windows: Query<(Has<PrimaryWindow>, Option<&ManagedWindow>)>,
    installed_monitors: Option<Res<Monitors>>,
    winit_monitors: Res<bevy::winit::WinitMonitors>,
    trace: Res<ProbeTrace>,
    frame_count_resource: Option<Res<FrameCount>>,
    _: NonSendMarker,
) {
    let mut fields = vec![
        field(FIELD_WINDOW, insert.entity),
        field(
            FIELD_WINDOW_KEY,
            window_key(insert.entity, &bindings, &windows),
        ),
        field(
            FIELD_MONITOR_ENTITY,
            links.get(insert.entity).ok().map(|link| link.0),
        ),
    ];
    fields.extend(native_monitor_fields(
        insert.entity,
        installed_monitors.as_deref(),
        &winit_monitors,
    ));
    record_component(
        &trace,
        frame_count(frame_count_resource),
        &lifecycle_producer(PHASE_INSERT, COMPONENT_ON_MONITOR),
        COMPONENT_ON_MONITOR,
        PHASE_INSERT,
        insert.entity,
        fields,
    );
}

pub(super) fn on_monitor_link_removed(
    remove: On<Remove, OnMonitor>,
    links: Query<&OnMonitor>,
    bindings: Res<WindowBindings>,
    windows: Query<(Has<PrimaryWindow>, Option<&ManagedWindow>)>,
    installed_monitors: Option<Res<Monitors>>,
    winit_monitors: Res<bevy::winit::WinitMonitors>,
    trace: Res<ProbeTrace>,
    frame_count_resource: Option<Res<FrameCount>>,
    _: NonSendMarker,
) {
    let mut fields = vec![
        field(FIELD_WINDOW, remove.entity),
        field(
            FIELD_WINDOW_KEY,
            window_key(remove.entity, &bindings, &windows),
        ),
        field(
            FIELD_MONITOR_ENTITY,
            links.get(remove.entity).ok().map(|link| link.0),
        ),
    ];
    fields.extend(native_monitor_fields(
        remove.entity,
        installed_monitors.as_deref(),
        &winit_monitors,
    ));
    record_component(
        &trace,
        frame_count(frame_count_resource),
        &lifecycle_producer(PHASE_REMOVE, COMPONENT_ON_MONITOR),
        COMPONENT_ON_MONITOR,
        PHASE_REMOVE,
        remove.entity,
        fields,
    );
}

pub(super) fn on_monitor_link_despawned(
    despawn: On<Despawn, OnMonitor>,
    bindings: Res<WindowBindings>,
    trace: Res<ProbeTrace>,
    frame_count_resource: Option<Res<FrameCount>>,
) {
    record_component(
        &trace,
        frame_count(frame_count_resource),
        &lifecycle_producer(PHASE_DESPAWN, COMPONENT_ON_MONITOR),
        COMPONENT_ON_MONITOR,
        PHASE_DESPAWN,
        despawn.entity,
        vec![field(FIELD_WINDOW_KEY, bindings.0.get(&despawn.entity))],
    );
}

pub(super) fn on_has_windows_added(
    add: On<Add, HasWindows>,
    relationships: Query<&HasWindows>,
    trace: Res<ProbeTrace>,
    frame_count_resource: Option<Res<FrameCount>>,
) {
    let windows = relationships
        .get(add.entity)
        .map(|relationship| relationship.iter().collect::<Vec<_>>());
    record_component(
        &trace,
        frame_count(frame_count_resource),
        &lifecycle_producer(PHASE_ADD, COMPONENT_HAS_WINDOWS),
        COMPONENT_HAS_WINDOWS,
        PHASE_ADD,
        add.entity,
        vec![field(FIELD_HAS_WINDOWS, windows)],
    );
}

pub(super) fn on_has_windows_removed(
    remove: On<Remove, HasWindows>,
    relationships: Query<&HasWindows>,
    trace: Res<ProbeTrace>,
    frame_count_resource: Option<Res<FrameCount>>,
) {
    let windows = relationships
        .get(remove.entity)
        .map(|relationship| relationship.iter().collect::<Vec<_>>());
    record_component(
        &trace,
        frame_count(frame_count_resource),
        &lifecycle_producer(PHASE_REMOVE, COMPONENT_HAS_WINDOWS),
        COMPONENT_HAS_WINDOWS,
        PHASE_REMOVE,
        remove.entity,
        vec![field(FIELD_HAS_WINDOWS, windows)],
    );
}

pub(super) fn on_has_windows_despawned(
    despawn: On<Despawn, HasWindows>,
    relationships: Query<&HasWindows>,
    trace: Res<ProbeTrace>,
    frame_count_resource: Option<Res<FrameCount>>,
) {
    let windows = relationships
        .get(despawn.entity)
        .map(|relationship| relationship.iter().collect::<Vec<_>>());
    record_component(
        &trace,
        frame_count(frame_count_resource),
        &lifecycle_producer(PHASE_DESPAWN, COMPONENT_HAS_WINDOWS),
        COMPONENT_HAS_WINDOWS,
        PHASE_DESPAWN,
        despawn.entity,
        vec![field(FIELD_HAS_WINDOWS, windows)],
    );
}
