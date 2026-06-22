use crate::monitors::MonitorInfo;
use crate::monitors::Monitors;

pub(crate) enum MonitorResolutionSource {
    Requested,
    FallbackToPrimary,
}

pub struct ResolvedMonitor<'a> {
    pub monitor_info:              &'a MonitorInfo,
    pub logical_position:          Option<(i32, i32)>,
    pub monitor_resolution_source: MonitorResolutionSource,
}

/// Resolve the target monitor from saved state and return an adjusted saved position.
#[must_use]
pub(crate) fn resolve_target_monitor_and_position(
    saved_monitor_index: usize,
    logical_saved_position: Option<(i32, i32)>,
    monitors: &Monitors,
) -> ResolvedMonitor<'_> {
    monitors.by_index(saved_monitor_index).map_or_else(
        || ResolvedMonitor {
            monitor_info:              monitors.first(),
            logical_position:          None,
            monitor_resolution_source: MonitorResolutionSource::FallbackToPrimary,
        },
        |monitor_info| ResolvedMonitor {
            monitor_info,
            logical_position: logical_saved_position,
            monitor_resolution_source: MonitorResolutionSource::Requested,
        },
    )
}
