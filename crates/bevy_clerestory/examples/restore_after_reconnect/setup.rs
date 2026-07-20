use bevy::diagnostic::FrameCount;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy::window::WindowResolution;
use bevy_clerestory::ManagedWindow;
use bevy_clerestory::Monitors;
use bevy_clerestory::Platform;

use super::ProbeMonitorIndex;
use super::SmokeExitFrame;
use super::constants::*;
use super::selected_window_position;
use super::trace::ProbeTrace;

fn field(name: &str, value: impl std::fmt::Debug) -> (String, String) {
    (name.into(), format!("{value:?}"))
}

pub(super) fn exit_after_smoke_frame(
    exit_frame: Option<Res<SmokeExitFrame>>,
    frame_count: Res<FrameCount>,
    mut app_exit: MessageWriter<AppExit>,
) {
    if exit_frame.is_some_and(|exit_frame| frame_count.0 >= exit_frame.0) {
        app_exit.write(AppExit::Success);
    }
}

pub(super) fn spawn_secondary_window(mut commands: Commands) {
    commands.spawn((
        Window {
            title: SECONDARY_WINDOW_TITLE.into(),
            position: WindowPosition::Automatic,
            resolution: WindowResolution::new(SECONDARY_WINDOW_WIDTH, SECONDARY_WINDOW_HEIGHT),
            ..default()
        },
        ManagedWindow {
            name: SECONDARY_WINDOW_KEY.into(),
        },
    ));
}

pub(super) fn position_probe_windows(
    monitor_index: Res<ProbeMonitorIndex>,
    platform: Res<Platform>,
    mut windows: Query<&mut Window, Or<(With<PrimaryWindow>, With<ManagedWindow>)>>,
) {
    for mut window in &mut windows {
        window.position = selected_window_position(*platform, monitor_index.0);
    }
}

pub(super) fn trace_probe_session(
    monitor_index: Res<ProbeMonitorIndex>,
    platform: Res<Platform>,
    monitors: Res<Monitors>,
    trace: Res<ProbeTrace>,
    frame_count: Res<FrameCount>,
) {
    trace.record(
        frame_count.0,
        PRODUCER_STARTUP_SESSION,
        KIND_PROBE_SESSION,
        vec![
            field(FIELD_PLATFORM, *platform),
            field(FIELD_SELECTED_MONITOR_INDEX, monitor_index.0),
            field(FIELD_PLACEMENT_CAPABILITY, platform.position_available()),
            field(
                FIELD_MONITOR,
                monitors
                    .iter()
                    .map(|monitor| (monitor.entity, *monitor.monitor_info))
                    .collect::<Vec<_>>(),
            ),
        ],
    );
}
