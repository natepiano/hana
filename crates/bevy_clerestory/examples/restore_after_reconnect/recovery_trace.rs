use bevy::diagnostic::FrameCount;
use bevy::prelude::*;
use bevy_clerestory::WindowRecoveryAvailable;
use bevy_clerestory::WindowRecoveryPending;

use super::constants::*;
use super::trace::ProbeTrace;

fn field(name: &str, value: impl std::fmt::Debug) -> (String, String) {
    (name.into(), format!("{value:?}"))
}

pub(super) fn on_window_recovery_pending(
    event: On<WindowRecoveryPending>,
    trace: Res<ProbeTrace>,
    frame_count_resource: Res<FrameCount>,
) {
    trace.record(
        frame_count_resource.0,
        PRODUCER_RECOVERY_PENDING,
        KIND_RECOVERY_PENDING,
        vec![
            field(FIELD_WINDOW_KEY, &event.window_key),
            field(FIELD_MONITOR, event.monitor_id),
        ],
    );
}

pub(super) fn on_window_recovery_available(
    event: On<WindowRecoveryAvailable>,
    trace: Res<ProbeTrace>,
    frame_count_resource: Res<FrameCount>,
) {
    trace.record(
        frame_count_resource.0,
        PRODUCER_RECOVERY_AVAILABLE,
        KIND_RECOVERY_AVAILABLE,
        vec![
            field(FIELD_WINDOW_KEY, &event.window_key),
            field(FIELD_MONITOR, event.monitor),
        ],
    );
}
