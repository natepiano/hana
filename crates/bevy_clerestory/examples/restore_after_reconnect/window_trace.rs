use std::collections::HashMap;

use bevy::diagnostic::FrameCount;
use bevy::ecs::system::NonSendMarker;
use bevy::prelude::*;
use bevy::window::OnMonitor;
use bevy::window::PrimaryWindow;
use bevy::window::WindowClosed;
use bevy::window::WindowClosing;
use bevy::window::WindowEvent;
use bevy::window::WindowMode;
use bevy::winit::WINIT_WINDOWS;
use bevy::winit::WinitMonitors;
use bevy_clerestory::ManagedWindow;
use bevy_clerestory::Monitors;
use bevy_clerestory::WindowKey;

use super::constants::*;
use super::trace::ProbeTrace;

#[derive(Default, Resource)]
pub(super) struct WindowBindings(pub(super) HashMap<Entity, WindowKey>);

#[derive(Clone, Copy, Debug, PartialEq)]
struct WindowSnapshot {
    window_position: WindowPosition,
    physical_size:   UVec2,
    window_mode:     WindowMode,
}

impl From<&Window> for WindowSnapshot {
    fn from(window: &Window) -> Self {
        Self {
            window_position: window.position,
            physical_size:   window.resolution.physical_size(),
            window_mode:     window.mode,
        }
    }
}

#[derive(Default, Resource)]
pub(super) struct WindowSnapshots(HashMap<Entity, WindowSnapshot>);

fn field(name: &str, value: impl std::fmt::Debug) -> (String, String) {
    (name.into(), format!("{value:?}"))
}

fn unmatched_native_monitor_fields(current_monitor_state: &str) -> Vec<(String, String)> {
    vec![
        field(FIELD_NATIVE_CURRENT_MONITOR_STATE, current_monitor_state),
        field(FIELD_NATIVE_MATCHED_ENTITY, VALUE_UNRESOLVED),
        field(FIELD_NATIVE_MATCHED_IDENTITY, VALUE_UNRESOLVED),
    ]
}

pub(super) fn window_key(
    entity: Entity,
    bindings: &WindowBindings,
    windows: &Query<(Has<PrimaryWindow>, Option<&ManagedWindow>)>,
) -> String {
    bindings
        .0
        .get(&entity)
        .cloned()
        .or_else(|| {
            windows.get(entity).ok().and_then(|(primary, managed)| {
                if primary {
                    Some(WindowKey::Primary)
                } else {
                    managed.map(|managed| WindowKey::Managed(managed.name.clone()))
                }
            })
        })
        .map_or_else(|| VALUE_UNBOUND.into(), |window_key| window_key.to_string())
}

pub(super) fn native_monitor_fields(
    entity: Entity,
    monitors: Option<&Monitors>,
    winit_monitors: &WinitMonitors,
) -> Vec<(String, String)> {
    WINIT_WINDOWS.with_borrow(|winit_windows| {
        let Some(winit_window) = winit_windows.get_window(entity) else {
            return unmatched_native_monitor_fields(VALUE_NATIVE_WINDOW_UNAVAILABLE);
        };
        let Some(native_monitor) = winit_window.current_monitor() else {
            return unmatched_native_monitor_fields(VALUE_CURRENT_MONITOR_NO_HANDLE);
        };

        let matched = monitors.and_then(|monitors| {
            monitors.iter().find(|monitor| {
                winit_monitors
                    .find_entity(monitor.entity)
                    .is_some_and(|cached_handle| cached_handle == native_monitor)
            })
        });
        vec![
            field(
                FIELD_NATIVE_CURRENT_MONITOR_STATE,
                VALUE_CURRENT_MONITOR_HANDLE_RETURNED,
            ),
            field(
                FIELD_NATIVE_MATCHED_ENTITY,
                matched.map_or_else(
                    || VALUE_UNRESOLVED.into(),
                    |monitor| format!("{:?}", monitor.entity),
                ),
            ),
            field(
                FIELD_NATIVE_MATCHED_IDENTITY,
                matched.map_or_else(
                    || VALUE_UNRESOLVED.into(),
                    |monitor| format!("{:?}", monitor.monitor_info.identity),
                ),
            ),
        ]
    })
}

pub(super) fn trace_os_window_events(
    mut events: MessageReader<WindowEvent>,
    bindings: Res<WindowBindings>,
    windows: Query<(Has<PrimaryWindow>, Option<&ManagedWindow>)>,
    window_components: Query<&Window>,
    trace: Res<ProbeTrace>,
    frame_count_resource: Res<FrameCount>,
    mut app_exit: MessageWriter<AppExit>,
) {
    let frame_count = frame_count_resource.0;
    for event in events.read() {
        match event {
            WindowEvent::WindowCreated(event) => trace.record(
                frame_count,
                PRODUCER_UPDATE_OS_WINDOW_EVENTS,
                KIND_WINDOW_CREATED,
                vec![
                    field(FIELD_WINDOW, event.window),
                    field(
                        FIELD_WINDOW_TITLE,
                        window_components
                            .get(event.window)
                            .ok()
                            .map(|window| &window.title),
                    ),
                    field(
                        FIELD_WINDOW_KEY,
                        window_key(event.window, &bindings, &windows),
                    ),
                ],
            ),
            WindowEvent::WindowResized(event) => trace.record(
                frame_count,
                PRODUCER_UPDATE_OS_WINDOW_EVENTS,
                KIND_WINDOW_RESIZED,
                vec![
                    field(FIELD_WINDOW, event.window),
                    field(
                        FIELD_WINDOW_KEY,
                        window_key(event.window, &bindings, &windows),
                    ),
                    field(FIELD_WINDOW_SIZE, (event.width, event.height)),
                ],
            ),
            WindowEvent::WindowMoved(event) => trace.record(
                frame_count,
                PRODUCER_UPDATE_OS_WINDOW_EVENTS,
                KIND_WINDOW_MOVED,
                vec![
                    field(FIELD_WINDOW, event.window),
                    field(
                        FIELD_WINDOW_KEY,
                        window_key(event.window, &bindings, &windows),
                    ),
                    field(FIELD_WINDOW_POSITION, event.position),
                ],
            ),
            WindowEvent::WindowCloseRequested(event) => {
                trace.record(
                    frame_count,
                    PRODUCER_UPDATE_OS_WINDOW_EVENTS,
                    KIND_CLOSE_INTENT,
                    vec![
                        field(FIELD_WINDOW, event.window),
                        field(
                            FIELD_WINDOW_KEY,
                            window_key(event.window, &bindings, &windows),
                        ),
                    ],
                );
                if bindings.0.get(&event.window) == Some(&WindowKey::Primary) {
                    app_exit.write(AppExit::Success);
                }
            },
            WindowEvent::WindowDestroyed(event) => trace.record(
                frame_count,
                PRODUCER_UPDATE_OS_WINDOW_EVENTS,
                KIND_WINDOW_DESTROYED,
                vec![
                    field(FIELD_WINDOW, event.window),
                    field(
                        FIELD_WINDOW_KEY,
                        window_key(event.window, &bindings, &windows),
                    ),
                ],
            ),
            _ => {},
        }
    }
}

pub(super) fn trace_internal_window_messages(
    mut closing: MessageReader<WindowClosing>,
    mut closed: MessageReader<WindowClosed>,
    bindings: Res<WindowBindings>,
    trace: Res<ProbeTrace>,
    frame_count_resource: Res<FrameCount>,
) {
    let frame_count = frame_count_resource.0;
    for event in closing.read() {
        trace.record(
            frame_count,
            PRODUCER_UPDATE_INTERNAL_WINDOW_MESSAGES,
            KIND_WINDOW_CLOSING,
            vec![
                field(FIELD_WINDOW, event.window),
                field(FIELD_WINDOW_KEY, bindings.0.get(&event.window)),
            ],
        );
    }
    for event in closed.read() {
        trace.record(
            frame_count,
            PRODUCER_UPDATE_INTERNAL_WINDOW_MESSAGES,
            KIND_WINDOW_CLOSED,
            vec![
                field(FIELD_WINDOW, event.window),
                field(FIELD_WINDOW_KEY, bindings.0.get(&event.window)),
            ],
        );
    }
}

pub(super) fn trace_window_component_changes(
    windows: Query<
        (
            Entity,
            &Window,
            Option<&OnMonitor>,
            Has<PrimaryWindow>,
            Option<&ManagedWindow>,
        ),
        Changed<Window>,
    >,
    installed_monitors: Option<Res<Monitors>>,
    winit_monitors: Res<WinitMonitors>,
    bindings: Res<WindowBindings>,
    mut snapshots: ResMut<WindowSnapshots>,
    trace: Res<ProbeTrace>,
    frame_count_resource: Res<FrameCount>,
    _: NonSendMarker,
) {
    for (entity, window, on_monitor, primary, managed_window) in &windows {
        let snapshot = WindowSnapshot::from(window);
        let previous = snapshots.0.get(&entity).copied();
        if previous == Some(snapshot) {
            continue;
        }
        snapshots.0.insert(entity, snapshot);
        let window_key = bindings.0.get(&entity).cloned().or_else(|| {
            if primary {
                Some(WindowKey::Primary)
            } else {
                managed_window.map(|managed_window| WindowKey::Managed(managed_window.name.clone()))
            }
        });
        let mut fields = vec![
            field(FIELD_WINDOW, entity),
            field(FIELD_WINDOW_KEY, &window_key),
            field(FIELD_WINDOW_TITLE, &window.title),
            field(FIELD_WINDOW_POSITION, window.position),
            field(FIELD_WINDOW_SIZE, window.resolution.physical_size()),
            field(FIELD_WINDOW_MODE, window.mode),
            field(
                FIELD_MONITOR_ENTITY,
                on_monitor.map(|on_monitor| on_monitor.0),
            ),
            field(
                FIELD_TRANSITION,
                previous.map(|previous| (previous, snapshot)),
            ),
        ];
        fields.extend(native_monitor_fields(
            entity,
            installed_monitors.as_deref(),
            &winit_monitors,
        ));
        trace.record(
            frame_count_resource.0,
            PRODUCER_POST_UPDATE_WINDOWS,
            KIND_WINDOW_COMPONENT_CHANGED,
            fields,
        );
        if let Some(previous) = previous
            && previous.window_mode != snapshot.window_mode
        {
            trace.record(
                frame_count_resource.0,
                PRODUCER_POST_UPDATE_WINDOWS,
                KIND_WINDOW_MODE_CHANGED,
                vec![
                    field(FIELD_WINDOW, entity),
                    field(FIELD_WINDOW_KEY, window_key),
                    field(
                        FIELD_TRANSITION,
                        (previous.window_mode, snapshot.window_mode),
                    ),
                ],
            );
        }
    }
}
